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
