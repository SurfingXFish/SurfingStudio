//! Unified plugin runtime used by the audio engine.

use super::backends::{Vst2Host, make_vst2_cc, send_vst2_events, send_vst2_gates};
use super::{ParamView, PluginKind};
use crate::audio::synth::{GateKind, SynthEvent};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use vst::plugin::Plugin as VstPluginTrait;
use vst3_host::Plugin as Vst3Plugin;
use vst3_host::midi::{MidiChannel, MidiEvent as Vst3MidiEvent};

/// 全量参数比对的参数数量上限：Kontakt 等采样器有数千个参数，逐个调用控制器会让 UI 线程
/// 卡住数秒并长期占用引擎锁（音频回调拿不到锁，表现为没有声音）；超过上限时只依赖插件主动
/// 上报的编辑手势日志。
/// Upper bound for a full parameter comparison: samplers such as Kontakt expose thousands of
/// parameters, and querying each one freezes the UI for seconds while it holds the engine lock
/// (the audio callback then times out, heard as silence). Above this bound only the plugin's own
/// edit-gesture log is used.
const MAX_FULL_SCAN_PARAMS: usize = 512;
/// 每帧最多格式化的编辑器反馈条数：分析器/电平类参数可能每帧上报数百条。
/// Maximum editor-feedback entries formatted per frame: analyser/meter parameters may report
/// hundreds of values every frame.
const MAX_EDITOR_FEEDBACK_UPDATES: usize = 64;
/// 本宿主 `save_state` 写出的状态块头；其它来源的字节不能安全地交给插件。
/// Magic header written by this host's `save_state`; bytes from other sources are unsafe to apply.
const VST3_STATE_MAGIC: &[u8; 16] = b"VST3HOST_STATE\0\0";

pub struct Vst3Runtime {
    /// 已绑定的 ARA 文档会话；插件不支持 ARA 时为 `None`。
    /// Bound ARA document session; `None` when the plug-in does not support ARA.
    ///
    /// 必须声明在 `plugin` 之前：Rust 按字段声明顺序析构，ARA 文档控制器必须在插件
    /// 卸载模块之前销毁，否则销毁回调会访问已卸载的插件代码。
    /// Must be declared before `plugin`: Rust drops fields in declaration order, and the ARA
    /// document controller has to be destroyed before the plug-in unloads its module, otherwise
    /// the teardown callbacks would execute unmapped plug-in code.
    pub ara: Option<crate::plugin::ara::AraSession>,
    pub plugin: Arc<Mutex<Vst3Plugin>>,
    pub name: String,
    pub params: Vec<ParamView>,
    pub out_channels: usize,
    pub has_editor: bool,
    pub kind: PluginKind,
    /// 规范化的插件路径（小写），单实例插件复用缓存按此匹配。
    /// Normalised plugin path (lower case) used to match the reusable single-instance cache.
    pub path_key: String,
    /// 实时线程复用的处理缓冲；块大小变化时才重新分配。
    /// Processing buffers reused by the realtime thread; reallocated only when the block size changes.
    pub audio_buffers: Option<vst3_host::AudioBuffers>,
    /// 尚未送达插件的播放状态：插件锁被音频/界面线程占用时保留，由下一块补发。
    /// Playing state not yet delivered: kept while the plugin lock is busy and sent on the next block.
    pub pending_playing: Option<bool>,
    /// 尚未送达插件的走带位置：同上，避免定位被静默丢弃。
    /// Transport position not yet delivered: same as above, so a seek is never silently dropped.
    pub pending_transport: Option<vst3_host::TransportPosition>,
    /// 只做走带同步、不产生音频（VOCALOID Bridge）：输出直通输入。
    /// Transport-sync only, produces no audio (VOCALOID Bridge): its output passes the input through.
    pub sync_only: bool,
    /// 插件上报的处理延迟（采样数）缓存；插件忙时沿用上一次的值。
    /// Cached reported processing latency in samples; the last value is kept while the plugin is busy.
    pub latency_samples: usize,
    /// 插件拒绝（`kResultFalse`）恢复的状态；编辑器就绪后重试。
    /// State the plugin declined (`kResultFalse`); retried once its editor is ready.
    pub pending_state: Option<Vec<u8>>,
    /// 已重试次数；超过上限后放弃，避免每秒把几十 KB 状态塞给插件。
    /// Retry count; after the cap the retry stops so tens of KB are not pushed every second.
    pub state_retries: u32,
}

/// 单实例插件的可复用实例：关闭工程时不终止，下一个工程按路径取回同一实例，
/// 避免 VOCALOID / ACE Bridge 再次拉起编辑器时被“已经有一个启动了”拒绝。
/// Reusable single-instance plugin: kept alive when a project closes and handed to the next project
/// with the same path, so VOCALOID / ACE Bridge do not refuse a second editor.
struct ReusableVst3 {
    path_key: String,
    name: String,
    params: Vec<ParamView>,
    out_channels: usize,
    has_editor: bool,
    kind: PluginKind,
    plugin: Arc<Mutex<Vst3Plugin>>,
}

static REUSABLE_VST3: std::sync::OnceLock<Mutex<Vec<ReusableVst3>>> = std::sync::OnceLock::new();

fn reusable_vst3() -> &'static Mutex<Vec<ReusableVst3>> {
    REUSABLE_VST3.get_or_init(|| Mutex::new(Vec::new()))
}

/// 所属模块：plugin / 统一运行时
/// 功能：判断插件是否属于单实例类型（VOCALOID / ACE Bridge / XStudio / Synthesizer V）。
/// 输入：`name`：插件显示名。
/// 输出/副作用：布尔值；不修改状态。
/// 复杂度：时间 O(名称长度)，空间 O(1)。
///
/// Module: plugin / unified runtime
/// Purpose: Whether the plugin is a single-instance type (VOCALOID / ACE Bridge / XStudio /
/// Synthesizer V).
/// Inputs: `name`: plugin display name.
/// Outputs / side effects: a boolean; no state change.
/// Complexity: O(name length) time and O(1) space.
fn is_single_instance_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    ["vocaloid", "ace bridge", "xstudio", "synthesizer v"]
        .iter()
        .any(|keyword| lower.contains(keyword))
}

/// 所属模块：plugin / 统一运行时
/// 功能：把单实例插件登记为可复用实例（关闭工程、不终止组件时调用）。
/// 输入：`runtime`：插件运行时。
/// 输出/副作用：关闭可能残留的编辑器视图，并把实例 Arc 放入进程级缓存；其它插件忽略。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: plugin / unified runtime
/// Purpose: Register a single-instance plugin as reusable (called when closing a project without
/// terminating components).
/// Inputs: `runtime`: plugin runtime.
/// Outputs / side effects: closes any leftover editor view and stores the instance Arc in the
/// process-wide cache; other plugins are ignored.
/// Complexity: O(1) time and space.
pub(crate) fn stash_reusable(runtime: &PluginRuntime) {
    let PluginRuntime::Vst3(v) = runtime else {
        return;
    };
    if !is_single_instance_name(&v.name) {
        return;
    }
    // ARA 文档控制器与插件实例绑定在一起；保留实例却不保留文档会让下次绑定失败，
    // 因此 ARA 插件不做跨工程复用（按普通插件卸载）。
    // An ARA document controller is tied to the instance; keeping the instance without its
    // document would break the next binding, so ARA plug-ins are not reused across projects.
    if v.ara.is_some() {
        crate::diag::info(
            "plugin.runtime",
            "plugin.reuse",
            &[("phase", "skip_ara"), ("name", &v.name)],
        );
        return;
    }
    {
        let mut plugin = v.plugin.lock().unwrap_or_else(|poison| poison.into_inner());
        // 宿主编辑器视图（如果有）在工程关闭时已经关闭；这里确保插件内部状态一致。
        // Any host editor view was closed with the project; make sure the plugin agrees.
        let _ = plugin.close_editor();
    }
    let entry = ReusableVst3 {
        path_key: v.path_key.clone(),
        name: v.name.clone(),
        params: v.params.clone(),
        out_channels: v.out_channels,
        has_editor: v.has_editor,
        kind: v.kind,
        plugin: v.plugin.clone(),
    };
    crate::diag::info(
        "plugin.runtime",
        "plugin.reuse",
        &[("phase", "stash"), ("name", &v.name), ("path", &v.path_key)],
    );
    if let Ok(mut cache) = reusable_vst3().lock() {
        // 同一路径可能有多份实例（例如工程里有 3 条 ACE Bridge 轨道），按出现顺序全部保留，
        // 重新加载时逐个取回。
        // A path may have several instances (for example three ACE Bridge tracks); keep them all in
        // order and take them one by one when the project is reopened.
        cache.push(entry);
    }
}

impl PluginRuntime {
    /// 所属模块：plugin / 统一运行时
    /// 功能：按路径取回单实例插件的可复用实例；没有时返回 `None`。
    /// 输入：`path_key`：规范化的插件路径。
    /// 输出/副作用：从缓存移除并返回 `PluginRuntime::Vst3`；不创建新插件。
    /// 复杂度：时间 O(缓存数)，空间 O(1)。
    ///
    /// Module: plugin / unified runtime
    /// Purpose: Take the reusable single-instance instance for a path, if any.
    /// Inputs: `path_key`: normalised plugin path.
    /// Outputs / side effects: removes and returns `PluginRuntime::Vst3`; no new instance is created.
    /// Complexity: O(cache size) time and O(1) space.
    pub(crate) fn take_reusable(path_key: &str) -> Option<PluginRuntime> {
        let entry = {
            let mut cache = reusable_vst3().lock().ok()?;
            let index = cache.iter().position(|item| item.path_key == path_key)?;
            cache.remove(index)
        };
        {
            let mut plugin = entry
                .plugin
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            // 旧工程关闭时没有 terminate；这里重新确保处理已启动（已启动时错误可忽略）。
            // The old project did not terminate it; make sure processing is running (errors are
            // harmless when it already is).
            let _ = plugin.start_processing();
        }
        crate::diag::info(
            "plugin.runtime",
            "plugin.reuse",
            &[
                ("phase", "take"),
                ("name", &entry.name),
                ("path", &entry.path_key),
            ],
        );
        Some(PluginRuntime::Vst3(Vst3Runtime {
            plugin: entry.plugin,
            name: entry.name,
            params: entry.params,
            out_channels: entry.out_channels,
            has_editor: entry.has_editor,
            kind: entry.kind,
            path_key: entry.path_key,
            audio_buffers: None,
            pending_playing: None,
            pending_transport: None,
            sync_only: false,
            latency_samples: 0,
            pending_state: None,
            state_retries: 0,
            ara: None,
        }))
    }
}

pub struct Vst2Runtime {
    pub instance: vst::host::PluginInstance,
    pub _host: Arc<Mutex<Vst2Host>>,
    pub name: String,
    pub params: Vec<ParamView>,
    pub num_inputs: usize,
    pub num_outputs: usize,
    pub is_instrument: bool,
    /// 实时线程复用的输入/输出通道缓冲；声道数或块大小变化时才重新分配。
    /// Reused input/output channel buffers for the realtime thread; reallocated only when the channel
    /// count or block size changes.
    pub process_inputs: Vec<Vec<f32>>,
    pub process_outputs: Vec<Vec<f32>>,
}

pub enum PluginRuntime {
    Vst3(Vst3Runtime),
    Vst2(Vst2Runtime),
    Failed(String),
}

impl PluginRuntime {
    /// 所属模块：plugin / 统一运行时 / ARA
    /// 功能：把 ARA 播放区域更新到工程时间范围（秒）；非 ARA 插件为空操作。
    /// 输入：`start` 起始秒、`duration` 时长秒。
    /// 输出/副作用：更新插件侧 ARA 模型；失败时返回中文错误。
    /// 复杂度：时间 O(1)（未变化时），空间 O(1)。
    ///
    /// Module: plugin / unified runtime / ARA
    /// Purpose: Move the ARA playback region to a project time range (seconds); a no-op for
    /// non-ARA plug-ins.
    /// Inputs: `start`, `duration`.
    /// Outputs / side effects: updates the plug-in's ARA model; errors carry Chinese context.
    /// Complexity: O(1) time when unchanged and O(1) space.
    pub fn set_ara_timeline(&mut self, start: f64, duration: f64) -> Result<(), String> {
        match self {
            PluginRuntime::Vst3(v) => match v.ara.as_mut() {
                Some(session) => session.set_timeline(start, duration),
                None => Ok(()),
            },
            _ => Ok(()),
        }
    }

    /// 所属模块：plugin / 统一运行时 / ARA
    /// 功能：把工程速度与拍号同步给 ARA 音乐上下文；未变化或非 ARA 插件为空操作。
    /// 输入：`bpm` 速度、`time_sig` 拍号。
    /// 输出/副作用：更新插件侧音乐上下文内容；失败时返回中文错误。
    /// 复杂度：时间 O(1)（未变化时），空间 O(1)。
    ///
    /// Module: plugin / unified runtime / ARA
    /// Purpose: Push project tempo and time signature into the ARA musical context; a no-op
    /// when unchanged or for non-ARA plug-ins.
    /// Inputs: `bpm`, `time_sig`.
    /// Outputs / side effects: updates the plug-in's musical context; errors carry Chinese context.
    /// Complexity: O(1) time when unchanged and O(1) space.
    pub fn set_ara_tempo(&mut self, bpm: f64, time_sig: (u8, u8)) -> Result<(), String> {
        match self {
            PluginRuntime::Vst3(v) => match v.ara.as_mut() {
                Some(session) => session.set_tempo(bpm, time_sig),
                None => Ok(()),
            },
            _ => Ok(()),
        }
    }

    /// 所属模块：plugin / 统一运行时 / ARA
    /// 功能：在文档控制器线程上把插件侧 ARA 状态导出到缓存。
    /// 输入：`self`。
    /// 输出/副作用：内容变化时调用 `storeObjectsToArchive`；线程不符时返回错误。
    /// 复杂度：时间 O(归档大小)，空间 O(归档大小)。
    ///
    /// Module: plugin / unified runtime / ARA
    /// Purpose: Export the plug-in-side ARA state into the cache on the document controller thread.
    /// Inputs: `self`.
    /// Outputs / side effects: calls `storeObjectsToArchive` when content changed; a thread
    /// mismatch returns an error.
    /// Complexity: O(archive size) time and space.
    pub fn refresh_ara_archive(&self) -> Result<(), String> {
        match self {
            PluginRuntime::Vst3(v) => match v.ara.as_ref() {
                Some(session) => session.refresh_archive(),
                None => Ok(()),
            },
            _ => Ok(()),
        }
    }

    /// 所属模块：plugin / 统一运行时 / ARA
    /// 功能：返回缓存的 ARA 归档；没有归档时为 `None`。
    /// 输入：`self`。
    /// 输出/副作用：返回归档副本；不调用插件。
    /// 复杂度：时间 O(归档大小)，空间 O(归档大小)。
    ///
    /// Module: plugin / unified runtime / ARA
    /// Purpose: Return the cached ARA archive, or `None` when empty.
    /// Inputs: `self`.
    /// Outputs / side effects: a copy of the archive; no plug-in calls.
    /// Complexity: O(archive size) time and space.
    pub fn ara_archive_snapshot(&self) -> Option<Vec<u8>> {
        match self {
            PluginRuntime::Vst3(v) => v.ara.as_ref().map(|session| session.archive_snapshot()),
            _ => None,
        }
    }

    /// 所属模块：plugin / 统一运行时 / ARA
    /// 功能：把保存的 ARA 归档恢复到插件侧文档。
    /// 输入：`bytes` 归档字节。
    /// 输出/副作用：调用插件恢复状态；空归档或非 ARA 插件为空操作。
    /// 复杂度：时间 O(归档大小)，空间 O(归档大小)。
    ///
    /// Module: plugin / unified runtime / ARA
    /// Purpose: Restore a saved ARA archive into the plug-in-side document.
    /// Inputs: `bytes`.
    /// Outputs / side effects: restores the plug-in state; empty archives and non-ARA plug-ins are
    /// no-ops.
    /// Complexity: O(archive size) time and space.
    pub fn restore_ara_archive(&self, bytes: &[u8]) -> Result<(), String> {
        match self {
            PluginRuntime::Vst3(v) => match v.ara.as_ref() {
                Some(session) => session.restore_archive(bytes),
                None => Ok(()),
            },
            _ => Ok(()),
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            PluginRuntime::Vst3(v) => &v.name,
            PluginRuntime::Vst2(v) => &v.name,
            PluginRuntime::Failed(_) => "加载失败",
        }
    }

    pub fn params(&self) -> &[ParamView] {
        match self {
            PluginRuntime::Vst3(v) => &v.params,
            PluginRuntime::Vst2(v) => &v.params,
            PluginRuntime::Failed(_) => &[],
        }
    }

    pub fn refresh_params(&mut self) {
        match self {
            PluginRuntime::Vst3(v) => {
                let plugin = v.plugin.lock().unwrap_or_else(|p| p.into_inner());
                if let Ok(list) = plugin.get_parameters() {
                    v.params = list
                        .into_iter()
                        .map(|p| {
                            let display = plugin
                                .format_parameter(p.id, p.value)
                                .unwrap_or_else(|_| format!("{:.2}", p.value * 100.0));
                            ParamView {
                                id: p.id,
                                name: p.name,
                                value: p.value,
                                display,
                            }
                        })
                        .collect();
                }
            }
            PluginRuntime::Vst2(v) => {
                let count = v.params.len();
                let obj = v.instance.get_parameter_object();
                for i in 0..count {
                    let value = obj.get_parameter(i as i32) as f64;
                    let display = obj.get_parameter_text(i as i32);
                    if let Some(param) = v.params.get_mut(i) {
                        param.value = value;
                        param.display = if display.is_empty() {
                            format!("{:.0}%", value * 100.0)
                        } else {
                            display
                        };
                    }
                }
            }
            PluginRuntime::Failed(_) => {}
        }
    }

    pub fn set_param(&mut self, index: usize, value: f64) -> Result<(), String> {
        match self {
            PluginRuntime::Vst3(v) => {
                let mut plugin = v.plugin.lock().unwrap_or_else(|p| p.into_inner());
                let Some(param) = v.params.get(index).cloned() else {
                    return Err("参数不存在".into());
                };
                let value = value.clamp(0.0, 1.0);
                plugin
                    .set_parameter(param.id, value)
                    .map_err(|e| e.to_string())?;
                let display = plugin
                    .format_parameter(param.id, value)
                    .unwrap_or_else(|_| format!("{:.2}", value * 100.0));
                if let Some(p) = v.params.get_mut(index) {
                    p.value = value;
                    p.display = display;
                }
                Ok(())
            }
            PluginRuntime::Vst2(v) => {
                let Some(param) = v.params.get_mut(index) else {
                    return Err("参数不存在".into());
                };
                let value = value.clamp(0.0, 1.0);
                let obj = v.instance.get_parameter_object();
                obj.set_parameter(param.id as i32, value as f32);
                let display = obj.get_parameter_text(param.id as i32);
                param.value = value;
                param.display = if display.is_empty() {
                    format!("{:.0}%", value * 100.0)
                } else {
                    display
                };
                Ok(())
            }
            PluginRuntime::Failed(_) => Err("插件未加载".into()),
        }
    }

    /// 所属模块：plugin / 统一运行时
    /// 功能：插件是否提供可视化编辑器。
    /// 输入：`self`。
    /// 输出/副作用：返回能力标记；VST1/VST2 统一按“有编辑器”处理，实际打开失败时再回报。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / unified runtime
    /// Purpose: Whether the plugin exposes a visual editor.
    /// Inputs: `self`.
    /// Outputs / side effects: capability flag; VST1/VST2 is treated as having an
    /// editor and a failed open reports the error later.
    /// Complexity: O(1) time and space.
    pub fn has_editor(&self) -> bool {
        match self {
            PluginRuntime::Vst3(v) => v.has_editor,
            PluginRuntime::Vst2(_) => true,
            PluginRuntime::Failed(_) => false,
        }
    }

    pub fn vst3_plugin(&self) -> Option<Arc<Mutex<Vst3Plugin>>> {
        match self {
            PluginRuntime::Vst3(v) => Some(v.plugin.clone()),
            _ => None,
        }
    }

    /// 所属模块：plugin / 统一运行时
    /// 功能：取出 VST1/VST2 编辑器对象（每个插件实例只能取一次）。
    /// 输入：`self`。
    /// 输出/副作用：返回编辑器对象；VST3/失败态返回 `None`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / unified runtime
    /// Purpose: Take the VST1/VST2 editor object (once per plugin instance).
    /// Inputs: `self`.
    /// Outputs / side effects: returns the editor; `None` for VST3 or failed plugins.
    /// Complexity: O(1) time and space.
    pub fn take_editor(&mut self) -> Option<Box<dyn vst::editor::Editor>> {
        match self {
            PluginRuntime::Vst2(v) => v.instance.get_editor(),
            PluginRuntime::Vst3(_) | PluginRuntime::Failed(_) => None,
        }
    }

    /// 所属模块：plugin / 统一运行时
    /// 功能：终止插件组件但保留 Rust 实例，用于关闭工程时释放插件拉起的外部应用窗口。
    /// 输入：`self`。
    /// 输出/副作用：VST3 调用 `shutdown_component`，随后把自身替换为 `Failed` 并泄漏一份
    /// `Plugin` 强引用，避免在 UI 线程运行插件析构；外部编辑进程由应用层单独清理。
    /// VST2/失败态不做处理。
    /// 复杂度：时间 O(1 + 插件终止)，空间 O(1)（泄漏一个实例）。
    ///
    /// Module: plugin / unified runtime
    /// Purpose: Terminate the plugin component while keeping the Rust instance alive, so closing a
    /// project releases plugin-spawned external app windows without running the destructor on the
    /// UI thread; the app layer separately terminates the plugin's external editor process.
    /// Inputs: `self`.
    /// Outputs / side effects: VST3 calls `shutdown_component`, then replaces itself with `Failed`
    /// and leaks one strong `Plugin` reference; VST2/failed runtimes are untouched.
    /// Complexity: O(1 + plugin termination) time and O(1) space (one leaked instance).
    pub fn shutdown(&mut self) {
        let PluginRuntime::Vst3(v) = self else {
            return;
        };
        // 中毒的锁也继续终止：一次无关 panic 不能让插件拉起的窗口残留在系统里。
        // Continue even with a poisoned lock: an unrelated panic must not leave plugin-spawned
        // windows behind.
        let mut plugin = v.plugin.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Err(error) = plugin.shutdown_component() {
            crate::diag::warn(
                "plugin.runtime",
                "plugin.shutdown",
                &[("status", "error"), ("error", &error.to_string())],
            );
        }
        drop(plugin);
        // 不在这里析构：VOCALOID / ACE Bridge 的析构会等待 UI 消息循环；模块也保持加载，
        // 避免部分插件不支持「卸载后在同一进程重新加载」。单实例由应用层终止其外部编辑进程解决。
        // Do not destroy here: VOCALOID / ACE Bridge destructors wait for the UI message loop, and
        // keeping the module loaded avoids plugins that cannot be unloaded and reloaded in the same
        // process. The app layer resolves the single-instance conflict by terminating the external
        // editor process.
        let leaked = v.plugin.clone();
        *self = PluginRuntime::Failed("插件已关闭".into());
        std::mem::forget(leaked);
    }

    /// Capture state blob and parameter snapshot for project persistence.
    pub fn capture_state(&self) -> (Vec<u8>, Vec<f32>) {
        match self {
            PluginRuntime::Vst3(v) => {
                let plugin = v.plugin.lock().unwrap_or_else(|p| p.into_inner());
                let state = plugin.save_state().unwrap_or_default();
                (state, Vec::new())
            }
            PluginRuntime::Vst2(v) => {
                let params = v.params.iter().map(|p| p.value as f32).collect();
                (Vec::new(), params)
            }
            PluginRuntime::Failed(_) => (Vec::new(), Vec::new()),
        }
    }

    /// 所属模块：plugin / 统一运行时
    /// 功能：恢复插件状态：VST3 只接受本宿主 `save_state` 写出的状态块，VST2 按参数快照恢复。
    /// 输入：
    /// - `state`：VST3 状态块字节。
    /// - `params`：VST2 参数值快照。
    /// 输出/副作用：可能修改插件参数并刷新参数缓存；来源不可信的状态块直接跳过，
    /// 避免插件在 `setState` 内崩溃。
    /// 复杂度：时间 O(状态长度 + 参数数)，空间 O(参数数)。
    ///
    /// Module: plugin / unified runtime
    /// Purpose: Restore plugin state: VST3 accepts only blobs written by this host's `save_state`,
    /// VST2 restores from a parameter snapshot.
    /// Inputs:
    /// - `state`: VST3 state blob.
    /// - `params`: VST2 parameter snapshot.
    /// Outputs / side effects: may change plugin parameters and refresh the parameter cache;
    /// untrusted blobs are skipped so a plugin cannot crash inside `setState`.
    /// Complexity: O(state length + parameter count) time and O(parameter count) space.
    pub fn restore_state(&mut self, state: &[u8], params: &[f32]) {
        let mut refresh = false;
        match self {
            PluginRuntime::Vst3(v) => {
                // 旧工程或外部导入的状态可能不是本宿主的封装格式；Ample Sound 这类插件
                // 收到无法识别的字节会在 setState 内触发 0xC0000005，因此只恢复带宿主头的数据。
                // Legacy or imported state may not use this host's envelope; plugins such as Ample
                // Sound crash with 0xC0000005 in setState on unrecognised bytes, so only blobs with
                // the host header are restored.
                if !state.is_empty() && state.starts_with(VST3_STATE_MAGIC) {
                    let mut plugin = v.plugin.lock().unwrap_or_else(|p| p.into_inner());
                    // `setState` 可能返回 `kResultFalse`（VOCALOID 6 在编辑器尚未就绪时会拒绝），
                    // 此时状态没有被应用：记下字节，等编辑器起来后再重试，并写日志而不是静默丢弃。
                    // `setState` may return `kResultFalse` (VOCALOID 6 declines it until its editor is
                    // ready). The state is then NOT applied: keep the bytes, retry once the editor is
                    // up, and log it instead of dropping it silently.
                    match plugin.load_state(state) {
                        Ok(()) => v.pending_state = None,
                        Err(error) => {
                            v.pending_state = Some(state.to_vec());
                            v.state_retries = 0;
                            let bytes = state.len().to_string();
                            crate::diag::warn(
                                "app.plugin",
                                "plugin.state",
                                &[
                                    ("name", &v.name),
                                    ("status", "declined"),
                                    ("bytes", &bytes),
                                    ("error", &error.to_string()),
                                ],
                            );
                        }
                    }
                    // 恢复状态后同步参数快照，否则加载完成后的首次全量扫描会把
                    // 恢复出来的值当成用户改动，触发一次多余的重新同步。
                    // Refresh the snapshot after restoring so the first full scan
                    // does not mistake restored values for user edits and trigger
                    // a pointless re-sync.
                    if let Ok(list) = plugin.get_parameters() {
                        v.params = list
                            .into_iter()
                            .map(|p| ParamView {
                                id: p.id,
                                name: p.name,
                                value: p.value,
                                display: plugin
                                    .format_parameter(p.id, p.value)
                                    .unwrap_or_else(|_| format!("{:.2}", p.value * 100.0)),
                            })
                            .collect();
                    }
                }
            }
            PluginRuntime::Vst2(v) => {
                if !params.is_empty() {
                    let obj = v.instance.get_parameter_object();
                    for (i, value) in params.iter().enumerate() {
                        obj.set_parameter(i as i32, *value);
                    }
                    refresh = true;
                }
            }
            PluginRuntime::Failed(_) => {}
        }
        if refresh {
            self.refresh_params();
        }
    }

    /// 所属模块：plugin / 统一运行时
    /// 功能：按程序（预设）索引切换插件音色，用于导入工程在状态不可恢复时还原预设选择。
    /// 输入：`program`：0 基程序索引（SoundBug 的 `programNum`）。
    /// 输出/副作用：VST3 调用 `IUnitInfo` 选择根单元的程序并刷新参数；越界、
    /// 插件无程序表或 VST2 时静默忽略。
    /// 复杂度：时间 O(参数数)，空间 O(参数数)。
    ///
    /// Module: plugin / unified runtime
    /// Purpose: Select a plugin program (preset) so imported projects can restore the preset even
    /// when their state blob cannot be applied.
    /// Inputs: `program`: zero-based program index (SoundBug's `programNum`).
    /// Outputs / side effects: VST3 selects the root unit's program through `IUnitInfo` and
    /// refreshes parameters; out-of-range indices, plugins without a program list and VST2 runtimes
    /// are silently ignored.
    /// Complexity: O(parameter count) time and space.
    pub fn select_program(&mut self, program: i32) {
        let PluginRuntime::Vst3(v) = self else {
            return;
        };
        {
            let mut plugin = v.plugin.lock().unwrap_or_else(|p| p.into_inner());
            if plugin.select_program(0, program).is_err() {
                return;
            }
        }
        self.refresh_params();
    }

    pub fn set_tempo(&mut self, bpm: f64, time_sig: (u8, u8)) {
        match self {
            PluginRuntime::Vst3(v) => {
                if let Ok(mut plugin) = v.plugin.lock() {
                    let _ = plugin.set_tempo(bpm);
                    let _ = plugin.set_time_signature(time_sig.0 as i32, time_sig.1 as i32);
                }
            }
            PluginRuntime::Vst2(_) | PluginRuntime::Failed(_) => {}
        }
    }

    /// 所属模块：plugin / 运行时
    /// 功能：把走带播放状态写进插件收到的 ProcessContext（只在状态变化时调用）。
    /// 输入：`playing` 是否正在播放。
    /// 输出/副作用：先记入待发送状态；拿到插件锁时立即发送，否则由下一块补发。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / runtime
    /// Purpose: Push the transport playing state into the plugin's ProcessContext; call it only
    /// when the state changes.
    /// Inputs: `playing`.
    /// Outputs / side effects: records it as pending, sends immediately when the lock is free and
    /// otherwise by the next block.
    /// Complexity: O(1) time and space.
    pub fn set_playing(&mut self, playing: bool) {
        let PluginRuntime::Vst3(v) = self else {
            return;
        };
        // 插件锁被占用时不能丢弃状态：VOCALOID 等插件收不到 kPlaying 就不会推进内部时间轴。
        // Never drop the state when the lock is busy: without kPlaying, plugins such as VOCALOID
        // never advance their internal timeline.
        v.pending_playing = Some(playing);
        if let Some(mut plugin) = lock_plugin(&v.plugin) {
            let _ = plugin.set_playing(playing);
            v.pending_playing = None;
        }
    }

    /// 所属模块：plugin / 运行时
    /// 功能：把走带位置写进插件 ProcessContext；只在定位、开始播放与循环回绕时调用。
    /// 输入：`quarter_notes` 四分音符位置（拍）、`samples` 工程采样位置。
    /// 输出/副作用：先记入待发送状态；拿到插件锁时立即发送，否则由下一块补发。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / runtime
    /// Purpose: Push the transport position into the plugin ProcessContext; call it only on seek,
    /// playback start and loop wrap.
    /// Inputs: `quarter_notes` (beats), `samples` (project sample position).
    /// Outputs / side effects: records it as pending, sends immediately when the lock is free and
    /// otherwise by the next block.
    /// Complexity: O(1) time and space.
    pub fn seek_transport(&mut self, quarter_notes: f64, samples: i64) {
        let PluginRuntime::Vst3(v) = self else {
            return;
        };
        // 每个 block 都调用会让插件反复重置内部定位（VOCALOID 6 会直接卡住并产生巨大延迟），
        // 因此位置只在时间轴不连续时发送，连续播放由插件自行推进。
        // 锁被占用时保留待发送位置：定位丢了会让插件从旧位置继续播放，听感上就是“不跟随走带”。
        // Calling this every block makes plugins re-seek constantly (VOCALOID 6 freezes and adds
        // huge latency), so the position is sent only on discontinuities and the plugin advances on
        // its own during continuous playback. A busy lock keeps the position pending: a dropped seek
        // makes the plugin keep playing from the old position, heard as "not following the host".
        let position = vst3_host::TransportPosition {
            samples,
            quarter_notes,
        };
        v.pending_transport = Some(position);
        if let Some(mut plugin) = lock_plugin(&v.plugin) {
            let _ = plugin.set_transport_position(position);
            v.pending_transport = None;
        }
    }

    /// 所属模块：plugin / 运行时
    /// 功能：返回插件上报的处理延迟（采样数），用于轨道延迟补偿。
    /// 输入：`self`。
    /// 输出/副作用：插件忙（音频线程正在处理）时返回上一次缓存值；VST2 与失败实例返回 0。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / runtime
    /// Purpose: Return the reported processing latency in samples for track delay compensation.
    /// Inputs: `self`.
    /// Outputs / side effects: the last cached value while the audio thread is inside the plugin;
    /// VST2 and failed runtimes report 0.
    /// Complexity: O(1) time and space.
    pub fn latency_samples(&mut self) -> usize {
        let PluginRuntime::Vst3(v) = self else {
            return 0;
        };
        if let Some(plugin) = lock_plugin(&v.plugin) {
            v.latency_samples = plugin.latency_samples() as usize;
        }
        v.latency_samples
    }

    /// 所属模块：plugin / 运行时
    /// 功能：处理插件通过 `restartComponent` 提出的请求（延迟/总线变化），并刷新延迟缓存。
    /// 输入：`self`。
    /// 输出/副作用：必要时按 VST3 要求暂停/停用/重启用插件；返回延迟采样数是否变化。
    /// 插件锁被占用时直接跳过，UI 线程绝不阻塞。
    /// 复杂度：时间 O(插件请求数)，空间 O(1)。
    ///
    /// Module: plugin / runtime
    /// Purpose: Service requests the plugin raised through `restartComponent` (latency / bus
    /// changes) and refresh the cached latency.
    /// Inputs: `self`.
    /// Outputs / side effects: suspends/deactivates/reactivates the plugin as VST3 requires when
    /// needed; returns whether the latency in samples changed. Skipped while the plugin lock is
    /// busy so the UI thread never blocks.
    /// Complexity: O(plugin requests) time and O(1) space.
    pub fn refresh_host_requests(&mut self) -> bool {
        let PluginRuntime::Vst3(v) = self else {
            return false;
        };
        let Some(mut plugin) = lock_plugin(&v.plugin) else {
            return false;
        };
        // Pro-Q 4 这类插件在切换到线性相位或改变过采样时会发 kLatencyChanged；不处理的话宿主
        // 一直按旧延迟（常见是 0）补偿，该轨就会整体晚一截。
        // Plugins such as Pro-Q 4 raise kLatencyChanged when switching to linear phase or changing
        // oversampling. Ignoring it leaves the host compensating with the old latency (usually 0),
        // so that track stays late.
        let _ = plugin.service_host_requests();
        let latency = plugin.latency_samples() as usize;
        if latency == v.latency_samples {
            return false;
        }
        v.latency_samples = latency;
        true
    }

    /// 所属模块：plugin / 统一运行时
    /// 功能：重试此前被插件拒绝的状态恢复（编辑器就绪后通常即可成功）。
    /// 输入：`self`。
    /// 输出/副作用：成功时清空待恢复字节并写 `plugin.state`（`phase=retry`）；
    /// 插件忙或仍未接受时保持原样。
    /// 复杂度：时间 O(状态长度)，空间 O(1)。
    ///
    /// Module: plugin / unified runtime
    /// Purpose: Retry a state restore the plugin declined earlier (usually succeeds once its editor
    /// is ready).
    /// Inputs: `self`.
    /// Outputs / side effects: clears the pending bytes and writes `plugin.state`
    /// (`phase=retry`) on success; leaves everything as is while the plugin is busy or still
    /// declines.
    /// Complexity: O(state length) time and O(1) space.
    pub fn retry_state_restore(&mut self) {
        let PluginRuntime::Vst3(v) = self else {
            return;
        };
        let Some(state) = v.pending_state.clone() else {
            return;
        };
        // 状态块可达数十 KB；只在插件编辑器刚起来的窗口期重试有限次，避免每秒都塞给插件。
        // The blob can be tens of KB; retry a bounded number of times right after the editor comes
        // up instead of pushing it every second forever.
        v.state_retries = v.state_retries.saturating_add(1);
        if v.state_retries > 8 {
            v.pending_state = None;
            crate::diag::warn(
                "app.plugin",
                "plugin.state",
                &[("name", &v.name), ("status", "gave_up")],
            );
            return;
        }
        let Some(mut plugin) = lock_plugin(&v.plugin) else {
            return;
        };
        if plugin.load_state(&state).is_ok() {
            v.pending_state = None;
            crate::diag::info(
                "app.plugin",
                "plugin.state",
                &[("name", &v.name), ("status", "ok"), ("phase", "retry")],
            );
        }
    }

    /// 所属模块：plugin / 运行时
    /// 功能：读取插件内部走带位置（采样数与拍），用于确认插件是否跟随宿主播放。
    /// 输入：`self`。
    /// 输出/副作用：插件忙或非 VST3 时返回 `None`；不修改插件状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / runtime
    /// Purpose: Read the plugin's internal transport position (samples and beats) to verify it
    /// follows host playback.
    /// Inputs: `self`.
    /// Outputs / side effects: `None` when the plugin is busy or not VST3; no state change.
    /// Complexity: O(1) time and space.
    pub fn transport_snapshot(&self) -> Option<(i64, f64)> {
        let PluginRuntime::Vst3(v) = self else {
            return None;
        };
        let plugin = v.plugin.try_lock().ok()?;
        plugin
            .transport_position()
            .ok()
            .map(|position| (position.samples, position.quarter_notes))
    }

    /// Poll plugin editors for parameter gestures made behind our back.
    ///
    /// `full_scan` additionally compares every VST3 parameter value against the
    /// cached snapshot. Some plugins change their parameters without emitting
    /// `performEdit`, so the cheap gesture log alone misses them.
    pub fn poll_parameters_changed(&mut self, full_scan: bool) -> bool {
        match self {
            PluginRuntime::Vst3(v) => {
                // 插件正被音频/渲染线程使用时跳过本轮轮询，绝不阻塞 UI。
                // Skip this poll while the audio/render thread is inside the
                // plugin; the UI thread must never block on it.
                let Some(mut plugin) = lock_plugin(&v.plugin) else {
                    return false;
                };
                // Draining the gesture log is required even when we fall back
                // to a full scan, otherwise the queue grows without bound.
                let edits = plugin.take_parameter_edits();
                let mut changed_ids: Vec<(u32, f64)> = edits
                    .into_iter()
                    .filter_map(|edit| edit.value.map(|value| (edit.id, value)))
                    .collect();
                if changed_ids.is_empty() && full_scan && v.params.len() <= MAX_FULL_SCAN_PARAMS {
                    if let Ok(list) = plugin.get_parameters() {
                        // 先建索引再比对：参数多时逐个 `find` 是平方复杂度，会把 UI 线程和
                        // 引擎锁一起拖住。
                        // Index first, then compare: a per-parameter `find` is quadratic and would
                        // hold both the UI thread and the engine lock.
                        let live: HashMap<u32, f64> =
                            list.into_iter().map(|item| (item.id, item.value)).collect();
                        for param in &mut v.params {
                            let Some(&value) = live.get(&param.id) else {
                                continue;
                            };
                            if (value - param.value).abs() > 1e-4 {
                                param.value = value;
                                changed_ids.push((param.id, value));
                            }
                        }
                    }
                }
                let changed = !changed_ids.is_empty();
                for (id, value) in changed_ids {
                    let display = plugin
                        .format_parameter(id, value)
                        .unwrap_or_else(|_| format!("{:.2}", value * 100.0));
                    if let Some(param) = v.params.iter_mut().find(|param| param.id == id) {
                        param.value = value;
                        param.display = display;
                    }
                }
                changed
            }
            PluginRuntime::Vst2(v) => {
                // VST2 has no edit notification: compare the GUI-visible
                // parameter values with our snapshot. Only the changed entries
                // get their (expensive) display text refreshed.
                let obj = v.instance.get_parameter_object();
                let mut changed = false;
                for (index, param) in v.params.iter_mut().enumerate() {
                    let value = obj.get_parameter(index as i32) as f64;
                    if (value - param.value).abs() > 1e-4 {
                        param.value = value;
                        let display = obj.get_parameter_text(index as i32);
                        param.display = if display.is_empty() {
                            format!("{:.0}%", value * 100.0)
                        } else {
                            display
                        };
                        changed = true;
                    }
                }
                changed
            }
            PluginRuntime::Failed(_) => false,
        }
    }

    pub fn kind(&self) -> PluginKind {
        match self {
            PluginRuntime::Vst3(v) => v.kind,
            PluginRuntime::Vst2(v) => {
                if v.is_instrument {
                    PluginKind::Instrument
                } else {
                    PluginKind::Effect
                }
            }
            PluginRuntime::Failed(_) => PluginKind::Unknown,
        }
    }

    /// Whether the attached editor accepts resizing. Fixed-size (or
    /// fixed-aspect) editors return `false` here once attached.
    /// 编辑器是否可缩放。插件实例正被音频/渲染线程使用时返回 `None`，
    /// 由 UI 沿用缓存值，避免阻塞主线程。
    /// Whether the editor can resize. `None` while the plugin is busy on the
    /// audio/render thread so the UI keeps its cached value instead of blocking.
    pub fn editor_can_resize(&self) -> Option<bool> {
        match self {
            PluginRuntime::Vst3(v) => {
                let plugin = lock_plugin(&v.plugin)?;
                if let Some(can_resize) = plugin.attached_editor_can_resize() {
                    return Some(can_resize);
                }
                Some(plugin.editor_can_resize())
            }
            PluginRuntime::Vst2(_) | PluginRuntime::Failed(_) => Some(false),
        }
    }

    /// Size the plugin editor reports for itself, in physical pixels.
    pub fn editor_size(&self) -> Option<(f32, f32)> {
        match self {
            PluginRuntime::Vst3(v) => lock_plugin(&v.plugin)?
                .get_editor_size()
                .ok()
                .map(|(width, height)| (width as f32, height as f32)),
            PluginRuntime::Vst2(_) | PluginRuntime::Failed(_) => None,
        }
    }

    pub fn panic(&mut self) {
        match self {
            PluginRuntime::Vst3(v) => {
                // 音频线程绝不等待插件锁：GUI 初始化/参数轮询可能长时间持有它。
                // The audio thread never waits for the plugin lock: GUI initialisation or parameter
                // polling may hold it for a long time.
                if let Some(mut plugin) = lock_plugin(&v.plugin) {
                    let _ = plugin.midi_panic();
                }
            }
            PluginRuntime::Vst2(v) => {
                let ev = make_vst2_cc(123, 0);
                send_vst2_events(&mut v.instance, &[ev]);
            }
            PluginRuntime::Failed(_) => {}
        }
    }

    /// 所属模块：plugin / 统一运行时
    /// 功能：把处理器输出的参数反馈回写到 `IEditController`，并同步宿主的参数快照。
    /// 输入：`self`。
    /// 输出/副作用：可能调用 `setParamNormalized` 并更新参数显示；插件锁忙时跳过本轮。
    /// 复杂度：时间 O(本块反馈点数 + 参数数)，空间 O(本块反馈点数)。
    ///
    /// Module: plugin / unified runtime
    /// Purpose: Mirror processor output-parameter feedback onto `IEditController` and refresh
    /// the host parameter snapshot.
    /// Inputs: `self`.
    /// Outputs / side effects: may call `setParamNormalized` and update parameter displays;
    /// skips this round while the plugin lock is held by the audio thread.
    /// Complexity: O(feedback points + parameters) time and O(feedback points) space.
    pub fn pump_editor_feedback(&mut self) {
        let PluginRuntime::Vst3(v) = self else {
            return;
        };
        // 音频线程正在处理该插件时跳过本轮，绝不阻塞 UI。
        // Skip this round while the audio thread is inside the plugin; never block the UI.
        let Some(plugin) = lock_plugin(&v.plugin) else {
            return;
        };
        // 每帧最多格式化前 64 条：分析器/电平参数可能成百上千条，逐条调用插件的
        // `format_parameter` 会长时间占用引擎锁；剩余值下一帧继续刷新。
        // Format at most 64 entries per frame: analysers and meters can report hundreds of values,
        // and calling the plugin's `format_parameter` for each one holds the engine lock for too
        // long; the rest refresh on the next frame.
        for (id, value) in plugin
            .get_parameter_changes()
            .into_iter()
            .take(MAX_EDITOR_FEEDBACK_UPDATES)
        {
            if let Some(param) = v.params.iter_mut().find(|param| param.id == id) {
                param.value = value;
                param.display = plugin
                    .format_parameter(id, value)
                    .unwrap_or_else(|_| format!("{:.2}", value * 100.0));
            }
        }
    }

    pub fn send_cc(&mut self, controller: u8, value: u8) {
        match self {
            PluginRuntime::Vst3(v) => {
                if let Some(mut plugin) = lock_plugin(&v.plugin) {
                    let _ = plugin.send_midi_event(Vst3MidiEvent::ControlChange {
                        channel: MidiChannel::Ch1,
                        controller,
                        value: value & 0x7F,
                    });
                }
            }
            PluginRuntime::Vst2(v) => {
                let event = make_vst2_cc(controller, value);
                send_vst2_events(&mut v.instance, &[event]);
            }
            PluginRuntime::Failed(_) => {}
        }
    }

    pub fn send_pitch_bend(&mut self, normalized: f32) {
        let value = (normalized.clamp(0.0, 1.0) * 16_383.0).round() as u16;
        match self {
            PluginRuntime::Vst3(v) => {
                if let Some(mut plugin) = lock_plugin(&v.plugin) {
                    let _ = plugin.send_midi_event(Vst3MidiEvent::PitchBend {
                        channel: MidiChannel::Ch1,
                        value,
                    });
                }
            }
            PluginRuntime::Vst2(v) => {
                let lsb = (value & 0x7F) as u8;
                let msb = ((value >> 7) & 0x7F) as u8;
                let event = vst::api::MidiEvent {
                    event_type: vst::api::EventType::Midi,
                    byte_size: std::mem::size_of::<vst::api::MidiEvent>() as i32,
                    delta_frames: 0,
                    flags: 0,
                    note_length: 0,
                    note_offset: 0,
                    midi_data: [0xE0, lsb, msb],
                    _midi_reserved: 0,
                    detune: 0,
                    note_off_velocity: 0,
                    _reserved1: 0,
                    _reserved2: 0,
                };
                send_vst2_events(&mut v.instance, &[event]);
            }
            PluginRuntime::Failed(_) => {}
        }
    }

    pub fn process(
        &mut self,
        sample_rate: f64,
        frames: usize,
        input: Option<(&[f32], &[f32])>,
        gates: &[SynthEvent],
        out_l: &mut [f32],
        out_r: &mut [f32],
        transport: Option<vst3_host::TransportPosition>,
    ) {
        out_l.fill(0.0);
        out_r.fill(0.0);
        match self {
            PluginRuntime::Vst3(v) => {
                // GUI 打开/初始化期间插件可能长时间持锁；音频线程必须跳过本块而不是等待，
                // 否则整个输出流会停住（表现为 Kontakt 等插件“没有声音”）。
                // A plugin may hold its lock during GUI initialisation; the audio thread must skip this
                // block instead of waiting, otherwise the whole output stream stalls (heard as silence
                // with plugins such as Kontakt).
                let Some(mut plugin) = lock_plugin(&v.plugin) else {
                    // 本块该插件没有参与处理：统计一次，并记下宿主位置。
                    // 插件内部的走带只在真正处理时推进，跳过一块它就会落后一份；
                    // 不补发位置的话，每跳过一次就永久多欠一截，听感是“越放越晚”，
                    // VOCALOID 这类插件连编辑器都停在旧位置。
                    // This plugin did not process the block: count it and remember the host position.
                    // Its internal timeline only advances while it processes, so a skipped block makes
                    // it lag; without re-sending the position every skip adds permanent delay ("gets
                    // later and later") and editors such as VOCALOID stay at the old position.
                    crate::diag::note_plugin_lock_miss();
                    if let Some(position) = transport {
                        v.pending_transport = Some(position);
                    }
                    return;
                };
                // 每块都把宿主位置写进 ProcessContext（与 Cubase 等宿主一致）：插件据此推进
                // 编辑器里的播放头与内部时间轴。这里只改结构体字段，不会触发插件的重新定位。
                // VOCALOID 6 这类“独立编辑器窗口”的插件只有收到每块的宿主位置才会跟着走条。
                // Write the host position into the ProcessContext on every block, as Cubase and
                // other DAWs do: editors such as VOCALOID 6 follow the transport from it. This only
                // updates struct fields and never makes the plugin re-seek.
                if let Some(position) = transport {
                    let _ = plugin.set_transport_position(position);
                    v.pending_transport = None;
                } else if let Some(position) = v.pending_transport.take() {
                    // 没有本块位置时（效果器）才补发旧记录，绝不覆盖新的宿主位置。
                    // Only without a current block position (effects) is the older record delivered;
                    // it must never override the fresh host position.
                    let _ = plugin.set_transport_position(position);
                }
                // 补发此前因插件忙而没能送达的播放状态。
                // Deliver the playing state that could not be sent earlier because the plugin was busy.
                if let Some(playing) = v.pending_playing.take() {
                    let _ = plugin.set_playing(playing);
                }
                for gate in gates {
                    let event = match gate.kind {
                        GateKind::On => Vst3MidiEvent::NoteOn {
                            channel: MidiChannel::Ch1,
                            note: gate.pitch,
                            velocity: gate.velocity,
                        },
                        GateKind::Off => Vst3MidiEvent::NoteOff {
                            channel: MidiChannel::Ch1,
                            note: gate.pitch,
                            velocity: 0,
                        },
                    };
                    let _ = plugin.send_midi_event_at(event, gate.frame as i32);
                }

                // 实时线程只在容量不足时分配缓冲，其余时间复用：块长在 512/512/128 之间
                // 来回变化时，按块长重建会让每个插件每块做多次堆分配。
                // The realtime thread allocates only when the capacity is too small and reuses the
                // buffers otherwise: rebuilding by block length makes every plugin allocate several
                // times per callback while the block length alternates between 512/512/128.
                let capacity = v.audio_buffers.as_ref().map(|buffers| buffers.block_size);
                let needs_buffers = match capacity {
                    Some(capacity) => {
                        capacity < frames
                            || v.audio_buffers.as_ref().is_none_or(|buffers| {
                                buffers.inputs.len() < 2 || buffers.outputs.len() < 2
                            })
                    }
                    None => true,
                };
                if needs_buffers {
                    let block_size = capacity.unwrap_or(0).max(frames);
                    v.audio_buffers =
                        Some(vst3_host::AudioBuffers::new(2, 2, block_size, sample_rate));
                }
                if let Some(buffers) = v.audio_buffers.as_mut() {
                    buffers.sample_rate = sample_rate;
                    // 先清零再按本块长度收缩/展开：容量保留，插件看到的仍是本块真实帧数。
                    // Clear, then shrink/grow to this block's length: capacity is kept while the
                    // plugin still sees the true frame count of this block.
                    for channel in buffers.inputs.iter_mut().chain(buffers.outputs.iter_mut()) {
                        channel.clear();
                        channel.resize(frames, 0.0);
                    }
                    if let Some((l, r)) = input {
                        if let Some(dst) = buffers.inputs.get_mut(0) {
                            dst[..frames.min(l.len())].copy_from_slice(&l[..frames.min(l.len())]);
                        }
                        if let Some(dst) = buffers.inputs.get_mut(1) {
                            dst[..frames.min(r.len())].copy_from_slice(&r[..frames.min(r.len())]);
                        }
                    }
                    if let Err(error) = plugin.process_audio(buffers) {
                        // 插件拒绝处理时输出保持静音；失败原因必须留痕，否则现象只是“没有声音”。
                        // A rejected process leaves the output silent; the reason must be logged or
                        // the only symptom is "no sound".
                        use std::sync::atomic::{AtomicU32, Ordering};
                        static WARNED: AtomicU32 = AtomicU32::new(0);
                        if WARNED.fetch_add(1, Ordering::Relaxed) < 5 {
                            crate::diag::warn(
                                "plugin.runtime",
                                "plugin.process_failed",
                                &[("plugin", &v.name), ("error", &error.to_string())],
                            );
                        }
                    } else {
                        let first = buffers.outputs.first();
                        if let Some(src) = first {
                            let n = frames.min(src.len());
                            out_l[..n].copy_from_slice(&src[..n]);
                        }
                        let second = if v.out_channels <= 1 {
                            buffers.outputs.first()
                        } else {
                            buffers.outputs.get(1).or_else(|| buffers.outputs.first())
                        };
                        if let Some(src) = second {
                            let n = frames.min(src.len());
                            out_r[..n].copy_from_slice(&src[..n]);
                        }
                        // 同步桥不发声：保留输入原样，编辑器里的试听与 VSTi 人声都不会被
                        // 它的静音渲染替换。走带/速度已经在上面的 `process_audio` 里送达。
                        // A sync bridge makes no sound: keep the input untouched so its silent render
                        // can never replace the editor preview or the VOCALOID VSTi output; transport
                        // was already delivered by `process_audio` above.
                        if v.sync_only {
                            if let Some((l, r)) = input {
                                let n = frames.min(l.len()).min(r.len());
                                out_l[..n].copy_from_slice(&l[..n]);
                                out_r[..n].copy_from_slice(&r[..n]);
                            }
                            // 只需一条证据：Bridge 是否真的在走带时被处理（收到 ProcessContext）。
                            // One proof line is enough: is the Bridge really processed during
                            // playback (i.e. does it receive the ProcessContext)?
                            use std::sync::atomic::{AtomicBool, Ordering};
                            static PROCESSED: AtomicBool = AtomicBool::new(false);
                            if !PROCESSED.swap(true, Ordering::Relaxed) {
                                let samples = transport.map(|p| p.samples).unwrap_or(-1);
                                let music = transport.map(|p| p.quarter_notes).unwrap_or(-1.0);
                                crate::diag::info(
                                    "plugin.ara",
                                    "ara.bridge_processed",
                                    &[
                                        ("plugin", &v.name),
                                        ("samples", &samples.to_string()),
                                        ("quarter_notes", &format!("{music:.3}")),
                                    ],
                                );
                            }
                        }
                    }
                }
            }
            PluginRuntime::Vst2(v) => {
                send_vst2_gates(&mut v.instance, gates);

                let in_count = v.num_inputs.max(0);
                let out_count = v.num_outputs.max(1);
                // 复用通道缓冲：只在声道数或块大小变化时重新分配。
                // Reuse the channel buffers: allocate only when the channel count or block size
                // changes.
                let needs_inputs = v.process_inputs.len() != in_count
                    || v.process_inputs.iter().any(|data| data.len() != frames);
                if needs_inputs {
                    v.process_inputs = (0..in_count).map(|_| vec![0.0_f32; frames]).collect();
                } else {
                    for data in &mut v.process_inputs {
                        data.fill(0.0);
                    }
                }
                let needs_outputs = v.process_outputs.len() != out_count
                    || v.process_outputs.iter().any(|data| data.len() != frames);
                if needs_outputs {
                    v.process_outputs = (0..out_count).map(|_| vec![0.0_f32; frames]).collect();
                } else {
                    for data in &mut v.process_outputs {
                        data.fill(0.0);
                    }
                }
                if let Some((l, r)) = input {
                    for (ch, data) in v.process_inputs.iter_mut().enumerate() {
                        let src = if ch == 0 {
                            l
                        } else if ch == 1 {
                            r
                        } else if in_count == 1 {
                            l
                        } else {
                            r
                        };
                        let n = frames.min(src.len());
                        data[..n].copy_from_slice(&src[..n]);
                    }
                }
                let inputs = &v.process_inputs;
                let outputs = &mut v.process_outputs;
                let mut host_buffer = vst::host::HostBuffer::<f32>::new(in_count, out_count);
                let mut audio_buffer = host_buffer.bind(inputs, outputs);
                v.instance.process(&mut audio_buffer);
                if let Some(first) = v.process_outputs.first() {
                    let n = frames.min(first.len());
                    out_l[..n].copy_from_slice(&first[..n]);
                    if let Some(second) = v.process_outputs.get(1) {
                        out_r[..n].copy_from_slice(&second[..n]);
                    } else {
                        out_r[..n].copy_from_slice(&first[..n]);
                    }
                }
            }
            PluginRuntime::Failed(_) => {}
        }
    }
}

/// 非阻塞地借用 VST3 插件实例；被音频/渲染线程占用时返回 `None`。
/// 中毒的锁照常恢复，避免一次无关 panic 让参数界面永久失效。
///
/// Borrow a VST3 plugin instance without blocking; `None` while the
/// audio/render thread is inside it. Poisoned locks are recovered so an
/// unrelated panic cannot disable the parameter UI forever.
fn lock_plugin(plugin: &Mutex<Vst3Plugin>) -> Option<std::sync::MutexGuard<'_, Vst3Plugin>> {
    match plugin.try_lock() {
        Ok(guard) => Some(guard),
        Err(std::sync::TryLockError::Poisoned(poison)) => Some(poison.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => None,
    }
}
