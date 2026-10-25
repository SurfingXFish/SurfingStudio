//! 所属模块：core / 工程运行时 / SoundBug 后端 / 转译适配
//! 文件用途：
//! - 把 SoundBug 导入实现适配为框架的 `ProjectCodec` 后端。
//! - 将导入警告与来源标记封装进 `ProjectSession`。
//! - SoundBug 为只读后端，不实现编码。
//!
//! 函数清单：
//! - `SoundBugCodec::info` / `decode`：后端转译接口实现。
//!
//! Module: core / project runtime / SoundBug backend / translation adapter
//! Purpose:
//! - Adapt the SoundBug import implementation to the framework's `ProjectCodec` backend.
//! - Wrap import warnings and the source marker into `ProjectSession`.
//! - SoundBug is read-only and does not implement encoding.
//!
//! Functions:
//! - `SoundBugCodec::info` / `decode`: backend translation interface.

use super::import::import_project;
use crate::core::project_runtime::{CodecInfo, ProjectCodec, ProjectSession, ProjectSource};
use std::path::Path;

/// SoundBug `.sndp` 格式后端。
/// SoundBug `.sndp` format backend.
pub struct SoundBugCodec;

impl ProjectCodec for SoundBugCodec {
    /// 所属模块：core / 工程运行时 / SoundBug 后端
    /// 功能：返回 SoundBug 后端能力：只读转译。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回能力描述；不修改状态。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / SoundBug backend
    /// Purpose: Return the SoundBug capabilities: read-only translation.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the descriptor; no state change.
    ///
    /// Complexity: O(1) time and space.
    fn info(&self) -> CodecInfo {
        CodecInfo {
            id: "soundbug",
            label: "SoundBug 工程",
            extensions: &["sndp"],
            can_read: true,
            can_write: false,
        }
    }

    /// 所属模块：core / 工程运行时 / SoundBug 后端
    /// 功能：导入 `.sndp` 并转译为规范工程会话。
    /// 输入：`path` `.sndp` 文件路径。
    ///
    /// 输出/副作用：返回工程会话；不修改原始文件。
    ///
    /// 复杂度：时间 O(清单大小 + comp.dat 大小 + 音符数)，空间 O(解压后大小)。
    ///
    /// Module: core / project runtime / SoundBug backend
    /// Purpose: Import `.sndp` and translate it into a canonical project session.
    /// Inputs: `path` — `.sndp` file path.
    ///
    /// Outputs / side effects: returns the session; original files are untouched.
    ///
    /// Complexity: O(manifest size + comp.dat size + note count) time and
    /// O(decompressed size) space.
    fn decode(&self, path: &Path) -> Result<ProjectSession, String> {
        let imported = import_project(path)?;
        Ok(ProjectSession::new(
            path.to_path_buf(),
            imported.project,
            Vec::new(),
            imported.warnings,
            ProjectSource::Imported { format: "SoundBug" },
        ))
    }
}
