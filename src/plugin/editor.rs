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
        AdjustWindowRectEx, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW,
        DefWindowProcW, DestroyWindow, GetWindowLongPtrW, IDC_ARROW, IsWindow, LoadCursorW,
        MoveWindow, RegisterClassExW, SC_KEYMENU, SW_HIDE, SW_RESTORE, SW_SHOW, SetFocus,
        SetForegroundWindow, SetWindowLongPtrW, ShowWindow, UpdateWindow, VK_MENU, WM_CLOSE,
        WM_NCCREATE, WM_SIZE, WM_SYSCOMMAND, WM_SYSKEYDOWN, WNDCLASSEXW, WS_CHILD, WS_CLIPCHILDREN,
        WS_CLIPSIBLINGS, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
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
        unsafe {
            if message == WM_NCCREATE {
                let create = lparam as *const CREATESTRUCTW;
                if !create.is_null() {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*create).lpCreateParams as isize);
                }
            }
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState;
            // 该窗口没有宿主菜单；吞掉 Alt 的系统菜单请求，避免 DefWindowProc 进入阻塞菜单循环。
            // The window has no host menu; swallow Alt's system-menu request so DefWindowProc does not
            // enter a blocking menu loop.
            if message == WM_SYSCOMMAND && (wparam & 0xFFF0) == SC_KEYMENU as WPARAM {
                return 0;
            }
            if message == WM_SYSKEYDOWN && wparam == VK_MENU as WPARAM {
                return 0;
            }
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

    /// 所属模块：plugin / 编辑器窗口
    /// 功能：分发本线程已到期的 `WM_TIMER`。
    /// 输入：无。
    /// 输出/副作用：只把 `WM_TIMER` 派发给本线程的窗口（含插件编辑器），其它消息留给 winit。
    /// 复杂度：时间 O(到期定时器数)，空间 O(1)。
    ///
    /// Win32 的 `WM_TIMER` 是低优先级消息：线程在 `MsgWaitForMultipleObjectsEx` 上空闲等待时，
    /// 定时器不会唤醒它，插件的悬停计时（FabFilter/VOCALOID 的二级菜单延迟）就一直不触发——
    /// 现象是菜单必须等到有键盘/鼠标输入把消息循环唤醒才出现。这里每帧主动取出并派发一次。
    ///
    /// Module: plugin / editor windows
    /// Purpose: Dispatch due `WM_TIMER` messages for this thread.
    /// Inputs: none.
    /// Outputs / side effects: dispatches only `WM_TIMER` to this thread's windows (including plugin
    /// editors); other messages stay with winit.
    /// Complexity: O(due timers) time and O(1) space.
    ///
    /// Win32 treats `WM_TIMER` as a low-priority message: an idle `MsgWaitForMultipleObjectsEx`
    /// wait is not woken by it, so plugin hover timers (FabFilter / VOCALOID submenu delays) never
    /// fire until keyboard or mouse input wakes the loop. This drains them once per frame.
    pub fn pump_editor_timers() {
        use winapi::um::winuser::{
            DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage, WM_TIMER,
        };
        let mut message: MSG = unsafe { std::mem::zeroed() };
        // 只取 WM_TIMER：普通输入仍由 winit 处理，避免在帧内重入输入逻辑。
        // Take only WM_TIMER; regular input stays with winit to avoid re-entering input handling
        // in the middle of a frame.
        while unsafe {
            PeekMessageW(
                &mut message,
                std::ptr::null_mut(),
                WM_TIMER,
                WM_TIMER,
                PM_REMOVE,
            )
        } != 0
        {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    /// 所属模块：plugin / 编辑器窗口
    /// 功能：每 200 ms 让插件**自建**的顶层窗口（VOCALOID6 Editor 等）重绘一次，并记录它们的线程 id。
    /// 输入：无。
    /// 输出/副作用：对本进程内非宿主窗口调用 `RedrawWindow`；窗口集合变化时写
    /// `plugin.window.external`（`titles` / `threads`）。
    /// 复杂度：时间 O(本进程窗口数)，空间 O(窗口数)。
    ///
    /// Module: plugin / editor windows
    /// Purpose: Every 200 ms force a repaint of top-level windows **created by plugins** (VOCALOID6
    /// Editor and friends) and record their thread ids.
    /// Inputs: none.
    /// Outputs / side effects: `RedrawWindow` on this process's non-host windows; writes
    /// `plugin.window.external` (`titles` / `threads`) when the set changes.
    /// Complexity: O(process windows) time and space.
    ///
    /// 背景：这类窗口不在宿主创建的编辑器列表里，后台时不会自行刷新——只有输入消息到来才重绘，
    /// 表现为“不点击就完全没反应、也不跟随播放”。强制失效 + 派发本线程定时器可以让它们持续工作。
    /// Background: such windows are not in the host's editor list and do not repaint while in the
    /// background; only an input message wakes them, observed as "no response at all unless clicked,
    /// and the playhead never follows". Forced invalidation plus dispatching this thread's timers
    /// keeps them alive.
    pub fn nudge_external_windows() {
        use std::sync::{Mutex, OnceLock};
        use winapi::shared::minwindef::{BOOL, DWORD, LPARAM, TRUE};
        use winapi::shared::windef::HWND;
        use winapi::um::processthreadsapi::GetCurrentProcessId;
        use winapi::um::winuser::{
            EnumWindows, GW_OWNER, GetWindow, GetWindowTextW, GetWindowThreadProcessId,
            IsWindowVisible, RDW_ALLCHILDREN, RDW_INVALIDATE, RedrawWindow,
        };

        static LAST_RUN: OnceLock<Mutex<Option<std::time::Instant>>> = OnceLock::new();
        static LAST_SIGNATURE: OnceLock<Mutex<String>> = OnceLock::new();
        const INTERVAL_MS: u64 = 200;

        let last_run = LAST_RUN.get_or_init(|| Mutex::new(None));
        {
            let mut last = match last_run.lock() {
                Ok(guard) => guard,
                Err(poison) => poison.into_inner(),
            };
            let now = std::time::Instant::now();
            if let Some(previous) = *last {
                if now.duration_since(previous).as_millis() < INTERVAL_MS as u128 {
                    return;
                }
            }
            *last = Some(now);
        }

        struct State {
            pid: DWORD,
            windows: Vec<(HWND, u32, String)>,
        }

        unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let state = unsafe { &mut *(lparam as *mut State) };
            let mut pid: DWORD = 0;
            let tid = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
            if pid != state.pid || unsafe { IsWindowVisible(hwnd) } == 0 {
                return TRUE;
            }
            let mut buffer = [0u16; 256];
            let length = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
            if length <= 0 {
                return TRUE;
            }
            let title = String::from_utf16_lossy(&buffer[..length as usize]);
            // 跳过宿主自己的窗口（标题统一带 SurfingStudio）与其附属窗口。
            // Skip the host's own windows (titles contain SurfingStudio) and their owned windows.
            if title.contains("SurfingStudio") {
                return TRUE;
            }
            let owner = unsafe { GetWindow(hwnd, GW_OWNER) };
            if !owner.is_null() {
                let mut owner_buffer = [0u16; 256];
                let owner_length = unsafe {
                    GetWindowTextW(owner, owner_buffer.as_mut_ptr(), owner_buffer.len() as i32)
                };
                if owner_length > 0 {
                    let owner_title =
                        String::from_utf16_lossy(&owner_buffer[..owner_length as usize]);
                    if owner_title.contains("SurfingStudio") {
                        return TRUE;
                    }
                }
            }
            unsafe {
                RedrawWindow(
                    hwnd,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    RDW_INVALIDATE | RDW_ALLCHILDREN,
                );
            }
            state.windows.push((hwnd, tid, title));
            TRUE
        }

        let mut state = State {
            pid: unsafe { GetCurrentProcessId() },
            windows: Vec::new(),
        };
        unsafe { EnumWindows(Some(collect), &mut state as *mut _ as LPARAM) };
        if state.windows.is_empty() {
            return;
        }
        let titles = state
            .windows
            .iter()
            .map(|(_, _, title)| title.clone())
            .collect::<Vec<_>>()
            .join(" | ");
        let threads = state
            .windows
            .iter()
            .map(|(_, tid, _)| tid.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let signature = format!("{titles}#{threads}");
        let last_signature = LAST_SIGNATURE.get_or_init(|| Mutex::new(String::new()));
        {
            let mut previous = match last_signature.lock() {
                Ok(guard) => guard,
                Err(poison) => poison.into_inner(),
            };
            if *previous == signature {
                return;
            }
            *previous = signature;
        }
        crate::diag::info(
            "app.plugin",
            "plugin.window.external",
            &[("titles", &titles), ("threads", &threads)],
        );
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

    /// 非 Windows 平台没有原生定时器需要派发。
    /// No native timers to dispatch off Windows.
    pub fn pump_editor_timers() {}

    /// 非 Windows 平台的占位实现。
    /// Placeholder implementation off Windows.
    pub fn nudge_external_windows() {}
}

#[cfg(not(target_os = "windows"))]
pub use fallback::{Vst2EditorWindow, nudge_external_windows, pump_editor_timers};
#[cfg(target_os = "windows")]
pub use windows::{Vst2EditorWindow, nudge_external_windows, pump_editor_timers};
