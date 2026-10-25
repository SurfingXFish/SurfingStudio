//! 所属模块：app / 导出窗口
//! 文件用途：
//! - 维护独立导出窗口的状态：格式、采样率、位深、声道布局、范围、尾长、GPU 与目标路径。
//! - 在后台线程离线渲染并写出音频文件，结果由 `poll_export` 非阻塞接收。
//! - 提供默认路径与文件对话框；真正的界面绘制在 `ui/export.rs`。
//!
//! 函数清单：
//! - `ExportFormat` / `ExportChannels` / `ExportState`：导出窗口的数据契约。
//! - `ExportState::ensure_extension` / `Default::default`：路径扩展名与默认值。
//! - `SurfingApp::open_export_window` / `export_default_path` / `export_pick_path`：
//!   打开窗口、默认路径与后台文件对话框。
//! - `SurfingApp::start_export` / `poll_export` / `poll_export_dialog`：后台导出与轮询。
//!
//! Module: app / export window
//! Purpose:
//! - Hold the standalone export window state: format, sample rate, bit depth, channel layout,
//!   range, tail, GPU and destination path.
//! - Render offline and write the audio file on a worker thread; `poll_export` receives the result
//!   without blocking.
//! - Provide the default path and file dialog; the UI itself lives in `ui/export.rs`.
//!
//! Functions:
//! - `ExportFormat` / `ExportChannels` / `ExportState`: export-window contracts.
//! - `ExportState::ensure_extension` / `Default::default`: path extension and defaults.
//! - `SurfingApp::open_export_window` / `export_default_path` / `export_pick_path`:
//!   open the window, default path and background file dialog.
//! - `SurfingApp::start_export` / `poll_export` / `poll_export_dialog`: background export and polling.

use super::*;
use crate::audio::engine::{OfflineRenderOptions, RenderRange};
use crate::audio::io::{AudioBitDepth, AudioEncodeOptions, AudioRuntime};
use crate::core::model::ChannelLayout;
use crate::diag;
use std::path::{Path, PathBuf};

/// 导出容器格式。
/// Export container format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportFormat {
    /// WAV：支持 16/24-bit PCM 与 32-bit 浮点。
    /// WAV: 16/24-bit PCM and 32-bit float.
    #[default]
    Wav,
    /// AIFF：支持 16/24-bit PCM（32-bit 浮点回退 24-bit）。
    /// AIFF: 16/24-bit PCM (32-bit float falls back to 24-bit).
    Aiff,
}

impl ExportFormat {
    /// 全部格式；下拉框按此顺序展示。
    /// Every format; the dropdown lists them in this order.
    pub const ALL: [ExportFormat; 2] = [ExportFormat::Wav, ExportFormat::Aiff];

    /// 所属模块：app / 导出窗口
    /// 功能：返回面向用户的格式名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / export window
    /// Purpose: Return the user-facing format label.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Wav => "WAV",
            ExportFormat::Aiff => "AIFF",
        }
    }

    /// 所属模块：app / 导出窗口
    /// 功能：返回文件扩展名（不含点）。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / export window
    /// Purpose: Return the file extension (without the dot).
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Wav => "wav",
            ExportFormat::Aiff => "aiff",
        }
    }
}

/// 导出声道布局选项：跟随工程、强制立体声或指定环绕布局。
/// Export channel layout option: follow the project, force stereo or pick a surround layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportChannels {
    /// 跟随工程的主输出布局。
    /// Follow the project master layout.
    #[default]
    Project,
    /// 强制立体声。
    /// Force stereo.
    Stereo,
    /// 强制 5.1。
    /// Force 5.1.
    Surround51,
    /// 强制 7.1。
    /// Force 7.1.
    Surround71,
}

impl ExportChannels {
    /// 全部选项；下拉框按此顺序展示。
    /// Every option; the dropdown lists them in this order.
    pub const ALL: [ExportChannels; 4] = [
        ExportChannels::Project,
        ExportChannels::Stereo,
        ExportChannels::Surround51,
        ExportChannels::Surround71,
    ];

    /// 所属模块：app / 导出窗口
    /// 功能：返回面向用户的布局名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / export window
    /// Purpose: Return the user-facing layout label.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            ExportChannels::Project => "跟随工程布局",
            ExportChannels::Stereo => ChannelLayout::Stereo.label(),
            ExportChannels::Surround51 => ChannelLayout::Surround51.label(),
            ExportChannels::Surround71 => ChannelLayout::Surround71.label(),
        }
    }

    /// 所属模块：app / 导出窗口
    /// 功能：结合工程布局解析出实际导出布局。
    /// 输入：`project_layout`：工程主输出布局。
    /// 输出/副作用：实际布局；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / export window
    /// Purpose: Resolve the actual export layout together with the project layout.
    /// Inputs: `project_layout`: project master layout.
    /// Outputs / side effects: the resolved layout; no state change.
    /// Complexity: O(1) time and space.
    pub fn resolve(self, project_layout: ChannelLayout) -> ChannelLayout {
        match self {
            ExportChannels::Project => project_layout,
            ExportChannels::Stereo => ChannelLayout::Stereo,
            ExportChannels::Surround51 => ChannelLayout::Surround51,
            ExportChannels::Surround71 => ChannelLayout::Surround71,
        }
    }
}

/// 独立导出窗口的完整状态。
/// Complete state of the standalone export window.
pub struct ExportState {
    /// 窗口是否打开。
    /// Whether the window is open.
    pub open: bool,
    /// 下一帧是否置前（只生效一次）。
    /// Whether to raise the window next frame (one shot).
    pub focus: bool,
    /// 容器格式。
    /// Container format.
    pub format: ExportFormat,
    /// 采样率（Hz）。
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// 位深。
    /// Bit depth.
    pub bit_depth: AudioBitDepth,
    /// 声道布局选项。
    /// Channel-layout option.
    pub channels: ExportChannels,
    /// 渲染范围。
    /// Render range.
    pub range: RenderRange,
    /// 尾音长度（beat）。
    /// Tail length in beats.
    pub tail_beats: f64,
    /// 是否使用 GPU 执行离线主总线软限幅。
    /// Whether the GPU runs the offline master limiter.
    pub use_gpu: bool,
    /// 目标路径文本。
    /// Destination path text.
    pub path: String,
    /// 状态提示（成功、失败或进行中）。
    /// Status message (success, failure or in progress).
    pub status: String,
    /// 后台导出是否进行中。
    /// Whether a background export is running.
    pub busy: bool,
    /// 后台导出结果通道。
    /// Background export result channel.
    pub rx: Option<Receiver<Result<String, String>>>,
}

impl Default for ExportState {
    /// 所属模块：app / 导出窗口
    /// 功能：返回默认导出状态：WAV、48 kHz、16-bit、跟随工程、整个工程、2 beat 尾音、启用 GPU。
    /// 输入：无。
    /// 输出/副作用：默认状态；不修改外部。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / export window
    /// Purpose: Default state: WAV, 48 kHz, 16-bit, follow project, whole project, 2-beat tail, GPU
    /// enabled.
    /// Inputs: none.
    /// Outputs / side effects: the default state; no external change.
    /// Complexity: O(1) time and space.
    fn default() -> Self {
        Self {
            open: false,
            focus: false,
            format: ExportFormat::Wav,
            sample_rate: 48_000,
            bit_depth: AudioBitDepth::Int16,
            channels: ExportChannels::Project,
            range: RenderRange::Project,
            tail_beats: 2.0,
            use_gpu: true,
            path: String::new(),
            status: String::new(),
            busy: false,
            rx: None,
        }
    }
}

impl ExportState {
    /// 所属模块：app / 导出窗口
    /// 功能：保证路径扩展名与当前格式一致；空路径保持不变。
    /// 输入：`self`。
    /// 输出/副作用：可能替换 `path` 的扩展名；不修改其他状态。
    /// 复杂度：时间 O(路径长度)，空间 O(1)。
    ///
    /// Module: app / export window
    /// Purpose: Make the path extension match the format; an empty path is untouched.
    /// Inputs: `self`.
    /// Outputs / side effects: may replace the extension; no other change.
    /// Complexity: O(path length) time and O(1) space.
    pub fn ensure_extension(&mut self) {
        let path = Path::new(&self.path);
        if self.path.trim().is_empty() {
            return;
        }
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case(self.format.extension()))
        {
            return;
        }
        self.path = path
            .with_extension(self.format.extension())
            .to_string_lossy()
            .to_string();
    }
}

impl SurfingApp {
    /// 所属模块：app / 导出窗口
    /// 功能：打开独立导出窗口；已打开时置前；首次打开时填入默认路径与设置默认值。
    /// 输入：`self`。
    /// 输出/副作用：读写 `export`；不启动渲染。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: app / export window
    /// Purpose: Open the standalone export window, raise it when already open, and fill the default
    /// path/defaults on first use.
    /// Inputs: `self`.
    /// Outputs / side effects: reads/writes `export`; no render is started.
    /// Complexity: O(path length) time and space.
    pub fn open_export_window(&mut self) {
        if self.export.busy {
            self.status = "正在导出，请稍候…".into();
            return;
        }
        if self.export.path.trim().is_empty() {
            self.export.format = ExportFormat::Wav;
            self.export.channels = ExportChannels::Project;
            self.export.range = RenderRange::Project;
            self.export.tail_beats = 2.0;
            self.export.bit_depth = AudioBitDepth::Int16;
            self.export.sample_rate = self.settings.audio_sample_rate.max(8_000);
            self.export.use_gpu = self.settings.gpu_audio_enabled;
            self.export.path = self.export_default_path().to_string_lossy().to_string();
        }
        self.export.open = true;
        self.export.focus = true;
    }

    /// 所属模块：app / 导出窗口
    /// 功能：计算默认导出路径：工程所在目录，否则设置中的默认存储地址，否则平台工程目录。
    /// 输入：`self`。
    /// 输出/副作用：返回路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: app / export window
    /// Purpose: Compute the default export path: project directory, then the configured default
    /// directory, then the platform project directory.
    /// Inputs: `self`.
    /// Outputs / side effects: the path; no directory is created.
    /// Complexity: O(path length) time and space.
    pub(super) fn export_default_path(&self) -> PathBuf {
        let directory = self
            .project_path
            .as_ref()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(|| self.settings.default_project_dir.clone())
            .or_else(super::files::default_project_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        directory.join(format!(
            "{}.{}",
            self.project.name,
            self.export.format.extension()
        ))
    }

    /// 所属模块：app / 导出窗口
    /// 功能：在后台线程打开保存对话框选择导出路径（不阻塞 UI）。
    /// 输入：`self`。
    /// 输出/副作用：设置 `export_dialog_rx`；结果由 `poll_export_dialog` 应用。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / export window
    /// Purpose: Open the save dialog on a worker thread to pick the destination (never blocks the
    /// UI).
    /// Inputs: `self`.
    /// Outputs / side effects: sets `export_dialog_rx`; `poll_export_dialog` applies the result.
    /// Complexity: O(1) time and space.
    pub(crate) fn export_pick_path(&mut self) {
        if self.export_dialog_rx.is_some() {
            return;
        }
        let filters: &[(&str, &[&str])] = &[("音频文件", &["wav", "aiff", "aif"])];
        let default_name = self
            .export_default_path()
            .file_name()
            .map(|name| name.to_string_lossy().to_string());
        let start_dir = self.settings.default_project_dir.clone();
        let parent = self.dialog_parent();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            // 请求借用移动进线程的字符串，保证后台对话框不在 UI 线程阻塞。
            // The request borrows strings moved into the thread, so the dialog never blocks the UI.
            let request = crate::platform::FileRequest {
                title: "导出音频",
                filters,
                start_dir: start_dir.as_deref(),
                default_name: default_name.as_deref(),
                parent,
            };
            let _ = tx.send(crate::platform::save_file(&request));
        });
        self.export_dialog_rx = Some(rx);
    }

    /// 所属模块：app / 导出窗口
    /// 功能：非阻塞接收导出路径选择结果并写入路径文本。
    /// 输入：`self`。
    /// 输出/副作用：清空 `export_dialog_rx`；可能更新 `export.path` 与状态栏。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / export window
    /// Purpose: Non-blocking receive of the destination picker result and store it as path text.
    /// Inputs: `self`.
    /// Outputs / side effects: clears `export_dialog_rx`; may update `export.path` and the status bar.
    /// Complexity: O(1) time and space.
    pub(super) fn poll_export_dialog(&mut self) {
        let Some(rx) = &self.export_dialog_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(Some(path)) => {
                self.export_dialog_rx = None;
                self.export.path = path.to_string_lossy().to_string();
                self.export.ensure_extension();
            }
            Ok(None) => self.export_dialog_rx = None,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.export_dialog_rx = None;
                self.export.status = "选择路径失败：对话框中断".into();
            }
        }
    }

    /// 所属模块：app / 导出窗口
    /// 功能：按窗口参数离线渲染并在后台线程写出音频文件。
    /// 输入：`self`。
    /// 输出/副作用：暂停播放、抓取插件状态、克隆工程快照、设置 `export.busy` 与 `export.rx`；
    /// 完成后由 `poll_export` 更新状态。
    /// 复杂度：时间 O(工程大小)（快照克隆），渲染在后台线程。
    ///
    /// Module: app / export window
    /// Purpose: Render offline with the window options and write the audio file on a worker thread.
    /// Inputs: `self`.
    /// Outputs / side effects: pauses playback, captures plugin state, clones the project snapshot,
    /// sets `export.busy` and `export.rx`; `poll_export` updates the status once done.
    /// Complexity: O(project size) time for the snapshot (rendering runs on the worker).
    pub(crate) fn start_export(&mut self) {
        if self.export.busy {
            return;
        }
        if self.engine_sync_rx.is_some() || self.instrument_load_pending {
            self.export.status = "正在加载插件，请稍候…".into();
            return;
        }
        self.export.ensure_extension();
        let path = PathBuf::from(self.export.path.trim());
        if self.export.path.trim().is_empty() {
            self.export.status = "请先选择导出路径".into();
            return;
        }
        let layout = self.export.channels.resolve(self.project.output_layout);
        let sample_rate = self.export.sample_rate.max(8_000) as f64;
        self.pause_transport();
        if !self.engine_stale {
            let engine = self.engine.clone();
            if let Ok(engine) = engine.lock() {
                engine.capture_plugin_states(&mut self.project);
            };
        }
        let mut snapshot = self.project.clone();
        snapshot.version = crate::core::model::CURRENT_PROJECT_VERSION;
        let gpu = self
            .export
            .use_gpu
            .then(|| self.settings.gpu_audio_config())
            .flatten();
        let render = OfflineRenderOptions {
            sample_rate,
            layout,
            range: self.export.range,
            tail_beats: self.export.tail_beats.clamp(0.0, 16.0),
            gpu,
        };
        let encode = AudioEncodeOptions {
            sample_rate,
            channels: layout.channels(),
            bit_depth: self.export.bit_depth,
        };
        let (tx, rx) = channel();
        let path_text = path.to_string_lossy().to_string();
        self.export.busy = true;
        self.export.status = "正在离线渲染并导出…".into();
        let path_for_thread = path.clone();
        std::thread::spawn(move || {
            let span = diag::Span::start("app.project", "project.export_window");
            if let Some(parent) = path_for_thread.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let result = AudioRuntime::export_project_with_options(
                &snapshot,
                &path_for_thread,
                &render,
                &encode,
            )
            .map(|warning| match warning {
                Some(warning) => {
                    format!("已导出 {}（{warning}）", path_for_thread.display())
                }
                None => format!("已导出 {}", path_for_thread.display()),
            });
            match &result {
                Ok(_) => span.end(&[("status", "ok"), ("path", &path_text)]),
                Err(error) => {
                    span.end(&[("status", "error"), ("path", &path_text), ("error", error)])
                }
            }
            let _ = tx.send(result);
        });
        self.export.rx = Some(rx);
    }

    /// 所属模块：app / 导出窗口
    /// 功能：非阻塞接收导出结果并更新窗口状态与状态栏。
    /// 输入：`self`。
    /// 输出/副作用：清空 `export.rx`、`export.busy`；更新 `export.status` 与 `status`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / export window
    /// Purpose: Non-blocking receive of the export result and update the window/status bar.
    /// Inputs: `self`.
    /// Outputs / side effects: clears `export.rx`/`export.busy`; updates `export.status` and `status`.
    /// Complexity: O(1) time and space.
    pub(super) fn poll_export(&mut self) {
        let Some(rx) = &self.export.rx else {
            return;
        };
        match rx.try_recv() {
            Ok(result) => {
                self.export.rx = None;
                self.export.busy = false;
                match result {
                    Ok(message) => {
                        self.export.status = message.clone();
                        self.status = message;
                    }
                    Err(error) => {
                        self.export.status = format!("导出失败：{error}");
                        self.status = format!("导出失败：{error}");
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.export.rx = None;
                self.export.busy = false;
                self.export.status = "导出失败：后台线程中断".into();
            }
        }
    }
}
