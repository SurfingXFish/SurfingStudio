//! 所属模块：app / 文件操作
//! 文件用途：
//! - 新建工程窗口、加载（只调用工程运行时框架 `ProjectRuntime`）、保存工程与 WAV 导出。
//! - 轮询后台加载/保存线程结果，并维护工程路径、未保存标记与导入警告。
//!
//! 函数清单：
//! - `reset_new_project` / `reset_project_from_template` / `reset_project_with` / `request_new_project`：初始化空白/模板工程、请求外壳打开新工程窗口。
//! - `open_project_dialog` / `begin_load_project` / `poll_load_project`：原生对话框、后台框架加载与结果应用。
//! - `apply_loaded_project`：应用框架返回的工程会话，恢复峰值、延迟同步；导入工程清空原路径。
//! - `save_project` / `save_project_as` / `save_project_in_background` / `poll_save_project`：
//!   抓取插件状态、后台框架保存与保存结果轮询（含自动保存与关闭确认后的关闭）。
//! - `collect_peak_entries`：收集编曲区波形峰值。
//! - `export_wav_dialog` / `pause_transport`：离线导出与导出前暂停。
//! - `summarize_warnings`：压缩导入警告用于状态栏。
//!
//! Module: app / file operations
//! Purpose:
//! - Create project windows, load (only through the `ProjectRuntime` framework), save projects and
//!   export WAV.
//! - Poll background load/save workers and maintain the project path, dirty flag
//!   and import warnings.
//!
//! Functions:
//! - `reset_new_project` / `reset_project_from_template` / `reset_project_with` / `request_new_project`:
//!   initialize an empty/template project and ask the shell to open a new project window.
//! - `open_project_dialog` / `begin_load_project` / `poll_load_project`: native
//!   dialog, background framework load and result application.
//! - `apply_loaded_project`: apply the framework session, restore peaks and defer
//!   sync; imported projects clear the original path.
//! - `save_project` / `save_project_as` / `save_project_in_background` / `poll_save_project`:
//!   capture plugin state, save through the framework in the background and poll the result
//!   (including autosave and closing after the close confirmation).
//! - `collect_peak_entries`: collect waveform peaks for the arrangement.
//! - `export_wav_dialog` / `pause_transport`: offline export and pre-export pause.
//! - `summarize_warnings`: compress import warnings for the status bar.

use super::*;
use crate::diag;
use crate::platform::DialogParent;

/// 所属模块：app / 文件操作 / 默认位置
/// 功能：返回没有系统文件对话框时的默认工程目录（平台数据目录下的 `Projects`）。
/// 输入：无。
/// 输出/副作用：目录路径或 `None`；不创建目录。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: app / file operations / default location
/// Purpose: Return the default project directory used when no native file dialog exists
/// (`Projects` under the platform data directory).
/// Inputs: none.
/// Outputs / side effects: the path or `None`; nothing is created.
/// Complexity: O(path length) time and space.
fn default_project_dir() -> Option<PathBuf> {
    crate::platform::config_dir().map(|base| base.join("Projects"))
}

/// 所属模块：app / 文件操作 / 公共目录
/// 功能：返回“导出副本”使用的临时目录（系统临时目录下的 `SurfingStudio/export`），并确保存在。
/// 输入：无。
/// 输出/副作用：创建目录；返回路径或 `None`（不可写）。
/// 复杂度：时间 O(1)，空间 O(路径长度)。
///
/// Module: app / file operations / public folder
/// Purpose: Return the scratch directory used by "export a copy"
/// (`SurfingStudio/export` under the temp directory) and make sure it exists.
/// Inputs: none.
/// Outputs / side effects: creates the directory; returns the path, or `None` when unwritable.
/// Complexity: O(1) time and O(path length) space.
fn export_scratch_dir() -> Option<PathBuf> {
    let dir = crate::platform::temp_dir()
        .join(crate::platform::APP_DIR_NAME)
        .join("export");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

impl SurfingApp {
    // ----- files ---------------------------------------------------------

    /// 所属模块：app / 文件操作
    /// 功能：返回本窗口的对话框父窗口；没有原生句柄（非 Windows 或尚未解析）时返回 `None`。
    /// 输入：`self`。
    /// 输出/副作用：父窗口句柄；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / file operations
    /// Purpose: Return this window's dialog parent; `None` without a native handle (off Windows or
    /// not resolved yet).
    /// Inputs: `self`.
    /// Outputs / side effects: the parent handle; no state change.
    /// Complexity: O(1) time and space.
    pub(super) fn dialog_parent(&self) -> Option<DialogParent> {
        match (self.raw_window, self.raw_display) {
            (Some(window), Some(display)) => Some(DialogParent { window, display }),
            _ => None,
        }
    }

    /// 所属模块：app / 文件操作
    /// 功能：把当前窗口初始化为一个空白工程（用于新建工程窗口）。
    /// 输入：`self`。
    /// 输出/副作用：替换工程并清空选择、卷帘与波形缓存；`dirty` / `needs_sync` 置位。
    /// 复杂度：时间 O(1)（新工程只有一个轨道），空间 O(1)。
    ///
    /// Module: app / file operations
    /// Purpose: Initialize this window with an empty project (used for a new project window).
    /// Inputs: `self`.
    /// Outputs / side effects: replaces the project and clears the selection, rolls and waveform
    /// cache; sets `dirty` / `needs_sync`.
    /// Complexity: O(1) time (the new project has one track) and O(1) space.
    pub(super) fn reset_new_project(&mut self) {
        self.reset_project_with(Project::empty(), "已新建工程".into());
    }

    /// 所属模块：app / 文件操作
    /// 功能：用模板/范例预填充当前窗口，并重置编辑状态。
    /// 输入：`template` 模板或范例标识。
    /// 输出/副作用：替换工程并清空选择、卷帘与波形缓存；未知标识回退为空白工程。
    /// 复杂度：时间 O(模板音符数)，空间 O(模板音符数)。
    ///
    /// Module: app / file operations
    /// Purpose: Pre-fill this window from a template/example and reset the editing state.
    /// Inputs: `template` template or example identifier.
    /// Outputs / side effects: replaces the project and clears selection, rolls and waveform cache;
    /// an unknown id falls back to an empty project.
    /// Complexity: O(template notes) time and space.
    pub(super) fn reset_project_from_template(&mut self, template: TemplateId) {
        match Project::from_template(template) {
            Some(project) => {
                let name = project.name.clone();
                self.reset_project_with(project, format!("已从模板「{name}」创建工程"));
            }
            None => self.reset_project_with(Project::empty(), "模板不可用，已新建空白工程".into()),
        }
    }

    /// 所属模块：app / 文件操作
    /// 功能：替换工程并清空选择、卷帘、波形缓存与撤销栈；工程没有文件路径，因此标记未保存。
    /// 输入：
    /// - `project`：新的工程内容。
    /// - `status`：状态栏提示。
    /// 输出/副作用：`dirty` / `needs_sync` 置位；工程代次递增。
    /// 复杂度：时间 O(工程轨道数)，空间 O(1)（工程所有权转移）。
    ///
    /// Module: app / file operations
    /// Purpose: Replace the project and clear selection, rolls, waveform cache and history; the
    /// project has no file path, so it is marked dirty.
    /// Inputs:
    /// - `project`: new project content.
    /// - `status`: status-bar message.
    /// Outputs / side effects: sets `dirty` / `needs_sync`; advances the project epoch.
    /// Complexity: O(track count) time and O(1) space (the project is moved).
    fn reset_project_with(&mut self, project: Project, status: String) {
        self.project_epoch = self.project_epoch.wrapping_add(1);
        self.project = project;
        self.project_path = None;
        self.reset_autosave_state();
        self.clear_history();
        self.selected_track = 0;
        self.selected_clip = None;
        self.selected_note = None;
        self.selected_clips.clear();
        self.selected_notes.clear();
        self.selected_effect = None;
        self.selected_controller = None;
        self.selected_effects_lane = None;
        self.piano_track = None;
        self.piano_views.clear();
        self.ui_waveforms.clear();
        self.view_start = 0.0;
        self.bottom_view = BottomView::PianoRoll;
        self.bottom_open = false;
        self.dirty = true;
        self.needs_sync = true;
        self.view = WorkspaceView::Studio;
        self.status = status;
    }

    /// 所属模块：app / 文件操作
    /// 功能：请求外壳打开一个新的空白工程窗口；当前窗口与工程保持不变。
    /// 输入：`self`。
    /// 输出/副作用：向 `pending_window_actions` 追加 `NewProject { template: None }`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / file operations
    /// Purpose: Ask the shell to open a new empty project window; the current window and project
    /// stay untouched.
    /// Inputs: `self`.
    /// Outputs / side effects: pushes `NewProject { template: None }` onto `pending_window_actions`.
    /// Complexity: O(1) time and space.
    pub(super) fn request_new_project(&mut self) {
        self.pending_window_actions
            .push(WindowAction::NewProject { template: None });
        self.status = "正在打开新工程窗口…".into();
    }

    /// 所属模块：app / 文件操作
    /// 功能：在后台线程打开原生文件对话框，支持 `.sfsd` 与 SoundBug `.sndp`。
    /// 输入：`self`。
    ///
    /// 输出/副作用：设置 `dialog_rx`；对话框结果由 `poll_load_project` 消费，并在外壳中打开新窗口。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / file operations
    /// Purpose: Open the native file dialog on a worker thread, accepting `.sfsd`
    /// and SoundBug `.sndp`.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: sets `dialog_rx`; `poll_load_project` consumes the result and the
    /// shell opens a new project window for it.
    ///
    /// Complexity: O(1) time and space.
    pub fn open_project_dialog(&mut self) {
        if self.dialog_rx.is_some() || self.load_rx.is_some() {
            return;
        }
        // 首次打开原生对话框可能较慢；放到工作线程还能让加载提示正常绘制。
        // Show the native dialog on a worker thread: opening it the first time
        // can take a while, and a blocking dialog also prevents the progress
        // prompt from ever being painted.
        let (tx, rx) = channel();
        // 以主窗口为父窗口，保证对话框模态并显示在最前面。
        // Parent the dialog to the main window so it stays modal and is shown
        // in front even though it runs on a worker thread.
        let parent = self.dialog_parent();
        // 打开对话框默认从“默认存储地址”开始；未设置时沿用系统上次位置。
        // The open dialog starts from the configured default project directory when present.
        let default_dir = self.settings.default_project_dir.clone();
        std::thread::spawn(move || {
            // 桌面走原生对话框；安卓走 SAF 选择器并把文件复制进应用沙箱（都在工作线程等待）。
            // Desktop uses the native dialog; Android uses the SAF picker and copies the file into
            // the sandbox (both block on this worker thread).
            let filters: &[(&str, &[&str])] = &[
                ("SurfingStudio 工程", &["sfsd"]),
                ("SoundBug 工程", &["sndp"]),
                ("所有文件", &["*"]),
            ];
            let request = crate::platform::FileRequest {
                title: "加载工程",
                filters,
                start_dir: default_dir.as_deref(),
                default_name: None,
                parent,
            };
            let _ = tx.send(crate::platform::import_file(&request));
        });
        self.dialog_rx = Some(rx);
    }

    /// 所属模块：app / 文件操作
    /// 功能：在后台线程通过工程运行时框架加载工程，并显示加载提示。
    /// 输入：
    /// - `path`：用户选择的工程文件路径。
    ///
    /// 输出/副作用：设置 `loading_message` 与 `load_rx`；结果由 `poll_load_project` 消费。
    ///
    /// 复杂度：时间 O(1)（解析在后台线程），空间 O(1)。
    ///
    /// Module: app / file operations
    /// Purpose: Load a project through the `ProjectRuntime` framework on a worker
    /// thread, showing the loading prompt.
    /// Inputs:
    /// - `path`: the project file selected by the user.
    ///
    /// Outputs / side effects: sets `loading_message` and `load_rx`; the result is
    /// consumed by `poll_load_project`.
    ///
    /// Complexity: O(1) time (parsing runs on the worker thread) and O(1) space.
    pub(super) fn begin_load_project(&mut self, path: PathBuf) {
        // 开始加载即进入工作室，加载提示与后续插件同步都绘制在编曲界面上。
        // Enter the studio as soon as loading starts so the prompt and plugin sync are painted
        // over the arrangement.
        self.view = WorkspaceView::Studio;
        self.loading_message = Some("正在加载工程…".into());
        self.loading_until = None;
        self.status = "正在加载工程…".into();
        // 切换工程前立即停止播放并关闭独立编辑器，避免旧工程插件在后台加载期间仍被
        // 音频线程处理、或编辑器晚于插件实例释放。
        // Stop playback and close standalone editors as soon as a switch begins, so old
        // plug-ins are not processed while the new project loads or outlive their runtimes.
        self.release_held_notes();
        if let Ok(mut engine) = self.engine.try_lock() {
            engine.pause();
            engine.reset_playback_state();
        }
        self.ui_playing = false;
        self.pending_play = false;
        self.pending_plugin_apply = None;
        self.close_all_editors();
        let (tx, rx) = channel();
        let path_text = path.to_string_lossy().to_string();
        diag::info(
            "app.project",
            "project.load",
            &[("phase", "begin"), ("path", &path_text)],
        );
        std::thread::spawn(move || {
            // 格式分派、解码与转译全部由框架完成，应用层不接触任何后端模块。
            // Format dispatch, decoding and translation are all handled by the
            // framework; the app never touches backend modules.
            let span = diag::Span::start("app.project", "project.load.worker");
            let result = crate::core::project_runtime::ProjectRuntime::open_project(&path);
            match &result {
                Ok(_) => span.end(&[("status", "ok"), ("path", &path_text)]),
                Err(error) => {
                    span.end(&[("status", "error"), ("path", &path_text), ("error", error)])
                }
            }
            let _ = tx.send(result);
        });
        self.load_rx = Some(rx);
    }

    /// 所属模块：app / 文件操作
    /// 功能：非阻塞轮询对话框、加载结果与暂存工程，并在下一帧应用。
    /// 输入：`self`。
    ///
    /// 输出/副作用：可能替换工程、修改状态栏与加载提示；不阻塞 UI。
    ///
    /// 复杂度：时间 O(1)/帧（工程替换另计），空间 O(1)。
    ///
    /// Module: app / file operations
    /// Purpose: Non-blocking poll of the dialog, load result and pending project,
    /// applying it one frame later.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: may replace the project and update the status bar and
    /// loading prompt; never blocks the UI.
    ///
    /// Complexity: O(1) per frame (project replacement excluded), O(1) space.
    pub(super) fn poll_load_project(&mut self) {
        // 1. 原生对话框结果。
        // 1. Native file dialog result.
        if let Some(rx) = &self.dialog_rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.dialog_rx = None;
                    match result {
                        // 打开工程总是新建一个工程窗口：当前窗口的工程不会被替换。
                        // Opening a project always creates a project window; the current project
                        // is never replaced.
                        Ok(Some(path)) => self
                            .pending_window_actions
                            .push(WindowAction::OpenProject { path }),
                        Ok(None) => {
                            self.status = "已取消加载工程".into();
                        }
                        Err(message) => self.status = message,
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.dialog_rx = None;
                    self.loading_message = None;
                    self.loading_until = None;
                }
            }
        }

        // 2. 已解析的工程延后一帧应用，确保加载提示至少绘制一帧。
        // 2. A parsed project waits for the next frame so the prompt that was
        //    set when it arrived has definitely been painted before the UI
        //    thread takes over the project.
        if let Some(session) = self.loaded_project.take() {
            self.apply_loaded_project(session);
        }

        // 3. 后台读取/解析结果。
        // 3. Background read/parse.
        let Some(rx) = &self.load_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(session)) => {
                self.load_rx = None;
                self.loaded_project = Some(session);
                self.loading_message = Some("正在加载工程…".into());
                self.loading_until = None;
            }
            Ok(Err(error)) => {
                self.load_rx = None;
                self.loading_message = None;
                self.loading_until = None;
                self.status = error;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.load_rx = None;
                self.loading_message = None;
                self.loading_until = None;
                self.status = "加载工程失败: 读取线程中断".into();
            }
        }
    }

    /// 所属模块：app / 文件操作
    /// 功能：应用工程运行时框架返回的会话，清理旧状态并准备插件同步。
    /// 输入：
    /// - `session`：框架统一返回的工程会话（工程、峰值、警告、来源与原始路径）。
    ///
    /// 输出/副作用：
    /// - 替换 `self.project`，清空缓存与撤销栈；导入工程将 `project_path = None`、`dirty = true`。
    ///
    /// 复杂度：时间 O(轨道数 + 片段数 + 峰值条目数)，空间 O(1)（工程所有权转移）。
    ///
    /// Module: app / file operations
    /// Purpose: Apply the session returned by the `ProjectRuntime` framework, clear
    /// stale state and prepare plugin sync.
    /// Inputs:
    /// - `session`: unified framework session (project, peaks, warnings, source and origin).
    ///
    /// Outputs / side effects:
    /// - Replaces `self.project`, clears caches and history; imported projects and history versions
    ///   set `project_path = None` (imports also set `dirty = true`).
    ///
    /// Complexity: O(track count + clip count + peak entries) time and O(1) extra
    /// space (the project is moved).
    fn apply_loaded_project(&mut self, session: ProjectSession) {
        let is_imported = session.source.is_imported();
        let source_label = session.source.label();
        let ProjectSession {
            origin: path,
            project,
            peaks,
            warnings,
            ..
        } = session;
        let path_text = path.to_string_lossy().to_string();
        let tracks_text = project.tracks.len().to_string();
        let clips_text = project
            .tracks
            .iter()
            .map(|track| track.clips.len())
            .sum::<usize>()
            .to_string();
        // 关闭工程时插件实例必须在 UI 线程析构；先记录本工程用到哪些插件，
        // 便于在卡死/崩溃后定位具体插件。
        // Plugin instances are destroyed on the UI thread when the project closes; record which
        // plugins this project uses so a hang/crash can be traced to a specific plugin.
        let mut plugin_names: Vec<String> = Vec::new();
        for track in &project.tracks {
            if let InstrumentRef::Plugin(plugin) = &track.instrument {
                plugin_names.push(format!("inst:{} [{}]", plugin.name, plugin.path.display()));
            }
            for effect in &track.effects {
                if let Some(plugin) = effect.plugin() {
                    plugin_names.push(format!("fx:{} [{}]", plugin.name, plugin.path.display()));
                }
            }
        }
        if let Some(plugin) = project
            .master_effect
            .as_ref()
            .and_then(crate::core::model::EffectRef::plugin)
        {
            plugin_names.push(format!(
                "master:{} [{}]",
                plugin.name,
                plugin.path.display()
            ));
        }
        let plugin_count = plugin_names.len().to_string();
        let plugin_list = plugin_names.join(", ");
        diag::info(
            "app.project",
            "project.plugins",
            &[("count", &plugin_count), ("list", &plugin_list)],
        );
        let span = diag::Span::start_logged(
            "app.project",
            "project.apply",
            &[
                ("path", &path_text),
                ("imported", if is_imported { "true" } else { "false" }),
            ],
        );
        self.release_held_notes();
        // 先停止音频线程再关闭编辑器，避免处理器仍在运行时 detach 视图。
        // Stop the audio thread before closing editors, so views are not detached while
        // their processor is still running.
        if let Ok(mut engine) = self.engine.try_lock() {
            engine.pause();
            engine.reset_playback_state();
        }
        self.ui_playing = false;
        self.close_all_editors();
        self.pending_plugin_apply = None;
        self.roll_windows.clear();
        self.roll_drag = None;
        // 旧工程可能很大，放到独立线程析构，避免阻塞加载提示所在的帧。
        // Freeing a large previous project must not stall the frame that shows
        // the loading prompt.
        let previous = std::mem::replace(&mut self.project, project);
        // 力度控制器已从自动化子轨移除：旧工程中残留的力度子轨不再参与布局与绘制。
        // The velocity automation lane no longer exists: drop leftover lanes from older projects so
        // the arrangement layout and hit-testing stay consistent.
        for track in &mut self.project.tracks {
            track
                .visible_controllers
                .retain(|kind| *kind != ControllerKind::Velocity);
        }
        std::thread::spawn(move || drop(previous));
        self.project_epoch = self.project_epoch.wrapping_add(1);
        self.reset_autosave_state();
        self.clear_history();
        self.pending_play = false;
        // 外部格式工程只能转换为 `.sfsd` 保存，因此不记录原路径；历史版本同样不记录，
        // 避免 Ctrl+S 覆盖 Backups 中的版本文件。
        // Imported formats can only be saved as `.sfsd`; history versions also drop the path so
        // Ctrl+S never overwrites a file inside Backups.
        let is_history = super::backup::is_history_path(&path);
        self.project_path = if is_imported || is_history {
            None
        } else {
            Some(path.clone())
        };
        // 打开过的工程进入项目列表，主页下次显示即可一键加载；历史版本不写入，避免列表被版本文件占满。
        // Opened projects join the project list so the home page can reload them; history versions
        // are skipped so the list never fills with backup files.
        if !super::backup::is_history_path(&path) {
            self.settings.push_recent(&path);
            self.commit_settings();
        }
        self.view = WorkspaceView::Studio;
        self.selected_track = 0;
        self.selected_clip = self.project.tracks.first().and_then(|track| {
            if track.clips.is_empty() {
                None
            } else {
                Some(0)
            }
        });
        self.selected_note = None;
        self.selected_notes.clear();
        self.selected_clips.clear();
        self.view_start = 0.0;
        self.piano_view_beat = 0.0;
        self.piano_track = None;
        self.piano_views.clear();
        // 先恢复持久化的波形包络；音频样本本身由后台线程解码。
        // Restore the persisted waveform envelopes immediately; the audio
        // samples themselves are decoded in the background.
        self.ui_waveforms.clear();
        for peak in peaks {
            // 缺失的源必须继续显示“未加载”，不能复用上次打开的旧包络。
            // A missing source must keep showing as "not loaded" instead of a
            // stale envelope from the last time the project was open.
            if !peak.bins.is_empty() && peak.path.exists() {
                self.ui_waveforms.insert(peak.path, peak.bins);
            }
        }
        self.bottom_view = BottomView::PianoRoll;
        self.bottom_open = false;
        self.focus_piano_on_selection();
        // 导入的工程尚未保存为本软件格式，需要提示用户另存。
        // The imported project has not been saved in the native format yet.
        self.dirty = is_imported;
        let has_plugins = self
            .project
            .master_effect
            .as_ref()
            .is_some_and(|effect| effect.plugin().is_some())
            || self.project.tracks.iter().any(|track| {
                matches!(track.instrument, InstrumentRef::Plugin(_))
                    || track.effects.iter().any(|effect| effect.plugin().is_some())
            });
        // 插件运行时必须在播放前就绪，因此在加载提示下完成同步。
        // Plugin runtimes must be ready before playback, so sync while the
        // loading prompt is visible instead of waiting for the first play.
        self.engine_stale = false;
        self.needs_sync = true;
        // 先让 UI 绘制一帧加载提示，再把插件同步放到后台线程。
        // Give the UI one frame to paint the loading prompt, then sync plugins
        // on a worker thread so the window stays responsive.
        self.defer_sync = true;
        self.sync_in_background = true;
        self.loading_until = None;
        self.loading_message = Some(if has_plugins {
            "正在加载插件…".into()
        } else {
            "正在准备工程…".into()
        });
        let mut status = if is_imported {
            format!(
                "已导入 {source_label} 工程 {}，保存时将转换为 .sfsd",
                path.display()
            )
        } else {
            format!("正在加载 {}", path.display())
        };
        if !warnings.is_empty() {
            status.push_str(" · ");
            status.push_str(&summarize_warnings(&warnings));
        }
        self.status = status;
        span.end(&[
            ("status", "ok"),
            ("tracks", &tracks_text),
            ("clips", &clips_text),
            ("warnings", &warnings.len().to_string()),
        ]);
    }

    /// 所属模块：app / 文件操作
    /// 功能：保存当前工程（沿用已有路径；无路径时弹出保存对话框）。
    /// 输入：`self`。
    /// 输出/副作用：见 `save_project_in_background`。
    /// 复杂度：时间 O(1)，空间 O(1)（写盘在后台线程）。
    ///
    /// Module: app / file operations
    /// Purpose: Save the current project (reuse the existing path; ask for one when missing).
    /// Inputs: `self`.
    /// Outputs / side effects: see `save_project_in_background`.
    /// Complexity: O(1) time and O(1) space (the write runs on a worker thread).
    pub fn save_project(&mut self) {
        self.save_project_in_background(false, false);
    }

    /// 所属模块：app / 文件操作
    /// 功能：另存为：强制弹出保存对话框后写入 `.sfsd`。
    /// 输入：
    /// - `force_dialog`：总是显示保存对话框。
    /// 输出/副作用：见 `save_project_in_background`。
    /// 复杂度：时间 O(1)，空间 O(1)（写盘在后台线程）。
    ///
    /// Module: app / file operations
    /// Purpose: Save as: always show the save dialog, then write `.sfsd`.
    /// Inputs:
    /// - `force_dialog`: always show the save dialog.
    /// Outputs / side effects: see `save_project_in_background`.
    /// Complexity: O(1) time and O(1) space (the write runs on a worker thread).
    pub fn save_project_as(&mut self, force_dialog: bool) {
        self.save_project_in_background(force_dialog, false);
    }

    /// 所属模块：app / 文件操作
    /// 功能：抓取插件状态后把工程快照交给后台线程写入 `.sfsd`；自动保存复用同一流程。
    /// 输入：
    /// - `force_dialog`：为 `true` 或工程尚无路径时弹出保存对话框。
    /// - `autosave`：为 `true` 时更新状态文本、跳过“最近项目”写盘，并在完成后参与关闭流程。
    ///
    /// 输出/副作用：设置 `save_rx` 与状态栏；扩展名会被规范为 `.sfsd`。
    ///
    /// 复杂度：时间 O(工程大小)（快照克隆，插件状态为 `Arc` 不复制），空间 O(工程大小)。
    ///
    /// Module: app / file operations
    /// Purpose: Capture the plugin state and hand a project snapshot to a worker thread that writes
    /// `.sfsd`; the autosave reuses the same flow.
    /// Inputs:
    /// - `force_dialog`: show the save dialog when true, or when the project has no path.
    /// - `autosave`: when true, adjust the status text, skip the recent-project write and take part
    ///   in the close flow once finished.
    ///
    /// Outputs / side effects: sets `save_rx` and the status bar; the extension is normalized to
    /// `.sfsd`.
    ///
    /// Complexity: O(project size) time for the snapshot clone (plugin states are `Arc` and not
    /// copied) and O(project size) space.
    pub(super) fn save_project_in_background(&mut self, force_dialog: bool, autosave: bool) {
        if self.engine_sync_rx.is_some() || self.instrument_load_pending {
            self.status = "正在加载插件，请稍候…".into();
            return;
        }
        if self.save_rx.is_some() {
            self.status = "正在保存工程，请稍候…".into();
            return;
        }
        let path = if force_dialog || self.project_path.is_none() {
            let default_name = format!("{}.sfsd", self.project.name);
            let filters: &[(&str, &[&str])] = &[("SurfingStudio 工程", &["sfsd"])];
            let request = crate::platform::FileRequest {
                title: "保存工程",
                filters,
                start_dir: self.settings.default_project_dir.as_deref(),
                default_name: Some(&default_name),
                parent: self.dialog_parent(),
            };
            match crate::platform::save_file(&request) {
                Some(path) => Some(path),
                // 没有系统文件对话框的平台（安卓）直接写进应用工程目录，避免无法保存。
                // Platforms without a native file dialog (Android) write into the app project
                // directory instead, so saving always works.
                None if !crate::platform::caps().native_file_dialog => {
                    default_project_dir().map(|dir| dir.join(&default_name))
                }
                None => None,
            }
        } else {
            self.project_path.clone()
        };
        let Some(path) = path else {
            return;
        };
        let path = if path
            .extension()
            .map(|extension| extension.eq_ignore_ascii_case("sfsd"))
            .unwrap_or(false)
        {
            path
        } else {
            // 只允许保存为软件自有格式，避免把 SFS2 数据写成 `.sndp` 等扩展名。
            // Only the native format is allowed, so SFS2 data never gets another extension.
            path.with_extension("sfsd")
        };
        {
            let engine = self.engine.clone();
            if !self.engine_stale {
                if let Ok(engine) = engine.lock() {
                    engine.capture_plugin_states(&mut self.project);
                };
            }
        }
        // 写入线程持有快照：插件状态是 `Arc<[u8]>`，克隆不会复制可能数 MB 的状态块，
        // UI 线程也无需等待压缩或磁盘 IO。
        // The writer thread owns a snapshot: plugin states are `Arc<[u8]>`, so
        // this clone does not copy the (potentially multi-megabyte) state
        // blobs, and the UI never waits for compression or disk I/O.
        let mut snapshot = self.project.clone();
        snapshot.version = crate::core::model::CURRENT_PROJECT_VERSION;
        let peaks = self.collect_peak_entries();
        let edit_seq = self.edit_seq;
        let project_epoch = self.project_epoch;
        let thread_path = path.clone();
        // 历史版本在写盘线程创建：复制可能数 MB 的工程不能让 UI 等待。
        // Create history versions on the writer thread: copying a multi-megabyte project must not
        // block the UI.
        let backup_retention = self
            .settings
            .auto_backup
            .then_some(self.settings.backup_retention);
        let (tx, rx) = channel();
        let save_path_text = path.to_string_lossy().to_string();
        let autosave_text = if autosave { "true" } else { "false" };
        std::thread::spawn(move || {
            // 目标目录可能还不存在（安卓的默认工程目录）；先创建再写入。
            // The target directory may not exist yet (Android's default project directory).
            if let Some(parent) = thread_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Some(retention) = backup_retention {
                let backup_span = diag::Span::start("app.project", "project.backup");
                let outcome = super::backup::create_backup(&thread_path, retention);
                match outcome {
                    Ok(_) => backup_span.end(&[("status", "ok")]),
                    Err(error) => backup_span.end(&[("status", "error"), ("error", &error)]),
                }
            }
            let span = diag::Span::start("app.project", "project.save.worker");
            let result = crate::core::project_runtime::ProjectRuntime::save_project(
                &thread_path,
                &snapshot,
                &peaks,
            );
            match &result {
                Ok(()) => span.end(&[
                    ("status", "ok"),
                    ("path", &save_path_text),
                    ("autosave", autosave_text),
                ]),
                Err(error) => span.end(&[
                    ("status", "error"),
                    ("path", &save_path_text),
                    ("autosave", autosave_text),
                    ("error", error),
                ]),
            }
            let _ = tx.send(SaveOutcome {
                path: thread_path,
                edit_seq,
                project_epoch,
                autosave,
                result,
            });
        });
        self.save_rx = Some(rx);
        self.status = if autosave {
            format!("正在自动保存 {}", path.display())
        } else {
            format!("正在保存 {}", path.display())
        };
    }

    /// 所属模块：app / 文件操作
    /// 功能：把编曲区当前保留的波形包络收集为可保存的峰值列表。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回峰值列表；不修改状态。
    ///
    /// 复杂度：时间 O(波形条目数 × bin 数)，空间 O(峰值总大小)。
    ///
    /// Module: app / file operations
    /// Purpose: Collect waveform envelopes kept by the arrangement into saveable peaks.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the peak list; no state change.
    ///
    /// Complexity: O(entry count × bin count) time and O(total peak size) space.
    pub(super) fn collect_peak_entries(&self) -> Vec<crate::core::project_runtime::WaveformPeak> {
        self.ui_waveforms
            .iter()
            .map(|(path, bins)| crate::core::project_runtime::WaveformPeak {
                path: path.clone(),
                bins: bins.clone(),
            })
            .collect()
    }

    /// 所属模块：app / 文件操作
    /// 功能：非阻塞接收后台保存结果：校验工程代次，更新路径、未保存标记与最近项目；自动保存不会写入
    /// 最近项目，关闭确认中的保存在成功且无新修改后继续关闭窗口。
    /// 输入：`self`。
    /// 输出/副作用：可能更新 `project_path`、`dirty`、状态栏与 `settings.recent_projects` 并写盘；
    /// 成功后删除崩溃恢复文件；可能设置 `pending_close`。
    /// 复杂度：时间 O(最近项目数)，空间 O(1)。
    ///
    /// Module: app / file operations
    /// Purpose: Non-blocking receive of the background save result: validate the project epoch and
    /// update the path, dirty flag and recent projects. Autosave skips the recent list; a save that
    /// runs while the close confirmation is open closes the window when it succeeds without newer
    /// edits.
    /// Inputs: `self`.
    /// Outputs / side effects: may update `project_path`, `dirty`, the status bar and
    /// `settings.recent_projects` (writing the settings file); on success removes the crash-recovery
    /// file; may set `pending_close`.
    /// Complexity: O(recent projects) time and O(1) space.
    pub(super) fn poll_save_project(&mut self) {
        let Some(rx) = &self.save_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(outcome) => {
                self.save_rx = None;
                match outcome.result {
                    Ok(()) => {
                        if self.project_epoch != outcome.project_epoch {
                            // The user switched projects while this save was
                            // running; do not apply it to the new one.
                            return;
                        }
                        self.project_path = Some(outcome.path.clone());
                        // 保存成功同样记入最近项目，主页可直接再次打开；自动保存不重复写入设置。
                        // A successful save also joins the recent list for the home page; autosave
                        // does not rewrite the settings file again.
                        if !outcome.autosave {
                            self.settings.push_recent(&outcome.path);
                            self.commit_settings();
                        }
                        // Edits made while the writer was running are still
                        // unsaved; keep the dirty marker in that case.
                        if self.edit_seq == outcome.edit_seq {
                            self.dirty = false;
                        }
                        let label = if outcome.autosave {
                            "已自动保存"
                        } else {
                            "已保存"
                        };
                        if self.dirty {
                            self.status = format!(
                                "{label} {}（保存期间又有新的修改）",
                                outcome.path.display()
                            );
                        } else {
                            self.status = format!("{label} {}", outcome.path.display());
                            // 工程已落盘，恢复文件不再需要；若关闭确认仍打开，继续关闭窗口。
                            // The project is on disk, so the recovery file is obsolete; if the
                            // close confirmation is still open, continue closing the window.
                            self.remove_recovery_file();
                            if self.close_prompt {
                                self.close_prompt = false;
                                self.pending_close = true;
                            }
                        }
                    }
                    Err(error) => self.status = format!("保存失败: {error}"),
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.save_rx = None;
                self.status = "保存失败: 写入线程中断".into();
            }
        }
    }

    /// 所属模块：app / 文件操作
    /// 功能：离线导出 WAV：抓取插件状态、按设置决定是否启用 GPU 软限幅并同步渲染。
    /// 输入：`self`。
    /// 输出/副作用：弹出保存对话框、暂停传输、写盘；状态栏显示导出结果或 GPU 回退原因。
    /// 复杂度：时间 O(工程长度 × 轨道数)，空间 O(渲染样本数)。
    ///
    /// Module: app / file operations
    /// Purpose: Export WAV offline: capture plugin states, honour the GPU limiter setting and render
    /// synchronously.
    /// Inputs: `self`.
    /// Outputs / side effects: opens the save dialog, pauses the transport and writes the file; the
    /// status bar reports the result or the GPU fallback reason.
    /// Complexity: O(project length × tracks) time and O(rendered samples) space.
    pub fn export_wav_dialog(&mut self) {
        if self.engine_sync_rx.is_some() || self.instrument_load_pending {
            self.status = "正在加载插件，请稍候…".into();
            return;
        }
        let default_name = format!("{}.wav", self.project.name);
        let filters: &[(&str, &[&str])] = &[("WAV 音频", &["wav"])];
        let request = crate::platform::FileRequest {
            title: "导出 WAV",
            filters,
            start_dir: self.settings.default_project_dir.as_deref(),
            default_name: Some(&default_name),
            parent: self.dialog_parent(),
        };
        // 没有系统对话框时导出到应用工程目录，保证导出始终可用。
        // Without a native dialog the file lands in the app project directory.
        let Some(path) = crate::platform::save_file(&request).or_else(|| {
            (!crate::platform::caps().native_file_dialog)
                .then(|| default_project_dir().map(|dir| dir.join(&default_name)))
                .flatten()
        }) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        self.pause_transport();
        if !self.engine_stale {
            let engine = self.engine.clone();
            if let Ok(engine) = engine.lock() {
                engine.capture_plugin_states(&mut self.project);
            };
        }
        self.status = "正在离线渲染…".into();
        let gpu = self.settings.gpu_audio_config();
        let path_text = path.to_string_lossy().to_string();
        // 离线导出是同步渲染：卡死或特别慢时，日志里的这条区间就是现场。
        // Offline export renders synchronously; this span identifies the scene when it hangs or is
        // unusually slow.
        let span =
            diag::Span::start_logged("app.project", "project.export_wav", &[("path", &path_text)]);
        diag::set_activity("project.export");
        match crate::audio::io::AudioRuntime::export_project_with_gpu(
            &self.project,
            &path,
            48_000.0,
            gpu.as_ref(),
        ) {
            Ok(None) => {
                self.status = format!("已导出 {}", path.display());
                span.end(&[("status", "ok")]);
            }
            Ok(Some(warning)) => {
                self.status = format!("已导出 {}（{warning}）", path.display());
                span.end(&[("status", "ok"), ("warning", &warning)]);
            }
            Err(error) => {
                self.status = format!("导出失败: {error}");
                span.end(&[("status", "error"), ("error", &error)]);
            }
        }
        diag::set_activity("ui.frame");
    }

    /// 所属模块：app / 文件操作 / 公共目录
    /// 功能：把当前工程另存一份到用户选择的公共目录（安卓用 SAF，桌面用“另存为”）。
    /// 输入：`self`。
    /// 输出/副作用：抓取插件状态后在后台线程写临时文件并复制；状态栏显示目标或错误。
    /// 复杂度：时间 O(工程大小)（后台），空间 O(工程快照)。
    ///
    /// Module: app / file operations / public folder
    /// Purpose: Save a copy of the current project into a user-chosen public folder (SAF on
    /// Android, save-as on desktop).
    /// Inputs: `self`.
    /// Outputs / side effects: captures plugin state, writes a temporary file on a worker thread and
    /// copies it; the status bar reports the target or the error.
    /// Complexity: O(project size) time (background) and O(project snapshot) space.
    pub fn export_project_copy_to_public(&mut self) {
        if self.engine_sync_rx.is_some() || self.instrument_load_pending {
            self.status = "正在加载插件，请稍候…".into();
            return;
        }
        if self.public_copy_rx.is_some() {
            self.status = "正在导出副本，请稍候…".into();
            return;
        }
        if !self.engine_stale {
            let engine = self.engine.clone();
            if let Ok(engine) = engine.lock() {
                engine.capture_plugin_states(&mut self.project);
            };
        }
        let mut snapshot = self.project.clone();
        snapshot.version = crate::core::model::CURRENT_PROJECT_VERSION;
        let peaks = self.collect_peak_entries();
        let name = format!("{}.sfsd", self.project.name);
        let Some(temp_dir) = export_scratch_dir() else {
            self.status = "导出失败: 没有可写的临时目录".into();
            return;
        };
        let temp_path = temp_dir.join(&name);
        let (tx, rx) = channel();
        self.status = "正在准备工程副本…".into();
        std::thread::spawn(move || {
            let result = crate::core::project_runtime::ProjectRuntime::save_project(
                &temp_path, &snapshot, &peaks,
            )
            .map_err(|error| format!("写入临时工程失败: {error}"))
            .and_then(|()| {
                let filters: &[(&str, &[&str])] = &[("SurfingStudio 工程", &["sfsd"])];
                let request = crate::platform::FileRequest {
                    title: "保存工程副本",
                    filters,
                    start_dir: None,
                    default_name: Some(&name),
                    parent: None,
                };
                crate::platform::export_copy(&temp_path, &request)
            });
            let _ = tx.send(result);
        });
        self.public_copy_rx = Some(rx);
    }

    /// 所属模块：app / 文件操作 / 公共目录
    /// 功能：离线渲染 WAV 并复制到用户选择的公共目录（安卓用 SAF）。
    /// 输入：`self`。
    /// 输出/副作用：暂停播放、抓取插件状态、同步渲染到临时目录，再由后台线程复制。
    /// 复杂度：时间 O(工程长度 × 轨道数) + 用户操作，空间 O(渲染样本数)。
    ///
    /// Module: app / file operations / public folder
    /// Purpose: Render WAV offline and copy it into a user-chosen public folder (SAF on Android).
    /// Inputs: `self`.
    /// Outputs / side effects: pauses playback, captures plugin state, renders synchronously into
    /// the scratch directory, then a worker thread copies it.
    /// Complexity: O(project length × tracks) + user time; O(rendered samples) space.
    pub fn export_wav_copy_to_public(&mut self) {
        if self.engine_sync_rx.is_some() || self.instrument_load_pending {
            self.status = "正在加载插件，请稍候…".into();
            return;
        }
        if self.public_copy_rx.is_some() {
            self.status = "正在导出副本，请稍候…".into();
            return;
        }
        let name = format!("{}.wav", self.project.name);
        let Some(temp_dir) = export_scratch_dir() else {
            self.status = "导出失败: 没有可写的临时目录".into();
            return;
        };
        let temp_path = temp_dir.join(&name);
        self.pause_transport();
        if !self.engine_stale {
            let engine = self.engine.clone();
            if let Ok(engine) = engine.lock() {
                engine.capture_plugin_states(&mut self.project);
            };
        }
        self.status = "正在离线渲染…".into();
        let gpu = self.settings.gpu_audio_config();
        let span = diag::Span::start_logged("app.project", "project.export_wav_copy", &[]);
        diag::set_activity("project.export");
        let rendered = crate::audio::io::AudioRuntime::export_project_with_gpu(
            &self.project,
            &temp_path,
            48_000.0,
            gpu.as_ref(),
        );
        diag::set_activity("ui.frame");
        match rendered {
            Ok(_warning) => {
                span.end(&[("status", "ok")]);
                let (tx, rx) = channel();
                self.status = "请选择保存位置…".into();
                std::thread::spawn(move || {
                    let filters: &[(&str, &[&str])] = &[("WAV 音频", &["wav"])];
                    let request = crate::platform::FileRequest {
                        title: "导出 WAV",
                        filters,
                        start_dir: None,
                        default_name: Some(&name),
                        parent: None,
                    };
                    let _ = tx.send(crate::platform::export_copy(&temp_path, &request));
                });
                self.public_copy_rx = Some(rx);
            }
            Err(error) => {
                span.end(&[("status", "error"), ("error", &error)]);
                self.status = format!("导出失败: {error}");
            }
        }
    }

    /// 所属模块：app / 文件操作 / 公共目录
    /// 功能：非阻塞接收“导出副本到公共目录”的结果并写入状态栏。
    /// 输入：`self`。
    /// 输出/副作用：清空 `public_copy_rx`；更新状态栏文本。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / file operations / public folder
    /// Purpose: Non-blocking receive of the "copy to public folder" result and show it in the status
    /// bar.
    /// Inputs: `self`.
    /// Outputs / side effects: clears `public_copy_rx`; updates the status text.
    /// Complexity: O(1) time and space.
    pub(super) fn poll_public_copy(&mut self) {
        let Some(rx) = &self.public_copy_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(Some(target))) => {
                self.public_copy_rx = None;
                self.status = format!("已导出副本 {target}");
            }
            Ok(Ok(None)) => {
                self.public_copy_rx = None;
                self.status = "已取消导出副本".into();
            }
            Ok(Err(error)) => {
                self.public_copy_rx = None;
                self.status = format!("导出副本失败: {error}");
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.public_copy_rx = None;
                self.status = "导出副本失败: 后台线程中断".into();
            }
        }
    }

    pub fn pause_transport(&mut self) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.pause();
        }
        self.ui_playing = false;
    }
}

/// 所属模块：app / 文件操作
/// 功能：把导入警告压缩成适合状态栏显示的短文本。
/// 输入：`warnings` 警告列表。
///
/// 输出/副作用：返回最多两条提示加总数；不修改状态。
///
/// 复杂度：时间 O(警告数)，空间 O(提示长度)。
///
/// Module: app / file operations
/// Purpose: Compress import warnings into a short status-bar message.
/// Inputs: `warnings` — warning list.
///
/// Outputs / side effects: returns up to two messages plus a total count.
///
/// Complexity: O(warning count) time and O(message length) space.
fn summarize_warnings(warnings: &[String]) -> String {
    if warnings.len() <= 2 {
        warnings.join(" · ")
    } else {
        format!(
            "{} · 等 {} 条提示",
            warnings[..2].join(" · "),
            warnings.len()
        )
    }
}
