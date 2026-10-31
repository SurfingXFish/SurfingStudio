//! Unified plugin runtime used by the audio engine.

use super::backends::{make_vst2_cc, send_vst2_events, send_vst2_gates, Vst2Host};
use super::{ParamView, PluginKind};
use crate::audio::synth::{GateKind, SynthEvent};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use vst::plugin::Plugin as VstPluginTrait;
use vst3_host::midi::{MidiChannel, MidiEvent as Vst3MidiEvent};
use vst3_host::Plugin as Vst3Plugin;

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

pub struct Vst3Runtime {
    pub plugin: Arc<Mutex<Vst3Plugin>>,
    pub name: String,
    pub params: Vec<ParamView>,
    pub out_channels: usize,
    pub has_editor: bool,
    pub kind: PluginKind,
    /// 实时线程复用的处理缓冲；块大小变化时才重新分配。
    /// Processing buffers reused by the realtime thread; reallocated only when the block size changes.
    pub audio_buffers: Option<vst3_host::AudioBuffers>,
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
    /// `Plugin` 强引用，避免在 UI 线程运行插件析构（VOCALOID/ACE Bridge 会等待消息循环）；
    /// VST2/失败态不做处理。
    /// 复杂度：时间 O(1 + 插件终止)，空间 O(1)（泄漏一个实例）。
    ///
    /// Module: plugin / unified runtime
    /// Purpose: Terminate the plugin component while keeping the Rust instance alive, so closing a
    /// project releases plugin-spawned external app windows.
    /// Inputs: `self`.
    /// Outputs / side effects: VST3 calls `shutdown_component`, then replaces itself with `Failed`
    /// and leaks one strong `Plugin` reference so the plugin destructor never runs on the UI thread
    /// (VOCALOID/ACE Bridge wait for a message loop); VST2/failed runtimes are untouched.
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
        // 泄漏一份强引用：Vst3Runtime 被替换后，插件对象与已终止的组件都不会析构。
        // Leak one strong reference: after Vst3Runtime is replaced, neither the object nor the
        // terminated component is destroyed.
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

    pub fn restore_state(&mut self, state: &[u8], params: &[f32]) {
        let mut refresh = false;
        match self {
            PluginRuntime::Vst3(v) => {
                if !state.is_empty() {
                    let mut plugin = v.plugin.lock().unwrap_or_else(|p| p.into_inner());
                    let _ = plugin.load_state(state);
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
                    return;
                };
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

                // 实时线程只在块大小变化时分配缓冲，其余时间复用并清零。
                // The realtime thread allocates only when the block size changes and reuses the
                // buffers otherwise.
                let needs_buffers = match &v.audio_buffers {
                    Some(buffers) => {
                        buffers.block_size != frames
                            || buffers.inputs.len() < 2
                            || buffers.outputs.len() < 2
                    }
                    None => true,
                };
                if needs_buffers {
                    v.audio_buffers = Some(vst3_host::AudioBuffers::new(2, 2, frames, sample_rate));
                }
                if let Some(buffers) = v.audio_buffers.as_mut() {
                    buffers.sample_rate = sample_rate;
                    buffers.clear();
                    if let Some((l, r)) = input {
                        if let Some(dst) = buffers.inputs.get_mut(0) {
                            dst[..frames.min(l.len())].copy_from_slice(&l[..frames.min(l.len())]);
                        }
                        if let Some(dst) = buffers.inputs.get_mut(1) {
                            dst[..frames.min(r.len())].copy_from_slice(&r[..frames.min(r.len())]);
                        }
                    }
                    if plugin.process_audio(buffers).is_ok() {
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
