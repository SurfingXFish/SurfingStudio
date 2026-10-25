//! 所属模块：app / 全局快捷键
//! 文件用途：
//! - 在应用 UI 线程安装线程级消息钩子，观察独立窗口（插件编辑器、浮动卷帘）中的空格键。
//! - 把捕获到的空格按下交给当前工程窗口的 `handle_keyboard`。
//! 函数清单：
//! - `install`：在 UI 线程安装一次 `WH_GETMESSAGE` 钩子。
//! - `register_window`：登记本进程的应用窗口，避免重复处理它们自己的空格。
//! - `take_space_toggle`：取走本次待处理的空格切换请求（读取后清空）。
//! - `get_message_hook`：线程消息钩子，只观察应用窗口之外的空格按下。
//!
//! Module: app / global shortcuts
//! Purpose:
//! - Install a thread-level message hook on the UI thread so Space is observed in
//!   standalone windows (plugin editors, floating rolls).
//! - Hand the captured Space press to the current project window's `handle_keyboard`.
//! Functions:
//! - `install`: install one `WH_GETMESSAGE` hook on the UI thread.
//! - `register_window`: remember an app window so its own Space is not handled twice.
//! - `take_space_toggle`: take (and clear) the pending Space toggle request.
//! - `get_message_hook`: thread message hook that only observes Space outside app windows.

#[cfg(target_os = "windows")]
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "windows")]
use std::sync::Mutex;

/// 本次帧内是否收到独立窗口的空格按下。
/// Whether a Space press in a standalone window arrived for the current frame.
static SPACE_REQUESTED: AtomicBool = AtomicBool::new(false);
/// 本进程的应用窗口句柄（Windows `HWND` 的数值形式）；这些窗口的空格仍交给 egui 处理。
/// App window handles of this process (numeric Windows `HWND`); Space there stays with egui.
#[cfg(target_os = "windows")]
static APP_WINDOWS: Mutex<Vec<isize>> = Mutex::new(Vec::new());
/// 已安装的消息钩子；`0` 表示尚未安装或安装失败。
/// Installed message hook; `0` means not installed or installation failed.
#[cfg(target_os = "windows")]
static MESSAGE_HOOK: AtomicIsize = AtomicIsize::new(0);

#[cfg(target_os = "windows")]
use winapi::shared::minwindef::{LPARAM, LRESULT, WPARAM};
#[cfg(target_os = "windows")]
use winapi::um::processthreadsapi::GetCurrentThreadId;
#[cfg(target_os = "windows")]
use winapi::um::winuser::{
    CallNextHookEx, SetWindowsHookExW, MSG, PM_REMOVE, VK_SPACE, WH_GETMESSAGE, WM_KEYDOWN,
};

/// 所属模块：app / 全局快捷键
/// 功能：在 UI 线程安装一次线程级消息钩子。
/// 输入：无。
/// 输出/副作用：安装 `WH_GETMESSAGE` 钩子（整个进程生命周期只安装一次）；非 Windows 平台为空操作。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global shortcuts
/// Purpose: Install one thread-level message hook on the UI thread.
/// Inputs: none.
/// Outputs / side effects: installs a `WH_GETMESSAGE` hook once per process; no-op off Windows.
/// Complexity: O(1) time and space.
pub(super) fn install() {
    #[cfg(target_os = "windows")]
    {
        if MESSAGE_HOOK.load(Ordering::Relaxed) != 0 {
            return;
        }
        // SAFETY: 线程级钩子不需要 DLL 注入；钩子只读取消息内容。
        // SAFETY: a thread-level hook needs no DLL injection; the hook only reads the message.
        unsafe {
            let hook = SetWindowsHookExW(
                WH_GETMESSAGE,
                Some(get_message_hook),
                std::ptr::null_mut(),
                GetCurrentThreadId(),
            );
            if !hook.is_null() {
                MESSAGE_HOOK.store(hook as isize, Ordering::Relaxed);
            }
        }
    }
}

/// 所属模块：app / 全局快捷键
/// 功能：登记一个应用窗口句柄，让钩子忽略该窗口自己的空格消息。
/// 输入：`hwnd`：窗口原生句柄；未知时传 `None`。
/// 输出/副作用：把句柄加入全局列表；非 Windows 平台为空操作。
/// 复杂度：时间 O(窗口数)，空间 O(窗口数)。
///
/// Module: app / global shortcuts
/// Purpose: Register an app window so the hook ignores Space messages sent to it.
/// Inputs: `hwnd`: native handle, or `None` when unavailable.
/// Outputs / side effects: adds the handle to the global list; no-op off Windows.
/// Complexity: O(windows) time and space.
pub(super) fn register_window(hwnd: Option<isize>) {
    #[cfg(target_os = "windows")]
    {
        let Some(hwnd) = hwnd else {
            return;
        };
        let mut windows = APP_WINDOWS
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if !windows.contains(&hwnd) {
            windows.push(hwnd);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = hwnd;
    }
}

/// 所属模块：app / 全局快捷键
/// 功能：取走待处理的空格切换请求。
/// 输入：无。
/// 输出/副作用：返回是否有请求，并清空标记；不修改其他状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global shortcuts
/// Purpose: Take the pending Space toggle request.
/// Inputs: none.
/// Outputs / side effects: whether a request was pending; clears the flag.
/// Complexity: O(1) time and space.
pub(super) fn take_space_toggle() -> bool {
    SPACE_REQUESTED.swap(false, Ordering::Relaxed)
}

/// 所属模块：app / 全局快捷键
/// 功能：观察 UI 线程消息，记录应用窗口之外的空格按下（忽略长按重复）。
/// 输入：
/// - `code`：钩子代码，小于 `0` 时必须直接转发。
/// - `wparam`：`PM_REMOVE` / `PM_NOREMOVE`。
/// - `lparam`：指向 `MSG` 的指针。
/// 输出/副作用：设置 `SPACE_REQUESTED`；消息本身始终放行，插件仍能收到按键。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global shortcuts
/// Purpose: Observe UI-thread messages and record Space presses outside the app windows
/// (auto-repeat ignored).
/// Inputs:
/// - `code`: hook code; must be forwarded directly when negative.
/// - `wparam`: `PM_REMOVE` / `PM_NOREMOVE`.
/// - `lparam`: pointer to the `MSG`.
/// Outputs / side effects: sets `SPACE_REQUESTED`; the message always passes through, so
/// plugins still receive the key.
/// Complexity: O(1) time and space.
#[cfg(target_os = "windows")]
unsafe extern "system" fn get_message_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam == PM_REMOVE as WPARAM {
        let message = lparam as *const MSG;
        if !message.is_null()
            && (*message).message == WM_KEYDOWN
            && (*message).wParam == VK_SPACE as WPARAM
            // bit 30 表示按键此前已按下，用它过滤长按自动重复。
            // Bit 30 marks a key that was already down; use it to filter auto-repeat.
            && ((*message).lParam & (1 << 30)) == 0
        {
            // 应用窗口自己的空格由 egui 处理，钩子只在插件编辑器/浮动卷帘等窗口上生效。
            // The app's own windows handle Space through egui; the hook only acts on plugin
            // editors, floating rolls and other standalone windows.
            let windows = APP_WINDOWS
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if !windows.contains(&((*message).hwnd as isize)) {
                SPACE_REQUESTED.store(true, Ordering::Relaxed);
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}
