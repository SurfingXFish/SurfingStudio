//! 所属模块：app / AI 资料文件存储
//! 文件用途：
//! - 把 AI 资料库的持久化从 `settings.json` 迁移到用户数据目录 `ai/`：
//!   智能体 JSON、技能 Markdown、乐器 JSON 与顺序清单分类存放。
//! - 首次运行（或资料库目录不存在）时把内存中的资料释放到用户目录；升级时从旧安装目录迁移。
//! - 通过运行时加载 `surfing_ai_data.dll`，由该 DLL 负责 markdown/json 文件格式与导入导出。
//! - 安卓不使用 DLL，继续以 `settings.json` 内的资料库为准。
//!
//! 函数清单：
//! - `ai_root`：返回用户数据目录下的 `ai/`。
//! - `legacy_ai_root`：返回旧版 `<安装目录>/data/ai`（升级迁移）。
//! - `load_library_from`：从指定目录读取有效资料库。
//! - `load`：从文件加载资料库；新目录无效时迁移旧数据或在缺失时释放默认资料。
//! - `save`：把资料库写回分类文件。
//! - `import_file`：解析导入文件并返回 `(类型, 条目 JSON)`。
//! - `export_entry`：导出单条资料。
//! - `export_all`：导出某一类的全部资料。
//!
//! Module: app / AI library file storage
//! Purpose:
//! - Move AI-library persistence from `settings.json` to the user data directory `ai/`: agent JSON,
//!   skill Markdown, instrument JSON and an order manifest stored by category.
//! - Release the in-memory library into the user directory on first run (or when absent); migrate
//!   from the legacy install directory during upgrades.
//! - Load `surfing_ai_data.dll` at runtime; that DLL owns the markdown/json formats and import/export.
//! - Android stays on `settings.json` because it has no DLL.
//!
//! Functions:
//! - `ai_root`: return `ai/` under the user data directory.
//! - `legacy_ai_root`: return the legacy `<install>/data/ai` (upgrade migration).
//! - `load_library_from`: read a valid library from the given directory.
//! - `load`: load the library from files, migrating legacy data or releasing defaults when needed.
//! - `save`: write the library back into categorized files.
//! - `import_file`: parse an import file into `(kind, entry JSON)`.
//! - `export_entry`: export one entry.
//! - `export_all`: export every entry of one kind.

use std::path::{Path, PathBuf};

use crate::ai::AiLibrary;

/// AI 资料库在数据目录下的子目录名。
/// AI library sub-directory name under the data directory.
const AI_DIR: &str = "ai";

/// 所属模块：app / AI 资料文件存储
/// 功能：返回资料库根目录：用户数据目录（Windows 为 `%APPDATA%\SurfingStudio`）下的 `ai/`。
/// 输入：无。
/// 输出/副作用：目录路径或 `None`；不创建目录。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: app / AI library file storage
/// Purpose: Return the library root: `ai/` under the user data directory
/// (`%APPDATA%\SurfingStudio` on Windows).
/// Inputs: none.
/// Outputs / side effects: the path or `None`; nothing is created.
/// Complexity: O(path length) time and space.
pub(super) fn ai_root() -> Option<PathBuf> {
    crate::platform::data_dir().map(|base| base.join(AI_DIR))
}

/// 所属模块：app / AI 资料文件存储 / 升级迁移
/// 功能：返回旧版安装目录下的资料库根 `<安装目录>/data/ai`；安卓没有旧布局时返回 `None`。
/// 输入：无。
/// 输出/副作用：目录路径或 `None`；不创建目录。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: app / AI library file storage / upgrade migration
/// Purpose: Return the legacy library root `<install>/data/ai`; `None` when Android has no legacy
/// layout.
/// Inputs: none.
/// Outputs / side effects: the path or `None`; nothing is created.
/// Complexity: O(path length) time and space.
#[cfg(not(target_os = "android"))]
fn legacy_ai_root() -> Option<PathBuf> {
    crate::platform::legacy_data_dir().map(|base| base.join(AI_DIR))
}

/// 所属模块：app / AI 资料文件存储
/// 功能：从指定目录读取资料库；目录缺失、读取失败、解析失败或没有任何有效条目时返回 `None`。
/// 输入：`root`：资料库根目录。
/// 输出/副作用：有效资料库或 `None`；只读文件，失败写入诊断日志。
/// 复杂度：时间 O(文件数 × 文件大小)，空间 O(资料库大小)。
///
/// Module: app / AI library file storage
/// Purpose: Read the library from the given directory; returns `None` when the directory is absent,
/// reading fails, parsing fails, or the library has no entries.
/// Inputs: `root`: library root directory.
/// Outputs / side effects: a valid library or `None`; reads files and logs failures.
/// Complexity: O(files × file size) time and O(library size) space.
#[cfg(not(target_os = "android"))]
fn load_library_from(root: &Path) -> Option<AiLibrary> {
    let json = match bridge::load(root) {
        Ok(json) => json,
        Err(error) => {
            warn(&format!(
                "读取 AI 资料库失败（{}）：{error}",
                root.display()
            ));
            return None;
        }
    };
    let mut library: AiLibrary = match serde_json::from_str(&json) {
        Ok(library) => library,
        Err(error) => {
            warn(&format!("解析 AI 资料库失败，已忽略该目录：{error}"));
            return None;
        }
    };
    if library.agents.is_empty()
        && library.basics.is_empty()
        && library.techniques.is_empty()
        && library.instruments.is_empty()
    {
        return None;
    }
    library.migrate();
    Some(library)
}

/// 所属模块：app / AI 资料文件存储
/// 功能：从用户数据目录加载资料库；新目录没有有效条目时优先从旧安装目录迁移，仍无数据则把
/// `fallback` 释放为默认文件。
/// 输入：`fallback`：`settings.json` 中的旧资料库（也是首次释放的内容）。
/// 输出/副作用：资料库；可能创建目录、写盘并迁移旧数据；失败时回退到 `fallback`。
/// 复杂度：时间 O(文件数 × 文件大小)，空间 O(资料库大小)。
///
/// Module: app / AI library file storage
/// Purpose: Load the library from the user data directory; when the new directory has no valid
/// entries, migrate from the legacy install directory first and otherwise release `fallback` as the
/// default files.
/// Inputs: `fallback`: the legacy library from `settings.json` (also the seed on first release).
/// Outputs / side effects: the library; may create directories, write disk and migrate legacy data;
/// falls back on failure.
/// Complexity: O(files × file size) time and O(library size) space.
pub(super) fn load(fallback: AiLibrary) -> AiLibrary {
    #[cfg(target_os = "android")]
    {
        let _ = ai_root();
        return fallback;
    }
    #[cfg(not(target_os = "android"))]
    {
        let Some(root) = ai_root() else {
            return fallback;
        };
        // 新目录不存在时直接进入迁移/释放流程，避免对空目录产生读取失败日志。
        // When the new directory is absent go straight to migration/release so an empty location does
        // not produce a read-failure log.
        if root.is_dir() {
            if let Some(library) = load_library_from(&root) {
                return library;
            }
        }
        // 新位置没有有效条目：先迁移旧安装目录的数据，避免升级后丢失自定义智能体与技能。
        // The new location has no valid entries: migrate the legacy install directory first so
        // custom agents and skills survive an upgrade.
        if let Some(library) = legacy_ai_root()
            .filter(|legacy| legacy.is_dir())
            .and_then(|legacy| load_library_from(&legacy))
        {
            save(&library);
            return library;
        }
        save(&fallback);
        fallback
    }
}

/// 所属模块：app / AI 资料文件存储
/// 功能：把资料库写回用户数据目录的分类文件；失败只记录诊断日志。
/// 输入：`library` 当前资料库。
/// 输出/副作用：创建目录并写文件；不修改内存状态。
/// 复杂度：时间 O(条目数 × 条目大小)，空间 O(资料库 JSON)。
///
/// Module: app / AI library file storage
/// Purpose: Write the library back into categorized files under the user data directory; failures
/// only go to the diagnostic log.
/// Inputs: `library`.
/// Outputs / side effects: creates directories and writes files; no in-memory change.
/// Complexity: O(entries × entry size) time and O(library JSON) space.
pub(super) fn save(library: &AiLibrary) {
    #[cfg(target_os = "android")]
    {
        let _ = library;
    }
    #[cfg(not(target_os = "android"))]
    {
        let Some(root) = ai_root() else {
            return;
        };
        let json = match serde_json::to_string(library) {
            Ok(json) => json,
            Err(error) => {
                warn(&format!("序列化 AI 资料库失败：{error}"));
                return;
            }
        };
        if let Err(error) = bridge::save(&root, &json) {
            warn(&format!("写入 AI 资料库失败：{error}"));
        }
    }
}

/// 所属模块：app / AI 资料文件存储
/// 功能：解析一个技能 / 智能体 / 乐器文件。
/// 输入：`file` 源文件；`kind_hint` 当前页面类型。
/// 输出/副作用：`(类型, 条目 JSON)`；只读文件。
/// 复杂度：时间 O(文件大小)，空间 O(文件大小)。
///
/// Module: app / AI library file storage
/// Purpose: Parse one skill / agent / instrument file.
/// Inputs: `file`; `kind_hint`.
/// Outputs / side effects: `(kind, entry JSON)`; reads the file only.
/// Complexity: O(file size) time and space.
#[cfg(not(target_os = "android"))]
pub(super) fn import_file(file: &Path, kind_hint: &str) -> Result<(String, String), String> {
    bridge::import(file, kind_hint)
}

/// 所属模块：app / AI 资料文件存储
/// 功能：导出资料库中的单条记录。
/// 输入：`library` 当前资料库；`kind`；`id`；`target` 目标文件。
/// 输出/副作用：写文件；失败返回中文原因。
/// 复杂度：时间 O(条目大小)，空间 O(条目大小)。
///
/// Module: app / AI library file storage
/// Purpose: Export one entry of the current library.
/// Inputs: `library`; `kind`; `id`; `target`.
/// Outputs / side effects: writes the target file; returns a Chinese error on failure.
/// Complexity: O(entry size) time and space.
#[cfg(not(target_os = "android"))]
pub(super) fn export_entry(
    library: &AiLibrary,
    kind: &str,
    id: &str,
    target: &Path,
) -> Result<(), String> {
    let json =
        serde_json::to_string(library).map_err(|error| format!("序列化 AI 资料库失败：{error}"))?;
    bridge::export(&json, kind, id, target)
}

/// 所属模块：app / AI 资料文件存储
/// 功能：导出某一类的全部记录。
/// 输入：`library`；`kind`；`directory` 目标目录。
/// 输出/副作用：写文件；返回导出数量。
/// 复杂度：时间 O(条目数 × 条目大小)，空间 O(资料库 JSON)。
///
/// Module: app / AI library file storage
/// Purpose: Export every entry of one kind.
/// Inputs: `library`; `kind`; `directory`.
/// Outputs / side effects: writes files; returns the exported count.
/// Complexity: O(entries × entry size) time and O(library JSON) space.
#[cfg(not(target_os = "android"))]
pub(super) fn export_all(
    library: &AiLibrary,
    kind: &str,
    directory: &Path,
) -> Result<usize, String> {
    let json =
        serde_json::to_string(library).map_err(|error| format!("序列化 AI 资料库失败：{error}"))?;
    bridge::export_all(&json, kind, directory)
}

/// 所属模块：app / AI 资料文件存储
/// 功能：写入诊断日志；未初始化日志时静默忽略。
/// 输入：`message` 中文说明。
/// 输出/副作用：追加一条 WARN 记录；不修改业务状态。
/// 复杂度：时间 O(消息长度)，空间 O(消息长度)。
///
/// Module: app / AI library file storage
/// Purpose: Write a diagnostic warning; silently ignored when logging is not initialized.
/// Inputs: `message`.
/// Outputs / side effects: appends one WARN record; no business-state change.
/// Complexity: O(message length) time and space.
#[cfg(not(target_os = "android"))]
fn warn(message: &str) {
    crate::diag::warn("ai.library", "ai.library.storage", &[("error", message)]);
}

/// 所属模块：app / AI 资料文件存储 / DLL 桥
/// 文件用途：
/// - 运行时加载同目录的 `surfing_ai_data.dll`，通过 C ABI 完成文件读写与导入导出。
/// - 返回值约定：`OK`、`OK\n<JSON>` 或 `ERR\n<中文原因>`；字符串由 DLL 分配、本模块释放。
///
/// Module: app / AI library file storage / DLL bridge
/// Purpose:
/// - Load `surfing_ai_data.dll` from the install directory at runtime and use a C ABI for file
///   access and import/export.
/// - Reply contract: `OK`, `OK\n<JSON>` or `ERR\n<Chinese reason>`; the DLL allocates and this
///   module frees.
#[cfg(not(target_os = "android"))]
mod bridge {
    use std::ffi::{CStr, CString, c_char};
    use std::path::Path;
    use std::sync::OnceLock;

    /// 资料库 DLL 文件名（按平台）。
    /// Library DLL file name (per platform).
    const LIBRARY_NAME: &str = if cfg!(target_os = "windows") {
        "surfing_ai_data.dll"
    } else if cfg!(target_os = "macos") {
        "libsurfing_ai_data.dylib"
    } else {
        "libsurfing_ai_data.so"
    };

    type LoadFn = unsafe extern "C" fn(*const c_char) -> *mut c_char;
    type SaveFn = unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_char;
    type ImportFn = unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_char;
    type ExportFn = unsafe extern "C" fn(
        *const c_char,
        *const c_char,
        *const c_char,
        *const c_char,
    ) -> *mut c_char;
    type ExportAllFn =
        unsafe extern "C" fn(*const c_char, *const c_char, *const c_char) -> *mut c_char;
    type FreeFn = unsafe extern "C" fn(*mut c_char);

    /// 已加载的 DLL 与函数表；`_library` 只用于保持映像不被卸载。
    /// Loaded DLL and function table; `_library` keeps the image from being unloaded.
    struct Api {
        _library: libloading::Library,
        load: LoadFn,
        save: SaveFn,
        import: ImportFn,
        export: ExportFn,
        export_all: ExportAllFn,
        free: FreeFn,
    }

    // 函数指针与 libloading 句柄都可跨线程共享；DLL 内部不保存调用状态。
    // Function pointers and the libloading handle are shareable; the DLL keeps no call state.
    unsafe impl Send for Api {}
    unsafe impl Sync for Api {}

    static API: OnceLock<Result<Api, String>> = OnceLock::new();

    /// 所属模块：app / AI 资料文件存储 / DLL 桥
    /// 功能：读取资料库目录并返回资料库 JSON。
    /// 输入：`root` 资料库目录。
    /// 输出/副作用：JSON 文本；DLL 缺失时返回中文错误。
    /// 复杂度：时间 O(文件数 × 文件大小)，空间 O(资料库大小)。
    ///
    /// Module: app / AI library file storage / DLL bridge
    /// Purpose: Read the library directory and return library JSON.
    /// Inputs: `root`.
    /// Outputs / side effects: JSON text; a Chinese error when the DLL is missing.
    /// Complexity: O(files × file size) time and O(library size) space.
    pub(super) fn load(root: &Path) -> Result<String, String> {
        let api = api()?;
        let root = c_text(&root.to_string_lossy(), "资料库目录")?;
        // SAFETY: 参数是以 NUL 结尾的有效指针，返回字符串由 DLL 分配。
        // SAFETY: the argument is a valid NUL-terminated pointer and the reply is DLL-allocated.
        unsafe { call(api, (api.load)(root.as_ptr())) }
    }

    /// 所属模块：app / AI 资料文件存储 / DLL 桥
    /// 功能：把资料库 JSON 写入目录。
    /// 输入：`root`；`json` 资料库 JSON。
    /// 输出/副作用：成功返回空；失败返回中文原因。
    /// 复杂度：时间 O(条目数 × 条目大小)，空间 O(JSON 大小)。
    ///
    /// Module: app / AI library file storage / DLL bridge
    /// Purpose: Write library JSON into the directory.
    /// Inputs: `root`; `json`.
    /// Outputs / side effects: empty on success; a Chinese error on failure.
    /// Complexity: O(entries × entry size) time and O(JSON size) space.
    pub(super) fn save(root: &Path, json: &str) -> Result<(), String> {
        let api = api()?;
        let root = c_text(&root.to_string_lossy(), "资料库目录")?;
        let json = c_text(json, "资料库")?;
        // SAFETY: 两个参数都是以 NUL 结尾的有效指针。
        // SAFETY: both arguments are valid NUL-terminated pointers.
        unsafe { call(api, (api.save)(root.as_ptr(), json.as_ptr())).map(|_| ()) }
    }

    /// 所属模块：app / AI 资料文件存储 / DLL 桥
    /// 功能：解析导入文件。
    /// 输入：`file`；`kind_hint`。
    /// 输出/副作用：`(类型, 条目 JSON)`；只读文件。
    /// 复杂度：时间 O(文件大小)，空间 O(文件大小)。
    ///
    /// Module: app / AI library file storage / DLL bridge
    /// Purpose: Parse an import file.
    /// Inputs: `file`; `kind_hint`.
    /// Outputs / side effects: `(kind, entry JSON)`; reads the file only.
    /// Complexity: O(file size) time and space.
    pub(super) fn import(file: &Path, kind_hint: &str) -> Result<(String, String), String> {
        let api = api()?;
        let file = c_text(&file.to_string_lossy(), "导入文件")?;
        let hint = c_text(kind_hint, "资料类型")?;
        // SAFETY: 两个参数都是以 NUL 结尾的有效指针。
        // SAFETY: both arguments are valid NUL-terminated pointers.
        let payload = unsafe { call(api, (api.import)(file.as_ptr(), hint.as_ptr()))? };
        let value: serde_json::Value =
            serde_json::from_str(&payload).map_err(|error| format!("解析导入结果失败：{error}"))?;
        let kind = value
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "导入结果缺少类型".to_string())?
            .to_string();
        let entry = value
            .get("entry")
            .ok_or_else(|| "导入结果缺少条目".to_string())?;
        Ok((kind, entry.to_string()))
    }

    /// 所属模块：app / AI 资料文件存储 / DLL 桥
    /// 功能：导出单条资料。
    /// 输入：`json` 资料库 JSON；`kind`；`id`；`target`。
    /// 输出/副作用：写文件；失败返回中文原因。
    /// 复杂度：时间 O(条目大小)，空间 O(JSON 大小)。
    ///
    /// Module: app / AI library file storage / DLL bridge
    /// Purpose: Export one entry.
    /// Inputs: `json`; `kind`; `id`; `target`.
    /// Outputs / side effects: writes the target file; returns a Chinese error on failure.
    /// Complexity: O(entry size) time and O(JSON size) space.
    pub(super) fn export(json: &str, kind: &str, id: &str, target: &Path) -> Result<(), String> {
        let api = api()?;
        let json = c_text(json, "资料库")?;
        let kind = c_text(kind, "资料类型")?;
        let id = c_text(id, "条目 id")?;
        let target = c_text(&target.to_string_lossy(), "导出路径")?;
        // SAFETY: 四个参数都是以 NUL 结尾的有效指针。
        // SAFETY: all four arguments are valid NUL-terminated pointers.
        unsafe {
            call(
                api,
                (api.export)(json.as_ptr(), kind.as_ptr(), id.as_ptr(), target.as_ptr()),
            )
            .map(|_| ())
        }
    }

    /// 所属模块：app / AI 资料文件存储 / DLL 桥
    /// 功能：导出某一类的全部资料。
    /// 输入：`json`；`kind`；`directory`。
    /// 输出/副作用：写文件；返回导出数量。
    /// 复杂度：时间 O(条目数 × 条目大小)，空间 O(JSON 大小)。
    ///
    /// Module: app / AI library file storage / DLL bridge
    /// Purpose: Export every entry of one kind.
    /// Inputs: `json`; `kind`; `directory`.
    /// Outputs / side effects: writes files; returns the exported count.
    /// Complexity: O(entries × entry size) time and O(JSON size) space.
    pub(super) fn export_all(json: &str, kind: &str, directory: &Path) -> Result<usize, String> {
        let api = api()?;
        let json = c_text(json, "资料库")?;
        let kind = c_text(kind, "资料类型")?;
        let directory = c_text(&directory.to_string_lossy(), "导出目录")?;
        // SAFETY: 三个参数都是以 NUL 结尾的有效指针。
        // SAFETY: all three arguments are valid NUL-terminated pointers.
        let payload = unsafe {
            call(
                api,
                (api.export_all)(json.as_ptr(), kind.as_ptr(), directory.as_ptr()),
            )?
        };
        let value: serde_json::Value =
            serde_json::from_str(&payload).map_err(|error| format!("解析导出结果失败：{error}"))?;
        Ok(value
            .get("count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default() as usize)
    }

    /// 所属模块：app / AI 资料文件存储 / DLL 桥
    /// 功能：返回已加载的 API，首次调用时从安装目录加载 DLL。
    /// 输入：无。
    /// 输出/副作用：API 引用或中文错误；只加载一次。
    /// 复杂度：时间 O(加载 DLL)，空间 O(1)。
    ///
    /// Module: app / AI library file storage / DLL bridge
    /// Purpose: Return the loaded API, loading the DLL from the install directory on first use.
    /// Inputs: none.
    /// Outputs / side effects: the API reference or a Chinese error; loaded once.
    /// Complexity: O(DLL load) time and O(1) space.
    fn api() -> Result<&'static Api, String> {
        API.get_or_init(load_api)
            .as_ref()
            .map_err(|error| error.to_string())
    }

    /// 所属模块：app / AI 资料文件存储 / DLL 桥
    /// 功能：从安装目录加载资料库 DLL 并解析全部导出符号。
    /// 输入：无。
    /// 输出/副作用：API 或中文错误；加载失败不会 panic。
    /// 复杂度：时间 O(加载 DLL + 符号数)，空间 O(1)。
    ///
    /// Module: app / AI library file storage / DLL bridge
    /// Purpose: Load the library DLL from the install directory and resolve every export.
    /// Inputs: none.
    /// Outputs / side effects: the API or a Chinese error; never panics.
    /// Complexity: O(DLL load + symbols) time and O(1) space.
    fn load_api() -> Result<Api, String> {
        let directory =
            crate::platform::install_dir().ok_or_else(|| "无法确定软件安装目录".to_string())?;
        let path = directory.join(LIBRARY_NAME);
        if !path.is_file() {
            return Err(format!(
                "缺少 AI 资料组件：{}；请重新安装 SurfingStudio",
                path.display()
            ));
        }
        // SAFETY: 库在进程剩余生命周期内保持加载；符号签名由 DLL 的 C ABI 保证。
        // SAFETY: the library stays loaded for the rest of the process and the DLL's C ABI defines the
        // symbol signatures.
        unsafe {
            let library = libloading::Library::new(&path)
                .map_err(|error| format!("加载 AI 资料组件失败：{error}"))?;
            let load = *library
                .get::<LoadFn>(b"sfs_ai_load\0")
                .map_err(|error| format!("AI 资料组件缺少 sfs_ai_load：{error}"))?;
            let save = *library
                .get::<SaveFn>(b"sfs_ai_save\0")
                .map_err(|error| format!("AI 资料组件缺少 sfs_ai_save：{error}"))?;
            let import = *library
                .get::<ImportFn>(b"sfs_ai_import\0")
                .map_err(|error| format!("AI 资料组件缺少 sfs_ai_import：{error}"))?;
            let export = *library
                .get::<ExportFn>(b"sfs_ai_export\0")
                .map_err(|error| format!("AI 资料组件缺少 sfs_ai_export：{error}"))?;
            let export_all = *library
                .get::<ExportAllFn>(b"sfs_ai_export_all\0")
                .map_err(|error| format!("AI 资料组件缺少 sfs_ai_export_all：{error}"))?;
            let free = *library
                .get::<FreeFn>(b"sfs_ai_free\0")
                .map_err(|error| format!("AI 资料组件缺少 sfs_ai_free：{error}"))?;
            Ok(Api {
                _library: library,
                load,
                save,
                import,
                export,
                export_all,
                free,
            })
        }
    }

    /// 所属模块：app / AI 资料文件存储 / DLL 桥
    /// 功能：调用 DLL 并复制、释放返回字符串。
    /// 输入：`api`；`ptr` DLL 返回的字符串指针。
    /// 输出/副作用：`OK` 后的负载或中文错误；释放 DLL 字符串。
    /// 复杂度：时间 O(返回长度)，空间 O(返回长度)。
    ///
    /// Module: app / AI library file storage / DLL bridge
    /// Purpose: Call the DLL, copy and free the returned string.
    /// Inputs: `api`; `ptr`.
    /// Outputs / side effects: the payload after `OK` or a Chinese error; frees the DLL string.
    /// Complexity: O(reply length) time and space.
    unsafe fn call(api: &Api, ptr: *mut c_char) -> Result<String, String> {
        if ptr.is_null() {
            return Err("AI 资料组件返回空指针".into());
        }
        // SAFETY: 指针由 DLL 分配且以 NUL 结尾，复制完成后立即归还。
        // SAFETY: the pointer is DLL-allocated and NUL-terminated; it is returned right after copying.
        let text = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: 每个返回指针只释放一次。
        // SAFETY: each returned pointer is freed exactly once.
        unsafe { (api.free)(ptr) };
        if let Some(error) = text.strip_prefix("ERR\n") {
            return Err(error.to_string());
        }
        if text == "OK" {
            return Ok(String::new());
        }
        Ok(text
            .strip_prefix("OK\n")
            .map(str::to_string)
            .unwrap_or(text))
    }

    /// 所属模块：app / AI 资料文件存储 / DLL 桥
    /// 功能：把文本转换为以 NUL 结尾的 C 字符串。
    /// 输入：`text`；`label` 中文参数名。
    /// 输出/副作用：C 字符串；包含 NUL 时返回错误。
    /// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
    ///
    /// Module: app / AI library file storage / DLL bridge
    /// Purpose: Convert text into a NUL-terminated C string.
    /// Inputs: `text`; `label`.
    /// Outputs / side effects: C string; interior NUL yields an error.
    /// Complexity: O(text length) time and space.
    fn c_text(text: &str, label: &str) -> Result<CString, String> {
        CString::new(text).map_err(|_| format!("{label}包含非法空字符"))
    }
}
