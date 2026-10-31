//! SurfingStudio —— 以 MIDI 为核心的 Rust 音乐工作站。
//!
//! 库划分为以下层次：
//!
//! - [`core`]：纯数据模型、工程运行时框架与辅助工具，不依赖音频/GUI。
//! - [`platform`]：平台运行时框架（类 JVM）：统一的内部平台 API 与各系统后端。
//! - [`audio`]：实时音频引擎、内置乐器、音频 I/O 运行时框架与 cpal 输出。
//! - [`plugin`]：插件运行时框架、VST3 / VST2 后端、扫描与参数管理。
//! - [`diag`]：进程级诊断日志（`.sflog`）、耗时区间、卡死/崩溃监控与确定性退出。
//! - [`app`] / [`ui`]：eframe 应用状态与各个编辑视图。
//!
//! 二进制入口只负责创建窗口，所有可测试逻辑都放在本库中。
//!
//! `vst` 0.4 已停止维护，但仍是 VST2 宿主能力的主要实现；egui 0.34 也保留
//! 了若干已重命名的面板 API。这里统一允许这些已知的弃用项，避免淹没真正的
//! 警告。
//!
//! SurfingStudio — a MIDI-centric Rust music workstation.
//!
//! Layers:
//!
//! - [`core`]: pure data model, project runtime framework and helpers; no audio/GUI.
//! - [`audio`]: real-time engine, built-in instruments, audio I/O runtime framework and cpal output.
//! - [`plugin`]: plugin runtime framework, VST3 / VST2 backends, scanning and parameters.
//! - [`diag`]: process-wide diagnostic log (`.sflog`), duration spans, stall/crash monitoring and
//!   deterministic exit.
//! - [`app`] / [`ui`]: eframe application state and editor views.
//!
//! The binary entry only creates the window; all testable logic lives in this library.
//!
//! `vst` 0.4 is unmaintained but still the main VST2 host implementation; egui 0.34
//! also keeps some renamed panel APIs. Known deprecations are allowed here so real
//! warnings stay visible.
#![allow(deprecated)]

pub mod app;
pub mod audio;
pub mod core;
pub mod diag;
pub mod platform;
pub mod plugin;
pub mod ui;

pub use app::{SurfingApp, WindowHost};

/// 所属模块：库根 / 版本
/// 功能：返回面向用户、日志与安卓清单的版本号：把 Cargo 的 SemVer 预发布后缀
/// `-a` / `-t` 还原为发布号 `a` / `t`（例如 `1.1.0-a` → `1.1.0a`）。
/// 输入：无。
/// 输出/副作用：静态版本字符串；只初始化一次，不修改其他状态。
/// 复杂度：时间 O(1)（首次 O(版本长度)），空间 O(版本长度)。
///
/// Module: library root / version
/// Purpose: Return the user/log/Android-manifest version: restore Cargo's SemVer prerelease
/// suffixes `-a` / `-t` to the release labels `a` / `t` (for example `1.1.0-a` → `1.1.0a`).
/// Inputs: none.
/// Outputs / side effects: static version text; initialised once, no other state change.
/// Complexity: O(1) time (O(version length) on first use) and O(version length) space.
pub fn version_label() -> &'static str {
    // Cargo 只接受 SemVer 预发布写法（`1.1.0-a`），而发布规范使用 `1.1.0a`；
    // 统一在这里转换，避免界面、日志与安卓清单出现两种写法。
    // Cargo only accepts SemVer prereleases (`1.1.0-a`) while the release rules use `1.1.0a`;
    // convert here so the UI, logs and Android manifest never disagree.
    static LABEL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    LABEL.get_or_init(|| format_version(env!("CARGO_PKG_VERSION")))
}

/// 所属模块：库根 / 版本
/// 功能：把 Cargo 的 SemVer 预发布后缀 `-a` / `-t` 转为发布号 `a` / `t`，其它形式原样返回。
/// 输入：`version`：`CARGO_PKG_VERSION` 形式的版本字符串。
/// 输出/副作用：返回显示用版本号；不修改输入。
/// 复杂度：时间 O(版本长度)，空间 O(版本长度)。
///
/// Module: library root / version
/// Purpose: Convert Cargo's SemVer prerelease suffixes `-a` / `-t` into release labels `a` / `t`;
/// other forms pass through.
/// Inputs: `version`: version text in `CARGO_PKG_VERSION` form.
/// Outputs / side effects: display version text; the input is unchanged.
/// Complexity: O(version length) time and space.
fn format_version(version: &str) -> String {
    if let Some(core) = version.strip_suffix("-a") {
        format!("{core}a")
    } else if let Some(core) = version.strip_suffix("-t") {
        format!("{core}t")
    } else {
        version.to_string()
    }
}

#[cfg(test)]
mod version_tests {
    use super::format_version;

    /// 覆盖：构建版/测试版后缀去掉连字符，正式版保持原样。
    /// Covers: build/test suffixes lose the hyphen while releases pass through.
    #[test]
    fn release_suffixes_are_rendered_without_hyphen() {
        assert_eq!(format_version("1.1.0-a"), "1.1.0a");
        assert_eq!(format_version("1.1.0-t"), "1.1.0t");
        assert_eq!(format_version("1.1.0"), "1.1.0");
    }
}
