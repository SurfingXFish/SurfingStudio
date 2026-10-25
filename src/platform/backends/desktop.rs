//! 所属模块：platform / 桌面后端
//! 文件用途：
//! - 桌面（Windows / macOS / Linux）平台后端：沿用原有目录约定与 rfd 文件对话框。
//! - Windows 上额外提供 `TerminateProcess` 退出，跳过插件卸载死锁。
//!
//! 函数清单：
//! - `backend`：返回桌面后端静态实例。
//! - `DesktopPlatform::info` / `caps`：标识与能力。
//! - `DesktopPlatform::config_dir` / `cache_dir` / `data_dir` / `log_dir` / `temp_dir`：目录约定。
//! - `DesktopPlatform::pick_file` / `pick_folder` / `save_file`：rfd 对话框。
//! - `DesktopPlatform::exit_process`：终止进程。
//!
//! Module: platform / desktop backend
//! Purpose:
//! - Desktop (Windows / macOS / Linux) backend: keeps the existing directory conventions and the
//!   rfd file dialogs.
//! - On Windows it terminates through `TerminateProcess`, skipping plugin-unload deadlocks.
//!
//! Functions:
//! - `backend`: return the desktop backend singleton.
//! - `DesktopPlatform::info` / `caps`: identity and capabilities.
//! - `DesktopPlatform::config_dir` / `cache_dir` / `data_dir` / `log_dir` / `temp_dir`: conventions.
//! - `DesktopPlatform::pick_file` / `pick_folder` / `save_file`: rfd dialogs.
//! - `DesktopPlatform::exit_process`: terminate the process.

use super::super::{FileRequest, PlatformBackend, PlatformCaps, PlatformInfo};
use super::super::{APP_DIR_NAME, LOG_DIR_SUFFIX};
use std::path::PathBuf;

/// 桌面后端实例。
/// Desktop backend instance.
struct DesktopPlatform;

/// 静态后端：运行时不持有任何状态，避免全局可变单例。
/// Static backend: the runtime holds no state, so no mutable global singleton is needed.
static BACKEND: DesktopPlatform = DesktopPlatform;

/// 所属模块：platform / 桌面后端
/// 功能：返回桌面后端。
/// 输入：无。
/// 输出/副作用：静态引用；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: platform / desktop backend
/// Purpose: Return the desktop backend.
/// Inputs: none.
/// Outputs / side effects: a static reference; no state change.
/// Complexity: O(1) time and space.
pub(super) fn backend() -> &'static dyn PlatformBackend {
    &BACKEND
}

impl PlatformBackend for DesktopPlatform {
    /// 所属模块：platform / 桌面后端
    /// 功能：返回桌面后端标识。
    /// 输入：`self`。
    /// 输出/副作用：标识；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / desktop backend
    /// Purpose: Return the desktop backend identity.
    /// Inputs: `self`.
    /// Outputs / side effects: the descriptor; no state change.
    /// Complexity: O(1) time and space.
    fn info(&self) -> PlatformInfo {
        #[cfg(target_os = "windows")]
        {
            PlatformInfo {
                id: "windows",
                label: "Windows",
            }
        }
        #[cfg(target_os = "macos")]
        {
            PlatformInfo {
                id: "macos",
                label: "macOS",
            }
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            PlatformInfo {
                id: "linux",
                label: "Linux",
            }
        }
    }

    /// 所属模块：platform / 桌面后端
    /// 功能：返回桌面能力；插件原生编辑器与结构化异常过滤仅 Windows 可用。
    /// 输入：`self`。
    /// 输出/副作用：能力；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / desktop backend
    /// Purpose: Return desktop capabilities; native plugin editors and the SEH crash filter exist
    /// on Windows only.
    /// Inputs: `self`.
    /// Outputs / side effects: the descriptor; no state change.
    /// Complexity: O(1) time and space.
    fn caps(&self) -> PlatformCaps {
        #[cfg(target_os = "windows")]
        {
            PlatformCaps::DESKTOP
        }
        #[cfg(not(target_os = "windows"))]
        {
            PlatformCaps {
                native_plugin_editor: false,
                crash_trap: false,
                ..PlatformCaps::DESKTOP
            }
        }
    }

    /// 所属模块：platform / 桌面后端 / 存储
    /// 功能：返回配置目录：Windows 用 `%APPDATA%`，其他平台用 XDG/HOME。
    /// 输入：`self`。
    /// 输出/副作用：目录路径或 `None`；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / desktop backend / storage
    /// Purpose: Return the config directory: `%APPDATA%` on Windows, XDG/HOME elsewhere.
    /// Inputs: `self`.
    /// Outputs / side effects: the path or `None`; nothing is created.
    /// Complexity: O(path length) time and space.
    fn config_dir(&self) -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        let base = std::env::var_os("APPDATA").map(PathBuf::from);
        #[cfg(not(target_os = "windows"))]
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
        base.map(|base| base.join(APP_DIR_NAME))
    }

    /// 所属模块：platform / 桌面后端 / 存储
    /// 功能：返回缓存目录：Windows 用 `%LOCALAPPDATA%`，其他平台用 XDG/HOME 缓存目录。
    /// 输入：`self`。
    /// 输出/副作用：目录路径或 `None`；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / desktop backend / storage
    /// Purpose: Return the cache directory: `%LOCALAPPDATA%` on Windows, the XDG/HOME cache
    /// directory elsewhere.
    /// Inputs: `self`.
    /// Outputs / side effects: the path or `None`; nothing is created.
    /// Complexity: O(path length) time and space.
    fn cache_dir(&self) -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from));
        #[cfg(not(target_os = "windows"))]
        let base = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")));
        base.map(|base| base.join(APP_DIR_NAME))
    }

    /// 所属模块：platform / 桌面后端 / 存储
    /// 功能：桌面平台的数据目录与配置目录相同。
    /// 输入：`self`。
    /// 输出/副作用：目录路径或 `None`；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / desktop backend / storage
    /// Purpose: On desktop the data directory equals the config directory.
    /// Inputs: `self`.
    /// Outputs / side effects: the path or `None`; nothing is created.
    /// Complexity: O(path length) time and space.
    fn data_dir(&self) -> Option<PathBuf> {
        self.config_dir()
    }

    /// 所属模块：platform / 桌面后端 / 存储
    /// 功能：返回日志目录：Windows 用本地应用数据目录，其他平台用系统临时目录。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / desktop backend / storage
    /// Purpose: Return the log directory: local app data on Windows, the temp directory elsewhere.
    /// Inputs: `self`.
    /// Outputs / side effects: the path; nothing is created.
    /// Complexity: O(path length) time and space.
    fn log_dir(&self) -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            if let Some(base) = std::env::var_os("LOCALAPPDATA") {
                if !base.to_string_lossy().trim().is_empty() {
                    return PathBuf::from(base).join(APP_DIR_NAME).join(LOG_DIR_SUFFIX);
                }
            }
        }
        std::env::temp_dir().join(APP_DIR_NAME).join(LOG_DIR_SUFFIX)
    }

    /// 所属模块：platform / 桌面后端 / 存储
    /// 功能：返回系统临时目录。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / desktop backend / storage
    /// Purpose: Return the system temp directory.
    /// Inputs: `self`.
    /// Outputs / side effects: the path; nothing is created.
    /// Complexity: O(path length) time and space.
    fn temp_dir(&self) -> PathBuf {
        std::env::temp_dir()
    }

    /// 所属模块：platform / 桌面后端 / 对话框
    /// 功能：用 rfd 打开“选择文件”对话框。
    /// 输入：`request` 对话框请求。
    /// 输出/副作用：阻塞直到用户选择；返回路径。
    /// 复杂度：时间取决于用户操作，空间 O(路径长度)。
    ///
    /// Module: platform / desktop backend / dialogs
    /// Purpose: Show an open-file dialog through rfd.
    /// Inputs: `request`: dialog request.
    /// Outputs / side effects: blocks until the user picks; returns the path.
    /// Complexity: user-bound time, O(path length) space.
    fn pick_file(&self, request: &FileRequest<'_>) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_title(request.title);
        dialog = apply_filters(dialog, request);
        if let Some(directory) = request.start_dir {
            dialog = dialog.set_directory(directory);
        }
        if let Some(parent) = request.parent.as_ref() {
            dialog = dialog.set_parent(parent);
        }
        dialog.pick_file()
    }

    /// 所属模块：platform / 桌面后端 / 对话框
    /// 功能：用 rfd 打开“选择文件夹”对话框。
    /// 输入：`request` 对话框请求。
    /// 输出/副作用：阻塞直到用户选择；返回目录。
    /// 复杂度：时间取决于用户操作，空间 O(路径长度)。
    ///
    /// Module: platform / desktop backend / dialogs
    /// Purpose: Show a pick-folder dialog through rfd.
    /// Inputs: `request`: dialog request.
    /// Outputs / side effects: blocks until the user picks; returns the directory.
    /// Complexity: user-bound time, O(path length) space.
    fn pick_folder(&self, request: &FileRequest<'_>) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_title(request.title);
        if let Some(directory) = request.start_dir {
            dialog = dialog.set_directory(directory);
        }
        if let Some(parent) = request.parent.as_ref() {
            dialog = dialog.set_parent(parent);
        }
        dialog.pick_folder()
    }

    /// 所属模块：platform / 桌面后端 / 对话框
    /// 功能：用 rfd 打开“另存为”对话框。
    /// 输入：`request` 对话框请求。
    /// 输出/副作用：阻塞直到用户确认；返回目标路径。
    /// 复杂度：时间取决于用户操作，空间 O(路径长度)。
    ///
    /// Module: platform / desktop backend / dialogs
    /// Purpose: Show a save-as dialog through rfd.
    /// Inputs: `request`: dialog request.
    /// Outputs / side effects: blocks until the user confirms; returns the target path.
    /// Complexity: user-bound time, O(path length) space.
    fn save_file(&self, request: &FileRequest<'_>) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_title(request.title);
        dialog = apply_filters(dialog, request);
        if let Some(name) = request.default_name {
            dialog = dialog.set_file_name(name);
        }
        if let Some(directory) = request.start_dir {
            dialog = dialog.set_directory(directory);
        }
        if let Some(parent) = request.parent.as_ref() {
            dialog = dialog.set_parent(parent);
        }
        dialog.save_file()
    }

    /// 所属模块：platform / 桌面后端 / 退出
    /// 功能：立即结束进程：Windows 用 `TerminateProcess`（跳过插件卸载），其他平台用
    /// `std::process::exit`。
    /// 输入：`code` 进程退出码。
    /// 输出/副作用：结束进程；正常情况下不返回。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / desktop backend / exit
    /// Purpose: End the process immediately: `TerminateProcess` on Windows (skips plugin unload),
    /// `std::process::exit` elsewhere.
    /// Inputs: `code`: process exit code.
    /// Outputs / side effects: ends the process; normally never returns.
    /// Complexity: O(1) time and space.
    fn exit_process(&self, code: i32) -> ! {
        #[cfg(target_os = "windows")]
        {
            // SAFETY: 当前进程伪句柄始终有效；TerminateProcess 不返回，返回即为失败。
            // SAFETY: the current-process pseudo handle is always valid; TerminateProcess does not
            // return on success, so returning means it failed.
            unsafe {
                winapi::um::processthreadsapi::TerminateProcess(
                    winapi::um::processthreadsapi::GetCurrentProcess(),
                    code as u32,
                );
            }
        }
        std::process::exit(code)
    }
}

/// 所属模块：platform / 桌面后端 / 对话框
/// 功能：把请求中的过滤器追加到 rfd 对话框。
/// 输入：
/// - `dialog`：rfd 对话框。
/// - `request`：对话框请求。
/// 输出/副作用：返回追加过滤器后的对话框；不修改状态。
/// 复杂度：时间 O(过滤器数)，空间 O(过滤器数)。
///
/// Module: platform / desktop backend / dialogs
/// Purpose: Append the request filters to an rfd dialog.
/// Inputs:
/// - `dialog`: rfd dialog.
/// - `request`: dialog request.
/// Outputs / side effects: the dialog with filters; no state change.
/// Complexity: O(filters) time and space.
fn apply_filters<'a>(mut dialog: rfd::FileDialog, request: &'a FileRequest<'a>) -> rfd::FileDialog {
    for (label, extensions) in request.filters {
        dialog = dialog.add_filter(*label, extensions);
    }
    dialog
}
