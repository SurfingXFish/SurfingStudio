//! 所属模块：plugin / 插件编辑器窗口
//! 文件用途：
//! - 为 VST1/VST2 插件提供独立的原生编辑器窗口。
//! - 复用同一个 `Editor` 对象，关闭后可再次打开，避免重复实例化插件。
//!
//! 函数清单：
//! - `Vst2EditorWindow::new` / `new_with_owner`：创建窗口并打开插件编辑器（可指定宿主主窗口所有者）。
//! - `Vst2EditorWindow::service` / `is_visible` / `focus_or_reopen`：每帧维护、可见性与再次打开。
//! - `Vst2EditorWindow::Drop`：关闭编辑器并销毁窗口。
//!
//! Module: plugin / plugin editor windows
//! Purpose:
//! - Provide a standalone native editor window for VST1/VST2 plugins.
//! - Reuse the same `Editor` object so closing and reopening never instantiates
//!   a second plugin instance.
//!
//! Functions:
//! - `Vst2EditorWindow::new` / `new_with_owner`: create the window and open the plugin editor
//!   (optionally owned by the host main window).
//! - `Vst2EditorWindow::service` / `is_visible` / `focus_or_reopen`: per-frame upkeep,
//!   visibility and reopen.
//! - `Vst2EditorWindow::Drop`: close the editor and destroy the window.

use vst::editor::Editor;

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
    use winapi::ctypes::c_void as WinVoid;
    use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
    use winapi::shared::windef::HWND;
    use winapi::um::libloaderapi::GetModuleHandleW;
    use winapi::um::winuser::{
        AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW,
        IsWindow, LoadCursorW, MoveWindow, RegisterClassExW, SetFocus, SetForegroundWindow,
        SetWindowLongPtrW, ShowWindow, UpdateWindow, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW,
        CW_USEDEFAULT, IDC_ARROW, SW_HIDE, SW_RESTORE, SW_SHOW, WM_CLOSE, WM_NCCREATE, WM_SIZE,
        WNDCLASSEXW, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };

    struct WindowState {
        closed: AtomicBool,
        /// 插件编辑器附加到的子窗口，随宿主窗口缩放。
        /// Child window the plugin editor attaches to; follows host resizes.
        container: AtomicPtr<WinVoid>,
    }

    /// 窗口过程只记录关闭请求；真正的 detach + 销毁由 `service` 执行。
    /// The window procedure only records close requests; detach and destruction
    /// happen in `service` so the editor is closed before its parent dies.
    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: UINT,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_NCCREATE {
            let create = lparam as *const CREATESTRUCTW;
            if !create.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*create).lpCreateParams as isize);
            }
        }
        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState;
        if message == WM_CLOSE {
            if !state.is_null() {
                (*state).closed.store(true, Ordering::SeqCst);
            }
            return 0;
        }
        if message == WM_SIZE && !state.is_null() {
            let container = (*state).container.load(Ordering::SeqCst);
            if !container.is_null() {
                let width = (lparam & 0xFFFF) as i32;
                let height = ((lparam >> 16) & 0xFFFF) as i32;
                MoveWindow(container as HWND, 0, 0, width.max(1), height.max(1), 1);
            }
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }

    use winapi::um::winuser::GWLP_USERDATA;

    /// 一个独立的 VST1/VST2 编辑器窗口。
    /// A standalone VST1/VST2 editor window.
    pub struct Vst2EditorWindow {
        hwnd: HWND,
        editor: Option<Box<dyn Editor>>,
        state: Box<WindowState>,
        title: String,
        /// 宿主主窗口句柄；有值时作为窗口所有者，主窗口销毁时编辑器一并销毁。
        /// Host main-window handle; when present the window is owned by it, so destroying the host
        /// destroys the editor as well.
        owner: Option<isize>,
    }

    impl Vst2EditorWindow {
        /// 所属模块：plugin / 编辑器窗口
        /// 功能：创建原生窗口并让插件编辑器附加到其客户区。
        /// 输入：
        /// - `editor`：插件编辑器对象（来自 `PluginInstance::get_editor`）。
        /// - `title`：窗口标题。
        /// 输出/副作用：创建并显示窗口；失败时返回中文错误。
        /// 复杂度：时间 O(1)（插件内部初始化另计），空间 O(1)。
        ///
        /// Module: plugin / editor window
        /// Purpose: Create the native window and attach the plugin editor to its client area.
        /// Inputs:
        /// - `editor`: plugin editor object from `PluginInstance::get_editor`.
        /// - `title`: window title.
        /// Outputs / side effects: creates and shows the window; failures carry Chinese context.
        /// Complexity: O(1) time (plugin initialization excluded) and O(1) space.
        pub fn new(editor: Box<dyn Editor>, title: &str) -> Result<Self, String> {
            Self::new_with_owner(editor, title, None)
        }

        /// 所属模块：plugin / 编辑器窗口
        /// 功能：创建编辑器窗口，并指定宿主主窗口作为所有者。
        /// 输入：
        /// - `editor` / `title`：同 `new`。
        /// - `owner`：宿主主窗口句柄；`None` 时创建独立窗口。
        /// 输出/副作用：创建并显示窗口；失败时返回中文错误。
        /// 复杂度：时间 O(1)（插件内部初始化另计），空间 O(1)。
        ///
        /// Module: plugin / editor window
        /// Purpose: Create the editor window owned by the host main window.
        /// Inputs:
        /// - `editor` / `title`: same as `new`.
        /// - `owner`: host main-window handle; `None` creates an independent window.
        /// Outputs / side effects: creates and shows the window; failures carry Chinese context.
        /// Complexity: O(1) time (plugin initialization excluded) and O(1) space.
        pub fn new_with_owner(
            editor: Box<dyn Editor>,
            title: &str,
            owner: Option<isize>,
        ) -> Result<Self, String> {
            let mut window = Self {
                hwnd: std::ptr::null_mut(),
                editor: Some(editor),
                state: Box::new(WindowState {
                    closed: AtomicBool::new(false),
                    container: AtomicPtr::new(std::ptr::null_mut()),
                }),
                title: title.to_string(),
                owner,
            };
            window.create_window()?;
            Ok(window)
        }

        fn create_window(&mut self) -> Result<(), String> {
            let Some(editor) = &mut self.editor else {
                return Err("插件编辑器已释放".into());
            };
            let (width, height) = editor.size();
            let (client_w, client_h) = if width > 80 && height > 60 {
                (width, height)
            } else {
                (860, 620)
            };
            let title = wide(&self.title);
            let class_name = wide("SurfingStudioVst2Editor");
            unsafe {
                let hinstance = GetModuleHandleW(std::ptr::null());
                let mut class: WNDCLASSEXW = std::mem::zeroed();
                class.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
                class.style = CS_HREDRAW | CS_VREDRAW;
                class.lpfnWndProc = Some(window_proc);
                class.hInstance = hinstance;
                class.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
                class.lpszClassName = class_name.as_ptr();
                // 重复注册同名窗口类是幂等的；错误（已存在）可以忽略。
                // Registering the same class twice is idempotent; ignore the error.
                RegisterClassExW(&class);

                let style = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_CLIPSIBLINGS;
                let mut frame = winapi::shared::windef::RECT {
                    left: 0,
                    top: 0,
                    right: client_w,
                    bottom: client_h,
                };
                AdjustWindowRectEx(&mut frame, style, 0, 0);
                let hwnd = CreateWindowExW(
                    0,
                    class_name.as_ptr(),
                    title.as_ptr(),
                    style | WS_VISIBLE,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    frame.right - frame.left,
                    frame.bottom - frame.top,
                    // 有宿主主窗口时作为所有者：主窗口销毁时系统一并销毁编辑器窗口。
                    // The host main window, when present, owns this window so Windows destroys the
                    // editor together with it.
                    self.owner
                        .map_or(std::ptr::null_mut(), |owner| owner as HWND),
                    std::ptr::null_mut(),
                    hinstance,
                    &*self.state as *const WindowState as *mut WinVoid,
                );
                if hwnd.is_null() {
                    return Err("创建 VST2 编辑器窗口失败".into());
                }
                SetWindowLongPtrW(
                    hwnd,
                    GWLP_USERDATA,
                    &*self.state as *const WindowState as isize,
                );
                // 标准 VST 宿主会创建一个子窗口承载编辑器；VST1 插件通常
                // 只在子父窗口内绘制，直接挂到顶层窗口会显示空白。
                // Standard VST hosts attach the editor to a child window; VST1
                // plugins often only paint inside a child parent.
                let static_class = wide("STATIC");
                let container = CreateWindowExW(
                    0,
                    static_class.as_ptr(),
                    std::ptr::null(),
                    WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN,
                    0,
                    0,
                    client_w,
                    client_h,
                    hwnd,
                    std::ptr::null_mut(),
                    hinstance,
                    std::ptr::null_mut(),
                );
                if container.is_null() {
                    DestroyWindow(hwnd);
                    return Err("创建 VST2 编辑器子窗口失败".into());
                }
                self.state
                    .container
                    .store(container as *mut WinVoid, Ordering::SeqCst);
                if !editor.open(container as *mut std::ffi::c_void) {
                    DestroyWindow(hwnd);
                    return Err("插件没有提供可用的编辑器".into());
                }
                ShowWindow(hwnd, SW_SHOW);
                UpdateWindow(hwnd);
                // 立即激活编辑器窗口：部分插件只在窗口处于激活状态时运行动画。
                // Activate the editor at once: some plugins only animate while active.
                SetForegroundWindow(hwnd);
                SetFocus(container);
                self.hwnd = hwnd;
            }
            Ok(())
        }

        /// 所属模块：plugin / 编辑器窗口
        /// 功能：每帧维护窗口：处理关闭请求并驱动插件 idle。
        /// 输入：`self`。
        /// 输出/副作用：关闭时 detach 编辑器并销毁窗口；可见时调用 `idle`。
        /// 复杂度：时间 O(1)，空间 O(1)。
        ///
        /// Module: plugin / editor window
        /// Purpose: Per-frame upkeep: handle close requests and drive plugin idle.
        /// Inputs: `self`.
        /// Outputs / side effects: detaches and destroys on close; calls `idle` when visible.
        /// Complexity: O(1) time and space.
        pub fn service(&mut self) {
            if self.state.closed.swap(false, Ordering::SeqCst) {
                self.hide();
                return;
            }
            if self.is_visible() {
                if let Some(editor) = &mut self.editor {
                    editor.idle();
                }
            }
        }

        fn hide(&mut self) {
            if self.hwnd.is_null() {
                return;
            }
            if let Some(editor) = &mut self.editor {
                editor.close();
            }
            self.state
                .container
                .store(std::ptr::null_mut(), Ordering::SeqCst);
            unsafe {
                if IsWindow(self.hwnd) != 0 {
                    DestroyWindow(self.hwnd);
                }
            }
            self.hwnd = std::ptr::null_mut();
        }

        /// 所属模块：plugin / 编辑器窗口
        /// 功能：窗口是否正在显示。
        /// 输入：`self`。
        /// 输出/副作用：返回可见性；不修改状态。
        /// 复杂度：时间 O(1)，空间 O(1)。
        ///
        /// Module: plugin / editor window
        /// Purpose: Whether the window is currently visible.
        /// Inputs: `self`.
        /// Outputs / side effects: visibility; no state change.
        /// Complexity: O(1) time and space.
        pub fn is_visible(&self) -> bool {
            !self.hwnd.is_null()
                && !self.state.closed.load(Ordering::SeqCst)
                && unsafe { IsWindow(self.hwnd) != 0 }
        }

        /// 所属模块：plugin / 编辑器窗口
        /// 功能：把窗口置前；已关闭时重新创建窗口并再次打开同一个编辑器。
        /// 输入：`self`。
        /// 输出/副作用：可能创建新窗口并调用 `editor.open`；失败返回中文错误。
        /// 复杂度：时间 O(1)，空间 O(1)。
        ///
        /// Module: plugin / editor window
        /// Purpose: Raise the window; when hidden, recreate it and reopen the same editor.
        /// Inputs: `self`.
        /// Outputs / side effects: may create a window and call `editor.open`; failures
        /// carry Chinese context.
        /// Complexity: O(1) time and space.
        pub fn focus_or_reopen(&mut self) -> Result<(), String> {
            if self.is_visible() {
                unsafe {
                    ShowWindow(self.hwnd, SW_RESTORE);
                    SetForegroundWindow(self.hwnd);
                    let container = self.state.container.load(Ordering::SeqCst);
                    if !container.is_null() {
                        SetFocus(container as HWND);
                    }
                }
                return Ok(());
            }
            self.create_window()
        }

        /// 所属模块：plugin / 编辑器窗口
        /// 功能：退出时直接隐藏并销毁窗口，不调用插件的 `close()`。
        /// 输入：`self`。
        /// 输出/副作用：窗口立即消失；编辑器对象不再关闭（进程即将退出）。
        /// 复杂度：时间 O(1)，空间 O(1)。
        ///
        /// Module: plugin / editor window
        /// Purpose: Hide and destroy the window on shutdown without calling the plugin's
        /// `close()`.
        /// Inputs: `self`.
        /// Outputs / side effects: the window disappears immediately; the editor is not closed
        /// (the process is exiting).
        /// Complexity: O(1) time and space.
        pub fn destroy_for_exit(&mut self) {
            if self.hwnd.is_null() {
                return;
            }
            unsafe {
                if IsWindow(self.hwnd) != 0 {
                    ShowWindow(self.hwnd, SW_HIDE);
                    DestroyWindow(self.hwnd);
                }
            }
            self.hwnd = std::ptr::null_mut();
        }
    }

    impl Drop for Vst2EditorWindow {
        fn drop(&mut self) {
            self.hide();
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
}

#[cfg(not(target_os = "windows"))]
mod fallback {
    use super::*;

    /// 非 Windows 平台的占位实现。
    /// Placeholder implementation for non-Windows platforms.
    pub struct Vst2EditorWindow;

    impl Vst2EditorWindow {
        pub fn new(_editor: Box<dyn Editor>, _title: &str) -> Result<Self, String> {
            Err("VST1/VST2 编辑器窗口目前仅支持 Windows".into())
        }

        pub fn new_with_owner(
            _editor: Box<dyn Editor>,
            _title: &str,
            _owner: Option<isize>,
        ) -> Result<Self, String> {
            Err("VST1/VST2 编辑器窗口目前仅支持 Windows".into())
        }

        pub fn service(&mut self) {}

        pub fn destroy_for_exit(&mut self) {}

        pub fn is_visible(&self) -> bool {
            false
        }

        pub fn focus_or_reopen(&mut self) -> Result<(), String> {
            Err("VST1/VST2 编辑器窗口目前仅支持 Windows".into())
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub use fallback::Vst2EditorWindow;
#[cfg(target_os = "windows")]
pub use windows::Vst2EditorWindow;
