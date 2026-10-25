//! 所属模块：platform / 原生对话框句柄
//! 文件用途：
//! - 持有自绘窗口的原始句柄，让文件对话框在后台线程打开时仍以主窗口为父窗口（模态、置前）。
//! - 在 Windows 上按标题解析本进程的顶层窗口；其他平台返回 `None`。
//!
//! 函数清单：
//! - `DialogParent::window_handle` / `display_handle`：向对话框实现暴露原始句柄。
//! - `parent_by_title`：按窗口标题解析父窗口（Windows）。
//!
//! Module: platform / native dialog handle
//! Purpose:
//! - Own the raw handles of a self-drawn window so a file dialog opened on a worker thread stays
//!   modal to and in front of the main window.
//! - Resolve this process's top-level window by title on Windows; return `None` elsewhere.
//!
//! Functions:
//! - `DialogParent::window_handle` / `display_handle`: expose the raw handles to dialog backends.
//! - `parent_by_title`: resolve the parent window by title (Windows).

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WindowHandle,
};

/// 拥有所有权的原始父窗口句柄。
/// Owned raw parent-window handles.
#[derive(Clone, Copy, Debug)]
pub struct DialogParent {
    pub window: RawWindowHandle,
    pub display: RawDisplayHandle,
}

impl HasWindowHandle for DialogParent {
    /// 所属模块：platform / 原生对话框句柄
    /// 功能：借用主窗口的原始窗口句柄。
    /// 输入：`self`。
    /// 输出/副作用：返回借用句柄；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / native dialog handle
    /// Purpose: Borrow the raw window handle of the main window.
    /// Inputs: `self`.
    /// Outputs / side effects: the borrowed handle; no state change.
    /// Complexity: O(1) time and space.
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // 原始句柄指向主窗口，其生命周期长于对话框线程。
        // The raw handle points at the main window, which outlives the dialog thread.
        Ok(unsafe { WindowHandle::borrow_raw(self.window) })
    }
}

impl HasDisplayHandle for DialogParent {
    /// 所属模块：platform / 原生对话框句柄
    /// 功能：借用主窗口的原始显示句柄。
    /// 输入：`self`。
    /// 输出/副作用：返回借用句柄；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / native dialog handle
    /// Purpose: Borrow the raw display handle of the main window.
    /// Inputs: `self`.
    /// Outputs / side effects: the borrowed handle; no state change.
    /// Complexity: O(1) time and space.
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(unsafe { DisplayHandle::borrow_raw(self.display) })
    }
}

// 原始句柄指向主窗口，其生命周期长于对话框线程。
// The raw handles point at the main window, which outlives the dialog thread.
unsafe impl Send for DialogParent {}

/// 所属模块：platform / 原生对话框句柄
/// 功能：在 Windows 上按标题查找本进程的顶层窗口，作为文件对话框的父窗口。
/// 输入：`title`：窗口标题。
/// 输出/副作用：返回父窗口句柄；找不到或不属于本进程时为 `None`。
/// 复杂度：时间 O(顶层窗口数)，空间 O(标题长度)。
///
/// Module: platform / native dialog handle
/// Purpose: Find a top-level window of this process by title on Windows, to parent a file dialog.
/// Inputs: `title`: window title.
/// Outputs / side effects: the parent handle, or `None` when missing or owned by another process.
/// Complexity: O(top-level windows) time and O(title length) space.
#[cfg(target_os = "windows")]
pub fn parent_by_title(title: &str) -> Option<DialogParent> {
    use std::num::NonZeroIsize;
    use winapi::um::processthreadsapi::GetCurrentProcessId;
    use winapi::um::winuser::{FindWindowW, GetWindowThreadProcessId};

    let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    // 安全性：只按标题查找窗口，并在使用前校验它属于当前进程。
    // SAFETY: look the window up by title and verify it belongs to this process before use.
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), wide.as_ptr());
        if hwnd.is_null() {
            return None;
        }
        let mut process_id = 0_u32;
        GetWindowThreadProcessId(hwnd, &mut process_id);
        if process_id != GetCurrentProcessId() {
            return None;
        }
        let handle = NonZeroIsize::new(hwnd as isize)?;
        Some(DialogParent {
            window: RawWindowHandle::Win32(raw_window_handle::Win32WindowHandle::new(handle)),
            display: RawDisplayHandle::Windows(raw_window_handle::WindowsDisplayHandle::new()),
        })
    }
}

/// 所属模块：platform / 原生对话框句柄
/// 功能：非 Windows 平台没有可用的窗口句柄。
/// 输入：`_title`：窗口标题。
/// 输出/副作用：返回 `None`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: platform / native dialog handle
/// Purpose: No window handles are available off Windows.
/// Inputs: `_title`: window title.
/// Outputs / side effects: returns `None`.
/// Complexity: O(1) time and space.
#[cfg(not(target_os = "windows"))]
pub fn parent_by_title(_title: &str) -> Option<DialogParent> {
    None
}
