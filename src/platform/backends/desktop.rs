//! 所属模块：platform / 桌面后端
//! 文件用途：
//! - 桌面（Windows / macOS / Linux）平台后端：使用系统用户目录（Windows 的 Roaming / Local
//!   AppData，macOS / Linux 的 XDG 目录）与 rfd 文件对话框，安装目录保持只读。
//! - Windows 上额外提供 `TerminateProcess` 退出，跳过插件卸载死锁。
//!
//! 函数清单：
//! - `backend`：返回桌面后端静态实例。
//! - `DesktopPlatform::info` / `caps`：标识与能力。
//! - `DesktopPlatform::install_dir` / `config_dir` / `cache_dir` / `data_dir` / `log_dir` /
//!   `temp_dir` / `documents_dir`：目录约定（用户配置、缓存、日志、临时文件与文档）。
//! - `DesktopPlatform::pick_file` / `pick_folder` / `save_file`：rfd 对话框。
//! - `DesktopPlatform::system_locale`：系统区域标识（Windows 用户默认区域 / 其他系统环境变量）。
//! - `DesktopPlatform::exit_process`：终止进程。
//! - `config_base` / `cache_base` / `app_temp_dir`：用户目录根（按平台选择环境变量）。
//! - `env_path`：读取非空环境变量并转换为路径。
//! - `windows_documents_dir`：Windows Known Folder `FOLDERID_Documents`（含重定向）。
//! - `apply_filters`：把请求过滤器追加到 rfd 对话框。
//!
//! Module: platform / desktop backend
//! Purpose:
//! - Desktop (Windows / macOS / Linux) backend: system user directories (Roaming / Local AppData
//!   on Windows, XDG directories on macOS / Linux) and rfd file dialogs; the install directory stays
//!   read-only.
//! - On Windows it terminates through `TerminateProcess`, skipping plugin-unload deadlocks.
//!
//! Functions:
//! - `backend`: return the desktop backend singleton.
//! - `DesktopPlatform::info` / `caps`: identity and capabilities.
//! - `DesktopPlatform::install_dir` / `config_dir` / `cache_dir` / `data_dir` / `log_dir` /
//!   `temp_dir` / `documents_dir`: directory conventions (user config, cache, logs, temp and
//!   documents).
//! - `DesktopPlatform::pick_file` / `pick_folder` / `save_file`: rfd dialogs.
//! - `DesktopPlatform::system_locale`: system locale (Windows user default / environment elsewhere).
//! - `DesktopPlatform::exit_process`: terminate the process.
//! - `config_base` / `cache_base` / `app_temp_dir`: user-directory roots (environment-dependent).
//! - `env_path`: read a non-empty environment variable as a path.
//! - `windows_documents_dir`: the Windows Known Folder `FOLDERID_Documents` (redirect aware).
//! - `apply_filters`: append request filters to an rfd dialog.

use super::super::{APP_DIR_NAME, CACHE_DIR_SUFFIX, LOG_DIR_SUFFIX, TEMP_DIR_SUFFIX};
use super::super::{FileRequest, PlatformBackend, PlatformCaps, PlatformInfo};
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
    /// 功能：返回安装目录：可执行文件（瘦启动器）所在目录。
    /// 输入：`self`。
    /// 输出/副作用：目录路径或 `None`；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / desktop backend / storage
    /// Purpose: Return the install directory: the directory of the executable (thin launcher).
    /// Inputs: `self`.
    /// Outputs / side effects: the path or `None`; nothing is created.
    /// Complexity: O(path length) time and space.
    fn install_dir(&self) -> Option<PathBuf> {
        std::env::current_exe().ok()?.parent().map(PathBuf::from)
    }

    /// 所属模块：platform / 桌面后端 / 存储
    /// 功能：返回用户配置目录：Windows 为 `%APPDATA%\SurfingStudio`，macOS / Linux 为
    /// `$XDG_CONFIG_HOME/SurfingStudio`（回退 `~/.config/SurfingStudio`）；无法解析时退回系统临时目录。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / desktop backend / storage
    /// Purpose: Return the user config directory: `%APPDATA%\SurfingStudio` on Windows and
    /// `$XDG_CONFIG_HOME/SurfingStudio` (falling back to `~/.config/SurfingStudio`) on macOS / Linux;
    /// fall back to the system temp directory when it cannot be resolved.
    /// Inputs: `self`.
    /// Outputs / side effects: the path; nothing is created.
    /// Complexity: O(path length) time and space.
    fn config_dir(&self) -> Option<PathBuf> {
        Some(config_base())
    }

    /// 所属模块：platform / 桌面后端 / 存储
    /// 功能：返回缓存目录：Windows 为 `%LOCALAPPDATA%\SurfingStudio\cache`，macOS / Linux 为
    /// `$XDG_CACHE_HOME/SurfingStudio/cache`。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / desktop backend / storage
    /// Purpose: Return the cache directory: `%LOCALAPPDATA%\SurfingStudio\cache` on Windows and
    /// `$XDG_CACHE_HOME/SurfingStudio/cache` on macOS / Linux.
    /// Inputs: `self`.
    /// Outputs / side effects: the path or `None`; nothing is created.
    /// Complexity: O(path length) time and space.
    fn cache_dir(&self) -> Option<PathBuf> {
        Some(cache_base().join(CACHE_DIR_SUFFIX))
    }

    /// 所属模块：platform / 桌面后端 / 存储
    /// 功能：桌面平台的数据目录与配置目录相同（用户配置根）。
    /// 输入：`self`。
    /// 输出/副作用：目录路径或 `None`；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / desktop backend / storage
    /// Purpose: On desktop the data directory equals the config directory (the user config root).
    /// Inputs: `self`.
    /// Outputs / side effects: the path or `None`; nothing is created.
    /// Complexity: O(path length) time and space.
    fn data_dir(&self) -> Option<PathBuf> {
        self.config_dir()
    }

    /// 所属模块：platform / 桌面后端 / 存储
    /// 功能：返回日志目录：Windows 为 `%LOCALAPPDATA%\SurfingStudio\logs`，macOS / Linux 为
    /// `$XDG_CACHE_HOME/SurfingStudio/logs`。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / desktop backend / storage
    /// Purpose: Return the log directory: `%LOCALAPPDATA%\SurfingStudio\logs` on Windows and
    /// `$XDG_CACHE_HOME/SurfingStudio/logs` on macOS / Linux.
    /// Inputs: `self`.
    /// Outputs / side effects: the path; nothing is created.
    /// Complexity: O(path length) time and space.
    fn log_dir(&self) -> PathBuf {
        cache_base().join(LOG_DIR_SUFFIX)
    }

    /// 所属模块：platform / 桌面后端 / 存储
    /// 功能：返回应用临时目录：Windows 为 `%LOCALAPPDATA%\SurfingStudio\temp`，macOS / Linux 为
    /// 系统临时目录下的 `SurfingStudio`。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / desktop backend / storage
    /// Purpose: Return the app temp directory: `%LOCALAPPDATA%\SurfingStudio\temp` on Windows and
    /// `SurfingStudio` under the system temp directory on macOS / Linux.
    /// Inputs: `self`.
    /// Outputs / side effects: the path; nothing is created.
    /// Complexity: O(path length) time and space.
    fn temp_dir(&self) -> PathBuf {
        app_temp_dir()
    }

    /// 所属模块：platform / 桌面后端 / 存储
    /// 功能：返回系统“文档”目录：Windows 通过 Known Folder `FOLDERID_Documents`（兼容 OneDrive
    /// 重定向），失败回退 `%USERPROFILE%\Documents`，再失败回退配置目录；macOS / Linux 为
    /// `~/Documents`。
    /// 输入：`self`。
    /// 输出/副作用：目录路径或 `None`；不创建目录。
    /// 复杂度：时间 O(系统调用 + 路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / desktop backend / storage
    /// Purpose: Return the system "Documents" directory: the Known Folder `FOLDERID_Documents` on
    /// Windows (OneDrive redirection aware), falling back to `%USERPROFILE%\Documents` and then the
    /// config directory; `~/Documents` on macOS / Linux.
    /// Inputs: `self`.
    /// Outputs / side effects: the path or `None`; nothing is created.
    /// Complexity: O(system call + path length) time and O(path length) space.
    fn documents_dir(&self) -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            windows_documents_dir()
                .or_else(|| env_path("USERPROFILE").map(|home| home.join("Documents")))
                .or_else(|| self.config_dir())
        }
        #[cfg(not(target_os = "windows"))]
        {
            env_path("HOME")
                .map(|home| home.join("Documents"))
                .or_else(|| self.config_dir())
        }
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

    /// 所属模块：platform / 桌面后端 / 区域
    /// 功能：返回系统区域标识；Windows 读取用户默认区域，其余系统读取环境变量。
    /// 输入：`self`。
    /// 输出/副作用：区域标识或 `None`；不修改状态。
    /// 复杂度：时间 O(系统调用)，空间 O(区域名长度)。
    ///
    /// Module: platform / desktop backend / locale
    /// Purpose: Return the system locale; Windows reads the user default locale, other systems read
    /// environment variables.
    /// Inputs: `self`.
    /// Outputs / side effects: the locale id or `None`; no state change.
    /// Complexity: O(system call) time and O(locale length) space.
    fn system_locale(&self) -> Option<String> {
        #[cfg(target_os = "windows")]
        {
            // LOCALE_NAME_MAX_LENGTH；缓冲区必须覆盖最长区域名。
            // LOCALE_NAME_MAX_LENGTH; the buffer must hold the longest locale name.
            let mut buffer = [0u16; 85];
            // SAFETY: 缓冲区长度与 API 约定一致；返回值为写入的字符数（含结尾 0）。
            // SAFETY: the buffer matches the API contract; the return value is the written length
            // including the terminator.
            let length = unsafe {
                winapi::um::winnls::GetUserDefaultLocaleName(
                    buffer.as_mut_ptr(),
                    buffer.len() as i32,
                )
            };
            if length > 1 {
                let end = (length as usize - 1).min(buffer.len());
                return Some(String::from_utf16_lossy(&buffer[..end]));
            }
            super::super::backend::locale_from_env()
        }
        #[cfg(not(target_os = "windows"))]
        {
            super::super::backend::locale_from_env()
        }
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

/// 所属模块：platform / 桌面后端 / 前置辅助 / 存储
/// 功能：返回用户配置根目录：Windows 为 `%APPDATA%\SurfingStudio`，macOS / Linux 为
/// `$XDG_CONFIG_HOME/SurfingStudio`（回退 `~/.config/SurfingStudio`）；无法解析时退回系统临时目录。
/// 输入：无。
/// 输出/副作用：目录路径；不创建目录。
/// 复杂度：时间 O(环境变量数 × 路径长度)，空间 O(路径长度)。
///
/// Module: platform / desktop backend / helpers / storage
/// Purpose: Return the user config root: `%APPDATA%\SurfingStudio` on Windows and
/// `$XDG_CONFIG_HOME/SurfingStudio` (falling back to `~/.config/SurfingStudio`) on macOS / Linux;
/// fall back to the system temp directory when it cannot be resolved.
/// Inputs: none.
/// Outputs / side effects: the path; nothing is created.
/// Complexity: O(variables × path length) time and O(path length) space.
fn config_base() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        env_path("APPDATA")
            .map(|base| base.join(APP_DIR_NAME))
            .unwrap_or_else(|| std::env::temp_dir().join(APP_DIR_NAME))
    }
    #[cfg(not(target_os = "windows"))]
    {
        env_path("XDG_CONFIG_HOME")
            .or_else(|| env_path("HOME").map(|home| home.join(".config")))
            .map(|base| base.join(APP_DIR_NAME))
            .unwrap_or_else(|| std::env::temp_dir().join(APP_DIR_NAME))
    }
}

/// 所属模块：platform / 桌面后端 / 前置辅助 / 存储
/// 功能：返回用户缓存根目录：Windows 为 `%LOCALAPPDATA%\SurfingStudio`（缺失时回退配置根），
/// macOS / Linux 为 `$XDG_CACHE_HOME/SurfingStudio`（回退 `~/.cache/SurfingStudio`）。
/// 输入：无。
/// 输出/副作用：目录路径；不创建目录。
/// 复杂度：时间 O(环境变量数 × 路径长度)，空间 O(路径长度)。
///
/// Module: platform / desktop backend / helpers / storage
/// Purpose: Return the user cache root: `%LOCALAPPDATA%\SurfingStudio` on Windows (falling back to
/// the config root) and `$XDG_CACHE_HOME/SurfingStudio` (falling back to `~/.cache/SurfingStudio`)
/// on macOS / Linux.
/// Inputs: none.
/// Outputs / side effects: the path; nothing is created.
/// Complexity: O(variables × path length) time and O(path length) space.
fn cache_base() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        env_path("LOCALAPPDATA")
            .map(|base| base.join(APP_DIR_NAME))
            .unwrap_or_else(config_base)
    }
    #[cfg(not(target_os = "windows"))]
    {
        env_path("XDG_CACHE_HOME")
            .or_else(|| env_path("HOME").map(|home| home.join(".cache")))
            .map(|base| base.join(APP_DIR_NAME))
            .unwrap_or_else(config_base)
    }
}

/// 所属模块：platform / 桌面后端 / 前置辅助 / 存储
/// 功能：返回应用临时目录：Windows 为缓存根下的 `temp`，macOS / Linux 为系统临时目录下的
/// `SurfingStudio`。
/// 输入：无。
/// 输出/副作用：目录路径；不创建目录。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: platform / desktop backend / helpers / storage
/// Purpose: Return the app temp directory: `temp` under the cache root on Windows and
/// `SurfingStudio` under the system temp directory on macOS / Linux.
/// Inputs: none.
/// Outputs / side effects: the path; nothing is created.
/// Complexity: O(path length) time and space.
fn app_temp_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        cache_base().join(TEMP_DIR_SUFFIX)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::temp_dir().join(APP_DIR_NAME)
    }
}

/// 所属模块：platform / 桌面后端 / 前置辅助 / 环境
/// 功能：读取非空环境变量并转换为路径；空值视为未设置。
/// 输入：`key`：环境变量名。
/// 输出/副作用：路径或 `None`；不修改状态。
/// 复杂度：时间 O(变量长度)，空间 O(路径长度)。
///
/// Module: platform / desktop backend / helpers / environment
/// Purpose: Read a non-empty environment variable as a path; an empty value counts as unset.
/// Inputs: `key`: environment variable name.
/// Outputs / side effects: the path or `None`; no state change.
/// Complexity: O(value length) time and O(path length) space.
fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// 所属模块：platform / 桌面后端 / 前置辅助 / 存储
/// 功能：通过 Windows Known Folder `FOLDERID_Documents` 获取“文档”目录，兼容 OneDrive 等重定向；
/// 调用失败或返回空路径时返回 `None`。
/// 输入：无。
/// 输出/副作用：目录路径或 `None`；释放 API 分配的内存，不创建目录。
/// 复杂度：时间 O(系统调用 + 路径长度)，空间 O(路径长度)。
///
/// Module: platform / desktop backend / helpers / storage
/// Purpose: Resolve the "Documents" directory through the Windows Known Folder
/// `FOLDERID_Documents`, honouring OneDrive-style redirection; returns `None` on failure or an empty
/// path.
/// Inputs: none.
/// Outputs / side effects: the path or `None`; frees the API-allocated buffer, creates nothing.
/// Complexity: O(system call + path length) time and O(path length) space.
#[cfg(target_os = "windows")]
fn windows_documents_dir() -> Option<PathBuf> {
    use winapi::um::combaseapi::CoTaskMemFree;
    use winapi::um::knownfolders::FOLDERID_Documents;
    use winapi::um::shlobj::SHGetKnownFolderPath;

    let mut raw: *mut u16 = std::ptr::null_mut();
    // SAFETY: 输出参数是有效的可写指针，未传入用户令牌（使用当前用户），标志位为 0。
    // SAFETY: the out-parameter is a valid writable pointer, no user token is passed (current user)
    // and the flags are zero.
    let result =
        unsafe { SHGetKnownFolderPath(&FOLDERID_Documents, 0, std::ptr::null_mut(), &mut raw) };
    if result < 0 || raw.is_null() {
        return None;
    }
    // SAFETY: 成功时 API 返回以 0 结尾的 UTF-16 字符串，且必须由 CoTaskMemFree 释放；
    // 这里先测量长度、复制成 String，再立即释放，避免悬垂指针。
    // SAFETY: on success the API returns a NUL-terminated UTF-16 string that must be released with
    // CoTaskMemFree; the length is measured and copied into a String before freeing immediately.
    let path = unsafe {
        let length = (0..).take_while(|index| *raw.add(*index) != 0).count();
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(raw, length));
        CoTaskMemFree(raw.cast());
        PathBuf::from(text)
    };
    (!path.as_os_str().is_empty()).then_some(path)
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
