//! 所属模块：app / UI 心跳
//! 文件用途：
//! - 在插件编辑器打开期间，定时向 UI 线程投递空消息并请求 egui 重绘。
//! - 解决 Win32 低优先级 `WM_TIMER` 无法唤醒 `MsgWaitForMultipleObjectsEx`、导致插件 GUI 停止刷新的问题。
//! 函数清单：
//! - `install`：记录 egui 上下文与 UI 线程 id，首次激活时启动心跳线程。
//! - `set_active`：按当前是否有插件编辑器开关心跳。
//! - `worker`：每 16 ms 请求重绘并向 UI 线程投递 `WM_NULL`。
//!
//! Module: app / UI heartbeat
//! Purpose:
//! - While a plugin editor is open, periodically post an empty message to the UI thread and
//!   request an egui repaint.
//! - Fix Win32 low-priority `WM_TIMER` messages not waking `MsgWaitForMultipleObjectsEx`,
//!   which otherwise freezes plugin editor animation.
//! Functions:
//! - `install`: remember the egui context and UI thread id; start the heartbeat on first use.
//! - `set_active`: enable the heartbeat while any plugin editor is open.
//! - `worker`: request a repaint and post `WM_NULL` to the UI thread every 16 ms.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

/// 是否需要在编辑器打开期间保持 UI 线程活跃。
/// Whether the UI thread must be kept awake while an editor is open.
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// 心跳线程是否已启动（整个进程只启动一次）。
/// Whether the heartbeat worker has been started (once per process).
static STARTED: AtomicBool = AtomicBool::new(false);
/// UI 线程 id；`0` 表示尚未获取。
/// UI thread id; `0` means not known yet.
static UI_THREAD: AtomicU32 = AtomicU32::new(0);
/// egui 上下文，用于从心跳线程请求重绘。
/// egui context used to request repaints from the heartbeat thread.
static CONTEXT: Mutex<Option<eframe::egui::Context>> = Mutex::new(None);

/// 心跳间隔：与播放/编辑器重绘节奏一致。
/// Heartbeat interval, matching the playback/editor repaint cadence.
const HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

/// 所属模块：app / UI 心跳
/// 功能：记录 egui 上下文与 UI 线程 id；首次调用时启动心跳线程。
/// 输入：
/// - `ctx`：egui 上下文。
/// - `ui_thread`：主窗口所在线程 id；`0` 表示未知。
/// 输出/副作用：保存全局上下文；必要时启动一个常驻线程。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / UI heartbeat
/// Purpose: Remember the egui context and UI thread id; start the heartbeat on first use.
/// Inputs:
/// - `ctx`: egui context.
/// - `ui_thread`: thread id owning the main window; `0` when unknown.
/// Outputs / side effects: stores the global context and may spawn one persistent thread.
/// Complexity: O(1) time and space.
pub(super) fn install(ctx: &eframe::egui::Context, ui_thread: u32) {
    if ui_thread != 0 {
        UI_THREAD.store(ui_thread, Ordering::Relaxed);
    }
    if let Ok(mut slot) = CONTEXT.lock() {
        if slot.is_none() {
            *slot = Some(ctx.clone());
        }
    }
    if !STARTED.swap(true, Ordering::Relaxed) {
        std::thread::spawn(worker);
    }
}

/// 所属模块：app / UI 心跳
/// 功能：按是否存在打开的插件编辑器开关心跳。
/// 输入：`active` 是否需要保持 UI 线程活跃。
/// 输出/副作用：更新全局开关；不阻塞。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / UI heartbeat
/// Purpose: Enable or disable the heartbeat based on whether a plugin editor is open.
/// Inputs: `active` — whether the UI thread must stay awake.
/// Outputs / side effects: updates the global flag; never blocks.
/// Complexity: O(1) time and space.
pub(super) fn set_active(active: bool) {
    ACTIVE.store(active, Ordering::Relaxed);
}

/// 所属模块：app / UI 心跳
/// 功能：心跳线程主体：请求 egui 重绘并向 UI 线程投递 `WM_NULL`。
/// 输入：无。
/// 输出/副作用：调用 egui 的线程安全重绘接口与 Win32 `PostThreadMessageW`；线程常驻。
/// 复杂度：时间 O(1)/次，空间 O(1)。
///
/// Module: app / UI heartbeat
/// Purpose: Heartbeat worker: request an egui repaint and post `WM_NULL` to the UI thread.
/// Inputs: none.
/// Outputs / side effects: calls egui's thread-safe repaint request and Win32
/// `PostThreadMessageW`; the thread lives for the process lifetime.
/// Complexity: O(1) per tick and O(1) space.
fn worker() {
    loop {
        if ACTIVE.load(Ordering::Relaxed) {
            if let Some(ctx) = CONTEXT
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .clone()
            {
                ctx.request_repaint();
            }
            #[cfg(target_os = "windows")]
            {
                let thread_id = UI_THREAD.load(Ordering::Relaxed);
                if thread_id != 0 {
                    // SAFETY: 只向 UI 线程投递空消息，不携带指针；用于唤醒消息循环。
                    // SAFETY: only posts an empty message to the UI thread, carrying no
                    // pointers; it exists to wake the message loop.
                    unsafe {
                        winapi::um::winuser::PostThreadMessageW(
                            thread_id,
                            winapi::um::winuser::WM_NULL,
                            0,
                            0,
                        );
                    }
                }
            }
        }
        std::thread::sleep(HEARTBEAT_INTERVAL);
    }
}
