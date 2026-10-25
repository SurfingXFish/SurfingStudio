//! 所属模块：app / 无边框窗口
//! 文件用途：
//! - 自绘窗口边框的最大化/还原；Windows 下按显示器工作区精确铺满，避免 DWM 把无边框窗口
//!   向外扩 8 像素导致应用栏与窗口按钮被屏幕边缘裁切。
//! - 记录最大化前的窗口外框，供还原使用。
//! 函数清单：
//! - `SurfingApp::is_window_maximized`：返回当前是否处于最大化（含原生最大化）。
//! - `SurfingApp::remember_window_rect`：记录正常状态下的窗口外框，供最大化后还原/拖动还原使用。
//! - `SurfingApp::toggle_window_maximize`：在最大化与还原之间切换。
//! - `SurfingApp::begin_window_drag`：最大化时先按指针位置还原窗口，再进入系统拖动。
//! - `window_work_area`：读取窗口所在显示器的工作区（仅 Windows）。
//!
//! Module: app / borderless window
//! Purpose:
//! - Maximize/restore for the hand-drawn window chrome; on Windows it fills the monitor work
//!   area exactly, because DWM extends a borderless maximized window 8 px outside the screen
//!   and would clip the app bar and window buttons.
//! - Remember the pre-maximize outer rect and restore the window under the pointer when the user
//!   starts dragging the app bar while maximized.
//! Functions:
//! - `SurfingApp::is_window_maximized`: whether the window is maximized (including native).
//! - `SurfingApp::remember_window_rect`: remember the normal outer rect for restore/drag-restore.
//! - `SurfingApp::toggle_window_maximize`: switch between maximized and restored.
//! - `SurfingApp::begin_window_drag`: restore under the pointer when maximized, then start dragging.
//! - `window_work_area`: read the work area of the window monitor (Windows only).

use super::*;

impl SurfingApp {
    /// 所属模块：app / 无边框窗口
    /// 功能：返回窗口是否处于最大化；同时考虑应用接管的最大化与 winit 上报的原生最大化。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：返回布尔值；不修改状态。
    /// 复杂度：O(1)。
    ///
    /// Module: app / borderless window
    /// Purpose: Whether the window is maximized, combining the app-driven state with winit's
    /// native flag.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: returns a bool; no state change.
    /// Complexity: O(1).
    pub(super) fn is_window_maximized(&self, ctx: &egui::Context) -> bool {
        self.window_maximized || ctx.input(|i| i.viewport().maximized.unwrap_or(false))
    }

    /// 所属模块：app / 无边框窗口
    /// 功能：在窗口处于正常状态时记录外框，供最大化还原与“最大化时拖动还原”使用；刚还原的
    /// 一帧跳过，避免把仍是最大化尺寸的旧外框写回。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：更新 `window_restore_rect`；可能清除 `window_restore_guard`。
    /// 复杂度：O(1)。
    ///
    /// Module: app / borderless window
    /// Purpose: Remember the outer rect while the window is normal, for maximize-restore and
    /// drag-restore; skip one frame right after restoring so the stale maximized rect is not saved.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: updates `window_restore_rect`; may clear `window_restore_guard`.
    /// Complexity: O(1).
    pub(super) fn remember_window_rect(&mut self, ctx: &egui::Context) {
        if self.window_restore_guard {
            self.window_restore_guard = false;
            return;
        }
        if self.window_maximized || ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            if rect.width() > 1.0 && rect.height() > 1.0 {
                self.window_restore_rect =
                    Some((rect.min.x, rect.min.y, rect.width(), rect.height()));
            }
        }
    }

    /// 所属模块：app / 无边框窗口
    /// 功能：在最大化与还原之间切换。Windows 下用显示器工作区手动铺满，避免 DWM 对无边框
    /// 最大化窗口外扩 8 像素造成的裁切；其他平台沿用 `ViewportCommand::Maximized`。
    /// 输入：`ctx`：egui 上下文，用于读取窗口外框并发送视口命令。
    /// 输出/副作用：修改 `window_maximized` / `window_restore_rect`，发送尺寸与位置命令。
    /// 复杂度：O(1)。
    ///
    /// Module: app / borderless window
    /// Purpose: Toggle maximize. On Windows the window is placed on the monitor work area
    /// manually so DWM's 8 px borderless bleed cannot clip the app bar; other platforms use
    /// `ViewportCommand::Maximized`.
    /// Inputs: `ctx`: egui context used to read the outer rect and send viewport commands.
    /// Outputs / side effects: mutates `window_maximized` / `window_restore_rect` and sends size
    /// and position commands.
    /// Complexity: O(1).
    pub(super) fn toggle_window_maximize(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "windows")]
        {
            // 原生最大化（Win+↑ 等）先交还给 winit 还原，避免记录错误的还原矩形。
            // Native maximize (Win+Up etc.) is restored through winit first, so the saved
            // restore rect is never taken from a maximized window.
            if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
                self.window_maximized = false;
                self.window_restore_guard = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
                return;
            }
            if self.window_maximized {
                self.window_maximized = false;
                self.window_restore_guard = true;
                if let Some((x, y, width, height)) = self.window_restore_rect {
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        width, height,
                    )));
                }
                return;
            }
            let current = ctx.input(|i| i.viewport().outer_rect);
            let work_area = window_work_area(self.raw_window, ctx.pixels_per_point());
            if let (Some(rect), Some((x, y, width, height))) = (current, work_area) {
                self.window_restore_rect =
                    Some((rect.min.x, rect.min.y, rect.width(), rect.height()));
                self.window_maximized = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(width, height)));
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }
    }

    /// 所属模块：app / 无边框窗口
    /// 功能：开始拖动窗口。最大化时先按指针在窗口中的横向比例还原到原尺寸，并让指针保持在
    /// 应用栏中线，再交给系统拖动，复现原生标题栏“拖动即还原”的行为。
    /// 输入：
    /// - `ctx`：egui 上下文。
    /// - `title_bar_height`：应用栏高度（逻辑点），用于把抓取点保持在应用栏内。
    /// 输出/副作用：修改 `window_maximized` / `window_restore_guard`，发送还原与拖动命令。
    /// 复杂度：O(1)。
    ///
    /// Module: app / borderless window
    /// Purpose: Start dragging the window. When maximized, first restore to the remembered size
    /// with the pointer's horizontal ratio and keep the grab point in the app bar, then hand over
    /// to the OS drag loop, reproducing the native "drag to restore" behavior.
    /// Inputs:
    /// - `ctx`: egui context.
    /// - `title_bar_height`: app bar height in points, keeping the grab point inside the bar.
    /// Outputs / side effects: mutates `window_maximized` / `window_restore_guard` and sends
    /// restore plus drag commands.
    /// Complexity: O(1).
    pub(super) fn begin_window_drag(&mut self, ctx: &egui::Context, title_bar_height: f32) {
        if !crate::platform::caps().window_drag {
            // 移动端窗口由系统全屏管理，拖动应用栏不能移动窗口。
            // Mobile windows are managed by the OS in full screen, so the app bar cannot drag them.
            return;
        }
        let native_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        if !self.window_maximized && !native_maximized {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            return;
        }

        let current = ctx.input(|i| i.viewport().outer_rect);
        let pointer = ctx.input(|i| i.pointer.interact_pos());
        if let (Some(current), Some(pointer), Some((_, _, width, height))) =
            (current, pointer, self.window_restore_rect)
        {
            let cursor = current.min + pointer.to_vec2();
            // 指针在最大化窗口中的横向比例决定还原后的窗口位置；纵向固定抓在应用栏中部。
            // The pointer's horizontal ratio places the restored window under the cursor; vertically
            // the grab point stays in the middle of the app bar.
            let ratio = (pointer.x / current.width().max(1.0)).clamp(0.15, 0.85);
            let new_x = cursor.x - width * ratio;
            let new_y = cursor.y - title_bar_height * 0.5;
            if native_maximized {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                new_x, new_y,
            )));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(width, height)));
            self.window_maximized = false;
            self.window_restore_guard = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        } else if native_maximized {
            // 缺少还原矩形时交给系统还原，再开始拖动。
            // Without a restore rect, let the OS restore the window and then start dragging.
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
            self.window_maximized = false;
            self.window_restore_guard = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }
}

/// 所属模块：app / 无边框窗口
/// 功能：读取窗口所在显示器的工作区，并换算成 egui 逻辑点；Windows 之外返回 `None`。
/// 输入：
/// - `raw`：主窗口原始句柄；非 Win32 或缺失时返回 `None`。
/// - `ppp`：egui 的物理像素/逻辑点比例，用于把 Win32 物理像素换算成逻辑点。
/// 输出/副作用：返回 `(x, y, width, height)` 逻辑点；不修改应用状态。
/// 复杂度：O(1)。
///
/// Module: app / borderless window
/// Purpose: Read the work area of the monitor owning the window and convert it to egui points;
/// returns `None` outside Windows.
/// Inputs:
/// - `raw`: raw handle of the main window; returns `None` when missing or not Win32.
/// - `ppp`: egui pixels-per-point used to convert Win32 physical pixels to points.
/// Outputs / side effects: `(x, y, width, height)` in points; no app state change.
/// Complexity: O(1).
#[cfg(target_os = "windows")]
fn window_work_area(raw: Option<RawWindowHandle>, ppp: f32) -> Option<(f32, f32, f32, f32)> {
    use raw_window_handle::RawWindowHandle;
    use winapi::shared::windef::RECT;
    use winapi::um::winuser::{
        GetMonitorInfoW, MonitorFromWindow, SystemParametersInfoW, MONITORINFO,
        MONITOR_DEFAULTTONEAREST, SPI_GETWORKAREA,
    };

    let RawWindowHandle::Win32(handle) = raw? else {
        return None;
    };
    let hwnd = handle.hwnd.get() as winapi::shared::windef::HWND;
    // 安全性：hwnd 来自仍然存活的 eframe 主窗口；仅在 UI 线程读取显示器信息，不接管所有权。
    // SAFETY: hwnd comes from the live eframe main window; we only read monitor information on
    // the UI thread and never take ownership.
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let work = if !monitor.is_null() && GetMonitorInfoW(monitor, &mut info) != 0 {
            info.rcWork
        } else {
            let mut rect: RECT = std::mem::zeroed();
            if SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                &mut rect as *mut RECT as *mut winapi::ctypes::c_void,
                0,
            ) == 0
            {
                return None;
            }
            rect
        };
        Some((
            work.left as f32 / ppp,
            work.top as f32 / ppp,
            (work.right - work.left) as f32 / ppp,
            (work.bottom - work.top) as f32 / ppp,
        ))
    }
}
