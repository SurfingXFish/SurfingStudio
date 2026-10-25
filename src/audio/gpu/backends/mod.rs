//! 所属模块：audio::gpu / 后端注册
//! 文件用途：
//! - 声明 GPU 渲染后端实现；新增后端只需在此声明并在 `GpuRuntime::standard` 注册。
//!
//! Module: audio::gpu / backend registry
//! Purpose:
//! - Declare GPU rendering backends; a new backend only needs a module here and registration in
//!   `GpuRuntime::standard`.

pub mod wgpu_compute;
