//! Plugin window management
//!
//! This module provides platform-specific window creation and management
//! for VST3 plugin GUIs.

use crate::error::{Error, Result};
use crate::plugin::Plugin;
use std::sync::{Arc, Mutex};

#[cfg(target_os = "macos")]
use objc2::{rc::Retained, MainThreadMarker, MainThreadOnly};
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSApplication, NSBackingStoreType, NSView, NSWindow, NSWindowStyleMask};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

#[cfg(target_os = "windows")]
use winapi::{
    shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM},
    shared::windef::{HWND, POINT, RECT},
    um::libloaderapi::GetModuleHandleW,
    um::wingdi::CreateRectRgn,
    um::winuser::{
        ClientToScreen, CreateWindowExW, DefWindowProcW, DestroyWindow, GetForegroundWindow,
        IsWindow, LoadCursorW, RedrawWindow, RegisterClassExW, SetFocus, SetForegroundWindow,
        SetWindowPos, SetWindowRgn, ShowWindow, UpdateWindow, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
        IDC_ARROW, RDW_ALLCHILDREN, RDW_INVALIDATE, SIZE_MINIMIZED, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOZORDER, SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE, WM_CLOSE, WM_DPICHANGED, WM_SIZE,
        WNDCLASSEXW, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_TOOLWINDOW, WS_OVERLAPPEDWINDOW,
        WS_POPUP,
    },
};

#[cfg(any(test, target_os = "windows"))]
fn dpi_scale_factor(dpi: u32) -> Option<f32> {
    (dpi > 0).then_some(dpi as f32 / 96.0)
}

/// Editor windows whose window procedure has seen `WM_CLOSE`, keyed by `HWND` (as `usize`,
/// since `HWND` is a raw pointer and not `Send`).
///
/// `DefWindowProcW` answers `WM_CLOSE` by destroying the window, which would leave the plugin's
/// `IPlugView` attached to a freed `HWND`. [`plugin_window_proc`] records the request here
/// instead, so the host can tear the editor down in the right order.
#[cfg(target_os = "windows")]
fn close_requests() -> &'static Mutex<std::collections::HashSet<usize>> {
    static REQUESTS: std::sync::OnceLock<Mutex<std::collections::HashSet<usize>>> =
        std::sync::OnceLock::new();
    REQUESTS.get_or_init(Mutex::default)
}

/// Latest pending DPI for each editor window. The window procedure cannot call into the plugin:
/// `setContentScaleFactor` may synchronously enter arbitrary plugin/UI code, while callers often
/// hold the plugin mutex when Windows dispatches a message. The procedure records only the
/// newest DPI and [`PluginWindow::service_platform_events`] applies it after the callback returns.
#[cfg(target_os = "windows")]
fn dpi_changes() -> &'static Mutex<std::collections::HashMap<usize, u32>> {
    static CHANGES: std::sync::OnceLock<Mutex<std::collections::HashMap<usize, u32>>> =
        std::sync::OnceLock::new();
    CHANGES.get_or_init(Mutex::default)
}

/// 用户拖动窗口边框后待转发给插件的客户区尺寸，按 `HWND` 记录。
/// 窗口过程里不能直接调用插件（插件可能重入宿主消息循环），因此只记录，
/// 由 [`PluginWindow::service_platform_events`] 在消息回调返回后调用
/// `IPlugView::onSize`。
///
/// Client-area sizes produced by the user dragging the window frame, keyed by `HWND`.
/// The window procedure cannot call into the plugin (it may re-enter the host message
/// loop), so it only records the size; [`PluginWindow::service_platform_events`] forwards
/// it to `IPlugView::onSize` after the callback returns.
#[cfg(target_os = "windows")]
fn pending_client_sizes() -> &'static Mutex<std::collections::HashMap<usize, (i32, i32)>> {
    static SIZES: std::sync::OnceLock<Mutex<std::collections::HashMap<usize, (i32, i32)>>> =
        std::sync::OnceLock::new();
    SIZES.get_or_init(Mutex::default)
}

/// 编辑器窗口样式：保留系统边框（可拖动缩放），并阻止父窗口擦除插件子窗口。
/// Editor window style: keep the resizable system frame and stop the parent from
/// painting over the plugin's child window.
#[cfg(target_os = "windows")]
const EDITOR_WINDOW_STYLE: u32 = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_CLIPSIBLINGS;

#[cfg(target_os = "windows")]
#[allow(dead_code)]
fn dpi_from_wparam(wparam: WPARAM) -> Option<u32> {
    // WM_DPICHANGED packs the new X DPI into LOWORD and Y DPI into HIWORD. VST3 has one content
    // scale, and Windows normally reports both equally, so use X as Microsoft recommends.
    let dpi = (wparam & 0xffff) as u32;
    (dpi > 0).then_some(dpi)
}

/// Window procedure for plugin editor windows: record control-plane work and defer plugin calls
/// until after Windows returns from the callback.
///
/// # Safety
///
/// Called by the OS with the arguments of a window procedure; all it does with them is forward
/// them to `DefWindowProcW`.
#[cfg(target_os = "windows")]
unsafe extern "system" fn plugin_window_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_CLOSE {
        if let Ok(mut requests) = close_requests().lock() {
            requests.insert(hwnd as usize);
        }
        return 0;
    }
    if msg == WM_DPICHANGED {
        // SerumStudio patch: the editor window was sized and normalised when it
        // opened (see `PluginWindow::open`). Applying the suggested rectangle
        // here would shrink the client area without telling the plugin, so keep
        // the size the editor was opened with.
        return 0;
    }
    if msg == WM_SIZE && wparam != SIZE_MINIMIZED as WPARAM {
        // 只记录用户缩放后的客户区尺寸，真正的 `onSize` 由宿主每帧服务时转发。
        // Record the client size only; the host forwards it to `onSize` per frame.
        let width = (lparam & 0xffff) as i32;
        let height = ((lparam >> 16) & 0xffff) as i32;
        if width > 0 && height > 0 {
            if let Ok(mut sizes) = pending_client_sizes().lock() {
                sizes.insert(hwnd as usize, (width, height));
            }
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// An X11 window (connection + window id) backing a plugin editor on Linux.
///
/// Ported from the khremeviuc1004 fork's XCB implementation.
#[cfg(target_os = "linux")]
struct XcbWindowState {
    connection: xcb::Connection,
    window: xcb::x::Window,
}

/// A plugin window that manages the native window and plugin editor lifecycle
pub struct PluginWindow {
    plugin: Arc<Mutex<Plugin>>,
    #[cfg(target_os = "macos")]
    native_window: Option<Retained<NSWindow>>,
    /// The view the plugin attached its editor into. Kept so a plugin-initiated resize can
    /// grow the container along with the window.
    #[cfg(target_os = "macos")]
    container_view: Option<Retained<NSView>>,
    #[cfg(target_os = "windows")]
    native_window: Option<HWND>,
    /// 为 `true` 时窗口是无边框的宿主附属窗口（效果器卷帘内嵌模式）。
    /// `true` when the window is a frameless window owned by the host (inline effect-roll mode).
    #[cfg(target_os = "windows")]
    embedded: bool,
    /// 内嵌窗口的宿主主窗口；用于把卡片客户区坐标换算为屏幕坐标。
    /// Host window of an inline editor; used to map card client coordinates to the screen.
    #[cfg(target_os = "windows")]
    embedded_parent: Option<HWND>,
    /// 宿主 DPI 缩放：内嵌内容缩放的上限。
    /// Host DPI scale: the upper bound of the inline content scale.
    #[cfg(target_os = "windows")]
    embedded_host_scale: f32,
    /// 内嵌编辑器当前的内容缩放。
    /// Current content scale of the inline editor.
    #[cfg(target_os = "windows")]
    embedded_scale: f32,
    /// 内嵌编辑器在缩放 1.0 时的逻辑尺寸。
    /// Logical size of the inline editor at scale 1.0.
    #[cfg(target_os = "windows")]
    embedded_base_size: Option<(f32, f32)>,
    /// 内嵌编辑器当前的窗口尺寸（物理像素）。
    /// Current inline window size in physical pixels.
    #[cfg(target_os = "windows")]
    embedded_size: Option<(i32, i32)>,
    /// 内嵌编辑器是否支持内容缩放；不支持时退回 `onSize` 布局。
    /// Whether the inline editor supports content scaling; otherwise fall back to `onSize`.
    #[cfg(target_os = "windows")]
    embedded_scalable: bool,
    #[cfg(target_os = "linux")]
    native_window: Option<XcbWindowState>,
    #[cfg(target_os = "android")]
    native_window: Option<()>,
}

impl PluginWindow {
    /// Create a new plugin window for the given plugin
    pub fn new(plugin: Arc<Mutex<Plugin>>) -> Self {
        Self {
            plugin,
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                target_os = "linux",
                target_os = "android"
            ))]
            native_window: None,
            #[cfg(target_os = "windows")]
            embedded: false,
            #[cfg(target_os = "windows")]
            embedded_parent: None,
            #[cfg(target_os = "windows")]
            embedded_host_scale: 1.0,
            #[cfg(target_os = "windows")]
            embedded_scale: 1.0,
            #[cfg(target_os = "windows")]
            embedded_base_size: None,
            #[cfg(target_os = "windows")]
            embedded_size: None,
            #[cfg(target_os = "windows")]
            embedded_scalable: false,
            #[cfg(target_os = "macos")]
            container_view: None,
        }
    }

    /// Open the plugin window
    pub fn open(&mut self) -> Result<()> {
        self.open_with_owner(None)
    }

    /// 所属模块：vst3-host / 编辑器窗口
    /// 功能：打开编辑器窗口，可指定宿主主窗口作为**所有者**。
    /// 输入：`owner` 主窗口句柄；`None` 时创建无所有者的独立窗口。
    /// 输出/副作用：创建并显示原生窗口，把插件视图附加到它；失败时返回错误。
    /// 复杂度：时间 O(插件编辑器初始化)，空间 O(1)。
    ///
    /// Module: vst3-host / editor window
    /// Purpose: Open the editor window, optionally owned by the host main window.
    /// Inputs: `owner` — main window handle; `None` creates an unowned top-level window.
    /// Outputs / side effects: creates and shows the native window and attaches the plugin view;
    /// failures are returned.
    /// Complexity: O(plugin editor initialization) time and O(1) space.
    pub fn open_with_owner(&mut self, owner: Option<*mut std::ffi::c_void>) -> Result<()> {
        #[cfg(not(target_os = "windows"))]
        let _ = owner;
        // Check if plugin has editor
        let has_editor = self
            .plugin
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .has_editor();
        if !has_editor {
            return Err(Error::Other(
                "Plugin does not have a GUI editor".to_string(),
            ));
        }

        // Close existing window if any. Keyed on the handle rather than `is_open()`, which also
        // reports `false` for a window the user already dismissed — that one still needs closing.
        if self.native_window.is_some() {
            self.close();
        }
        #[cfg(target_os = "windows")]
        {
            self.embedded = false;
            self.embedded_parent = None;
            self.embedded_host_scale = 1.0;
            self.embedded_scale = 1.0;
            self.embedded_base_size = None;
            self.embedded_size = None;
            self.embedded_scalable = false;
        }

        // Get plugin info for window title
        let plugin_info = self
            .plugin
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .info()
            .clone();

        // 先用占位尺寸创建窗口，附加后再按插件报告的尺寸调整：`get_editor_size()` 会额外创建
        // 一个临时视图，对 Kontakt 8 这类重型插件要花十余秒，而且它在 attach 前还会拒绝
        // `getSize`，白白让 GUI 打不开。窗口在 `ShowWindow` 之前完成尺寸调整，用户看不到占位大小。
        // Create the window at a placeholder size and resize it after attaching: `get_editor_size()`
        // spawns an extra temporary view, costs ten-plus seconds on heavy plugins such as Kontakt 8,
        // and for those plugins `getSize` fails before attaching anyway — which used to abort the
        // GUI entirely. The window is resized before `ShowWindow`, so the placeholder is invisible.
        let (width, height) = (800, 600);

        // Create native window
        #[cfg(target_os = "macos")]
        {
            // AppKit objects must be created on the main thread.
            let mtm = MainThreadMarker::new().ok_or_else(|| {
                Error::Other("plugin editor window must be opened on the main thread".to_string())
            })?;

            let frame = NSRect::new(
                NSPoint::new(100.0, 100.0),
                NSSize::new(width as f64, height as f64),
            );
            let style = NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Miniaturizable;

            // SAFETY: standard AppKit window/view construction on the main thread.
            let window = unsafe {
                NSWindow::initWithContentRect_styleMask_backing_defer(
                    NSWindow::alloc(mtm),
                    frame,
                    style,
                    NSBackingStoreType::Buffered,
                    false,
                )
            };

            // Programmatic NSWindows default to `releasedWhenClosed = YES`; closing one would
            // then release it while our `Retained<NSWindow>` also releases on drop — a
            // double-free that crashes on close. We own the lifetime, so opt out.
            // SAFETY: standard AppKit setter on the main thread.
            unsafe { window.setReleasedWhenClosed(false) };

            let title = NSString::from_str(&format!("{} - VST3", plugin_info.name));
            window.setTitle(&title);

            // A container view, sized to the editor, that the plugin attaches its view into.
            let container_frame = NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(width as f64, height as f64),
            );
            let container_view = NSView::initWithFrame(NSView::alloc(mtm), container_frame);
            if let Some(content_view) = window.contentView() {
                content_view.addSubview(&container_view);
            }

            // Hand the plugin the container NSView to embed its editor in.
            // SAFETY: `container_view` is a live NSView this `PluginWindow` keeps alive (via
            // the retained window it was added to) for as long as the editor is attached —
            // `close()` detaches the editor before dropping the window.
            let window_handle = unsafe {
                crate::plugin::WindowHandle::from_nsview(
                    Retained::as_ptr(&container_view) as *mut std::ffi::c_void
                )
            };
            self.plugin
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .open_editor(window_handle)?;

            // 附加后再取尺寸：部分插件在 attach 前拒绝 `getSize`。
            // Query the size after attaching: some plugins refuse `getSize` beforehand.
            let editor_size = {
                let plugin = self.plugin.lock().unwrap_or_else(|p| p.into_inner());
                plugin
                    .attached_editor_size()
                    .ok()
                    .flatten()
                    .or_else(|| plugin.declared_editor_size())
            };
            let content_size = match editor_size {
                Some((editor_w, editor_h)) => NSSize::new(editor_w as f64, editor_h as f64),
                None => container_frame.size,
            };

            // Match the window to the editor size, then show and center it.
            window.setContentSize(content_size);
            window.makeKeyAndOrderFront(None);
            window.center();

            self.native_window = Some(window);
            self.container_view = Some(container_view);
        }

        #[cfg(target_os = "windows")]
        {
            unsafe {
                use std::mem;
                use std::ptr;

                // Register window class if not already registered
                let class_name = "VST3PluginWindow\0".encode_utf16().collect::<Vec<u16>>();
                let mut wc: WNDCLASSEXW = mem::zeroed();
                wc.cbSize = mem::size_of::<WNDCLASSEXW>() as UINT;
                wc.style = CS_HREDRAW | CS_VREDRAW;
                wc.lpfnWndProc = Some(plugin_window_proc);
                wc.hInstance = GetModuleHandleW(ptr::null());
                wc.hCursor = LoadCursorW(ptr::null_mut(), IDC_ARROW);
                wc.lpszClassName = class_name.as_ptr();

                // Try to register, ignore if already registered
                RegisterClassExW(&wc);

                // Create window
                let window_title = format!("{} - VST3\0", plugin_info.name);
                let window_name = window_title.encode_utf16().collect::<Vec<u16>>();

                // Calculate window size including borders
                let mut rect = RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                };

                winapi::um::winuser::AdjustWindowRectEx(
                    &mut rect,
                    EDITOR_WINDOW_STYLE,
                    0, // No menu
                    0, // No extended style
                );

                let window_width = rect.right - rect.left;
                let window_height = rect.bottom - rect.top;

                let hwnd = CreateWindowExW(
                    0,
                    class_name.as_ptr(),
                    window_name.as_ptr(),
                    EDITOR_WINDOW_STYLE,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    window_width,
                    window_height,
                    // 有主窗口时作为所有者创建：主窗口销毁时系统会一并销毁编辑器，
                    // 插件即使在宿主失忆（未跟踪窗口）时也不会留在屏幕上。
                    // When a main window exists it becomes the owner: Windows destroys owned
                    // windows together with the owner, so an editor can never be left on screen
                    // even if the host lost track of it.
                    owner.map_or(ptr::null_mut(), |owner| owner as HWND),
                    ptr::null_mut(),
                    GetModuleHandleW(ptr::null()),
                    ptr::null_mut(),
                );

                if hwnd.is_null() {
                    return Err(Error::Other("Failed to create native window".to_string()));
                }

                // Try to open plugin editor.
                // SAFETY: `hwnd` was just created above and null-checked; it is destroyed only
                // after the editor is detached (in `close()`, or the error arm below).
                let window_handle =
                    crate::plugin::WindowHandle::from_hwnd(hwnd as *mut std::ffi::c_void);
                let mut plugin = self.plugin.lock().unwrap_or_else(|p| p.into_inner());
                // Host content scale from the monitor the window landed on.
                let host_scale = match winapi::um::winuser::GetDpiForWindow(hwnd) {
                    dpi if dpi > 0 => dpi as f32 / 96.0,
                    _ => 1.0,
                };
                // Remember the host scale before attaching (a view cannot report
                // support yet); it is applied when the view attaches.
                let _ = plugin.set_editor_scale_factor(host_scale);
                match plugin.open_editor(window_handle) {
                    Ok(()) => {
                        // SerumStudio patch: some editors bake their own GUI zoom into the
                        // attached view. Serum 2, for example, reports 1190x740 before attach
                        // but asks for 4748x2952 once attached (its stored 399 % zoom). When
                        // the editor supports VST3 content scaling, normalise the attached view
                        // back to the size it declared before attach, never enlarging it, and
                        // keep the host DPI scale for everything else.
                        let declared = plugin.declared_editor_size();
                        let attached = plugin.attached_editor_size().ok().flatten();
                        let mut final_size = None;
                        // 插件在 attach 期间通过 `IPlugFrame::resizeView` 请求的窗口尺寸。
                        // Window size the plugin requested through `IPlugFrame::resizeView` while
                        // attaching. Only the normalised flow discards it (it carries Serum 2's
                        // inflated zoom size); otherwise it is the size the plugin wants, e.g.
                        // Kontakt 8's Qt editor asks for its real size during attach.
                        let mut plugin_request = None;
                        // With the view attached this reports whether the plugin
                        // implements IPlugViewContentScaleSupport.
                        let scale_supported =
                            plugin.set_editor_scale_factor(host_scale).unwrap_or(false);
                        if scale_supported {
                            if let (Some((natural_w, natural_h)), Some((declared_w, declared_h))) =
                                (attached, declared)
                            {
                                let natural_w = natural_w as f32;
                                let natural_h = natural_h as f32;
                                let target_w = declared_w as f32 * host_scale;
                                let target_h = declared_h as f32 * host_scale;
                                let ratio = (target_w / natural_w).min(target_h / natural_h);
                                if ratio > 0.0 && ratio < 0.999 {
                                    let normalized = (host_scale * ratio).clamp(0.1, 4.0);
                                    if plugin
                                        .set_editor_scale_factor(normalized)
                                        .unwrap_or(false)
                                    {
                                        if let Ok(Some(size)) = plugin.attached_editor_size() {
                                            final_size = Some(size);
                                        }
                                    }
                                }
                            }
                            if final_size.is_some() {
                                // 归一化后丢弃插件请求的放大尺寸。
                                // Normalisation applied: drop the plugin's inflated request.
                                let _ = plugin.take_editor_resize_request();
                            } else {
                                plugin_request = plugin.take_editor_resize_request();
                            }
                        }
                        drop(plugin);
                        self.native_window = Some(hwnd);
                        // 优先使用调整后的尺寸，其次是插件在 attach 时请求的尺寸，
                        // 再其次是 attach 后报告的尺寸，最后是 attach 前声明的尺寸。
                        // Prefer the normalised size, then the size the plugin requested while
                        // attaching, then the post-attach size, then the pre-attach declared size.
                        if let Some((final_w, final_h)) = final_size
                            .or(plugin_request)
                            .or(attached)
                            .or(declared)
                        {
                            self.resize_native_window(final_w, final_h);
                        }
                        ShowWindow(hwnd, SW_SHOW);
                        UpdateWindow(hwnd);
                        // 让编辑器窗口立即成为前台窗口：部分插件（FabFilter 等）只在
                        // 窗口处于激活状态时运行动画/定时器。
                        // Activate the editor immediately: some plugins (FabFilter and
                        // friends) only run their timers while the window is active.
                        SetForegroundWindow(hwnd);
                        SetFocus(hwnd);
                        // 打开阶段的 `WM_SIZE` 属于宿主自身的布局，不需要再回灌插件。
                        // The `WM_SIZE` emitted while opening is host bookkeeping and must
                        // not be forwarded back to the plugin.
                        if let Ok(mut sizes) = pending_client_sizes().lock() {
                            sizes.remove(&(hwnd as usize));
                        }
                    }
                    Err(e) => {
                        // 失败时先让插件清理可能已创建（但未登记）的部分视图，再销毁窗口：
                        // Qt 插件（Kontakt 8）会在 attach 前就创建自己的 QuickWindow，
                        // 直接销毁父窗口会让它收到外部的 WM_DESTROY 并报错。
                        // Let the plugin clean up a partially created view before the window dies:
                        // Qt plugins (Kontakt 8) create their QuickWindow before attaching, and
                        // destroying the parent first hands them an external WM_DESTROY.
                        let _ = plugin.close_editor();
                        drop(plugin);
                        DestroyWindow(hwnd);
                        return Err(e);
                    }
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            use xcb::Xid;

            // Create an X11 window via XCB and embed the plugin editor into it using the
            // VST3 X11EmbedWindowID platform type (handled in plugin_impl::open_editor).
            let (connection, screen_number) = xcb::Connection::connect(None)
                .map_err(|e| Error::Other(format!("Failed to connect to X server: {e}")))?;
            let setup = connection.get_setup();
            let screen = setup
                .roots()
                .nth(screen_number as usize)
                .ok_or_else(|| Error::Other("No X11 screen found".to_string()))?;
            let window = connection.generate_id();

            connection
                .send_and_check_request(&xcb::x::CreateWindow {
                    depth: xcb::x::COPY_FROM_PARENT as u8,
                    wid: window,
                    parent: screen.root(),
                    x: 0,
                    y: 0,
                    width: width as u16,
                    height: height as u16,
                    border_width: 0,
                    class: xcb::x::WindowClass::InputOutput,
                    visual: screen.root_visual(),
                    value_list: &[
                        xcb::x::Cw::BackPixel(screen.white_pixel()),
                        xcb::x::Cw::EventMask(
                            xcb::x::EventMask::EXPOSURE | xcb::x::EventMask::KEY_PRESS,
                        ),
                    ],
                })
                .map_err(|e| Error::Other(format!("Failed to create X11 window: {e}")))?;

            // Window title.
            let title = format!("{} - VST3", plugin_info.name);
            connection.send_request(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window,
                property: xcb::x::ATOM_WM_NAME,
                r#type: xcb::x::ATOM_STRING,
                data: title.as_bytes(),
            });

            // Show the window, then attach the plugin editor to its X11 id.
            connection.send_request(&xcb::x::MapWindow { window });
            let _ = connection.flush();

            let handle = crate::plugin::WindowHandle::from_x11(window.resource_id());
            self.plugin
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .open_editor(handle)?;

            self.native_window = Some(XcbWindowState { connection, window });
        }

        #[cfg(target_os = "android")]
        {
            return Err(Error::Other(
                "PluginWindow::open() is not supported on Android".to_string(),
            ));
        }

        Ok(())
    }

    /// 所属模块：vst3-host / 编辑器窗口
    /// 功能：打开无边框的宿主附属编辑器窗口（效果器卷帘内嵌模式）。
    /// 输入：`parent` 宿主主窗口的原生句柄（Windows `HWND`），作为附属窗口的 owner。
    /// 输出/副作用：创建 `WS_POPUP` 顶层窗口并附加插件视图；失败时返回中文上下文错误。
    /// 复杂度：时间 O(插件编辑器初始化)，空间 O(1)。
    ///
    /// Module: vst3-host / editor window
    /// Purpose: Open a frameless editor window owned by the host (inline effect-roll mode).
    /// Inputs: `parent` — native handle of the host main window (Windows `HWND`), used as the
    /// owner of the popup.
    /// Outputs / side effects: creates a `WS_POPUP` top-level window and attaches the plugin
    /// view; failures carry Chinese context.
    /// Complexity: O(plugin editor initialization) time and O(1) space.
    #[cfg(target_os = "windows")]
    pub fn open_embedded(&mut self, parent: *mut std::ffi::c_void) -> Result<()> {
        use std::mem;
        use std::ptr;

        if parent.is_null() {
            return Err(Error::Other("内嵌编辑器缺少父窗口".to_string()));
        }
        let has_editor = self
            .plugin
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .has_editor();
        if !has_editor {
            return Err(Error::Other(
                "Plugin does not have a GUI editor".to_string(),
            ));
        }
        if self.native_window.is_some() {
            self.close();
        }
        // 先用占位尺寸创建窗口：`get_editor_size()` 会额外创建一个临时视图，
        // 对 Auto-Tune 这类大界面插件要花近 100 ms，而宿主马上会重新摆放窗口。
        // Create the window with a placeholder size: `get_editor_size()` spawns an extra
        // temporary view (~100 ms for large editors such as Auto-Tune) and the host
        // repositions the window right away anyway.
        let (width, height) = (400, 300);

        unsafe {
            let class_name = "VST3PluginEmbeddedWindow\0"
                .encode_utf16()
                .collect::<Vec<u16>>();
            let mut wc: WNDCLASSEXW = mem::zeroed();
            wc.cbSize = mem::size_of::<WNDCLASSEXW>() as UINT;
            wc.style = CS_HREDRAW | CS_VREDRAW;
            wc.lpfnWndProc = Some(plugin_window_proc);
            wc.hInstance = GetModuleHandleW(ptr::null());
            wc.hCursor = LoadCursorW(ptr::null_mut(), IDC_ARROW);
            wc.lpszClassName = class_name.as_ptr();
            RegisterClassExW(&wc);

            // 内嵌编辑器必须使用**顶层无边框窗口**，而不是 `WS_CHILD` 子窗口：
            // 主窗口由 OpenGL 交换缓冲绘制，子窗口会被父窗口的缓冲区整块覆盖。
            // 顶层附属窗口由 DWM 单独合成，能稳定显示在卡片位置上方。
            // The inline editor must be a frameless *top-level* window, not a `WS_CHILD`:
            // the main window presents through an OpenGL swap chain that paints over child
            // windows. An owned top-level window is composited separately by DWM.
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class_name.as_ptr(),
                ptr::null(),
                // 先不显示：宿主第一帧摆放好位置后再 `ShowWindow`，避免闪现在左上角。
                // Start hidden: the host shows it once it has been placed on the first frame.
                WS_POPUP | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                0,
                0,
                width,
                height,
                parent as HWND,
                ptr::null_mut(),
                GetModuleHandleW(ptr::null()),
                ptr::null_mut(),
            );
            if hwnd.is_null() {
                return Err(Error::Other("创建内嵌编辑器窗口失败".to_string()));
            }

            let window_handle =
                crate::plugin::WindowHandle::from_hwnd(hwnd as *mut std::ffi::c_void);
            let mut plugin = self.plugin.lock().unwrap_or_else(|p| p.into_inner());
            let host_scale = match winapi::um::winuser::GetDpiForWindow(parent as HWND) {
                dpi if dpi > 0 => dpi as f32 / 96.0,
                _ => 1.0,
            };
            let _ = plugin.set_editor_scale_factor(host_scale);
            if let Err(error) = plugin.open_editor(window_handle) {
                drop(plugin);
                DestroyWindow(hwnd);
                return Err(error);
            }
            // 附加后再确认内容缩放支持：此时 `IPlugViewContentScaleSupport` 才可见。
            // The view only reports content-scale support once it is attached.
            let scalable = plugin.set_editor_scale_factor(host_scale).unwrap_or(false);
            let size_at_scale = plugin.attached_editor_size().ok().flatten();
            // 内嵌模式由宿主控制尺寸，丢弃附加阶段插件排队的 resize 请求。
            // The host owns the size in inline mode; drop the resize queued during attach.
            let _ = plugin.take_editor_resize_request();
            drop(plugin);

            self.native_window = Some(hwnd);
            self.embedded = true;
            self.embedded_parent = Some(parent as HWND);
            self.embedded_host_scale = host_scale;
            self.embedded_scale = host_scale;
            self.embedded_scalable = scalable;
            self.embedded_size = size_at_scale;
            self.embedded_base_size = size_at_scale
                .map(|(width, height)| (width as f32 / host_scale, height as f32 / host_scale));
        }
        Ok(())
    }

    /// 所属模块：vst3-host / 编辑器窗口
    /// 功能：非 Windows 平台不支持内嵌编辑器。
    /// 输入：`parent` 未使用。
    /// 输出/副作用：返回中文错误。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: vst3-host / editor window
    /// Purpose: Inline editors are not supported off Windows.
    /// Inputs: `parent` — unused.
    /// Outputs / side effects: returns a Chinese error.
    /// Complexity: O(1) time and space.
    #[cfg(not(target_os = "windows"))]
    pub fn open_embedded(&mut self, _parent: *mut std::ffi::c_void) -> Result<()> {
        Err(Error::Other(
            "内嵌编辑器目前仅支持 Windows".to_string(),
        ))
    }

    /// 所属模块：vst3-host / 编辑器窗口
    /// 功能：移动并缩放内嵌编辑器窗口（物理像素，相对宿主客户区）。
    /// 输入：`x` / `y` / `width` / `height` 目标矩形。
    /// 输出/副作用：把客户区坐标换算为屏幕坐标后调用 `SetWindowPos`；不影响普通顶层窗口。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: vst3-host / editor window
    /// Purpose: Move and resize the inline editor (physical pixels in the host client area).
    /// Inputs: `x` / `y` / `width` / `height` — target rectangle.
    /// Outputs / side effects: maps client coordinates to the screen and calls `SetWindowPos`;
    /// standalone windows are unaffected.
    /// Complexity: O(1) time and space.
    pub fn set_embedded_bounds(&self, x: i32, y: i32, width: i32, height: i32) {
        #[cfg(target_os = "windows")]
        if self.embedded {
            if let (Some(hwnd), Some(parent)) = (self.native_window, self.embedded_parent) {
                unsafe {
                    // 附属窗口是顶层窗口，位置使用屏幕坐标。
                    // The owned inline window is top-level, so it is placed in screen coordinates.
                    if IsWindow(parent) == 0 {
                        return;
                    }
                    let mut origin = POINT { x, y };
                    ClientToScreen(parent, &mut origin);
                    SetWindowPos(
                        hwnd,
                        std::ptr::null_mut(),
                        origin.x,
                        origin.y,
                        width.max(1),
                        height.max(1),
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        let _ = (x, y, width, height);
    }

    /// 所属模块：vst3-host / 编辑器窗口
    /// 功能：按可用空间等比缩放内嵌编辑器，返回窗口应使用的物理像素尺寸。
    /// 输入：`available_width` / `available_height` 卡片主体可用的物理像素尺寸。
    /// 输出/副作用：必要时调用 `set_editor_scale_factor`；插件不支持内容缩放时返回可用尺寸。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: vst3-host / editor window
    /// Purpose: Scale the inline editor to fit the available space and return the window size in
    /// physical pixels.
    /// Inputs: `available_width` / `available_height` — card body size in physical pixels.
    /// Outputs / side effects: may call `set_editor_scale_factor`; when the plugin does not
    /// implement content scaling, the available size is returned so `onSize` can lay it out.
    /// Complexity: O(1) time and space.
    pub fn fit_embedded(&mut self, available_width: i32, available_height: i32) -> (i32, i32) {
        #[cfg(target_os = "windows")]
        {
            let available_width = available_width.max(1);
            let available_height = available_height.max(1);
            if !self.embedded || !self.embedded_scalable {
                return (available_width, available_height);
            }
            let Some((base_width, base_height)) = self.embedded_base_size else {
                return (available_width, available_height);
            };
            if base_width <= 0.0 || base_height <= 0.0 {
                return (available_width, available_height);
            }
            let fit = (available_width as f32 / base_width)
                .min(available_height as f32 / base_height)
                .min(self.embedded_host_scale)
                .clamp(0.1, 4.0);
            if (fit - self.embedded_scale).abs() > 0.01 {
                let applied = if let Some(mut plugin) = self.try_lock_plugin() {
                    let supported = plugin.set_editor_scale_factor(fit).unwrap_or(false);
                    if supported {
                        // 缩放后重新读取视图尺寸；失败时按比例估算。
                        // Read the view size after scaling; fall back to the ratio.
                        let size = plugin.attached_editor_size().ok().flatten().or(Some((
                            (base_width * fit).round() as i32,
                            (base_height * fit).round() as i32,
                        )));
                        let _ = plugin.take_editor_resize_request();
                        Some(size)
                    } else {
                        None
                    }
                } else {
                    None
                };
                match applied {
                    Some(size) => {
                        self.embedded_scale = fit;
                        self.embedded_size = size;
                    }
                    // 暂时拿不到插件锁时沿用上一帧的尺寸，避免窗口在音频块之间来回跳动。
                    // When the plugin lock is busy, keep the previous size instead of jumping
                    // between audio blocks.
                    None => return self.embedded_size.unwrap_or((available_width, available_height)),
                }
            }
            self.embedded_size
                .unwrap_or((available_width, available_height))
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (available_width, available_height);
            (available_width, available_height)
        }
    }

    /// 内嵌模式是否自己处理尺寸（内容缩放），此时不需要把 `WM_SIZE` 回灌 `onSize`。
    /// Whether the inline mode owns its size through content scaling, in which case
    /// `WM_SIZE` must not be forwarded to `onSize`.
    #[cfg(target_os = "windows")]
    fn embedded_handles_own_size(&self) -> bool {
        self.embedded && self.embedded_scalable
    }

    /// 非 Windows 平台没有内嵌模式。
    /// No inline mode off Windows.
    #[cfg(not(target_os = "windows"))]
    fn embedded_handles_own_size(&self) -> bool {
        false
    }

    /// 所属模块：vst3-host / 编辑器窗口
    /// 功能：按可见交集裁剪内嵌编辑器（子窗口局部坐标，物理像素）。
    /// 输入：`left` / `top` / `right` / `bottom` 裁剪矩形。
    /// 输出/副作用：调用 `SetWindowRgn`；窗口区域外的鼠标事件穿透到宿主。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: vst3-host / editor window
    /// Purpose: Clip the inline editor to the visible intersection (child-local physical pixels).
    /// Inputs: `left` / `top` / `right` / `bottom` — clip rectangle.
    /// Outputs / side effects: calls `SetWindowRgn`; mouse events outside the region fall
    /// through to the host.
    /// Complexity: O(1) time and space.
    pub fn set_embedded_clip(&self, left: i32, top: i32, right: i32, bottom: i32) {
        #[cfg(target_os = "windows")]
        if self.embedded {
            if let Some(hwnd) = self.native_window {
                unsafe {
                    let region = CreateRectRgn(left, top, right, bottom);
                    // `SetWindowRgn` 接管区域所有权，成功后不能再删除。
                    // `SetWindowRgn` takes ownership of the region on success.
                    SetWindowRgn(hwnd, region, 1);
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        let _ = (left, top, right, bottom);
    }

    /// 所属模块：vst3-host / 编辑器窗口
    /// 功能：显示或隐藏内嵌编辑器窗口。
    /// 输入：`visible` 是否显示。
    /// 输出/副作用：调用 `ShowWindow`；不改变插件视图。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: vst3-host / editor window
    /// Purpose: Show or hide the inline editor window.
    /// Inputs: `visible` — whether to show it.
    /// Outputs / side effects: calls `ShowWindow`; the plugin view is unchanged.
    /// Complexity: O(1) time and space.
    pub fn set_embedded_visible(&self, visible: bool) {
        #[cfg(target_os = "windows")]
        if self.embedded {
            if let Some(hwnd) = self.native_window {
                unsafe {
                    ShowWindow(hwnd, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        let _ = visible;
    }

    /// Pump the editor's window-level traffic that has to cross into the plugin.
    ///
    /// **Call this from your UI event loop, once per frame, while the window is open.** Two
    /// things depend on it:
    ///
    /// - **Plugin-initiated resizes.** A resizable editor (VSTGUI zoom, a "big view" toggle)
    ///   asks the host to resize through `IPlugFrame::resizeView`. This applies the request to
    ///   the native window, so the editor is not clipped by a window that never grew.
    /// - **Windows DPI changes.** `WM_DPICHANGED` applies its suggested native rectangle
    ///   immediately in the window procedure and queues the new content scale for here; making
    ///   the COM call outside the window procedure prevents reentrant deadlocks on the plugin
    ///   mutex.
    ///
    /// Deliberately non-blocking: if the plugin mutex is held elsewhere (the audio callback
    /// holds it per block) this returns without doing anything, and the pending work is picked
    /// up on the next call.
    ///
    /// [`Self::is_open`] and [`Self::closed_by_user`] do *not* run this — they stay lock-free
    /// so they are safe to call while holding the plugin lock.
    pub fn service_platform_events(&self) -> Result<()> {
        if self.native_window.is_none() {
            return Ok(());
        }
        let Some(mut plugin) = self.try_lock_plugin() else {
            return Ok(());
        };

        // 后台线程排队的分析器/电平消息必须先送达控制器，GUI 才会刷新。
        // Deliver analyser/meter messages queued by background threads before anything else,
        // otherwise the editor stays on its last painted frame.
        plugin.service_connection_messages();
        plugin.service_run_loop();
        // SerumStudio patch: ignore pending DPI scale changes. The editor was
        // opened at the right scale and size (including plugins that inflate
        // their own GUI); re-applying a monitor DPI here would undo that.
        let _ = self.take_pending_scale_factor();
        let resize = plugin.take_editor_resize_request();
        // 用户拖动窗口边框后，把新的客户区尺寸转发给插件；
        // 固定尺寸编辑器忽略该请求，保留插件自己声明的尺寸。
        // Forward the client size after a user frame drag; fixed-size editors ignore
        // it and keep the size they declared themselves.
        let requested_client = self.take_pending_client_size();
        let mut accepted_client = None;
        if let Some((width, height)) = requested_client {
            // 支持内容缩放的内嵌编辑器由 `fit_embedded` 决定尺寸，不再回灌 `onSize`
            //（否则物理像素会被当成逻辑像素，视图被二次放大）。
            // Inline editors that support content scaling are sized by `fit_embedded`; do not
            // feed `WM_SIZE` into `onSize` (physical pixels would be treated as logical ones).
            if !self.embedded_handles_own_size()
                && plugin.is_editor_open()
                && plugin.attached_editor_can_resize() == Some(true)
            {
                accepted_client = plugin.resize_editor(width, height).ok();
            }
        }

        // Released before touching the native window: on Windows, resizing it may synchronously
        // dispatch window messages back into host code that wants this same lock.
        drop(plugin);

        if let Some((width, height)) = resize {
            self.resize_native_window(width, height);
        } else if let Some((width, height)) = accepted_client {
            // 插件对尺寸做了约束（宽高比、最小尺寸）时，让窗口跟随它接受的尺寸。
            // When the plugin constrains the size (aspect ratio, minimum), make the
            // window follow the size it accepted.
            if requested_client != Some((width, height)) {
                self.resize_native_window(width, height);
            }
        }
        Ok(())
    }

    /// Take the plugin lock without blocking, recovering a lock poisoned by an unrelated panic
    /// (a poisoned mutex is permanent, and treating it as failure would stop servicing the
    /// editor for the rest of the session).
    fn try_lock_plugin(&self) -> Option<std::sync::MutexGuard<'_, Plugin>> {
        match self.plugin.try_lock() {
            Ok(guard) => Some(guard),
            Err(std::sync::TryLockError::Poisoned(poison)) => Some(poison.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => None,
        }
    }

    /// The newest DPI the window procedure recorded for this window, as a VST3 content scale.
    #[cfg(target_os = "windows")]
    fn take_pending_scale_factor(&self) -> Option<f32> {
        let hwnd = self.native_window?;
        dpi_changes()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(&(hwnd as usize))
            .and_then(dpi_scale_factor)
    }

    /// Only Windows reports DPI changes through the window procedure.
    #[cfg(not(target_os = "windows"))]
    fn take_pending_scale_factor(&self) -> Option<f32> {
        None
    }

    /// 取出窗口过程记录的待处理客户区尺寸（物理像素）。
    /// Take the pending client size (physical pixels) recorded by the window procedure.
    #[cfg(target_os = "windows")]
    fn take_pending_client_size(&self) -> Option<(i32, i32)> {
        let hwnd = self.native_window?;
        pending_client_sizes()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(&(hwnd as usize))
    }

    /// 非 Windows 平台没有窗口过程尺寸记录。
    /// Non-Windows platforms have no window-procedure size records.
    #[cfg(not(target_os = "windows"))]
    fn take_pending_client_size(&self) -> Option<(i32, i32)> {
        None
    }

    /// Resize the native window so it fits an editor of `width` x `height` pixels.
    ///
    /// The plugin has already resized its own view (the host answered `resizeView` with
    /// `onSize`); this catches the window up.
    fn resize_native_window(&self, width: i32, height: i32) {
        if width <= 0 || height <= 0 {
            return;
        }
        log::debug!("editor asked the host to resize its window to {width}x{height}");

        #[cfg(target_os = "macos")]
        {
            let Some(window) = self.native_window.as_ref() else {
                return;
            };
            let size = NSSize::new(width as f64, height as f64);
            if let Some(container) = self.container_view.as_ref() {
                container.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), size));
            }
            window.setContentSize(size);
        }

        #[cfg(target_os = "windows")]
        {
            let Some(hwnd) = self.native_window else {
                return;
            };
            // 内嵌窗口的位置和尺寸由宿主卡片决定，忽略插件自己请求的尺寸，
            // 否则窗口会在宿主摆放与插件请求之间来回跳动。
            // Inline window geometry is owned by the host card; ignore plugin-initiated
            // resizes, otherwise the window fights the host every frame.
            if self.embedded {
                return;
            }
            // The plugin sizes its *client* area; grow the frame by the window's chrome, the
            // same way `open()` sizes the window to the editor in the first place.
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            unsafe {
                winapi::um::winuser::AdjustWindowRectEx(&mut rect, EDITOR_WINDOW_STYLE, 0, 0);
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }

        #[cfg(target_os = "linux")]
        {
            let Some(state) = self.native_window.as_ref() else {
                return;
            };
            state.connection.send_request(&xcb::x::ConfigureWindow {
                window: state.window,
                value_list: &[
                    xcb::x::ConfigWindow::Width(width as u32),
                    xcb::x::ConfigWindow::Height(height as u32),
                ],
            });
            let _ = state.connection.flush();
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        let _ = (width, height);
    }

    /// 所属模块：vst3-host / 编辑器窗口
    /// 功能：退出时直接隐藏并销毁原生窗口，不调用插件的 `close_editor`。
    /// 输入：`self`。
    /// 输出/副作用：窗口立即从屏幕消失；插件视图不再 detach（进程即将退出）。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: vst3-host / editor window
    /// Purpose: Hide and destroy the native window on shutdown without calling the plugin's
    /// `close_editor`.
    /// Inputs: `self`.
    /// Outputs / side effects: the window disappears immediately; the view is not detached
    /// (the process is exiting).
    /// Complexity: O(1) time and space.
    pub fn destroy_native_window(&mut self) {
        #[cfg(target_os = "windows")]
        {
            if let Some(hwnd) = self.native_window.take() {
                if let Ok(mut requests) = close_requests().lock() {
                    requests.remove(&(hwnd as usize));
                }
                if let Ok(mut changes) = dpi_changes().lock() {
                    changes.remove(&(hwnd as usize));
                }
                if let Ok(mut sizes) = pending_client_sizes().lock() {
                    sizes.remove(&(hwnd as usize));
                }
                unsafe {
                    ShowWindow(hwnd, SW_HIDE);
                    DestroyWindow(hwnd);
                }
                self.embedded = false;
                self.embedded_parent = None;
            }
        }
        #[cfg(target_os = "macos")]
        {
            self.container_view = None;
            if let Some(window) = self.native_window.take() {
                window.close();
            }
        }
        #[cfg(target_os = "linux")]
        {
            if let Some(state) = self.native_window.take() {
                let _ = state.connection.send_request(&xcb::x::DestroyWindow {
                    window: state.window,
                });
                let _ = state.connection.flush();
            }
        }
        #[cfg(target_os = "android")]
        {
            let _ = self.native_window.take();
        }
    }

    /// Close the plugin window
    pub fn close(&mut self) {
        // Detach the plugin editor first — it must never outlive the window it is embedded in.
        // A poisoned lock (an earlier panic on the audio thread) is recovered rather than
        // skipped, exactly as every other lock site here does: skipping it would destroy the
        // native window with the plugin's view still attached to it.
        let _ = self
            .plugin
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .close_editor();

        // Then close the native window
        #[cfg(target_os = "macos")]
        {
            self.container_view = None;
            if let Some(window) = self.native_window.take() {
                window.close();
            }
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(hwnd) = self.native_window.take() {
                if let Ok(mut requests) = close_requests().lock() {
                    requests.remove(&(hwnd as usize));
                }
                if let Ok(mut changes) = dpi_changes().lock() {
                    changes.remove(&(hwnd as usize));
                }
                if let Ok(mut sizes) = pending_client_sizes().lock() {
                    sizes.remove(&(hwnd as usize));
                }
                unsafe {
                    DestroyWindow(hwnd);
                }
                self.embedded = false;
                self.embedded_parent = None;
            }
        }

        #[cfg(target_os = "linux")]
        {
            if let Some(state) = self.native_window.take() {
                state.connection.send_request(&xcb::x::UnmapWindow {
                    window: state.window,
                });
                state.connection.send_request(&xcb::x::DestroyWindow {
                    window: state.window,
                });
                let _ = state.connection.flush();
            }
        }

        #[cfg(target_os = "android")]
        {
            let _ = self.native_window.take();
        }
    }

    /// Check if the window is currently open.
    ///
    /// Reports `false` once the user has dismissed the window themselves, even though the
    /// handle is still around — see [`Self::closed_by_user`].
    ///
    /// Reads native window state only: it never takes the plugin lock, so it is safe to call
    /// while holding it. Pair it with [`Self::service_platform_events`], which does the work
    /// that has to reach the plugin.
    pub fn is_open(&self) -> bool {
        self.native_window.is_some() && !self.native_window_dismissed()
    }

    /// Whether the user closed the window themselves, through its title-bar close button.
    ///
    /// Neither the plugin nor the host is told when that happens, so a host that tracks "the
    /// editor is open" in its own UI should poll this each frame and drop or
    /// [`close`](Self::close) the window when it reports `true`. Otherwise the editor stays
    /// attached to a window that is gone from the screen.
    ///
    /// Cheap and lock-free in the same sense as [`Self::is_open`]: native window state only,
    /// never the plugin lock.
    pub fn closed_by_user(&self) -> bool {
        self.native_window_dismissed()
    }

    /// 所属模块：vst3-host / 编辑器窗口
    /// 功能：请求编辑器窗口及其子窗口重绘，用于主窗口聚焦时保持插件 GUI 持续刷新。
    /// 输入：`self`。
    /// 输出/副作用：向系统投递 `WM_PAINT`（不阻塞、不进入插件代码）；非 Windows 为空操作。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: vst3-host / editor window
    /// Purpose: Request a repaint of the editor window and its children, keeping plugin GUIs
    /// refreshing while the host's main window has focus.
    /// Inputs: `self`.
    /// Outputs / side effects: queues `WM_PAINT` (never blocks or enters plugin code); no-op off
    /// Windows.
    /// Complexity: O(1) time and space.
    pub fn refresh_repaint(&self) {
        #[cfg(target_os = "windows")]
        if let Some(hwnd) = self.native_window {
            // SAFETY: only invalidates the window region; Windows repaints it on the UI thread
            // that owns the window.
            unsafe {
                RedrawWindow(
                    hwnd,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    RDW_INVALIDATE | RDW_ALLCHILDREN,
                );
            }
        }
    }

    /// 所属模块：vst3-host / 编辑器窗口
    /// 功能：判断插件编辑器窗口是否已经是系统前台窗口。
    /// 输入：`self`。
    /// 输出/副作用：返回布尔值；非 Windows 恒为 `false`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: vst3-host / editor window
    /// Purpose: Whether the plugin editor window is currently the foreground window.
    /// Inputs: `self`.
    /// Outputs / side effects: a boolean; always `false` off Windows.
    /// Complexity: O(1) time and space.
    pub fn is_foreground(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            if let Some(hwnd) = self.native_window {
                // SAFETY: `GetForegroundWindow` has no preconditions and the handle is owned by us.
                return unsafe { GetForegroundWindow() == hwnd };
            }
        }
        false
    }

    /// Platform probe behind [`Self::closed_by_user`].
    #[cfg(target_os = "macos")]
    fn native_window_dismissed(&self) -> bool {
        let Some(window) = self.native_window.as_ref() else {
            return false;
        };
        // A closed window is ordered out — but so is a miniaturized one, and so is every window
        // of a hidden application. Neither of those means the user dismissed the editor.
        if window.isVisible() || window.isMiniaturized() {
            return false;
        }
        match MainThreadMarker::new() {
            Some(mtm) => !NSApplication::sharedApplication(mtm).isHidden(),
            // AppKit state can only be read from the main thread; assume the window still stands.
            None => false,
        }
    }

    /// Platform probe behind [`Self::closed_by_user`].
    #[cfg(target_os = "windows")]
    fn native_window_dismissed(&self) -> bool {
        let Some(hwnd) = self.native_window else {
            return false;
        };
        close_requests()
            .lock()
            .is_ok_and(|requests| requests.contains(&(hwnd as usize)))
    }

    /// Platform probe behind [`Self::closed_by_user`]. X11 and Android editor windows carry no
    /// close affordance of their own, so there is nothing to observe.
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn native_window_dismissed(&self) -> bool {
        false
    }
}

impl Drop for PluginWindow {
    fn drop(&mut self) {
        self.close();
    }
}

/// Builder for creating plugin windows with egui integration
#[cfg(feature = "egui-widgets")]
pub struct PluginWindowBuilder {
    plugin: Arc<Mutex<Plugin>>,
}

#[cfg(feature = "egui-widgets")]
impl PluginWindowBuilder {
    /// Create a new builder for the given plugin
    pub fn new(plugin: Arc<Mutex<Plugin>>) -> Self {
        Self { plugin }
    }

    /// Build and open a standalone plugin window
    pub fn open_standalone(&self) -> Result<PluginWindow> {
        let mut window = PluginWindow::new(self.plugin.clone());
        window.open()?;
        Ok(window)
    }
}

#[cfg(test)]
mod tests {
    use super::dpi_scale_factor;

    #[test]
    fn windows_dpi_converts_to_vst_content_scale() {
        assert_eq!(dpi_scale_factor(0), None);
        assert_eq!(dpi_scale_factor(96), Some(1.0));
        assert_eq!(dpi_scale_factor(120), Some(1.25));
        assert_eq!(dpi_scale_factor(144), Some(1.5));
        assert_eq!(dpi_scale_factor(192), Some(2.0));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn wm_dpi_wparam_uses_horizontal_dpi() {
        let packed = (192usize << 16) | 144;
        assert_eq!(super::dpi_from_wparam(packed), Some(144));
        assert_eq!(super::dpi_from_wparam(0), None);
    }
}
