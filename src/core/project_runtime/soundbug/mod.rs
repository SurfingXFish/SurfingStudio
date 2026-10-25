//! 所属模块：core / 工程运行时 / SoundBug 后端 / 模块聚合
//! 文件用途：
//! - 声明清单、EDIT 树、转换、导入入口与后端转译适配子模块。
//! - 只做声明与 re-export，不承载实现逻辑。
//!
//! 导出项：
//! - `SoundBugCodec`：实现 `ProjectCodec` 的 `.sndp` 只读后端。
//!
//! Module: core / project runtime / SoundBug backend / module aggregation
//! Purpose:
//! - Declare the manifest, EDIT tree, conversion, entry and backend-adapter submodules.
//! - Only declare and re-export; no implementation logic.
//!
//! Exports:
//! - `SoundBugCodec`: the read-only `.sndp` backend implementing `ProjectCodec`.

mod codec;
mod convert;
mod edit;
mod import;
mod manifest;

pub use codec::SoundBugCodec;
