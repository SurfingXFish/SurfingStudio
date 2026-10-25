//! 所属模块：platform / 平台运行时
//! 文件用途：
//! - 定义应用层唯一的平台能力入口（类 JVM：统一的内部 API + 各系统后端）。
//! - 收敛存储目录、原生文件对话框、窗口能力、插件能力、退出方式与音频系统能力。
//! - 叶子模块：只被 `app` / `ui` / `diag` / `main` 单向调用，自身不依赖任何业务层。
//!
//! 函数清单：
//! - `PlatformRuntime::standard`：返回进程内唯一的平台运行时。
//! - `PlatformRuntime::info` / `caps`：后端标识与能力描述。
//! - `PlatformRuntime::config_dir` / `cache_dir` / `data_dir` / `log_dir` / `temp_dir`：平台存储目录。
//! - `PlatformRuntime::pick_file` / `pick_folder` / `save_file`：原生文件对话框。
//! - `PlatformRuntime::exit_process`：确定性退出（各平台自行选择实现）。
//! - `caps` / `config_dir` / `cache_dir` / `log_dir` / `temp_dir` / `pick_file` / `pick_folder` /
//!   `save_file` / `exit_process`：对运行时的模块级快捷方式。
//!
//! Module: platform / platform runtime
//! Purpose:
//! - Define the single platform entry point used by the app layer (JVM-like: one internal API with
//!   per-OS backends).
//! - Collect storage directories, native file dialogs, window capabilities, plugin capabilities,
//!   exit behaviour and audio-system capabilities.
//! - Leaf module: only called by `app` / `ui` / `diag` / `main`; never depends on business layers.
//!
//! Functions:
//! - `PlatformRuntime::standard`: return the process-wide platform runtime.
//! - `PlatformRuntime::info` / `caps`: backend identity and capabilities.
//! - `PlatformRuntime::config_dir` / `cache_dir` / `data_dir` / `log_dir` / `temp_dir`: platform storage.
//! - `PlatformRuntime::pick_file` / `pick_folder` / `save_file`: native file dialogs.
//! - `PlatformRuntime::exit_process`: deterministic exit (each platform picks its own mechanism).
//! - `caps` / `config_dir` / `cache_dir` / `log_dir` / `temp_dir` / `pick_file` / `pick_folder` /
//!   `save_file` / `exit_process`: module-level shortcuts over the runtime.

mod backend;
mod backends;
mod dialog;

pub use backend::{FileRequest, PlatformBackend, PlatformCaps, PlatformInfo};
pub use dialog::{parent_by_title, DialogParent};

#[cfg(target_os = "android")]
pub use backends::{install_activity, install_storage};

use std::path::{Path, PathBuf};

/// 全局配置目录名（各平台一致）。
/// Global configuration directory name (identical on every platform).
pub const APP_DIR_NAME: &str = "SurfingStudio";
/// 日志子目录名（位于 `APP_DIR_NAME` 下）。
/// Log sub-directory name under `APP_DIR_NAME`.
pub const LOG_DIR_SUFFIX: &str = "logs";

/// 平台运行时：持有当前系统后端，暴露唯一的上层入口。
/// Platform runtime: owns the current OS backend and exposes the only upper-layer entry.
pub struct PlatformRuntime {
    backend: &'static dyn PlatformBackend,
}

impl PlatformRuntime {
    /// 所属模块：platform / 平台运行时
    /// 功能：返回进程内唯一的平台运行时（后端在编译期按目标系统选定）。
    /// 输入：无。
    /// 输出/副作用：静态引用；不初始化任何系统资源。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / platform runtime
    /// Purpose: Return the process-wide platform runtime (the backend is chosen at compile time).
    /// Inputs: none.
    /// Outputs / side effects: a static reference; no system resource is initialized.
    /// Complexity: O(1) time and space.
    pub fn standard() -> &'static Self {
        // 后端是零状态静态实例，用 `LazyLock` 延迟初始化即可，无需可变全局单例。
        // Backends are stateless singletons; `LazyLock` defers initialization without mutable state.
        static RUNTIME: std::sync::LazyLock<PlatformRuntime> =
            std::sync::LazyLock::new(|| PlatformRuntime {
                backend: backends::selected(),
            });
        &RUNTIME
    }

    /// 所属模块：platform / 平台运行时
    /// 功能：返回后端标识与展示名称。
    /// 输入：`self`。
    /// 输出/副作用：描述结构；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / platform runtime
    /// Purpose: Return the backend id and display name.
    /// Inputs: `self`.
    /// Outputs / side effects: the descriptor; no state change.
    /// Complexity: O(1) time and space.
    pub fn info(&self) -> PlatformInfo {
        self.backend.info()
    }

    /// 所属模块：platform / 平台运行时
    /// 功能：返回当前平台的能力集合；应用层据此隐藏或降级不可用功能。
    /// 输入：`self`。
    /// 输出/副作用：能力结构；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / platform runtime
    /// Purpose: Return the capability set; the app layer hides or degrades unsupported features.
    /// Inputs: `self`.
    /// Outputs / side effects: the capability descriptor; no state change.
    /// Complexity: O(1) time and space.
    pub fn caps(&self) -> PlatformCaps {
        self.backend.caps()
    }

    /// 所属模块：platform / 存储目录
    /// 功能：返回用户配置目录（`settings.json` 所在目录）。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / storage directories
    /// Purpose: Return the user config directory (where `settings.json` lives).
    /// Inputs: `self`.
    /// Outputs / side effects: the directory path; nothing is created.
    /// Complexity: O(path length) time and space.
    pub fn config_dir(&self) -> Option<PathBuf> {
        self.backend.config_dir()
    }

    /// 所属模块：platform / 存储目录
    /// 功能：返回缓存目录（插件缓存等）。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / storage directories
    /// Purpose: Return the cache directory (plugin cache and similar).
    /// Inputs: `self`.
    /// Outputs / side effects: the directory path; nothing is created.
    /// Complexity: O(path length) time and space.
    pub fn cache_dir(&self) -> Option<PathBuf> {
        self.backend.cache_dir()
    }

    /// 所属模块：platform / 存储目录
    /// 功能：返回应用私有数据目录；桌面平台与配置目录一致。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / storage directories
    /// Purpose: Return the app-private data directory (the config directory on desktop).
    /// Inputs: `self`.
    /// Outputs / side effects: the directory path; nothing is created.
    /// Complexity: O(path length) time and space.
    pub fn data_dir(&self) -> Option<PathBuf> {
        self.backend.data_dir()
    }

    /// 所属模块：platform / 存储目录
    /// 功能：返回诊断日志目录。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / storage directories
    /// Purpose: Return the diagnostic log directory.
    /// Inputs: `self`.
    /// Outputs / side effects: the directory path; nothing is created.
    /// Complexity: O(path length) time and space.
    pub fn log_dir(&self) -> PathBuf {
        self.backend.log_dir()
    }

    /// 所属模块：platform / 存储目录
    /// 功能：返回系统临时目录（不可写目录的回退位置）。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / storage directories
    /// Purpose: Return the system temp directory (fallback for unwritable locations).
    /// Inputs: `self`.
    /// Outputs / side effects: the directory path; nothing is created.
    /// Complexity: O(path length) time and space.
    pub fn temp_dir(&self) -> PathBuf {
        self.backend.temp_dir()
    }

    /// 所属模块：platform / 文件对话框
    /// 功能：打开“选择文件”对话框；平台不支持时返回 `None`（与用户取消等价）。
    /// 输入：`request`：标题、过滤器、起始目录与父窗口。
    /// 输出/副作用：阻塞当前线程直到用户选择；返回所选路径。
    /// 复杂度：时间取决于用户操作，空间 O(路径长度)。
    ///
    /// Module: platform / file dialogs
    /// Purpose: Show an "open file" dialog; returns `None` when unsupported (same as cancel).
    /// Inputs: `request`: title, filters, start directory and parent window.
    /// Outputs / side effects: blocks the calling thread until the user picks a path.
    /// Complexity: user-bound time, O(path length) space.
    pub fn pick_file(&self, request: &FileRequest<'_>) -> Option<PathBuf> {
        self.backend.pick_file(request)
    }

    /// 所属模块：platform / 文件对话框
    /// 功能：打开“选择文件夹”对话框；平台不支持时返回 `None`。
    /// 输入：`request`：标题、起始目录与父窗口。
    /// 输出/副作用：阻塞当前线程直到用户选择；返回所选目录。
    /// 复杂度：时间取决于用户操作，空间 O(路径长度)。
    ///
    /// Module: platform / file dialogs
    /// Purpose: Show a "pick folder" dialog; returns `None` when unsupported.
    /// Inputs: `request`: title, start directory and parent window.
    /// Outputs / side effects: blocks the calling thread until the user picks a directory.
    /// Complexity: user-bound time, O(path length) space.
    pub fn pick_folder(&self, request: &FileRequest<'_>) -> Option<PathBuf> {
        self.backend.pick_folder(request)
    }

    /// 所属模块：platform / 文件对话框
    /// 功能：打开“另存为”对话框；平台不支持时返回 `None`。
    /// 输入：`request`：标题、过滤器、起始目录、默认文件名与父窗口。
    /// 输出/副作用：阻塞当前线程直到用户确认；返回目标路径。
    /// 复杂度：时间取决于用户操作，空间 O(路径长度)。
    ///
    /// Module: platform / file dialogs
    /// Purpose: Show a "save as" dialog; returns `None` when unsupported.
    /// Inputs: `request`: title, filters, start directory, default name and parent window.
    /// Outputs / side effects: blocks the calling thread until the user confirms.
    /// Complexity: user-bound time, O(path length) space.
    pub fn save_file(&self, request: &FileRequest<'_>) -> Option<PathBuf> {
        self.backend.save_file(request)
    }

    /// 所属模块：platform / 文件导入
    /// 功能：让用户选择一个文件并返回**可由 `std::fs` 读取的本地路径**；取消返回 `Ok(None)`。
    /// 输入：`request` 对话框请求。
    /// 输出/副作用：可能打开系统界面并阻塞当前线程（安卓会把文件复制进沙箱）。
    /// 复杂度：时间取决于用户操作 + 文件大小，空间 O(文件大小)。
    ///
    /// Module: platform / file import
    /// Purpose: Let the user pick a file and return a **local path readable by `std::fs`**;
    /// `Ok(None)` means cancelled.
    /// Inputs: `request`: dialog request.
    /// Outputs / side effects: may open a system UI and block this thread (Android copies the file
    /// into the sandbox).
    /// Complexity: user-bound time + file size; O(file size) space.
    pub fn import_file(&self, request: &FileRequest<'_>) -> Result<Option<PathBuf>, String> {
        self.backend.import_file(request)
    }

    /// 所属模块：platform / 文件导出
    /// 功能：把本地文件复制到用户选择的位置；取消返回 `Ok(None)`，成功返回目标描述。
    /// 输入：
    /// - `source`：本地源文件。
    /// - `request`：对话框请求（默认文件名决定 MIME）。
    /// 输出/副作用：可能打开系统界面并阻塞当前线程。
    /// 复杂度：时间取决于用户操作 + 文件大小，空间 O(缓冲区)。
    ///
    /// Module: platform / file export
    /// Purpose: Copy a local file to a user-chosen location; `Ok(None)` means cancelled, otherwise
    /// returns a description of the target.
    /// Inputs:
    /// - `source`: local source file.
    /// - `request`: dialog request (the default name decides the MIME type).
    /// Outputs / side effects: may open a system UI and block this thread.
    /// Complexity: user-bound time + file size; O(buffer) space.
    pub fn export_copy(
        &self,
        source: &Path,
        request: &FileRequest<'_>,
    ) -> Result<Option<String>, String> {
        self.backend.export_copy(source, request)
    }

    /// 所属模块：platform / 退出
    /// 功能：立即结束进程：Windows 使用 `TerminateProcess` 跳过插件卸载，其他平台走
    /// `std::process::exit`。
    /// 输入：`code`：进程退出码。
    /// 输出/副作用：结束进程；正常情况下不返回。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / exit
    /// Purpose: End the process immediately: `TerminateProcess` on Windows (skips plugin unload),
    /// `std::process::exit` elsewhere.
    /// Inputs: `code`: process exit code.
    /// Outputs / side effects: ends the process; normally never returns.
    /// Complexity: O(1) time and space.
    pub fn exit_process(&self, code: i32) -> ! {
        self.backend.exit_process(code)
    }
}

/// 所属模块：platform / 快捷入口
/// 功能：返回当前平台能力集合；等价于 `PlatformRuntime::standard().caps()`。
/// 输入：无。
/// 输出/副作用：能力结构；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: platform / shortcuts
/// Purpose: Return the capability set; same as `PlatformRuntime::standard().caps()`.
/// Inputs: none.
/// Outputs / side effects: the capability descriptor; no state change.
/// Complexity: O(1) time and space.
pub fn caps() -> PlatformCaps {
    PlatformRuntime::standard().caps()
}

/// 所属模块：platform / 快捷入口
/// 功能：返回用户配置目录；等价于 `PlatformRuntime::standard().config_dir()`。
/// 输入：无。
/// 输出/副作用：目录路径或 `None`；不创建目录。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: platform / shortcuts
/// Purpose: Return the user config directory; same as
/// `PlatformRuntime::standard().config_dir()`.
/// Inputs: none.
/// Outputs / side effects: the path or `None`; nothing is created.
/// Complexity: O(path length) time and space.
pub fn config_dir() -> Option<PathBuf> {
    PlatformRuntime::standard().config_dir()
}

/// 所属模块：platform / 快捷入口
/// 功能：返回缓存目录；等价于 `PlatformRuntime::standard().cache_dir()`。
/// 输入：无。
/// 输出/副作用：目录路径或 `None`；不创建目录。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: platform / shortcuts
/// Purpose: Return the cache directory; same as `PlatformRuntime::standard().cache_dir()`.
/// Inputs: none.
/// Outputs / side effects: the path or `None`; nothing is created.
/// Complexity: O(path length) time and space.
pub fn cache_dir() -> Option<PathBuf> {
    PlatformRuntime::standard().cache_dir()
}

/// 所属模块：platform / 快捷入口
/// 功能：返回诊断日志目录；等价于 `PlatformRuntime::standard().log_dir()`。
/// 输入：无。
/// 输出/副作用：目录路径；不创建目录。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: platform / shortcuts
/// Purpose: Return the log directory; same as `PlatformRuntime::standard().log_dir()`.
/// Inputs: none.
/// Outputs / side effects: the path; nothing is created.
/// Complexity: O(path length) time and space.
pub fn log_dir() -> PathBuf {
    PlatformRuntime::standard().log_dir()
}

/// 所属模块：platform / 快捷入口
/// 功能：返回系统临时目录；等价于 `PlatformRuntime::standard().temp_dir()`。
/// 输入：无。
/// 输出/副作用：目录路径；不创建目录。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: platform / shortcuts
/// Purpose: Return the temp directory; same as `PlatformRuntime::standard().temp_dir()`.
/// Inputs: none.
/// Outputs / side effects: the path; nothing is created.
/// Complexity: O(path length) time and space.
pub fn temp_dir() -> PathBuf {
    PlatformRuntime::standard().temp_dir()
}

/// 所属模块：platform / 快捷入口
/// 功能：打开“选择文件”对话框；等价于 `PlatformRuntime::standard().pick_file(..)`。
/// 输入：`request` 对话框请求。
/// 输出/副作用：阻塞直到用户选择；返回路径或 `None`。
/// 复杂度：时间取决于用户操作，空间 O(路径长度)。
///
/// Module: platform / shortcuts
/// Purpose: Show an open-file dialog; same as `PlatformRuntime::standard().pick_file(..)`.
/// Inputs: `request` — dialog request.
/// Outputs / side effects: blocks until the user picks; returns the path or `None`.
/// Complexity: user-bound time, O(path length) space.
pub fn pick_file(request: &FileRequest<'_>) -> Option<PathBuf> {
    PlatformRuntime::standard().pick_file(request)
}

/// 所属模块：platform / 快捷入口
/// 功能：打开“选择文件夹”对话框；等价于 `PlatformRuntime::standard().pick_folder(..)`。
/// 输入：`request` 对话框请求。
/// 输出/副作用：阻塞直到用户选择；返回目录或 `None`。
/// 复杂度：时间取决于用户操作，空间 O(路径长度)。
///
/// Module: platform / shortcuts
/// Purpose: Show a pick-folder dialog; same as `PlatformRuntime::standard().pick_folder(..)`.
/// Inputs: `request` — dialog request.
/// Outputs / side effects: blocks until the user picks; returns the directory or `None`.
/// Complexity: user-bound time, O(path length) space.
pub fn pick_folder(request: &FileRequest<'_>) -> Option<PathBuf> {
    PlatformRuntime::standard().pick_folder(request)
}

/// 所属模块：platform / 快捷入口
/// 功能：打开“另存为”对话框；等价于 `PlatformRuntime::standard().save_file(..)`。
/// 输入：`request` 对话框请求。
/// 输出/副作用：阻塞直到用户确认；返回路径或 `None`。
/// 复杂度：时间取决于用户操作，空间 O(路径长度)。
///
/// Module: platform / shortcuts
/// Purpose: Show a save-as dialog; same as `PlatformRuntime::standard().save_file(..)`.
/// Inputs: `request` — dialog request.
/// Outputs / side effects: blocks until the user confirms; returns the path or `None`.
/// Complexity: user-bound time, O(path length) space.
pub fn save_file(request: &FileRequest<'_>) -> Option<PathBuf> {
    PlatformRuntime::standard().save_file(request)
}

/// 所属模块：platform / 快捷入口
/// 功能：让用户选择文件并返回本地路径；等价于 `PlatformRuntime::standard().import_file(..)`。
/// 输入：`request` 对话框请求。
/// 输出/副作用：阻塞当前线程；返回路径、取消（`Ok(None)`）或错误。
/// 复杂度：时间取决于用户操作 + 文件大小，空间 O(文件大小)。
///
/// Module: platform / shortcuts
/// Purpose: Let the user pick a file and return a local path; same as
/// `PlatformRuntime::standard().import_file(..)`.
/// Inputs: `request`: dialog request.
/// Outputs / side effects: blocks this thread; returns the path, `Ok(None)` on cancel, or an error.
/// Complexity: user-bound time + file size; O(file size) space.
pub fn import_file(request: &FileRequest<'_>) -> Result<Option<PathBuf>, String> {
    PlatformRuntime::standard().import_file(request)
}

/// 所属模块：platform / 快捷入口
/// 功能：把本地文件复制到用户选择的位置；等价于 `PlatformRuntime::standard().export_copy(..)`。
/// 输入：
/// - `source`：本地源文件。
/// - `request`：对话框请求。
/// 输出/副作用：阻塞当前线程；返回目标描述或取消。
/// 复杂度：时间取决于用户操作 + 文件大小，空间 O(缓冲区)。
///
/// Module: platform / shortcuts
/// Purpose: Copy a local file to a user-chosen location; same as
/// `PlatformRuntime::standard().export_copy(..)`.
/// Inputs:
/// - `source`: local source file.
/// - `request`: dialog request.
/// Outputs / side effects: blocks this thread; returns the target description or a cancel.
/// Complexity: user-bound time + file size; O(buffer) space.
pub fn export_copy(source: &Path, request: &FileRequest<'_>) -> Result<Option<String>, String> {
    PlatformRuntime::standard().export_copy(source, request)
}

/// 所属模块：platform / 快捷入口
/// 功能：立即结束进程；等价于 `PlatformRuntime::standard().exit_process(code)`。
/// 输入：`code` 进程退出码。
/// 输出/副作用：结束进程；正常情况下不返回。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: platform / shortcuts
/// Purpose: End the process immediately; same as `PlatformRuntime::standard().exit_process(code)`.
/// Inputs: `code`: process exit code.
/// Outputs / side effects: ends the process; normally never returns.
/// Complexity: O(1) time and space.
pub fn exit_process(code: i32) -> ! {
    PlatformRuntime::standard().exit_process(code)
}

/// 所属模块：platform / 存储目录
/// 功能：判断目录是否可写（创建后立即删除探针文件）；用于缓存目录回退。
/// 输入：`dir` 待检测目录。
/// 输出/副作用：必要时创建目录并写探针文件；返回是否可写。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: platform / storage directories
/// Purpose: Whether a directory is writable (create a probe file and delete it) for cache fallback.
/// Inputs: `dir`: directory to test.
/// Outputs / side effects: may create the directory and a probe file; returns writability.
/// Complexity: O(1) time and space.
pub fn is_writable_dir(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".surfingstudio-write-test");
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}
