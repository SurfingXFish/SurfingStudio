//! 所属模块：app / 应用状态机
//! 文件用途：
//! - 定义 `SurfingApp` 的单一工程窗口状态与每帧编排顺序。
//! - 聚合文件、编辑、历史、传输、插件、输入、面板、主页、全局设置、无边框窗口与浮动卷帘子模块。
//! - 每个窗口一个 `SurfingApp`：主页窗口与各工程窗口共享 `SharedServices`。
//!
//! 函数清单：
//! - `SurfingApp::new_home` / `new_project`：创建主页窗口或工程窗口（空白 / 打开 / 模板）。
//! - `SurfingApp::build`：共享初始化（引擎、设置快照、插件缓存快照、视图、崩溃恢复线程）。
//! - `SurfingApp::ui_frame`：每帧输入 → 同步 → 渲染 → 绘制（主页/工作室）→ 关闭确认 → 撤销编排。
//! - `SurfingApp::sync_shared_state`：按版本号同步全局设置与插件缓存。
//! - `SurfingApp::resolve_native_window`：解析当前窗口的原生句柄（Windows）。
//! - `Drop::drop`：退出前关闭恢复线程并等待后台保存完成。
//!
//! Module: app / application state machine
//! Purpose:
//! - Define the per-window `SurfingApp` state and the per-frame orchestration.
//! - Aggregate file, edit, history, transport, plugin, input, panel, home, settings, window chrome,
//!   autosave and roll modules.
//! - One `SurfingApp` per window; the home window and every project window share `SharedServices`.
//!
//! Functions:
//! - `SurfingApp::new_home` / `new_project`: create the home window or a project window (empty/open/template).
//! - `SurfingApp::build`: shared initialization (engine, settings/cache snapshots, view, crash
//!   recovery thread).
//! - `SurfingApp::ui_frame`: per-frame input → sync → render → paint (home/studio) → close
//!   confirmation → undo orchestration.
//! - `SurfingApp::sync_shared_state`: pull global settings and the plugin cache when they changed.
//! - `SurfingApp::resolve_native_window`: resolve the native handle of this window (Windows).
//! - `Drop::drop`: stop the recovery thread and wait for the background save before exiting.

use crate::audio::{format_position, Engine, SlotKind};
use crate::core::model::{
    AutomationPoint, Clip, ControllerKind, InstrumentRef, Note, PluginFormat, PluginRef, Project,
    TemplateFamily, TemplateId, Track, TrackId,
};
use crate::core::project_runtime::ProjectSession;
use crate::diag;
use crate::plugin::{ParamView, PluginHost, ScannedPlugin};
use crate::ui::{theme, ArrangementDrag, BottomView, BrowserState, MouseMode, PianoDrag};
use eframe::egui;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use vst3_host::PluginWindow;

mod autosave;
mod backup;
mod edit;
mod export;
mod files;
mod heartbeat;
mod history;
mod home;
mod hotkeys;
mod input;
mod panels;
mod plugins;
use plugins::PendingPluginApply;
mod rolls;
mod settings;
mod shared;
mod shell;
mod shortcuts;
mod state;
mod time_fmt;
mod transport;
use transport::PendingAudition;
mod window_chrome;

pub use export::{ExportChannels, ExportFormat, ExportState};
pub use plugins::EditorWindow;
pub use settings::{gpu_render_startup, GpuRenderApi, GpuRenderStartup};
pub use shared::SharedServices;
pub use shell::WindowHost;
pub use shortcuts::{ScrollDirection, ShortcutAction};
pub use state::*;

/// 插件编辑器打开时的重绘间隔：持续分发插件定时器、重绘与 `idle` 消息。
/// Repaint interval while a plugin editor is open: keeps plugin timers, paints and
/// `idle` calls serviced.
const EDITOR_REPAINT_INTERVAL: Duration = Duration::from_millis(16);
/// 插件编辑器不在前台时的强制重绘间隔：插件自身通常有绘制循环，宿主只做低频兜底，
/// 每帧 `RedrawWindow` 会与 Kontakt 8 的 Qt Quick 绘制互相拖住。
/// Forced repaint interval for a background plugin editor: plugins normally run their own paint
/// loop, so the host only nudges them at a low rate; a per-frame `RedrawWindow` fights Kontakt 8's
/// Qt Quick painter.
const EDITOR_REPAINT_BACKGROUND_INTERVAL: Duration = Duration::from_millis(200);
/// 窗口关闭时等待音频/后台线程释放引擎引用的最长时间。
/// Maximum time spent waiting for the audio/background threads to release the engine on close.
const ENGINE_RELEASE_WAIT: Duration = Duration::from_millis(200);

pub(super) struct SaveOutcome {
    pub path: PathBuf,
    pub edit_seq: u64,
    pub project_epoch: u64,
    /// 是否为自动保存：不写“最近项目”，状态栏显示“已自动保存”。
    /// Whether this was an autosave: skips the recent list and says so in the status bar.
    pub autosave: bool,
    pub result: Result<(), String>,
}

/// 后台加载/导入返回的工程会话（框架统一结果）。
/// Project session returned by the background load/import worker (unified framework result).
type ProjectSessionPayload = ProjectSession;

pub struct SurfingApp {
    /// 与其他窗口共享的设置、插件缓存、MIDI 与音频路由。
    /// Settings, plugin cache, MIDI and audio routing shared with the other windows.
    pub shared: Arc<SharedServices>,
    /// 进程内唯一的窗口编号；用于视口 id、浮动卷帘 id 与空格快捷键归属。
    /// Process-unique window id used by viewport ids, roll viewport ids and Space ownership.
    pub window_id: u64,
    /// 本窗口标题；Windows 下用它定位原生窗口句柄。
    /// This window's title; used to resolve the native handle on Windows.
    pub window_title: String,
    /// 窗口级动作（打开主页 / 打开工程窗口）；由外壳每帧消费。
    /// Window-level actions (show home / open project window); consumed by the shell each frame.
    pub pending_window_actions: Vec<WindowAction>,
    /// 主页“继续当前工程”是否可用；由外壳更新。
    /// Whether the home page's "resume project" action is available; updated by the shell.
    pub can_resume_project: bool,
    /// 当前打开的工程窗口数量；由外壳更新，供主页展示。
    /// Number of open project windows; updated by the shell for the home page.
    pub open_project_count: usize,
    /// 本地设置与插件缓存快照对应的共享版本号。
    /// Shared generations that the local settings/cache snapshots correspond to.
    settings_generation: u64,
    plugin_cache_generation: u64,
    /// MIDI 输入状态文本快照。
    /// Snapshot of the MIDI input status text.
    pub midi_status: String,
    /// 后台文件夹选择（设置中的自定义 VST 路径）。
    /// Background folder picker for custom VST paths in the settings window.
    pub folder_rx: Option<Receiver<Option<PathBuf>>>,
    pub project: Project,
    pub engine: Arc<Mutex<Engine>>,
    pub audio_status: String,
    pub status: String,
    pub project_path: Option<PathBuf>,
    pub dirty: bool,
    pub needs_sync: bool,
    /// 关闭窗口前的未保存确认是否显示；显示期间窗口保持打开。
    /// Whether the unsaved-changes confirmation is shown; the window stays open meanwhile.
    close_prompt: bool,
    /// 保存完成或用户放弃修改后，下一帧发送 `ViewportCommand::Close`。
    /// Next frame sends `ViewportCommand::Close` after a save or an explicit discard.
    pending_close: bool,
    /// 加载/插件同步期间收到关闭请求：等同步结束后再发送关闭。
    /// A close arrived while loading or syncing: close once the background work finishes.
    close_after_ready: bool,
    /// 延迟关闭的等待上限；超过后强制关闭并放弃引擎，避免插件加载卡死窗口。
    /// Upper bound for the deferred close; after it the window force-closes and abandons the
    /// engine so a hung plugin load cannot freeze the window.
    close_ready_deadline: Option<Instant>,
    /// 强制关闭时泄漏引擎强引用，避免挂起的后台线程在退出阶段析构未初始化完成的插件。
    /// When force-closing, leak one engine reference so a hung worker cannot destroy a partially
    /// initialised plugin during shutdown.
    abandon_engine: bool,
    /// 上一次自动保存的起点；仅在工程有路径且存在未保存修改时推进。
    /// Start of the last autosave; only advanced when the project has a path and is dirty.
    last_autosave: Instant,
    /// 崩溃恢复线程（仅工程窗口创建）；窗口关闭时随句柄析构退出。
    /// Crash-recovery thread (created for project windows only); exits when its handle drops.
    recovery: Option<autosave::RecoveryWorker>,
    /// 当前崩溃恢复文件路径；用于工程保存/放弃后清理。
    /// Current crash-recovery file path, removed once the project is saved or discarded.
    recovery_path: Option<PathBuf>,
    /// 已推送的（工程代次, 编辑序号）；避免重复克隆同一状态。
    /// Last pushed (project epoch, edit sequence); avoids cloning unchanged state.
    last_recovery_marker: Option<(u64, u64)>,
    /// 上一次推送恢复快照的时间，限制克隆频率。
    /// Time of the last recovery-snapshot push, throttling the clone.
    last_recovery_push: Instant,
    /// 未保存工程生成临时恢复文件后，是否已提示过一次。
    /// Whether the temporary recovery file of a never-saved project was announced once.
    recovery_notice_shown: bool,

    pub selected_track: usize,
    pub selected_clip: Option<usize>,
    /// (clip, note) index of the primary selected note.
    pub selected_note: Option<(usize, usize)>,
    /// Multi-selection: (track, clip) pairs, may span tracks.
    pub selected_clips: Vec<(usize, usize)>,
    /// Multi-selection: (clip, note) pairs, may span clips of one track.
    pub selected_notes: Vec<(usize, usize)>,
    /// Currently selected effect slot (track, index) for Delete.
    pub selected_effect: Option<(TrackId, usize)>,
    /// Currently selected controller lane (track, kind) for Delete.
    pub selected_controller: Option<(TrackId, ControllerKind)>,
    /// Currently selected effects child lane (track id).
    pub selected_effects_lane: Option<TrackId>,
    /// Pending overwrite confirmation for a child-lane paste.
    pub lane_paste_prompt: Option<LanePastePrompt>,
    /// 浮动编辑器卷帘，一个轨道最多一个。
    /// Floating editor rolls, one per track.
    pub roll_windows: Vec<RollWindow>,
    /// 本帧是否有浮动卷帘窗口获得焦点；用于避免空格被父窗口与卷帘重复处理。
    /// Whether a floating roll window is focused this frame; prevents Space from being handled
    /// twice by the parent window and the roll.
    pub roll_window_focused: bool,
    /// Active toolbar drag that pulls a roll out of the bottom panel.
    pub roll_drag: Option<RollDragState>,
    pub clipboard: Clipboard,
    /// Active pointer tool for the arrangement and both rolls.
    pub mouse_mode: MouseMode,

    pub view_start: f64,
    pub visible_beats: f64,
    pub snap_beats: f64,

    pub arr_drag: ArrangementDrag,
    pub piano_drag: PianoDrag,
    pub arr_scroll: f32,
    /// Vertical zoom of arrangement rows (Ctrl+Shift+wheel).
    pub arr_row_scale: f32,
    pub piano_top_pitch: i32,
    /// Vertical zoom (row height in points) of the piano roll.
    pub piano_note_h: f32,
    /// Horizontal zoom of the piano roll (1.0 fits the whole clip).
    pub piano_zoom: f32,
    /// Left edge of the piano roll view, in clip beats.
    pub piano_view_beat: f64,
    /// Scale guidance in the piano roll.
    pub scale_root: i32,
    /// Index into the piano-roll scale list (major, natural minor, …).
    pub scale_mode: usize,
    /// Whether the pinned master bus is shown in the arrangement.
    pub show_master: bool,
    /// Height of the pinned master bus in points. It is independent of the
    /// track row zoom and is adjusted by dragging the master's bottom edge.
    pub master_height: f32,
    /// Per-track piano-roll view state.
    pub piano_views: HashMap<TrackId, PianoViewState>,
    /// Track currently shown in the piano roll, for fit-on-open.
    pub piano_track: Option<TrackId>,
    /// 钢琴卷帘底部是否显示力度控制器；仅 UI 状态，不持久化。
    /// Whether the piano roll shows its velocity lane; UI-only and never persisted.
    pub piano_velocity_open: bool,

    /// 当前工作区：主页或工作室；仅 UI 状态，不持久化。
    /// Current workspace: home page or studio; UI-only and never persisted.
    pub view: WorkspaceView,
    /// 主页右侧内容当前对应的左侧菜单项。
    /// Home sidebar entry that selects the right-hand content page.
    pub home_page: HomePage,
    /// 主页项目搜索文本。
    /// Project search text on the home page.
    pub home_search: String,
    /// 主页插件搜索文本。
    /// Plugin search text on the home page.
    pub home_plugin_search: String,
    /// 主页模板搜索文本。
    /// Template search text on the home page.
    pub home_template_search: String,
    /// 主页模板家族筛选；`None` 表示全部。
    /// Template-family filter on the home page; `None` means all.
    pub home_template_family: Option<TemplateFamily>,
    /// 主页项目列表中已展开的项目（可同时展开多个）。
    /// Projects whose detail panel is expanded on the home page (several may be open).
    pub home_project_expanded: HashSet<PathBuf>,
    /// 项目信息缓存：路径 → 解析结果；展开时后台加载一次。
    /// Project-info cache: path → parse result; loaded once in the background when expanded.
    home_project_info: HashMap<PathBuf, Result<home::HomeProjectSummary, String>>,
    /// 正在加载的项目信息（路径 + 后台通道）。
    /// Project info currently loading (path + worker channel).
    home_project_info_rx: Option<(PathBuf, Receiver<Result<home::HomeProjectSummary, String>>)>,
    /// 上一次清理失效项目的时间；避免每帧对全部项目做存在性检查。
    /// Last time missing projects were pruned; avoids checking every path every frame.
    home_project_sweep_at: Instant,
    /// 主页插件页中被收起的品牌；默认全部展开。
    /// Brands collapsed on the home plugin page; every brand is expanded by default.
    pub home_plugin_collapsed: HashSet<String>,
    /// 主页插件页中展开详细信息的插件路径。
    /// Plugin paths whose details are expanded on the home plugin page.
    pub home_plugin_expanded: HashSet<PathBuf>,
    /// 独立设置窗口是否打开。
    /// Whether the standalone settings window is open.
    pub settings_open: bool,
    /// 设置窗口左侧选中的分类。
    /// Currently selected settings category in the left column.
    settings_page: settings::SettingsPage,
    /// 后台文件夹选择的目标用途。
    /// Purpose of the background folder picker.
    settings_folder_target: Option<settings::SettingsFolderTarget>,
    /// 设置 → 音频页的音频系统缓存；进入页面或手动刷新时重建。
    /// Audio-system cache of the settings audio page; rebuilt on entry or manual refresh.
    settings_audio_systems: Option<Vec<crate::audio::backend::AudioSystemInfo>>,
    /// 设置 → 音频页的设备缓存：音频系统、输出设备、输入设备。
    /// Device cache of the settings audio page: system, outputs and inputs.
    settings_audio_devices: Option<(crate::audio::AudioSystem, Vec<String>, Vec<String>)>,
    /// 设置 → 控制器页的 MIDI 端口缓存：输入端口、输出端口。
    /// MIDI-port cache of the settings controller page: inputs and outputs.
    settings_midi_ports: Option<(Vec<String>, Vec<String>)>,
    /// 设置 → 渲染页的 GPU 设备缓存；进入页面时枚举，手动刷新时重建。
    /// GPU-device cache of the settings render page; enumerated on entry and rebuilt on refresh.
    settings_gpu_devices: Option<Result<Vec<crate::audio::gpu::GpuDeviceInfo>, String>>,
    /// 设置窗口是否需要在本帧置前（只生效一次）。
    /// Whether the settings window should be raised this frame (one shot).
    settings_focus: bool,
    /// 全局设置（界面缩放、启动主页、最近项目、快捷键）。
    /// Global settings (UI scale, startup home page, recent projects, shortcuts).
    pub settings: settings::AppSettings,
    /// 正在录制新快捷键的动作；仅 UI 状态，不持久化。
    /// Action currently recording a new shortcut; UI-only and never persisted.
    pub hotkey_recording: Option<ShortcutAction>,
    /// 快捷键录制提示（冲突或已保存）；仅 UI 状态。
    /// Shortcut-recording hint (conflict or saved); UI-only.
    pub hotkey_hint: Option<String>,

    pub show_help: bool,
    /// 应用栏主菜单（文件 / 编辑 / VST 插件 / 帮助）是否展开；仅 UI 状态，不持久化。
    /// Whether the app bar main menu (file / edit / VST / help) is expanded; UI-only, never persisted.
    pub menu_bar_visible: bool,
    pub browser: Option<BrowserState>,
    pub plugin_cache: Vec<ScannedPlugin>,
    /// 插件缓存文件中的文件指纹（路径小写 → 修改时间秒 + 字节数），用于启动时的增量扫描。
    /// File fingerprints from the plugin cache (lower-case path → mtime seconds + size) used by
    /// the incremental startup scan.
    plugin_cache_fingerprints: HashMap<String, (u64, u64)>,
    /// 乐器窗口状态；打开时显示左侧种类与右侧乐器列表。
    /// Instrument window state; when open it shows categories on the left and instruments on
    /// the right.
    pub instrument_picker: Option<InstrumentPicker>,
    pub scan_rx: Option<Receiver<Result<plugins::ScanOutcome, String>>>,
    pub scanning: bool,
    /// 扫描完成后按需识别插件类型的后台通道。
    /// Background channel that classifies plugin types after a fast scan.
    plugin_probe_rx: Option<Receiver<ScannedPlugin>>,
    /// 是否仍在后台识别插件类型。
    /// Whether plugin types are still being classified in the background.
    pub classifying: bool,
    /// Background project loading.
    load_rx: Option<Receiver<Result<ProjectSessionPayload, String>>>,
    /// Native file dialog running on a worker thread so the UI never freezes.
    dialog_rx: Option<Receiver<Result<Option<PathBuf>, String>>>,
    /// Parsed project waiting one frame so the loading prompt is painted first.
    loaded_project: Option<ProjectSessionPayload>,
    /// Background save; the snapshot is already owned by the writer thread.
    save_rx: Option<Receiver<SaveOutcome>>,
    /// 后台把工程 / WAV 副本写到公共目录（SAF 或“另存为”）；结果只用于状态栏。
    /// Background copy of a project/WAV snapshot to a public folder (SAF or save-as); the result
    /// only feeds the status bar.
    public_copy_rx: Option<Receiver<Result<Option<String>, String>>>,
    /// 独立导出窗口的状态（格式、参数、路径与后台任务）。
    /// Standalone export-window state (format, options, path and background job).
    pub export: ExportState,
    /// 后台选择导出路径的对话框结果。
    /// Background destination picker for the export window.
    export_dialog_rx: Option<Receiver<Option<PathBuf>>>,
    /// 后台插件同步：加载工程时在独立线程构建插件运行时，避免 UI 冻结。
    /// Background plugin sync used when loading a project; keeps the UI responsive.
    engine_sync_rx: Option<Receiver<(Vec<String>, Vec<PathBuf>)>>,
    /// 常驻同步线程的任务入口；线程同时泵送插件创建的消息窗口。
    /// Request channel of the persistent sync thread, which also pumps messages
    /// for plugin-created windows.
    engine_sync_tx: Option<Sender<self::transport::SyncRequest>>,
    /// 下一次同步是否放到后台线程（加载工程时启用）。
    /// Whether the next engine sync should run in the background.
    sync_in_background: bool,
    /// 当前正在进行的同步是否由后台线程执行（用于标记后台创建的插件实例）。
    /// Whether the in-flight sync runs on the worker thread (used to mark worker-created
    /// plugin instances).
    background_sync_in_flight: bool,
    /// 加载工程后仍在分批补载 VST 乐器插件；为真时每帧只加载一个时间片，保证窗口持续重绘。
    /// VST instrument plugins are still being loaded in per-frame slices after a project load;
    /// while true each frame loads at most one slice so the window keeps repainting.
    instrument_load_pending: bool,
    /// 分批补载期间累计的插件加载警告，全部就绪后写入状态栏。
    /// Plugin load warnings accumulated during the sliced load; shown in the status bar once done.
    instrument_load_warnings: Vec<String>,
    /// 分批补载涉及的乐器插件总数，用于进度提示。
    /// Number of instrument plugins involved in the sliced load, used for the progress prompt.
    instrument_load_total: usize,
    /// 分批补载开始的时刻，用于记录整体耗时。
    /// Start instant of the sliced load, used to log the overall duration.
    instrument_load_started: Instant,
    /// Incremented whenever the edited project is replaced (new/open), so a
    /// save finishing late cannot overwrite the new project's path or dirty
    /// state.
    project_epoch: u64,
    loading_message: Option<String>,
    /// While set, keep the loading prompt on screen until this instant so it is
    /// actually painted even when loading finishes within one frame.
    loading_until: Option<Instant>,
    defer_sync: bool,
    /// The UI project changed but plugin loading was postponed until playback.
    engine_stale: bool,
    pending_play: bool,
    /// Throttles project syncs while playing, so continuous edits do not
    /// repeatedly block the audio callback.
    last_engine_sync: Instant,

    pub editors: Vec<((TrackId, SlotKind), EditorWindow)>,
    /// 上一次强制重绘插件编辑器的时刻；限制后台编辑器的 `RedrawWindow` 频率，避免与
    /// Kontakt 等插件自身的绘制循环互相拖住。
    /// Last forced repaint of each plugin editor; throttles `RedrawWindow` for background editors
    /// so it never fights a plugin's own paint loop (Kontakt).
    editor_repaint: HashMap<(TrackId, SlotKind), Instant>,
    /// 本帧已摆放的内嵌插件编辑器槽位；帧末未摆放的会被隐藏。
    /// Slots whose inline editor was placed this frame; unplaced ones are hidden at frame end.
    inline_placed: HashSet<(TrackId, SlotKind)>,
    /// 内嵌失败的槽位及原因：避免每帧重试造成卡顿。
    /// Slots whose inline editor failed, with the reason; prevents retrying every frame.
    pub inline_failed: HashMap<(TrackId, SlotKind), String>,
    /// 每个槽位是否提供原生界面；引擎锁忙时沿用缓存，避免误显示“没有界面”。
    /// Whether a slot offers a native editor; cached so a busy engine lock cannot make the
    /// card claim there is no GUI.
    pub effect_editor_flags: HashMap<(TrackId, SlotKind), bool>,
    /// 等待后台同步完成的插件应用收尾信息。
    /// Pending post-load bookkeeping for a plugin applied to a slot.
    pending_plugin_apply: Option<PendingPluginApply>,
    /// 由后台同步线程创建的插件槽位：打开 GUI 前需要在 UI 线程重建实例。
    /// Slots whose instance was created by the background sync thread; they must be rebuilt on
    /// the UI thread before their editor opens.
    background_synced: HashSet<(TrackId, SlotKind)>,
    /// 后台插件类型识别的取消标志。
    /// Cancellation flag of the background plugin classification worker.
    plugin_probe_cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,

    pub audition_track: Option<TrackId>,
    pub held_pitches: HashSet<u8>,
    /// 引擎锁被音频回调占用时暂存的试听事件，下一帧按顺序补发。
    /// Audition events queued while the audio callback holds the engine lock; replayed in order on
    /// a later frame.
    pending_auditions: Vec<PendingAudition>,
    keyboard_held: HashMap<egui::Key, u8>,
    pub virtual_key_note: Option<u8>,
    pub style_ready: bool,

    /// Cached transport/level values for the UI. Reading them with a blocking
    /// lock every frame can steal the engine lock from the audio callback.
    pub ui_playhead: f64,
    pub ui_playing: bool,
    pub ui_levels: Vec<f32>,
    pub ui_levels_l: Vec<f32>,
    pub ui_levels_r: Vec<f32>,
    pub ui_master_levels: (f32, f32),
    pub ui_waveforms: HashMap<PathBuf, Vec<f32>>,
    pub ui_cpu_load: f32,
    pub ui_auto_stopped: bool,
    /// Audio sources are being decoded in the background.
    pub ui_audio_loading: bool,

    pub bottom_view: BottomView,
    /// Whether the bottom editor roll is visible.
    pub bottom_open: bool,
    pub controller_tool: crate::ui::controller::Tool,
    pub controller_line_start: Option<AutomationPoint>,
    pub controller_selection: Vec<usize>,
    pub controller_marquee_start: Option<egui::Pos2>,
    pub raw_window: Option<RawWindowHandle>,
    pub raw_display: Option<RawDisplayHandle>,
    /// 无边框窗口是否由应用接管为最大化（Windows 下铺满显示器工作区）。
    /// Whether the borderless window is maximized by the app (fills the monitor work area on Windows).
    pub window_maximized: bool,
    /// 最大化前的窗口外框（逻辑点，monitor 空间）：x、y、宽、高。
    /// Outer rect before maximizing (points, monitor space): x, y, width, height.
    window_restore_rect: Option<(f32, f32, f32, f32)>,
    /// 还原窗口后的下一帧跳过外框记录，避免把仍为最大化尺寸的旧外框写回还原矩形。
    /// Skip outer-rect bookkeeping for one frame after restoring, so a stale maximized rect is
    /// not written back as the restore rect.
    window_restore_guard: bool,
    pub effects_scroll: f32,
    /// Cached plugin parameter lists. Reading them from the engine every UI
    /// frame would hold the engine lock while cloning hundreds of parameter
    /// strings, which starves the audio callback.
    plugin_param_cache: HashMap<(TrackId, SlotKind), Vec<ParamView>>,
    plugin_param_cache_at: HashMap<(TrackId, SlotKind), Instant>,
    /// Per-card width overrides for the effect chain roll.
    pub effect_widths: HashMap<(TrackId, usize), f32>,
    /// Cached editor-resizable flags for effect slots.
    pub effect_can_resize: HashMap<(TrackId, usize), bool>,
    pub effect_drag_from: Option<usize>,
    pub effect_drag_target: Option<usize>,
    last_auto_stopped: bool,

    undo_stack: Vec<Project>,
    redo_stack: Vec<Project>,
    pending_undo: Option<Project>,
    pending_undo_seq: u64,
    edit_seq: u64,
    last_undo_at: Option<Instant>,
}

impl Drop for SurfingApp {
    /// 所属模块：app / 窗口析构
    /// 功能：窗口关闭时取消输出目标、等待引擎引用释放、停止后台线程并销毁编辑器窗口。
    /// 输入：`self`。
    /// 输出/副作用：引擎（含插件实例）在本线程析构；必要时泄漏引擎/编辑窗口/撤销栈以避开插件死锁；
    /// 每个阶段写一条 `window.drop` 日志，卡死时最后一条记录即现场。
    /// 复杂度：时间 O(编辑器数 + 撤销栈大小)（插件析构另计），空间 O(1)。
    ///
    /// Module: app / window teardown
    /// Purpose: On window close, retire the output target, wait for engine references, stop worker
    /// threads and destroy editor windows.
    /// Inputs: `self`.
    /// Outputs / side effects: the engine (with plugin instances) is destroyed on this thread;
    /// the engine/editors/undo stacks may be leaked to avoid plugin deadlocks; every step writes a
    /// `window.drop` record so a hang leaves the last step in the log.
    /// Complexity: O(editors + undo stacks) time (plugin teardown excluded) and O(1) space.
    fn drop(&mut self) {
        let started = Instant::now();
        let window_text = self.window_id.to_string();
        let tid_text = diag::thread_id().to_string();
        // 窗口在导出期间关闭：先取消会话，恢复引擎状态并释放插件视图。
        // The window closed during an export: cancel the session and restore the engine first.
        if self.export.is_rendering() {
            self.cancel_export();
        }
        let instrument_count = self
            .project
            .tracks
            .iter()
            .filter(|track| matches!(track.instrument, InstrumentRef::Plugin(_)))
            .count();
        let effect_count: usize = self
            .project
            .tracks
            .iter()
            .map(|track| {
                track
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
        let instrument_text = instrument_count.to_string();
        let effect_text = effect_count.to_string();
        let plugins_total = instrument_count + effect_count;
        let plugins_text = plugins_total.to_string();
        diag::info(
            "app.window",
            "window.drop",
            &[
                ("phase", "begin"),
                ("id", &window_text),
                ("tid", &tid_text),
                ("plugin_instruments", &instrument_text),
                ("plugin_effects", &effect_text),
                (
                    "abandon",
                    if self.abandon_engine { "true" } else { "false" },
                ),
            ],
        );
        // 先取消本窗口的输出目标并短暂等待音频回调/后台同步释放引擎引用：
        // 引擎由 UI 线程最后析构，插件实例（尤其是 VOCALOID 的 WPF 窗口）不会在
        // 音频或后台线程被销毁。
        // First drop this window as the output target and briefly wait for the audio callback and
        // background sync to release their engine references: the engine is then destroyed on the
        // UI thread, so plugin instances (notably VOCALOID's WPF window) never die on the audio or
        // worker thread.
        self.shared.audio.unregister(&self.engine);
        diag::info(
            "app.window",
            "window.drop",
            &[("phase", "audio_deactivated"), ("tid", &tid_text)],
        );
        if let Ok(mut engine) = self.engine.try_lock() {
            engine.pause();
        }
        let release_start = Instant::now();
        let deadline = Instant::now() + ENGINE_RELEASE_WAIT;
        while Arc::strong_count(&self.engine) > 1 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        let release_text = Arc::strong_count(&self.engine).to_string();
        let release_ms = release_start.elapsed().as_millis().to_string();
        diag::info(
            "app.window",
            "window.drop",
            &[
                ("phase", "engine_refs"),
                ("strong_count", &release_text),
                ("waited_ms", &release_ms),
                ("tid", &tid_text),
            ],
        );
        // Disconnect the persistent plugin-sync thread; it exits after any
        // in-flight sync request.
        self.engine_sync_tx = None;
        // 停止崩溃恢复线程：句柄析构会发送 Shutdown，线程写完当前文件后退出。
        // Stop the crash-recovery thread: dropping the handle sends Shutdown, so the thread exits
        // after finishing its current write.
        self.recovery = None;
        diag::info(
            "app.window",
            "window.drop",
            &[("phase", "workers_stopped"), ("tid", &tid_text)],
        );
        // 先终止 VST3 组件，再销毁可能仍附加着的编辑器窗口：VOCALOID / ACE Bridge 等插件
        // 拉起的独立应用窗口会随 `IComponent::terminate` 关闭；`close_editor` 也在窗口仍有效时
        // 执行，避免 `removed()` 访问已销毁的原生窗口。
        // Terminate VST3 components before destroying any editor window that is still attached:
        // plugins such as VOCALOID / ACE Bridge close their spawned app windows on
        // `IComponent::terminate`, and `close_editor` runs while the native window is still valid.
        // 音频回调可能刚好持锁：短暂重试，确保组件一定被终止（外部应用窗口随之关闭）。
        // The audio callback may hold the lock for a moment: retry briefly so the component is
        // always terminated (and its spawned app windows close with it).
        if !self.terminate_engine_plugins(Duration::from_millis(200)) {
            diag::warn(
                "app.window",
                "window.drop",
                &[("phase", "plugins_terminate_timeout"), ("tid", &tid_text)],
            );
        }
        diag::info(
            "app.window",
            "window.drop",
            &[("phase", "plugins_terminated"), ("tid", &tid_text)],
        );
        let editor_count = self.editors.len().to_string();
        for (_, mut window) in self.editors.drain(..) {
            window.destroy_for_exit();
            std::mem::forget(window);
        }
        diag::info(
            "app.window",
            "window.drop",
            &[
                ("phase", "editors_destroyed"),
                ("editors", &editor_count),
                ("tid", &tid_text),
            ],
        );
        // Leaking the audio stream avoids driver-side stop deadlocks; the OS
        // closes the device when the process exits.
        // 音频流由共享服务持有：单个窗口关闭时不停止设备，避免驱动侧死锁。
        // The audio stream is owned by the shared services: closing one window must not stop the
        // device, which can deadlock inside the driver.
        // Large project snapshots do not need deallocation during shutdown.
        std::mem::forget(std::mem::take(&mut self.undo_stack));
        std::mem::forget(std::mem::take(&mut self.redo_stack));
        diag::info(
            "app.window",
            "window.drop",
            &[("phase", "history_leaked"), ("tid", &tid_text)],
        );
        // Saving is asynchronous; closing the window immediately after Ctrl+S
        // must still let the writer finish, but never block shutdown forever.
        if let Some(rx) = self.save_rx.take() {
            let wait_start = Instant::now();
            let _ = rx.recv_timeout(Duration::from_secs(3));
            let waited = wait_start.elapsed().as_millis().to_string();
            diag::info(
                "app.window",
                "window.drop",
                &[
                    ("phase", "save_waited"),
                    ("waited_ms", &waited),
                    ("tid", &tid_text),
                ],
            );
        }
        // 插件实例的析构不可中断：VOCALOID VSTi / ACE Bridge 等插件会在析构里等待 UI 消息循环，
        // 在 UI 线程直接析构会永久卡死；换到其他线程析构又可能触发 0xC0000005。
        // 因此只要工程仍持有插件实例，就保留引擎（连同插件实例）直到进程结束，只把窗口从
        // 音频路由中摘除；纯内置乐器的工程仍正常析构。
        // Plugin teardown cannot be interrupted: plugins such as VOCALOID VSTi / ACE Bridge wait for
        // the UI message loop inside their destructors, so destroying them on the UI thread hangs
        // forever, while another thread may raise 0xC0000005. Whenever the project still owns plugin
        // instances, the engine (and its instances) is retained until the process ends and only the
        // window is removed from the audio router. Built-in-only projects are still torn down.
        let retain_engine = self.abandon_engine || plugins_total > 0;
        if retain_engine {
            // 泄漏一个强引用：即使后台线程稍后释放自己的引用，引擎与插件实例也不会析构。
            // Leak one strong reference so the engine and its plugin instances survive even when a
            // background thread later releases its own reference.
            std::mem::forget(self.engine.clone());
            diag::warn(
                "app.window",
                "window.drop",
                &[
                    ("phase", "engine_retained"),
                    (
                        "reason",
                        if self.abandon_engine {
                            "forced_close"
                        } else {
                            "plugin_teardown"
                        },
                    ),
                    ("plugins", &plugins_text),
                    ("tid", &tid_text),
                ],
            );
        } else {
            // 引擎字段（及其中的内置乐器）在本函数返回之后才析构：先留下最后一条记录，
            // 若日志停在此处，说明卡死发生在内置乐器析构中。
            // The engine field (with its built-in instruments) is dropped after this function
            // returns: the final record marks that boundary, so a log ending here means built-in
            // teardown hung.
            let elapsed_text = started.elapsed().as_millis().to_string();
            diag::info(
                "app.window",
                "window.drop",
                &[
                    ("phase", "engine_drop_next"),
                    ("body_ms", &elapsed_text),
                    ("plugin_effects", &effect_text),
                    ("tid", &tid_text),
                ],
            );
        }
    }
}

impl SurfingApp {
    /// 所属模块：app / 插件生命周期
    /// 功能：在限定时间内取得引擎锁并终止全部 VST3 组件，让插件拉起的外部应用窗口随工程关闭。
    /// 输入：`timeout`：最长等待时间。
    /// 输出/副作用：停止处理并调用 `IComponent::terminate`；返回是否完成插件收尾（终止或安全跳过）。
    /// 复杂度：时间 O(等待 + 插件终止)，空间 O(1)。
    ///
    /// Module: app / plugin lifecycle
    /// Purpose: Acquire the engine lock within the timeout and terminate every VST3 component so
    /// plugin-spawned external app windows close with the project.
    /// Inputs: `timeout`: maximum wait.
    /// Outputs / side effects: stops processing and calls `IComponent::terminate`; returns whether
    /// plugin shutdown completed (terminated or safely skipped).
    /// Complexity: O(wait + plugin termination) time and O(1) space.
    fn terminate_engine_plugins(&mut self, timeout: Duration) -> bool {
        // 先请求插件自己拉起的顶层窗口（VOCALOID6 Editor 等）关闭：这是插件的正常关闭路径，
        // 不会像 `IComponent::terminate` 那样在 WPF 的 OnClosing 里抛空引用异常并卡住 UI。
        // First ask plugin-spawned top-level windows (VOCALOID6 Editor etc.) to close: this is the
        // plugin's normal close path and avoids the WPF `OnClosing` null-reference exception that
        // `IComponent::terminate` triggers on the UI thread.
        let (closed_windows, all_closed) = self.close_plugin_owned_windows(Duration::from_secs(3));
        if closed_windows > 0 {
            diag::info(
                "app.window",
                "window.drop",
                &[
                    ("phase", "plugin_windows_closed"),
                    ("windows", &closed_windows.to_string()),
                    ("all_closed", if all_closed { "true" } else { "false" }),
                ],
            );
        }
        if !all_closed {
            // 插件窗口拒绝关闭（例如弹出模态对话框）：此时终止组件会把 UI 线程永久卡住。
            // 保留实例（窗口关闭路径本就会泄漏引擎），只记录告警。
            // A plugin window refused to close (for example a modal dialog is open): terminating the
            // component would hang the UI thread forever. Keep the instance (the window-close path
            // already leaks the engine) and log a warning.
            diag::warn(
                "app.window",
                "window.drop",
                &[("phase", "plugin_windows_still_open")],
            );
            return true;
        }
        let deadline = Instant::now() + timeout;
        loop {
            match self.engine.try_lock() {
                Ok(mut engine) => {
                    engine.shutdown_plugins();
                    return true;
                }
                Err(std::sync::TryLockError::Poisoned(poison)) => {
                    poison.into_inner().shutdown_plugins();
                    return true;
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    if Instant::now() >= deadline {
                        return false;
                    }
                    std::thread::yield_now();
                }
            }
        }
    }

    /// 所属模块：app / 插件生命周期
    /// 功能：向本进程内由插件创建的可见顶层窗口发送 `WM_CLOSE`，并泵消息等待它们关闭。
    /// 输入：`timeout`：最长等待时间。
    /// 输出/副作用：关闭插件窗口；返回（已发送关闭请求的窗口数，是否全部关闭）。
    /// 复杂度：时间 O(窗口数 × 等待迭代)，空间 O(窗口数)。
    ///
    /// Module: app / plugin lifecycle
    /// Purpose: Send `WM_CLOSE` to visible top-level windows created by plugins in this process and
    /// pump messages until they close.
    /// Inputs: `timeout`: maximum wait.
    /// Outputs / side effects: closes plugin windows; returns (close requests sent, all closed).
    /// Complexity: O(windows × wait iterations) time and O(windows) space.
    #[cfg(target_os = "windows")]
    fn close_plugin_owned_windows(&self, timeout: Duration) -> (usize, bool) {
        use winapi::shared::minwindef::{BOOL, DWORD, LPARAM};
        use winapi::shared::windef::HWND;
        use winapi::um::winuser::{
            DispatchMessageW, EnumWindows, GetWindow, GetWindowTextLengthW, GetWindowTextW,
            GetWindowThreadProcessId, IsWindowVisible, MsgWaitForMultipleObjectsEx, PeekMessageW,
            PostMessageW, TranslateMessage, GW_OWNER, MSG, MWMO_INPUTAVAILABLE, PM_REMOVE,
            QS_ALLINPUT, WM_CLOSE,
        };

        unsafe fn title_of(hwnd: HWND) -> Option<String> {
            let length = unsafe { GetWindowTextLengthW(hwnd) };
            if length <= 0 {
                return None;
            }
            let mut buffer = vec![0u16; length as usize + 1];
            let written = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
            (written > 0).then(|| String::from_utf16_lossy(&buffer[..written as usize]))
        }

        unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let state = unsafe { &mut *(lparam as *mut (DWORD, Vec<HWND>)) };
            let mut pid: DWORD = 0;
            unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
            // 只看可见窗口：winit/egui 会创建若干隐藏辅助窗口，它们不会响应 WM_CLOSE，
            // 若把它们算进来，关闭工程时会永远“有窗口未关闭”而跳过 terminate。
            // Only visible windows count: winit/egui create hidden helper windows that ignore
            // WM_CLOSE; counting them would always report "windows still open" and skip terminate.
            if pid != state.0 || unsafe { IsWindowVisible(hwnd) } == 0 {
                return 1;
            }
            let Some(title) = (unsafe { title_of(hwnd) }) else {
                return 1;
            };
            // 跳过宿主自己的窗口：标题统一带 SurfingStudio。宿主创建的插件编辑器窗口以工程
            // 窗口为 owner（标题同样带 SurfingStudio），也一并跳过，避免多窗口时关掉其它
            // 工程的编辑器；插件自建窗口的 owner 是插件的隐藏窗口，仍会被关闭。
            // Skip the host's own windows: their titles contain SurfingStudio. Host-created plugin
            // editors are owned by a project window (whose title also contains SurfingStudio) and
            // are skipped, so closing one project never closes another project's editor. A
            // plugin-created window is owned by the plugin's hidden window and is still targeted.
            if title.contains("SurfingStudio") {
                return 1;
            }
            let owner = unsafe { GetWindow(hwnd, GW_OWNER) };
            if !owner.is_null() {
                if let Some(owner_title) = unsafe { title_of(owner) } {
                    if owner_title.contains("SurfingStudio") {
                        return 1;
                    }
                }
            }
            state.1.push(hwnd);
            1
        }

        let pid = unsafe { winapi::um::processthreadsapi::GetCurrentProcessId() };
        let mut state: (DWORD, Vec<HWND>) = (pid, Vec::new());
        unsafe { EnumWindows(Some(collect), &mut state as *mut _ as LPARAM) };
        if state.1.is_empty() {
            return (0, true);
        }
        let titles: Vec<String> = state
            .1
            .iter()
            .filter_map(|hwnd| unsafe { title_of(*hwnd) })
            .collect();
        diag::info(
            "app.window",
            "window.drop",
            &[
                ("phase", "plugin_windows_visible"),
                ("titles", &titles.join(" | ")),
            ],
        );
        let mut requested = 0;
        for hwnd in &state.1 {
            if unsafe { PostMessageW(*hwnd, WM_CLOSE, 0, 0) } != 0 {
                requested += 1;
            }
        }
        if requested == 0 {
            // 无法投递关闭请求：按“未关闭”处理，避免随后 terminate 卡住 UI。
            // Could not post a close request: treat as not closed so `terminate` cannot hang the UI.
            return (0, false);
        }
        // 插件（尤其 WPF）需要本线程继续分发消息才能完成关闭。
        // Plugins (notably WPF) need this thread to keep dispatching messages to finish closing.
        let deadline = Instant::now() + timeout;
        let mut remaining_count = state.1.len();
        while Instant::now() < deadline {
            unsafe {
                MsgWaitForMultipleObjectsEx(
                    0,
                    std::ptr::null(),
                    16,
                    QS_ALLINPUT,
                    MWMO_INPUTAVAILABLE,
                );
            }
            let mut message: MSG = unsafe { std::mem::zeroed() };
            while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0
            {
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            let mut remaining: (DWORD, Vec<HWND>) = (pid, Vec::new());
            unsafe { EnumWindows(Some(collect), &mut remaining as *mut _ as LPARAM) };
            remaining_count = remaining.1.len();
            if remaining_count == 0 {
                break;
            }
        }
        (requested, remaining_count == 0)
    }

    /// 所属模块：app / 插件生命周期
    /// 功能：非 Windows 平台没有插件自建的 Win32 顶层窗口。
    /// 输入：`_timeout`。
    /// 输出/副作用：恒为（0，true）。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / plugin lifecycle
    /// Purpose: No plugin-created Win32 top-level windows exist off Windows.
    /// Inputs: `_timeout`.
    /// Outputs / side effects: always `(0, true)`.
    /// Complexity: O(1) time and space.
    #[cfg(not(target_os = "windows"))]
    fn close_plugin_owned_windows(&self, _timeout: Duration) -> (usize, bool) {
        (0, true)
    }
}

/// 工程窗口的初始内容。
/// Initial content of a project window.
#[derive(Clone, Debug)]
pub enum ProjectStart {
    /// 新建一个空白工程。
    /// Create an empty project.
    New,
    /// 在后台打开一个工程文件。
    /// Open a project file in the background.
    Open(PathBuf),
    /// 用模板/范例预填充新工程。
    /// Pre-fill a new project from a template/example.
    Template(TemplateId),
}

impl SurfingApp {
    /// 所属模块：app / 初始化
    /// 功能：创建主页窗口（应用根视口）：共享服务、空工程与主页视图。
    /// 输入：
    /// - `ctx`：egui 上下文。
    /// - `shared`：与其他窗口共享的服务。
    /// 输出/副作用：注册引擎到音频路由；主页窗口启动一次增量插件扫描。
    /// 复杂度：时间 O(插件缓存大小 + 设备注册)，空间 O(1)。
    ///
    /// Module: app / initialization
    /// Purpose: Create the home window (root viewport): shared services, an empty project and the
    /// home view.
    /// Inputs:
    /// - `ctx`: egui context.
    /// - `shared`: services shared with the other windows.
    /// Outputs / side effects: registers the engine with the audio router; the home window starts
    /// one incremental plugin scan.
    /// Complexity: O(plugin cache size + device registration) time and O(1) space.
    pub fn new_home(ctx: &egui::Context, shared: Arc<SharedServices>) -> Self {
        Self::build(ctx, shared, WorkspaceView::Home, "SurfingStudio")
    }

    /// 所属模块：app / 初始化
    /// 功能：创建一个独立工程窗口：空白工程、后台加载指定工程或模板预填充。
    /// 输入：
    /// - `ctx`：egui 上下文。
    /// - `shared`：共享服务。
    /// - `start`：新建、打开指定路径或从模板/范例创建。
    /// 输出/副作用：注册引擎到音频路由；打开路径时启动后台加载并显示加载提示；模板直接生成工程。
    /// 复杂度：时间 O(1)（加载/模板展开另计），空间 O(1)。
    ///
    /// Module: app / initialization
    /// Purpose: Create one standalone project window: an empty project, a background load or a
    /// template-filled project.
    /// Inputs:
    /// - `ctx`: egui context.
    /// - `shared`: shared services.
    /// - `start`: create, open a path, or build from a template/example.
    /// Outputs / side effects: registers the engine with the audio router; opening a path starts a
    /// background load and shows the loading prompt; templates build immediately.
    /// Complexity: O(1) time (loading/template expansion excluded) and O(1) space.
    pub fn new_project(
        ctx: &egui::Context,
        shared: Arc<SharedServices>,
        start: ProjectStart,
    ) -> Self {
        let title = match &start {
            ProjectStart::New => "新工程 - SurfingStudio".to_string(),
            ProjectStart::Open(path) => {
                let name = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
                    .filter(|stem| !stem.is_empty())
                    .unwrap_or_else(|| "工程".into());
                format!("{name} - SurfingStudio")
            }
            ProjectStart::Template(template) => {
                let name =
                    crate::core::model::template::display_name(template).unwrap_or("模板工程");
                format!("{name} - SurfingStudio")
            }
        };
        let mut app = Self::build(ctx, shared, WorkspaceView::Studio, &title);
        match start {
            ProjectStart::New => app.reset_new_project(),
            ProjectStart::Open(path) => app.begin_load_project(path),
            ProjectStart::Template(template) => app.reset_project_from_template(template),
        }
        app
    }

    /// 所属模块：app / 初始化
    /// 功能：构造窗口的公共部分：设置/插件缓存快照、独立引擎、视图与全部 UI 状态。
    /// 输入：
    /// - `ctx`：egui 上下文（保留给主题与 DPI 使用）。
    /// - `shared`：共享服务。
    /// - `view`：主页或工作室。
    /// - `title`：窗口标题（Windows 下用于定位原生句柄）。
    /// 输出/副作用：把新引擎注册到共享音频路由；同步一次空工程。
    /// 复杂度：时间 O(设置 + 插件缓存大小 + 空工程同步)，空间 O(插件数量)。
    ///
    /// Module: app / initialization
    /// Purpose: Build the common window state: settings/cache snapshots, a private engine, the view
    /// and all UI state.
    /// Inputs:
    /// - `ctx`: egui context (kept for theme and DPI use).
    /// - `shared`: shared services.
    /// - `view`: home or studio.
    /// - `title`: window title (used to resolve the native handle on Windows).
    /// Outputs / side effects: registers the new engine with the shared audio router and syncs the
    /// empty project once.
    /// Complexity: O(settings + plugin cache size + empty project sync) time and O(plugin count) space.
    fn build(
        ctx: &egui::Context,
        shared: Arc<SharedServices>,
        view: WorkspaceView,
        title: &str,
    ) -> Self {
        let _ = ctx;
        let (settings, settings_generation) = shared.settings_snapshot();
        let (plugin_cache, plugin_cache_fingerprints, plugin_cache_generation) =
            shared.plugin_cache_snapshot();
        let midi_status = shared.midi_status();
        let engine = Arc::new(Mutex::new(Engine::new(shared.audio.sample_rate())));
        let project = Project::empty();
        if let Ok(mut engine) = engine.lock() {
            engine.sync_project(&project);
        }
        shared.audio.register(&engine);
        let window_id = shared.next_window_id();
        let mut app = Self {
            shared,
            window_id,
            window_title: title.to_string(),
            pending_window_actions: Vec::new(),
            can_resume_project: false,
            open_project_count: 0,
            settings_generation,
            plugin_cache_generation,
            midi_status,
            folder_rx: None,
            project,
            engine,
            audio_status: String::new(),
            status: String::new(),
            project_path: None,
            dirty: false,
            needs_sync: false,
            close_prompt: false,
            pending_close: false,
            close_after_ready: false,
            close_ready_deadline: None,
            abandon_engine: false,
            last_autosave: Instant::now(),
            // 主页窗口只显示空工程，不需要崩溃恢复线程。
            // The home window only holds an empty project and needs no crash-recovery thread.
            recovery: (view == WorkspaceView::Studio).then(autosave::RecoveryWorker::spawn),
            recovery_path: None,
            last_recovery_marker: None,
            last_recovery_push: Instant::now(),
            recovery_notice_shown: false,
            selected_track: 0,
            selected_clip: None,
            selected_note: None,
            selected_clips: Vec::new(),
            selected_notes: Vec::new(),
            selected_effect: None,
            selected_controller: None,
            selected_effects_lane: None,
            lane_paste_prompt: None,
            roll_windows: Vec::new(),
            roll_window_focused: false,
            roll_drag: None,
            clipboard: Clipboard::Empty,
            mouse_mode: MouseMode::Select,
            view_start: 0.0,
            visible_beats: 32.0,
            snap_beats: 0.25,
            arr_drag: ArrangementDrag::None,
            piano_drag: PianoDrag::None,
            arr_scroll: 0.0,
            arr_row_scale: 1.0,
            piano_top_pitch: 88,
            piano_note_h: 15.0,
            piano_zoom: 2.0,
            piano_view_beat: 0.0,
            scale_root: 0,
            scale_mode: 0,
            show_master: true,
            master_height: 90.0,
            piano_views: HashMap::new(),
            piano_track: None,
            piano_velocity_open: true,
            view,
            home_page: HomePage::Projects,
            home_search: String::new(),
            home_plugin_search: String::new(),
            home_template_search: String::new(),
            home_template_family: None,
            home_project_expanded: HashSet::new(),
            home_project_info: HashMap::new(),
            home_project_info_rx: None,
            home_project_sweep_at: Instant::now(),
            home_plugin_collapsed: HashSet::new(),
            home_plugin_expanded: HashSet::new(),
            settings_open: false,
            settings_page: settings::SettingsPage::default(),
            settings_folder_target: None,
            settings_audio_systems: None,
            settings_audio_devices: None,
            settings_midi_ports: None,
            settings_gpu_devices: None,
            settings_focus: false,
            settings,
            hotkey_recording: None,
            hotkey_hint: None,
            show_help: false,
            menu_bar_visible: false,
            browser: None,
            plugin_cache,
            plugin_cache_fingerprints,
            instrument_picker: None,
            scan_rx: None,
            scanning: false,
            plugin_probe_rx: None,
            classifying: false,
            load_rx: None,
            dialog_rx: None,
            loaded_project: None,
            save_rx: None,
            public_copy_rx: None,
            export: ExportState::default(),
            export_dialog_rx: None,
            engine_sync_rx: None,
            engine_sync_tx: None,
            sync_in_background: false,
            background_sync_in_flight: false,
            instrument_load_pending: false,
            instrument_load_warnings: Vec::new(),
            instrument_load_total: 0,
            instrument_load_started: Instant::now(),
            project_epoch: 0,
            loading_message: None,
            loading_until: None,
            defer_sync: false,
            engine_stale: false,
            pending_play: false,
            last_engine_sync: Instant::now(),
            editors: Vec::new(),
            editor_repaint: HashMap::new(),
            inline_placed: HashSet::new(),
            inline_failed: HashMap::new(),
            effect_editor_flags: HashMap::new(),
            pending_plugin_apply: None,
            background_synced: HashSet::new(),
            plugin_probe_cancel: None,
            audition_track: None,
            held_pitches: HashSet::new(),
            pending_auditions: Vec::new(),
            keyboard_held: HashMap::new(),
            virtual_key_note: None,
            style_ready: false,
            ui_playhead: 0.0,
            ui_playing: false,
            ui_levels: Vec::new(),
            ui_levels_l: Vec::new(),
            ui_levels_r: Vec::new(),
            ui_master_levels: (0.0, 0.0),
            ui_waveforms: HashMap::new(),
            ui_cpu_load: 0.0,
            ui_auto_stopped: false,
            ui_audio_loading: false,
            bottom_view: BottomView::PianoRoll,
            // Editors open on an explicit double-click, never on a single
            // selection click.
            bottom_open: false,
            controller_tool: crate::ui::controller::Tool::Brush,
            controller_line_start: None,
            controller_selection: Vec::new(),
            controller_marquee_start: None,
            raw_window: None,
            raw_display: None,
            window_maximized: false,
            window_restore_rect: None,
            window_restore_guard: false,
            effects_scroll: 0.0,
            effect_widths: HashMap::new(),
            plugin_param_cache: HashMap::new(),
            plugin_param_cache_at: HashMap::new(),
            effect_can_resize: HashMap::new(),
            effect_drag_from: None,
            effect_drag_target: None,
            last_auto_stopped: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pending_undo: None,
            pending_undo_seq: 0,
            edit_seq: 0,
            last_undo_at: None,
        };
        app.audio_status = app.shared.audio_status();
        app.status = if view == WorkspaceView::Home {
            format!("SurfingStudio 已就绪 · {}", app.midi_status)
        } else {
            "正在准备工程…".into()
        };
        // 多窗口排查需要知道每个窗口的编号与角色。
        // Multi-window analysis needs the id and role of every window.
        diag::info(
            "app.window",
            "window.create",
            &[
                ("id", &app.window_id.to_string()),
                (
                    "role",
                    if view == WorkspaceView::Home {
                        "home"
                    } else {
                        "studio"
                    },
                ),
                ("title", title),
            ],
        );
        // 每次启动自动做一次增量扫描：已有记录直接复用，只识别新增或变化的插件文件。
        // Run an incremental scan on every launch: cached records are reused and only new or
        // changed plugin files are classified.
        if view == WorkspaceView::Home {
            app.start_startup_scan();
        }
        app
    }

    /// 所属模块：app / 共享状态
    /// 功能：按版本号把共享设置与插件缓存同步到本窗口，并刷新音频/MIDI 状态文本。
    /// 输入：`self`。
    /// 输出/副作用：可能替换 `settings`、`plugin_cache` 与两个版本号；不写盘。
    /// 复杂度：时间 O(1)（未变化时），空间 O(变化数据大小)。
    ///
    /// Module: app / shared state
    /// Purpose: Pull shared settings and the plugin cache into this window when their generations
    /// changed, and refresh the audio/MIDI status text.
    /// Inputs: `self`.
    /// Outputs / side effects: may replace `settings`, `plugin_cache` and both generations; never
    /// writes to disk.
    /// Complexity: O(1) time when unchanged, O(changed data) space.
    fn sync_shared_state(&mut self) {
        if let Some((settings, generation)) =
            self.shared.settings_if_newer(self.settings_generation)
        {
            self.settings = settings;
            self.settings_generation = generation;
        }
        if let Some((plugins, fingerprints, generation)) = self
            .shared
            .plugin_cache_if_newer(self.plugin_cache_generation)
        {
            self.plugin_cache = plugins;
            self.plugin_cache_fingerprints = fingerprints;
            self.plugin_cache_generation = generation;
        }
        self.audio_status = self.shared.audio_status();
        self.midi_status = self.shared.midi_status();
    }

    /// 所属模块：app / 窗口标题
    /// 功能：返回工程窗口的标题基名：优先文件名，其次工程名。
    /// 输入：`self`。
    /// 输出/副作用：展示文本；不修改状态。
    /// 复杂度：时间 O(路径长度)，空间 O(名称长度)。
    ///
    /// Module: app / window title
    /// Purpose: Return the base title of a project window: the file stem when saved, otherwise the
    /// project name.
    /// Inputs: `self`.
    /// Outputs / side effects: display text; no state change.
    /// Complexity: O(path length) time and O(name length) space.
    pub fn project_title(&self) -> String {
        if let Some(path) = &self.project_path {
            if let Some(stem) = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .filter(|stem| !stem.is_empty())
            {
                return stem;
            }
        }
        let name = self.project.name.trim();
        if name.is_empty() {
            "未命名工程".into()
        } else {
            name.to_string()
        }
    }

    /// 所属模块：app / 文件操作
    /// 功能：判断本窗口是否仍在后台加载工程。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / file operations
    /// Purpose: Whether this window is still loading a project in the background.
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1) time and space.
    pub fn is_loading_project(&self) -> bool {
        self.load_rx.is_some() || self.loaded_project.is_some()
    }

    /// 所属模块：app / 窗口句柄
    /// 功能：丢弃本窗口缓存的原始句柄，使其在下一帧按当前标题重新解析。
    /// 输入：`self`。
    /// 输出/副作用：清空 `raw_window` / `raw_display`；不修改工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / window handle
    /// Purpose: Drop the cached raw handles so they are resolved again for the current title.
    /// Inputs: `self`.
    /// Outputs / side effects: clears `raw_window` / `raw_display`; the project is untouched.
    /// Complexity: O(1) time and space.
    pub fn forget_native_window(&mut self) {
        self.raw_window = None;
        self.raw_display = None;
    }

    /// 所属模块：app / 后台轮询
    /// 功能：主页窗口未绘制时（根窗口显示工程且主页窗口已关闭），继续处理插件扫描、类型识别
    /// 与保存结果，避免后台任务卡住。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：可能更新共享插件缓存、状态栏与保存状态；不绘制界面。
    /// 复杂度：时间 O(1)/帧，空间 O(1)。
    ///
    /// Module: app / background polling
    /// Purpose: While the home window is not painted (the root shows a project and home is closed),
    /// keep handling plugin scans, classification and save results so background work never stalls.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: may update the shared plugin cache, status bar and save state; paints
    /// nothing.
    /// Complexity: O(1) per frame and O(1) space.
    pub fn poll_background(&mut self, ctx: &egui::Context) {
        self.sync_shared_state();
        if self.view != WorkspaceView::Home {
            return;
        }
        self.poll_scan();
        self.poll_plugin_probes();
        self.poll_save_project();
        if self.scanning || self.classifying || self.save_rx.is_some() {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }

    /// 所属模块：app / 共享状态
    /// 功能：把本窗口的全局设置写回共享服务（同时保存到用户配置目录）。
    /// 输入：`self`。
    /// 输出/副作用：更新共享设置与其他窗口的副本；写盘。
    /// 复杂度：时间 O(设置大小)，空间 O(设置大小)。
    ///
    /// Module: app / shared state
    /// Purpose: Publish this window's global settings to the shared services (and save them).
    /// Inputs: `self`.
    /// Outputs / side effects: updates the shared settings and the other windows' copies; writes
    /// the file.
    /// Complexity: O(settings size) time and space.
    pub(crate) fn commit_settings(&mut self) {
        self.settings_generation = self.shared.write_settings(&self.settings);
    }

    /// 所属模块：app / 共享状态
    /// 功能：把本窗口的插件缓存写回共享服务（同时保存缓存文件）。
    /// 输入：`self`。
    /// 输出/副作用：更新共享缓存与其他窗口的副本；写盘。
    /// 复杂度：时间 O(插件数量)，空间 O(插件数量)。
    ///
    /// Module: app / shared state
    /// Purpose: Publish this window's plugin cache to the shared services (and save the file).
    /// Inputs: `self`.
    /// Outputs / side effects: updates the shared cache and the other windows' copies; writes the
    /// cache file.
    /// Complexity: O(plugin count) time and space.
    pub(super) fn publish_plugin_cache(&mut self) {
        self.plugin_cache_generation = self
            .shared
            .write_plugin_cache(&self.plugin_cache, &self.plugin_cache_fingerprints);
    }

    /// 所属模块：app / 窗口句柄
    /// 功能：按窗口标题在 Windows 上定位本窗口的原生句柄，供文件对话框与内嵌插件使用。
    /// 输入：`ctx`：egui 上下文（未使用，保留跨平台签名）。
    /// 输出/副作用：写入 `raw_window` / `raw_display`；找不到时下一帧重试。
    /// 复杂度：时间 O(顶层窗口数)，空间 O(1)。
    ///
    /// Module: app / window handle
    /// Purpose: Resolve this window's native handle on Windows by its title, for file dialogs and
    /// embedded plugins.
    /// Inputs: `ctx`: egui context (unused, kept for a cross-platform signature).
    /// Outputs / side effects: sets `raw_window` / `raw_display`; retries next frame when missing.
    /// Complexity: O(top-level windows) time and O(1) space.
    #[cfg(target_os = "windows")]
    fn resolve_native_window(&mut self, _ctx: &egui::Context) {
        let expected: Vec<u16> = self.window_title.encode_utf16().collect();
        // 缓存的句柄必须仍然存活、属于本进程且标题与当前窗口一致：窗口在“根工程↔主页”
        // 之间切换时标题会变化，旧句柄也可能已被销毁。
        // The cached handle must still be alive, owned by this process and carry the current
        // title: switching the root between project and home changes titles and may destroy old
        // windows.
        if let Some(RawWindowHandle::Win32(handle)) = self.raw_window {
            let hwnd = handle.hwnd.get() as winapi::shared::windef::HWND;
            let mut text = [0_u16; 512];
            let length = unsafe {
                winapi::um::winuser::GetWindowTextW(hwnd, text.as_mut_ptr(), text.len() as i32)
            };
            let mut process_id = 0_u32;
            unsafe {
                winapi::um::winuser::GetWindowThreadProcessId(hwnd, &mut process_id);
            }
            let alive = unsafe { winapi::um::winuser::IsWindow(hwnd) } != 0;
            if alive
                && process_id == unsafe { winapi::um::processthreadsapi::GetCurrentProcessId() }
                && length.max(0) as usize == expected.len()
                && text[..expected.len()] == expected[..]
            {
                return;
            }
            self.raw_window = None;
            self.raw_display = None;
        }
        let title: Vec<u16> = self
            .window_title
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // 安全性：只按标题查找本进程的顶层窗口，并在使用前校验进程 id。
        // SAFETY: look up a top-level window of this process by title and verify its process id.
        unsafe {
            let hwnd = winapi::um::winuser::FindWindowW(std::ptr::null(), title.as_ptr());
            if hwnd.is_null() {
                return;
            }
            let mut process_id = 0_u32;
            winapi::um::winuser::GetWindowThreadProcessId(hwnd, &mut process_id);
            if process_id != winapi::um::processthreadsapi::GetCurrentProcessId() {
                return;
            }
            let Some(handle) = std::num::NonZeroIsize::new(hwnd as isize) else {
                return;
            };
            self.raw_window = Some(RawWindowHandle::Win32(
                raw_window_handle::Win32WindowHandle::new(handle),
            ));
            self.raw_display = Some(RawDisplayHandle::Windows(
                raw_window_handle::WindowsDisplayHandle::new(),
            ));
        }
    }

    /// 所属模块：app / 窗口句柄
    /// 功能：非 Windows 平台没有可解析的原生句柄。
    /// 输入：`ctx`：egui 上下文（未使用）。
    /// 输出/副作用：无。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / window handle
    /// Purpose: No native handle to resolve off Windows.
    /// Inputs: `ctx`: egui context (unused).
    /// Outputs / side effects: none.
    /// Complexity: O(1) time and space.
    #[cfg(not(target_os = "windows"))]
    fn resolve_native_window(&mut self, _ctx: &egui::Context) {}
}

impl SurfingApp {
    /// 所属模块：app / 每帧编排
    /// 功能：每帧执行输入 → 后台轮询（含自动保存/崩溃恢复）→ 快照 → 工程同步 → 绘制（主页或
    /// 工作室）→ 未保存关闭确认 → 撤销手势，并安排下一次重绘。由 `WindowHost` 分别在自己的窗口
    /// 视口里调用。
    /// 输入：`ctx`：当前窗口的 egui 上下文。
    /// 输出/副作用：绘制本窗口全部面板（含浮动卷帘、设置视口与关闭确认）；可能修改工程、插件窗口、
    /// 崩溃恢复文件与状态栏。
    /// 复杂度：每帧 O(可见控件数 + 轨道/片段数 + 插件数)。
    ///
    /// Module: app / per-frame orchestration
    /// Purpose: Each frame: input → background polling (including autosave/crash recovery) →
    /// snapshot → project sync → paint (home or studio) → unsaved close confirmation → undo
    /// gesture, then schedule the next repaint. `WindowHost` calls this once per window inside that
    /// window's viewport.
    /// Inputs: `ctx`: egui context of this window.
    /// Outputs / side effects: paints all panels of this window (including floating rolls, the
    /// settings viewport and the close confirmation); may modify the project, plugin windows, the
    /// crash-recovery file and the status bar.
    /// Complexity: O(visible widgets + tracks/clips + plugins) per frame.
    pub fn ui_frame(&mut self, ctx: &egui::Context) {
        diag::set_activity("ui.frame");
        self.sync_shared_state();
        self.resolve_native_window(ctx);
        // 最近获得焦点的工程窗口负责 MIDI 输入与插件窗口里的空格快捷键。
        // The project window focused last owns MIDI input and the Space shortcut from plugin windows.
        if self.view == WorkspaceView::Studio
            && ctx.input(|i| i.viewport().focused.unwrap_or(false))
        {
            self.shared.set_last_focused_window(self.window_id);
        }
        if !self.style_ready {
            // eframe applies the OS theme on the first frame; re-apply ours after that.
            theme::apply_style(ctx);
            self.style_ready = true;
        }
        // 独立窗口（插件编辑器 / 浮动卷帘）里的空格不会进入 egui 主视口输入，
        // 用线程消息钩子补上；本窗口的空格仍由 egui 处理。
        // Space in standalone windows (plugin editors / floating rolls) never reaches the egui
        // viewport, so observe it with a thread message hook; Space in this window stays with egui.
        #[cfg(target_os = "windows")]
        let native_window = match self.raw_window {
            Some(RawWindowHandle::Win32(handle)) => Some(handle.hwnd.get()),
            _ => None,
        };
        #[cfg(not(target_os = "windows"))]
        let native_window: Option<isize> = None;
        hotkeys::register_window(native_window);
        // 插件编辑器的 GUI 定时器使用低优先级 WM_TIMER；宿主空闲等待时它不会唤醒消息循环。
        // 心跳线程每 16 ms 投递空消息并请求重绘，保证该编辑器持续刷新。
        // Plugin GUI timers use low-priority WM_TIMER, which does not wake an idle host wait.
        // The heartbeat posts an empty message and requests a repaint every 16 ms.
        #[cfg(target_os = "windows")]
        let ui_thread = native_window.map_or(0, |hwnd| unsafe {
            winapi::um::winuser::GetWindowThreadProcessId(
                hwnd as winapi::shared::windef::HWND,
                std::ptr::null_mut(),
            )
        });
        #[cfg(not(target_os = "windows"))]
        let ui_thread = 0;
        heartbeat::install(ctx, ui_thread);
        if ctx.input(|i| i.pointer.any_pressed()) {
            self.begin_edit_gesture();
        }
        // 收到关闭请求时：有未保存修改则取消关闭并弹出确认；否则立即销毁插件 GUI，
        // 避免窗口在 eframe 退出流程中继续停留。
        // On a close request: cancel it and ask for confirmation when there are unsaved changes;
        // otherwise destroy plugin GUIs immediately so they do not linger through eframe's shutdown.
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.abandon_engine {
                // 超时强制关闭：不再取消关闭，直接隐藏插件窗口。
                // Forced close after a load timeout: keep the close and hide plugin windows.
                for (_, mut window) in self.editors.drain(..) {
                    window.destroy_for_exit();
                    std::mem::forget(window);
                }
            } else if !self.can_close_now() {
                // 加载/插件同步仍在进行：先取消关闭，等后台工作结束后再关闭窗口。
                // Loading or plugin sync is still running: cancel the close and finish it once the
                // background work lands.
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.request_close_when_ready();
            } else if self.has_unsaved_changes() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                if !self.close_prompt {
                    self.close_prompt = true;
                    self.status = "工程有未保存的修改".into();
                }
            } else {
                // 正常关闭工程窗口：按插件生命周期先 detach 视图再销毁窗口，避免插件实例
                // 释放时才发现编辑器窗口被强行销毁（VOCALOID 6 的 WPF OnClosing 会因此抛出
                // NullReferenceException）。
                // Normal project-window close: detach the editor views before destroying their
                // windows, so a plugin instance is never released after its editor window was
                // destroyed behind its back (VOCALOID 6's WPF OnClosing throws a
                // NullReferenceException).
                self.close_all_editors();
            }
        }
        self.service_editors();
        diag::set_activity("ui.poll");
        self.poll_scan();
        self.poll_plugin_probes();
        self.poll_midi();
        self.poll_load_project();
        self.poll_save_project();
        self.poll_public_copy();
        self.poll_export_dialog();
        self.poll_export();
        self.poll_autosave();
        self.poll_engine_sync();
        // 加载/插件同步结束、关闭请求得以继续时，本帧发送关闭。
        // Once loading and plugin sync have finished, continue a close requested earlier.
        if self.close_after_ready {
            if self.can_close_now() {
                self.close_after_ready = false;
                self.close_prompt = false;
                self.pending_close = true;
            } else if self
                .close_ready_deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
            {
                // 后台插件加载超时：放弃引擎并关闭窗口，宁可泄漏一次实例也不让窗口卡死。
                // The background plugin load timed out: abandon the engine and close the window;
                // leaking one instance is better than freezing the window.
                self.close_after_ready = false;
                self.close_prompt = false;
                self.abandon_engine = true;
                self.pending_close = true;
                self.status = "插件加载超时，正在强制关闭窗口…".into();
            }
        }
        // 保存完成或用户放弃修改后，本帧统一请求关闭，避免在轮询函数里依赖 egui 上下文。
        // After a save finishes or the user discards changes, request the close here instead of
        // depending on the egui context inside the polling functions.
        if self.pending_close {
            self.pending_close = false;
            self.close_prompt = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // One lock-free snapshot per frame; all panels (and keyboard-driven
        // paste positioning) read the cached values.
        // 先把上一帧因引擎锁被音频回调占用而暂存的试听事件按顺序补发，再读取遥测快照。
        // Replay audition events queued while the audio callback held the engine lock, in order,
        // before reading the telemetry snapshot.
        self.flush_pending_auditions();
        let changed_tracks = self.refresh_engine_snapshot();
        self.handle_keyboard(ctx);
        self.validate_selection();
        self.update_roll_drag(ctx);

        let playing = self.ui_playing;
        if !changed_tracks.is_empty() {
            // 插件实例已经按新参数发声，这里只刷新界面提示。
            // The plugin instance already plays the new values; only refresh the UI hint.
            self.status = "插件参数已更新".into();
        }

        if self.instrument_load_pending {
            // 每帧只补载一个时间片的乐器插件，帧与帧之间保持重绘与响应。
            // Load one time slice of instrument plugins per frame so the window keeps repainting
            // and responding between slices.
            self.step_instrument_load();
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if self.needs_sync {
            if self.engine_sync_rx.is_some() {
                // A background plugin sync is running; keep painting the loading
                // prompt and apply the newest edit afterwards.
                ctx.request_repaint_after(Duration::from_millis(16));
            } else if self.defer_sync {
                // Paint one frame with the loading prompt before the sync runs.
                self.defer_sync = false;
                ctx.request_repaint();
            } else if playing
                && self.last_engine_sync.elapsed() < Duration::from_millis(60)
                && self.loading_message.is_none()
            {
                // A drag can mark the project dirty every frame. Throttle the
                // sync while the transport runs so the audio callback does not
                // miss blocks behind the engine lock.
                ctx.request_repaint_after(Duration::from_millis(16));
            } else if self.sync_in_background {
                self.sync_in_background = false;
                self.begin_async_sync();
                self.last_engine_sync = Instant::now();
                ctx.request_repaint_after(Duration::from_millis(16));
            } else {
                self.sync_engine();
                self.last_engine_sync = Instant::now();
                if self.loading_message.is_some() {
                    if let Some(path) = &self.project_path {
                        self.status = format!("已加载 {}", path.display());
                    }
                    self.loading_message = Some("就绪".into());
                    self.loading_until = Some(Instant::now() + Duration::from_millis(200));
                }
                if self.pending_play {
                    self.pending_play = false;
                    self.toggle_play();
                }
            }
        }

        if self.ui_audio_loading {
            // Keep polling until the background decoder publishes its result;
            // the arrangement can then draw the waveform without user input.
            ctx.request_repaint_after(Duration::from_millis(120));
        }

        let auto_stopped = self.ui_auto_stopped;
        if auto_stopped && !self.last_auto_stopped {
            self.status = "已在最后一个 MIDI 片段结束后 2 小节自动停止".into();
        }
        self.last_auto_stopped = auto_stopped;

        diag::set_activity("ui.paint");
        self.app_bar(ctx);
        // 主页不显示编曲工具栏；状态栏在两种视图下都保留，用于展示音频/MIDI 与加载提示。
        // The home page hides the arrangement toolbar; the status bar stays in both views so audio,
        // MIDI and loading state remain visible.
        if self.view == WorkspaceView::Studio {
            self.main_toolbar(ctx);
        }
        self.status_bar(ctx);

        // 主页只绘制两栏内容；编曲面板、检查器与编辑卷帘只在工作室视图下存在。
        // The home page only paints its two columns; the arrangement, inspector and editor rolls
        // exist in the studio view only.
        if self.view == WorkspaceView::Home {
            self.ui_home(ctx);
        } else {
            if self.bottom_open {
                // 插件 GUI 会等比缩放到卡片大小，底部面板不再需要按插件高度撑开。
                // Plugin GUIs scale to fit their card, so the panel no longer grows to the
                // plugin's declared height.
                let editor_min_height = 190.0;
                let editor_max_height = 760.0_f32.max(editor_min_height);

                egui::TopBottomPanel::bottom("editor_panel")
                    .resizable(true)
                    .default_height(370.0_f32.max(editor_min_height))
                    .min_height(editor_min_height)
                    .max_height(editor_max_height)
                    .frame(
                        egui::Frame::NONE
                            .fill(theme::BG)
                            .inner_margin(egui::Margin::symmetric(8, 6))
                            .stroke(egui::Stroke::new(1.0_f32, theme::LINE)),
                    )
                    .show(ctx, |ui| match self.bottom_view {
                        BottomView::PianoRoll => self.ui_piano_roll(ui),
                        BottomView::Controller { track_id, kind } => {
                            self.ui_controller_lane(ui, track_id, kind, false)
                        }
                        BottomView::Effects { track_id } => self.ui_effects(ui, track_id, false),
                    });
            }

            egui::SidePanel::right("inspector")
                .resizable(true)
                .default_width(340.0)
                .min_width(300.0)
                .max_width(460.0)
                .frame(
                    egui::Frame::NONE
                        .fill(theme::PANEL)
                        .inner_margin(egui::Margin::symmetric(12, 10)),
                )
                .show(ctx, |ui| {
                    self.ui_inspector(ui);
                });

            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(theme::BG))
                .show(ctx, |ui| {
                    self.ui_arrangement(ui);
                });

            self.update_roll_drag(ctx);
            self.ui_roll_windows(ctx);
            self.ui_lane_paste_prompt(ctx);
        }
        // 无系统边框后，窗口缩放改由应用栏同层的边缘热区接管。
        // Without OS decorations, window resizing is handled by edge hit areas.
        self.window_resize_edges(ctx);
        // 插件浏览器、帮助与设置窗口在主页同样可用。
        // The plugin browser, help and settings windows are available on the home page too.
        self.ui_browser(ctx);
        self.ui_instrument_picker(ctx);
        self.ui_help(ctx);
        self.ui_settings_window(ctx);
        self.ui_export_window(ctx);
        // 未保存关闭确认绘制在所有面板之上；确认期间窗口保持打开。
        // The unsaved-changes confirmation is painted above every panel; the window stays open
        // while it is shown.
        if self.close_prompt {
            self.ui_close_prompt(ctx);
        }
        // 本帧没有被摆放到卡片里的内嵌编辑器必须隐藏，否则会停在旧位置。
        // Inline editors not placed into a card this frame must be hidden or they stay put.
        self.finish_inline_layout();
        // 编辑器打开时即使本窗口未获得焦点也保持 16 ms 重绘：插件 GUI 的参数反馈与原生
        // 定时器依赖本窗口每帧执行 `service_editors`。
        // While an editor is open, keep this viewport repainting at 16 ms even when unfocused:
        // plugin GUI feedback and native timers rely on `service_editors` running every frame.
        if !self.editors.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        if let Some(until) = self.loading_until {
            if Instant::now() >= until {
                self.loading_until = None;
                self.loading_message = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(40));
            }
        }
        if let Some(message) = self.loading_message.clone() {
            egui::Window::new("加载")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .movable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_min_width(260.0);
                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Spinner::new().size(28.0));
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(message).size(14.0).color(theme::TEXT));
                    });
                    ui.add_space(10.0);
                });
        }

        if ctx.input(|i| i.pointer.any_down()) {
            self.commit_edit_gesture();
        } else {
            self.end_edit_gesture();
        }

        let playing = self.ui_playing;
        if self.engine_sync_rx.is_some() {
            // Keep the loading spinner and the responsive UI animating while
            // plugins load on the background thread.
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        } else if playing || !self.editors.is_empty() || self.export.busy {
            // 插件 GUI 依赖本线程持续分发窗口消息与 idle；编辑器打开时保持 60 FPS 轮询。
            // Plugin GUIs rely on this thread dispatching window messages and idle calls;
            // keep a 60 FPS tick while an editor is open.
            ctx.request_repaint_after(EDITOR_REPAINT_INTERVAL);
        } else if self.ui_audio_loading {
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        } else if self.classifying {
            // 后台识别插件类型时保持轮询，让列表实时更新。
            // Keep polling while plugin types are classified in the background so the
            // list updates live.
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }
    }
}
