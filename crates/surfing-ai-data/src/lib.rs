//! 所属模块：surfing-ai-data / DLL 入口
//! 文件用途：
//! - 向运行时 DLL 暴露稳定的 C ABI：加载、保存、导入、导出 AI 资料库。
//! - 所有函数返回 `OK` / `OK\n<JSON>` 或 `ERR\n<中文原因>`，由运行时释放字符串。
//! - panic 在 FFI 边界内收敛，避免拖垮宿主进程。
//!
//! 函数清单：
//! - `sfs_ai_load`：读取资料库目录并返回资料库 JSON。
//! - `sfs_ai_save`：把资料库 JSON 拆分写入目录。
//! - `sfs_ai_import`：解析单个技能 / 智能体 / 乐器文件。
//! - `sfs_ai_export`：导出单条资料。
//! - `sfs_ai_export_all`：导出某一类的全部资料。
//! - `sfs_ai_free`：释放本库返回的字符串。
//!
//! Module: surfing-ai-data / DLL entry
//! Purpose:
//! - Expose a stable C ABI to the runtime DLL: load, save, import and export the AI library.
//! - Every function returns `OK` / `OK\n<JSON>` or `ERR\n<Chinese reason>`; the runtime frees it.
//! - Panics are contained inside the FFI boundary so the host process survives.
//!
//! Functions:
//! - `sfs_ai_load`: read the library directory and return library JSON.
//! - `sfs_ai_save`: split library JSON into files.
//! - `sfs_ai_import`: parse one skill / agent / instrument file.
//! - `sfs_ai_export`: export one entry.
//! - `sfs_ai_export_all`: export every entry of one kind.
//! - `sfs_ai_free`: free a string returned by this library.

mod markdown;
mod store;

use std::ffi::{CStr, CString, c_char};
use std::path::PathBuf;

/// 成功且无负载时的返回文本。
/// Reply text for success without payload.
const OK: &str = "OK";
/// 失败前缀。
/// Failure prefix.
const ERR_PREFIX: &str = "ERR\n";

/// 所属模块：surfing-ai-data / DLL 入口
/// 功能：读取资料库目录并返回资料库 JSON。
/// 输入：`root`：以 NUL 结尾的 UTF-8 目录路径。
/// 输出/副作用：C 字符串（`OK\n<JSON>` 或 `ERR\n<原因>`）；只读文件。
/// 复杂度：时间 O(文件数 × 文件大小)，空间 O(资料库大小)。
///
/// Module: surfing-ai-data / DLL entry
/// Purpose: Read the library directory and return library JSON.
/// Inputs: `root`: NUL-terminated UTF-8 directory path.
/// Outputs / side effects: a C string (`OK\n<JSON>` or `ERR\n<reason>`); reads files only.
/// Complexity: O(files × file size) time and O(library size) space.
#[unsafe(no_mangle)]
pub extern "C" fn sfs_ai_load(root: *const c_char) -> *mut c_char {
    reply(guard(|| {
        let root = path_arg(root, "资料库目录")?;
        let library = store::load(&root)?;
        serde_json::to_string(&library).map_err(|error| format!("序列化资料库失败: {error}"))
    }))
}

/// 所属模块：surfing-ai-data / DLL 入口
/// 功能：把资料库 JSON 拆分写入目录。
/// 输入：`root` 目录；`library_json` 资料库 JSON。
/// 输出/副作用：返回 `OK` 或 `ERR\n<原因>`；写文件。
/// 复杂度：时间 O(条目数 × 条目大小)，空间 O(单条大小)。
///
/// Module: surfing-ai-data / DLL entry
/// Purpose: Split library JSON into files under the directory.
/// Inputs: `root`; `library_json`.
/// Outputs / side effects: `OK` or `ERR\n<reason>`; writes files.
/// Complexity: O(entries × entry size) time and O(one entry) space.
#[unsafe(no_mangle)]
pub extern "C" fn sfs_ai_save(root: *const c_char, library_json: *const c_char) -> *mut c_char {
    reply(guard(|| {
        let root = path_arg(root, "资料库目录")?;
        let library = json_arg(library_json, "资料库")?;
        store::save(&root, &library).map(|_| String::new())
    }))
}

/// 所属模块：surfing-ai-data / DLL 入口
/// 功能：解析单个技能 / 智能体 / 乐器文件。
/// 输入：`file` 源文件；`kind_hint` 当前页面类型。
/// 输出/副作用：C 字符串（`OK\n{"kind":"…","entry":{…}}` 或 `ERR\n<原因>`）。
/// 复杂度：时间 O(文件大小)，空间 O(文件大小)。
///
/// Module: surfing-ai-data / DLL entry
/// Purpose: Parse one skill / agent / instrument file.
/// Inputs: `file`; `kind_hint`.
/// Outputs / side effects: `OK\n{"kind":"…","entry":{…}}` or `ERR\n<reason>`.
/// Complexity: O(file size) time and space.
#[unsafe(no_mangle)]
pub extern "C" fn sfs_ai_import(file: *const c_char, kind_hint: *const c_char) -> *mut c_char {
    reply(guard(|| {
        let file = path_arg(file, "导入文件")?;
        let hint = text_arg(kind_hint, "资料类型")?;
        let (kind, entry) = store::import(&file, &hint)?;
        serde_json::to_string(&serde_json::json!({ "kind": kind, "entry": entry }))
            .map_err(|error| format!("序列化导入结果失败: {error}"))
    }))
}

/// 所属模块：surfing-ai-data / DLL 入口
/// 功能：导出资料库中的单条记录。
/// 输入：`library_json`；`kind`；`id`；`target` 目标文件。
/// 输出/副作用：返回 `OK` 或 `ERR\n<原因>`；写文件。
/// 复杂度：时间 O(条目大小)，空间 O(条目大小)。
///
/// Module: surfing-ai-data / DLL entry
/// Purpose: Export one library entry.
/// Inputs: `library_json`; `kind`; `id`; `target`.
/// Outputs / side effects: `OK` or `ERR\n<reason>`; writes the target file.
/// Complexity: O(entry size) time and space.
#[unsafe(no_mangle)]
pub extern "C" fn sfs_ai_export(
    library_json: *const c_char,
    kind: *const c_char,
    id: *const c_char,
    target: *const c_char,
) -> *mut c_char {
    reply(guard(|| {
        let library = json_arg(library_json, "资料库")?;
        let kind = text_arg(kind, "资料类型")?;
        let id = text_arg(id, "条目 id")?;
        let target = path_arg(target, "导出路径")?;
        store::export(&library, &kind, &id, &target).map(|_| String::new())
    }))
}

/// 所属模块：surfing-ai-data / DLL 入口
/// 功能：导出某一类的全部记录。
/// 输入：`library_json`；`kind`；`directory` 目标目录。
/// 输出/副作用：C 字符串（`OK\n{"count":N}` 或 `ERR\n<原因>`）；写文件。
/// 复杂度：时间 O(条目数 × 条目大小)，空间 O(单条大小)。
///
/// Module: surfing-ai-data / DLL entry
/// Purpose: Export every entry of one kind.
/// Inputs: `library_json`; `kind`; `directory`.
/// Outputs / side effects: `OK\n{"count":N}` or `ERR\n<reason>`; writes files.
/// Complexity: O(entries × entry size) time and O(one entry) space.
#[unsafe(no_mangle)]
pub extern "C" fn sfs_ai_export_all(
    library_json: *const c_char,
    kind: *const c_char,
    directory: *const c_char,
) -> *mut c_char {
    reply(guard(|| {
        let library = json_arg(library_json, "资料库")?;
        let kind = text_arg(kind, "资料类型")?;
        let directory = path_arg(directory, "导出目录")?;
        let count = store::export_all(&library, &kind, &directory)?;
        Ok(serde_json::json!({ "count": count }).to_string())
    }))
}

/// 所属模块：surfing-ai-data / DLL 入口
/// 功能：释放本库返回的字符串。
/// 输入：`text` 由本库返回的指针；空指针被忽略。
/// 输出/副作用：释放堆内存；不修改其他状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: surfing-ai-data / DLL entry
/// Purpose: Free a string returned by this library.
/// Inputs: `text`: pointer returned by this library; null is ignored.
/// Outputs / side effects: frees heap memory; no other state change.
/// Complexity: O(1) time and space.
#[unsafe(no_mangle)]
pub extern "C" fn sfs_ai_free(text: *mut c_char) {
    if text.is_null() {
        return;
    }
    // SAFETY: 指针必须由本库的 `CString::into_raw` 产生，且只释放一次。
    // SAFETY: the pointer must come from this library's `CString::into_raw` and is freed once.
    unsafe {
        drop(CString::from_raw(text));
    }
}

/// 所属模块：surfing-ai-data / DLL 入口
/// 功能：捕获 panic 并转换为中文错误，避免跨越 FFI 边界。
/// 输入：`task` 返回 `Result<String, String>` 的闭包。
/// 输出/副作用：C 字符串；内存由 `sfs_ai_free` 释放。
/// 复杂度：时间 O(任务)，空间 O(载荷长度)。
///
/// Module: surfing-ai-data / DLL entry
/// Purpose: Catch panics and convert them into Chinese errors so they never unwind across FFI.
/// Inputs: `task`: closure returning `Result<String, String>`.
/// Outputs / side effects: a C string freed by `sfs_ai_free`.
/// Complexity: O(task) time and O(payload length) space.
fn reply(result: Result<String, String>) -> *mut c_char {
    let text = match result {
        Ok(payload) if payload.is_empty() => OK.to_string(),
        Ok(payload) => format!("OK\n{payload}"),
        Err(error) => format!("{ERR_PREFIX}{error}"),
    };
    // CString 不允许内部 NUL；替换掉以保留错误文本而不是返回空指针。
    // CString rejects interior NUL; replace them so the caller still gets the message.
    let text = text.replace('\0', " ");
    CString::new(text)
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

/// 所属模块：surfing-ai-data / DLL 入口
/// 功能：在 panic 边界内执行任务。
/// 输入：`task` 闭包。
/// 输出/副作用：任务结果或中文 panic 说明。
/// 复杂度：时间 O(任务)，空间 O(任务)。
///
/// Module: surfing-ai-data / DLL entry
/// Purpose: Run a task inside a panic boundary.
/// Inputs: `task`.
/// Outputs / side effects: the task result or a Chinese panic message.
/// Complexity: O(task) time and space.
fn guard(task: impl FnOnce() -> Result<String, String>) -> Result<String, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(task))
        .unwrap_or_else(|_| Err("AI 资料组件内部错误".into()))
}

/// 所属模块：surfing-ai-data / DLL 入口
/// 功能：把 C 字符串参数转换为 UTF-8 文本。
/// 输入：`ptr` 指针；`label` 中文参数名。
/// 输出/副作用：文本；空指针或非法 UTF-8 返回中文错误。
/// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
///
/// Module: surfing-ai-data / DLL entry
/// Purpose: Convert a C string argument into UTF-8 text.
/// Inputs: `ptr`; `label`.
/// Outputs / side effects: text; null or invalid UTF-8 yields a Chinese error.
/// Complexity: O(text length) time and space.
fn text_arg(ptr: *const c_char, label: &str) -> Result<String, String> {
    if ptr.is_null() {
        return Err(format!("{label}为空"));
    }
    // SAFETY: 调用方保证指针在调用期间有效且以 NUL 结尾。
    // SAFETY: the caller guarantees a valid NUL-terminated pointer for the call duration.
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| format!("{label}不是有效的 UTF-8 文本"))?;
    Ok(text.to_string())
}

/// 所属模块：surfing-ai-data / DLL 入口
/// 功能：把 C 字符串参数转换为本地路径。
/// 输入：`ptr` 指针；`label` 中文参数名。
/// 输出/副作用：路径；空指针或空文本返回中文错误。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: surfing-ai-data / DLL entry
/// Purpose: Convert a C string argument into a local path.
/// Inputs: `ptr`; `label`.
/// Outputs / side effects: path; null or empty text yields a Chinese error.
/// Complexity: O(path length) time and space.
fn path_arg(ptr: *const c_char, label: &str) -> Result<PathBuf, String> {
    let text = text_arg(ptr, label)?;
    if text.trim().is_empty() {
        return Err(format!("{label}为空"));
    }
    Ok(PathBuf::from(text))
}

/// 所属模块：surfing-ai-data / DLL 入口
/// 功能：把 C 字符串参数解析为 JSON。
/// 输入：`ptr` 指针；`label` 中文参数名。
/// 输出/副作用：JSON 值；解析失败返回中文错误。
/// 复杂度：时间 O(文本长度)，空间 O(JSON 大小)。
///
/// Module: surfing-ai-data / DLL entry
/// Purpose: Parse a C string argument into JSON.
/// Inputs: `ptr`; `label`.
/// Outputs / side effects: JSON value; parse failures yield a Chinese error.
/// Complexity: O(text length) time and O(JSON size) space.
fn json_arg(ptr: *const c_char, label: &str) -> Result<serde_json::Value, String> {
    let text = text_arg(ptr, label)?;
    serde_json::from_str(&text).map_err(|error| format!("{label} JSON 解析失败: {error}"))
}
