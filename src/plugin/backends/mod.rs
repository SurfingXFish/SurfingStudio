//! 所属模块：plugin / 后端 / 模块聚合
//! 文件用途：
//! - 声明并导出 VST3 / VST2 两个插件格式后端。
//! - 后端只负责各自格式的加载与扫描，由 `PluginHost` 统一分派。
//!
//! 导出项：
//! - `Vst3Backend` / `Vst2Backend`。
//!
//! Module: plugin / backends / module aggregation
//! Purpose:
//! - Declare and export the VST3 / VST2 plugin format backends.
//! - Backends only handle their own loading and scanning; `PluginHost` dispatches.
//!
//! Exports:
//! - `Vst3Backend` / `Vst2Backend`.

mod vst2;
mod vst3;

pub use vst2::Vst2Backend;
pub use vst3::Vst3Backend;

pub(crate) use vst2::{make_vst2_cc, send_vst2_events, send_vst2_gates, Vst2Host};
