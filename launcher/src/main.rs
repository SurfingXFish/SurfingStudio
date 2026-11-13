//! 所属模块：launcher / 瘦启动器
//! 文件用途：
//! - 作为安装包内唯一的可执行入口，只负责定位并加载同目录的 `surfing_studio.dll`。
//! - 调用 DLL 导出的 `surfing_studio_run`，自身不包含任何业务逻辑，充当“车钥匙”。
//! - 加载失败时用系统消息框给出可操作的中文提示，而不是静默退出。
//!
//! 函数清单：
//! - `main`：执行启动流程并以运行时返回码结束进程（不触发 DLL 卸载）。
//! - `exe_dir`：返回启动器所在目录。
//! - `runtime_library_name`：按平台返回运行时 DLL 文件名。
//! - `run`：加载运行时 DLL 并调用其入口。
//! - `show_error`：显示致命错误并返回失败码。
//!
//! Module: launcher / thin launcher
//! Purpose:
//! - The only executable inside the installer; it locates and loads `surfing_studio.dll` from its own
//!   directory.
//! - Calls the exported `surfing_studio_run` and contains no business logic of its own, acting as a
//!   "car key".
//! - Shows an actionable native message box when loading fails instead of exiting silently.
//!
//! Functions:
//! - `main`: run the startup flow and exit with the runtime code (without unloading the DLL).
//! - `exe_dir`: return the directory containing this launcher.
//! - `runtime_library_name`: return the platform-specific runtime DLL name.
//! - `run`: load the runtime DLL and call its entry.
//! - `show_error`: show a fatal error and return a failure code.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::path::PathBuf;

/// 运行时 DLL 导出的入口符号。
/// Entry symbol exported by the runtime DLL.
const RUNTIME_ENTRY: &[u8] = b"surfing_studio_run\0";

/// 所属模块：launcher / 瘦启动器
/// 功能：执行启动流程并以运行时返回码结束进程。
/// 输入：无。
/// 输出/副作用：加载运行时 DLL、创建窗口；结束时直接终止进程，跳过 DLL 卸载。
/// 复杂度：时间 O(启动)，空间 O(运行时镜像)。
///
/// Module: launcher / thin launcher
/// Purpose: Run the startup flow and exit with the runtime code.
/// Inputs: none.
/// Outputs / side effects: loads the runtime DLL and creates windows; terminates directly at the end
/// so the DLL is never unloaded.
/// Complexity: O(startup) time and O(runtime image) space.
fn main() {
    // 直接结束进程而不正常返回：插件 DLL 的 DllMain(DETACH) 可能在退出时等待 UI 线程。
    // End the process directly instead of returning: plugin DLL DllMain(DETACH) may wait for the UI
    // thread during shutdown.
    std::process::exit(run());
}

/// 所属模块：launcher / 瘦启动器
/// 功能：加载运行时 DLL 并调用其入口。
/// 输入：无。
/// 输出/副作用：返回运行时退出码；加载失败时弹窗并返回 1。
/// 复杂度：时间 O(加载 DLL)，空间 O(路径长度)。
///
/// Module: launcher / thin launcher
/// Purpose: Load the runtime DLL and call its entry.
/// Inputs: none.
/// Outputs / side effects: the runtime exit code; shows a message box and returns 1 on failure.
/// Complexity: O(DLL load) time and O(path length) space.
fn run() -> i32 {
    let Some(directory) = exe_dir() else {
        return show_error("无法确定程序所在目录，请重新安装 SurfingStudio。");
    };
    let path = directory.join(runtime_library_name());
    if !path.is_file() {
        return show_error(&format!(
            "缺少运行时组件：{}\n\n请重新安装 SurfingStudio，或将安装包完整解压到同一目录。",
            path.display()
        ));
    }
    // SAFETY: 库在进程剩余生命周期内保持加载；符号签名与运行时 DLL 的导出一致。
    // SAFETY: the library stays loaded for the rest of the process and the symbol signature matches
    // the runtime DLL export.
    unsafe {
        let library = match libloading::Library::new(&path) {
            Ok(library) => library,
            Err(error) => {
                return show_error(&format!(
                    "无法加载运行时组件：{}\n\n{error}\n\n请检查杀毒软件隔离区或重新安装。",
                    path.display()
                ));
            }
        };
        let entry: libloading::Symbol<'_, unsafe extern "C" fn() -> i32> =
            match library.get(RUNTIME_ENTRY) {
                Ok(entry) => entry,
                Err(error) => {
                    return show_error(&format!(
                        "运行时组件缺少入口函数 `surfing_studio_run`：\n{error}\n\n请重新安装。"
                    ));
                }
            };
        let code = entry();
        drop(entry);
        // 运行时正常退出时已经终止进程；这里故意不卸载 DLL，避免插件析构阻塞退出。
        // The runtime normally terminates the process already; deliberately keep the DLL loaded so
        // plugin teardown cannot block exit.
        std::mem::forget(library);
        code
    }
}

/// 所属模块：launcher / 瘦启动器
/// 功能：返回启动器所在目录；被移动或路径异常时返回 `None`。
/// 输入：无。
/// 输出/副作用：目录路径；不创建目录。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: launcher / thin launcher
/// Purpose: Return the launcher directory; `None` when the path is unavailable.
/// Inputs: none.
/// Outputs / side effects: the directory path; nothing is created.
/// Complexity: O(path length) time and space.
fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(PathBuf::from)
}

/// 所属模块：launcher / 瘦启动器
/// 功能：返回当前平台的运行时 DLL 文件名。
/// 输入：无。
/// 输出/副作用：静态文件名；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: launcher / thin launcher
/// Purpose: Return the runtime DLL file name for this platform.
/// Inputs: none.
/// Outputs / side effects: static file name; no state change.
/// Complexity: O(1) time and space.
fn runtime_library_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "surfing_studio.dll"
    }
    #[cfg(target_os = "macos")]
    {
        "libsurfing_studio.dylib"
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        "libsurfing_studio.so"
    }
}

/// 所属模块：launcher / 瘦启动器
/// 功能：显示致命错误提示并返回失败码。
/// 输入：`message` 中文错误文本。
/// 输出/副作用：Windows 弹出系统消息框，其他平台写入标准错误；不修改状态。
/// 复杂度：时间 O(消息长度)，空间 O(消息长度)。
///
/// Module: launcher / thin launcher
/// Purpose: Show a fatal error and return a failure code.
/// Inputs: `message`: Chinese error text.
/// Outputs / side effects: a native message box on Windows, standard error elsewhere.
/// Complexity: O(message length) time and space.
fn show_error(message: &str) -> i32 {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        let title: Vec<u16> = std::ffi::OsStr::new("SurfingStudio")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let text: Vec<u16> = std::ffi::OsStr::new(message)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: 两个缓冲区都以 NUL 结尾，句柄为 0 表示无父窗口，MB_OK 不读取返回值。
        // SAFETY: both buffers are NUL-terminated, a null owner means no parent and MB_OK ignores
        // the return value.
        unsafe {
            winapi::um::winuser::MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                winapi::um::winuser::MB_OK | winapi::um::winuser::MB_ICONERROR,
            );
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        eprintln!("{message}");
    }
    1
}
