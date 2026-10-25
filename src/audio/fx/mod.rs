//! 所属模块：audio / 内置效果器
//! 文件用途：
//! - 声明内置原生效果器 DSP 模块。
//! - 对引擎暴露统一的 `NativeEffectProcessor`，避免把 DSP 细节写进实时渲染文件。
//!
//! Module: audio / built-in effects
//! Purpose:
//! - Declare the native effect DSP module.
//! - Expose `NativeEffectProcessor` so the engine never embeds DSP details in the renderer.

mod native;

pub use native::NativeEffectProcessor;
