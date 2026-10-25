//! 所属模块：app / 传输与引擎同步
//! 文件用途：
//! - 播放、暂停、定位、试听与 UI 遥测快照。
//! - 工程同步与常驻后台插件同步线程。
//!
//! 函数清单：
//! - `spawn_audio_loads`：后台解码 WAV 并发布回引擎。
//! - `SyncRequest` / `run_sync_service` / `pump_plugin_messages`：常驻 STA 同步线程与插件消息泵。
//! - `SurfingApp::sync_engine` / `begin_async_sync` / `poll_engine_sync` / `finish_sync`：同步编排。
//! - `SurfingApp::begin_instrument_load` / `step_instrument_load` / `finish_instrument_load`：
//!   加载工程后在 UI 线程按时间片补载 VST 乐器插件。
//! - `SurfingApp::finish_loading_state`：加载完成后恢复加载提示、补完挂起的插件应用与播放。
//! - `SurfingApp::project_has_plugin_instruments` / `can_close_now` / `request_close_when_ready`：
//!   乐器插件线程判断与加载期间的延迟关闭。
//! - `SurfingApp::refresh_engine_snapshot` / `playhead`：每帧 UI 遥测快照。
//! - `SurfingApp::toggle_play` / `seek` / `jump_to_start` / `jump_to_end`：传输控制。
//! - `SurfingApp::audition_on` / `audition_off` / `release_held_notes`：试听与挂音清理。
//! - `PendingAudition` / `SurfingApp::queue_audition` / `SurfingApp::flush_pending_auditions`：
//!   引擎锁被占用时暂存并按顺序补发试听事件。
//!
//! Module: app / transport and engine sync
//! Purpose:
//! - Playback, pause, seeking, audition and UI telemetry snapshots.
//! - Project sync and the persistent background plugin sync thread.
//!
//! Functions:
//! - `spawn_audio_loads`: decode WAV files in the background and publish them to the engine.
//! - `SyncRequest` / `run_sync_service` / `pump_plugin_messages`: persistent STA sync thread and message pump.
//! - `SurfingApp::sync_engine` / `begin_async_sync` / `poll_engine_sync` / `finish_sync`: sync orchestration.
//! - `SurfingApp::begin_instrument_load` / `step_instrument_load` / `finish_instrument_load`:
//!   load VST instrument plugins in UI-thread time slices after a project load.
//! - `SurfingApp::finish_loading_state`: restore the loading prompt and finish pending plugin
//!   applications and playback once loading completes.
//! - `SurfingApp::project_has_plugin_instruments` / `can_close_now` / `request_close_when_ready`:
//!   instrument-thread detection and deferred close during loading.
//! - `SurfingApp::refresh_engine_snapshot` / `playhead`: per-frame UI telemetry snapshot.
//! - `SurfingApp::toggle_play` / `seek` / `jump_to_start` / `jump_to_end`: transport control.
//! - `SurfingApp::audition_on` / `audition_off` / `release_held_notes`: audition and note release.
//! - `PendingAudition` / `SurfingApp::queue_audition` / `SurfingApp::flush_pending_auditions`:
//!   buffer audition events while the engine lock is busy and replay them in order.

use super::*;
use crate::diag;
use std::sync::mpsc::Sender as MpscSender;
use std::sync::Weak;

/// 加载中关闭窗口时等待插件同步的上限；超过后强制关闭并放弃引擎。
/// Maximum wait for a plugin sync when closing during loading; after it the window force-closes
/// and the engine is abandoned.
const CLOSE_SYNC_TIMEOUT: Duration = Duration::from_secs(5);
/// 前台工程同步超过该耗时才写入日志（毫秒）；播放中同步约每 60 ms 一次，不能逐条记录。
/// UI-thread project syncs are logged only above this duration (milliseconds); playback syncs run
/// about every 60 ms and must not be logged one by one.
const ENGINE_SYNC_SLOW_MS: u64 = 30;
/// 乐器插件补载的时间片：每帧最多在该时长内实例化插件，避免 UI 线程长时间无响应。
/// Time slice for loading instrument plugins: one frame instantiates plugins only within this
/// budget, so the UI thread never blocks long enough to be flagged as unresponsive.
const INSTRUMENT_LOAD_SLICE: Duration = Duration::from_millis(120);

/// 待送入引擎的一条试听事件。
/// One audition event waiting to be handed to the engine.
///
/// 所属模块：app / 传输与引擎同步 / 试听
/// 功能：缓存一条 Note On/Off，引擎锁被音频回调占用时稍后按顺序补发。
/// 输入：字段见结构体定义。
/// 输出/副作用：无。
/// 复杂度：O(1)。
///
/// Module: app / transport and engine sync / audition
/// Purpose: Buffer one Note On/Off so it can be replayed in order when the audio callback holds
/// the engine lock.
/// Inputs: the struct fields below.
/// Outputs / side effects: none.
/// Complexity: O(1).
#[derive(Clone, Copy, Debug)]
pub(super) struct PendingAudition {
    track_id: TrackId,
    pitch: u8,
    on: bool,
    velocity: u8,
}

/// 后台插件同步请求。
/// A background plugin sync request.
pub(super) struct SyncRequest {
    pub project: Project,
    /// 为真时不在后台线程创建 VST 乐器实例，交由 UI 线程完成。
    /// When true, VST instrument instances are not created on the worker thread; the UI thread
    /// loads them afterwards.
    pub defer_instruments: bool,
    pub response: MpscSender<(Vec<String>, Vec<PathBuf>)>,
}

/// Decode audio sources on a worker thread so opening a project with long WAV
/// files never blocks the UI. Decoded files are published back into the engine
/// under its mutex.
fn spawn_audio_loads(engine: Weak<Mutex<Engine>>, paths: Vec<PathBuf>) {
    std::thread::spawn(move || {
        for path in paths {
            let result = crate::audio::io::AudioRuntime::import_audio(&path);
            // 弱引用：窗口关闭后不再延长引擎寿命，插件实例仍由 UI 线程析构。
            // Weak reference: a closed window no longer extends the engine's lifetime, so plugin
            // instances are still destroyed on the UI thread.
            let Some(engine) = engine.upgrade() else {
                return;
            };
            if let Ok(mut guard) = engine.lock() {
                match result {
                    Ok(file) => guard.install_audio_file(path, Arc::new(file)),
                    Err(error) => guard.fail_audio_file(path, error),
                }
            };
        }
    });
}

/// 所属模块：app / 后台插件同步
/// 功能：常驻同步线程：串行执行插件加载，并持续泵送插件创建的消息窗口。
/// 输入：
/// - `engine`：共享引擎。
/// - `requests`：同步请求队列；发送端断开后线程退出。
///
/// 输出/副作用：加载插件、暂停传输、回传音频解码请求；线程保持存活以维持插件窗口与定时器。
///
/// 复杂度：时间 O(插件加载)，空间 O(工程快照)。
///
/// Module: app / background plugin sync
/// Purpose: Persistent sync thread: load plugins serially and keep pumping
/// messages for windows created by plugins.
/// Inputs:
/// - `engine`: shared engine.
/// - `requests`: sync request queue; the thread exits when the sender is dropped.
///
/// Outputs / side effects: loads plugins, pauses the transport and returns audio
/// decode requests; the thread stays alive to keep plugin windows and timers valid.
///
/// Complexity: O(plugin load) time and O(project snapshot) space.
fn run_sync_service(engine: Weak<Mutex<Engine>>, requests: Receiver<SyncRequest>) {
    // 插件可能创建窗口或使用 STA COM；常驻线程保持 STA 与消息泵。
    // Plugins may create windows or use STA COM; keep an STA apartment and a
    // message pump on the persistent thread.
    #[cfg(target_os = "windows")]
    unsafe {
        winapi::um::combaseapi::CoInitializeEx(
            std::ptr::null_mut(),
            winapi::um::objbase::COINIT_APARTMENTTHREADED,
        );
    }
    loop {
        match requests.try_recv() {
            Ok(request) => {
                let Some(engine) = engine.upgrade() else {
                    break;
                };
                let tracks_text = request.project.tracks.len().to_string();
                let span = diag::Span::start("app.engine", "engine.sync.background");
                let outcome = match engine.lock() {
                    Ok(mut guard) => {
                        guard.pause();
                        let warnings =
                            guard.sync_project_with(&request.project, request.defer_instruments);
                        (warnings, guard.take_audio_requests())
                    }
                    Err(_) => (vec!["引擎锁已损坏，插件未加载".into()], Vec::new()),
                };
                span.end(&[
                    ("status", "ok"),
                    ("tracks", &tracks_text),
                    ("warnings", &outcome.0.len().to_string()),
                ]);
                let _ = request.response.send(outcome);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                pump_plugin_messages();
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
        }
    }
    // 不调用 CoUninitialize：插件实例仍由引擎持有，进程退出时由系统回收。
    // Do not call CoUninitialize: plugin instances are still owned by the engine;
    // the OS reclaims the apartment at process exit.
}

/// 所属模块：app / 后台插件同步
/// 功能：泵送当前线程的消息队列，服务插件加载时创建的窗口与定时器。
/// 输入：无。
/// 输出/副作用：分发本线程的 Win32 消息；非 Windows 平台为空操作。
/// 复杂度：时间 O(消息数)，空间 O(1)。
///
/// Module: app / background plugin sync
/// Purpose: Pump this thread's message queue so windows and timers created by
/// plugins during loading keep working.
/// Inputs: none.
/// Outputs / side effects: dispatches this thread's Win32 messages; no-op elsewhere.
/// Complexity: O(message count) time and O(1) space.
#[cfg(target_os = "windows")]
fn pump_plugin_messages() {
    use winapi::um::winuser::{DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE};
    let mut message: MSG = unsafe { std::mem::zeroed() };
    while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn pump_plugin_messages() {}

impl SurfingApp {
    /// 所属模块：app / 传输同步
    /// 功能：判断是否仍有插件同步待执行、正在后台执行或正在分批补载乐器。
    /// 输入：`self`。
    /// 输出/副作用：返回布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / transport sync
    /// Purpose: Whether a plugin sync is still pending, running in the background or loading
    /// instruments in slices.
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1) time and space.
    pub fn engine_sync_pending(&self) -> bool {
        self.needs_sync || self.engine_sync_rx.is_some() || self.instrument_load_pending
    }

    /// 所属模块：app / 传输同步
    /// 功能：判断工程是否包含 VST 乐器插件（需要在 UI 线程实例化）。
    /// 输入：`self`。
    /// 输出/副作用：返回布尔值；不修改状态。
    /// 复杂度：时间 O(轨道数)，空间 O(1)。
    ///
    /// Module: app / transport sync
    /// Purpose: Whether the project contains a VST instrument plugin, which must be instantiated on
    /// the UI thread.
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(tracks) time and O(1) space.
    pub(super) fn project_has_plugin_instruments(&self) -> bool {
        self.project
            .tracks
            .iter()
            .any(|track| matches!(track.instrument, InstrumentRef::Plugin(_)))
    }

    /// 所属模块：app / 关闭流程
    /// 功能：判断当前窗口能否立即关闭：加载解析与插件同步都已完成。
    /// 输入：`self`。
    /// 输出/副作用：返回布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / close flow
    /// Purpose: Whether the window can close now: project loading and plugin sync have finished.
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1) time and space.
    pub fn can_close_now(&self) -> bool {
        !self.is_loading_project()
            && self.engine_sync_rx.is_none()
            && !self.sync_in_background
            && !self.defer_sync
            && !self.instrument_load_pending
    }

    /// 所属模块：app / 关闭流程
    /// 功能：是否已因加载超时进入强制关闭（跳过 `can_close_now` 检查）。
    /// 输入：`self`。
    /// 输出/副作用：返回布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / close flow
    /// Purpose: Whether a load timeout already requested a forced close (bypasses
    /// `can_close_now`).
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1) time and space.
    pub fn force_closing(&self) -> bool {
        self.abandon_engine
    }

    /// 所属模块：app / 关闭流程
    /// 功能：在加载/插件同步进行中请求关闭：放弃尚未开始的部分，等后台同步结束后再关闭窗口。
    /// 输入：`self`。
    /// 输出/副作用：可能丢弃加载结果、清除加载提示并设置 `close_after_ready`；
    /// 不卸载仍在后台线程加载的插件。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / close flow
    /// Purpose: Request a close while loading or syncing: abandon what has not started yet and wait
    /// for the background sync to finish before closing.
    /// Inputs: `self`.
    /// Outputs / side effects: may drop the load result, clear the loading prompt and set
    /// `close_after_ready`; plugins still loading on the worker thread are not unloaded.
    /// Complexity: O(1) time and space.
    pub fn request_close_when_ready(&mut self) {
        if self.close_after_ready {
            return;
        }
        self.close_after_ready = true;
        self.pending_play = false;
        // 尚未应用的加载结果直接丢弃：窗口马上关闭，不应再替换工程。
        // Drop a load result that has not been applied: the window is closing, so the project
        // must not be replaced.
        self.load_rx = None;
        self.loaded_project = None;
        self.loading_until = None;
        // 此时不会再有新编辑，避免关闭时弹出未保存确认。
        // No further edits happen now; a close confirmation must not appear.
        self.dirty = false;
        if self.engine_sync_rx.is_some() {
            // 插件正在后台线程加载：不能取消，等它结束后再关闭窗口，避免在插件初始化
            // 中途析构窗口（VOCALOID 会在自己的 OnClosing 中访问尚未初始化的对象）。
            // Plugins are loading on the worker thread and cannot be cancelled; wait for the
            // sync to finish so windows are never destroyed in the middle of plugin init
            // (VOCALOID's OnClosing touches objects that are not initialised yet).
            self.loading_message = Some("正在停止加载…".into());
            self.status = "正在停止加载…".into();
            self.close_ready_deadline = Some(Instant::now() + CLOSE_SYNC_TIMEOUT);
        } else {
            // 分批补载只在帧与帧之间停顿，没有进行中的插件初始化，可以直接放弃剩余乐器。
            // A sliced load only pauses between frames and has no plugin initialisation in flight,
            // so the remaining instruments can be abandoned immediately.
            self.instrument_load_pending = false;
            self.instrument_load_warnings.clear();
            self.needs_sync = false;
            self.sync_in_background = false;
            self.defer_sync = false;
            self.loading_message = None;
            self.close_ready_deadline = None;
            self.status = "正在关闭工程…".into();
        }
    }

    /// 所属模块：app / 传输同步
    /// 功能：在 UI 线程完整同步一次工程（编辑后、插件增删后）；后台同步进行中时延后到下一轮。
    /// 输入：`self`。
    /// 输出/副作用：更新引擎、启动音频解码、写状态栏；插件实例在 UI 线程创建。
    /// 复杂度：时间 O(轨道数 + 片段数 + 插件加载)，空间 O(工程大小)。
    ///
    /// Module: app / transport sync
    /// Purpose: Run one full project sync on the UI thread (after edits and plugin changes); defer
    /// when a background sync is still running.
    /// Inputs: `self`.
    /// Outputs / side effects: updates the engine, starts audio decoding and writes the status bar;
    /// plugin instances are created on the UI thread.
    /// Complexity: O(tracks + clips + plugin load) time and O(project) space.
    pub fn sync_engine(&mut self) {
        if self.engine_sync_rx.is_some() {
            self.needs_sync = true;
            return;
        }
        self.needs_sync = false;
        self.engine_stale = false;
        self.sync_in_background = false;
        // 完整同步会覆盖分批补载的中间状态，直接结束分批流程。
        // A full sync supersedes the sliced loader, so end the slicing state here.
        self.instrument_load_pending = false;
        self.instrument_load_warnings.clear();
        // 同步要持有引擎锁，并在 UI 线程加载插件乐器；卡死时日志能指出这一步。
        // The sync holds the engine lock and loads instrument plugins on the UI thread; the span
        // points at this step when it hangs.
        diag::set_activity("engine.sync");
        let span = diag::Span::start("app.engine", "engine.sync");
        let tracks_text = self.project.tracks.len().to_string();
        let engine = self.engine.clone();
        let (warnings, requests) = match engine.lock() {
            Ok(mut guard) => {
                // 同步时顺带应用实时 GPU 配置；配置未变化时不会重建设备。
                // Also apply the realtime GPU config during sync; an unchanged config keeps the device.
                guard.configure_gpu_realtime(self.settings.gpu_realtime_config());
                let warnings = guard.sync_project(&self.project);
                (warnings, guard.take_audio_requests())
            }
            Err(_) => (Vec::new(), Vec::new()),
        };
        span.end_slow(
            ENGINE_SYNC_SLOW_MS,
            &[
                ("status", if warnings.is_empty() { "ok" } else { "warning" }),
                ("tracks", &tracks_text),
                ("warnings", &warnings.len().to_string()),
            ],
        );
        diag::set_activity("ui.frame");
        self.finish_sync(warnings, requests);
    }

    /// 所属模块：app / 传输同步
    /// 功能：在后台线程执行一次完整的引擎同步（加载工程时使用）。
    /// 输入：`self`。
    /// 输出/副作用：设置 `engine_sync_rx` 与 `needs_sync=false`；结果由 `poll_engine_sync` 应用。
    /// 复杂度：时间 O(1)，空间 O(工程快照)。
    ///
    /// Module: app / transport sync
    /// Purpose: Run a full engine sync on a worker thread (used when loading projects).
    /// Inputs: `self`.
    /// Outputs / side effects: sets `engine_sync_rx` and clears `needs_sync`; the
    /// result is applied by `poll_engine_sync`.
    /// Complexity: O(1) time and O(project snapshot) space.
    pub(super) fn begin_async_sync(&mut self) {
        self.needs_sync = false;
        self.engine_stale = false;
        // 常驻线程只创建一次：插件在加载时创建的隐藏窗口/定时器属于该线程，
        // 线程退出会让 Vocaloid 等插件的窗口失效、Pro-Q 的动态 GUI 停止刷新。
        // Create the persistent thread once: hidden windows and timers created by
        // plugins during loading belong to this thread; letting it exit breaks
        // Vocaloid's startup window and freezes Pro-Q's animated GUI.
        if self.engine_sync_tx.is_none() {
            let (tx, rx) = channel::<SyncRequest>();
            // 只持弱引用：窗口关闭后同步线程不会延长引擎寿命，插件实例在 UI 线程析构。
            // Weak only: a closed window no longer keeps the engine alive, so plugin instances are
            // destroyed on the UI thread.
            let engine = Arc::downgrade(&self.engine);
            std::thread::spawn(move || run_sync_service(engine, rx));
            self.engine_sync_tx = Some(tx);
        }
        let (tx, rx) = channel();
        self.engine_sync_rx = Some(rx);
        self.background_sync_in_flight = true;
        diag::info(
            "app.engine",
            "engine.sync.background",
            &[
                ("phase", "begin"),
                ("tracks", &self.project.tracks.len().to_string()),
            ],
        );
        let request = SyncRequest {
            project: self.project.clone(),
            defer_instruments: true,
            response: tx,
        };
        let sent = self
            .engine_sync_tx
            .as_ref()
            .map(|service| service.send(request).is_ok())
            .unwrap_or(false);
        if !sent {
            self.engine_sync_rx = None;
            self.background_sync_in_flight = false;
            self.status = "后台同步线程不可用，请重启应用".into();
        }
    }

    /// 所属模块：app / 传输同步
    /// 功能：轮询后台同步结果并恢复加载提示/待播放状态。
    /// 输入：`self`。
    /// 输出/副作用：可能启动音频解码、更新状态栏与加载提示、继续待播放操作。
    /// 复杂度：时间 O(1)/帧，空间 O(1)。
    ///
    /// Module: app / transport sync
    /// Purpose: Poll the background sync result and finish the loading prompt or
    /// pending playback.
    /// Inputs: `self`.
    /// Outputs / side effects: may start audio decoding, update the status bar and
    /// loading prompt, and continue a pending play request.
    /// Complexity: O(1) per frame and O(1) space.
    pub(super) fn poll_engine_sync(&mut self) {
        let outcome = match &self.engine_sync_rx {
            Some(rx) => match rx.try_recv() {
                Ok(value) => Some(Ok(value)),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(())),
            },
            None => return,
        };
        let Some(outcome) = outcome else {
            return;
        };
        let was_background = self.background_sync_in_flight;
        self.background_sync_in_flight = false;
        self.engine_sync_rx = None;
        self.sync_in_background = false;
        // 从请求发出到结果应用的等待时间；插件加载卡住时该值会明显偏大。
        // Wait from request to result; a stuck plugin load shows up as an unusually large value.
        let wait_ms = self.last_engine_sync.elapsed().as_millis() as u64;
        self.last_engine_sync = Instant::now();
        match outcome {
            Ok((warnings, requests)) => {
                diag::info(
                    "app.engine",
                    "engine.sync.background",
                    &[
                        ("phase", "end"),
                        ("status", "ok"),
                        ("wait_ms", &wait_ms.to_string()),
                        ("warnings", &warnings.len().to_string()),
                    ],
                );
                self.finish_sync(warnings, requests);
                if was_background {
                    // 后台线程只创建效果器实例；乐器插件留到这里由 UI 线程加载，
                    // 避免 VOCALOID 这类插件在错误的线程创建/析构窗口。
                    // The worker thread only creates effect instances; instrument plugins are
                    // loaded here on the UI thread so plugins such as VOCALOID never create or
                    // destroy their windows on the wrong thread.
                    self.mark_background_plugins();
                    if !self.close_after_ready && self.project_has_plugin_instruments() {
                        // 乐器插件必须在 UI 线程实例化；按时间片分帧补载，加载提示持续可见，
                        // 不会出现一次同步阻塞十秒导致窗口“未响应”。
                        // Instrument plugins must be instantiated on the UI thread; loading them
                        // in per-frame slices keeps the loading prompt painting and avoids a
                        // ten-second sync that makes the window appear unresponsive.
                        self.begin_instrument_load();
                    }
                }
                if !self.instrument_load_pending {
                    self.finish_loading_state();
                }
            }
            Err(()) => {
                diag::error(
                    "app.engine",
                    "engine.sync.background",
                    &[
                        ("phase", "end"),
                        ("status", "disconnected"),
                        ("wait_ms", &wait_ms.to_string()),
                    ],
                );
                self.status = "同步插件失败: 后台同步线程中断".into();
                self.loading_message = None;
                self.loading_until = None;
                self.pending_plugin_apply = None;
            }
        }
    }

    /// 所属模块：app / 传输同步
    /// 功能：把所有插件槽位标记为“实例由后台同步线程创建”。
    /// 输入：`self`。
    /// 输出/副作用：填充 `background_synced`；打开 GUI 前据此在 UI 线程重建实例。
    /// 复杂度：时间 O(轨道数 + 效果器数)，空间 O(槽位数)。
    ///
    /// Module: app / transport sync
    /// Purpose: Mark every plugin slot as "instance created by the background sync thread".
    /// Inputs: `self`.
    /// Outputs / side effects: fills `background_synced`; editors rebuild the instance on the
    /// UI thread before opening.
    /// Complexity: O(tracks + effects) time and O(slots) space.
    fn mark_background_plugins(&mut self) {
        for track in &self.project.tracks {
            // 乐器插件已在 UI 线程加载（见 `poll_engine_sync`），无需重建。
            // Instrument plugins are already loaded on the UI thread (see `poll_engine_sync`),
            // so they never need a rebuild.
            for (index, effect) in track.effects.iter().enumerate() {
                if effect.plugin().is_some() {
                    self.background_synced
                        .insert((track.id, SlotKind::Effect(index)));
                }
            }
        }
        if self
            .project
            .master_effect
            .as_ref()
            .is_some_and(|effect| effect.plugin().is_some())
        {
            self.background_synced
                .insert((crate::core::model::MASTER_TRACK_ID, SlotKind::Master));
        }
    }

    /// 所属模块：app / 传输同步
    /// 功能：开始分批补载 VST 乐器插件：记录总数并保持加载提示可见。
    /// 输入：`self`。
    /// 输出/副作用：置位 `instrument_load_pending`、清空累计警告并写入状态栏/加载提示；
    /// 不创建插件实例（实例由 `step_instrument_load` 每帧补载）。
    /// 复杂度：时间 O(轨道数)，空间 O(1)。
    ///
    /// Module: app / transport sync
    /// Purpose: Start loading VST instrument plugins in slices: record the total and keep the
    /// loading prompt visible.
    /// Inputs: `self`.
    /// Outputs / side effects: sets `instrument_load_pending`, clears accumulated warnings and
    /// writes the status bar / loading prompt; no plugin instance is created here (the per-frame
    /// `step_instrument_load` does that).
    /// Complexity: O(tracks) time and O(1) space.
    fn begin_instrument_load(&mut self) {
        self.instrument_load_pending = true;
        self.instrument_load_warnings.clear();
        self.instrument_load_total = self
            .project
            .tracks
            .iter()
            .filter(|track| matches!(track.instrument, InstrumentRef::Plugin(_)))
            .count();
        self.instrument_load_started = Instant::now();
        // 分批补载本身完成同步，先把待同步标记清掉，避免同一帧再触发一次完整同步。
        // The sliced loader syncs the project itself, so clear the pending flag to keep the same
        // frame from also running a full sync.
        self.needs_sync = false;
        self.engine_stale = false;
        self.loading_message = Some("正在加载插件…".into());
        self.loading_until = None;
        diag::info(
            "app.engine",
            "engine.sync.instruments",
            &[
                ("phase", "begin"),
                ("plugins", &self.instrument_load_total.to_string()),
            ],
        );
    }

    /// 所属模块：app / 传输同步
    /// 功能：推进一个时间片的乐器插件补载，并在全部就绪后结束加载提示。
    /// 输入：`self`。
    /// 输出/副作用：可能在 UI 线程创建插件实例、启动后台音频解码、更新状态栏与加载提示；
    /// 引擎锁被音频回调占用时本帧跳过，下一帧重试。
    /// 复杂度：时间 O(时间片内的插件加载 + 轨道数)，空间 O(1)。
    ///
    /// Module: app / transport sync
    /// Purpose: Advance one time slice of instrument plugin loading and finish the loading prompt
    /// once every instrument is ready.
    /// Inputs: `self`.
    /// Outputs / side effects: may create plugin instances on the UI thread, start background audio
    /// decoding and update the status bar / loading prompt; when the audio callback holds the
    /// engine lock the slice is skipped and retried next frame.
    /// Complexity: O(plugin loads inside the slice + tracks) time and O(1) space.
    pub(super) fn step_instrument_load(&mut self) {
        let engine = self.engine.clone();
        let Ok(mut guard) = engine.try_lock() else {
            return;
        };
        let (warnings, requests, pending) =
            guard.sync_project_slice(&self.project, INSTRUMENT_LOAD_SLICE);
        drop(guard);
        self.needs_sync = false;
        self.instrument_load_warnings.extend(warnings);
        if !requests.is_empty() {
            spawn_audio_loads(Arc::downgrade(&self.engine), requests);
            self.ui_audio_loading = true;
        }
        if pending == 0 {
            self.finish_instrument_load();
            return;
        }
        let loaded = self.instrument_load_total.saturating_sub(pending);
        self.loading_message = Some(format!(
            "正在加载插件…（{loaded}/{}）",
            self.instrument_load_total
        ));
    }

    /// 所属模块：app / 传输同步
    /// 功能：结束分批补载：写出耗时与警告，并收尾加载提示、挂起的插件应用与待播放。
    /// 输入：`self`。
    /// 输出/副作用：清除 `instrument_load_pending`；警告写入状态栏；加载提示转为“就绪”。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / transport sync
    /// Purpose: Finish the sliced load: log the duration and warnings, then wrap up the loading
    /// prompt, a pending plugin application and playback waiting to start.
    /// Inputs: `self`.
    /// Outputs / side effects: clears `instrument_load_pending`; warnings go to the status bar and
    /// the loading prompt turns into "就绪".
    /// Complexity: O(1) time and space.
    fn finish_instrument_load(&mut self) {
        self.instrument_load_pending = false;
        let warnings = std::mem::take(&mut self.instrument_load_warnings);
        diag::info(
            "app.engine",
            "engine.sync.instruments",
            &[
                ("phase", "end"),
                ("plugins", &self.instrument_load_total.to_string()),
                ("warnings", &warnings.len().to_string()),
                (
                    "dur_us",
                    &self
                        .instrument_load_started
                        .elapsed()
                        .as_micros()
                        .to_string(),
                ),
            ],
        );
        self.finish_loading_state();
        // 警告最后写入状态栏，避免被“已加载 …”覆盖。
        // Apply warnings last so "已加载 …" cannot hide a load failure.
        self.finish_sync(warnings, Vec::new());
    }

    /// 所属模块：app / 传输同步
    /// 功能：工程加载收尾：更新状态栏与加载提示，并补完挂起的插件应用与待播放操作。
    /// 输入：`self`。
    /// 输出/副作用：可能隐藏加载提示、读取插件实际名称/类型、开始待播放；
    /// 不修改工程内容（插件状态除外）。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / transport sync
    /// Purpose: Wrap up a project load: update the status bar and loading prompt, then finish a
    /// pending plugin application and pending playback.
    /// Inputs: `self`.
    /// Outputs / side effects: may hide the loading prompt, read the loaded plugin name/kind and
    /// start pending playback; project content is unchanged apart from stored plugin state.
    /// Complexity: O(1) time and space.
    fn finish_loading_state(&mut self) {
        if self.loading_message.is_some() {
            if let Some(path) = &self.project_path {
                self.status = format!("已加载 {}", path.display());
            }
            self.loading_message = Some("就绪".into());
            self.loading_until = Some(Instant::now() + Duration::from_millis(200));
        }
        // 插件加载完成后读取实际名称/类型并更新状态栏。
        // Read the loaded name/kind and update the status bar once loading lands.
        if let Some(apply) = self.pending_plugin_apply.take() {
            self.finish_plugin_apply(apply);
        }
        if self.pending_play {
            self.pending_play = false;
            self.toggle_play();
        }
    }

    fn finish_sync(&mut self, warnings: Vec<String>, requests: Vec<PathBuf>) {
        // 后台同步不会读取 UI 设置，这里补一次实时 GPU 配置（未变化时开销为一次比较）。
        // The background sync never reads UI settings, so apply the realtime GPU config here (a
        // single comparison when unchanged).
        if let Ok(mut engine) = self.engine.try_lock() {
            engine.configure_gpu_realtime(self.settings.gpu_realtime_config());
        }
        if !requests.is_empty() {
            spawn_audio_loads(Arc::downgrade(&self.engine), requests);
            self.ui_audio_loading = true;
        }
        if !warnings.is_empty() {
            self.status = warnings.join(" · ");
        }
    }

    /// Refresh the UI-facing engine snapshot exactly once per frame.
    ///
    /// All panels then read `ui_*` fields instead of taking the engine lock:
    /// the audio callback only ever contends with this single `try_lock`.
    /// Returns the tracks whose plugin parameters changed since the last poll.
    pub fn refresh_engine_snapshot(&mut self) -> Vec<TrackId> {
        let engine_handle = self.engine.clone();
        let Ok(mut engine) = engine_handle.try_lock() else {
            return Vec::new();
        };
        let changed = engine.poll_parameter_changes();
        self.ui_playing = engine.is_playing();
        self.ui_playhead = engine.position_beats();
        self.ui_cpu_load = engine.cpu_load();
        self.ui_auto_stopped = engine.auto_stopped();
        self.ui_audio_loading = engine.is_loading_audio();
        let audio_warnings = engine.take_audio_warnings();

        let (left, right, master) = engine.stereo_levels();
        self.ui_levels_l.clear();
        self.ui_levels_l.extend_from_slice(left);
        self.ui_levels_r.clear();
        self.ui_levels_r.extend_from_slice(right);
        self.ui_levels.clear();
        self.ui_levels.extend(
            self.ui_levels_l
                .iter()
                .zip(self.ui_levels_r.iter())
                .map(|(l, r)| l.max(*r)),
        );
        self.ui_master_levels = master;

        for track in &self.project.tracks {
            for clip in &track.clips {
                let Some(source) = &clip.audio else {
                    continue;
                };
                if !self.ui_waveforms.contains_key(&source.path) {
                    if let Some(peaks) = engine.audio_waveform(&source.path) {
                        self.ui_waveforms.insert(source.path.clone(), peaks);
                    }
                }
            }
        }
        if !audio_warnings.is_empty() {
            self.status = audio_warnings.join(" · ");
        }
        if let Some(warning) = engine.take_gpu_realtime_warning() {
            self.status = warning;
        }
        changed
    }

    /// Cached playhead position, updated by [`Self::refresh_engine_snapshot`].
    pub fn playhead(&self) -> f64 {
        self.ui_playhead
    }

    pub fn toggle_play(&mut self) {
        if self.engine_sync_rx.is_some() || self.instrument_load_pending {
            self.status = "正在加载插件，请稍候…".into();
            return;
        }
        // 后台线程创建的效果器实例在播放前重建到 UI 线程：播放中再打开 GUI 就不会触发
        // 阻塞式重建，避免音频欠载后出现的颗粒化输出。
        // Rebuild worker-created effect instances on the UI thread before playback: opening a GUI
        // while playing then never triggers a blocking rebuild, which caused audio dropouts.
        if !self.ui_playing {
            self.prepare_playback_plugins();
        }
        if self.engine_stale {
            self.begin_engine_load();
            self.pending_play = true;
            return;
        }
        let engine_handle = self.engine.clone();
        let Ok(engine) = engine_handle.try_lock() else {
            return;
        };
        if engine.is_playing() {
            drop(engine);
            if let Ok(mut engine) = self.engine.lock() {
                engine.pause();
            }
            self.ui_playing = false;
            return;
        }
        drop(engine);
        // 播放前把本窗口设为当前输出目标：其他仍在播放的工程会被暂停，避免两个工程同时
        // 输出到同一设备。
        // Before playback, make this window the output target: other playing projects are paused
        // so two projects never mix on one device.
        self.shared.audio.activate(&self.engine);
        if let Ok(mut engine) = self.engine.lock() {
            // 播放前应用实时 GPU 配置：用户刚切换设置后也能在本次播放生效。
            // Apply the realtime GPU config before playback so a just-changed setting takes effect.
            engine.configure_gpu_realtime(self.settings.gpu_realtime_config());
            engine.play();
        }
        self.ui_playing = true;
    }

    pub fn seek(&mut self, beat: f64) {
        if self.engine_sync_rx.is_some() {
            return;
        }
        if let Ok(mut engine) = self.engine.try_lock() {
            engine.seek(beat);
        }
        self.ui_playhead = beat.max(0.0);
    }

    pub fn jump_to_start(&mut self) {
        self.seek(0.0);
    }

    pub fn jump_to_end(&mut self) {
        let end = self.project.content_end_beat().max(0.0);
        self.seek(end);
    }

    /// 所属模块：app / 试听
    /// 功能：触发试听音符。事件先进入待发送队列，由引擎锁空闲时按顺序补发，避免音频回调占用
    /// 引擎锁期间直接丢弃（插件越重、锁占用越久，越容易出现连续音符漏音）。
    /// 输入：
    /// - `pitch`：MIDI 音高（0~127）。
    /// - `velocity`：力度（1~127）。
    /// 输出/副作用：更新 `audition_track` / `held_pitches` 与 `pending_auditions`；可能激活本窗口音频输出。
    /// 复杂度：时间 O(待发送事件数)，空间 O(1)。
    ///
    /// Module: app / audition
    /// Purpose: Trigger an audition note. Events go through the pending queue and are replayed in
    /// order once the engine lock is free, so notes are never dropped while the audio callback holds
    /// the lock (heavy plugins make lock contention, and missed consecutive notes, far more likely).
    /// Inputs:
    /// - `pitch`: MIDI pitch (0~127).
    /// - `velocity`: velocity (1~127).
    /// Outputs / side effects: updates `audition_track` / `held_pitches` / `pending_auditions`; may
    /// make this window the audio output target.
    /// Complexity: O(pending events) time and O(1) space.
    pub fn audition_on(&mut self, pitch: u8, velocity: u8) {
        if self.engine_sync_rx.is_some() {
            return;
        }
        if self.engine_stale {
            self.begin_engine_load();
            return;
        }
        let Some(track_id) = self.current_track_id() else {
            return;
        };
        if self.audition_track != Some(track_id) {
            self.release_held_notes();
            self.audition_track = Some(track_id);
        }
        // 试听同样把本窗口设为输出目标，否则未激活的窗口不会发声。
        // Audition also makes this window the output target; otherwise an inactive window
        // would stay silent.
        self.shared.audio.activate(&self.engine);
        // 同一音高重复触发时先补一条 Note Off 再发送 Note On：插件会重新起音，且不会留下挂音。
        // A repeated Note On for a held pitch first sends a Note Off, so plugins retrigger the note
        // without leaving a hanging voice.
        if !self.held_pitches.insert(pitch) {
            self.queue_audition(track_id, pitch, false, 0);
        }
        self.queue_audition(track_id, pitch, true, velocity);
    }

    /// 所属模块：app / 试听
    /// 功能：释放试听音符；同样经过待发送队列，锁忙时不会漏掉 Note Off。
    /// 输入：`pitch`：MIDI 音高（0~127）。
    /// 输出/副作用：更新 `held_pitches` 与 `pending_auditions`。
    /// 复杂度：时间 O(待发送事件数)，空间 O(1)。
    ///
    /// Module: app / audition
    /// Purpose: Release an audition note; it also goes through the pending queue so a busy lock
    /// never loses the Note Off.
    /// Inputs: `pitch`: MIDI pitch (0~127).
    /// Outputs / side effects: updates `held_pitches` and `pending_auditions`.
    /// Complexity: O(pending events) time and O(1) space.
    pub fn audition_off(&mut self, pitch: u8) {
        if !self.held_pitches.remove(&pitch) {
            return;
        }
        if let Some(track_id) = self.audition_track {
            self.queue_audition(track_id, pitch, false, 0);
        }
    }

    /// 所属模块：app / 试听
    /// 功能：释放本窗口全部挂起的试听音符，并清空键盘与虚拟键盘状态。
    /// 输入：`self`。
    /// 输出/副作用：`held_pitches` 清空，Note Off 进入待发送队列；不修改工程。
    /// 复杂度：时间 O(挂起音高数 + 待发送事件数)，空间 O(挂起音高数)。
    ///
    /// Module: app / audition
    /// Purpose: Release every held audition note and reset the keyboard/virtual-key state.
    /// Inputs: `self`.
    /// Outputs / side effects: clears `held_pitches` and queues Note Offs; the project is untouched.
    /// Complexity: O(held pitches + pending events) time and O(held pitches) space.
    pub fn release_held_notes(&mut self) {
        let pitches: Vec<u8> = self.held_pitches.drain().collect();
        if let Some(track_id) = self.audition_track {
            for pitch in pitches {
                self.queue_audition(track_id, pitch, false, 0);
            }
        }
        self.keyboard_held.clear();
        self.virtual_key_note = None;
    }

    /// 所属模块：app / 试听
    /// 功能：把一条试听事件追加到待发送队列，并立即尝试发送队首事件。
    /// 输入：
    /// - `track_id`：目标轨道 id。
    /// - `pitch`：MIDI 音高（0~127）。
    /// - `on`：`true` 为 Note On，`false` 为 Note Off。
    /// - `velocity`：Note On 力度（1~127），Note Off 忽略。
    /// 输出/副作用：追加并可能清空 `pending_auditions`；不修改工程。
    /// 复杂度：时间 O(待发送事件数)，空间 O(1)。
    ///
    /// Module: app / audition
    /// Purpose: Append an audition event to the pending queue and try to flush it right away.
    /// Inputs:
    /// - `track_id`: target track id.
    /// - `pitch`: MIDI pitch (0~127).
    /// - `on`: `true` for Note On, `false` for Note Off.
    /// - `velocity`: Note On velocity (1~127); ignored for Note Off.
    /// Outputs / side effects: appends to and may drain `pending_auditions`; the project is untouched.
    /// Complexity: O(pending events) time and O(1) space.
    fn queue_audition(&mut self, track_id: TrackId, pitch: u8, on: bool, velocity: u8) {
        self.pending_auditions.push(PendingAudition {
            track_id,
            pitch,
            on,
            velocity,
        });
        self.flush_pending_auditions();
    }

    /// 所属模块：app / 试听
    /// 功能：引擎锁可用时按顺序发送全部待发送试听事件；锁被音频回调占用时保留到下一帧。
    /// 输入：`self`。
    /// 输出/副作用：可能清空 `pending_auditions` 并调用 `Engine::audition`；不修改工程。
    /// 复杂度：时间 O(待发送事件数)，空间 O(1)。
    ///
    /// Module: app / audition
    /// Purpose: Send every pending audition event in order when the engine lock is free; keep them
    /// queued for the next frame while the audio callback holds the lock.
    /// Inputs: `self`.
    /// Outputs / side effects: may drain `pending_auditions` and call `Engine::audition`; the
    /// project is untouched.
    /// Complexity: O(pending events) time and O(1) space.
    pub(super) fn flush_pending_auditions(&mut self) {
        if self.pending_auditions.is_empty() {
            return;
        }
        let engine = self.engine.clone();
        let Ok(mut engine) = engine.try_lock() else {
            return;
        };
        for event in self.pending_auditions.drain(..) {
            engine.audition(event.track_id, event.pitch, event.on, event.velocity);
        }
    }
}
