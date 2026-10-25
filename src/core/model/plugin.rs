//! 所属模块：core / 数据模型 / 插件引用
//! 文件用途：
//! - 定义插件格式与 `PluginRef`：路径、名称、UID、状态块、参数快照与启用开关。
//! - 提供插件身份键，供运行时判断实例是否可复用。
//!
//! 函数清单：
//! - `PluginFormat::label`：格式显示名。
//! - `PluginRef::new`：创建默认启用的插件引用。
//! - `PluginRef::key`：格式 + uid + 路径的身份字符串。
//! - `default_state` / `default_restore_state` / `default_enabled`：serde 默认值。
//!
//! Module: core / data model / plugin reference
//! Purpose:
//! - Define plugin formats and `PluginRef`: path, name, UID, state blob, parameter snapshot and
//!   the enable switch.
//! - Provide the identity key used to decide whether a runtime instance can be reused.
//!
//! Functions:
//! - `PluginFormat::label`: display name of the format.
//! - `PluginRef::new`: create a plugin reference that is enabled by default.
//! - `PluginRef::key`: identity string of format + uid + path.
//! - `default_state` / `default_restore_state` / `default_enabled`: serde defaults.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginFormat {
    Vst3,
    Vst2,
}

impl PluginFormat {
    /// 所属模块：core / 数据模型 / 插件引用
    /// 功能：返回插件格式的显示名（VST3 / VST2）。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / plugin reference
    /// Purpose: Return the display name of the plugin format (VST3 / VST2).
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(&self) -> &'static str {
        match self {
            PluginFormat::Vst3 => "VST3",
            PluginFormat::Vst2 => "VST2",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginRef {
    pub format: PluginFormat,
    pub path: PathBuf,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub uid: String,
    /// Opaque plugin state captured when the project is saved (VST3 only).
    /// Stored as a separate binary chunk by the project container, so it is
    /// never serialized inline. `Arc` keeps project snapshots (undo, save)
    /// cheap even when a plugin state is several megabytes.
    #[serde(skip, default = "default_state")]
    pub state: Arc<[u8]>,
    /// 是否允许把 `state` 自动传给插件。导入来源（SoundBug）的部分状态并非
    /// 标准 VST3 状态，直接恢复会让个别插件崩溃；此类状态只保留数据，不自动应用。
    /// Whether `state` may be applied automatically. Some imported SoundBug
    /// states are not standard VST3 states; restoring them can crash plugins,
    /// so they are preserved but not applied automatically.
    #[serde(default = "default_restore_state")]
    pub restore_state: bool,
    /// Normalized parameter snapshot, used as a fallback for VST2 and as a quick restore.
    #[serde(default)]
    pub params: Vec<f32>,
    /// 该效果器是否参与音频处理；关闭时旁路但保留实例、参数与状态。
    /// Whether this effect processes audio; when disabled it is bypassed while the
    /// instance, parameters and state are kept.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// 所属模块：core / 数据模型 / 插件引用
/// 功能：`state` 的 serde 默认值：空状态块。
/// 输入：无。
/// 输出/副作用：返回空 `Arc<[u8]>`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / plugin reference
/// Purpose: serde default for `state`: an empty state blob.
/// Inputs: none.
/// Outputs / side effects: empty `Arc<[u8]>`; no state change.
/// Complexity: O(1) time and space.
fn default_state() -> Arc<[u8]> {
    Arc::from(Vec::new())
}

/// 所属模块：core / 数据模型 / 插件引用
/// 功能：`restore_state` 的 serde 默认值：允许自动恢复状态。
/// 输入：无。
/// 输出/副作用：返回 `true`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / plugin reference
/// Purpose: serde default for `restore_state`: allow automatic state restore.
/// Inputs: none.
/// Outputs / side effects: `true`; no state change.
/// Complexity: O(1) time and space.
fn default_restore_state() -> bool {
    true
}

/// 所属模块：core / 数据模型 / 插件引用
/// 功能：`enabled` 的 serde 默认值：旧工程加载后效果器默认生效。
/// 输入：无。
/// 输出/副作用：返回 `true`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / plugin reference
/// Purpose: serde default for `enabled`: effects from older projects stay active.
/// Inputs: none.
/// Outputs / side effects: `true`; no state change.
/// Complexity: O(1) time and space.
fn default_enabled() -> bool {
    true
}

impl PluginRef {
    /// 所属模块：core / 数据模型 / 插件引用
    /// 功能：创建一个默认启用、无状态与参数的插件引用。
    /// 输入：
    /// - `format`：插件格式。
    /// - `path`：插件路径。
    /// - `name`：显示名称。
    /// 输出/副作用：返回新的 `PluginRef`；不修改状态。
    /// 复杂度：时间 O(名称长度)，空间 O(名称长度)。
    ///
    /// Module: core / data model / plugin reference
    /// Purpose: Create a plugin reference that is enabled by default with empty state and params.
    /// Inputs:
    /// - `format`: plugin format.
    /// - `path`: plugin path.
    /// - `name`: display name.
    /// Outputs / side effects: a new `PluginRef`; no state change.
    /// Complexity: O(name length) time and space.
    pub fn new(format: PluginFormat, path: PathBuf, name: String) -> Self {
        Self {
            format,
            path,
            name,
            uid: String::new(),
            state: Arc::from(Vec::new()),
            restore_state: true,
            params: Vec::new(),
            enabled: true,
        }
    }

    /// 所属模块：core / 数据模型 / 插件引用
    /// 功能：返回插件身份键（格式 + uid + 路径），用于判断运行时实例能否复用。
    /// 输入：`self`。
    /// 输出/副作用：身份字符串；不修改状态。
    /// 复杂度：时间 O(路径长度)，空间 O(身份字符串长度)。
    ///
    /// Module: core / data model / plugin reference
    /// Purpose: Return the plugin identity key (format + uid + path) used to decide whether a
    /// runtime instance can be reused.
    /// Inputs: `self`.
    /// Outputs / side effects: identity string; no state change.
    /// Complexity: O(path length) time and O(key length) space.
    pub fn key(&self) -> String {
        format!(
            "{}:{}:{}",
            self.format.label(),
            self.uid,
            self.path.to_string_lossy()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：旧工程缺少 `enabled` 字段时，插件应默认启用。
    /// Covers: a plugin defaults to enabled when an older project omits `enabled`.
    #[test]
    fn enabled_defaults_to_true_when_the_field_is_missing() {
        let json = r#"{"format":"Vst3","path":"C:/plugins/test.vst3","name":"Test"}"#;
        let plugin: PluginRef = serde_json::from_str(json).expect("deserialize plugin");
        assert!(
            plugin.enabled,
            "legacy projects must load with enabled = true"
        );
        assert!(
            plugin.restore_state,
            "legacy projects keep state restore enabled"
        );
    }
}
