//! 所属模块：platform / 安卓后端 / SAF 桥
//! 文件用途：
//! - 通过 JNI 调用 Android Storage Access Framework：打开文档、新建文档，并从/向 `content://`
//!   URI 复制字节（用 `ParcelFileDescriptor.detachFd` 拿真实文件描述符，避免逐字节过 JNI）。
//! - 结果由 `MainActivity.onActivityResult` 经 `nativeOnActivityResult` 回传，放进进程内队列；
//!   **等待必须在工作线程上进行**，因为 UI 线程同时是 Android 主线程，阻塞它会导致结果回不来。
//!
//! 函数清单：
//! - `install`：记录 JavaVM 与 Activity 的全局引用（`android_main` 调用）。
//! - `pick_document`：发起“打开文档”并阻塞等待结果，返回 `(uri, 显示名)`。
//! - `create_document`：发起“新建文档”并阻塞等待结果，返回 `(uri, 显示名)`。
//! - `copy_from_uri` / `copy_to_uri`：把 SAF 文档与本地文件互相复制。
//! - `Java_com_surfingstudio_app_MainActivity_nativeOnActivityResult`：Java → Rust 的结果回调。
//! - `trace` / `clear_exception`：诊断日志与挂起异常清理（jni-rs 只返回错误、不自动清除异常）。
//!
//! Module: platform / Android backend / SAF bridge
//! Purpose:
//! - Drive the Android Storage Access Framework over JNI: open/create documents and copy bytes to
//!   and from `content://` URIs (via `ParcelFileDescriptor.detachFd`, so bytes never cross JNI).
//! - Results come back from `MainActivity.onActivityResult` through `nativeOnActivityResult` into a
//!   process-wide queue; **waiting must happen on a worker thread**, because the UI thread is also
//!   the Android main thread and blocking it would prevent the result from arriving.
//!
//! Functions:
//! - `install`: store the JavaVM and a global Activity reference (called from `android_main`).
//! - `pick_document`: start ACTION_OPEN_DOCUMENT and block for the result; returns `(uri, name)`.
//! - `create_document`: start ACTION_CREATE_DOCUMENT and block for the result.
//! - `copy_from_uri` / `copy_to_uri`: copy between SAF documents and local files.
//! - `Java_com_surfingstudio_app_MainActivity_nativeOnActivityResult`: Java → Rust result callback.
//! - `trace` / `clear_exception`: diagnostics and pending-exception cleanup (jni-rs reports an error
//!   but leaves the Java exception pending, which aborts the process on the next JNI call).

use jni::objects::{GlobalRef, JObject, JString, JValue};
use jni::sys::jint;
use jni::{JNIEnv, JavaVM};
use std::path::Path;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

/// 打开文档的请求码，必须与 `MainActivity.REQUEST_OPEN_DOCUMENT` 一致。
/// Request code for open-document; must match `MainActivity.REQUEST_OPEN_DOCUMENT`.
const REQUEST_OPEN_DOCUMENT: jint = 1001;
/// 新建文档的请求码，必须与 `MainActivity.REQUEST_CREATE_DOCUMENT` 一致。
/// Request code for create-document; must match `MainActivity.REQUEST_CREATE_DOCUMENT`.
const REQUEST_CREATE_DOCUMENT: jint = 1002;
/// 等待用户完成选择的超时时间：用户可能长时间停留在文件选择器里。
/// How long to wait for the user; they may stay in the picker for a while.
const PICK_TIMEOUT: Duration = Duration::from_secs(600);

/// Activity 与 JVM 的全局引用。
/// Global references to the Activity and the JVM.
struct SafContext {
    vm: JavaVM,
    activity: GlobalRef,
}

/// 运行期安装一次的上下文。
/// Context installed once at runtime.
static CONTEXT: OnceLock<SafContext> = OnceLock::new();

/// 等待中的结果：`(请求码, uri, 显示名)`；uri 为 `None` 表示用户取消。
/// Pending result: `(request code, uri, display name)`; `uri == None` means the user cancelled.
static RESULTS: Mutex<Vec<(jint, Option<String>, Option<String>)>> = Mutex::new(Vec::new());
/// 结果到达通知。
/// Notification that a result arrived.
static RESULT_READY: Condvar = Condvar::new();

/// 所属模块：platform / 安卓后端 / SAF（诊断）
/// 功能：把一行文本写入 Android 日志（tag=`SurfingStudioSaf`），用于定位 SAF 流程的卡点。
/// 输入：`text` 日志文本。
/// 输出/副作用：写 logcat；不修改应用状态。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: platform / Android backend / SAF (diagnostics)
/// Purpose: Write one line to logcat (tag `SurfingStudioSaf`) to locate SAF flow stalls.
/// Inputs: `text`: log text.
/// Outputs / side effects: writes logcat; no app state change.
/// Complexity: O(text length) time and O(1) space.
fn trace(text: &str) {
    use std::ffi::CString;
    extern "C" {
        fn __android_log_write(prio: i32, tag: *const u8, text: *const u8) -> i32;
    }
    let Ok(text) = CString::new(text) else {
        return;
    };
    let tag = b"SurfingStudioSaf\0";
    // 安全性：字符串以 NUL 结尾且指针在调用期间有效。
    // SAFETY: both strings are NUL-terminated and outlive the call.
    unsafe {
        __android_log_write(4, tag.as_ptr(), text.as_ptr() as *const u8);
    }
}

/// 所属模块：platform / 安卓后端 / SAF（诊断）
/// 功能：清除挂起的 Java 异常并写入日志；jni-rs 只把异常变成 `Err`，不清除会让下一次 JNI
///   调用（或线程 detach）直接 abort 进程。
/// 输入：`env`：JNI 环境。
/// 输出/副作用：异常描述写入 logcat 并清除异常；无异常时什么也不做。
/// 复杂度：时间 O(异常栈)，空间 O(1)。
///
/// Module: platform / Android backend / SAF (diagnostics)
/// Purpose: Clear a pending Java exception and log it; jni-rs turns it into `Err` but leaves it
///   pending, which aborts the process on the next JNI call (or on thread detach).
/// Inputs: `env`: JNI environment.
/// Outputs / side effects: writes the exception to logcat and clears it; no-op without one.
/// Complexity: O(stack) time and O(1) space.
fn clear_exception(env: &mut JNIEnv) {
    if !env.exception_check().unwrap_or(false) {
        return;
    }
    let _ = env.exception_describe();
    let _ = env.exception_clear();
    trace("java exception cleared");
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：记录 JavaVM 与 Activity 指针；重复调用只保留第一次。
/// 输入：
/// - `vm`：`AndroidApp::vm_as_ptr()` 返回的 JVM 指针。
/// - `activity`：`AndroidApp::activity_as_ptr()` 返回的 Activity 指针。
/// 输出/副作用：写入进程级上下文；失败时静默（SAF 不可用，调用方回退）。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Store the JavaVM and Activity pointers; later calls are ignored.
/// Inputs:
/// - `vm`: JVM pointer from `AndroidApp::vm_as_ptr()`.
/// - `activity`: Activity pointer from `AndroidApp::activity_as_ptr()`.
/// Outputs / side effects: fills the process-wide context; failures are silent (SAF is then
/// unavailable and callers fall back).
/// Complexity: O(1) time and space.
pub fn install(vm: *mut std::ffi::c_void, activity: *mut std::ffi::c_void) {
    if vm.is_null() || activity.is_null() || CONTEXT.get().is_some() {
        return;
    }
    // 安全性：指针来自 android-activity，进程内始终有效。
    // SAFETY: the pointers come from android-activity and stay valid for the process lifetime.
    let vm = match unsafe { JavaVM::from_raw(vm as *mut jni::sys::JavaVM) } {
        Ok(vm) => vm,
        Err(_) => return,
    };
    // 全局引用必须在同一个 JNI 环境里创建；离开作用域后再移动 `vm`。
    // The global reference must be created in the same JNI environment; `vm` is moved afterwards.
    let activity = {
        let env = match vm.attach_current_thread() {
            Ok(env) => env,
            Err(_) => return,
        };
        // 安全性：Activity 指针在进程内始终有效。
        // SAFETY: the Activity pointer is valid for the process lifetime.
        let activity = unsafe { JObject::from_raw(activity as jni::sys::jobject) };
        match env.new_global_ref(activity) {
            Ok(activity) => activity,
            Err(_) => return,
        }
    };
    let _ = CONTEXT.set(SafContext { vm, activity });
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：发起“打开文档”并阻塞等待用户选择（必须在工作线程调用）。
/// 输入：
/// - `mime`：MIME 过滤；`*/*` 表示全部。
/// 输出/副作用：打开系统文件选择器；返回 `(uri, 显示名)`，取消或超时返回 `None`。
/// 复杂度：时间取决于用户操作，空间 O(路径长度)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Start ACTION_OPEN_DOCUMENT and block until the user picks (worker thread only).
/// Inputs:
/// - `mime`: MIME filter; `*/*` accepts everything.
/// Outputs / side effects: opens the system picker; returns `(uri, display name)`, or `None` on
/// cancel/timeout.
/// Complexity: user-bound time, O(path length) space.
pub fn pick_document(mime: &str) -> Option<(String, Option<String>)> {
    request_document(REQUEST_OPEN_DOCUMENT, mime, None)
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：发起“新建文档”并阻塞等待用户确认（必须在工作线程调用）。
/// 输入：
/// - `mime`：新文件的 MIME。
/// - `title`：默认文件名。
/// 输出/副作用：打开系统“保存到”选择器；返回 `(uri, 显示名)`，取消或超时返回 `None`。
/// 复杂度：时间取决于用户操作，空间 O(路径长度)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Start ACTION_CREATE_DOCUMENT and block until the user confirms (worker thread only).
/// Inputs:
/// - `mime`: MIME of the new file.
/// - `title`: suggested file name.
/// Outputs / side effects: opens the system "save to" picker; returns `(uri, display name)`, or
/// `None` on cancel/timeout.
/// Complexity: user-bound time, O(path length) space.
pub fn create_document(mime: &str, title: &str) -> Option<(String, Option<String>)> {
    request_document(REQUEST_CREATE_DOCUMENT, mime, Some(title))
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：构造 Intent、调用 `startActivityForResult`，然后在当前线程等待结果。
/// 输入：
/// - `request_code`：请求码。
/// - `mime`：MIME 过滤。
/// - `title`：新建文档的默认文件名；`None` 表示打开文档。
/// 输出/副作用：启动系统界面并在当前线程阻塞等待；返回选择结果。
/// 复杂度：时间取决于用户操作，空间 O(路径长度)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Build the Intent, call `startActivityForResult` and wait for the result on this thread.
/// Inputs:
/// - `request_code`: request code.
/// - `mime`: MIME filter.
/// - `title`: suggested name for create-document; `None` for open-document.
/// Outputs / side effects: launches the system UI and blocks this thread; returns the selection.
/// Complexity: user-bound time, O(path length) space.
fn request_document(
    request_code: jint,
    mime: &str,
    title: Option<&str>,
) -> Option<(String, Option<String>)> {
    let context = CONTEXT.get()?;
    let action = match request_code {
        REQUEST_CREATE_DOCUMENT => "android.intent.action.CREATE_DOCUMENT",
        _ => "android.intent.action.OPEN_DOCUMENT",
    };
    {
        let mut env = context.vm.attach_current_thread().ok()?;
        let built = build_intent(&mut env, action, mime, title);
        let started = match built {
            Some(intent) => env
                .call_method(
                    &context.activity,
                    "startActivityForResult",
                    "(Landroid/content/Intent;I)V",
                    &[JValue::Object(&intent), JValue::Int(request_code)],
                )
                .is_ok(),
            None => false,
        };
        if !started {
            clear_exception(&mut env);
        }
        trace(&format!("request {request_code} started={started}"));
        if !started {
            return None;
        }
    }
    wait_result(request_code)
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：构造 SAF Intent：动作、类别、MIME 与可选默认文件名。
/// 输入：
/// - `env`：JNI 环境。
/// - `action`：Intent 动作。
/// - `mime`：MIME 过滤。
/// - `title`：新建文档的默认文件名。
/// 输出/副作用：返回 Intent 对象；失败时返回 `None`。
/// 复杂度：时间 O(1)，空间 O(MIME/文件名长度)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Build the SAF Intent: action, category, MIME and the optional suggested file name.
/// Inputs:
/// - `env`: JNI environment.
/// - `action`: Intent action.
/// - `mime`: MIME filter.
/// - `title`: suggested name for create-document.
/// Outputs / side effects: the Intent object, or `None` on failure.
/// Complexity: O(1) time and O(MIME/name length) space.
fn build_intent<'local>(
    env: &mut JNIEnv<'local>,
    action: &str,
    mime: &str,
    title: Option<&str>,
) -> Option<JObject<'local>> {
    let intent = env.new_object("android/content/Intent", "()V", &[]).ok()?;
    let action = env.new_string(action).ok()?;
    env.call_method(
        &intent,
        "setAction",
        "(Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&action)],
    )
    .ok()?;
    let category = env.new_string("android.intent.category.OPENABLE").ok()?;
    env.call_method(
        &intent,
        "addCategory",
        "(Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&category)],
    )
    .ok()?;
    let mime = env.new_string(mime).ok()?;
    env.call_method(
        &intent,
        "setType",
        "(Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&mime)],
    )
    .ok()?;
    if let Some(title) = title {
        let extra_key = env.new_string("android.intent.extra.TITLE").ok()?;
        let extra_value = env.new_string(title).ok()?;
        env.call_method(
            &intent,
            "putExtra",
            "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",
            &[JValue::Object(&extra_key), JValue::Object(&extra_value)],
        )
        .ok()?;
    }
    Some(intent)
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：阻塞等待指定请求码的结果；超时返回 `None`。
/// 输入：`request_code` 请求码。
/// 输出/副作用：消费队列中的结果；等待期间释放结果锁。
/// 复杂度：时间 O(等待时长)，空间 O(1)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Block until the result of `request_code` arrives; `None` on timeout.
/// Inputs: `request_code`: request code.
/// Outputs / side effects: consumes the queued result; the result lock is released while waiting.
/// Complexity: O(wait) time and O(1) space.
fn wait_result(request_code: jint) -> Option<(String, Option<String>)> {
    let deadline = std::time::Instant::now() + PICK_TIMEOUT;
    let mut results = RESULTS.lock().unwrap_or_else(|poison| poison.into_inner());
    loop {
        if let Some(index) = results
            .iter()
            .position(|(code, _, _)| *code == request_code)
        {
            let (_, uri, name) = results.remove(index);
            trace(&format!("result {request_code} uri={:?}", uri.is_some()));
            return uri.map(|uri| (uri, name));
        }
        let now = std::time::Instant::now();
        if now >= deadline {
            return None;
        }
        let (guard, timeout) = RESULT_READY
            .wait_timeout(results, deadline - now)
            .unwrap_or_else(|poison| poison.into_inner());
        results = guard;
        if timeout.timed_out() && results.is_empty() {
            return None;
        }
    }
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：把 Java 结果转换成 Rust 字符串并放进队列，唤醒等待线程。
/// 输入：
/// - `_env`：JNI 环境（读取字符串时使用）。
/// - `_class`：调用方类（静态方法）。
/// - `request_code`：请求码。
/// - `result_code`：`Activity.RESULT_OK` 为 -1，其余视为取消。
/// - `uri`：所选文档 URI，可能为 null。
/// - `name`：文档显示名，可能为 null。
/// 输出/副作用：写入结果队列并通知等待者；不返回值。
/// 复杂度：时间 O(字符串长度)，空间 O(字符串长度)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Convert the Java result to Rust strings, queue it and wake the waiting thread.
/// Inputs:
/// - `_env`: JNI environment (used to read the strings).
/// - `_class`: caller class (static method).
/// - `request_code`: request code.
/// - `result_code`: -1 (`RESULT_OK`) means success; anything else is a cancel.
/// - `uri`: picked document URI, may be null.
/// - `name`: document display name, may be null.
/// Outputs / side effects: fills the result queue and notifies waiters; returns nothing.
/// Complexity: O(string length) time and space.
#[no_mangle]
pub extern "system" fn Java_com_surfingstudio_app_MainActivity_nativeOnActivityResult(
    mut env: JNIEnv,
    _class: jni::objects::JClass,
    request_code: jint,
    result_code: jint,
    uri: JString,
    name: JString,
) {
    // `Activity.RESULT_OK == -1`；其余（取消/失败）都按取消处理。
    // `Activity.RESULT_OK == -1`; everything else (cancel/failure) is treated as cancelled.
    trace("callback enter");
    let uri = if result_code == -1 {
        java_string(&mut env, &uri)
    } else {
        None
    };
    let name = java_string(&mut env, &name);
    trace("callback strings ok");
    let mut results = RESULTS.lock().unwrap_or_else(|poison| poison.into_inner());
    results.push((request_code, uri, name));
    drop(results);
    RESULT_READY.notify_all();
    trace("callback queued");
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：把 Java 字符串转换为 Rust 字符串；null 返回 `None`。
/// 输入：
/// - `env`：JNI 环境。
/// - `value`：Java 字符串对象。
/// 输出/副作用：返回 UTF-8 文本；不修改外部状态。
/// 复杂度：时间 O(字符串长度)，空间 O(字符串长度)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Convert a Java string to Rust; null yields `None`.
/// Inputs:
/// - `env`: JNI environment.
/// - `value`: Java string object.
/// Outputs / side effects: UTF-8 text; no external state change.
/// Complexity: O(string length) time and space.
fn java_string(env: &mut JNIEnv, value: &JString) -> Option<String> {
    if value.is_null() {
        return None;
    }
    env.get_string(value).ok().map(|text| text.into())
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：把 SAF 文档复制到本地文件（导入）。
/// 输入：
/// - `uri`：`content://` URI。
/// - `target`：本地目标路径；父目录会自动创建。
/// 输出/副作用：写目标文件；失败时返回中文原因。
/// 复杂度：时间 O(文件大小)，空间 O(缓冲区)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Copy a SAF document into a local file (import).
/// Inputs:
/// - `uri`: `content://` URI.
/// - `target`: local target path; parent directories are created.
/// Outputs / side effects: writes the target file; returns a Chinese error on failure.
/// Complexity: O(file size) time and O(buffer) space.
pub fn copy_from_uri(uri: &str, target: &Path) -> Result<(), String> {
    trace("copy_from start");
    let mut source = open_fd(uri, "r")?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建目录失败: {error}"))?;
    }
    let mut file =
        std::fs::File::create(target).map_err(|error| format!("创建文件失败: {error}"))?;
    std::io::copy(&mut source, &mut file).map_err(|error| format!("复制文件失败: {error}"))?;
    trace("copy_from done");
    Ok(())
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：把本地文件写入 SAF 文档（导出）。
/// 输入：
/// - `uri`：`content://` URI。
/// - `source`：本地源文件。
/// 输出/副作用：覆盖写入目标文档；失败时返回中文原因。
/// 复杂度：时间 O(文件大小)，空间 O(缓冲区)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Write a local file into a SAF document (export).
/// Inputs:
/// - `uri`: `content://` URI.
/// - `source`: local source file.
/// Outputs / side effects: overwrites the target document; returns a Chinese error on failure.
/// Complexity: O(file size) time and O(buffer) space.
pub fn copy_to_uri(uri: &str, source: &Path) -> Result<(), String> {
    trace("copy_to start");
    let mut input =
        std::fs::File::open(source).map_err(|error| format!("打开源文件失败: {error}"))?;
    let mut output = open_fd(uri, "w")?;
    trace("copy_to fd ok");
    std::io::copy(&mut input, &mut output).map_err(|error| format!("写入文件失败: {error}"))?;
    trace("copy_to done");
    Ok(())
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：打开 SAF 文档的文件描述符并转成 `File`（`detachFd` 后由 Rust 负责关闭）。
/// 输入：
/// - `uri`：`content://` URI。
/// - `mode`：`"r"` 或 `"w"`。
/// 输出/副作用：返回可读/可写的文件句柄；失败时返回中文原因，并清除挂起的 Java 异常。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: platform / Android backend / SAF
/// Purpose: Open a SAF document descriptor and turn it into a `File` (`detachFd` hands ownership
/// to Rust).
/// Inputs:
/// - `uri`: `content://` URI.
/// - `mode`: `"r"` or `"w"`.
/// Outputs / side effects: a readable/writable handle; returns a Chinese error on failure and
/// clears any pending Java exception.
/// Complexity: O(1) time and space.
fn open_fd(uri: &str, mode: &str) -> Result<std::fs::File, String> {
    use std::os::fd::FromRawFd;
    let context = CONTEXT
        .get()
        .ok_or_else(|| "文件选择器尚未初始化".to_string())?;
    let mut env = context
        .vm
        .attach_current_thread()
        .map_err(|error| format!("连接 JVM 失败: {error}"))?;
    let fd = open_fd_inner(&mut env, &context.activity, uri, mode);
    if fd.is_err() {
        clear_exception(&mut env);
    }
    let fd = fd.map_err(|error| format!("系统文件接口调用失败: {error}"))?;
    if fd < 0 {
        trace("open_fd null descriptor");
        return Err("无法打开所选文件".into());
    }
    trace(&format!("fd ok mode={mode}"));
    // 安全性：`detachFd` 已把描述符所有权交给调用方，`File` 负责关闭。
    // SAFETY: `detachFd` transfers ownership, so `File` is responsible for closing it.
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

/// 所属模块：platform / 安卓后端 / SAF
/// 功能：`open_fd` 的 JNI 部分：`Uri.parse` → `ContentResolver.openFileDescriptor` → `detachFd`。
/// 输入：
/// - `env`：JNI 环境。
/// - `activity`：Activity 全局引用（取 ContentResolver）。
/// - `uri`：`content://` URI。
/// - `mode`：`"r"` 或 `"w"`。
/// 输出/副作用：返回文件描述符；`ParcelFileDescriptor` 为 null 时返回 -1。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: platform / Android backend / SAF
/// Purpose: The JNI half of `open_fd`: `Uri.parse` → `ContentResolver.openFileDescriptor` →
/// `detachFd`.
/// Inputs:
/// - `env`: JNI environment.
/// - `activity`: Activity global reference (to obtain the ContentResolver).
/// - `uri`: `content://` URI.
/// - `mode`: `"r"` or `"w"`.
/// Outputs / side effects: the raw file descriptor; returns -1 when the descriptor is null.
/// Complexity: O(1) time and space.
fn open_fd_inner(
    env: &mut JNIEnv,
    activity: &GlobalRef,
    uri: &str,
    mode: &str,
) -> Result<i32, jni::errors::Error> {
    let uri_string = env.new_string(uri)?;
    let uri_object = env
        .call_static_method(
            "android/net/Uri",
            "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&uri_string)],
        )?
        .l()?;
    if uri_object.is_null() {
        return Ok(-1);
    }
    let resolver = env
        .call_method(
            activity,
            "getContentResolver",
            "()Landroid/content/ContentResolver;",
            &[],
        )?
        .l()?;
    let mode = env.new_string(mode)?;
    let descriptor = env
        .call_method(
            &resolver,
            "openFileDescriptor",
            "(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;",
            &[JValue::Object(&uri_object), JValue::Object(&mode)],
        )?
        .l()?;
    if descriptor.is_null() {
        return Ok(-1);
    }
    env.call_method(&descriptor, "detachFd", "()I", &[])?.i()
}
