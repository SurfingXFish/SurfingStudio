//! 所属模块：app / 轨道渲染
//! 文件用途：
//! - 把选中轨道（含组合轨道子树）离线渲染为 WAV，并在原轨道下方创建音频轨道。
//! - 维护渲染确认弹窗、逐帧渲染任务、写盘线程与“重新渲染”覆盖流程。
//!
//! 函数清单：
//! - `TrackRenderState` / `TrackRenderPhase` / `RenderRequest`：渲染流程状态与请求。
//! - `SurfingApp::request_track_render` / `request_track_rerender`：右键菜单入口。
//! - `SurfingApp::confirm_track_render` / `cancel_track_render`：确认与取消。
//! - `SurfingApp::poll_track_render` / `step_track_render`：每帧推进渲染。
//! - `SurfingApp::begin_next_track_render` / `spawn_track_render_writer`：开始任务与写盘线程。
//! - `SurfingApp::apply_track_render` / `finish_track_render`：应用结果与收尾。
//! - `SurfingApp::build_render_project` / `track_render_path`：渲染工程快照与输出路径。
//!
//! Module: app / track rendering
//! Purpose:
//! - Render selected tracks (including group subtrees) offline to WAV and add an audio track
//!   below the source track.
//! - Hold the confirmation modal, per-frame render job, writer thread and re-render flow.
//!
//! Functions:
//! - `TrackRenderState` / `TrackRenderPhase` / `RenderRequest`: render flow state and requests.
//! - `SurfingApp::request_track_render` / `request_track_rerender`: context-menu entries.
//! - `SurfingApp::confirm_track_render` / `cancel_track_render`: confirm and cancel.
//! - `SurfingApp::poll_track_render` / `step_track_render`: advance rendering per frame.
//! - `SurfingApp::begin_next_track_render` / `spawn_track_render_writer`: start a job and write it.
//! - `SurfingApp::apply_track_render` / `finish_track_render`: apply the result and wrap up.
//! - `SurfingApp::build_render_project` / `track_render_path`: render snapshot and output path.

use super::*;
use crate::audio::engine::{OfflineRenderOptions, RenderRange};
use crate::audio::io::{AudioBitDepth, AudioEncodeOptions, AudioRuntime};
use crate::core::model::{AudioSource, ChannelLayout, TrackKind};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Weak;

/// 每帧推进的离线块数初始值；按本帧耗时自适应。
/// Initial offline blocks per frame; the runtime adapts it to the measured step time.
const TRACK_RENDER_BLOCKS_PER_FRAME: usize = 8;
/// 渲染尾音：内容结束后多渲染 2 beat，保留混响与释放尾巴。
/// Render tail: two beats after the content so reverb and release tails survive.
const TRACK_RENDER_TAIL_BEATS: f64 = 2.0;

/// 一条渲染请求：来源轨道与可选的“重新渲染”目标音频轨道。
/// One render request: the source track and an optional audio track to replace on re-render.
#[derive(Clone, Debug)]
pub struct RenderRequest {
    pub target: TrackId,
    pub replace: Option<TrackId>,
}

/// 渲染流程阶段。
/// Track render phase.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrackRenderPhase {
    /// 未在渲染。
    /// Idle.
    #[default]
    Idle,
    /// 已弹出提醒，等待用户确认。
    /// The reminder modal is open and waiting for confirmation.
    Confirm,
    /// 正在逐帧渲染或写盘。
    /// Rendering or writing the file.
    Rendering,
}

/// 一次渲染任务：目标轨道、可选的替换轨道与输出路径。
/// One render task: target track, optional replacement track and output path.
#[derive(Clone, Debug)]
struct RenderTask {
    target: TrackId,
    replace: Option<TrackId>,
    path: PathBuf,
}

/// 正在渲染的任务与自适应块数。
/// The task currently rendering plus the adaptive block count.
struct RenderJob {
    task: RenderTask,
    blocks_per_step: usize,
}

/// 写盘线程返回的结果：任务与音频时长（秒）。
/// Writer-thread result: the task and the rendered duration in seconds.
struct RenderWrite {
    task: RenderTask,
    result: Result<f64, String>,
}

/// 轨道渲染状态：确认弹窗、任务队列、进度与写盘通道。
/// Track-render state: confirmation modal, task queue, progress and the writer channel.
pub struct TrackRenderState {
    pub phase: TrackRenderPhase,
    pub progress: f32,
    pub status: String,
    pub(crate) requests: Vec<RenderRequest>,
    queue: VecDeque<RenderTask>,
    total: usize,
    completed: usize,
    job: Option<RenderJob>,
    rx: Option<Receiver<RenderWrite>>,
}

impl Default for TrackRenderState {
    /// 所属模块：app / 轨道渲染
    /// 功能：返回空闲状态。
    /// 输入：无。
    /// 输出/副作用：默认状态；不修改外部。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / track rendering
    /// Purpose: Return the idle state.
    /// Inputs: none.
    /// Outputs / side effects: the default state; no external change.
    /// Complexity: O(1) time and space.
    fn default() -> Self {
        Self {
            phase: TrackRenderPhase::Idle,
            progress: 0.0,
            status: String::new(),
            requests: Vec::new(),
            queue: VecDeque::new(),
            total: 0,
            completed: 0,
            job: None,
            rx: None,
        }
    }
}

impl TrackRenderState {
    /// 所属模块：app / 轨道渲染
    /// 功能：是否正在渲染（用于锁定播放与工程同步）。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / track rendering
    /// Purpose: Whether rendering is in progress (locks playback and project sync).
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1) time and space.
    pub fn is_rendering(&self) -> bool {
        self.phase == TrackRenderPhase::Rendering
    }

    /// 所属模块：app / 轨道渲染
    /// 功能：面向弹窗的提示文本。
    /// 输入：`self`。
    /// 输出/副作用：静态或拼接文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / track rendering
    /// Purpose: Modal caption text.
    /// Inputs: `self`.
    /// Outputs / side effects: text; no state change.
    /// Complexity: O(1) time and space.
    pub fn caption(&self) -> String {
        if self.total == 0 {
            "渲染为音频".into()
        } else {
            format!(
                "渲染为音频（{}/{}）",
                (self.completed + 1).min(self.total),
                self.total
            )
        }
    }
}

/// 所属模块：app / 轨道渲染
/// 功能：在后台解码重新渲染后的音频文件并替换引擎中的旧样本。
/// 输入：
/// - `engine`：引擎弱引用；窗口关闭后不再延长寿命。
/// - `path`：已写盘的 WAV 路径。
/// 输出/副作用：解码完成后在引擎锁内替换 `audio_files`；失败写入音频警告。
/// 复杂度：时间 O(文件解码)，空间 O(音频样本)。
///
/// Module: app / track rendering
/// Purpose: Decode a re-rendered audio file on a worker thread and replace the engine's samples.
/// Inputs:
/// - `engine`: weak engine reference; a closed window no longer keeps it alive.
/// - `path`: the written WAV path.
/// Outputs / side effects: replaces `audio_files` under the engine lock; failures become audio
/// warnings.
/// Complexity: O(decode) time and O(samples) space.
fn spawn_audio_reload(engine: Weak<Mutex<Engine>>, path: PathBuf) {
    std::thread::spawn(move || {
        let result = AudioRuntime::import_audio(&path);
        let Some(engine) = engine.upgrade() else {
            return;
        };
        if let Ok(mut guard) = engine.lock() {
            match result {
                Ok(file) => guard.install_audio_file(path, Arc::new(file)),
                Err(error) => guard.fail_audio_file(path, error),
            }
        }
    });
}

impl SurfingApp {
    /// 所属模块：app / 轨道渲染
    /// 功能：打开渲染确认弹窗，目标为当前选中的一条或多条轨道。
    /// 输入：`self`。
    /// 输出/副作用：写入 `track_render.requests` 与阶段；未修改工程。
    /// 复杂度：时间 O(选中轨道数)，空间 O(选中轨道数)。
    ///
    /// Module: app / track rendering
    /// Purpose: Open the render confirmation modal for the selected track(s).
    /// Inputs: `self`.
    /// Outputs / side effects: fills `track_render.requests` and the phase; the project is untouched.
    /// Complexity: O(selected tracks) time and space.
    pub fn request_track_render(&mut self) {
        if self.track_render.is_rendering() || self.export.busy {
            self.status = "已有渲染任务正在进行".into();
            return;
        }
        if self.instrument_load_pending {
            self.status = "正在加载插件，请稍候…".into();
            return;
        }
        let mut indices = self.selected_tracks.clone();
        if indices.is_empty() {
            indices.push(self.selected_track);
        }
        indices.sort_unstable();
        indices.dedup();
        // 选中轨道里只保留最上层：渲染组合轨道时已经包含其成员。
        // Keep only the topmost selection: rendering a group already covers its members.
        let selected = indices.clone();
        indices.retain(|index| {
            !selected.iter().any(|other| {
                other != index && *other < *index && *index < self.project.subtree_end(*other)
            })
        });
        let mut requests: Vec<RenderRequest> = indices
            .iter()
            .filter_map(|index| self.project.tracks.get(*index))
            .map(|track| RenderRequest {
                target: track.id,
                replace: None,
            })
            .collect();
        if requests.is_empty() {
            self.status = "没有可渲染的轨道".into();
            return;
        }
        requests.retain(|request| {
            self.project
                .track_index_of(request.target)
                .map(|index| self.project.subtree_content_end(index) > 0.0)
                .unwrap_or(false)
        });
        if requests.is_empty() {
            self.status = "选中的轨道没有音频内容".into();
            return;
        }
        self.track_render.requests = requests;
        self.track_render.phase = TrackRenderPhase::Confirm;
    }

    /// 所属模块：app / 轨道渲染
    /// 功能：为一条渲染音频轨道打开“重新渲染”确认弹窗。
    /// 输入：`audio_track_id`：渲染生成的音频轨道 id。
    /// 输出/副作用：写入 `track_render.requests` 与阶段；来源轨道缺失时写状态栏。
    /// 复杂度：时间 O(轨道数)，空间 O(1)。
    ///
    /// Module: app / track rendering
    /// Purpose: Open the re-render confirmation for one rendered audio track.
    /// Inputs: `audio_track_id`: the rendered audio track id.
    /// Outputs / side effects: fills `track_render.requests` and the phase; a missing source track
    /// writes the status bar.
    /// Complexity: O(tracks) time and O(1) space.
    pub fn request_track_rerender(&mut self, audio_track_id: TrackId) {
        if self.track_render.is_rendering() || self.export.busy {
            self.status = "已有渲染任务正在进行".into();
            return;
        }
        let Some(track) = self
            .project
            .tracks
            .iter()
            .find(|track| track.id == audio_track_id)
        else {
            return;
        };
        let Some(source) = track.rendered_from else {
            self.status = "该轨道不是渲染生成的音频轨道".into();
            return;
        };
        if self.project.track_index_of(source).is_none() {
            self.status = "来源轨道已被删除，无法重新渲染".into();
            return;
        }
        self.track_render.requests = vec![RenderRequest {
            target: source,
            replace: Some(audio_track_id),
        }];
        self.track_render.phase = TrackRenderPhase::Confirm;
    }

    /// 所属模块：app / 轨道渲染
    /// 功能：确认渲染：暂停播放、生成任务队列并开始第一个任务。
    /// 输入：`self`。
    /// 输出/副作用：暂停走带、建立任务、进入渲染阶段；不修改工程。
    /// 复杂度：时间 O(轨道数 + 选中数)，空间 O(选中数)。
    ///
    /// Module: app / track rendering
    /// Purpose: Confirm rendering: pause the transport, build the task queue and start the first job.
    /// Inputs: `self`.
    /// Outputs / side effects: pauses the transport, builds tasks and enters the render phase; the
    /// project is untouched.
    /// Complexity: O(tracks + requests) time and O(requests) space.
    pub fn confirm_track_render(&mut self) {
        if self.track_render.phase != TrackRenderPhase::Confirm {
            return;
        }
        let requests = std::mem::take(&mut self.track_render.requests);
        let mut tasks = VecDeque::new();
        for request in requests {
            let Some(index) = self.project.track_index_of(request.target) else {
                continue;
            };
            let path = match request.replace {
                Some(audio_track_id) => {
                    self.project
                        .tracks
                        .iter()
                        .find(|track| track.id == audio_track_id)
                        .and_then(|track| {
                            track.clips.iter().find_map(|clip| {
                                clip.audio.as_ref().map(|audio| audio.path.clone())
                            })
                        })
                        .unwrap_or_else(|| self.track_render_path(index))
                }
                None => self.track_render_path(index),
            };
            tasks.push_back(RenderTask {
                target: request.target,
                replace: request.replace,
                path,
            });
        }
        if tasks.is_empty() {
            self.track_render.phase = TrackRenderPhase::Idle;
            self.status = "没有可渲染的轨道".into();
            return;
        }
        self.pause_transport();
        self.track_render.total = tasks.len();
        self.track_render.completed = 0;
        self.track_render.progress = 0.0;
        self.track_render.queue = tasks;
        self.track_render.job = None;
        self.track_render.rx = None;
        self.track_render.phase = TrackRenderPhase::Rendering;
        self.track_render.status = "正在准备渲染…".into();
        self.begin_next_track_render();
    }

    /// 所属模块：app / 轨道渲染
    /// 功能：取消渲染：结束引擎离线会话并回到空闲；已完成的音频保留。
    /// 输入：`self`。
    /// 输出/副作用：恢复引擎工程与走带、清空任务；不清除已经生成的音频轨道。
    /// 复杂度：时间 O(工程同步)，空间 O(1)。
    ///
    /// Module: app / track rendering
    /// Purpose: Cancel rendering: end the engine offline session and return to idle; finished audio
    /// stays.
    /// Inputs: `self`.
    /// Outputs / side effects: restores the engine project/transport and clears the queue; already
    /// produced audio tracks are kept.
    /// Complexity: O(project sync) time and O(1) space.
    pub fn cancel_track_render(&mut self) {
        if self.track_render.phase == TrackRenderPhase::Idle {
            return;
        }
        self.track_render.requests.clear();
        self.track_render.queue.clear();
        self.track_render.job = None;
        self.track_render.rx = None;
        self.track_render.phase = TrackRenderPhase::Idle;
        self.track_render.progress = 0.0;
        if let Ok(mut engine) = self.engine.lock() {
            engine.cancel_offline_render();
        }
        self.needs_sync = true;
        self.track_render.status = "已取消渲染".into();
        self.status = self.track_render.status.clone();
    }

    /// 所属模块：app / 轨道渲染
    /// 功能：每帧推进渲染：接收写盘结果、开始下一个任务或渲染一个时间片。
    /// 输入：`self`。
    /// 输出/副作用：更新 `track_render` 状态与进度；完成后修改工程。
    /// 复杂度：时间 O(每帧块数 × 512 × 轨道数)，空间 O(已渲染样本)。
    ///
    /// Module: app / track rendering
    /// Purpose: Advance rendering per frame: receive writer results, start the next task or render a
    /// time slice.
    /// Inputs: `self`.
    /// Outputs / side effects: updates `track_render`; applies project changes when finished.
    /// Complexity: O(blocks per frame × 512 × tracks) time and O(rendered samples) space.
    pub(super) fn poll_track_render(&mut self) {
        if let Some(rx) = &self.track_render.rx {
            match rx.try_recv() {
                Ok(write) => {
                    self.track_render.rx = None;
                    match write.result {
                        Ok(seconds) => self.apply_track_render(&write.task, seconds),
                        Err(error) => self.finish_track_render(&format!("渲染失败：{error}")),
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.track_render.rx = None;
                    self.finish_track_render("渲染失败：后台写入线程中断");
                }
            }
            return;
        }
        if self.track_render.phase != TrackRenderPhase::Rendering {
            return;
        }
        if self.track_render.job.is_none() {
            if self.track_render.completed >= self.track_render.total {
                self.finish_track_render("渲染完成，原轨道已静音");
            } else {
                self.begin_next_track_render();
            }
            return;
        }
        self.step_track_render();
    }

    /// 所属模块：app / 轨道渲染
    /// 功能：从队列取出下一个任务，构建只含目标子树的工程快照并在实时引擎上开始离线渲染。
    /// 输入：`self`。
    /// 输出/副作用：暂停已被确认阶段完成；写入 `track_render.job`；失败时结束流程。
    /// 复杂度：时间 O(工程同步)，空间 O(工程快照)。
    ///
    /// Module: app / track rendering
    /// Purpose: Take the next task, build a snapshot containing only the target subtree and start the
    /// offline render on the live engine.
    /// Inputs: `self`.
    /// Outputs / side effects: writes `track_render.job`; failures end the flow.
    /// Complexity: O(project sync) time and O(project snapshot) space.
    fn begin_next_track_render(&mut self) {
        let Some(task) = self.track_render.queue.pop_front() else {
            self.finish_track_render("渲染完成，原轨道已静音");
            return;
        };
        let Some(index) = self.project.track_index_of(task.target) else {
            self.finish_track_render("渲染失败：来源轨道已被删除");
            return;
        };
        let (render_project, end_beat) = match self.build_render_project(index) {
            Ok(value) => value,
            Err(error) => {
                self.finish_track_render(&format!("渲染失败：{error}"));
                return;
            }
        };
        if end_beat <= 0.0 {
            self.finish_track_render("渲染失败：选中的轨道没有音频内容");
            return;
        }
        let engine = self.engine.clone();
        let mut guard = match engine.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                // 音频回调正持锁：把任务放回队列，下一帧重试。
                // The audio callback holds the lock: put the task back and retry next frame.
                self.track_render.queue.push_front(task);
                return;
            }
        };
        let options = OfflineRenderOptions {
            sample_rate: guard.sample_rate,
            layout: ChannelLayout::Stereo,
            range: RenderRange::Loop,
            tail_beats: 0.0,
            gpu: None,
        };
        let total = match guard.begin_offline_render(&render_project, &options) {
            Ok(total) => total,
            Err(error) => {
                drop(guard);
                self.finish_track_render(&format!("渲染失败：{error}"));
                return;
            }
        };
        drop(guard);
        self.track_render.status = format!(
            "{}：正在渲染（共 {total} 帧）",
            task.path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "轨道音频".into())
        );
        self.status = self.track_render.status.clone();
        self.track_render.job = Some(RenderJob {
            task,
            blocks_per_step: TRACK_RENDER_BLOCKS_PER_FRAME,
        });
        self.track_render.progress = 0.0;
    }

    /// 所属模块：app / 轨道渲染
    /// 功能：推进一个渲染时间片；完成后把样本交给写盘线程。
    /// 输入：`self`。
    /// 输出/副作用：更新进度；完成时启动写盘线程。
    /// 复杂度：时间 O(块数 × 512 × 轨道数)，空间 O(已渲染样本)。
    ///
    /// Module: app / track rendering
    /// Purpose: Advance one render time slice; once complete hand the samples to the writer thread.
    /// Inputs: `self`.
    /// Outputs / side effects: updates progress and starts the writer when done.
    /// Complexity: O(blocks × 512 × tracks) time and O(rendered samples) space.
    fn step_track_render(&mut self) {
        let Some(mut job) = self.track_render.job.take() else {
            return;
        };
        let engine = self.engine.clone();
        let mut guard = match engine.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.track_render.job = Some(job);
                return;
            }
        };
        let started = Instant::now();
        let step = guard.render_offline_step(job.blocks_per_step);
        let elapsed = started.elapsed();
        if elapsed > Duration::from_millis(40) {
            job.blocks_per_step = (job.blocks_per_step / 2).max(1);
        } else if elapsed < Duration::from_millis(10) {
            job.blocks_per_step = (job.blocks_per_step * 2).min(64);
        }
        match step {
            Ok(progress) => {
                self.track_render.progress = progress;
                if !guard.offline_render_finished() {
                    self.track_render.job = Some(job);
                    return;
                }
                let result = guard.finish_offline_render();
                if result.is_err() {
                    guard.cancel_offline_render();
                }
                drop(guard);
                match result {
                    Ok(render) => {
                        let task = job.task.clone();
                        self.spawn_track_render_writer(task, render);
                    }
                    Err(error) => self.finish_track_render(&format!("渲染失败：{error}")),
                }
            }
            Err(error) => {
                drop(guard);
                if let Ok(mut guard) = engine.lock() {
                    guard.cancel_offline_render();
                }
                self.finish_track_render(&format!("渲染失败：{error}"));
            }
        }
    }

    /// 所属模块：app / 轨道渲染
    /// 功能：在后台线程把已渲染样本写成 24-bit WAV，并回传音频时长。
    /// 输入：
    /// - `task`：当前任务。
    /// - `render`：交错立体声样本。
    /// 输出/副作用：设置 `track_render.rx`；写盘在后台完成。
    /// 复杂度：时间 O(样本数)，空间 O(样本数)。
    ///
    /// Module: app / track rendering
    /// Purpose: Write the rendered samples to a 24-bit WAV on a worker thread and report the duration.
    /// Inputs:
    /// - `task`: current task.
    /// - `render`: interleaved stereo samples.
    /// Outputs / side effects: sets `track_render.rx`; writing happens in the background.
    /// Complexity: O(samples) time and space.
    fn spawn_track_render_writer(
        &mut self,
        task: RenderTask,
        render: crate::audio::engine::OfflineRender,
    ) {
        let (tx, rx) = channel();
        self.track_render.rx = Some(rx);
        self.track_render.progress = 1.0;
        self.track_render.status = "正在写入音频文件…".into();
        self.status = self.track_render.status.clone();
        let sample_rate = render.sample_rate;
        let channels = render.channels.max(1);
        let seconds = render.samples.len() as f64 / (channels as f64 * sample_rate.max(1.0));
        let encode = AudioEncodeOptions {
            sample_rate,
            channels,
            bit_depth: AudioBitDepth::Int24,
        };
        let path = task.path.clone();
        std::thread::spawn(move || {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let result = AudioRuntime::standard()
                .export_with_options(&path, &render.samples, &encode)
                .map(|()| seconds)
                .map_err(|error| format!("写入 {} 失败：{error}", path.display()));
            let _ = tx.send(RenderWrite { task, result });
        });
    }

    /// 所属模块：app / 轨道渲染
    /// 功能：把完成的渲染应用到工程：新建音频轨道或替换重新渲染的片段，并静音来源轨道。
    /// 输入：
    /// - `task`：已完成的任务。
    /// - `seconds`：渲染音频时长。
    /// 输出/副作用：可撤销地修改工程、刷新波形缓存与选择；后台重新解码音频文件。
    /// 复杂度：时间 O(轨道数 + 片段数)，空间 O(1)。
    ///
    /// Module: app / track rendering
    /// Purpose: Apply a finished render: add an audio track or replace the re-rendered clip, then
    /// mute the source track.
    /// Inputs:
    /// - `task`: the finished task.
    /// - `seconds`: rendered duration.
    /// Outputs / side effects: undoably edits the project, refreshes the waveform cache and selection
    /// and re-decodes the audio file in the background.
    /// Complexity: O(tracks + clips) time and O(1) space.
    fn apply_track_render(&mut self, task: &RenderTask, seconds: f64) {
        let Some(target_index) = self.project.track_index_of(task.target) else {
            self.finish_track_render("渲染失败：来源轨道已被删除");
            return;
        };
        self.push_undo_now();
        let bpm = self.project.bpm.max(1.0);
        let length = (seconds * bpm / 60.0).max(0.05);
        let mut applied = false;
        if let Some(audio_track_id) = task.replace {
            if let Some(audio_index) = self.project.track_index_of(audio_track_id) {
                let mut replaced = false;
                if let Some(track) = self.project.tracks.get_mut(audio_index) {
                    if let Some(clip) = track.clips.iter_mut().find(|clip| clip.audio.is_some()) {
                        clip.length = length;
                        if let Some(audio) = clip.audio.as_mut() {
                            audio.path = task.path.clone();
                            audio.length_seconds = seconds;
                        }
                        clip.bump_revision();
                        replaced = true;
                    }
                    track.bump_revision();
                }
                applied = replaced;
            }
        }
        if !applied {
            let name = self.project.tracks[target_index].name.clone();
            let color = self.project.tracks[target_index].color;
            let parent = self.project.tracks[target_index].group;
            let insert_at = self.project.subtree_end(target_index);
            let id = self.project.next_track_id();
            let clip_id = self.project.next_clip_id();
            let mut track = Track::new(
                id,
                &format!("{name} · 渲染音频"),
                color,
                InstrumentRef::Synth(Default::default()),
            );
            track.kind = TrackKind::Audio;
            track.group = parent;
            track.rendered_from = Some(task.target);
            let mut clip = Clip::new(clip_id, &format!("{name} · 渲染音频"), 0.0, length);
            clip.audio = Some(AudioSource {
                path: task.path.clone(),
                name: format!("{name} · 渲染音频"),
                length_seconds: seconds,
                offset_seconds: 0.0,
                gain: 1.0,
            });
            track.clips.push(clip);
            self.project.tracks.insert(insert_at, track);
            self.selected_track = insert_at;
            self.selected_tracks = vec![insert_at];
            self.selected_clip = Some(0);
            self.selected_clips = vec![(insert_at, 0)];
        }
        // 渲染完成后来源轨道自动静音，避免与原音频重复播放。
        // Mute the source track so it does not play twice with the rendered audio.
        if let Some(index) = self.project.track_index_of(task.target) {
            self.project.tracks[index].muted = true;
            self.project.tracks[index].bump_revision();
        }
        self.ui_waveforms.remove(&task.path);
        self.mark_dirty();
        // 重新渲染覆盖了同名文件，必须主动重新解码；新渲染的路径由工程同步按需解码。
        // A re-render overwrote an existing file, so it must be decoded explicitly; a fresh path is
        // requested by the project sync.
        if task.replace.is_some() {
            spawn_audio_reload(Arc::downgrade(&self.engine), task.path.clone());
        }
        self.track_render.completed += 1;
        self.track_render.job = None;
        if self.track_render.completed >= self.track_render.total {
            self.finish_track_render("渲染完成，原轨道已静音");
        } else {
            self.begin_next_track_render();
        }
    }

    /// 所属模块：app / 轨道渲染
    /// 功能：结束渲染流程并写状态栏。
    /// 输入：`message`：完成或失败提示。
    /// 输出/副作用：回到空闲阶段、清空任务与进度、请求工程同步。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / track rendering
    /// Purpose: End the render flow and report it.
    /// Inputs: `message`: completion or failure text.
    /// Outputs / side effects: returns to idle, clears tasks/progress and requests a project sync.
    /// Complexity: O(1) time and space.
    fn finish_track_render(&mut self, message: &str) {
        self.track_render.phase = TrackRenderPhase::Idle;
        self.track_render.requests.clear();
        self.track_render.queue.clear();
        self.track_render.job = None;
        self.track_render.rx = None;
        self.track_render.progress = 0.0;
        self.track_render.status = message.to_string();
        self.status = message.to_string();
        self.needs_sync = true;
    }

    /// 所属模块：app / 轨道渲染
    /// 功能：构建只保留目标子树的渲染工程：主总线中性、非目标轨道静音、独奏清空。
    /// 输入：`index`：目标轨道下标。
    /// 输出/副作用：返回工程快照与渲染结束 beat；不修改当前工程。
    /// 复杂度：时间 O(轨道数 + 片段数)，空间 O(工程快照)。
    ///
    /// Module: app / track rendering
    /// Purpose: Build a render snapshot with only the target subtree audible: neutral master, every
    /// other track muted and all solos cleared.
    /// Inputs: `index`: target track index.
    /// Outputs / side effects: the snapshot and the render end beat; the current project is untouched.
    /// Complexity: O(tracks + clips) time and O(project snapshot) space.
    fn build_render_project(&self, index: usize) -> Result<(Project, f64), String> {
        let Some(target) = self.project.tracks.get(index) else {
            return Err("轨道不存在".into());
        };
        let end_beat = self.project.subtree_content_end(index);
        let is_group = target.is_group();
        let subtree_end = self.project.subtree_end(index);
        let mut render_project = self.project.clone();
        render_project.output_layout = ChannelLayout::Stereo;
        render_project.loop_enabled = false;
        render_project.loop_start = 0.0;
        render_project.loop_end = (end_beat + TRACK_RENDER_TAIL_BEATS).max(0.25);
        render_project.master_volume = 1.0;
        render_project.master_controllers = Default::default();
        render_project.master_effect = None;
        for (track_index, track) in render_project.tracks.iter_mut().enumerate() {
            track.solo = false;
            let in_target = if is_group {
                track_index >= index && track_index < subtree_end
            } else {
                track_index == index
            };
            if !in_target {
                track.muted = true;
            }
        }
        if let Some(track) = render_project.tracks.get_mut(index) {
            track.muted = false;
            // 目标脱离上级组合：只渲染自身（组合轨道）的混音，不叠加外层组合处理。
            // Detach the target from its parent so only its own mix (or group bus) is rendered.
            track.group = None;
        }
        Ok((render_project, end_beat))
    }

    /// 所属模块：app / 轨道渲染
    /// 功能：计算渲染输出路径：工程目录或默认工程目录下的 `Renders` 文件夹。
    /// 输入：`index`：目标轨道下标。
    /// 输出/副作用：返回 WAV 路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: app / track rendering
    /// Purpose: Compute the render output path under `Renders` next to the project or in the default
    /// project directory.
    /// Inputs: `index`: target track index.
    /// Outputs / side effects: the WAV path; no directory is created.
    /// Complexity: O(path length) time and space.
    fn track_render_path(&self, index: usize) -> PathBuf {
        let directory = self
            .project_path
            .as_ref()
            .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
            .or_else(|| self.settings.default_project_dir.clone())
            .or_else(super::files::default_project_dir)
            .unwrap_or_else(|| std::env::temp_dir())
            .join("Renders");
        let (name, id) = self
            .project
            .tracks
            .get(index)
            .map(|track| (track.name.clone(), track.id))
            .unwrap_or_else(|| ("轨道".into(), 0));
        let stem: String = name
            .chars()
            .map(|character| {
                if matches!(
                    character,
                    '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                ) {
                    '_'
                } else {
                    character
                }
            })
            .collect();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        directory.join(format!("{stem}_{id}_render{stamp}.wav"))
    }
}
