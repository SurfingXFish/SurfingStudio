//! 所属模块：audio / 聚合
//! 文件用途：
//! - 声明音频引擎、内置乐器、系统输出（cpal / ASIO）、GPU 渲染与音频 I/O 运行时框架。
//!
//! Module: audio / aggregation
//! Purpose:
//! - Declare the audio engine, built-in instruments, system output (cpal / ASIO), GPU rendering and
//!   the audio I/O runtime framework.

pub mod backend;
pub mod engine;
pub mod fx;
pub mod gpu;
pub mod io;
pub mod synth;

#[cfg(target_os = "windows")]
pub mod asio;

pub use backend::{AudioOptions, AudioOutput, AudioRouter, AudioSystem};
pub use engine::{format_position, Engine, SlotKind};
pub use gpu::{GpuDeviceInfo, GpuRenderConfig, GpuRuntime};
pub use io::{AudioFile, AudioRuntime};
