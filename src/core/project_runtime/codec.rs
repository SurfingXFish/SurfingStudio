//! 所属模块：core / 工程运行时 / 编解码接口
//! 文件用途：
//! - 定义格式后端的统一转译接口 `ProjectCodec`。
//! - 描述每种格式的能力（读取 / 写入）与扩展名。
//! - 后端只负责“外部字节 ↔ 规范工程表示”的转译，不感知应用层。
//!
//! 函数清单：
//! - `CodecInfo::supports`：扩展名匹配。
//! - `ProjectCodec::info` / `matches` / `decode` / `encode`：后端转译接口。
//!
//! Module: core / project runtime / codec interface
//! Purpose:
//! - Define the unified translation interface `ProjectCodec` for format backends.
//! - Describe per-format capabilities (read / write) and extensions.
//! - Backends only translate external bytes to/from the canonical project
//!   representation; they are unaware of the application layer.
//!
//! Functions:
//! - `CodecInfo::supports`: extension matching.
//! - `ProjectCodec::info` / `matches` / `decode` / `encode`: backend translation interface.

use super::{ProjectSession, WaveformPeak};
use crate::core::model::Project;
use std::path::Path;

/// 格式后端的能力描述。
/// Capability descriptor of a format backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodecInfo {
    /// 稳定的小写格式标识。
    /// Stable lowercase format id.
    pub id: &'static str,
    /// 面向用户的格式名称。
    /// User-facing format name.
    pub label: &'static str,
    /// 支持的扩展名（不含点，小写）。
    /// Supported extensions (lowercase, without the dot).
    pub extensions: &'static [&'static str],
    /// 是否支持把外部格式转译为规范工程。
    /// Whether the codec can translate external formats into the canonical project.
    pub can_read: bool,
    /// 是否支持把规范工程编码回外部格式。
    /// Whether the codec can encode the canonical project back to its format.
    pub can_write: bool,
}

impl CodecInfo {
    /// 所属模块：core / 工程运行时 / 编解码接口
    /// 功能：按扩展名判断该格式是否负责此路径（大小写不敏感）。
    /// 输入：`path` 文件路径。
    ///
    /// 输出/副作用：返回布尔值；不修改状态。
    ///
    /// 复杂度：时间 O(扩展名数量)，空间 O(1)。
    ///
    /// Module: core / project runtime / codec interface
    /// Purpose: Check whether the format handles the path by extension (case-insensitive).
    /// Inputs: `path` — file path.
    ///
    /// Outputs / side effects: returns a boolean; no state change.
    ///
    /// Complexity: O(extension count) time and O(1) space.
    pub fn supports(&self, path: &Path) -> bool {
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            return false;
        };
        self.extensions
            .iter()
            .any(|supported| extension.eq_ignore_ascii_case(supported))
    }
}

/// 格式后端转译接口：把外部工程格式转换为框架的规范表示，或反向编码。
/// Format backend translation interface: convert an external project format to the
/// framework's canonical representation, or encode it back.
pub trait ProjectCodec: Send + Sync {
    /// 所属模块：core / 工程运行时 / 编解码接口
    /// 功能：返回后端的能力描述。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回能力描述；不修改状态。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / codec interface
    /// Purpose: Return the backend capability descriptor.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the descriptor; no state change.
    ///
    /// Complexity: O(1) time and space.
    fn info(&self) -> CodecInfo;

    /// 所属模块：core / 工程运行时 / 编解码接口
    /// 功能：按扩展名判断该后端是否负责此路径。
    /// 输入：`path` 文件路径。
    ///
    /// 输出/副作用：返回布尔值；不修改状态。
    ///
    /// 复杂度：时间 O(扩展名数量)，空间 O(1)。
    ///
    /// Module: core / project runtime / codec interface
    /// Purpose: Check whether this backend handles the path by extension.
    /// Inputs: `path` — file path.
    ///
    /// Outputs / side effects: returns a boolean; no state change.
    ///
    /// Complexity: O(extension count) time and O(1) space.
    fn matches(&self, path: &Path) -> bool {
        self.info().supports(path)
    }

    /// 所属模块：core / 工程运行时 / 编解码接口
    /// 功能：把外部格式转译为规范工程会话；实现必须只读源文件。
    /// 输入：`path` 工程文件路径。
    ///
    /// 输出/副作用：返回会话；失败时返回中文上下文错误。
    ///
    /// 复杂度：由具体格式决定，通常为 O(文件大小)。
    ///
    /// Module: core / project runtime / codec interface
    /// Purpose: Translate an external format into a canonical project session; the
    /// implementation must be read-only.
    /// Inputs: `path` — project file path.
    ///
    /// Outputs / side effects: returns the session; failures carry Chinese context.
    ///
    /// Complexity: format-specific, typically O(file size).
    fn decode(&self, path: &Path) -> Result<ProjectSession, String>;

    /// 所属模块：core / 工程运行时 / 编解码接口
    /// 功能：把规范工程编码回该格式；只读后端默认返回“不支持写入”。
    /// 输入：
    /// - `path`：目标文件路径。
    /// - `project`：规范工程表示。
    /// - `peaks`：波形峰值。
    ///
    /// 输出/副作用：写入文件；不支持写入时返回错误。
    ///
    /// 复杂度：默认 O(1)；具体后端通常为 O(工程大小)。
    ///
    /// Module: core / project runtime / codec interface
    /// Purpose: Encode the canonical project back to this format; read-only backends
    /// return an "unsupported" error by default.
    /// Inputs:
    /// - `path`: destination path.
    /// - `project`: canonical project representation.
    /// - `peaks`: waveform peaks.
    ///
    /// Outputs / side effects: writes the file; returns an error when unsupported.
    ///
    /// Complexity: O(1) by default; concrete backends are typically O(project size).
    fn encode(
        &self,
        _path: &Path,
        _project: &Project,
        _peaks: &[WaveformPeak],
    ) -> Result<(), String> {
        Err(format!("{} 不支持写入", self.info().label))
    }
}
