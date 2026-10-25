//! 所属模块：core / 工程运行时 / 会话
//! 文件用途：
//! - 定义框架加载后的工程会话 `ProjectSession`：工程、峰值、警告、来源与原始路径。
//! - 定义格式无关的 `WaveformPeak` 与 `ProjectSource`。
//! - 会话是应用层与各格式后端之间的唯一数据契约。
//!
//! 函数清单：
//! - `ProjectSession::new` / `is_imported` / `into_project`：会话构造与查询。
//! - `ProjectSource::label` / `is_imported`：来源展示与导入判断。
//!
//! Module: core / project runtime / session
//! Purpose:
//! - Define the loaded `ProjectSession`: project, peaks, warnings, source and origin.
//! - Define the format-neutral `WaveformPeak` and `ProjectSource`.
//! - The session is the only data contract between the app and format backends.
//!
//! Functions:
//! - `ProjectSession::new` / `is_imported` / `into_project`: construction and queries.
//! - `ProjectSource::label` / `is_imported`: source label and import detection.

use crate::core::model::Project;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 波形峰值包络：路径 + 固定 bin 数的绝对值峰值。
/// Coarse waveform envelope: path plus absolute peaks in a fixed number of bins.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WaveformPeak {
    pub path: PathBuf,
    pub bins: Vec<f32>,
}

/// 工程来源：决定保存时是否允许覆盖原文件。
/// Project source: decides whether the original file may be overwritten on save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectSource {
    /// 框架原生格式（`.sfsd`），可直接覆盖保存。
    /// Native framework format (`.sfsd`) that can be saved back in place.
    Native,
    /// 外部格式转译而来，必须另存为 `.sfsd`。
    /// Translated from an external format that must be saved as `.sfsd`.
    Imported { format: &'static str },
}

impl ProjectSource {
    /// 所属模块：core / 工程运行时 / 来源查询
    /// 功能：返回工程来源的展示名，用于状态栏与日志。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回静态文本；不修改状态。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / source query
    /// Purpose: Return the display label of the project source for the status bar.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns static text; no state change.
    ///
    /// Complexity: O(1) time and space.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Native => "SurfingStudio",
            Self::Imported { format } => format,
        }
    }

    /// 所属模块：core / 工程运行时 / 来源查询
    /// 功能：判断工程是否为外部格式转译而来。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回布尔值；不修改状态。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / source query
    /// Purpose: Check whether the project was translated from an external format.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns a boolean; no state change.
    ///
    /// Complexity: O(1) time and space.
    pub fn is_imported(&self) -> bool {
        matches!(self, Self::Imported { .. })
    }
}

/// 框架加载后的工程会话。
/// A project session produced by the framework.
pub struct ProjectSession {
    /// 会话来源路径（外部格式为原始文件路径）。
    /// Origin path of the session (the original path for external formats).
    pub origin: PathBuf,
    pub project: Project,
    pub peaks: Vec<WaveformPeak>,
    pub warnings: Vec<String>,
    pub source: ProjectSource,
}

impl ProjectSession {
    /// 所属模块：core / 工程运行时 / 会话
    /// 功能：从后端转译结果构造会话。
    /// 输入：
    /// - `origin`：会话来源路径。
    /// - `project`：规范工程表示。
    /// - `peaks`：波形峰值。
    /// - `warnings`：转译警告。
    /// - `source`：工程来源。
    ///
    /// 输出/副作用：返回会话；不修改外部状态。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)（所有权转移）。
    ///
    /// Module: core / project runtime / session
    /// Purpose: Build a session from a backend translation result.
    /// Inputs:
    /// - `origin`: session origin path.
    /// - `project`: canonical project representation.
    /// - `peaks`: waveform peaks.
    /// - `warnings`: translation warnings.
    /// - `source`: project source.
    ///
    /// Outputs / side effects: returns the session; no external state change.
    ///
    /// Complexity: O(1) time and space (ownership is moved).
    pub fn new(
        origin: PathBuf,
        project: Project,
        peaks: Vec<WaveformPeak>,
        warnings: Vec<String>,
        source: ProjectSource,
    ) -> Self {
        Self {
            origin,
            project,
            peaks,
            warnings,
            source,
        }
    }

    /// 所属模块：core / 工程运行时 / 会话
    /// 功能：判断会话是否来自外部格式转译。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回布尔值；不修改状态。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / session
    /// Purpose: Check whether the session came from an external translation.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns a boolean; no state change.
    ///
    /// Complexity: O(1) time and space.
    pub fn is_imported(&self) -> bool {
        self.source.is_imported()
    }

    /// 所属模块：core / 工程运行时 / 会话
    /// 功能：取出会话中的工程，丢弃其它元数据。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回工程所有权。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / session
    /// Purpose: Take the project out of the session, dropping other metadata.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns project ownership.
    ///
    /// Complexity: O(1) time and space.
    pub fn into_project(self) -> Project {
        self.project
    }
}
