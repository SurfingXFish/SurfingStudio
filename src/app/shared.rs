//! 所属模块：app / 多窗口共享服务
//! 文件用途：
//! - 保存主页窗口与所有工程窗口共用的服务：全局设置、插件缓存、MIDI 输入、音频路由与
//!   窗口编号。
//! - 用 generation 计数让各窗口按需同步共享数据，避免每帧克隆插件列表。
//! - 在设置变更后重启音频输出、重连 MIDI 设备，并把状态文本同步给所有窗口。
//!
//! 函数清单：
//! - `SharedServices::new`：加载设置与插件缓存、启动唯一音频输出与 MIDI 输入。
//! - `SharedServices::start_audio`：按设置启动音频输出，失败时回退系统默认设备。
//! - `SharedServices::audio_status` / `restart_audio` / `reconnect_midi`：音频与 MIDI 状态。
//! - `SharedServices::settings_snapshot` / `settings_if_newer` / `write_settings`：全局设置快照与写回。
//! - `SharedServices::plugin_cache_snapshot` / `plugin_cache_if_newer` / `write_plugin_cache`：插件缓存快照与写回。
//! - `SharedServices::midi_status` / `midi_events`：MIDI 状态与事件队列。
//! - `SharedServices::next_window_id` / `last_focused_window` / `set_last_focused_window`：窗口编号与全局空格目标。
//!
//! Module: app / shared services for multiple windows
//! Purpose:
//! - Hold the services shared by the home window and every project window: global settings, the
//!   plugin cache, MIDI input, audio routing and window ids.
//! - Use generation counters so windows only copy shared data when it actually changed.
//! - Restart the audio output and reconnect MIDI after settings change, and share status text.
//!
//! Functions:
//! - `SharedServices::new`: load settings and the plugin cache, start the single audio output and MIDI input.
//! - `SharedServices::start_audio`: start the audio output from settings, falling back to the default device.
//! - `SharedServices::audio_status` / `restart_audio` / `reconnect_midi`: audio and MIDI status.
//! - `SharedServices::settings_snapshot` / `settings_if_newer` / `write_settings`: settings snapshot and write-back.
//! - `SharedServices::plugin_cache_snapshot` / `plugin_cache_if_newer` / `write_plugin_cache`: plugin cache snapshot and write-back.
//! - `SharedServices::midi_status` / `midi_events`: MIDI status and event queue.
//! - `SharedServices::next_window_id` / `last_focused_window` / `set_last_focused_window`: window ids and the global Space target.

use super::settings::AppSettings;
use super::*;
use crate::audio::backend::start_output;
use crate::audio::backend::start_output_with;
use crate::audio::{AudioOutput, AudioRouter};
use crate::core::midi_input::{MidiInputManager, MidiKeyEvent};
use crate::diag;
use crate::plugin::ScannedPlugin;
use std::sync::atomic::{AtomicU64, Ordering};

/// 全局设置快照与版本号。
/// Global settings snapshot and generation.
struct SharedSettings {
    settings: AppSettings,
    generation: u64,
}

/// 插件缓存快照与版本号。
/// Plugin cache snapshot and generation.
struct SharedPluginCache {
    plugins: Vec<ScannedPlugin>,
    fingerprints: HashMap<String, (u64, u64)>,
    generation: u64,
}

/// 多窗口共享服务。
/// Services shared by all windows.
pub struct SharedServices {
    settings: Mutex<SharedSettings>,
    plugin_cache: Mutex<SharedPluginCache>,
    midi: Mutex<MidiInputManager>,
    /// 全进程唯一的音频输出；保持存活以持续运行输出流。设置变更时整体替换。
    /// The single audio output of the process; kept alive so the stream keeps running. Replaced when
    /// the settings change.
    audio_output: Mutex<Option<AudioOutput>>,
    /// 各窗口引擎共用的音频路由。
    /// Audio router shared by every window engine.
    pub audio: Arc<AudioRouter>,
    /// 音频设备说明，供设置窗口与状态栏显示。
    /// Audio device description shown by the settings window and status bar.
    audio_status: Mutex<String>,
    next_window_id: AtomicU64,
    last_focused_window: AtomicU64,
}

impl SharedServices {
    /// 所属模块：app / 多窗口共享服务
    /// 功能：加载设置与插件缓存、启动音频输出与 MIDI 输入，并创建共享路由。
    /// 输入：无。
    /// 输出/副作用：打开音频设备与 MIDI 端口；读取配置文件与插件缓存文件。
    /// 复杂度：时间 O(配置文件 + 插件缓存大小 + 设备初始化)，空间 O(插件数量)。
    ///
    /// Module: app / shared services
    /// Purpose: Load settings and the plugin cache, start audio output and MIDI input, and create
    /// the shared router.
    /// Inputs: none.
    /// Outputs / side effects: opens the audio device and MIDI port; reads the config and plugin
    /// cache files.
    /// Complexity: O(config size + cache size + device setup) time and O(plugin count) space.
    pub fn new() -> Arc<Self> {
        let init_span = diag::Span::start_logged("app.boot", "shared.init", &[]);
        let settings_span = diag::Span::start("app.boot", "settings.load");
        let settings = AppSettings::load();
        settings_span.end(&[]);
        let cache_span = diag::Span::start("app.boot", "plugin_cache.load");
        let (plugins, fingerprints) = plugins::load_plugin_cache(settings.cache_dir.as_deref());
        cache_span.end(&[("plugins", &plugins.len().to_string())]);
        let audio = AudioRouter::new();
        let audio_span = diag::Span::start_logged("app.boot", "audio.open", &[]);
        let (audio_output, audio_status) = Self::start_audio(&audio, &settings);
        audio_span.end(&[
            (
                "status",
                if audio_output.is_some() {
                    "ok"
                } else {
                    "failed"
                },
            ),
            ("detail", &audio_status),
        ]);
        let midi = MidiInputManager::new(settings.controller_input_device.as_deref());
        let shared = Arc::new(Self {
            settings: Mutex::new(SharedSettings {
                settings,
                generation: 1,
            }),
            plugin_cache: Mutex::new(SharedPluginCache {
                plugins,
                fingerprints,
                generation: 1,
            }),
            midi: Mutex::new(midi),
            audio_output: Mutex::new(audio_output),
            audio,
            audio_status: Mutex::new(audio_status),
            next_window_id: AtomicU64::new(1),
            last_focused_window: AtomicU64::new(0),
        });
        init_span.end(&[("status", "ok")]);
        shared
    }

    /// 所属模块：app / 多窗口共享服务 / 音频
    /// 功能：按设置启动音频输出；失败时回退到系统默认设备并保留错误原因。
    /// 输入：
    /// - `router`：共享音频路由。
    /// - `settings`：包含音频系统、设备、采样率与缓冲。
    /// 输出/副作用：打开输出设备；返回输出句柄（失败为 `None`）与状态文本。
    /// 复杂度：时间 O(设备初始化)，空间 O(缓冲帧数)。
    ///
    /// Module: app / shared services / audio
    /// Purpose: Start the audio output from the settings, falling back to the system default device
    /// while keeping the error reason.
    /// Inputs:
    /// - `router`: shared audio routing.
    /// - `settings`: audio system, device, sample rate and buffer.
    /// Outputs / side effects: opens the output device; returns the handle (`None` on failure) and
    /// the status text.
    /// Complexity: O(device setup) time and O(buffer frames) space.
    fn start_audio(
        router: &Arc<AudioRouter>,
        settings: &AppSettings,
    ) -> (Option<AudioOutput>, String) {
        match start_output_with(router.clone(), &settings.audio_options()) {
            Ok(output) => {
                let description = output.description.clone();
                (Some(output), description)
            }
            Err(error) => match start_output(router.clone()) {
                Ok(output) => {
                    let description = output.description.clone();
                    (
                        Some(output),
                        format!("{error}；已回退到系统默认设备（{description}）"),
                    )
                }
                Err(fallback) => (None, format!("音频未启动: {error}（回退失败: {fallback}）")),
            },
        }
    }

    /// 所属模块：app / 多窗口共享服务 / 音频
    /// 功能：返回当前音频设备说明。
    /// 输入：`self`。
    /// 输出/副作用：状态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(状态长度)。
    ///
    /// Module: app / shared services / audio
    /// Purpose: Return the current audio device description.
    /// Inputs: `self`.
    /// Outputs / side effects: status text; no state change.
    /// Complexity: O(1) time and O(status length) space.
    pub fn audio_status(&self) -> String {
        self.audio_status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_else(|poison| poison.into_inner().clone())
    }

    /// 所属模块：app / 多窗口共享服务 / 音频
    /// 功能：按新设置停止旧输出并重新打开设备；失败时回退系统默认设备。
    /// 输入：`settings`：新的全局设置。
    /// 输出/副作用：替换输出流、更新共享状态；返回新的状态文本。
    /// 复杂度：时间 O(设备初始化)，空间 O(缓冲帧数)。
    ///
    /// Module: app / shared services / audio
    /// Purpose: Stop the old output and reopen the device from the new settings, falling back to the
    /// system default device on failure.
    /// Inputs: `settings`: the new global settings.
    /// Outputs / side effects: replaces the output stream, updates the shared status and returns it.
    /// Complexity: O(device setup) time and O(buffer frames) space.
    pub fn restart_audio(&self, settings: &AppSettings) -> String {
        let status = {
            let mut slot = self
                .audio_output
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            // 先释放旧流，驱动才能允许新流打开同一设备。
            // Release the old stream first so the driver lets the new one open the same device.
            *slot = None;
            let started = Self::start_audio(&self.audio, settings);
            *slot = started.0;
            started.1
        };
        if let Ok(mut shared) = self.audio_status.lock() {
            *shared = status.clone();
        }
        status
    }

    /// 所属模块：app / 多窗口共享服务 / MIDI
    /// 功能：按设置重新连接 MIDI 输入端口。
    /// 输入：`device`：端口名；`None` 自动选择第一个可用端口。
    /// 输出/副作用：替换 MIDI 连接并返回新的状态文本。
    /// 复杂度：时间 O(端口数)，空间 O(1)。
    ///
    /// Module: app / shared services / MIDI
    /// Purpose: Reconnect the MIDI input port from the settings.
    /// Inputs: `device`: port name; `None` selects the first available port.
    /// Outputs / side effects: replaces the MIDI connection and returns the new status text.
    /// Complexity: O(ports) time and O(1) space.
    pub fn reconnect_midi(&self, device: Option<&str>) -> String {
        self.midi
            .lock()
            .map(|mut midi| midi.set_device(device))
            .unwrap_or_else(|_| "MIDI 输入不可用".into())
    }

    /// 所属模块：app / 多窗口共享服务
    /// 功能：返回当前全局设置与版本号。
    /// 输入：`self`。
    /// 输出/副作用：设置快照；不修改状态。
    /// 复杂度：时间 O(最近项目数 + 自定义路径数)，空间 O(设置大小)。
    ///
    /// Module: app / shared services
    /// Purpose: Return the current settings and their generation.
    /// Inputs: `self`.
    /// Outputs / side effects: settings snapshot; no state change.
    /// Complexity: O(recent projects + custom paths) time and O(settings size) space.
    pub fn settings_snapshot(&self) -> (AppSettings, u64) {
        let guard = self
            .settings
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        (guard.settings.clone(), guard.generation)
    }

    /// 所属模块：app / 多窗口共享服务
    /// 功能：仅当共享设置比窗口副本更新时返回快照。
    /// 输入：`known_generation`：窗口已持有的版本号。
    /// 输出/副作用：较新的设置快照或 `None`；不修改状态。
    /// 复杂度：时间 O(1)（未更新时），空间 O(设置大小)。
    ///
    /// Module: app / shared services
    /// Purpose: Return a snapshot only when the shared settings are newer than the window copy.
    /// Inputs: `known_generation`: generation already held by the window.
    /// Outputs / side effects: a newer snapshot or `None`; no state change.
    /// Complexity: O(1) time when unchanged, O(settings size) space.
    pub fn settings_if_newer(&self, known_generation: u64) -> Option<(AppSettings, u64)> {
        let guard = self
            .settings
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        (guard.generation != known_generation).then(|| (guard.settings.clone(), guard.generation))
    }

    /// 所属模块：app / 多窗口共享服务
    /// 功能：写回全局设置：递增版本并保存到用户配置目录。
    /// 输入：`settings`：窗口修改后的设置。
    /// 输出/副作用：更新共享快照并写盘；返回新版本号。
    /// 复杂度：时间 O(设置大小)，空间 O(设置大小)。
    ///
    /// Module: app / shared services
    /// Purpose: Write settings back: bump the generation and save to the user config directory.
    /// Inputs: `settings`: settings edited by a window.
    /// Outputs / side effects: updates the shared snapshot, writes the file and returns the new
    /// generation.
    /// Complexity: O(settings size) time and space.
    pub fn write_settings(&self, settings: &AppSettings) -> u64 {
        let generation = {
            let mut guard = self
                .settings
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            guard.settings = settings.clone();
            guard.generation = guard.generation.wrapping_add(1);
            guard.generation
        };
        settings.save();
        generation
    }

    /// 所属模块：app / 多窗口共享服务
    /// 功能：返回插件缓存与版本号。
    /// 输入：`self`。
    /// 输出/副作用：缓存快照；不修改状态。
    /// 复杂度：时间 O(插件数量)，空间 O(插件数量)。
    ///
    /// Module: app / shared services
    /// Purpose: Return the plugin cache and its generation.
    /// Inputs: `self`.
    /// Outputs / side effects: cache snapshot; no state change.
    /// Complexity: O(plugin count) time and space.
    pub fn plugin_cache_snapshot(&self) -> (Vec<ScannedPlugin>, HashMap<String, (u64, u64)>, u64) {
        let guard = self
            .plugin_cache
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        (
            guard.plugins.clone(),
            guard.fingerprints.clone(),
            guard.generation,
        )
    }

    /// 所属模块：app / 多窗口共享服务
    /// 功能：仅当共享插件缓存比窗口副本更新时返回快照。
    /// 输入：`known_generation`：窗口已持有的版本号。
    /// 输出/副作用：较新的缓存或 `None`；不修改状态。
    /// 复杂度：时间 O(1)（未更新时），空间 O(插件数量)。
    ///
    /// Module: app / shared services
    /// Purpose: Return a cache snapshot only when it is newer than the window copy.
    /// Inputs: `known_generation`: generation already held by the window.
    /// Outputs / side effects: a newer snapshot or `None`; no state change.
    /// Complexity: O(1) time when unchanged, O(plugin count) space.
    pub fn plugin_cache_if_newer(
        &self,
        known_generation: u64,
    ) -> Option<(Vec<ScannedPlugin>, HashMap<String, (u64, u64)>, u64)> {
        let guard = self
            .plugin_cache
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        (guard.generation != known_generation).then(|| {
            (
                guard.plugins.clone(),
                guard.fingerprints.clone(),
                guard.generation,
            )
        })
    }

    /// 所属模块：app / 多窗口共享服务
    /// 功能：写回插件缓存：递增版本并保存到缓存文件。
    /// 输入：
    /// - `plugins`：扫描记录。
    /// - `fingerprints`：文件指纹。
    /// 输出/副作用：更新共享快照并写盘。
    /// 复杂度：时间 O(插件数量)，空间 O(插件数量)。
    ///
    /// Module: app / shared services
    /// Purpose: Write the plugin cache back: bump the generation and save the cache file.
    /// Inputs:
    /// - `plugins`: scan records.
    /// - `fingerprints`: file fingerprints.
    /// Outputs / side effects: updates the shared snapshot and writes the file.
    /// Complexity: O(plugin count) time and space.
    pub fn write_plugin_cache(
        &self,
        plugins: &[ScannedPlugin],
        fingerprints: &HashMap<String, (u64, u64)>,
    ) -> u64 {
        let generation = {
            let mut guard = self
                .plugin_cache
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            guard.plugins = plugins.to_vec();
            guard.fingerprints = fingerprints.clone();
            guard.generation = guard.generation.wrapping_add(1);
            guard.generation
        };
        // 缓存目录来自全局设置；写回时读取当前值，避免设置变更后仍写到旧目录。
        // The cache directory comes from the global settings; read it at write time so a changed
        // directory is honoured immediately.
        let cache_dir = self
            .settings
            .lock()
            .map(|guard| guard.settings.cache_dir.clone())
            .unwrap_or_default();
        plugins::save_plugin_cache(plugins, fingerprints, cache_dir.as_deref());
        generation
    }

    /// 所属模块：app / 多窗口共享服务
    /// 功能：返回 MIDI 输入状态文本。
    /// 输入：`self`。
    /// 输出/副作用：状态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(状态长度)。
    ///
    /// Module: app / shared services
    /// Purpose: Return the MIDI input status text.
    /// Inputs: `self`.
    /// Outputs / side effects: status text; no state change.
    /// Complexity: O(1) time and O(status length) space.
    pub fn midi_status(&self) -> String {
        self.midi
            .lock()
            .map(|midi| midi.status.clone())
            .unwrap_or_else(|_| "MIDI 输入不可用".into())
    }

    /// 所属模块：app / 多窗口共享服务
    /// 功能：取走当前 MIDI 事件；只有前台工程窗口会调用。
    /// 输入：`self`。
    /// 输出/副作用：返回事件列表并清空队列。
    /// 复杂度：时间 O(事件数)，空间 O(事件数)。
    ///
    /// Module: app / shared services
    /// Purpose: Take the pending MIDI events; only the foreground project window calls this.
    /// Inputs: `self`.
    /// Outputs / side effects: events are removed from the queue.
    /// Complexity: O(events) time and space.
    pub fn midi_events(&self) -> Vec<MidiKeyEvent> {
        self.midi
            .lock()
            .map(|midi| midi.drain())
            .unwrap_or_default()
    }

    /// 所属模块：app / 多窗口共享服务
    /// 功能：分配一个进程内唯一的窗口编号。
    /// 输入：`self`。
    /// 输出/副作用：返回递增编号；不涉及工程数据。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / shared services
    /// Purpose: Allocate a process-unique window id.
    /// Inputs: `self`.
    /// Outputs / side effects: an increasing id; no project data involved.
    /// Complexity: O(1) time and space.
    pub fn next_window_id(&self) -> u64 {
        self.next_window_id.fetch_add(1, Ordering::Relaxed)
    }

    /// 所属模块：app / 多窗口共享服务
    /// 功能：返回最近获得焦点的工程窗口编号；`0` 表示还没有。
    /// 输入：`self`。
    /// 输出/副作用：窗口编号；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / shared services
    /// Purpose: Return the project window that was focused last; `0` when none.
    /// Inputs: `self`.
    /// Outputs / side effects: window id; no state change.
    /// Complexity: O(1) time and space.
    pub fn last_focused_window(&self) -> u64 {
        self.last_focused_window.load(Ordering::Relaxed)
    }

    /// 所属模块：app / 多窗口共享服务
    /// 功能：记录最近获得焦点的工程窗口。
    /// 输入：`window_id`：窗口编号。
    /// 输出/副作用：更新共享编号；不修改工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / shared services
    /// Purpose: Record the project window that was focused last.
    /// Inputs: `window_id`: window id.
    /// Outputs / side effects: updates the shared id; the project is untouched.
    /// Complexity: O(1) time and space.
    pub fn set_last_focused_window(&self, window_id: u64) {
        self.last_focused_window.store(window_id, Ordering::Relaxed);
    }
}
