//! 所属模块：plugin / 后端接口
//! 文件用途：
//! - 定义插件格式后端的统一转译接口 `PluginBackend`。
//! - 描述每种后端支持的格式与扫描能力。
//! - 后端只负责各自格式的加载与扫描，不感知应用层。
//!
//! 函数清单：
//! - `PluginBackend::info` / `load` / `scan`：后端接口。
//!
//! Module: plugin / backend interface
//! Purpose:
//! - Define the unified plugin format backend interface `PluginBackend`.
//! - Describe the formats and scan capability of each backend.
//! - Backends only handle their own loading and scanning; the app is unaware of them.
//!
//! Functions:
//! - `PluginBackend::info` / `load` / `scan`: backend interface.

use super::{PluginRuntime, ScannedPlugin};
use crate::core::model::PluginFormat;
use std::path::{Path, PathBuf};

/// 插件后端的能力描述。
/// Capability descriptor of a plugin backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PluginBackendInfo {
    pub format: PluginFormat,
    pub label: &'static str,
    pub can_scan: bool,
}

/// 插件格式后端：把不同插件格式的加载/扫描差异收敛到同一接口。
/// Plugin format backend: unify loading/scanning differences of plugin formats.
pub trait PluginBackend: Send + Sync {
    /// 所属模块：plugin / 后端接口
    /// 功能：返回后端能力描述。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回能力描述；不修改状态。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / backend interface
    /// Purpose: Return the backend capability descriptor.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the descriptor; no state change.
    ///
    /// Complexity: O(1) time and space.
    fn info(&self) -> PluginBackendInfo;

    /// 所属模块：plugin / 后端接口
    /// 功能：加载一个插件实例并返回统一运行时。
    /// 输入：
    /// - `path`：插件路径。
    /// - `uid`：格式特定的类标识；不支持的格式可忽略。
    /// - `sample_rate`：采样率。
    /// - `block_size`：块大小。
    /// - `bpm` / `time_sig`：速度与拍号。
    ///
    /// 输出/副作用：返回 `PluginRuntime`；失败时返回中文上下文错误。
    ///
    /// 复杂度：时间 O(插件初始化)，空间 O(插件状态)。
    ///
    /// Module: plugin / backend interface
    /// Purpose: Load one plugin instance and return the unified runtime.
    /// Inputs:
    /// - `path`: plugin path.
    /// - `uid`: format-specific class id; may be ignored by formats that lack it.
    /// - `sample_rate`: sample rate.
    /// - `block_size`: block size.
    /// - `bpm` / `time_sig`: tempo and time signature.
    ///
    /// Outputs / side effects: returns `PluginRuntime`; failures carry Chinese context.
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
    ) -> Result<PluginRuntime, String>;

    /// 所属模块：plugin / 后端接口
    /// 功能：扫描该格式的插件。
    /// 输入：`custom_paths` 自定义扫描目录。
    ///
    /// 输出/副作用：返回扫描结果；只读文件系统。
    ///
    /// 复杂度：时间 O(目录项数 × 最大递归深度)。
    ///
    /// Module: plugin / backend interface
    /// Purpose: Scan plugins of this format.
    /// Inputs: `custom_paths` — custom scan directories.
    ///
    /// Outputs / side effects: returns scan results; the filesystem is read-only.
    ///
    /// Complexity: O(directory entries × max recursion depth) time.
    fn scan(&self, custom_paths: &[PathBuf]) -> Result<Vec<ScannedPlugin>, String>;

    /// 所属模块：plugin / 后端接口
    /// 功能：补齐单个插件的分类元数据（用于扫描后按需后台识别）。
    /// 输入：`path` 插件路径。
    ///
    /// 输出/副作用：返回完整扫描记录；不支持或失败时返回 `None`，不修改外部状态。
    ///
    /// 复杂度：时间 O(插件元数据读取)，空间 O(1)。
    ///
    /// Module: plugin / backend interface
    /// Purpose: Fill in classification metadata for one plugin (used for deferred
    /// background classification after a fast scan).
    /// Inputs: `path` — plugin path.
    ///
    /// Outputs / side effects: returns a complete scan record, or `None` when unsupported or
    /// failed; no external state change.
    ///
    /// Complexity: O(plugin metadata read) time and O(1) space.
    fn probe(&self, _path: &Path) -> Option<ScannedPlugin> {
        None
    }
}
