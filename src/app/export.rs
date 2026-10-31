//! 所属模块：app / 导出窗口
//! 文件用途：
//! - 维护独立导出窗口的状态：格式、采样率、位深、声道布局、范围、尾长、GPU 与目标路径。
//! - 复用实时引擎逐帧离线渲染（插件实例始终留在 UI 线程），渲染完成后由后台线程编码写盘；
//!   结果由 `poll_export` 非阻塞接收。
//! - 提供默认路径与文件对话框；真正的界面绘制在 `ui/export.rs`。
//!
//! 函数清单：
//! - `ExportFormat` / `ExportChannels` / `ExportState`：导出窗口的数据契约。
//! - `ExportState::ensure_extension` / `is_rendering` / `Default::default`：扩展名、渲染状态与默认值。
//! - `SurfingApp::open_export_window` / `export_default_path` / `export_pick_path`：
//!   打开窗口、默认路径与后台文件对话框。
//! - `SurfingApp::start_export` / `step_export_render` / `spawn_export_writer`：
//!   开始逐帧渲染、推进块并交给写盘线程。
//! - `SurfingApp::fail_export` / `cancel_export` / `poll_export` / `poll_export_dialog`：
//!   失败/取消收尾、轮询写盘结果与路径对话框。
//!
//! Module: app / export window
//! Purpose:
//! - Hold the standalone export window state: format, sample rate, bit depth, channel layout,
//!   range, tail, GPU and destination path.
//! - Render offline in steps on the live engine (plugin instances stay on the UI thread), then
//!   encode the samples on a worker thread; `poll_export` receives the result without blocking.
//! - Provide the default path and file dialog; the UI itself lives in `ui/export.rs`.
//!
//! Functions:
//! - `ExportFormat` / `ExportChannels` / `ExportState`: export-window contracts.
//! - `ExportState::ensure_extension` / `is_rendering` / `Default::default`: extension, render state
//!   and defaults.
//! - `SurfingApp::open_export_window` / `export_default_path` / `export_pick_path`:
//!   open the window, default path and background file dialog.
//! - `SurfingApp::start_export` / `step_export_render` / `spawn_export_writer`:
//!   start step rendering, advance blocks and hand samples to the writer.
//! - `SurfingApp::fail_export` / `cancel_export` / `poll_export` / `poll_export_dialog`:
//!   finish failures/cancellation, poll the writer result and the path dialog.

use super::*;
use crate::audio::engine::{OfflineRenderOptions, RenderRange};
use crate::audio::io::{AudioBitDepth, AudioEncodeOptions, AudioRuntime};
use crate::core::model::ChannelLayout;
use crate::diag;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// 每帧推进的离线块数初始值：8 × 512 帧约 85 ms 音频；运行中按耗时在 1~64 之间自适应。
/// Initial offline blocks per frame: 8 × 512 frames ≈ 85 ms of audio; the runtime adapts it
/// between 1 and 64 based on the time each step takes.
const EXPORT_BLOCKS_PER_FRAME: usize = 8;

/// 一次导出任务的参数；渲染会话本身保存在引擎中，`blocks_per_step` 会随耗时调整。
/// Parameters of one export job; the render session itself lives in the engine and
/// `blocks_per_step` adapts to the measured step time.
struct ExportJob {
    path: PathBuf,
    encode: AudioEncodeOptions,
    blocks_per_step: usize,
    span: diag::Span,
}

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
    /// 后台导出结果通道（仅编码/写盘阶段）。
    /// Background export result channel (encoding/writing stage only).
    pub rx: Option<Receiver<Result<String, String>>>,
    /// 当前渲染任务；`None` 表示还没有开始或已进入写盘阶段。
    /// Current render job; `None` before starting or once writing has begun.
    job: Option<ExportJob>,
    /// 渲染进度 0..1。
    /// Render progress in 0..1.
    pub progress: f32,
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
            job: None,
            progress: 0.0,
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

    /// 所属模块：app / 导出窗口
    /// 功能：是否正在逐帧渲染。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：O(1)。
    ///
    /// Module: app / export window
    /// Purpose: Whether step rendering is in progress.
    /// Inputs: `self`.
    /// Outputs / side effects: a boolean; no state change.
    /// Complexity: O(1).
    pub fn is_rendering(&self) -> bool {
        self.job.is_some()
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
    /// 功能：在 UI 线程启动逐帧离线渲染；插件实例与编辑器始终留在 UI 线程，避免后台线程
    /// 重新实例化 Kontakt / VOCALOID 等插件导致崩溃。
    /// 输入：`self`。
    /// 输出/副作用：暂停播放、抓取插件状态、关闭编辑器、在引擎上开始导出会话并记录任务；
    /// 后续由 `poll_export` 每帧推进少量块。
    /// 复杂度：时间 O(工程同步 + 插件重载)（开始阶段），之后每帧由 `poll_export` 分摊。
    ///
    /// Module: app / export window
    /// Purpose: Start step-driven offline rendering on the UI thread; plugin instances and editors stay
    /// on the UI thread so plugins such as Kontakt / VOCALOID are never instantiated on a worker and
    /// cannot crash the export.
    /// Inputs: `self`.
    /// Outputs / side effects: pauses playback, captures plugin state, closes editors, starts the
    /// engine export session and records the job; `poll_export` advances it per frame.
    /// Complexity: O(project sync + plugin reload) at start; the rest is spread over frames.
    pub(crate) fn start_export(&mut self) {
        if self.export.busy {
            return;
        }
        if self.engine_sync_rx.is_some() || self.instrument_load_pending {
            self.export.status = "正在加载插件，请稍候…".into();
            return;
        }
        self.export.ensure_extension();
        if self.export.path.trim().is_empty() {
            self.export.status = "请先选择导出路径".into();
            return;
        }
        let path = PathBuf::from(self.export.path.trim());
        let layout = self.export.channels.resolve(self.project.output_layout);
        let sample_rate = self.export.sample_rate.max(8_000) as f64;
        self.pause_transport();
        if !self.engine_stale {
            let engine = self.engine.clone();
            if let Ok(engine) = engine.lock() {
                engine.capture_plugin_states(&mut self.project);
            };
        }
        // 采样率或布局变化会重建插件实例；先关闭编辑器，避免 GUI 继续引用旧视图。
        // A sample-rate or layout change recreates plugin instances; close editors first so their GUIs
        // never reference an old view.
        self.close_all_editors();
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
        let span = diag::Span::start("app.project", "project.export_window");
        let engine = self.engine.clone();
        let total = match engine.lock() {
            Ok(mut guard) => guard.begin_offline_render(&self.project, &render),
            Err(_) => Err("音频引擎正忙，请稍后再试".to_string()),
        };
        let total = match total {
            Ok(total) => total,
            Err(error) => {
                span.end(&[("status", "error"), ("error", &error)]);
                self.export.status = format!("导出失败：{error}");
                self.status = format!("导出失败：{error}");
                return;
            }
        };
        self.export.job = Some(ExportJob {
            path,
            encode,
            blocks_per_step: EXPORT_BLOCKS_PER_FRAME,
            span,
        });
        self.export.busy = true;
        self.export.progress = 0.0;
        self.export.rx = None;
        self.export.status = format!("正在渲染 0%（共 {total} 帧）");
        self.status = self.export.status.clone();
    }

    /// 所属模块：app / 导出窗口
    /// 功能：每帧推进离线渲染；完成后启动编码写盘线程。
    /// 输入：`self`。
    /// 输出/副作用：更新 `export.progress` / `status`；完成时把样本交给写盘线程。
    /// 复杂度：时间 O(每帧块数 × 512 × 轨道数)，空间 O(已渲染样本)。
    ///
    /// Module: app / export window
    /// Purpose: Advance the offline render per frame; once complete, start the encoding thread.
    /// Inputs: `self`.
    /// Outputs / side effects: updates `export.progress`/`status`; hands samples to the writer thread.
    /// Complexity: O(blocks per frame × 512 × tracks) time and O(rendered samples) space.
    fn step_export_render(&mut self) {
        let Some(mut job) = self.export.job.take() else {
            return;
        };
        let engine = self.engine.clone();
        let mut guard = match engine.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                // 音频回调正持锁：下一帧继续，不阻塞 UI。
                // The audio callback holds the lock: retry next frame instead of blocking the UI.
                self.export.job = Some(job);
                return;
            }
        };
        let started = Instant::now();
        let step = guard.render_offline_step(job.blocks_per_step);
        // 按本帧实际耗时调整块数：重插件下自动减小块数保持界面流畅，轻工程下加速导出。
        // Adapt the block count to the time this step took: heavy plugins shrink it to keep the UI
        // responsive, while light projects speed up.
        let elapsed = started.elapsed();
        if elapsed > Duration::from_millis(40) {
            job.blocks_per_step = (job.blocks_per_step / 2).max(1);
        } else if elapsed < Duration::from_millis(10) {
            job.blocks_per_step = (job.blocks_per_step * 2).min(64);
        }
        match step {
            Ok(progress) => {
                self.export.progress = progress;
                if progress < 0.999 {
                    let (total, rendered) = guard.offline_frame_counts().unwrap_or((0, 0));
                    self.export.status = format!(
                        "正在渲染 {:.0}%（{} / {} 帧）",
                        progress * 100.0,
                        rendered,
                        total
                    );
                    self.export.job = Some(job);
                    return;
                }
                let result = guard.finish_offline_render();
                drop(guard);
                match result {
                    Ok(render) => {
                        job.span.end(&[
                            ("status", "rendered"),
                            ("channels", &render.channels.to_string()),
                        ]);
                        self.spawn_export_writer(job.path, job.encode, render);
                    }
                    Err(error) => {
                        job.span.end(&[("status", "error"), ("error", &error)]);
                        self.fail_export(&error);
                    }
                }
            }
            Err(error) => {
                job.span.end(&[("status", "error"), ("error", &error)]);
                drop(guard);
                let engine = self.engine.clone();
                if let Ok(mut guard) = engine.lock() {
                    guard.cancel_offline_render();
                }
                self.fail_export(&error);
            }
        }
    }

    /// 所属模块：app / 导出窗口
    /// 功能：把渲染好的样本交给后台线程编码写盘（此时已不持有任何插件实例）。
    /// 输入：`path` / `encode` / `render`。
    /// 输出/副作用：设置 `export.rx` 与状态；完成后由 `poll_export` 收尾。
    /// 复杂度：时间 O(1)（写盘在后台），空间 O(样本数)（移动所有权）。
    ///
    /// Module: app / export window
    /// Purpose: Hand rendered samples to a worker thread for encoding/writing (no plugin instances are
    /// involved at this point).
    /// Inputs: `path` / `encode` / `render`.
    /// Outputs / side effects: sets `export.rx` and status; `poll_export` finalises.
    /// Complexity: O(1) time (writing happens on the worker) and O(samples) space (ownership moves).
    fn spawn_export_writer(
        &mut self,
        path: PathBuf,
        encode: AudioEncodeOptions,
        render: crate::audio::engine::OfflineRender,
    ) {
        let (tx, rx) = channel();
        self.export.rx = Some(rx);
        self.export.progress = 1.0;
        self.export.status = "正在写入音频文件…".into();
        self.status = self.export.status.clone();
        // 导出期间工程可能被编辑；写盘开始后让下一次同步把当前工程重新应用到引擎。
        // The project may have been edited during the export; resync it once writing starts.
        self.needs_sync = true;
        let path_text = path.to_string_lossy().to_string();
        std::thread::spawn(move || {
            let span = diag::Span::start("app.project", "project.export_window.write");
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let result = AudioRuntime::standard()
                .export_with_options(&path, &render.samples, &encode)
                .map(|()| match render.warning {
                    Some(warning) => format!("已导出 {}（{warning}）", path.display()),
                    None => format!("已导出 {}", path.display()),
                });
            match &result {
                Ok(_) => span.end(&[("status", "ok"), ("path", &path_text)]),
                Err(error) => {
                    span.end(&[("status", "error"), ("path", &path_text), ("error", error)])
                }
            }
            let _ = tx.send(result);
        });
    }

    /// 所属模块：app / 导出窗口
    /// 功能：渲染失败时统一清理任务并更新状态。
    /// 输入：`error`：失败原因。
    /// 输出/副作用：清空 `job`、`busy`，写入窗口与状态栏。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / export window
    /// Purpose: Clear the task and report a render failure.
    /// Inputs: `error`: failure reason.
    /// Outputs / side effects: clears `job`/`busy`; updates the window and status bar.
    /// Complexity: O(1) time and space.
    fn fail_export(&mut self, error: &str) {
        self.export.job = None;
        self.export.progress = 0.0;
        self.export.busy = false;
        self.export.rx = None;
        self.needs_sync = true;
        self.export.status = format!("导出失败：{error}");
        self.status = self.export.status.clone();
    }

    /// 所属模块：app / 导出窗口
    /// 功能：取消正在渲染的导出并恢复实时引擎；写盘阶段不可取消。
    /// 输入：`self`。
    /// 输出/副作用：恢复工程/走带状态，清空任务并更新状态。
    /// 复杂度：时间 O(工程同步 + 插件重载)，空间 O(1)。
    ///
    /// Module: app / export window
    /// Purpose: Cancel an in-progress render and restore the live engine; writing cannot be cancelled.
    /// Inputs: `self`.
    /// Outputs / side effects: restores project/transport, clears the job and updates the status.
    /// Complexity: O(project sync + plugin reload) time and O(1) space.
    pub(crate) fn cancel_export(&mut self) {
        if let Some(job) = self.export.job.take() {
            let engine = self.engine.clone();
            if let Ok(mut guard) = engine.lock() {
                guard.cancel_offline_render();
            }
            job.span.end(&[("status", "cancelled")]);
            self.export.busy = false;
            self.export.progress = 0.0;
            self.needs_sync = true;
            self.export.status = "已取消导出".into();
            self.status = self.export.status.clone();
        } else if self.export.rx.is_some() {
            self.export.status = "正在写入文件，无法取消".into();
        }
    }

    /// 所属模块：app / 导出窗口
    /// 功能：非阻塞接收写盘结果并更新窗口状态与状态栏。
    /// 输入：`self`。
    /// 输出/副作用：清空 `export.rx`、`export.busy`；更新 `export.status` 与 `status`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / export window
    /// Purpose: Non-blocking receive of the writer result and update the window/status bar.
    /// Inputs: `self`.
    /// Outputs / side effects: clears `export.rx`/`export.busy`; updates `export.status` and `status`.
    /// Complexity: O(1) time and space.
    pub(super) fn poll_export(&mut self) {
        if let Some(rx) = &self.export.rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.export.rx = None;
                    self.export.busy = false;
                    match result {
                        Ok(message) => {
                            self.export.progress = 1.0;
                            self.export.status = message.clone();
                            self.status = message;
                        }
                        Err(error) => {
                            self.export.progress = 0.0;
                            self.export.status = format!("导出失败：{error}");
                            self.status = format!("导出失败：{error}");
                        }
                    }
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.export.rx = None;
                    self.export.busy = false;
                    self.export.status = "导出失败：后台线程中断".into();
                    return;
                }
            }
        }
        self.step_export_render();
    }
}
