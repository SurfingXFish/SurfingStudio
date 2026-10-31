//! 所属模块：app / 插件运行时
//! 文件用途：
//! - 插件加载/扫描、插件缓存与启动增量扫描。
//! - 插件参数缓存、编辑器窗口（独立 / 内嵌）、插件应用收尾。
//! 函数清单：
//! - `EditorWindow::service` / `closed_by_user` / `destroy_for_exit` / `is_standalone` / `is_visible` / `bring_to_front` / `hide_inline` / `place_inline`：编辑器窗口维护。
//! - `plugin_cache_path` / `load_plugin_cache` / `save_plugin_cache` / `plugin_fingerprint` / `cache_key`：插件缓存文件与指纹。
//! - `cached_plugin_params` / `invalidate_plugin_params` / `invalidate_plugin_param_slot`：参数缓存。
//! - `capture_live_plugin_state` / `begin_engine_load` / `prepare_playback_plugins` / `service_editors`：
//!   编辑器与运行时服务。
//! - `open_editor` / `close_editor` / `close_all_editors` / `open_inline_editor` / `place_inline_editor` / `finish_inline_layout` / `has_inline_editor`：编辑器窗口生命周期。
//! - `apply_plugin` / `finish_plugin_apply` / `ensure_ui_thread_instance`：插件写入槽位与收尾。
//! - `open_browser` / `start_scan` / `scan_vst3_only` / `scan_legacy_only` / `start_startup_scan` / `start_scan_with` / `poll_scan`：完整与增量扫描。
//! - `scan_full` / `scan_incremental`：完整与增量扫描的后台实现。
//! - `start_plugin_probes` / `cancel_plugin_probes` / `poll_plugin_probes`：未知 VST3 的后台类型识别。
//!
//! Module: app / plugin runtime
//! Purpose:
//! - Plugin loading/scanning, plugin cache and the incremental startup scan.
//! - Parameter caching, editor windows (standalone / inline) and plugin apply finalisation.
//! Functions:
//! - `EditorWindow::service` / `closed_by_user` / `destroy_for_exit` / `is_standalone` / `is_visible` / `bring_to_front` / `hide_inline` / `place_inline`: editor window maintenance.
//! - `plugin_cache_path` / `load_plugin_cache` / `save_plugin_cache` / `plugin_fingerprint` / `cache_key`: cache file and fingerprints.
//! - `cached_plugin_params` / `invalidate_plugin_params` / `invalidate_plugin_param_slot`: parameter cache.
//! - `capture_live_plugin_state` / `begin_engine_load` / `prepare_playback_plugins` / `service_editors`:
//!   editor and runtime services.
//! - `open_editor` / `close_editor` / `close_all_editors` / `open_inline_editor` / `place_inline_editor` / `finish_inline_layout` / `has_inline_editor`: editor window lifecycle.
//! - `apply_plugin` / `finish_plugin_apply` / `ensure_ui_thread_instance`: writing a plugin into a slot and finishing up.
//! - `open_browser` / `start_scan` / `scan_vst3_only` / `scan_legacy_only` / `start_startup_scan` / `start_scan_with` / `poll_scan`: full and incremental scans.
//! - `scan_full` / `scan_incremental`: background implementations of both scan modes.
//! - `start_plugin_probes` / `cancel_plugin_probes` / `poll_plugin_probes`: deferred classification of unknown VST3 plugins.

use super::*;
use crate::diag;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 插件缓存文件版本；结构不兼容时递增并丢弃旧缓存。
/// Plugin cache file version; bump when the layout changes incompatibly and drop old caches.
const PLUGIN_CACHE_VERSION: u32 = 1;

/// 一次扫描的返回值：插件记录、文件指纹与复用统计。
/// Result of one scan: plugin records, file fingerprints and reuse statistics.
pub struct ScanOutcome {
    pub plugins: Vec<ScannedPlugin>,
    pub fingerprints: HashMap<String, (u64, u64)>,
    /// 直接复用缓存、没有重新识别的插件数。
    /// Number of plugins reused from the cache without re-classification.
    pub reused: usize,
}

/// 插件缓存文件布局：记录、文件指纹与版本。
/// Plugin cache file layout: records, file fingerprints and version.
#[derive(Serialize, Deserialize, Default)]
struct PluginCacheFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    plugins: Vec<ScannedPlugin>,
    #[serde(default)]
    fingerprints: HashMap<String, (u64, u64)>,
}

/// 所属模块：app / 插件扫描 / 缓存
/// 功能：返回插件缓存的候选路径。用户指定缓存目录时优先使用该目录，其次平台缓存目录，
/// 不可写时回退到平台配置目录与系统临时目录。
/// 输入：`cache_dir`：设置中的自定义缓存目录；`None` 使用系统默认。
/// 输出/副作用：返回候选路径列表；不创建目录。
/// 复杂度：时间 O(1)，空间 O(路径长度)。
///
/// Module: app / plugin scanning / cache
/// Purpose: Return candidate cache paths. A user-selected cache directory comes first, then the
/// platform cache directory, falling back to the platform config directory and the system temp
/// directory when that location is not writable.
/// Inputs: `cache_dir` — custom cache directory from the settings; `None` uses the default.
/// Outputs / side effects: candidate paths; no directory is created.
/// Complexity: O(1) time and O(path length) space.
fn plugin_cache_paths(cache_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    if let Some(directory) = cache_dir {
        paths.push(directory.join("plugin-cache.json"));
    }
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Some(base) = crate::platform::cache_dir() {
        bases.push(base);
    }
    if let Some(base) = crate::platform::config_dir() {
        bases.push(base);
    }
    paths.extend(bases.into_iter().map(|base| base.join("plugin-cache.json")));
    paths.push(
        crate::platform::temp_dir()
            .join(crate::platform::APP_DIR_NAME)
            .join("plugin-cache.json"),
    );
    paths
}

/// 所属模块：app / 插件扫描 / 缓存
/// 功能：读取上次保存的插件记录与文件指纹；缺失或损坏时返回空缓存。
/// 输入：`cache_dir`：设置中的自定义缓存目录；`None` 使用系统默认。
/// 输出/副作用：返回 `(记录, 指纹)`；只读文件系统。
/// 复杂度：时间 O(缓存文件大小)，空间 O(插件数量)。
///
/// Module: app / plugin scanning / cache
/// Purpose: Read the previously saved plugin records and file fingerprints; an absent or
/// corrupt cache yields an empty result.
/// Inputs: `cache_dir` — custom cache directory from the settings; `None` uses the default.
/// Outputs / side effects: `(records, fingerprints)`; the filesystem is read-only.
/// Complexity: O(cache size) time and O(plugin count) space.
pub(super) fn load_plugin_cache(
    cache_dir: Option<&Path>,
) -> (Vec<ScannedPlugin>, HashMap<String, (u64, u64)>) {
    for path in plugin_cache_paths(cache_dir) {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(file) = serde_json::from_str::<PluginCacheFile>(&text) else {
            continue;
        };
        if file.version != PLUGIN_CACHE_VERSION {
            continue;
        }
        return (file.plugins, file.fingerprints);
    }
    (Vec::new(), HashMap::new())
}

/// 所属模块：app / 插件扫描 / 缓存
/// 功能：把插件记录与文件指纹写入缓存文件；失败时静默忽略。
/// 输入：
/// - `plugins`：扫描记录。
/// - `fingerprints`：路径小写 → (修改时间秒, 字节数)。
/// - `cache_dir`：设置中的自定义缓存目录；`None` 使用系统默认。
/// 输出/副作用：创建配置目录并写盘；不修改内存状态。
/// 复杂度：时间 O(插件数量)，空间 O(缓存文件大小)。
///
/// Module: app / plugin scanning / cache
/// Purpose: Persist plugin records and file fingerprints; failures are ignored.
/// Inputs:
/// - `plugins`: scan records.
/// - `fingerprints`: lower-case path → (mtime seconds, size).
/// - `cache_dir`: custom cache directory from the settings; `None` uses the default.
/// Outputs / side effects: creates the config directory and writes the file; no in-memory change.
/// Complexity: O(plugin count) time and O(cache size) space.
pub(super) fn save_plugin_cache(
    plugins: &[ScannedPlugin],
    fingerprints: &HashMap<String, (u64, u64)>,
    cache_dir: Option<&Path>,
) {
    let file = PluginCacheFile {
        version: PLUGIN_CACHE_VERSION,
        plugins: plugins.to_vec(),
        fingerprints: fingerprints.clone(),
    };
    let Ok(text) = serde_json::to_string(&file) else {
        return;
    };
    // 依次尝试候选目录：第一个可写目录用于保存，后续启动按同样顺序读取。
    // Try the candidate directories in order; the first writable one is used, and the next launch
    // reads them in the same order.
    for path in plugin_cache_paths(cache_dir) {
        if let Some(parent) = path.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                continue;
            }
        }
        if std::fs::write(&path, &text).is_ok() {
            return;
        }
    }
}

/// 所属模块：app / 插件扫描 / 缓存
/// 功能：计算插件文件的轻量指纹（修改时间秒 + 字节数），用于判断是否需要重新识别。
/// 输入：`path` 插件路径（bundle 目录或 DLL）。
/// 输出/副作用：返回 `(mtime, size)`；无法读取时返回 `(0, 0)`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / plugin scanning / cache
/// Purpose: Compute a lightweight plugin file fingerprint (mtime seconds + byte size) to decide
/// whether re-classification is needed.
/// Inputs: `path` — plugin path (bundle directory or DLL).
/// Outputs / side effects: `(mtime, size)`, or `(0, 0)` when unreadable.
/// Complexity: O(1) time and space.
fn plugin_fingerprint(path: &Path) -> (u64, u64) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return (0, 0);
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    (modified, metadata.len())
}

/// 所属模块：app / 插件扫描 / 缓存
/// 功能：返回路径的小写去重键，与缓存指纹保持一致。
/// 输入：`path` 插件路径。
/// 输出/副作用：返回小写路径文本；不修改状态。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: app / plugin scanning / cache
/// Purpose: Return the lower-case dedup key of a path so it matches cached fingerprints.
/// Inputs: `path` — plugin path.
/// Outputs / side effects: lower-case path text; no state change.
/// Complexity: O(path length) time and space.
fn cache_key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

/// A standalone plugin editor window: VST3 uses the host window, VST1/VST2 use
/// the generic native window.
pub enum EditorWindow {
    Vst3(PluginWindow),
    /// 内嵌在效果器卷帘卡片中的 VST3 编辑器（主窗口子窗口）。
    /// VST3 editor embedded inside an effect-roll card (a child of the main window).
    Vst3Inline(PluginWindow),
    Vst2(crate::plugin::Vst2EditorWindow),
}

/// 插件写入槽位后，等待后台同步完成的收尾信息。
/// Slot information kept until the background engine sync finishes.
#[derive(Clone, Copy, Debug)]
pub(super) struct PendingPluginApply {
    pub target: crate::ui::PluginTarget,
    pub track_id: Option<TrackId>,
    /// 写入时的轨道序号：后台加载期间用户可能切换选中轨道。
    /// Track index at apply time: the selection may change while the worker loads.
    pub track_index: usize,
    pub effect_index: Option<usize>,
}

impl EditorWindow {
    /// 所属模块：app / 插件编辑器窗口
    /// 功能：分发平台消息，并按需低频重绘 VST3 编辑器。
    /// 输入：`repaint`：本帧是否允许强制重绘（调用方已按时间限流）。
    /// 输出/副作用：处理插件消息窗口；可能调用 `RedrawWindow`。
    /// 复杂度：时间 O(插件消息数)，空间 O(1)。
    ///
    /// Module: app / plugin editor window
    /// Purpose: Service platform messages and optionally repaint a VST3 editor.
    /// Inputs: `repaint`: whether a forced repaint is allowed this frame (the caller throttles it).
    /// Outputs / side effects: dispatches plugin messages; may call `RedrawWindow`.
    /// Complexity: O(plugin messages) time and O(1) space.
    pub(super) fn service(&mut self, repaint: bool) {
        match self {
            EditorWindow::Vst3(window) => {
                let _ = window.service_platform_events();
                // 只在插件窗口不是系统前台窗口时低频强制重绘：Kontakt 8 的 Qt Quick 界面在前台时
                // 会自行刷新，宿主每帧 RedrawWindow 反而可能与其绘制循环互相拖住。
                // Repaint a background editor only at a low rate: Kontakt 8's Qt Quick UI paints
                // itself while focused, and a host-side RedrawWindow every frame can fight its paint
                // loop.
                if repaint && !window.is_foreground() {
                    window.refresh_repaint();
                }
            }
            EditorWindow::Vst3Inline(window) => {
                let _ = window.service_platform_events();
            }
            EditorWindow::Vst2(window) => window.service(),
        }
    }

    pub(super) fn closed_by_user(&self) -> bool {
        match self {
            // A closed VST2 window is hidden but kept so it can be reopened
            // without instantiating the plugin again.
            EditorWindow::Vst3(window) => window.closed_by_user(),
            // Inline editors have no close button; the host controls their lifetime.
            EditorWindow::Vst3Inline(_) => false,
            EditorWindow::Vst2(_) => false,
        }
    }

    /// 所属模块：app / 插件编辑器窗口
    /// 功能：退出时直接隐藏并销毁原生窗口，不调用插件 `close_editor`。
    /// 输入：`self`。
    /// 输出/副作用：窗口立即消失；插件视图保持附加状态（进程即将退出）。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / plugin editor window
    /// Purpose: Hide and destroy the native window on shutdown without calling the plugin's
    /// `close_editor`.
    /// Inputs: `self`.
    /// Outputs / side effects: the window disappears immediately; the plugin view stays
    /// attached (the process is exiting).
    /// Complexity: O(1) time and space.
    pub(super) fn destroy_for_exit(&mut self) {
        match self {
            EditorWindow::Vst3(window) | EditorWindow::Vst3Inline(window) => {
                window.destroy_native_window();
            }
            EditorWindow::Vst2(window) => window.destroy_for_exit(),
        }
    }

    /// 所属模块：app / 插件编辑器窗口
    /// 功能：判断窗口是否为独立窗口（非内嵌）。
    /// 输入：`self`。
    /// 输出/副作用：返回布尔值。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / plugin editor window
    /// Purpose: Whether this is a standalone (not inline) window.
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean.
    /// Complexity: O(1) time and space.
    pub(super) fn is_standalone(&self) -> bool {
        !matches!(self, EditorWindow::Vst3Inline(_))
    }

    pub(super) fn is_visible(&self) -> bool {
        match self {
            EditorWindow::Vst3(window) => !window.closed_by_user(),
            EditorWindow::Vst3Inline(_) => true,
            EditorWindow::Vst2(window) => window.is_visible(),
        }
    }

    pub(super) fn bring_to_front(&mut self) -> Result<(), String> {
        match self {
            EditorWindow::Vst3(window) => window
                .service_platform_events()
                .map_err(|error| error.to_string()),
            EditorWindow::Vst3Inline(window) => window
                .service_platform_events()
                .map_err(|error| error.to_string()),
            EditorWindow::Vst2(window) => window.focus_or_reopen(),
        }
    }

    /// 所属模块：app / 插件编辑器窗口
    /// 功能：隐藏内嵌编辑器（顶层窗口不受影响）。
    /// 输入：`self`。
    /// 输出/副作用：仅 `Vst3Inline` 调用 `ShowWindow(SW_HIDE)`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / plugin editor window
    /// Purpose: Hide an inline editor (top-level windows are unaffected).
    /// Inputs: `self`.
    /// Outputs / side effects: `Vst3Inline` calls `ShowWindow(SW_HIDE)`.
    /// Complexity: O(1) time and space.
    pub(super) fn hide_inline(&self) {
        if let EditorWindow::Vst3Inline(window) = self {
            window.set_embedded_visible(false);
        }
    }

    /// 所属模块：app / 插件编辑器窗口
    /// 功能：把内嵌编辑器放到给定矩形（逻辑点）并设置可见裁剪。
    /// 输入：
    /// - `rect`：卡片主体矩形（egui 逻辑点）。
    /// - `clip`：卷帘可见矩形（egui 逻辑点）。
    /// - `pixels_per_point`：DPI 缩放。
    /// 输出/副作用：移动/缩放子窗口并设置窗口区域；完全不可见时隐藏。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / plugin editor window
    /// Purpose: Place an inline editor at the given rect (points) and clip it to the roll.
    /// Inputs:
    /// - `rect`: card body rect in egui points.
    /// - `clip`: visible roll rect in egui points.
    /// - `pixels_per_point`: DPI scale.
    /// Outputs / side effects: moves/resizes the child window, sets its region, and hides it
    /// when the intersection is empty.
    /// Complexity: O(1) time and space.
    pub(super) fn place_inline(
        &mut self,
        rect: egui::Rect,
        clip: egui::Rect,
        pixels_per_point: f32,
    ) {
        let EditorWindow::Vst3Inline(window) = self else {
            return;
        };
        let available_x = (rect.left() * pixels_per_point).round() as i32;
        let available_y = (rect.top() * pixels_per_point).round() as i32;
        let available_width = (rect.width() * pixels_per_point).round() as i32;
        let available_height = (rect.height() * pixels_per_point).round() as i32;
        if available_width <= 0 || available_height <= 0 {
            window.set_embedded_visible(false);
            return;
        }
        // 按卡片可用空间等比缩放，并在卡片内居中。
        // Scale to fit the card and centre the editor inside it.
        let (width, height) = window.fit_embedded(available_width, available_height);
        let width = width.clamp(1, available_width);
        let height = height.clamp(1, available_height);
        let x = available_x + (available_width - width) / 2;
        let y = available_y + (available_height - height) / 2;
        window.set_embedded_bounds(x, y, width, height);

        let clip_left = (clip.left() * pixels_per_point).round() as i32;
        let clip_top = (clip.top() * pixels_per_point).round() as i32;
        let clip_right = (clip.right() * pixels_per_point).round() as i32;
        let clip_bottom = (clip.bottom() * pixels_per_point).round() as i32;
        let left = (clip_left.max(x) - x).max(0);
        let top = (clip_top.max(y) - y).max(0);
        let right = (clip_right.min(x + width) - x).max(0);
        let bottom = (clip_bottom.min(y + height) - y).max(0);
        if right <= left || bottom <= top {
            window.set_embedded_visible(false);
            return;
        }
        window.set_embedded_clip(left, top, right, bottom);
        window.set_embedded_visible(true);
    }
}

impl SurfingApp {
    pub fn cached_plugin_params(&mut self, track_id: TrackId, slot: SlotKind) -> Vec<ParamView> {
        let key = (track_id, slot);
        let now = Instant::now();
        if let Some(at) = self.plugin_param_cache_at.get(&key) {
            if now.duration_since(*at) < Duration::from_millis(200) {
                return self
                    .plugin_param_cache
                    .get(&key)
                    .cloned()
                    .unwrap_or_default();
            }
        }
        let engine_handle = self.engine.clone();
        // A background project sync may hold the engine lock for seconds while
        // plugins load; never block drawing behind it.
        let params = match engine_handle.try_lock() {
            Ok(engine) => engine.plugin_params(track_id, slot),
            Err(_) => {
                return self
                    .plugin_param_cache
                    .get(&key)
                    .cloned()
                    .unwrap_or_default();
            }
        };
        self.plugin_param_cache.insert(key, params.clone());
        self.plugin_param_cache_at.insert(key, now);
        params
    }

    pub fn invalidate_plugin_params(&mut self, track_id: TrackId) {
        self.plugin_param_cache.retain(|(id, _), _| *id != track_id);
        self.plugin_param_cache_at
            .retain(|(id, _), _| *id != track_id);
    }

    pub fn invalidate_plugin_param_slot(&mut self, track_id: TrackId, slot: SlotKind) {
        self.plugin_param_cache.remove(&(track_id, slot));
        self.plugin_param_cache_at.remove(&(track_id, slot));
    }

    /// 把实时插件实例的当前状态写回工程，供离线渲染引擎的第二套实例使用。
    /// Capture the live plugin state into the project so the offline render
    /// engine's second instance set can follow GUI edits.
    pub(super) fn capture_live_plugin_state(&mut self, track_id: TrackId) {
        let handle = self.engine.clone();
        let Ok(engine) = handle.try_lock() else {
            return;
        };
        engine.capture_track_plugin_states(track_id, &mut self.project);
    }

    /// Load the postponed plugin runtimes with a visible progress prompt.
    pub(super) fn begin_engine_load(&mut self) {
        self.engine_stale = false;
        self.needs_sync = true;
        self.defer_sync = true;
        // 接下来的同步在 UI 线程完成，不再需要“后台实例”标记。
        // The upcoming sync runs on the UI thread, so worker-instance markers are obsolete.
        self.background_synced.clear();
        self.loading_message = Some("正在加载插件…".into());
        self.loading_until = None;
        self.status = "正在加载插件…".into();
    }

    /// 所属模块：app / 插件运行时
    /// 功能：开始播放前，把后台同步线程创建的效果器实例重建到 UI 线程。
    /// 输入：`self`。
    /// 输出/副作用：把实时插件状态写回工程、标记恢复状态、按槽位失效运行时并同步；
    /// 不修改工程内容（状态除外）。
    /// 复杂度：时间 O(轨道数 + 重建插件加载)，空间 O(1)。
    ///
    /// Module: app / plugin runtime
    /// Purpose: Rebuild effect instances created by the background sync thread on the UI thread
    /// before playback starts.
    /// Inputs: `self`.
    /// Outputs / side effects: captures live plugin state, marks slots for restore, invalidates the
    /// affected runtimes and syncs; project content is unchanged apart from stored state.
    /// Complexity: O(tracks + plugin reload) time and O(1) space.
    pub(super) fn prepare_playback_plugins(&mut self) {
        if self.background_synced.is_empty() || self.engine_sync_rx.is_some() {
            return;
        }
        let slots: Vec<(TrackId, crate::audio::SlotKind)> =
            self.background_synced.drain().collect();
        let mut tracks: HashSet<TrackId> = HashSet::new();
        for (track_id, _) in &slots {
            tracks.insert(*track_id);
        }
        {
            let engine = self.engine.clone();
            if let Ok(guard) = engine.lock() {
                guard.capture_plugin_states(&mut self.project);
            };
        }
        // 只有这些槽位会重建，状态恢复也只标记它们，减少重建时的等待。
        // Only these slots are rebuilt, so mark only them for state restore to shorten the reload.
        for track in &mut self.project.tracks {
            if !tracks.contains(&track.id) {
                continue;
            }
            if let InstrumentRef::Plugin(pref) = &mut track.instrument {
                pref.restore_state = true;
            }
            for effect in &mut track.effects {
                if let Some(pref) = effect.plugin_mut() {
                    pref.restore_state = true;
                }
            }
        }
        if tracks.contains(&crate::core::model::MASTER_TRACK_ID) {
            if let Some(pref) = self
                .project
                .master_effect
                .as_mut()
                .and_then(crate::core::model::EffectRef::plugin_mut)
            {
                pref.restore_state = true;
            }
        }
        {
            let engine = self.engine.clone();
            if let Ok(mut guard) = engine.lock() {
                for (track_id, slot) in &slots {
                    guard.invalidate_plugin_slot(*track_id, *slot);
                }
            };
        }
        self.status = "正在准备插件…".into();
        self.sync_engine();
    }
    // ----- editor windows -------------------------------------------------

    pub fn service_editors(&mut self) {
        diag::set_activity("plugin.editors");
        let now = Instant::now();
        for (key, window) in &mut self.editors {
            let repaint = self
                .editor_repaint
                .get(key)
                .is_none_or(|last| now.duration_since(*last) >= EDITOR_REPAINT_BACKGROUND_INTERVAL);
            if repaint {
                self.editor_repaint.insert(*key, now);
            }
            window.service(repaint);
        }
        self.editors.retain(|(_, window)| !window.closed_by_user());
        self.editor_repaint
            .retain(|key, _| self.editors.iter().any(|(candidate, _)| candidate == key));
        if self.editors.is_empty() {
            return;
        }
        // 独立编辑器打开期间每帧把处理器输出参数反馈给控制器，驱动分析器与电平表；
        // 该服务与效果器卷帘是否可见无关。
        // While a standalone editor is open, forward processor output parameters to the
        // controller every frame so analysers and meters keep updating; this is
        // independent of whether the effects roll is visible.
        if let Ok(mut engine) = self.engine.try_lock() {
            for ((track_id, slot), _) in &self.editors {
                engine.service_editor_feedback(*track_id, *slot);
            }
        }
    }

    /// 所属模块：app / 插件编辑器
    /// 功能：返回主窗口原生句柄的数值形式。
    /// 输入：`self`。
    /// 输出/副作用：返回 `HWND`；非 Windows 或尚未获取时为 `None`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / plugin editor
    /// Purpose: Return the main window's numeric native handle.
    /// Inputs: `self`.
    /// Outputs / side effects: the `HWND`; `None` off Windows or before the first frame.
    /// Complexity: O(1) time and space.
    #[cfg(target_os = "windows")]
    fn main_window_handle(&self) -> Option<isize> {
        match self.raw_window {
            Some(RawWindowHandle::Win32(handle)) => Some(handle.hwnd.get()),
            _ => None,
        }
    }

    /// 所属模块：app / 插件编辑器
    /// 功能：非 Windows 平台没有可用的主窗口句柄。
    /// 输入：`self`。
    /// 输出/副作用：返回 `None`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / plugin editor
    /// Purpose: No native handle is available off Windows.
    /// Inputs: `self`.
    /// Outputs / side effects: `None`.
    /// Complexity: O(1) time and space.
    #[cfg(not(target_os = "windows"))]
    fn main_window_handle(&self) -> Option<isize> {
        None
    }

    /// 所属模块：app / 插件编辑器
    /// 功能：判断某个槽位是否已有内嵌编辑器。
    /// 输入：`track_id` / `slot`。
    /// 输出/副作用：返回布尔值；不修改状态。
    /// 复杂度：时间 O(编辑器数量)，空间 O(1)。
    ///
    /// Module: app / plugin editor
    /// Purpose: Whether a slot already has an inline editor.
    /// Inputs: `track_id` / `slot`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(editor count) time and O(1) space.
    pub fn has_inline_editor(&self, track_id: TrackId, slot: SlotKind) -> bool {
        self.editors.iter().any(|(key, window)| {
            *key == (track_id, slot) && matches!(window, EditorWindow::Vst3Inline(_))
        })
    }

    /// 所属模块：app / 插件编辑器
    /// 功能：判断槽位是否已经有过任何编辑器窗口（内嵌或独立）。
    /// 输入：`track_id` / `slot`。
    /// 输出/副作用：返回布尔值；不修改状态。
    /// 复杂度：时间 O(编辑器数量)，空间 O(1)。
    ///
    /// Module: app / plugin editor
    /// Purpose: Whether the slot already owns any editor window (inline or standalone).
    /// Inputs: `track_id` / `slot`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(editor count) time and O(1) space.
    pub fn has_editor_window(&self, track_id: TrackId, slot: SlotKind) -> bool {
        self.editors.iter().any(|(key, _)| *key == (track_id, slot))
    }

    /// 所属模块：app / 插件编辑器
    /// 功能：如果槽位的实例由后台同步线程创建，先在 UI 线程重建该轨道（或主输出）的插件。
    /// 输入：`track_id` / `slot`。
    /// 输出/副作用：把实时状态写回工程，丢弃旧运行时，再同步重建；不改工程内容。
    /// 复杂度：时间 O(该轨道插件加载)，空间 O(1)。
    ///
    /// Module: app / plugin editor
    /// Purpose: When the slot's instance was created by the background sync thread, rebuild
    /// that track's (or the master chain's) plugins on the UI thread first.
    /// Inputs: `track_id` / `slot`.
    /// Outputs / side effects: captures live state into the project, drops the old runtimes and
    /// syncs again; the project content is unchanged.
    /// Complexity: O(plugin load of that track) time and O(1) space.
    fn ensure_ui_thread_instance(&mut self, track_id: TrackId, slot: SlotKind) {
        if !self.background_synced.contains(&(track_id, slot)) {
            return;
        }
        // VST3 的编辑器视图必须与插件实例在同一线程创建；后台同步线程创建的实例
        // 在 UI 线程打开 GUI 会让 Auto-Tune 这类插件死锁。
        // A VST3 view must be created on the instance's thread; opening the GUI on the UI
        // thread for a worker-created instance deadlocks plugins such as Auto-Tune.
        {
            let engine = self.engine.clone();
            if let Ok(guard) = engine.lock() {
                guard.capture_plugin_states(&mut self.project);
            };
        }
        // 只标记并重建被打开的槽位，避免为了打开一个 GUI 重建整条轨道的插件链
        // （重建多个插件会在播放中造成长时间的引擎锁占用）。
        // Mark and rebuild only the opened slot instead of the whole track's plugin chain: reloading
        // several plugins would hold the engine lock long enough to starve the audio callback.
        match slot {
            SlotKind::Master => {
                if let Some(pref) = self
                    .project
                    .master_effect
                    .as_mut()
                    .and_then(crate::core::model::EffectRef::plugin_mut)
                {
                    pref.restore_state = true;
                }
            }
            SlotKind::Instrument => {
                if let Some(track) = self.project.tracks.iter_mut().find(|t| t.id == track_id) {
                    if let InstrumentRef::Plugin(pref) = &mut track.instrument {
                        pref.restore_state = true;
                    }
                }
            }
            SlotKind::Effect(index) => {
                if let Some(track) = self.project.tracks.iter_mut().find(|t| t.id == track_id) {
                    if let Some(pref) = track
                        .effects
                        .get_mut(index)
                        .and_then(crate::core::model::EffectRef::plugin_mut)
                    {
                        pref.restore_state = true;
                    }
                }
            }
        }
        self.background_synced.remove(&(track_id, slot));
        {
            let engine = self.engine.clone();
            if let Ok(mut guard) = engine.lock() {
                guard.invalidate_plugin_slot(track_id, slot);
            };
        }
        self.status = "正在重建插件实例…".into();
        self.sync_engine();
    }

    /// 所属模块：app / 插件编辑器
    /// 功能：为槽位创建内嵌 VST3 编辑器（已存在或失败过则跳过）。
    /// 输入：`track_id` / `slot`。
    /// 输出/副作用：成功时推入 `Vst3Inline`；失败时记录到 `inline_failed`，不弹独立窗口。
    /// 复杂度：时间 O(插件编辑器初始化)，空间 O(1)。
    ///
    /// Module: app / plugin editor
    /// Purpose: Create the inline VST3 editor for a slot (skips slots that already have a
    /// window or failed before).
    /// Inputs: `track_id` / `slot`.
    /// Outputs / side effects: pushes `Vst3Inline` on success; records failures in
    /// `inline_failed` instead of opening a standalone window.
    /// Complexity: O(plugin editor initialization) time and O(1) space.
    pub fn open_inline_editor(&mut self, track_id: TrackId, slot: SlotKind) -> bool {
        let key = (track_id, slot);
        if self.inline_failed.contains_key(&key) || self.has_editor_window(track_id, slot) {
            return false;
        }
        // 插件同步期间运行时不完整，等加载完成再内嵌。
        // The runtime is incomplete while a sync is pending; wait for it to finish.
        if self.engine_sync_pending() {
            return false;
        }
        self.ensure_ui_thread_instance(track_id, slot);
        // 主窗口句柄要到第一帧之后才可用；拿不到时下一帧再试，不记为失败。
        // The main handle only exists after the first frame; retry later instead of
        // recording a permanent failure.
        let Some(parent) = self.main_window_handle() else {
            return false;
        };
        let (plugin, has_editor) = {
            let engine = self.engine.clone();
            let outcome = match engine.try_lock() {
                Ok(engine) => (
                    engine.vst3_plugin(track_id, slot),
                    engine.has_editor(track_id, slot),
                ),
                // 引擎锁被音频/同步线程占用：下一帧再试。
                // The engine lock is held by audio or a sync worker: retry next frame.
                Err(_) => return false,
            };
            outcome
        };
        if !has_editor {
            // 运行时尚未就绪或插件确实没有界面；下一帧再判断，不记为失败。
            // The runtime is not ready yet, or the plugin has no editor; retry later.
            return false;
        }
        let Some(plugin) = plugin else {
            // VST1/VST2 没有可内嵌的 VST3 视图。
            // VST1/VST2 have no VST3 view to embed.
            self.inline_failed
                .insert(key, "该插件不是 VST3，无法内嵌".into());
            return false;
        };
        let mut window = PluginWindow::new(plugin);
        match window.open_embedded(parent as *mut std::ffi::c_void) {
            Ok(()) => {
                let _ = window.service_platform_events();
                self.editors.push((key, EditorWindow::Vst3Inline(window)));
                true
            }
            Err(error) => {
                let message = error.to_string();
                self.status = format!("内嵌界面失败，可双击标题在独立窗口打开: {message}");
                self.inline_failed.insert(key, message);
                false
            }
        }
    }

    /// 所属模块：app / 插件编辑器
    /// 功能：按卡片主体与卷帘可见区域摆放内嵌编辑器。
    /// 输入：`track_id` / `slot` / `rect` / `clip` / `pixels_per_point`。
    /// 输出/副作用：移动、裁剪并显示子窗口；不修改工程。
    /// 复杂度：时间 O(编辑器数量)，空间 O(1)。
    ///
    /// Module: app / plugin editor
    /// Purpose: Place an inline editor inside a card and clip it to the roll.
    /// Inputs: `track_id` / `slot` / `rect` / `clip` / `pixels_per_point`.
    /// Outputs / side effects: moves, clips and shows the child window; no project change.
    /// Complexity: O(editor count) time and O(1) space.
    pub fn place_inline_editor(
        &mut self,
        track_id: TrackId,
        slot: SlotKind,
        rect: egui::Rect,
        clip: egui::Rect,
        pixels_per_point: f32,
    ) {
        if let Some((_, window)) = self
            .editors
            .iter_mut()
            .find(|(key, _)| *key == (track_id, slot))
        {
            window.place_inline(rect, clip, pixels_per_point);
        }
        self.inline_placed.insert((track_id, slot));
    }

    /// 所属模块：app / 插件编辑器
    /// 功能：在帧末隐藏本帧没有被摆放的内嵌编辑器。
    /// 输入：`self`。
    /// 输出/副作用：对未摆放的 `Vst3Inline` 调用 `ShowWindow(SW_HIDE)`；清空本帧记录。
    /// 复杂度：时间 O(编辑器数量)，空间 O(1)。
    ///
    /// Module: app / plugin editor
    /// Purpose: Hide inline editors that were not placed during this frame.
    /// Inputs: `self`.
    /// Outputs / side effects: hides each unplaced `Vst3Inline`; clears the per-frame record.
    /// Complexity: O(editor count) time and O(1) space.
    pub(super) fn finish_inline_layout(&mut self) {
        for (key, window) in &self.editors {
            if !self.inline_placed.contains(key) {
                window.hide_inline();
            }
        }
        self.inline_placed.clear();
    }

    pub fn open_editor(&mut self, track_id: TrackId, slot: SlotKind) {
        // 插件仍在后台同步时，运行时可能还不存在；此时打开只会误报“没有编辑器”。
        // While a sync is pending the runtime may not exist yet; opening now would only
        // report a false "no editor".
        if self.engine_sync_pending() {
            self.status = "正在加载插件，请稍候…".into();
            return;
        }
        self.ensure_ui_thread_instance(track_id, slot);
        // 独立窗口与内嵌窗口互斥。
        // Standalone and inline editors are mutually exclusive.
        self.editors.retain(|(key, window)| {
            !(*key == (track_id, slot) && matches!(window, EditorWindow::Vst3Inline(_)))
        });
        // Existing window: focus it, or reopen a VST2 editor that was hidden by
        // its close button.
        if let Some((_, window)) = self
            .editors
            .iter_mut()
            .find(|(key, _)| *key == (track_id, slot))
        {
            match window.bring_to_front() {
                Ok(()) => self.status = "已切换到插件编辑器".into(),
                Err(error) => self.status = format!("打开编辑器失败: {error}"),
            }
            return;
        }
        // 绝不在 UI 线程阻塞等待引擎锁：同步线程加载插件时可能持有数秒。
        // Never block the UI thread on the engine lock: a sync worker may hold it for seconds.
        let plugin = {
            let engine = self.engine.clone();
            let outcome = match engine.try_lock() {
                Ok(engine) => engine.vst3_plugin(track_id, slot),
                Err(_) => {
                    self.status = "插件正忙，请稍后再试".into();
                    return;
                }
            };
            outcome
        };
        let has_editor = self.effect_editor_flags.get(&(track_id, slot)).copied();
        if let Some(plugin) = plugin {
            // `PluginWindow::open` sizes the editor window from the plugin's
            // own declared size and normalises editors that inflate themselves
            // (e.g. Serum 2 with a stored GUI zoom) when the plugin supports
            // content scaling.
            let span = diag::Span::start_logged(
                "app.plugin",
                "plugin.editor.open",
                &[("format", "vst3"), ("track_id", &track_id.to_string())],
            );
            diag::set_activity("plugin.editor");
            let mut window = PluginWindow::new(plugin);
            // 以主窗口为所有者：工程窗口销毁时系统会一并销毁插件 GUI，避免残留独立窗口。
            // Own the editor by the main window so Windows destroys the plugin GUI with the
            // project window instead of leaving a standalone window behind.
            let owner = self
                .main_window_handle()
                .map(|handle| handle as *mut std::ffi::c_void);
            let opened = match owner {
                Some(owner) => window.open_with_owner(Some(owner)),
                None => window.open(),
            };
            match opened {
                Ok(()) => {
                    let _ = window.service_platform_events();
                    self.editors
                        .push(((track_id, slot), EditorWindow::Vst3(window)));
                    self.status = "已打开插件编辑器".into();
                    // 记录编辑器实际尺寸：插件 GUI 空白/卡死时可据此判断是尺寸还是渲染问题。
                    // Log the editor size: a blank or frozen GUI can then be traced to sizing or
                    // rendering.
                    let size = {
                        let engine = self.engine.clone();
                        engine
                            .try_lock()
                            .ok()
                            .and_then(|engine| engine.plugin_editor_size(track_id, slot))
                    };
                    let size_text = size
                        .map(|(width, height)| format!("{width:.0}x{height:.0}"))
                        .unwrap_or_else(|| "unknown".into());
                    span.end(&[("status", "ok"), ("size", &size_text)]);
                }
                Err(error) => {
                    let error_text = error.to_string();
                    self.status = format!("打开编辑器失败: {error}");
                    span.end(&[("status", "error"), ("error", &error_text)]);
                }
            }
            diag::set_activity("ui.frame");
            return;
        }

        // VST1/VST2: open a standalone native window. `get_editor` can only be
        // taken once per plugin instance, so the window keeps the object alive
        // while it is hidden and reopens it on demand.
        let (editor, name) = {
            let engine = self.engine.clone();
            let outcome = match engine.try_lock() {
                Ok(mut engine) => (
                    engine.take_plugin_editor(track_id, slot),
                    engine.plugin_display_name(track_id, slot),
                ),
                Err(_) => {
                    self.status = "插件正忙，请稍后再试".into();
                    return;
                }
            };
            outcome
        };
        let Some(editor) = editor else {
            // 缓存显示插件有界面但运行时还没就绪时，不要说成“没有编辑器”。
            // If the cache says the plugin has an editor, the runtime is just not ready yet.
            self.status = if has_editor == Some(true) {
                "插件界面尚未就绪，请稍后再试".into()
            } else {
                "该插件没有可视化编辑器".into()
            };
            return;
        };
        let title = format!("{} - 插件", name.unwrap_or_else(|| "VST".into()));
        let span = diag::Span::start_logged(
            "app.plugin",
            "plugin.editor.open",
            &[("format", "vst2"), ("track_id", &track_id.to_string())],
        );
        diag::set_activity("plugin.editor");
        match crate::plugin::Vst2EditorWindow::new_with_owner(
            editor,
            &title,
            self.main_window_handle(),
        ) {
            Ok(window) => {
                self.editors
                    .push(((track_id, slot), EditorWindow::Vst2(window)));
                self.status = "已打开插件编辑器".into();
                span.end(&[("status", "ok")]);
            }
            Err(error) => {
                self.status = format!("打开编辑器失败: {error}");
                span.end(&[("status", "error"), ("error", &error)]);
            }
        }
        diag::set_activity("ui.frame");
    }

    pub fn close_editor(&mut self, track_id: TrackId, slot: SlotKind) {
        self.editors.retain(|(key, _)| *key != (track_id, slot));
        self.editor_repaint.remove(&(track_id, slot));
    }

    pub fn close_all_editors(&mut self) {
        self.editors.clear();
        self.editor_repaint.clear();
        // 工程/插件已变化，允许重新尝试内嵌。
        // The project or plugins changed; allow embedding attempts again.
        self.inline_failed.clear();
    }

    // ----- plugin application / scanning ---------------------------------

    /// 所属模块：app / 插件应用
    /// 功能：把选中的插件写入槽位，并在 UI 线程完成实例加载与收尾。
    /// 输入：
    /// - `target`：乐器 / 轨道效果器 / 主效果器槽。
    /// - `format` / `path` / `name` / `uid`：浏览器选中的插件。
    /// 输出/副作用：修改工程并同步引擎；名称/类型与状态由 `finish_plugin_apply` 更新。
    /// 复杂度：时间 O(插件加载)，空间 O(1)。
    ///
    /// Module: app / plugin application
    /// Purpose: Write the selected plugin into a slot and load the instance on the UI thread.
    /// Inputs:
    /// - `target`: instrument / track effect / master effect slot.
    /// - `format` / `path` / `name` / `uid`: the plugin selected in the browser.
    /// Outputs / side effects: edits the project and syncs the engine; names, kinds and the
    /// status bar are updated by `finish_plugin_apply`.
    /// Complexity: O(plugin load) time and O(1) space.
    pub fn apply_plugin(
        &mut self,
        target: crate::ui::PluginTarget,
        format: PluginFormat,
        path: PathBuf,
        name: String,
        uid: String,
    ) {
        if self.engine_sync_rx.is_some() {
            self.status = "正在加载插件，请稍候…".into();
            return;
        }
        // 停止后台插件识别：它正在 LoadLibrary，加载器锁会与本次插件加载互相阻塞。
        // Stop background classification: it is inside LoadLibrary, and the loader lock
        // would block this plugin load.
        self.cancel_plugin_probes();
        // 新插件可能可以内嵌界面，清除旧的失败记录。
        // A new plugin may embed fine; clear old failure records.
        self.inline_failed.clear();
        let mut pref = PluginRef::new(format, path, name);
        pref.uid = uid;
        let format_label = match pref.format {
            PluginFormat::Vst3 => "vst3",
            PluginFormat::Vst2 => "vst2",
        };
        let target_label = match target {
            crate::ui::PluginTarget::Instrument => "instrument",
            crate::ui::PluginTarget::Effect => "effect",
            crate::ui::PluginTarget::MasterEffect => "master_effect",
        };
        // 插件实例在下面的同步阶段创建；耗时由 `engine.sync` 区间记录。
        // The instance is created by the sync below; its duration is covered by the `engine.sync`
        // span.
        diag::info(
            "app.plugin",
            "plugin.apply",
            &[
                ("phase", "begin"),
                ("target", target_label),
                ("format", format_label),
                ("name", &pref.name),
                ("path", &pref.path.to_string_lossy()),
            ],
        );
        let track_id = self.current_track_id();
        if let Some(track_id) = track_id {
            self.invalidate_plugin_params(track_id);
        }
        self.invalidate_plugin_params(crate::core::model::MASTER_TRACK_ID);
        let mut effect_index = None;
        match target {
            crate::ui::PluginTarget::Instrument => {
                if let Some(track_id) = track_id {
                    self.close_editor(track_id, SlotKind::Instrument);
                    if let Some(track) = self.current_track_mut() {
                        track.instrument = InstrumentRef::Plugin(pref);
                        track.bump_revision();
                    }
                }
            }
            crate::ui::PluginTarget::Effect => {
                if track_id.is_some() {
                    if let Some(track) = self.current_track_mut() {
                        track
                            .effects
                            .push(crate::core::model::EffectRef::Plugin(pref));
                        effect_index = Some(track.effects.len() - 1);
                        track.bump_revision();
                    }
                }
            }
            crate::ui::PluginTarget::MasterEffect => {
                self.close_editor(0, SlotKind::Master);
                self.project.master_effect = Some(crate::core::model::EffectRef::Plugin(pref));
            }
        }
        // Adding an effect opens its chain roll straight away.
        match target {
            crate::ui::PluginTarget::Effect => {
                if let Some(track_id) = track_id {
                    self.request_roll(track_id, RollKind::Effects);
                    self.effects_scroll = 0.0;
                }
            }
            crate::ui::PluginTarget::MasterEffect => {
                self.bottom_view = BottomView::Effects {
                    track_id: crate::core::model::MASTER_TRACK_ID,
                };
                self.bottom_open = true;
                self.effects_scroll = 0.0;
            }
            crate::ui::PluginTarget::Instrument => {}
        }
        self.mark_dirty();
        // 必须在 UI 线程创建插件实例：VST3 编辑器视图与实例必须属于同一线程，
        // 放到后台同步线程会让 Auto-Tune 这类插件在打开 GUI 时死锁。
        // Create the instance on the UI thread: a VST3 view must be created on the same
        // thread as its instance; a background thread deadlocks Auto-Tune when its GUI opens.
        self.status = "正在加载插件…".into();
        self.sync_engine();
        let pending = PendingPluginApply {
            target,
            track_id,
            track_index: self.selected_track,
            effect_index,
        };
        if self.engine_sync_pending() {
            // 已有后台同步在跑：等它结束后再做收尾。
            // A background sync is already running; finish once it lands.
            self.pending_plugin_apply = Some(pending);
        } else {
            self.finish_plugin_apply(pending);
        }
    }

    /// 所属模块：app / 插件应用
    /// 功能：后台同步完成后读取加载结果、更新插件名/类型与状态栏。
    /// 输入：`apply` 插件应用时记录的槽位信息。
    /// 输出/副作用：更新工程中的插件名、插件缓存类型与状态栏；写入新实例状态。
    /// 复杂度：时间 O(插件数量)，空间 O(1)。
    ///
    /// Module: app / plugin application
    /// Purpose: After the background sync, read the load result and update names, kinds and
    /// the status bar.
    /// Inputs: `apply` — slot information recorded when the plugin was applied.
    /// Outputs / side effects: updates the plugin name, cached kind and status bar; persists
    /// the new instance's state.
    /// Complexity: O(plugin count) time and O(1) space.
    pub(super) fn finish_plugin_apply(&mut self, apply: PendingPluginApply) {
        let PendingPluginApply {
            target,
            track_id,
            track_index,
            effect_index,
        } = apply;
        if let Some(track_id) = track_id {
            // 新插件实例的状态写回工程，离线渲染的第二套实例也能拿到。
            // Persist the new instance's state so the offline renderer's second
            // instance set receives it too.
            self.capture_live_plugin_state(track_id);
        }
        let slot = match target {
            crate::ui::PluginTarget::Instrument => SlotKind::Instrument,
            crate::ui::PluginTarget::Effect => SlotKind::Effect(effect_index.unwrap_or(0)),
            crate::ui::PluginTarget::MasterEffect => SlotKind::Master,
        };
        let failure = {
            let engine = self.engine.clone();
            engine
                .lock()
                .ok()
                .and_then(|engine| engine.is_plugin_failed(track_id.unwrap_or(0), slot))
        };
        match failure {
            Some(error) => self.status = format!("插件加载失败: {error}"),
            None => {
                let engine = self.engine.clone();
                let guard = engine.lock();
                let (loaded_name, loaded_kind) = match guard {
                    Ok(engine) => (
                        engine.plugin_display_name(track_id.unwrap_or(0), slot),
                        engine.plugin_kind(track_id.unwrap_or(0), slot),
                    ),
                    Err(_) => (None, None),
                };
                if let Some(name) = loaded_name {
                    if !name.is_empty() {
                        match target {
                            crate::ui::PluginTarget::Instrument => {
                                if let Some(track) = self.project.tracks.get_mut(track_index) {
                                    if let InstrumentRef::Plugin(pref) = &mut track.instrument {
                                        pref.name = name.clone();
                                    }
                                }
                            }
                            crate::ui::PluginTarget::Effect => {
                                if let Some(track) = self.project.tracks.get_mut(track_index) {
                                    if let Some(pref) = effect_index
                                        .and_then(|index| track.effects.get_mut(index))
                                        .and_then(crate::core::model::EffectRef::plugin_mut)
                                    {
                                        pref.name = name.clone();
                                    }
                                }
                            }
                            crate::ui::PluginTarget::MasterEffect => {
                                if let Some(pref) = self
                                    .project
                                    .master_effect
                                    .as_mut()
                                    .and_then(crate::core::model::EffectRef::plugin_mut)
                                {
                                    pref.name = name.clone();
                                }
                            }
                        }
                    }
                }
                if let Some(kind) = loaded_kind {
                    let mismatch = matches!(target, crate::ui::PluginTarget::Instrument)
                        && kind == crate::plugin::PluginKind::Effect
                        || matches!(target, crate::ui::PluginTarget::Effect)
                            && kind == crate::plugin::PluginKind::Instrument;
                    if mismatch {
                        self.status =
                            format!("插件已加载，但类型为「{}」，与当前插槽不匹配", kind.label());
                    } else {
                        self.status = "插件已加载".into();
                    }
                    // Update cache classification for the plugin we just loaded.
                    if let Some(track) = self.project.tracks.get(track_index) {
                        let path = match target {
                            crate::ui::PluginTarget::Instrument => match &track.instrument {
                                InstrumentRef::Plugin(pref) => Some(pref.path.clone()),
                                _ => None,
                            },
                            crate::ui::PluginTarget::Effect => effect_index
                                .and_then(|index| track.effects.get(index))
                                .and_then(crate::core::model::EffectRef::plugin)
                                .map(|pref| pref.path.clone()),
                            crate::ui::PluginTarget::MasterEffect => self
                                .project
                                .master_effect
                                .as_ref()
                                .and_then(crate::core::model::EffectRef::plugin)
                                .map(|pref| pref.path.clone()),
                        };
                        if let Some(path) = path {
                            let mut changed = false;
                            for plugin in &mut self.plugin_cache {
                                if plugin.path == path {
                                    plugin.kind = kind;
                                    changed = true;
                                }
                            }
                            if changed {
                                save_plugin_cache(
                                    &self.plugin_cache,
                                    &self.plugin_cache_fingerprints,
                                    self.settings.cache_dir.as_deref(),
                                );
                            }
                        }
                    }
                } else {
                    self.status = "插件已加载".into();
                }
            }
        }
        let status_text = self.status.clone();
        diag::info(
            "app.plugin",
            "plugin.apply",
            &[("phase", "end"), ("status", &status_text)],
        );
    }

    /// 所属模块：app / 插件扫描 / 缓存
    /// 功能：从插件缓存中移除某个自定义目录下的全部记录与指纹，并立即写盘。
    /// 输入：`path` 被移除的自定义 VST 路径。
    /// 输出/副作用：修改 `plugin_cache` / `plugin_cache_fingerprints` 并写缓存文件。
    /// 复杂度：时间 O(插件数量)，空间 O(缓存大小)。
    ///
    /// Module: app / plugin scanning / cache
    /// Purpose: Drop every cached record and fingerprint under a removed custom directory and
    /// persist the cache immediately.
    /// Inputs: `path` — the removed custom VST path.
    /// Outputs / side effects: mutates `plugin_cache` / `plugin_cache_fingerprints` and writes the
    /// cache file.
    /// Complexity: O(plugin count) time and O(cache size) space.
    pub(super) fn prune_plugin_cache_path(&mut self, path: &Path) {
        let prefix = cache_key(path);
        self.plugin_cache
            .retain(|plugin| !cache_key(&plugin.path).starts_with(&prefix));
        self.plugin_cache_fingerprints
            .retain(|key, _| !key.starts_with(&prefix));
        self.publish_plugin_cache();
    }

    pub fn open_browser(&mut self, target: crate::ui::PluginTarget) {
        self.browser = Some(BrowserState::new(target));
        if self.plugin_cache.is_empty() && !self.scanning {
            self.start_scan();
        }
    }

    pub fn start_scan(&mut self) {
        self.start_scan_with(ScanMode::All, Vec::new());
    }

    pub fn scan_vst3_only(&mut self) {
        self.start_scan_with(ScanMode::Vst3, Vec::new());
    }

    pub fn scan_legacy_only(&mut self) {
        self.start_scan_with(ScanMode::Legacy, Vec::new());
    }

    /// 所属模块：app / 插件扫描
    /// 功能：启动启动时的增量扫描：复用缓存记录，只识别新增或变化的插件文件。
    /// 输入：`self`。
    /// 输出/副作用：启动后台线程并显示状态提示；不阻塞启动。
    /// 复杂度：时间 O(1)（扫描在后台），空间 O(1)。
    ///
    /// Module: app / plugin scanning
    /// Purpose: Start the incremental startup scan: reuse cached records and classify only new
    /// or changed plugin files.
    /// Inputs: `self`.
    /// Outputs / side effects: starts a worker and updates the status bar; never blocks startup.
    /// Complexity: O(1) time and space (the scan runs in the background).
    pub(super) fn start_startup_scan(&mut self) {
        self.start_scan_with(ScanMode::Incremental, Vec::new());
    }

    pub fn start_scan_with(&mut self, mode: ScanMode, extra_paths: Vec<PathBuf>) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        // 新扫描取代旧的识别任务。
        // A fresh scan supersedes any classification pass.
        self.cancel_plugin_probes();
        self.status = match mode {
            ScanMode::All => "正在扫描 VST3 / VST2 插件…".into(),
            ScanMode::Vst3 => "正在扫描 VST3 插件…".into(),
            ScanMode::Legacy => "正在扫描 VST / VST2 插件…".into(),
            ScanMode::Incremental => "正在检查插件目录（复用已有记录）…".into(),
        };
        let cached = self.plugin_cache.clone();
        let cached_fingerprints = self.plugin_cache_fingerprints.clone();
        // 设置中的自定义 VST 路径与本次一次性路径合并，去重后交给后台扫描。
        // Merge the persisted custom VST paths with this call's one-off paths.
        let mut custom_paths = self.settings.custom_vst_paths.clone();
        for path in extra_paths {
            if !custom_paths
                .iter()
                .any(|existing| cache_key(existing) == cache_key(&path))
            {
                custom_paths.push(path);
            }
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.scan_rx = Some(rx);
        let mode_label = match mode {
            ScanMode::All => "all",
            ScanMode::Vst3 => "vst3",
            ScanMode::Legacy => "legacy",
            ScanMode::Incremental => "incremental",
        };
        diag::info(
            "app.plugin",
            "plugin.scan",
            &[
                ("phase", "begin"),
                ("mode", mode_label),
                ("cached", &cached.len().to_string()),
            ],
        );
        std::thread::spawn(move || {
            let span = diag::Span::start("app.plugin", "plugin.scan.worker");
            let outcome = match mode {
                ScanMode::Incremental => {
                    scan_incremental(cached, cached_fingerprints, custom_paths)
                }
                _ => scan_full(mode, &custom_paths),
            };
            match &outcome {
                Ok(found) => span.end(&[
                    ("status", "ok"),
                    ("mode", mode_label),
                    ("plugins", &found.plugins.len().to_string()),
                    ("reused", &found.reused.to_string()),
                ]),
                Err(error) => {
                    span.end(&[("status", "error"), ("mode", mode_label), ("error", error)])
                }
            }
            let _ = tx.send(outcome);
        });
    }

    pub(super) fn poll_scan(&mut self) {
        let outcome = match &self.scan_rx {
            None => return,
            Some(receiver) => match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err("扫描线程意外结束".to_string()))
                }
            },
        };
        let Some(result) = outcome else {
            return;
        };
        self.scan_rx = None;
        self.scanning = false;
        match result {
            Ok(outcome) => {
                let ScanOutcome {
                    mut plugins,
                    mut fingerprints,
                    reused,
                } = outcome;
                plugins.sort_by(|a, b| {
                    a.name
                        .to_lowercase()
                        .cmp(&b.name.to_lowercase())
                        .then_with(|| a.vendor.cmp(&b.vendor))
                });
                // 保留上次已识别的类型与 CID，避免重复识别。
                // Keep previously classified kinds and CIDs so they are not probed twice.
                let previous = std::mem::take(&mut self.plugin_cache);
                for entry in &mut plugins {
                    if entry.kind != crate::plugin::PluginKind::Unknown {
                        continue;
                    }
                    let Some(old) = previous.iter().find(|old| old.path == entry.path) else {
                        continue;
                    };
                    if old.kind == crate::plugin::PluginKind::Unknown {
                        continue;
                    }
                    entry.kind = old.kind;
                    entry.category = old.category.clone();
                    entry.vendor = old.vendor.clone();
                    entry.name = old.name.clone();
                    entry.uid = old.uid.clone();
                    entry.has_midi_input = old.has_midi_input;
                }
                // 只扫描单一格式时，保留另一种格式的既有记录。
                // A single-format scan keeps the existing records of the other format.
                let covered: HashSet<PluginFormat> =
                    plugins.iter().map(|plugin| plugin.format).collect();
                for old in previous {
                    if !covered.contains(&old.format) {
                        let key = cache_key(&old.path);
                        if !fingerprints.contains_key(&key) && old.path.exists() {
                            fingerprints.insert(key, plugin_fingerprint(&old.path));
                        }
                        plugins.push(old);
                    }
                }
                // 同一路径在两个目录出现时只保留一条。
                // Keep one record per path when the same plugin appears under two roots.
                let mut seen = HashSet::new();
                plugins.retain(|plugin| seen.insert(cache_key(&plugin.path)));
                let vst3 = plugins
                    .iter()
                    .filter(|p| p.format == PluginFormat::Vst3)
                    .count();
                let vst2 = plugins.len() - vst3;
                self.plugin_cache = plugins;
                self.plugin_cache_fingerprints = fingerprints;
                self.publish_plugin_cache();
                self.status = if reused > 0 {
                    format!(
                        "扫描完成：{vst3} 个 VST3，{vst2} 个 VST / VST2（复用 {reused} 条缓存）"
                    )
                } else {
                    format!("扫描完成：{vst3} 个 VST3，{vst2} 个 VST / VST2")
                };
                self.start_plugin_probes();
            }
            Err(error) => {
                self.status = format!("插件扫描失败: {error}");
            }
        }
    }

    /// 所属模块：app / 插件扫描
    /// 功能：为快速扫描中类型未知的 VST3 启动后台识别。
    /// 输入：`self`。
    /// 输出/副作用：创建识别通道与后台线程；无未知插件时不启动。
    /// 复杂度：时间 O(插件数量)，空间 O(插件数量)。
    ///
    /// Module: app / plugin scanning
    /// Purpose: Start background classification for VST3 plugins left unknown by the fast scan.
    /// Inputs: `self`.
    /// Outputs / side effects: creates the channel and worker; does nothing without unknowns.
    /// Complexity: O(plugin count) time and space.
    fn start_plugin_probes(&mut self) {
        let paths: Vec<PathBuf> = self
            .plugin_cache
            .iter()
            .filter(|plugin| {
                plugin.format == PluginFormat::Vst3
                    && plugin.kind == crate::plugin::PluginKind::Unknown
            })
            .map(|plugin| plugin.path.clone())
            .collect();
        if paths.is_empty() {
            self.plugin_probe_rx = None;
            self.plugin_probe_cancel = None;
            self.classifying = false;
            return;
        }
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.plugin_probe_rx = Some(rx);
        self.plugin_probe_cancel = Some(cancel);
        self.classifying = true;
        self.status = format!("扫描完成，正在识别 {} 个插件类型…", paths.len());
        std::thread::spawn(move || {
            use std::sync::atomic::Ordering;
            for path in paths {
                // 用户开始加载插件/重新扫描时立即让出加载器锁。
                // Yield the loader lock as soon as the user loads a plugin or rescans.
                if worker_cancel.load(Ordering::Relaxed) {
                    break;
                }
                if let Some(plugin) = PluginHost::probe_plugin_metadata(PluginFormat::Vst3, &path) {
                    if tx.send(plugin).is_err() {
                        break;
                    }
                }
                // 逐个识别并留出间隙，避免连续 LoadLibrary 抢占 UI/音频线程。
                // Classify one at a time with a small gap so back-to-back LoadLibrary calls
                // do not starve the UI or audio threads.
                std::thread::sleep(Duration::from_millis(10));
            }
        });
    }

    /// 所属模块：app / 插件扫描
    /// 功能：停止后台插件类型识别并释放加载器锁。
    /// 输入：`self`。
    /// 输出/副作用：置取消标志、丢弃接收端、清除“识别中”状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / plugin scanning
    /// Purpose: Stop background classification and release the loader lock.
    /// Inputs: `self`.
    /// Outputs / side effects: sets the cancel flag, drops the receiver, clears the
    /// "classifying" state.
    /// Complexity: O(1) time and space.
    pub(super) fn cancel_plugin_probes(&mut self) {
        if let Some(cancel) = self.plugin_probe_cancel.take() {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.plugin_probe_rx = None;
        self.classifying = false;
    }

    /// 所属模块：app / 插件扫描
    /// 功能：接收后台识别结果并更新插件缓存。
    /// 输入：`self`。
    /// 输出/副作用：替换对应路径的插件记录；后台任务结束时更新状态。
    /// 复杂度：时间 O(本帧结果数 × 插件数)，空间 O(本帧结果数)。
    ///
    /// Module: app / plugin scanning
    /// Purpose: Apply background classification results to the plugin cache.
    /// Inputs: `self`.
    /// Outputs / side effects: replaces matching records; updates status when the worker ends.
    /// Complexity: O(results this frame × plugin count) time and O(results this frame) space.
    pub(super) fn poll_plugin_probes(&mut self) {
        let (updates, finished) = {
            let Some(receiver) = &self.plugin_probe_rx else {
                return;
            };
            let mut updates = Vec::new();
            let mut finished = false;
            loop {
                match receiver.try_recv() {
                    Ok(plugin) => updates.push(plugin),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
            (updates, finished)
        };
        for update in updates {
            if let Some(slot) = self
                .plugin_cache
                .iter_mut()
                .find(|plugin| plugin.path == update.path)
            {
                *slot = update;
            }
        }
        if finished {
            self.plugin_probe_rx = None;
            self.plugin_probe_cancel = None;
            self.classifying = false;
            // 分类结果在一次识别结束时统一写回缓存文件，避免逐项写盘。
            // Persist classification results once when the pass ends instead of per item.
            self.publish_plugin_cache();
            self.status = format!("扫描完成：{} 个插件", self.plugin_cache.len());
        }
    }
}

/// 所属模块：app / 插件扫描
/// 功能：执行完整扫描（全部 / 仅 VST3 / 仅 VST2），并记录每个插件文件的指纹。
/// 输入：
/// - `mode`：扫描范围。
/// - `extra_paths`：自定义插件目录。
/// 输出/副作用：返回扫描结果与指纹；只读文件系统，不修改应用状态。
/// 复杂度：时间 O(目录项数 × 递归深度 + VST2 实例化)，空间 O(插件数量)。
///
/// Module: app / plugin scanning
/// Purpose: Run a full scan (all / VST3 only / VST2 only) and record a fingerprint for every
/// plugin file.
/// Inputs:
/// - `mode`: scan scope.
/// - `extra_paths`: custom plugin directories.
/// Outputs / side effects: scan results and fingerprints; the filesystem is read-only and no
/// app state is modified.
/// Complexity: O(directory entries × depth + VST2 instantiation) time and O(plugin count) space.
fn scan_full(mode: ScanMode, extra_paths: &[PathBuf]) -> Result<ScanOutcome, String> {
    let mut plugins = Vec::new();
    let mut errors = Vec::new();
    if matches!(mode, ScanMode::All | ScanMode::Vst3) {
        match PluginHost::scan_plugin_format(PluginFormat::Vst3, extra_paths) {
            Ok(mut found) => plugins.append(&mut found),
            Err(error) => errors.push(format!("VST3: {error}")),
        }
    }
    if matches!(mode, ScanMode::All | ScanMode::Legacy) {
        match PluginHost::scan_plugin_format(PluginFormat::Vst2, extra_paths) {
            Ok(mut found) => plugins.append(&mut found),
            Err(error) => errors.push(format!("VST2: {error}")),
        }
    }
    if plugins.is_empty() && !errors.is_empty() {
        return Err(errors.join(" · "));
    }
    let fingerprints = plugins
        .iter()
        .map(|plugin| (cache_key(&plugin.path), plugin_fingerprint(&plugin.path)))
        .collect();
    Ok(ScanOutcome {
        plugins,
        fingerprints,
        reused: 0,
    })
}

/// 所属模块：app / 插件扫描
/// 功能：执行增量扫描：复用指纹未变化的缓存记录，只识别新增、变化或缺失的插件文件。
/// 输入：
/// - `cached`：上次保存的插件记录。
/// - `cached_fingerprints`：上次保存的文件指纹。
/// - `custom_paths`：设置中的自定义 VST 扫描路径。
/// 输出/副作用：返回合并后的扫描结果与最新指纹；只读文件系统。
/// 复杂度：时间 O(目录项数 × 递归深度 + 新插件识别数)，空间 O(插件数量)。
///
/// Module: app / plugin scanning
/// Purpose: Run an incremental scan: reuse cached records whose fingerprints are unchanged and
/// classify only new, changed or missing plugin files.
/// Inputs:
/// - `cached`: previously saved plugin records.
/// - `cached_fingerprints`: previously saved file fingerprints.
/// - `custom_paths`: custom VST scan paths from the settings.
/// Outputs / side effects: merged scan results and fresh fingerprints; read-only filesystem.
/// Complexity: O(directory entries × depth + new plugins) time and O(plugin count) space.
fn scan_incremental(
    cached: Vec<ScannedPlugin>,
    cached_fingerprints: HashMap<String, (u64, u64)>,
    custom_paths: Vec<PathBuf>,
) -> Result<ScanOutcome, String> {
    let files = crate::plugin::list_plugin_files(&custom_paths);
    let mut plugins: Vec<ScannedPlugin> = Vec::new();
    let mut fingerprints: HashMap<String, (u64, u64)> = HashMap::new();
    let mut reused = 0_usize;
    let cached_by_path: HashMap<String, ScannedPlugin> = cached
        .into_iter()
        .map(|plugin| (cache_key(&plugin.path), plugin))
        .collect();
    let mut new_vst3: Vec<PathBuf> = Vec::new();
    let mut vst2_parents: Vec<PathBuf> = Vec::new();

    for (format, path) in files {
        let key = cache_key(&path);
        let fingerprint = plugin_fingerprint(&path);
        fingerprints.insert(key.clone(), fingerprint);
        if let Some(old) = cached_by_path.get(&key) {
            if cached_fingerprints.get(&key) == Some(&fingerprint) {
                plugins.push(old.clone());
                reused += 1;
                continue;
            }
        }
        match format {
            PluginFormat::Vst3 => new_vst3.push(path),
            PluginFormat::Vst2 => {
                let parent = path.parent().unwrap_or(&path).to_path_buf();
                if !vst2_parents
                    .iter()
                    .any(|existing| cache_key(existing) == cache_key(&parent))
                {
                    vst2_parents.push(parent);
                }
            }
        }
    }

    // 标准目录之外的自定义插件：文件仍存在就沿用缓存记录。
    // Custom plugins outside the standard roots: keep their records while the file exists.
    for (key, old) in &cached_by_path {
        if fingerprints.contains_key(key) {
            continue;
        }
        if old.path.exists() {
            fingerprints.insert(key.clone(), plugin_fingerprint(&old.path));
            plugins.push(old.clone());
        }
    }

    // 新增或变化的 VST3 只读 moduleinfo.json，避免实例化引发授权窗口。
    // New or changed VST3 entries read moduleinfo.json only, avoiding licence windows from
    // instantiation.
    if !new_vst3.is_empty() {
        plugins.extend(crate::plugin::scan_vst3_paths(&new_vst3));
    }

    // 新增或变化的 VST2 只扫描相关目录，避免每次启动实例化全部 DLL。
    // New or changed VST2 entries scan only their directories, so startup never instantiates
    // every DLL again.
    if !vst2_parents.is_empty() {
        if let Ok(mut found) = PluginHost::scan_plugin_format(PluginFormat::Vst2, &vst2_parents) {
            plugins.append(&mut found);
        }
    }

    Ok(ScanOutcome {
        plugins,
        fingerprints,
        reused,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：增量扫描在第二次运行时复用全部指纹未变化的记录。
    /// Covers: a second incremental scan reuses every record whose fingerprint is unchanged.
    #[test]
    #[ignore = "reads the local VST installation"]
    fn incremental_scan_reuses_unchanged_plugins() {
        let first = scan_incremental(Vec::new(), HashMap::new(), Vec::new()).expect("first scan");
        let total = first.plugins.len();
        let second =
            scan_incremental(first.plugins, first.fingerprints, Vec::new()).expect("second scan");
        assert_eq!(second.plugins.len(), total, "plugin count must stay stable");
        assert_eq!(
            second.reused, total,
            "every unchanged plugin should be reused"
        );
    }
}
