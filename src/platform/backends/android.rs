//! 所属模块：platform / 安卓后端
//! 文件用途：
//! - 安卓平台后端：应用私有目录来自 NativeActivity，没有系统文件对话框，也没有 VST 宿主。
//! - 只提供系统音频宿主（AAudio / OpenSL ES 由 cpal 选择），退出使用 `std::process::exit`。
//!
//! 函数清单：
//! - `backend`：返回安卓后端静态实例。
//! - `install_storage`：记录应用私有目录与缓存目录（由 `android_main` 调用）。
//! - `AndroidPlatform::info` / `caps`：标识与能力。
//! - `AndroidPlatform::config_dir` / `cache_dir` / `data_dir` / `log_dir` / `temp_dir`：目录。
//! - `AndroidPlatform::pick_file` / `pick_folder` / `save_file`：不支持系统对话框，返回 `None`。
//! - `AndroidPlatform::exit_process`：终止进程。
//!
//! Module: platform / Android backend
//! Purpose:
//! - Android backend: the app-private directories come from NativeActivity, there is no system file
//!   dialog and no VST host.
//! - Only the system audio host is offered (cpal picks AAudio / OpenSL ES) and exit uses
//!   `std::process::exit`.
//!
//! Functions:
//! - `backend`: return the Android backend singleton.
//! - `install_storage`: record the app-private and cache directories (called from `android_main`).
//! - `AndroidPlatform::info` / `caps`: identity and capabilities.
//! - `AndroidPlatform::config_dir` / `cache_dir` / `data_dir` / `log_dir` / `temp_dir`: directories.
//! - `AndroidPlatform::pick_file` / `pick_folder` / `save_file`: no system dialog, always `None`.
//! - `AndroidPlatform::exit_process`: terminate the process.

use super::super::{FileRequest, PlatformBackend, PlatformCaps, PlatformInfo};
use std::path::PathBuf;
use std::sync::OnceLock;

/// 安卓应用私有的目录集合。
/// Android app-private directory set.
#[derive(Clone, Debug)]
struct AndroidStorage {
    /// 应用私有数据目录（NativeActivity 的 `internal_data_path`）。
    /// App-private data directory (NativeActivity's `internal_data_path`).
    data: PathBuf,
    /// 应用缓存目录。
    /// App cache directory.
    cache: PathBuf,
}

/// 运行期安装的目录；`android_main` 在窗口创建前写入。
/// Directories installed at runtime; `android_main` fills this before the window exists.
static STORAGE: OnceLock<AndroidStorage> = OnceLock::new();

/// 安卓后端实例。
/// Android backend instance.
struct AndroidPlatform;

/// 静态后端。
/// Static backend.
static BACKEND: AndroidPlatform = AndroidPlatform;

/// 所属模块：platform / 安卓后端
/// 功能：返回安卓后端。
/// 输入：无。
/// 输出/副作用：静态引用；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: platform / Android backend
/// Purpose: Return the Android backend.
/// Inputs: none.
/// Outputs / side effects: a static reference; no state change.
/// Complexity: O(1) time and space.
pub(super) fn backend() -> &'static dyn PlatformBackend {
    &BACKEND
}

/// 所属模块：platform / 安卓后端 / 存储
/// 功能：记录应用私有目录与缓存目录；缺省项回退到系统临时目录。
/// 输入：
/// - `data`：应用私有数据目录（通常为 `/data/user/0/<包名>/files`）。
/// - `cache`：应用缓存目录；`None` 时使用数据目录的同级 `cache`。
/// 输出/副作用：写入进程级目录表；重复调用只保留第一次的结果。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: platform / Android backend / storage
/// Purpose: Record the app-private and cache directories, falling back to the temp directory.
/// Inputs:
/// - `data`: app-private data directory (usually `/data/user/0/<package>/files`).
/// - `cache`: app cache directory; `None` uses `cache` next to the data directory.
/// Outputs / side effects: fills the process-wide table; later calls are ignored.
/// Complexity: O(path length) time and space.
pub fn install_storage(data: Option<PathBuf>, cache: Option<PathBuf>) {
    let fallback = std::env::temp_dir();
    let data = data
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| fallback.join(super::super::APP_DIR_NAME));
    let cache = cache
        .or_else(|| data.parent().map(|parent| parent.join("cache")))
        .unwrap_or_else(|| fallback.join(super::super::APP_DIR_NAME));
    let _ = STORAGE.set(AndroidStorage { data, cache });
}

/// 所属模块：platform / 安卓后端 / 存储
/// 功能：读取已安装的数据目录；未安装时回退到系统临时目录。
/// 输入：无。
/// 输出/副作用：数据目录；不修改状态。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: platform / Android backend / storage
/// Purpose: Read the installed data directory, falling back to the temp directory.
/// Inputs: none.
/// Outputs / side effects: the data directory; no state change.
/// Complexity: O(path length) time and space.
fn data_dir() -> PathBuf {
    STORAGE
        .get()
        .map(|storage| storage.data.clone())
        .unwrap_or_else(|| std::env::temp_dir().join(super::super::APP_DIR_NAME))
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：记录 JavaVM 与 Activity，供 Storage Access Framework 使用。
/// 输入：
/// - `vm`：`AndroidApp::vm_as_ptr()`。
/// - `activity`：`AndroidApp::activity_as_ptr()`。
/// 输出/副作用：写入 SAF 桥的进程级上下文；重复调用忽略。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Store the JavaVM and Activity for the Storage Access Framework.
/// Inputs:
/// - `vm`: `AndroidApp::vm_as_ptr()`.
/// - `activity`: `AndroidApp::activity_as_ptr()`.
/// Outputs / side effects: fills the SAF bridge context; later calls are ignored.
/// Complexity: O(1) time and space.
pub fn install_activity(vm: *mut std::ffi::c_void, activity: *mut std::ffi::c_void) {
    super::android_saf::install(vm, activity);
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：把 SAF 文档导入到应用沙箱：按显示名清洗后写到 `<数据>/Imported/`。
/// 输入：`request`：对话框请求（用于回退扩展名）。
/// 输出/副作用：打开系统文件选择器并在当前线程等待；返回沙箱内路径，取消返回 `Ok(None)`。
/// 复杂度：时间取决于用户操作 + O(文件大小)，空间 O(文件大小)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Import a SAF document into `<data>/Imported/`, named after its display name.
/// Inputs: `request`: dialog request (used for the fallback extension).
/// Outputs / side effects: opens the system picker and blocks this thread; returns the sandbox
/// path, or `Ok(None)` when cancelled.
/// Complexity: user-bound time + O(file size); O(file size) space.
fn import_via_saf(request: &FileRequest<'_>) -> Result<Option<PathBuf>, String> {
    // SAF 只能按 MIME 过滤；工程文件没有注册的 MIME，因此接受全部，由加载器校验格式。
    // SAF filters by MIME only; project files have no registered MIME, so accept everything and let
    // the loader validate the format.
    let Some((uri, display_name)) = super::android_saf::pick_document("*/*") else {
        return Ok(None);
    };
    let fallback_extension = request
        .filters
        .first()
        .and_then(|(_, extensions)| extensions.first().copied())
        .unwrap_or("sfsd");
    let name = sandbox_file_name(display_name.as_deref(), fallback_extension);
    let target = data_dir().join("Imported").join(name);
    super::android_saf::copy_from_uri(&uri, &target)?;
    Ok(Some(target))
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：把沙箱文件通过 SAF 写入用户选择的公共目录。
/// 输入：
/// - `source`：沙箱内源文件。
/// - `request`：对话框请求（默认文件名决定 MIME 与建议名称）。
/// 输出/副作用：打开系统“保存到”界面并在当前线程等待；返回目标显示名/URI，取消返回 `Ok(None)`。
/// 复杂度：时间取决于用户操作 + O(文件大小)，空间 O(缓冲区)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Write a sandbox file into a user-chosen public location through SAF.
/// Inputs:
/// - `source`: source file inside the sandbox.
/// - `request`: dialog request (the default name decides MIME and the suggested title).
/// Outputs / side effects: opens the system "save to" UI and blocks this thread; returns the target
/// display name/URI, or `Ok(None)` when cancelled.
/// Complexity: user-bound time + O(file size); O(buffer) space.
fn export_via_saf(
    source: &std::path::Path,
    request: &FileRequest<'_>,
) -> Result<Option<String>, String> {
    let title = request.default_name.unwrap_or("SurfingStudio");
    let mime = mime_for_name(title);
    let Some((uri, display_name)) = super::android_saf::create_document(mime, title) else {
        return Ok(None);
    };
    super::android_saf::copy_to_uri(&uri, source)?;
    Ok(Some(display_name.unwrap_or(uri)))
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：把显示名清洗成沙箱内安全的文件名，并保证带扩展名。
/// 输入：
/// - `display_name`：SAF 提供的显示名，可能缺失。
/// - `fallback_extension`：显示名缺少扩展名时使用的扩展名（不带点）。
/// 输出/副作用：安全文件名；不修改状态。
/// 复杂度：时间 O(名称长度)，空间 O(名称长度)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Sanitise a display name into a safe sandbox file name with an extension.
/// Inputs:
/// - `display_name`: display name from SAF, may be missing.
/// - `fallback_extension`: extension (no dot) used when the name lacks one.
/// Outputs / side effects: the safe file name; no state change.
/// Complexity: O(name length) time and space.
fn sandbox_file_name(display_name: Option<&str>, fallback_extension: &str) -> String {
    let raw = display_name.unwrap_or("imported");
    let cleaned: String = raw
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, '.' | '_' | '-' | ' ') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    let base = if trimmed.is_empty() {
        "imported"
    } else {
        trimmed
    };
    if std::path::Path::new(base).extension().is_some() {
        base.to_string()
    } else {
        format!("{base}.{fallback_extension}")
    }
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：按扩展名推断 SAF 使用的 MIME 类型。
/// 输入：`name` 文件名或带扩展名的名称。
/// 输出/副作用：MIME 字符串；不修改状态。
/// 复杂度：时间 O(名称长度)，空间 O(1)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Guess the SAF MIME type from a file extension.
/// Inputs: `name`: file name or a name carrying an extension.
/// Outputs / side effects: the MIME string; no state change.
/// Complexity: O(name length) time and O(1) space.
fn mime_for_name(name: &str) -> &'static str {
    let extension = std::path::Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case("wav") {
        "audio/wav"
    } else {
        "application/octet-stream"
    }
}

impl PlatformBackend for AndroidPlatform {
    /// 所属模块：platform / 安卓后端
    /// 功能：返回安卓后端标识。
    /// 输入：`self`。
    /// 输出/副作用：标识；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / Android backend
    /// Purpose: Return the Android backend identity.
    /// Inputs: `self`.
    /// Outputs / side effects: the descriptor; no state change.
    /// Complexity: O(1) time and space.
    fn info(&self) -> PlatformInfo {
        PlatformInfo {
            id: "android",
            label: "Android",
        }
    }

    /// 所属模块：platform / 安卓后端
    /// 功能：返回安卓能力：单窗口、无系统对话框、无插件宿主、固定系统音频。
    /// 输入：`self`。
    /// 输出/副作用：能力；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / Android backend
    /// Purpose: Return Android capabilities: one window, no system dialog, no plugin host and the
    /// fixed system audio path.
    /// Inputs: `self`.
    /// Outputs / side effects: the descriptor; no state change.
    /// Complexity: O(1) time and space.
    fn caps(&self) -> PlatformCaps {
        PlatformCaps::ANDROID
    }

    /// 所属模块：platform / 安卓后端 / 存储
    /// 功能：返回应用私有数据目录（设置文件写在其中）。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / Android backend / storage
    /// Purpose: Return the app-private data directory (settings are written there).
    /// Inputs: `self`.
    /// Outputs / side effects: the path; nothing is created.
    /// Complexity: O(path length) time and space.
    fn config_dir(&self) -> Option<PathBuf> {
        Some(data_dir())
    }

    /// 所属模块：platform / 安卓后端 / 存储
    /// 功能：返回应用缓存目录。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / Android backend / storage
    /// Purpose: Return the app cache directory.
    /// Inputs: `self`.
    /// Outputs / side effects: the path; nothing is created.
    /// Complexity: O(path length) time and space.
    fn cache_dir(&self) -> Option<PathBuf> {
        Some(
            STORAGE
                .get()
                .map(|storage| storage.cache.clone())
                .unwrap_or_else(|| std::env::temp_dir().join(super::super::APP_DIR_NAME)),
        )
    }

    /// 所属模块：platform / 安卓后端 / 存储
    /// 功能：返回应用私有数据目录。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / Android backend / storage
    /// Purpose: Return the app-private data directory.
    /// Inputs: `self`.
    /// Outputs / side effects: the path; nothing is created.
    /// Complexity: O(path length) time and space.
    fn data_dir(&self) -> Option<PathBuf> {
        Some(data_dir())
    }

    /// 所属模块：platform / 安卓后端 / 存储
    /// 功能：返回日志目录（数据目录下的 `logs`）。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / Android backend / storage
    /// Purpose: Return the log directory (`logs` under the data directory).
    /// Inputs: `self`.
    /// Outputs / side effects: the path; nothing is created.
    /// Complexity: O(path length) time and space.
    fn log_dir(&self) -> PathBuf {
        data_dir().join(super::super::LOG_DIR_SUFFIX)
    }

    /// 所属模块：platform / 安卓后端 / 存储
    /// 功能：返回系统临时目录（`TMPDIR` 指向应用缓存目录）。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / Android backend / storage
    /// Purpose: Return the temp directory (`TMPDIR` points at the app cache directory).
    /// Inputs: `self`.
    /// Outputs / side effects: the path; nothing is created.
    /// Complexity: O(path length) time and space.
    fn temp_dir(&self) -> PathBuf {
        std::env::temp_dir()
    }

    /// 所属模块：platform / 安卓后端 / 对话框
    /// 功能：安卓没有系统文件对话框，返回 `None`（与用户取消等价）。
    /// 输入：`_request` 对话框请求。
    /// 输出/副作用：返回 `None`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / Android backend / dialogs
    /// Purpose: Android has no system file dialog, so return `None` (same as cancel).
    /// Inputs: `_request`: dialog request.
    /// Outputs / side effects: returns `None`.
    /// Complexity: O(1) time and space.
    fn pick_file(&self, _request: &FileRequest<'_>) -> Option<PathBuf> {
        None
    }

    /// 所属模块：platform / 安卓后端 / 对话框
    /// 功能：安卓没有系统文件夹对话框，返回 `None`。
    /// 输入：`_request` 对话框请求。
    /// 输出/副作用：返回 `None`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / Android backend / dialogs
    /// Purpose: Android has no system folder dialog, so return `None`.
    /// Inputs: `_request`: dialog request.
    /// Outputs / side effects: returns `None`.
    /// Complexity: O(1) time and space.
    fn pick_folder(&self, _request: &FileRequest<'_>) -> Option<PathBuf> {
        None
    }

    /// 所属模块：platform / 安卓后端 / 对话框
    /// 功能：安卓没有系统另存为对话框，返回 `None`。
    /// 输入：`_request` 对话框请求。
    /// 输出/副作用：返回 `None`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / Android backend / dialogs
    /// Purpose: Android has no system save-as dialog, so return `None`.
    /// Inputs: `_request`: dialog request.
    /// Outputs / side effects: returns `None`.
    /// Complexity: O(1) time and space.
    fn save_file(&self, _request: &FileRequest<'_>) -> Option<PathBuf> {
        None
    }

    /// 所属模块：platform / 安卓后端 / SAF
    /// 功能：用 SAF 打开文档并复制进应用沙箱，返回可被 `std::fs` 读取的路径。
    /// 输入：`request` 对话框请求。
    /// 输出/副作用：打开系统文件选择器并阻塞当前线程（必须工作线程调用）；取消返回 `Ok(None)`。
    /// 复杂度：时间取决于用户操作 + O(文件大小)，空间 O(文件大小)。
    ///
    /// Module: platform / Android backend / SAF
    /// Purpose: Open a document with SAF, copy it into the sandbox and return a `std::fs`-readable
    /// path.
    /// Inputs: `request`: dialog request.
    /// Outputs / side effects: opens the system picker and blocks this thread (worker thread only);
    /// `Ok(None)` when cancelled.
    /// Complexity: user-bound time + O(file size); O(file size) space.
    fn import_file(&self, request: &FileRequest<'_>) -> Result<Option<PathBuf>, String> {
        import_via_saf(request)
    }

    /// 所属模块：platform / 安卓后端 / SAF
    /// 功能：用 SAF 在公共目录新建文档并写入本地文件。
    /// 输入：
    /// - `source`：沙箱内源文件。
    /// - `request`：对话框请求（默认文件名决定 MIME）。
    /// 输出/副作用：打开系统“保存到”界面并阻塞当前线程（必须工作线程调用）；取消返回 `Ok(None)`。
    /// 复杂度：时间取决于用户操作 + O(文件大小)，空间 O(缓冲区)。
    ///
    /// Module: platform / Android backend / SAF
    /// Purpose: Create a document in a public folder with SAF and write a local file into it.
    /// Inputs:
    /// - `source`: source file inside the sandbox.
    /// - `request`: dialog request (the default name decides the MIME type).
    /// Outputs / side effects: opens the system "save to" UI and blocks this thread (worker thread
    /// only); `Ok(None)` when cancelled.
    /// Complexity: user-bound time + O(file size); O(buffer) space.
    fn export_copy(
        &self,
        source: &std::path::Path,
        request: &FileRequest<'_>,
    ) -> Result<Option<String>, String> {
        export_via_saf(source, request)
    }

    /// 所属模块：platform / 安卓后端 / 退出
    /// 功能：立即结束进程（安卓没有插件 DLL 卸载链，直接退出即可）。
    /// 输入：`code` 进程退出码。
    /// 输出/副作用：结束进程；正常情况下不返回。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / Android backend / exit
    /// Purpose: End the process immediately (Android has no plugin DLL unload chain).
    /// Inputs: `code`: process exit code.
    /// Outputs / side effects: ends the process; normally never returns.
    /// Complexity: O(1) time and space.
    fn exit_process(&self, code: i32) -> ! {
        std::process::exit(code)
    }
}
