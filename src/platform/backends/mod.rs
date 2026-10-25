//! 所属模块：platform / 后端注册
//! 文件用途：
//! - 按目标系统选择平台后端：桌面（Windows / macOS / Linux）或安卓。
//! - 新增平台时只需新增后端文件并在此处注册，上层代码零修改。
//!
//! 函数清单：
//! - `selected`：返回当前目标系统的唯一后端实例。
//!
//! Module: platform / backend registration
//! Purpose:
//! - Select the platform backend for the target OS: desktop (Windows / macOS / Linux) or Android.
//! - Adding a platform only requires a new backend file and a registration here.
//!
//! Functions:
//! - `selected`: return the single backend instance for the current target.

#[cfg(not(target_os = "android"))]
mod desktop;

#[cfg(target_os = "android")]
mod android;

#[cfg(target_os = "android")]
mod android_saf;

#[cfg(target_os = "android")]
pub use android::{install_activity, install_storage};

use super::PlatformBackend;

/// 所属模块：platform / 后端注册
/// 功能：返回当前目标系统的平台后端。
/// 输入：无。
/// 输出/副作用：静态后端引用；不初始化系统资源。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: platform / backend registration
/// Purpose: Return the platform backend of the current target.
/// Inputs: none.
/// Outputs / side effects: a static backend reference; no system resource is initialized.
/// Complexity: O(1) time and space.
pub(super) fn selected() -> &'static dyn PlatformBackend {
    #[cfg(not(target_os = "android"))]
    {
        desktop::backend()
    }
    #[cfg(target_os = "android")]
    {
        android::backend()
    }
}
