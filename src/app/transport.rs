//! 所属模块：app / 传输与引擎同步
//! 文件用途：
//! - 播放、暂停、定位、试听与 UI 遥测快照。
//! - 工程同步；所有插件实例都在 UI 线程创建。
//!
//! 函数清单：
//! - `spawn_audio_loads`：后台解码 WAV 并发布回引擎。
//! - `SurfingApp::sync_engine` / `finish_sync`：同步编排。
//! - `SurfingApp::begin_instrument_load` / `step_instrument_load` / `finish_instrument_load`：
//!   加载工程后在 UI 线程按时间片补载插件。
//! - `SurfingApp::finish_loading_state`：加载完成后恢复加载提示、补完挂起的插件应用与播放。
//! - `SurfingApp::project_has_plugins` / `can_close_now` / `request_close_when_ready`：
//!   插件加载判断与加载期间的延迟关闭。
//! - `SurfingApp::refresh_engine_snapshot` / `playhead`：每帧 UI 遥测快照。
//! - `SurfingApp::toggle_play` / `seek` / `jump_to_start` / `jump_to_end`：传输控制。
//! - `SurfingApp::audition_on` / `audition_on_for` / `audition_off` / `audition_off_for` /
//!   `release_track_notes` / `release_held_notes`：试听与挂音清理。
//! - `PendingAudition` / `SurfingApp::queue_audition` / `SurfingApp::flush_pending_auditions`：
//!   引擎锁被占用时暂存并按顺序补发试听事件。
//!
//! Module: app / transport and engine sync
//! Purpose:
//! - Playback, pause, seeking, audition and UI telemetry snapshots.
//! - Project sync; every plug-in instance is created on the UI thread.
//!
//! Functions:
//! - `spawn_audio_loads`: decode WAV files in the background and publish them to the engine.
//! - `SurfingApp::sync_engine` / `finish_sync`: sync orchestration.
//! - `SurfingApp::begin_instrument_load` / `step_instrument_load` / `finish_instrument_load`:
//!   load plugins in UI-thread time slices after a project load.
//! - `SurfingApp::finish_loading_state`: restore the loading prompt and finish pending plugin
//!   applications and playback once loading completes.
//! - `SurfingApp::project_has_plugins` / `can_close_now` / `request_close_when_ready`:
//!   plugin-load detection and deferred close during loading.
//! - `SurfingApp::refresh_engine_snapshot` / `playhead`: per-frame UI telemetry snapshot.
//! - `SurfingApp::toggle_play` / `seek` / `jump_to_start` / `jump_to_end`: transport control.
//! - `SurfingApp::audition_on` / `audition_on_for` / `audition_off` / `audition_off_for` /
//!   `release_track_notes` / `release_held_notes`: audition and note release.
//! - `PendingAudition` / `SurfingApp::queue_audition` / `SurfingApp::flush_pending_auditions`:
//!   buffer audition events while the engine lock is busy and replay them in order.

use super::*;
use crate::diag;
use std::sync::Weak;

/// 前台工程同步超过该耗时才写入日志（毫秒）；播放中同步约每 60 ms 一次，不能逐条记录。
/// UI-thread project syncs are logged only above this duration (milliseconds); playback syncs run
/// about every 60 ms and must not be logged one by one.
const ENGINE_SYNC_SLOW_MS: u64 = 30;
/// 插件补载的时间片：每帧最多在该时长内实例化插件，避免 UI 线程长时间无响应。
/// Time slice for loading plugins: one frame instantiates plugins only within this budget, so the
/// UI thread never blocks long enough to be flagged as unresponsive.
const PLUGIN_LOAD_SLICE: Duration = Duration::from_millis(120);

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

impl SurfingApp {
    /// 所属模块：app / 传输同步
    /// 功能：判断是否仍有同步待执行或正在分批补载插件。
    /// 输入：`self`。
    /// 输出/副作用：返回布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / transport sync
    /// Purpose: Whether a sync is still pending or plugins are loading in slices.
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1) time and space.
    pub fn engine_sync_pending(&self) -> bool {
        self.needs_sync || self.instrument_load_pending
    }

    /// 所属模块：app / 传输同步
    /// 功能：判断工程是否包含任何插件（乐器、轨道效果器或主效果器）。
    /// 输入：`self`。
    /// 输出/副作用：返回布尔值；不修改状态。
    /// 复杂度：时间 O(轨道数 + 效果器数)，空间 O(1)。
    ///
    /// Module: app / transport sync
    /// Purpose: Whether the project contains any plugin (instrument, track effect or master effect).
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(tracks + effects) time and O(1) space.
    pub(super) fn project_has_plugins(&self) -> bool {
        self.project
            .master_effect
            .as_ref()
            .is_some_and(|effect| effect.plugin().is_some())
            || self.project.tracks.iter().any(|track| {
                matches!(track.instrument, InstrumentRef::Plugin(_))
                    || track.effects.iter().any(|effect| effect.plugin().is_some())
            })
    }

    /// 所属模块：app / 关闭流程
    /// 功能：判断当前窗口能否立即关闭：加载解析与插件加载都已完成。
    /// 输入：`self`。
    /// 输出/副作用：返回布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / close flow
    /// Purpose: Whether the window can close now: project loading and plugin loading have finished.
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1) time and space.
    pub fn can_close_now(&self) -> bool {
        !self.is_loading_project() && !self.instrument_load_pending
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
    /// 功能：在加载/插件补载进行中请求关闭：放弃尚未开始的部分，本帧结束后关闭窗口。
    /// 输入：`self`。
    /// 输出/副作用：可能丢弃加载结果、清除加载提示并设置 `close_after_ready`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / close flow
    /// Purpose: Request a close while loading or loading plugins: abandon what has not started and
    /// close at the end of the frame.
    /// Inputs: `self`.
    /// Outputs / side effects: may drop the load result, clear the loading prompt and set
    /// `close_after_ready`.
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
        // 分批补载只在帧与帧之间停顿，没有进行中的插件初始化，可以直接放弃剩余插件。
        // A sliced load only pauses between frames and has no plugin initialisation in flight, so
        // the remaining plugins can be abandoned immediately.
        self.instrument_load_pending = false;
        self.instrument_load_warnings.clear();
        self.needs_sync = false;
        self.loading_message = None;
        self.status = "正在关闭工程…".into();
    }

    /// 所属模块：app / 传输同步
    /// 功能：在 UI 线程完整同步一次工程（编辑后、插件增删后）。
    /// 输入：`self`。
    /// 输出/副作用：更新引擎、启动音频解码、写状态栏；插件实例在 UI 线程创建。
    /// 复杂度：时间 O(轨道数 + 片段数 + 插件加载)，空间 O(工程大小)。
    ///
    /// Module: app / transport sync
    /// Purpose: Run one full project sync on the UI thread (after edits and plugin changes).
    /// Inputs: `self`.
    /// Outputs / side effects: updates the engine, starts audio decoding and writes the status bar;
    /// plugin instances are created on the UI thread.
    /// Complexity: O(tracks + clips + plugin load) time and O(project) space.
    pub fn sync_engine(&mut self) {
        // 导出会话独占引擎的工程/走带状态；期间禁止任何同步覆盖它。
        // The export session owns the engine's project/transport; no sync may overwrite it meanwhile.
        if self.export.is_rendering() {
            self.needs_sync = true;
            return;
        }
        self.needs_sync = false;
        // 完整同步会覆盖分批补载的中间状态，直接结束分批流程。
        // A full sync supersedes the sliced loader, so end the slicing state here.
        self.instrument_load_pending = false;
        self.instrument_load_warnings.clear();
        // 同步要持有引擎锁并在 UI 线程加载插件；卡死时日志能指出这一步。
        // The sync holds the engine lock and loads plugins on the UI thread; the span points at this
        // step when it hangs.
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
    /// 功能：开始分批补载插件：记录总数并保持加载提示可见。
    /// 输入：`self`。
    /// 输出/副作用：置位 `instrument_load_pending`、清空累计警告并写入状态栏/加载提示；
    /// 不创建插件实例（实例由 `step_instrument_load` 每帧补载）。
    /// 复杂度：时间 O(轨道数)，空间 O(1)。
    ///
    /// Module: app / transport sync
    /// Purpose: Start loading plugins in slices: record the total and keep the loading prompt
    /// visible.
    /// Inputs: `self`.
    /// Outputs / side effects: sets `instrument_load_pending`, clears accumulated warnings and
    /// writes the status bar / loading prompt; no plugin instance is created here (the per-frame
    /// `step_instrument_load` does that).
    /// Complexity: O(tracks + effects) time and O(1) space.
    pub(super) fn begin_instrument_load(&mut self) {
        self.instrument_load_pending = true;
        self.instrument_load_warnings.clear();
        self.instrument_load_total = self
            .project
            .tracks
            .iter()
            .map(|track| {
                usize::from(matches!(track.instrument, InstrumentRef::Plugin(_)))
                    + track
                        .effects
                        .iter()
                        .filter(|effect| effect.plugin().is_some())
                        .count()
            })
            .sum::<usize>()
            + usize::from(
                self.project
                    .master_effect
                    .as_ref()
                    .is_some_and(|effect| effect.plugin().is_some()),
            );
        self.instrument_load_started = Instant::now();
        // 分批补载本身完成同步，先把待同步标记清掉，避免同一帧再触发一次完整同步。
        // The sliced loader syncs the project itself, so clear the pending flag to keep the same
        // frame from also running a full sync.
        self.needs_sync = false;
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
    /// 功能：推进一个时间片的插件补载，并在全部就绪后结束加载提示。
    /// 输入：`self`。
    /// 输出/副作用：可能在 UI 线程创建插件实例、启动后台音频解码、更新状态栏与加载提示；
    /// 引擎锁被音频回调占用时本帧跳过，下一帧重试。
    /// 复杂度：时间 O(时间片内的插件加载 + 轨道数)，空间 O(1)。
    ///
    /// Module: app / transport sync
    /// Purpose: Advance one time slice of plugin loading and finish the loading prompt once every
    /// plugin is ready.
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
            guard.sync_project_slice(&self.project, PLUGIN_LOAD_SLICE);
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
        // 每 5 秒记录插件内部走带与各轨电平：用于确认插件是否跟随、某轨到底有没有出声。
        // Log the plugin transport and per-track levels every 5 s: confirms whether a plugin follows
        // the host and whether a track produces any level at all.
        engine.log_playback_probe();

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
        if self.export.is_rendering() {
            self.status = "正在导出音频，传输已锁定".into();
            return;
        }
        if self.instrument_load_pending {
            self.status = "正在加载插件，请稍候…".into();
            return;
        }
        let engine_handle = self.engine.clone();
        let Ok(engine) = engine_handle.try_lock() else {
            return;
        };
        if engine.is_playing() {
            drop(engine);
            // 停止播放即退出录音：先按当前播放头收尾，再暂停走带。
            // Stopping playback ends recording: finish at the current playhead before pausing.
            self.finish_recording();
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
        if self.instrument_load_pending {
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
    /// 功能：触发当前选中轨道的试听音符。事件先进入待发送队列，由引擎锁空闲时按顺序补发，
    /// 避免音频回调占用引擎锁期间直接丢弃。
    /// 输入：
    /// - `pitch`：MIDI 音高（0~127）。
    /// - `velocity`：力度（1~127）。
    /// 输出/副作用：更新 `audition_track` / `held_notes` 与 `pending_auditions`；可能激活本窗口音频输出。
    /// 复杂度：时间 O(待发送事件数)，空间 O(1)。
    ///
    /// Module: app / audition
    /// Purpose: Trigger an audition note on the selected track. Events go through the pending queue
    /// and are replayed in order once the engine lock is free.
    /// Inputs:
    /// - `pitch`: MIDI pitch (0~127).
    /// - `velocity`: velocity (1~127).
    /// Outputs / side effects: updates `audition_track` / `held_notes` / `pending_auditions`; may
    /// make this window the audio output target.
    /// Complexity: O(pending events) time and O(1) space.
    pub fn audition_on(&mut self, pitch: u8, velocity: u8) {
        let Some(track_id) = self.current_track_id() else {
            return;
        };
        if self.audition_track != Some(track_id) {
            self.release_track_notes(self.audition_track);
            self.audition_track = Some(track_id);
        }
        self.audition_on_for(track_id, pitch, velocity);
    }

    /// 所属模块：app / 试听
    /// 功能：在指定轨道触发试听音符；同一音高重复触发时先补 Note Off。
    /// 输入：
    /// - `track_id`：目标轨道 id。
    /// - `pitch`：MIDI 音高（0~127）。
    /// - `velocity`：力度（1~127）。
    /// 输出/副作用：更新 `held_notes` 与 `pending_auditions`；可能激活本窗口音频输出。
    /// 复杂度：时间 O(待发送事件数)，空间 O(1)。
    ///
    /// Module: app / audition
    /// Purpose: Trigger an audition note on the given track; a repeated pitch first receives a
    /// Note Off so the plugin retriggers it.
    /// Inputs:
    /// - `track_id`: target track id.
    /// - `pitch`: MIDI pitch (0~127).
    /// - `velocity`: velocity (1~127).
    /// Outputs / side effects: updates `held_notes` and `pending_auditions`; may make this window
    /// the audio output target.
    /// Complexity: O(pending events) time and O(1) space.
    pub fn audition_on_for(&mut self, track_id: TrackId, pitch: u8, velocity: u8) {
        // 试听同样把本窗口设为输出目标，否则未激活的窗口不会发声。
        // Audition also makes this window the output target; otherwise an inactive window
        // would stay silent.
        self.shared.audio.activate(&self.engine);
        // 同一音高重复触发时先补一条 Note Off 再发送 Note On：插件会重新起音，且不会留下挂音。
        // A repeated Note On for a held pitch first sends a Note Off, so plugins retrigger the note
        // without leaving a hanging voice.
        let retrigger = !self.held_notes.entry(track_id).or_default().insert(pitch);
        if self.engine_sync_pending() {
            // 引擎还没跟上工程：事件排队，同步完成后的每帧 `flush_pending_auditions` 补发。
            // The engine has not caught up with the project yet: queue the events; the per-frame
            // `flush_pending_auditions` sends them once the sync finishes.
            if retrigger {
                self.pending_auditions.push(PendingAudition {
                    track_id,
                    pitch,
                    on: false,
                    velocity: 0,
                });
            }
            self.pending_auditions.push(PendingAudition {
                track_id,
                pitch,
                on: true,
                velocity,
            });
            return;
        }
        if retrigger {
            self.queue_audition(track_id, pitch, false, 0);
        }
        self.queue_audition(track_id, pitch, true, velocity);
    }

    /// 所属模块：app / 试听
    /// 功能：释放当前选中轨道的试听音符。
    /// 输入：`pitch`：MIDI 音高（0~127）。
    /// 输出/副作用：更新 `held_notes` 与 `pending_auditions`。
    /// 复杂度：时间 O(待发送事件数)，空间 O(1)。
    ///
    /// Module: app / audition
    /// Purpose: Release an audition note on the selected track.
    /// Inputs: `pitch`: MIDI pitch (0~127).
    /// Outputs / side effects: updates `held_notes` and `pending_auditions`.
    /// Complexity: O(pending events) time and O(1) space.
    pub fn audition_off(&mut self, pitch: u8) {
        if let Some(track_id) = self.audition_track {
            self.audition_off_for(track_id, pitch);
        }
    }

    /// 所属模块：app / 试听
    /// 功能：释放指定轨道上的一个试听音符。
    /// 输入：
    /// - `track_id`：目标轨道 id。
    /// - `pitch`：MIDI 音高（0~127）。
    /// 输出/副作用：从 `held_notes` 移除并排入 Note Off。
    /// 复杂度：时间 O(待发送事件数)，空间 O(1)。
    ///
    /// Module: app / audition
    /// Purpose: Release one audition note on the given track.
    /// Inputs:
    /// - `track_id`: target track id.
    /// - `pitch`: MIDI pitch (0~127).
    /// Outputs / side effects: removes the pitch from `held_notes` and queues a Note Off.
    /// Complexity: O(pending events) time and O(1) space.
    pub fn audition_off_for(&mut self, track_id: TrackId, pitch: u8) {
        let Some(held) = self.held_notes.get_mut(&track_id) else {
            return;
        };
        if !held.remove(&pitch) {
            return;
        }
        if held.is_empty() {
            self.held_notes.remove(&track_id);
        }
        self.queue_audition(track_id, pitch, false, 0);
    }

    /// 所属模块：app / 试听
    /// 功能：释放指定轨道全部挂起的试听音符。
    /// 输入：`track_id`：轨道 id，可为 `None`。
    /// 输出/副作用：Note Off 进入待发送队列；不修改工程。
    /// 复杂度：时间 O(挂起音高数 + 待发送事件数)，空间 O(挂起音高数)。
    ///
    /// Module: app / audition
    /// Purpose: Release every held audition note on one track.
    /// Inputs: `track_id`: track id, may be `None`.
    /// Outputs / side effects: queues Note Offs; the project is untouched.
    /// Complexity: O(held pitches + pending events) time and O(held pitches) space.
    pub fn release_track_notes(&mut self, track_id: Option<TrackId>) {
        let Some(track_id) = track_id else {
            return;
        };
        let Some(pitches) = self.held_notes.remove(&track_id) else {
            return;
        };
        for pitch in pitches {
            self.queue_audition(track_id, pitch, false, 0);
        }
    }

    /// 所属模块：app / 试听
    /// 功能：释放本窗口全部挂起的试听音符，并清空键盘与虚拟键盘状态。
    /// 输入：`self`。
    /// 输出/副作用：`held_notes` 清空，Note Off 进入待发送队列；不修改工程。
    /// 复杂度：时间 O(挂起音高数 + 待发送事件数)，空间 O(挂起音高数)。
    ///
    /// Module: app / audition
    /// Purpose: Release every held audition note and reset the keyboard/virtual-key state.
    /// Inputs: `self`.
    /// Outputs / side effects: clears `held_notes` and queues Note Offs; the project is untouched.
    /// Complexity: O(held pitches + pending events) time and O(held pitches) space.
    pub fn release_held_notes(&mut self) {
        let groups: Vec<(TrackId, Vec<u8>)> = self
            .held_notes
            .drain()
            .map(|(track_id, pitches)| (track_id, pitches.into_iter().collect()))
            .collect();
        for (track_id, pitches) in groups {
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
