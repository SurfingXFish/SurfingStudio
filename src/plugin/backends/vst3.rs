//! VST3 后端：加载、参数收集与扫描适配。
//! VST3 backend: loading, parameter collection and scan adapter.

use super::super::backend::{PluginBackend, PluginBackendInfo};
use super::super::scan::{classify_vst3, probe_vst3_path, scan_vst3};
use super::super::{ParamView, PluginRuntime, ScannedPlugin, Vst3Runtime};
use crate::core::model::PluginFormat;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use vst3_host::{Plugin as Vst3Plugin, Vst3Host};

/// VST3 格式后端。
/// VST3 format backend.
pub struct Vst3Backend;

impl PluginBackend for Vst3Backend {
    /// 所属模块：plugin / 后端 / VST3
    /// 功能：返回 VST3 后端能力。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回能力描述；不修改状态。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / backends / VST3
    /// Purpose: Return the VST3 backend capabilities.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the descriptor; no state change.
    ///
    /// Complexity: O(1) time and space.
    fn info(&self) -> PluginBackendInfo {
        PluginBackendInfo {
            format: PluginFormat::Vst3,
            label: "VST3",
            can_scan: true,
        }
    }

    /// 所属模块：plugin / 后端 / VST3
    /// 功能：加载 VST3 插件实例。
    /// 输入：同 `load`。
    ///
    /// 输出/副作用：返回统一插件运行时；失败时返回中文上下文错误。
    ///
    /// 复杂度：时间 O(插件初始化)，空间 O(插件状态)。
    ///
    /// Module: plugin / backends / VST3
    /// Purpose: Load a VST3 plugin instance.
    /// Inputs: same as `load`.
    ///
    /// Outputs / side effects: returns the unified plugin runtime; failures carry
    /// Chinese context.
    ///
    /// Complexity: O(plugin initialization) time and O(plugin state) space.
    fn load(
        &self,
        path: &Path,
        uid: &str,
        sample_rate: f64,
        block_size: usize,
        bpm: f64,
        time_sig: (u8, u8),
    ) -> Result<PluginRuntime, String> {
        load(path, uid, sample_rate, block_size, bpm, time_sig)
    }

    /// 所属模块：plugin / 后端 / VST3
    /// 功能：扫描 VST3 插件。
    /// 输入：`custom_paths` 自定义扫描目录。
    ///
    /// 输出/副作用：返回扫描结果；只读文件系统。
    ///
    /// 复杂度：时间 O(目录项数 × 最大递归深度)。
    ///
    /// Module: plugin / backends / VST3
    /// Purpose: Scan VST3 plugins.
    /// Inputs: `custom_paths` — custom scan directories.
    ///
    /// Outputs / side effects: returns scan results; the filesystem is read-only.
    ///
    /// Complexity: O(directory entries × max recursion depth) time.
    fn scan(&self, custom_paths: &[PathBuf]) -> Result<Vec<ScannedPlugin>, String> {
        scan_vst3(custom_paths)
    }

    /// 所属模块：plugin / 后端 / VST3
    /// 功能：用工厂元数据补齐单个 VST3 插件的分类信息。
    /// 输入：`path` 插件路径。
    ///
    /// 输出/副作用：返回完整扫描记录；读取失败时为 `None`。
    ///
    /// 复杂度：时间 O(模块元数据读取)，空间 O(1)。
    ///
    /// Module: plugin / backends / VST3
    /// Purpose: Fill one VST3 plugin's classification from factory metadata.
    /// Inputs: `path` — plugin path.
    ///
    /// Outputs / side effects: a complete scan record, or `None` when reading fails.
    ///
    /// Complexity: O(module metadata read) time and O(1) space.
    fn probe(&self, path: &Path) -> Option<ScannedPlugin> {
        probe_vst3_path(path)
    }
}

fn collect_vst3_params(plugin: &Vst3Plugin) -> Vec<ParamView> {
    plugin
        .get_parameters()
        .unwrap_or_default()
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
        .collect()
}

pub(super) fn load(
    path: &Path,
    uid: &str,
    sample_rate: f64,
    block_size: usize,
    bpm: f64,
    time_sig: (u8, u8),
) -> Result<PluginRuntime, String> {
    let mut host = Vst3Host::builder()
        .sample_rate(sample_rate)
        .block_size(block_size)
        .input_channels(2)
        .output_channels(2)
        .tempo(bpm)
        .time_signature(time_sig.0 as i32, time_sig.1 as i32)
        .build()
        .map_err(|e| format!("VST3 宿主初始化失败: {e}"))?;

    let valid_uid = uid.len() == 32 && uid.chars().all(|c| c.is_ascii_hexdigit());
    let loaded = if valid_uid {
        host.load_plugin_class(path, uid)
    } else {
        host.load_plugin(path)
    };
    let mut plugin = match loaded {
        Ok(plugin) => plugin,
        Err(first) => {
            if valid_uid {
                host.load_plugin(path)
                    .map_err(|e| format!("VST3 加载失败: {first} / {e}"))?
            } else {
                return Err(format!("VST3 加载失败: {first}"));
            }
        }
    };
    let name = if plugin.info().name.is_empty() {
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "VST3 插件".into())
    } else {
        plugin.info().name.clone()
    };
    let params = collect_vst3_params(&plugin);
    let out_channels = plugin.output_channel_count();
    let has_editor = plugin.has_editor();
    let info = plugin.info();
    let kind = classify_vst3(&info.category, info.has_midi_input, info.audio_inputs);
    plugin
        .start_processing()
        .map_err(|e| format!("VST3 启动处理失败: {e}"))?;

    Ok(PluginRuntime::Vst3(Vst3Runtime {
        plugin: Arc::new(Mutex::new(plugin)),
        name,
        params,
        out_channels,
        has_editor,
        kind,
        audio_buffers: None,
    }))
}
