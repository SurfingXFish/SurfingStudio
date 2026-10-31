//! 所属模块：audio::engine / 实时渲染
//! 文件用途：
//! - 实时音频回调的唯一渲染入口：推进走带、按块渲染轨道与主总线。
//! - 预计算并推进每轨音符事件，避免每个音频块扫描全部音符。
//! - 工程离线渲染（WAV 导出）。
//!
//! 函数清单：
//! - `NoteEvent`：预计算的音符开/关事件。
//! - `TrackEngine::refresh_events` / `seek_events` / `collect_events`：音符事件维护与收集。
//! - `Engine::process` / `render_block` / `render_chunk`：实时渲染路径。
//! - `Engine::render_offline` / `render_offline_with` / `OfflineRender`：整工程离线渲染（可交给 GPU）。
//! - `OfflineRenderSession` / `Engine::begin_offline_render` / `render_offline_step` /
//!   `finish_offline_render` / `cancel_offline_render`：复用实时引擎的逐帧导出会话。
//! - `mix_audio_clips` / `apply_controller_state` / `scale_velocities` / `soft_clip`：混音辅助。
//!
//! Module: audio::engine / realtime render
//! Purpose:
//! - The single realtime render entry: advance the transport and render tracks and the master bus.
//! - Precompute per-track note events so each audio block never scans every note.
//! - Render the whole project offline (WAV export).
//!
//! Functions:
//! - `NoteEvent`: precomputed note on/off event.
//! - `TrackEngine::refresh_events` / `seek_events` / `collect_events`: maintain and collect note events.
//! - `Engine::process` / `render_block` / `render_chunk`: realtime render path.
//! - `Engine::render_offline` / `render_offline_with` / `OfflineRender`: offline project render
//!   (optionally GPU-limited).
//! - `OfflineRenderSession` / `Engine::begin_offline_render` / `render_offline_step` /
//!   `finish_offline_render` / `cancel_offline_render`: step-driven export on the live engine.
//! - `mix_audio_clips` / `apply_controller_state` / `scale_velocities` / `soft_clip`: mixing helpers.

use super::*;
use crate::audio::gpu::{GpuRenderConfig, GpuRuntime};

/// 离线渲染范围。
/// Offline render range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderRange {
    /// 整个工程：从 0 到内容末尾加尾音。
    /// Whole project: from 0 to the content end plus the tail.
    #[default]
    Project,
    /// 循环区间：从 `loop_start` 到 `loop_end`（不循环回绕）。
    /// Loop region: from `loop_start` to `loop_end` (no wrap).
    Loop,
}

/// 离线渲染参数：采样率、声道布局、范围、尾长与可选 GPU 限幅。
/// Offline render options: sample rate, channel layout, range, tail and optional GPU limiter.
#[derive(Clone, Debug)]
pub struct OfflineRenderOptions {
    /// 渲染采样率（Hz）。
    /// Render sample rate in Hz.
    pub sample_rate: f64,
    /// 输出声道布局；会覆盖工程自身的布局。
    /// Output channel layout; overrides the project layout.
    pub layout: ChannelLayout,
    /// 渲染范围。
    /// Render range.
    pub range: RenderRange,
    /// 尾音长度（beat），仅 `RenderRange::Project` 使用。
    /// Tail length in beats, only used by `RenderRange::Project`.
    pub tail_beats: f64,
    /// GPU 主总线软限幅配置；`None` 使用 CPU。
    /// GPU master-limiter config; `None` uses the CPU.
    pub gpu: Option<GpuRenderConfig>,
}

impl OfflineRenderOptions {
    /// 所属模块：audio::engine / 离线渲染
    /// 功能：以工程布局与默认尾长构造标准导出参数。
    /// 输入：
    /// - `project`：用于读取布局的工程。
    /// - `sample_rate`：渲染采样率。
    /// - `gpu`：GPU 配置。
    /// 输出/副作用：返回参数；不修改工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / offline render
    /// Purpose: Build standard export options from the project layout and default tail.
    /// Inputs:
    /// - `project`: project whose layout is used.
    /// - `sample_rate`: render sample rate.
    /// - `gpu`: GPU config.
    /// Outputs / side effects: the options; the project is unchanged.
    /// Complexity: O(1) time and space.
    pub fn for_project(project: &Project, sample_rate: f64, gpu: Option<GpuRenderConfig>) -> Self {
        Self {
            sample_rate,
            layout: project.output_layout,
            range: RenderRange::Project,
            tail_beats: 2.0,
            gpu,
        }
    }
}

/// 离线渲染结果：交错多声道样本与可选回退提示。
/// Offline render result: interleaved multichannel samples plus an optional fallback warning.
pub struct OfflineRender {
    /// 交错多声道样本。
    /// Interleaved multichannel samples.
    pub samples: Vec<f32>,
    /// 渲染使用的声道数。
    /// Channel count used by the render.
    pub channels: usize,
    /// 渲染使用的采样率（Hz）。
    /// Sample rate used by the render, in Hz.
    pub sample_rate: f64,
    /// GPU 处理失败时说明已回退 CPU 渲染。
    /// Explains the CPU fallback when GPU processing failed.
    pub warning: Option<String>,
}

/// 逐帧推进的离线导出会话：复用当前实时引擎与插件实例，避免后台线程重新实例化插件。
/// Step-driven offline export session: reuses the live engine and plugin instances so no plugin is
/// instantiated on a worker thread.
pub struct OfflineRenderSession {
    saved_project: Project,
    saved_position: f64,
    saved_playing: bool,
    saved_auto_stop_enabled: bool,
    saved_defer_master_clip: bool,
    saved_sample_rate: f64,
    sample_rate: f64,
    channels: usize,
    total_frames: usize,
    rendered_frames: usize,
    buffers: Vec<Vec<f32>>,
    samples: Vec<f32>,
    gpu: Option<GpuRenderConfig>,
    warning: Option<String>,
    finished: bool,
}

impl OfflineRenderSession {
    /// 所属模块：audio::engine / 离线渲染
    /// 功能：返回 0..1 的渲染进度。
    /// 输入：`self`。
    /// 输出/副作用：进度值；不修改状态。
    /// 复杂度：O(1)。
    ///
    /// Module: audio::engine / offline render
    /// Purpose: Return render progress in 0..1.
    /// Inputs: `self`.
    /// Outputs / side effects: the progress; no state change.
    /// Complexity: O(1).
    pub fn progress(&self) -> f32 {
        if self.total_frames == 0 {
            return 1.0;
        }
        // 用 f64 计算并封顶在 0.9999：长工程（数千万帧）在 f32 下会提前四舍五入到 1.0，
        // 让调用方误以为渲染完成。
        // Compute in f64 and cap below 1.0 until finished: long renders (tens of millions of
        // frames) round to 1.0 in f32 and would look finished before the last block.
        let progress = (self.rendered_frames as f64 / self.total_frames as f64) as f32;
        if self.finished {
            1.0
        } else {
            progress.clamp(0.0, 0.9999)
        }
    }

    /// 所属模块：audio::engine / 离线渲染
    /// 功能：是否已渲染完所有帧。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：O(1)。
    ///
    /// Module: audio::engine / offline render
    /// Purpose: Whether every frame has been rendered.
    /// Inputs: `self`.
    /// Outputs / side effects: a boolean; no state change.
    /// Complexity: O(1).
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// 所属模块：audio::engine / 离线渲染
    /// 功能：返回总帧数与已渲染帧数。
    /// 输入：`self`。
    /// 输出/副作用：`(total, rendered)`；不修改状态。
    /// 复杂度：O(1)。
    ///
    /// Module: audio::engine / offline render
    /// Purpose: Return total and rendered frame counts.
    /// Inputs: `self`.
    /// Outputs / side effects: `(total, rendered)`; no state change.
    /// Complexity: O(1).
    pub fn frame_counts(&self) -> (usize, usize) {
        (self.total_frames, self.rendered_frames)
    }
}

/// 预计算的音符事件：同一音符产生一条开事件与一条关事件，按 beat 排序。
/// Precomputed note event: one note produces an on and an off event, sorted by beat.
#[derive(Clone, Copy, Debug)]
pub(super) struct NoteEvent {
    /// 事件发生的绝对 beat。
    /// Absolute beat at which the event fires.
    pub beat: f64,
    pub pitch: u8,
    pub velocity: u8,
    /// `true` 为 Note On，`false` 为 Note Off。
    /// `true` for Note On, `false` for Note Off.
    pub on: bool,
}

impl TrackEngine {
    /// 所属模块：audio::engine / 实时渲染
    /// 功能：当片段时间/内容变化时重建本轨的音符事件表，并按当前播放位置定位游标。
    /// 输入：
    /// - `track`：轨道快照。
    /// - `position_beats`：当前播放位置，用于初始化事件游标。
    /// 输出/副作用：更新 `events`、`event_signature` 与 `event_cursor`；不修改乐器状态。
    /// 复杂度：时间 O(n log n)，空间 O(n)，`n` = 本轨音符数。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Rebuild this track's note event table when clip content or timing changes,
    /// and position the cursor at the current playhead.
    /// Inputs:
    /// - `track`: track snapshot.
    /// - `position_beats`: current playhead used to initialise the event cursor.
    /// Outputs / side effects: updates `events`, `event_signature` and `event_cursor`;
    /// instrument state is unchanged.
    /// Complexity: O(n log n) time and O(n) space, `n` = notes in this track.
    pub(super) fn refresh_events(&mut self, track: &Track, position_beats: f64) {
        let signature = note_signature(track);
        if signature == self.event_signature {
            return;
        }
        self.event_signature = signature;
        self.events.clear();
        let total_notes: usize = track
            .clips
            .iter()
            .filter(|clip| clip.audio.is_none())
            .map(|clip| clip.notes.len())
            .sum();
        self.events.reserve(total_notes.saturating_mul(2));
        for clip in &track.clips {
            if clip.audio.is_some() {
                continue;
            }
            let clip_end = clip.end();
            for note in &clip.notes {
                let start = clip.start + note.start;
                let end = (start + note.length).min(clip_end);
                if end <= start + 1e-9 {
                    continue;
                }
                self.events.push(NoteEvent {
                    beat: start,
                    pitch: note.pitch,
                    velocity: note.velocity,
                    on: true,
                });
                self.events.push(NoteEvent {
                    beat: end,
                    pitch: note.pitch,
                    velocity: 0,
                    on: false,
                });
            }
        }
        // 同一 beat 上先关闭再开启，避免同音高重叠时把旧音误延长。
        // Close before opening on the same beat so overlapping same-pitch notes retrigger.
        self.events
            .sort_unstable_by(|a, b| a.beat.total_cmp(&b.beat).then_with(|| a.on.cmp(&b.on)));
        self.seek_events(position_beats);
    }

    /// 所属模块：audio::engine / 实时渲染
    /// 功能：把事件游标移动到指定 beat 之后（定位、播放、循环回绕时使用）。
    /// 输入：`beat`：目标绝对 beat。
    /// 输出/副作用：更新 `event_cursor`；不修改乐器。
    /// 复杂度：时间 O(log n)，空间 O(1)，`n` = 本轨事件数。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Move the event cursor to the first event at or after `beat`
    /// (used by seek, play and loop wrap).
    /// Inputs: `beat`: target absolute beat.
    /// Outputs / side effects: updates `event_cursor`; the instrument is unchanged.
    /// Complexity: O(log n) time and O(1) space, `n` = events in this track.
    pub(super) fn seek_events(&mut self, beat: f64) {
        self.event_cursor = self
            .events
            .partition_point(|event| event.beat < beat - 1e-9);
    }

    /// 所属模块：audio::engine / 实时渲染
    /// 功能：收集本块内（含块首）触发的音符事件，并推进游标。
    /// 输入：
    /// - `block_start` / `block_end`：本块 beat 范围。
    /// - `frames`：本块帧数。
    /// - `bps`：每个采样推进的 beat 数。
    /// - `out`：复用的事件缓冲。
    /// 输出/副作用：向 `out` 追加 `SynthEvent` 并推进 `event_cursor`。
    /// 复杂度：时间 O(本块事件数)，空间 O(本块事件数)。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Collect note events firing inside this block and advance the cursor.
    /// Inputs:
    /// - `block_start` / `block_end`: block range in beats.
    /// - `frames`: frames in this block.
    /// - `bps`: beats advanced per sample.
    /// - `out`: reused event buffer.
    /// Outputs / side effects: appends `SynthEvent`s and advances `event_cursor`.
    /// Complexity: O(events in block) time and space.
    pub(super) fn collect_events(
        &mut self,
        block_start: f64,
        block_end: f64,
        frames: usize,
        bps: f64,
        out: &mut Vec<SynthEvent>,
    ) {
        if frames == 0 || bps <= 0.0 {
            return;
        }
        let mut cursor = self.event_cursor;
        while cursor < self.events.len() {
            let event = self.events[cursor];
            // 块尾事件留给下一块，避免块边界重复触发。
            // Events exactly on the block end belong to the next block.
            if event.beat >= block_end - 1e-9 {
                break;
            }
            cursor += 1;
            let frame = if event.beat <= block_start {
                0
            } else {
                (((event.beat - block_start) / bps).floor() as usize).min(frames - 1)
            };
            if event.on {
                self.notes_active = true;
            }
            out.push(SynthEvent {
                frame,
                kind: if event.on {
                    GateKind::On
                } else {
                    GateKind::Off
                },
                pitch: event.pitch,
                velocity: if event.on { event.velocity.max(1) } else { 0 },
            });
        }
        self.event_cursor = cursor;
    }
}

/// 所属模块：audio::engine / 实时渲染
/// 功能：计算影响音符事件的轨道签名；签名不变时跳过事件重建。
/// 输入：`track`：轨道快照。
/// 输出/副作用：返回确定性签名；不修改状态。
/// 复杂度：时间 O(片段数)，空间 O(1)。
///
/// Module: audio::engine / realtime render
/// Purpose: Compute the track signature that affects note events; skip rebuilding when unchanged.
/// Inputs: `track`: track snapshot.
/// Outputs / side effects: deterministic signature; no state change.
/// Complexity: O(clip count) time and O(1) space.
fn note_signature(track: &Track) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    #[inline]
    fn mix(hash: u64, value: u64) -> u64 {
        (hash ^ value).wrapping_mul(PRIME)
    }

    let mut hash = OFFSET;
    // 只使用片段内容与时间：控制器/插件编辑 bump 的是 track.revision，不应触发排序。
    // Only clip content and timing are hashed: controller/plugin edits bump
    // track.revision and must not trigger a re-sort.
    for clip in &track.clips {
        hash = mix(hash, clip.id);
        hash = mix(hash, clip.revision);
        hash = mix(hash, clip.start.to_bits());
        hash = mix(hash, clip.length.to_bits());
        hash = mix(hash, clip.notes.len() as u64);
        hash = mix(hash, clip.audio.is_some() as u64);
    }
    hash
}

impl Engine {
    /// 所属模块：audio::engine / 实时渲染
    /// 功能：处理一次音频回调的所有帧，立体声便捷入口。
    /// 输入：
    /// - `out_l` / `out_r`：立体声输出缓冲。
    /// 输出/副作用：写入输出、推进播放位置、更新电平与 CPU 统计。
    /// 复杂度：时间 O(帧数 × 轨道数 + 本块事件数)，空间 O(1)（复用 scratch）。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Render one audio callback worth of frames through the stereo convenience entry.
    /// Inputs:
    /// - `out_l` / `out_r`: stereo output buffers.
    /// Outputs / side effects: writes the output, advances the playhead and updates levels and CPU
    /// statistics.
    /// Complexity: O(frames × tracks + events in block) time and O(1) space using scratch.
    pub fn process(&mut self, out_l: &mut [f32], out_r: &mut [f32]) {
        let frames = out_l.len().min(out_r.len());
        if frames == 0 {
            return;
        }
        // 复用两声道 scratch：回调内不新建 Vec，也不依赖调用方的缓冲数量。
        // Reuse two-channel scratch: no Vec is created in the callback and any caller buffer works.
        let mut channels = std::mem::take(&mut self.stereo_scratch);
        if channels.len() < 2 {
            channels.resize_with(2, Vec::new);
        }
        for buffer in channels.iter_mut().take(2) {
            buffer.clear();
            buffer.resize(frames, 0.0);
        }
        self.process_output(&mut channels[..2]);
        out_l[..frames].copy_from_slice(&channels[0][..frames]);
        out_r[..frames].copy_from_slice(&channels[1][..frames]);
        self.stereo_scratch = channels;
    }

    /// 所属模块：audio::engine / 实时渲染
    /// 功能：处理一次音频回调，把工程声道布局映射到设备声道数。
    /// 输入：`out`：设备声道缓冲，每个缓冲长度相同（帧数）。
    /// 输出/副作用：写入输出、推进播放位置、更新电平与 CPU 统计；布局与设备声道数不同时
    /// 自动下混或补零。
    /// 复杂度：时间 O(帧数 × (轨道数 + 声道数) + 本块事件数)，空间 O(1)（复用 scratch）。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Render one audio callback and map the project layout to the device channel count.
    /// Inputs: `out`: device channel buffers, all of the same length (frames).
    /// Outputs / side effects: writes the output, advances the playhead, updates levels and CPU
    /// statistics; when the layout and device channel count differ it downmixes or pads with zeros.
    /// Complexity: O(frames × (tracks + channels) + events in block) time and O(1) space.
    pub fn process_output(&mut self, out: &mut [Vec<f32>]) {
        // 导出会话期间实时回调必须静音：离线步骤会独占引擎推进走带，设备不应重复播放。
        // Silence the realtime callback during an export session: offline steps drive the transport,
        // so the device must not play the same project again.
        if self.offline.is_some() {
            for buffer in out.iter_mut() {
                buffer.fill(0.0);
            }
            return;
        }
        self.process_output_internal(out);
    }

    /// 所属模块：audio::engine / 实时渲染
    /// 功能：实际渲染一次回调；导出步骤复用该入口但不静音。
    /// 输入：`out`：设备声道缓冲。
    /// 输出/副作用：写入输出、推进播放位置、更新电平与 CPU 统计。
    /// 复杂度：O(帧数 × (轨道数 + 声道数) + 本块事件数)。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Render one callback; export steps reuse this entry without the silence guard.
    /// Inputs: `out`: device channel buffers.
    /// Outputs / side effects: writes the output, advances the playhead, updates levels and CPU.
    /// Complexity: O(frames × (tracks + channels) + events in block).
    fn process_output_internal(&mut self, out: &mut [Vec<f32>]) {
        let frames = out.first().map(Vec::len).unwrap_or(0);
        if frames == 0 || out.is_empty() {
            return;
        }
        let started = Instant::now();
        for buffer in out.iter_mut() {
            buffer[..frames].fill(0.0);
        }

        if !self.playing {
            if self.live_timeout > 0.0 {
                self.render_live(out, frames);
                self.live_timeout -= frames as f32 / self.sample_rate as f32;
            } else {
                for level in &mut self.levels {
                    *level *= 0.85;
                }
                for level in &mut self.levels_l {
                    *level *= 0.85;
                }
                for level in &mut self.levels_r {
                    *level *= 0.85;
                }
                self.master_peak *= 0.85;
                self.master_peak_l *= 0.85;
                self.master_peak_r *= 0.85;
                self.cpu_load *= 0.9;
            }
            return;
        }

        self.render_block(out, frames);

        let elapsed = started.elapsed().as_secs_f32();
        let budget = frames as f32 / self.sample_rate as f32;
        if budget > 0.0 {
            self.cpu_load = self.cpu_load * 0.9 + (elapsed / budget) * 0.1;
        }
    }

    /// 所属模块：audio::engine / 实时渲染
    /// 功能：按固定 block 推进走带，处理循环、自动停止与块边界。
    /// 输入：
    /// - `out`：设备声道缓冲。
    /// - `frames`：本回调帧数。
    /// 输出/副作用：写入输出、推进 `position_beats`、可能在自动停止点暂停。
    /// 复杂度：时间 O(帧数 × 轨道数 + 本块事件数)，空间 O(1)。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Advance the transport in fixed blocks, handling loop wrap, auto-stop and block
    /// boundaries.
    /// Inputs:
    /// - `out`: device channel buffers.
    /// - `frames`: frames in this callback.
    /// Outputs / side effects: writes the output, advances `position_beats` and may pause at the
    /// auto-stop point.
    /// Complexity: O(frames × tracks + events in block) time and O(1) space.
    fn render_block(&mut self, out: &mut [Vec<f32>], frames: usize) {
        let bps = self.project.bpm.max(1.0) / (60.0 * self.sample_rate);
        let mut offset = 0;
        while offset < frames {
            let mut chunk = self.block_size.min(frames - offset).max(1);
            let auto_stop_beat = self.auto_stop_beat();
            let loop_active = self.project.loop_enabled
                && self.project.loop_end > self.project.loop_start
                && !(auto_stop_beat.is_some() && self.project.loop_end > auto_stop_beat.unwrap());

            if loop_active && self.position_beats >= self.project.loop_end - 1e-9 {
                self.position_beats = self.project.loop_start.max(0.0);
                self.audio_cursors.clear();
                self.reset_note_state(self.position_beats);
            }
            if let Some(limit) = auto_stop_beat {
                if !loop_active && self.position_beats >= limit - 1e-9 {
                    self.stop_at_limit(limit);
                    break;
                }
                if !loop_active {
                    let until = ((limit - self.position_beats) / bps).ceil() as usize;
                    chunk = chunk.min(until.max(1));
                }
            }
            if loop_active {
                let until = ((self.project.loop_end - self.position_beats) / bps).ceil() as usize;
                chunk = chunk.min(until.max(1));
            }
            let end = (offset + chunk).min(frames);
            self.render_chunk(out, offset, end, true);
            offset = end;

            if let Some(limit) = auto_stop_beat {
                if !loop_active && self.position_beats >= limit - 1e-9 {
                    self.stop_at_limit(limit);
                    break;
                }
            }
        }
    }

    /// 所属模块：audio::engine / 实时渲染
    /// 功能：非播放状态下渲染试听音符。
    /// 输入：
    /// - `out`：设备声道缓冲。
    /// - `frames`：本回调帧数。
    /// 输出/副作用：写入输出并更新电平；不推进走带。
    /// 复杂度：时间 O(帧数 × 轨道数)，空间 O(1)。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Render auditioned notes while the transport is stopped.
    /// Inputs:
    /// - `out`: device channel buffers.
    /// - `frames`: frames in this callback.
    /// Outputs / side effects: writes the output and updates levels; the playhead does not move.
    /// Complexity: O(frames × tracks) time and O(1) space.
    fn render_live(&mut self, out: &mut [Vec<f32>], frames: usize) {
        let mut offset = 0;
        while offset < frames {
            let end = (offset + self.block_size).min(frames);
            self.render_chunk(out, offset, end, false);
            offset = end;
        }
    }

    /// 所属模块：audio::engine / 实时渲染
    /// 功能：渲染单个 block：音符事件 → 乐器 → 音频片段 → 插入效果器 → 音量/声像 → 通道对 → 主总线。
    /// 输入：
    /// - `out`：设备声道缓冲。
    /// - `start` / `end`：本块在缓冲中的帧范围。
    /// - `advance`：是否推进走带与时间轴事件（试听时为 `false`）。
    /// 输出/副作用：写入输出、更新轨道/主电平；`advance` 时推进 `position_beats`。
    /// 复杂度：时间 O(帧数 × 轨道数 + 本块事件数 + 帧数 × 重叠音频片段数)，
    /// 空间 O(1)（scratch 复用）。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Render one block: note events → instrument → audio clips → insert effects →
    /// volume/pan → channel pair → master bus.
    /// Inputs:
    /// - `out`: device channel buffers.
    /// - `start` / `end`: frame range of this block inside the buffers.
    /// - `advance`: whether to advance the transport and timeline events (`false` for audition).
    /// Outputs / side effects: writes the output and updates track/master levels; advances
    /// `position_beats` when `advance` is true.
    /// Complexity: O(frames × tracks + events in block + frames × overlapping audio clips) time
    /// and O(1) space with reused scratch buffers.
    fn render_chunk(&mut self, out: &mut [Vec<f32>], start: usize, end: usize, advance: bool) {
        let frames = end.saturating_sub(start);
        if frames == 0 || out.is_empty() {
            return;
        }
        let sample_rate = self.sample_rate;
        let bps = self.project.bpm.max(1.0) / (60.0 * sample_rate);
        let block_start = self.position_beats;
        let block_end = block_start + frames as f64 * bps;
        let solo_active = self.any_solo;
        let layout = self.output_layout;
        let master_count = layout.channels();

        let mut master = std::mem::take(&mut self.master_channels);
        master.resize_with(master_count, Vec::new);
        for channel in master.iter_mut() {
            channel.clear();
            channel.resize(frames, 0.0);
        }

        let mut track_l = std::mem::take(&mut self.audio_scratch_l);
        let mut track_r = std::mem::take(&mut self.audio_scratch_r);
        let mut wet_l = std::mem::take(&mut self.wet_scratch_l);
        let mut wet_r = std::mem::take(&mut self.wet_scratch_r);
        let mut gates = std::mem::take(&mut self.gates_scratch);

        // `sync_project` 按工程顺序重建轨道运行时，因此这里可以按索引直接访问。
        // `sync_project` rebuilds track runtimes in project order, so index access is safe.
        for track_index in 0..self.project.tracks.len() {
            let track = &self.project.tracks[track_index];
            let Some(runtime) = self.tracks.get_mut(track_index) else {
                continue;
            };
            let audible = !track.muted && (!solo_active || track.solo);

            if !audible {
                if runtime.notes_active || !runtime.live.is_empty() {
                    runtime.panic();
                }
                // 静音期间把事件游标推到块尾，取消静音时不会补发旧音符。
                // Skip events while muted so unmuting never replays stale notes.
                if advance {
                    runtime.seek_events(block_end);
                }
                runtime.level *= 0.7;
                runtime.level_l *= 0.7;
                runtime.level_r *= 0.7;
                if let Some(slot) = self.levels.get_mut(track_index) {
                    *slot *= 0.7;
                }
                if let Some(slot) = self.levels_l.get_mut(track_index) {
                    *slot *= 0.7;
                }
                if let Some(slot) = self.levels_r.get_mut(track_index) {
                    *slot *= 0.7;
                }
                continue;
            }

            apply_controller_state(runtime, track, block_start);

            gates.clear();
            if advance {
                runtime.collect_events(block_start, block_end, frames, bps, &mut gates);
            }
            let live = std::mem::take(&mut runtime.live);
            if live.iter().any(|event| event.kind == GateKind::On) {
                runtime.notes_active = true;
            }
            gates.extend(live);
            scale_velocities(&mut gates, &track.controllers, block_start, bps);

            track_l.clear();
            track_l.resize(frames, 0.0);
            track_r.clear();
            track_r.resize(frames, 0.0);

            match &mut runtime.instrument {
                InstrumentRuntime::Synth(synth) => {
                    synth.render(&mut track_l, &gates);
                    track_r.copy_from_slice(&track_l);
                }
                InstrumentRuntime::Drums(drums) => {
                    drums.render(&mut track_l, &gates);
                    track_r.copy_from_slice(&track_l);
                }
                InstrumentRuntime::Native(native) => {
                    native.render(&mut track_l, &gates);
                    track_r.copy_from_slice(&track_l);
                }
                InstrumentRuntime::Plugin(plugin) => {
                    plugin.process(
                        sample_rate,
                        frames,
                        None,
                        &gates,
                        &mut track_l,
                        &mut track_r,
                    );
                }
                InstrumentRuntime::Failed(_) => {}
            }

            // 音频片段在插入效果器之前汇入轨道，实时读取源文件。
            // Audio clips join the track before the insert chain and stream from source.
            if advance && runtime.has_audio_clips {
                let bend = track
                    .controllers
                    .value_at(ControllerKind::PitchBend, block_start)
                    .unwrap_or(0.5);
                let rate = 2.0_f64.powf(((bend * 2.0 - 1.0) * 2.0) as f64 / 12.0);
                let _ = mix_audio_clips(
                    &self.audio_files,
                    &mut self.audio_cursors,
                    track,
                    self.project.bpm,
                    block_start,
                    block_end,
                    bps,
                    frames,
                    rate,
                    1.0,
                    1.0,
                    &mut track_l,
                    &mut track_r,
                );
            }

            // 关闭的效果器直接旁路：实例、参数与状态保留，重新启用后立即恢复处理。
            // Disabled effects are bypassed: the instance, parameters and state stay loaded, so
            // re-enabling resumes processing immediately.
            for (effect_index, effect) in runtime.effects.iter_mut().enumerate() {
                if !track
                    .effects
                    .get(effect_index)
                    .is_some_and(|effect| effect.enabled())
                {
                    continue;
                }
                wet_l.clear();
                wet_l.resize(frames, 0.0);
                wet_r.clear();
                wet_r.resize(frames, 0.0);
                effect.process(
                    sample_rate,
                    frames,
                    Some((&track_l, &track_r)),
                    &[],
                    &mut wet_l,
                    &mut wet_r,
                );
                std::mem::swap(&mut track_l, &mut wet_l);
                std::mem::swap(&mut track_r, &mut wet_r);
            }

            let lane_volume = track
                .controllers
                .value_at(ControllerKind::Volume, block_start)
                .unwrap_or(0.5);
            let pan = track
                .controllers
                .value_at(ControllerKind::Pan, block_start)
                .map(|value| value * 2.0 - 1.0)
                .unwrap_or(track.pan);
            // 自动化 0.5 表示 100%，1.0 表示 200%。
            // Automation 0.5 means 100% and 1.0 means 200%.
            let volume = track.volume.clamp(0.0, 1.5) * lane_volume * 2.0;
            let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
            let gain_l = angle.cos() * volume;
            let gain_r = angle.sin() * volume;

            let mut peak = 0.0_f32;
            let mut peak_l = 0.0_f32;
            let mut peak_r = 0.0_f32;
            // 轨道输出路由到工程布局中的通道对；立体声布局下恒为前左/前右。
            // Track output routes to a channel pair of the project layout; stereo always uses front
            // left/right.
            let pair = (track.output_pair as usize).min(master_count / 2 - 1);
            let destination_l = pair * 2;
            let destination_r = destination_l + 1;
            // `split_at_mut` 同时取得左右声道，避免对同一 Vec 的两次可变借用。
            // `split_at_mut` yields both channels without two mutable borrows of the same Vec.
            if destination_r < master.len() {
                let (front, back) = master.split_at_mut(destination_r);
                let target_l = &mut front[destination_l];
                let target_r = &mut back[0];
                for frame in 0..frames {
                    let l = track_l[frame] * gain_l;
                    let r = track_r[frame] * gain_r;
                    target_l[frame] += l;
                    target_r[frame] += r;
                    peak = peak.max(l.abs()).max(r.abs());
                    peak_l = peak_l.max(l.abs());
                    peak_r = peak_r.max(r.abs());
                }
            }
            runtime.level = runtime.level * 0.6 + peak * 0.4;
            runtime.level_l = runtime.level_l * 0.6 + peak_l * 0.4;
            runtime.level_r = runtime.level_r * 0.6 + peak_r * 0.4;
            if let Some(slot) = self.levels.get_mut(track_index) {
                *slot = runtime.level;
            }
            if let Some(slot) = self.levels_l.get_mut(track_index) {
                *slot = runtime.level_l;
            }
            if let Some(slot) = self.levels_r.get_mut(track_index) {
                *slot = runtime.level_r;
            }
        }

        self.gates_scratch = gates;
        self.audio_scratch_l = track_l;
        self.audio_scratch_r = track_r;
        self.wet_scratch_l = wet_l;
        self.wet_scratch_r = wet_r;

        // 主效果器同样支持停用（旁路），关闭时保留实例供再次启用。环绕声布局下插件只处理
        // 前左/前右通道对，其余通道直通：插件固定 2 in / 2 out，逐对复用同一实例会串扰。
        // The master effect supports the same bypass; its instance is kept for re-enabling. With a
        // surround layout the plugin processes only the front L/R pair and the other channels pass
        // through: plugins are fixed 2-in/2-out, so reusing one instance per pair would crosstalk.
        let master_enabled = self
            .project
            .master_effect
            .as_ref()
            .is_some_and(|effect| effect.enabled());
        if master_enabled {
            if let Some(effect) = &mut self.master_effect {
                let mut master_wet_l = std::mem::take(&mut self.master_wet_scratch_l);
                let mut master_wet_r = std::mem::take(&mut self.master_wet_scratch_r);
                master_wet_l.clear();
                master_wet_l.resize(frames, 0.0);
                master_wet_r.clear();
                master_wet_r.resize(frames, 0.0);
                effect.process(
                    sample_rate,
                    frames,
                    Some((&master[0], &master[1])),
                    &[],
                    &mut master_wet_l,
                    &mut master_wet_r,
                );
                std::mem::swap(&mut master[0], &mut master_wet_l);
                std::mem::swap(&mut master[1], &mut master_wet_r);
                self.master_wet_scratch_l = master_wet_l;
                self.master_wet_scratch_r = master_wet_r;
            }
        }

        let master_gain = self.project.master_volume_at(block_start);
        for channel in master.iter_mut().take(master_count) {
            for sample in channel.iter_mut() {
                *sample *= master_gain;
            }
        }

        // GPU 导出时主总线只做增益，软限幅留到整段渲染完成后由计算着色器处理；实时 GPU 路径
        // 在限定的等待时间内完成限幅，否则本块回退 CPU，保证音频回调不被显卡拖住。
        // GPU export applies master gain only and limits the whole render afterwards. The realtime
        // GPU path limits within a bounded wait and falls back to the CPU for this block otherwise,
        // so the audio callback is never held hostage by the GPU.
        let defer_clip = self.defer_master_clip;
        if !defer_clip {
            let mut gpu_error: Option<String> = None;
            let mut limited_by_gpu = false;
            if self.gpu_realtime.is_some() {
                let scratch = &mut self.gpu_interleave_scratch;
                scratch.clear();
                scratch.resize(frames * master_count, 0.0);
                for frame in 0..frames {
                    for channel in 0..master_count {
                        scratch[frame * master_count + channel] = master[channel][frame];
                    }
                }
                let outcome = match self.gpu_realtime.as_mut() {
                    Some(limiter) => {
                        limiter.process(scratch, crate::audio::gpu::GPU_REALTIME_TIMEOUT)
                    }
                    None => Err("实时 GPU 限幅未初始化".to_string()),
                };
                match outcome {
                    Ok(()) => {
                        for frame in 0..frames {
                            for channel in 0..master_count {
                                master[channel][frame] = scratch[frame * master_count + channel];
                            }
                        }
                        limited_by_gpu = true;
                    }
                    Err(error) => gpu_error = Some(error),
                }
            }
            if !limited_by_gpu {
                for channel in master.iter_mut().take(master_count) {
                    for sample in channel.iter_mut() {
                        *sample = soft_clip(*sample);
                    }
                }
                if let Some(error) = gpu_error {
                    self.gpu_realtime = None;
                    self.gpu_realtime_warning =
                        Some(format!("实时 GPU 限幅超时或失败，已回退 CPU：{error}"));
                }
            }
        }

        let mut peak = 0.0_f32;
        let mut peak_l = 0.0_f32;
        let mut peak_r = 0.0_f32;
        for (channel_index, channel) in master.iter().take(master_count).enumerate() {
            for sample in channel.iter() {
                peak = peak.max(sample.abs());
                if channel_index == 0 {
                    peak_l = peak_l.max(sample.abs());
                } else if channel_index == 1 {
                    peak_r = peak_r.max(sample.abs());
                }
            }
        }
        self.master_peak = self.master_peak * 0.6 + peak * 0.4;
        self.master_peak_l = self.master_peak_l * 0.6 + peak_l * 0.4;
        self.master_peak_r = self.master_peak_r * 0.6 + peak_r * 0.4;

        map_master_to_device(layout, &master, out, start, frames);
        self.master_channels = master;

        if advance {
            self.position_beats += frames as f64 * bps;
        }
    }

    /// 所属模块：audio::engine / 离线渲染
    /// 功能：把当前工程离线渲染为交错立体声（不使用 GPU）。
    /// 输入：
    /// - `project`：工程快照（循环会被关闭）。
    /// - `sample_rate`：渲染采样率。
    /// 输出/副作用：返回交错立体声样本；不修改原工程。
    /// 复杂度：时间 O(渲染帧数 × 轨道数)，空间 O(渲染帧数 × 2)。
    ///
    /// Module: audio::engine / offline render
    /// Purpose: Render the project offline into interleaved stereo without the GPU.
    /// Inputs:
    /// - `project`: project snapshot (looping is disabled).
    /// - `sample_rate`: render sample rate.
    /// Outputs / side effects: interleaved stereo samples; the source project is unchanged.
    /// Complexity: O(rendered frames × tracks) time and O(rendered frames × 2) space.
    pub fn render_offline(project: &Project, sample_rate: f64) -> Result<Vec<f32>, String> {
        let options = OfflineRenderOptions {
            sample_rate,
            layout: ChannelLayout::Stereo,
            range: RenderRange::Project,
            tail_beats: 2.0,
            gpu: None,
        };
        Ok(Self::render_offline_options(project, &options)?.samples)
    }

    /// 所属模块：audio::engine / 离线渲染
    /// 功能：按兼容签名离线渲染工程；可选把主总线软限幅交给 GPU。
    /// 输入：
    /// - `project`：工程快照。
    /// - `sample_rate`：渲染采样率。
    /// - `gpu`：GPU 渲染配置；`None` 或执行失败时使用 CPU 软限幅。
    /// 输出/副作用：返回样本与可选回退提示；布局、范围与尾长使用工程默认值。
    /// 复杂度：时间 O(渲染帧数 × 轨道数 + 样本数)，空间 O(样本数)。
    ///
    /// Module: audio::engine / offline render
    /// Purpose: Render offline through the legacy signature; optionally hand the master soft limiter
    /// to the GPU.
    /// Inputs:
    /// - `project`: project snapshot.
    /// - `sample_rate`: render sample rate.
    /// - `gpu`: GPU render config; `None` or failure uses the CPU limiter.
    /// Outputs / side effects: samples plus an optional fallback warning; layout, range and tail use
    /// the project defaults.
    /// Complexity: O(rendered frames × tracks + samples) time and O(samples) space.
    pub fn render_offline_with(
        project: &Project,
        sample_rate: f64,
        gpu: Option<&GpuRenderConfig>,
    ) -> Result<OfflineRender, String> {
        let options = OfflineRenderOptions::for_project(project, sample_rate, gpu.cloned());
        Self::render_offline_options(project, &options)
    }

    /// 所属模块：audio::engine / 离线渲染
    /// 功能：按完整参数离线渲染工程：布局覆盖、范围、尾长与 GPU 限幅。
    /// 输入：
    /// - `project`：工程快照（循环会被关闭）。
    /// - `options`：渲染参数。
    /// 输出/副作用：返回交错多声道样本、声道数、采样率与可选回退提示；不修改原工程。
    /// 复杂度：时间 O(渲染帧数 × 轨道数 + 样本数)，空间 O(样本数)。
    ///
    /// Module: audio::engine / offline render
    /// Purpose: Render offline with full options: layout override, range, tail and GPU limiting.
    /// Inputs:
    /// - `project`: project snapshot (looping is disabled).
    /// - `options`: render options.
    /// Outputs / side effects: interleaved multichannel samples, channel count, sample rate and an
    /// optional fallback warning; the source project is unchanged.
    /// Complexity: O(rendered frames × tracks + samples) time and O(samples) space.
    pub fn render_offline_options(
        project: &Project,
        options: &OfflineRenderOptions,
    ) -> Result<OfflineRender, String> {
        let sample_rate = options.sample_rate.max(1.0);
        let mut render_project = project.clone();
        render_project.output_layout = options.layout;
        render_project.loop_enabled = false;
        let (start_beat, end_beat) = render_range(&render_project, options);
        let total_frames = ((end_beat - start_beat).max(0.0) * 60.0 / render_project.bpm.max(1.0)
            * sample_rate)
            .ceil() as usize;
        let channels = options.layout.channels();
        let block = 512usize;

        let mut engine = Engine::new(sample_rate);
        engine.auto_stop_enabled = false;
        engine.defer_master_clip = options.gpu.is_some();
        engine.sync_project(&render_project);
        // 离线渲染不能等待 UI 的后台解码线程。
        // Offline rendering cannot wait for the UI's background decoder.
        engine.load_pending_audio_blocking();
        engine.play();
        if start_beat > 0.0 {
            engine.seek(start_beat);
        }

        let mut interleaved = Vec::with_capacity(total_frames * channels);
        let mut buffers: Vec<Vec<f32>> = (0..channels).map(|_| vec![0.0_f32; block]).collect();
        let mut rendered = 0;
        while rendered < total_frames {
            let frames = block.min(total_frames - rendered);
            for buffer in buffers.iter_mut() {
                buffer.resize(frames, 0.0);
            }
            engine.process_output(&mut buffers);
            for frame in 0..frames {
                for buffer in buffers.iter().take(channels) {
                    interleaved.push(buffer[frame]);
                }
            }
            rendered += frames;
        }
        let mut warning = None;
        if let Some(config) = &options.gpu {
            if let Err(error) =
                GpuRuntime::standard().apply_master_limiter(config, &mut interleaved)
            {
                // GPU 失败时仍要保证导出内容经过限幅，不能直接写出未处理样本。
                // A failed GPU dispatch must still export limited samples, never raw ones.
                for sample in &mut interleaved {
                    *sample = soft_clip(*sample);
                }
                warning = Some(format!("GPU 加速不可用，已回退 CPU 渲染：{error}"));
            }
        }
        Ok(OfflineRender {
            samples: interleaved,
            channels,
            sample_rate,
            warning,
        })
    }

    /// 所属模块：audio::engine / 离线渲染
    /// 功能：在实时引擎上开始一次逐帧导出：复用已加载的插件实例，保存并覆盖工程/走带状态。
    /// 输入：
    /// - `project`：导出使用的工程快照。
    /// - `options`：采样率、布局、范围、尾长与 GPU 配置。
    /// 输出/副作用：保存原工程与走带状态、设置导出布局与范围、定位并开始内部播放；实时回调
    /// 在会话期间输出静音。返回总帧数。采样率不同会重载插件，调用方应先关闭编辑器窗口。
    /// 复杂度：时间 O(工程同步 + 插件重载)，空间 O(项目快照 + 声道缓冲)。
    ///
    /// Module: audio::engine / offline render
    /// Purpose: Begin a step-driven export on the live engine, reusing loaded plugin instances and
    /// saving/overriding project and transport state.
    /// Inputs:
    /// - `project`: project snapshot to export.
    /// - `options`: sample rate, layout, range, tail and GPU config.
    /// Outputs / side effects: saves the original project/transport, applies the export layout and
    /// range, seeks and starts internal playback; the realtime callback is silent during the session.
    /// Returns the total frame count. A differing sample rate reloads plugins, so callers should close
    /// editor windows first.
    /// Complexity: O(project sync + plugin reload) time and O(project snapshot + channel buffers) space.
    pub fn begin_offline_render(
        &mut self,
        project: &Project,
        options: &OfflineRenderOptions,
    ) -> Result<usize, String> {
        if self.offline.is_some() {
            return Err("已有导出任务正在进行".into());
        }
        let sample_rate = options.sample_rate.max(1.0);
        let mut render_project = project.clone();
        render_project.output_layout = options.layout;
        render_project.loop_enabled = false;
        let (start_beat, end_beat) = render_range(&render_project, options);
        let total_frames = ((end_beat - start_beat).max(0.0) * 60.0 / render_project.bpm.max(1.0)
            * sample_rate)
            .ceil() as usize;
        if total_frames == 0 {
            return Err("导出范围为空".into());
        }
        let saved_project = self.project.clone();
        let saved_position = self.position_beats;
        let saved_playing = self.playing;
        let saved_auto_stop_enabled = self.auto_stop_enabled;
        let saved_defer_master_clip = self.defer_master_clip;
        let saved_sample_rate = self.sample_rate;

        if (sample_rate - self.sample_rate).abs() > 1.0 {
            self.set_sample_rate(sample_rate);
        }
        self.auto_stop_enabled = false;
        self.defer_master_clip = options.gpu.is_some();
        self.sync_project(&render_project);
        self.load_pending_audio_blocking();
        let channels = options.layout.channels();
        let first_frames = total_frames.min(512);
        self.offline = Some(OfflineRenderSession {
            saved_project,
            saved_position,
            saved_playing,
            saved_auto_stop_enabled,
            saved_defer_master_clip,
            saved_sample_rate,
            sample_rate,
            channels,
            total_frames,
            rendered_frames: 0,
            buffers: (0..channels).map(|_| vec![0.0; first_frames]).collect(),
            samples: Vec::with_capacity(total_frames * channels),
            gpu: options.gpu.clone(),
            warning: None,
            finished: false,
        });
        self.seek(start_beat);
        self.play();
        Ok(total_frames)
    }

    /// 所属模块：audio::engine / 离线渲染
    /// 功能：推进最多 `max_blocks` 个 512 帧块的离线渲染。
    /// 输入：`max_blocks`：本次推进的块数上限。
    /// 输出/副作用：追加交错样本并推进内部走带；返回 0..1 进度。
    /// 复杂度：时间 O(块数 × 512 × 轨道数)，空间 O(采样数)。
    ///
    /// Module: audio::engine / offline render
    /// Purpose: Advance the offline render by at most `max_blocks` 512-frame blocks.
    /// Inputs: `max_blocks`: block limit for this step.
    /// Outputs / side effects: appends interleaved samples and advances the internal transport;
    /// returns progress in 0..1.
    /// Complexity: O(blocks × 512 × tracks) time and O(samples) space.
    pub fn render_offline_step(&mut self, max_blocks: usize) -> Result<f32, String> {
        let mut session = self
            .offline
            .take()
            .ok_or_else(|| "没有导出任务".to_string())?;
        if !session.finished {
            for _ in 0..max_blocks.max(1) {
                let remaining = session.total_frames.saturating_sub(session.rendered_frames);
                if remaining == 0 {
                    session.finished = true;
                    break;
                }
                let frames = remaining.min(512);
                for buffer in &mut session.buffers {
                    buffer.clear();
                    buffer.resize(frames, 0.0);
                }
                self.process_output_internal(&mut session.buffers);
                for frame in 0..frames {
                    for buffer in &session.buffers {
                        session.samples.push(buffer[frame]);
                    }
                }
                session.rendered_frames += frames;
                if session.rendered_frames >= session.total_frames {
                    session.finished = true;
                    break;
                }
            }
        }
        let progress = session.progress();
        self.offline = Some(session);
        Ok(progress)
    }

    /// 所属模块：audio::engine / 离线渲染
    /// 功能：结束导出：GPU 限幅整段样本，恢复原工程与走带状态并返回多声道结果。
    /// 输入：`self`。
    /// 输出/副作用：仅在 `render_offline_step` 完成后调用；恢复工程/循环/自动停止/采样率；
    /// 采样率不同会再次重载插件。
    /// 复杂度：时间 O(GPU 样本数 + 工程同步)，空间 O(样本数)。
    ///
    /// Module: audio::engine / offline render
    /// Purpose: Finish the export: GPU-limit the whole sample buffer, restore the original project and
    /// transport, and return the multichannel result.
    /// Inputs: `self`.
    /// Outputs / side effects: call only after `render_offline_step` completes; restores project/loop/
    /// auto-stop/sample rate; a differing sample rate reloads plugins again.
    /// Complexity: O(GPU samples + project sync) time and O(samples) space.
    pub fn finish_offline_render(&mut self) -> Result<OfflineRender, String> {
        let mut session = self
            .offline
            .take()
            .ok_or_else(|| "没有导出任务".to_string())?;
        if !session.finished {
            self.offline = Some(session);
            return Err("导出尚未完成".into());
        }
        if let Some(config) = &session.gpu {
            if let Err(error) =
                GpuRuntime::standard().apply_master_limiter(config, &mut session.samples)
            {
                for sample in &mut session.samples {
                    *sample = soft_clip(*sample);
                }
                session.warning = Some(format!("GPU 加速不可用，已回退 CPU 渲染：{error}"));
            }
        }
        let saved_project = std::mem::replace(&mut session.saved_project, Project::empty());
        let saved_position = session.saved_position;
        let saved_playing = session.saved_playing;
        let saved_auto_stop_enabled = session.saved_auto_stop_enabled;
        let saved_defer_master_clip = session.saved_defer_master_clip;
        let saved_sample_rate = session.saved_sample_rate;
        self.restore_after_offline(
            saved_project,
            saved_position,
            saved_playing,
            saved_auto_stop_enabled,
            saved_defer_master_clip,
            saved_sample_rate,
        );
        Ok(OfflineRender {
            samples: session.samples,
            channels: session.channels,
            sample_rate: session.sample_rate,
            warning: session.warning,
        })
    }

    /// 所属模块：audio::engine / 离线渲染
    /// 功能：返回当前导出会话的（总帧数，已渲染帧数）。
    /// 输入：`self`。
    /// 输出/副作用：无会话时为 `None`；不修改状态。
    /// 复杂度：O(1)。
    ///
    /// Module: audio::engine / offline render
    /// Purpose: Return `(total_frames, rendered_frames)` of the current export session.
    /// Inputs: `self`.
    /// Outputs / side effects: `None` without a session; no state change.
    /// Complexity: O(1).
    pub fn offline_frame_counts(&self) -> Option<(usize, usize)> {
        self.offline.as_ref().map(|session| session.frame_counts())
    }

    /// 所属模块：audio::engine / 离线渲染
    /// 功能：是否正在执行导出会话。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：O(1)。
    ///
    /// Module: audio::engine / offline render
    /// Purpose: Whether an export session is active.
    /// Inputs: `self`.
    /// Outputs / side effects: a boolean; no state change.
    /// Complexity: O(1).
    pub fn offline_render_active(&self) -> bool {
        self.offline.is_some()
    }

    /// 所属模块：audio::engine / 离线渲染
    /// 功能：当前导出会话是否已渲染完所有帧。
    /// 输入：`self`。
    /// 输出/副作用：无会话时为 `false`；不修改状态。
    /// 复杂度：O(1)。
    ///
    /// Module: audio::engine / offline render
    /// Purpose: Whether the current export session has rendered every frame.
    /// Inputs: `self`.
    /// Outputs / side effects: `false` without a session; no state change.
    /// Complexity: O(1).
    pub fn offline_render_finished(&self) -> bool {
        self.offline
            .as_ref()
            .is_some_and(OfflineRenderSession::is_finished)
    }

    /// 所属模块：audio::engine / 离线渲染
    /// 功能：取消导出并恢复原工程与走带状态，丢弃已渲染样本。
    /// 输入：`self`。
    /// 输出/副作用：无会话时为空操作；采样率不同会重载插件。
    /// 复杂度：时间 O(工程同步)，空间 O(1)。
    ///
    /// Module: audio::engine / offline render
    /// Purpose: Cancel the export, restore the original project/transport and drop rendered samples.
    /// Inputs: `self`.
    /// Outputs / side effects: no-op without a session; a differing sample rate reloads plugins.
    /// Complexity: O(project sync) time and O(1) space.
    pub fn cancel_offline_render(&mut self) {
        let Some(session) = self.offline.take() else {
            return;
        };
        self.restore_after_offline(
            session.saved_project,
            session.saved_position,
            session.saved_playing,
            session.saved_auto_stop_enabled,
            session.saved_defer_master_clip,
            session.saved_sample_rate,
        );
    }

    /// 所属模块：audio::engine / 离线渲染
    /// 功能：恢复导出前的工程、走带与采样率状态。
    /// 输入：保存的工程、位置、播放标记、自动停止、GPU 延迟限幅与采样率。
    /// 输出/副作用：暂停、按需重载插件、重新同步工程并定位。
    /// 复杂度：时间 O(工程同步 + 插件重载)，空间 O(1)。
    ///
    /// Module: audio::engine / offline render
    /// Purpose: Restore the project, transport and sample rate saved before the export.
    /// Inputs: saved project, position, playing flag, auto-stop, deferred clip and sample rate.
    /// Outputs / side effects: pauses, reloads plugins when needed, re-syncs the project and seeks.
    /// Complexity: O(project sync + plugin reload) time and O(1) space.
    fn restore_after_offline(
        &mut self,
        saved_project: Project,
        saved_position: f64,
        saved_playing: bool,
        saved_auto_stop_enabled: bool,
        saved_defer_master_clip: bool,
        saved_sample_rate: f64,
    ) {
        self.pause();
        self.auto_stop_enabled = saved_auto_stop_enabled;
        self.defer_master_clip = saved_defer_master_clip;
        if (self.sample_rate - saved_sample_rate).abs() > 1.0 {
            self.set_sample_rate(saved_sample_rate);
        }
        self.sync_project(&saved_project);
        self.seek(saved_position);
        self.playing = saved_playing;
        if saved_playing {
            self.reset_note_state(saved_position);
        }
    }

    /// 所属模块：audio::engine / 实时渲染
    /// 功能：返回缓存的自动停止 beat（未启用时为空）。
    /// 输入：`self`。
    /// 输出/副作用：自动停止 beat；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Return the cached auto-stop beat (`None` when disabled).
    /// Inputs: `self`.
    /// Outputs / side effects: auto-stop beat; no state change.
    /// Complexity: O(1) time and space.
    fn auto_stop_beat(&self) -> Option<f64> {
        if !self.auto_stop_enabled {
            return None;
        }
        self.auto_stop_at
    }

    /// 所属模块：audio::engine / 实时渲染
    /// 功能：在自动停止点暂停传输并关闭全部音符。
    /// 输入：`limit`：自动停止 beat。
    /// 输出/副作用：`playing=false`、`auto_stopped=true`，播放头定位到 `limit`。
    /// 复杂度：时间 O(轨道数)，空间 O(1)。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Pause the transport at the auto-stop point and release every note.
    /// Inputs: `limit`: auto-stop beat.
    /// Outputs / side effects: `playing=false`, `auto_stopped=true`, playhead moved to `limit`.
    /// Complexity: O(track count) time and O(1) space.
    fn stop_at_limit(&mut self, limit: f64) {
        self.playing = false;
        self.position_beats = limit;
        self.auto_stopped = true;
        self.audio_cursors.clear();
        self.reset_note_state(limit);
    }
}

/// 所属模块：audio::engine / 离线渲染
/// 功能：按导出参数计算起止 beat：整个工程为 `0 .. 内容末尾 + 尾长`，循环区间为 `loop_start .. loop_end`。
/// 输入：
/// - `project`：已写入导出布局且关闭循环的工程。
/// - `options`：渲染参数。
/// 输出/副作用：返回 `(start_beat, end_beat)`；不修改状态。
/// 复杂度：时间 O(片段数)，空间 O(1)。
///
/// Module: audio::engine / offline render
/// Purpose: Compute start/end beats from the export options: the whole project is
/// `0 .. content end + tail`, the loop range is `loop_start .. loop_end`.
/// Inputs:
/// - `project`: project with the export layout already applied and looping disabled.
/// - `options`: render options.
/// Outputs / side effects: `(start_beat, end_beat)`; no state change.
/// Complexity: O(clip count) time and O(1) space.
fn render_range(project: &Project, options: &OfflineRenderOptions) -> (f64, f64) {
    match options.range {
        RenderRange::Project => (
            0.0,
            project.content_end_beat().max(4.0) + options.tail_beats.max(0.0),
        ),
        RenderRange::Loop => {
            let start = project.loop_start.max(0.0);
            let end = project.loop_end.max(start + 0.25);
            (start, end)
        }
    }
}

/// 所属模块：audio::engine / 实时渲染
/// 功能：把工程布局的主总线缓冲映射到设备声道数；声道不足时按 ITU 系数下混，过多时补零。
/// 输入：
/// - `layout`：工程输出布局。
/// - `master`：主总线声道缓冲（长度 = 布局声道数）。
/// - `out`：设备声道缓冲。
/// - `start` / `frames`：写入的帧范围。
/// 输出/副作用：写入 `out[start..start+frames]`；不修改 `master`。
/// 复杂度：时间 O(帧数 × 声道数)，空间 O(1)。
///
/// Module: audio::engine / realtime render
/// Purpose: Map the project-layout master buffers onto the device channel count; fewer channels use
/// an ITU downmix and extra channels are padded with zeros.
/// Inputs:
/// - `layout`: project output layout.
/// - `master`: master channel buffers (length = layout channels).
/// - `out`: device channel buffers.
/// - `start` / `frames`: frame range to write.
/// Outputs / side effects: writes `out[start..start+frames]`; `master` is unchanged.
/// Complexity: O(frames × channels) time and O(1) space.
fn map_master_to_device(
    layout: ChannelLayout,
    master: &[Vec<f32>],
    out: &mut [Vec<f32>],
    start: usize,
    frames: usize,
) {
    let source = master.len();
    let destination = out.len();
    if source == 0 || destination == 0 || frames == 0 {
        return;
    }
    let end = start + frames;
    if destination == source {
        for channel in 0..source {
            out[channel][start..end].copy_from_slice(&master[channel][..frames]);
        }
        return;
    }
    if destination == 1 {
        // 单声道设备：取左右下混后的平均值。
        // Mono device: average of the downmixed stereo pair.
        for frame in 0..frames {
            let mut value = 0.0_f32;
            let mut weight = 0.0_f32;
            for (index, channel) in master.iter().enumerate() {
                let gain = match (layout, index) {
                    (ChannelLayout::Stereo, _) => 0.5,
                    (_, 0) | (_, 1) => 1.0,
                    (_, 2) => std::f32::consts::FRAC_1_SQRT_2,
                    (_, 3) => 0.5,
                    (_, 4) | (_, 5) => std::f32::consts::FRAC_1_SQRT_2,
                    _ => 0.5,
                };
                value += channel[frame] * gain;
                weight += gain;
            }
            out[0][start + frame] = if weight > 0.0 { value / weight } else { 0.0 };
        }
        return;
    }
    if destination == 2 && source > 2 {
        // 立体声设备：按电影标准下混环绕声道。
        // Stereo device: downmix surround channels using the film standard.
        const CENTER: f32 = std::f32::consts::FRAC_1_SQRT_2;
        for frame in 0..frames {
            let mut left = master[0][frame];
            let mut right = master[1][frame];
            if source >= 3 {
                left += master[2][frame] * CENTER;
                right += master[2][frame] * CENTER;
            }
            if source >= 4 {
                left += master[3][frame] * 0.5;
                right += master[3][frame] * 0.5;
            }
            if source >= 5 {
                left += master[4][frame] * CENTER;
            }
            if source >= 6 {
                right += master[5][frame] * CENTER;
            }
            if source >= 7 {
                left += master[6][frame] * 0.5;
            }
            if source >= 8 {
                right += master[7][frame] * 0.5;
            }
            out[0][start + frame] = left;
            out[1][start + frame] = right;
        }
        return;
    }
    // 设备声道多于布局：复制布局声道，剩余保持静音（`out` 已在入口清零）。
    // More device channels than the layout: copy the layout and keep the rest silent.
    if destination > source {
        for channel in 0..source {
            out[channel][start..end].copy_from_slice(&master[channel][..frames]);
        }
        return;
    }
    // 设备为 6/8 声道但布局更大（7.1 → 5.1）：多出的声道对折入最后一个目标对。
    // A 6/8-channel device with a larger layout (7.1 → 5.1): fold extra pairs into the last target.
    for channel in 0..destination {
        out[channel][start..end].copy_from_slice(&master[channel][..frames]);
    }
    for channel in destination..source {
        let pair = (channel / 2).min(destination / 2 - 1);
        let target = pair * 2 + (channel % 2);
        for frame in 0..frames {
            out[target][start + frame] += master[channel][frame] * 0.707;
        }
    }
}

/// 所属模块：audio::engine / 实时渲染
/// 功能：主总线软限幅：0.95 以下完全线性，以上平滑饱和到 1.0。
/// 输入：`sample`：主总线样本。
/// 输出/副作用：限幅后的样本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio::engine / realtime render
/// Purpose: Master soft limiter: perfectly linear below 0.95, smoothly saturating towards 1.0.
/// Inputs: `sample`: master bus sample.
/// Outputs / side effects: limited sample; no state change.
/// Complexity: O(1) time and space.
fn soft_clip(sample: f32) -> f32 {
    const KNEE: f32 = 0.95;
    let magnitude = sample.abs();
    if magnitude <= KNEE {
        return sample;
    }
    let sign = if sample < 0.0 { -1.0 } else { 1.0 };
    let range = 1.0 - KNEE;
    sign * (KNEE + range * (1.0 - (-(magnitude - KNEE) / range).exp()))
}

/// 所属模块：audio::engine / 实时渲染
/// 功能：把与当前块重叠的音频片段按源偏移与播放速率混合到轨道缓冲。
/// 输入：
/// - `files`：已解码音频文件表。
/// - `cursors`：每个片段的源游标，跨块保持连续。
/// - `track`：所属轨道及其片段。
/// - `bpm`：工程速度。
/// - `block_start` / `block_end`：本块时间范围，单位 beat。
/// - `bps`：每个采样推进的 beat 数。
/// - `frames`：本块帧数。
/// - `rate`：弯音得到的播放速率倍率。
/// - `gain_l` / `gain_r`：轨道音量与声像后的左右增益。
/// - `out_l` / `out_r`：立体声输出缓冲。
///
/// 输出/副作用：写入 `out_l`、`out_r` 与 `cursors`，返回本块峰值。
///
/// 复杂度：时间 O(帧数 × 重叠片段数)，空间 O(1)。
///
/// Module: audio::engine / realtime render
/// Purpose: Mix audio clips overlapping this block into the track buffers using the source
/// offset and playback rate.
/// Inputs:
/// - `files`: decoded audio files.
/// - `cursors`: per-clip source cursors kept continuous across blocks.
/// - `track`: owning track and its clips.
/// - `bpm`: project tempo.
/// - `block_start` / `block_end`: block range in beats.
/// - `bps`: beats advanced per sample.
/// - `frames`: frames in this block.
/// - `rate`: playback-rate multiplier from the pitch-bend lane.
/// - `gain_l` / `gain_r`: left/right gains after track volume and pan.
/// - `out_l` / `out_r`: stereo output buffers.
///
/// Outputs / side effects: writes `out_l`, `out_r` and `cursors`; returns the block peak.
///
/// Complexity: O(frames × overlapping clips) time and O(1) space.
// 实时混音需要同时传入文件表、游标、轨道、时间与增益，参数较多但避免每块分配。
// Realtime mixing needs files, cursors, track, time and gains together to avoid
// allocating per block.
#[allow(clippy::too_many_arguments)]
fn mix_audio_clips(
    files: &HashMap<PathBuf, Arc<AudioFile>>,
    cursors: &mut HashMap<(TrackId, u64), (f64, f64)>,
    track: &Track,
    bpm: f64,
    block_start: f64,
    block_end: f64,
    bps: f64,
    frames: usize,
    rate: f64,
    gain_l: f32,
    gain_r: f32,
    out_l: &mut [f32],
    out_r: &mut [f32],
) -> (f32, f32, f32) {
    let mut peak = 0.0_f32;
    let mut peak_l = 0.0_f32;
    let mut peak_r = 0.0_f32;
    let frames = frames.min(out_l.len()).min(out_r.len());
    for clip in &track.clips {
        let Some(source) = &clip.audio else {
            continue;
        };
        if clip.start >= block_end || clip.end() <= block_start {
            continue;
        }
        let Some(file) = files.get(&source.path) else {
            continue;
        };
        if file.left.len() < 2 {
            continue;
        }
        let samples_per_beat = 60.0 / bpm.max(1.0) * file.sample_rate;
        let step = bps * samples_per_beat * rate;
        // 导入的工程可能引用源文件中的靠后区间。
        // Imported projects can point at a later region of the source file.
        let offset_samples = source.offset_seconds.max(0.0) * file.sample_rate;
        // 播放连续时沿用上一块的游标；定位或循环回绕后按时间轴重新计算。
        // Continue from the previous block when playback is contiguous. After a
        // seek or loop wrap the timeline jumps, so the cursor is recomputed.
        let key = (track.id, clip.id);
        let start_index = match cursors.get(&key) {
            Some((cursor_beat, position)) if (cursor_beat - block_start).abs() < 1e-6 => *position,
            _ => offset_samples + (block_start - clip.start).max(0.0) * samples_per_beat,
        };
        let limit = ((offset_samples + clip.length * samples_per_beat) as usize)
            .min(file.left.len().saturating_sub(1));
        let frame_offset = if block_start < clip.start {
            (((clip.start - block_start) / bps).ceil() as usize).min(frames)
        } else {
            0
        };
        for frame in frame_offset..frames {
            let index = start_index + (frame - frame_offset) as f64 * step;
            if index < 0.0 {
                continue;
            }
            let base = index.floor() as usize;
            if base + 1 >= file.left.len() || base >= limit {
                break;
            }
            let frac = (index - base as f64) as f32;
            let l = file.left[base] * (1.0 - frac) + file.left[base + 1] * frac;
            let r = file.right[base] * (1.0 - frac) + file.right[base + 1] * frac;
            let gl = l * gain_l;
            let gr = r * gain_r;
            out_l[frame] += gl;
            out_r[frame] += gr;
            peak = peak.max(gl.abs()).max(gr.abs());
            peak_l = peak_l.max(gl.abs());
            peak_r = peak_r.max(gr.abs());
        }
        cursors.insert(
            key,
            (
                block_end,
                start_index + step * (frames - frame_offset) as f64,
            ),
        );
    }
    (peak, peak_l, peak_r)
}

/// 所属模块：audio::engine / 实时渲染
/// 功能：当延音/弯音值变化时把控制器状态推送给乐器。
/// 输入：
/// - `runtime`：轨道运行时。
/// - `track`：轨道快照。
/// - `block_start`：本块起始 beat。
/// 输出/副作用：可能发送 CC64 / Pitch Bend；更新 `last_sustain` / `last_bend`。
/// 复杂度：时间 O(自动化点)，空间 O(1)。
///
/// Module: audio::engine / realtime render
/// Purpose: Push controller state (sustain/pitch bend) to the instrument when it changes.
/// Inputs:
/// - `runtime`: track runtime.
/// - `track`: track snapshot.
/// - `block_start`: block start beat.
/// Outputs / side effects: may send CC64 / Pitch Bend; updates `last_sustain` / `last_bend`.
/// Complexity: O(automation points) time and O(1) space.
fn apply_controller_state(runtime: &mut TrackEngine, track: &Track, block_start: f64) {
    let sustain = track
        .controllers
        .value_at(ControllerKind::Sustain, block_start)
        .map(|value| value >= 0.5)
        .unwrap_or(false);
    let bend = track
        .controllers
        .value_at(ControllerKind::PitchBend, block_start)
        .unwrap_or(0.5);
    let bend_semitones = (bend * 2.0 - 1.0) * 2.0;

    if runtime.last_sustain != Some(sustain) {
        match &mut runtime.instrument {
            InstrumentRuntime::Synth(synth) => synth.set_sustain(sustain),
            InstrumentRuntime::Native(native) => native.set_sustain(sustain),
            InstrumentRuntime::Drums(drums) => drums.set_sustain(sustain),
            InstrumentRuntime::Plugin(plugin) => {
                plugin.send_cc(64, if sustain { 127 } else { 0 });
            }
            InstrumentRuntime::Failed(_) => {}
        }
        runtime.last_sustain = Some(sustain);
    }

    if runtime
        .last_bend
        .map(|previous| (previous - bend).abs() > 0.001)
        .unwrap_or(true)
    {
        match &mut runtime.instrument {
            InstrumentRuntime::Synth(synth) => synth.set_pitch_bend(bend_semitones),
            InstrumentRuntime::Native(native) => native.set_pitch_bend(bend_semitones),
            InstrumentRuntime::Drums(drums) => drums.set_pitch_bend(bend_semitones),
            InstrumentRuntime::Plugin(plugin) => plugin.send_pitch_bend(bend),
            InstrumentRuntime::Failed(_) => {}
        }
        runtime.last_bend = Some(bend);
    }
}

/// 所属模块：audio::engine / 实时渲染
/// 功能：按力度自动化缩放本块 Note On 的力度。
/// 输入：
/// - `gates`：本块音符事件。
/// - `controllers`：轨道自动化。
/// - `block_start`：本块起始 beat。
/// - `bps`：每个采样推进的 beat 数。
/// 输出/副作用：原地修改 `gates`；不修改其他状态。
/// 复杂度：时间 O(事件数 × 自动化点数)，空间 O(1)。
///
/// Module: audio::engine / realtime render
/// Purpose: Scale Note On velocities with the velocity automation lane.
/// Inputs:
/// - `gates`: note events of this block.
/// - `controllers`: track automation.
/// - `block_start`: block start beat.
/// - `bps`: beats advanced per sample.
/// Outputs / side effects: modifies `gates` in place; no other state changes.
/// Complexity: O(events × automation points) time and O(1) space.
fn scale_velocities(
    gates: &mut [SynthEvent],
    controllers: &ControllerLanes,
    block_start: f64,
    bps: f64,
) {
    if controllers.velocity.is_empty() {
        return;
    }
    for gate in gates.iter_mut() {
        if gate.kind != GateKind::On {
            continue;
        }
        let beat = block_start + gate.frame as f64 * bps;
        if let Some(value) = evaluate_automation(&controllers.velocity, beat) {
            let scaled = gate.velocity as f32 * value * 2.0;
            gate.velocity = scaled.clamp(1.0, 127.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ChannelLayout, Clip, InstrumentRef, Note, SynthSettings};

    /// 覆盖：实时渲染在播放后无需任何预渲染即可发声，并在内容结束后自动停止。
    /// Covers: live rendering produces audio without any pre-render and auto-stops after content.
    #[test]
    fn live_rendering_plays_without_pre_render() {
        let mut project = Project::default();
        project.bpm = 120.0;
        let mut track = Track::new(
            1,
            "Live",
            [0, 0, 0],
            InstrumentRef::Synth(SynthSettings::default()),
        );
        let mut clip = Clip::new(1, "clip", 0.0, 1.0);
        clip.notes.push(Note {
            start: 0.0,
            length: 0.5,
            pitch: 60,
            velocity: 100,
        });
        track.clips.push(clip);
        project.tracks.push(track);

        let mut engine = Engine::new(48_000.0);
        engine.sync_project(&project);
        engine.play();

        let mut left = vec![0.0_f32; 512];
        let mut right = vec![0.0_f32; 512];
        let mut audible = false;
        for _ in 0..64 {
            engine.process(&mut left, &mut right);
            audible |= left.iter().any(|sample| sample.abs() > 1e-4);
        }
        assert!(audible, "live rendering should produce audio immediately");

        for _ in 0..500 {
            engine.process(&mut left, &mut right);
            if engine.auto_stopped() {
                break;
            }
        }
        assert!(engine.auto_stopped(), "transport should auto-stop");
        assert!(
            (engine.position_beats() - 9.0).abs() < 0.1,
            "auto-stop at 9 beats, position={}",
            engine.position_beats()
        );
    }

    /// 覆盖：定位到音符内部后仍能收到结束事件，事件游标不会漏音。
    /// Covers: seeking into a held note still emits its note-off; the event cursor never skips notes.
    #[test]
    fn seeking_inside_a_note_keeps_the_note_off() {
        let mut project = Project::default();
        project.bpm = 120.0;
        let mut track = Track::new(
            1,
            "Seek",
            [0, 0, 0],
            InstrumentRef::Synth(SynthSettings::default()),
        );
        let mut clip = Clip::new(1, "clip", 0.0, 8.0);
        clip.notes.push(Note {
            start: 0.0,
            length: 4.0,
            pitch: 60,
            velocity: 100,
        });
        track.clips.push(clip);
        project.tracks.push(track);

        let mut engine = Engine::new(48_000.0);
        engine.sync_project(&project);
        engine.play();
        engine.seek(2.0);
        assert_eq!(
            engine.tracks[0].event_cursor, 1,
            "cursor should sit on the note-off event"
        );

        let mut left = vec![0.0_f32; 512];
        let mut right = vec![0.0_f32; 512];
        let mut released = false;
        for _ in 0..600 {
            engine.process(&mut left, &mut right);
            if engine.tracks[0].event_cursor >= engine.tracks[0].events.len() {
                released = true;
                break;
            }
        }
        assert!(
            released,
            "the note-off event must be consumed after seeking"
        );
    }

    /// 覆盖：5.1 布局下轨道按 `output_pair` 路由到环绕声道，前左/前右保持静音。
    /// Covers: with a 5.1 layout a track routes to the surround pair selected by `output_pair`, while
    /// the front left/right stay silent.
    #[test]
    fn surround_layout_routes_tracks_to_the_selected_pair() {
        let mut project = Project::default();
        project.bpm = 120.0;
        project.output_layout = ChannelLayout::Surround51;
        let mut track = Track::new(
            1,
            "Surround",
            [0, 0, 0],
            InstrumentRef::Synth(SynthSettings::default()),
        );
        track.output_pair = 2;
        let mut clip = Clip::new(1, "clip", 0.0, 1.0);
        clip.notes.push(Note {
            start: 0.0,
            length: 1.0,
            pitch: 60,
            velocity: 120,
        });
        track.clips.push(clip);
        project.tracks.push(track);

        let mut engine = Engine::new(48_000.0);
        engine.sync_project(&project);
        engine.play();
        let mut buffers: Vec<Vec<f32>> = (0..6).map(|_| vec![0.0_f32; 512]).collect();
        let mut peaks = [0.0_f32; 6];
        for _ in 0..24 {
            engine.process_output(&mut buffers);
            for (channel, buffer) in buffers.iter().enumerate() {
                for sample in buffer {
                    peaks[channel] = peaks[channel].max(sample.abs());
                }
            }
            for buffer in &mut buffers {
                buffer.fill(0.0);
            }
        }
        assert!(peaks[0] < 1e-6, "front left should be silent: {}", peaks[0]);
        assert!(
            peaks[1] < 1e-6,
            "front right should be silent: {}",
            peaks[1]
        );
        assert!(
            peaks[4] > 1e-4 && peaks[5] > 1e-4,
            "surround pair should carry audio: {peaks:?}"
        );
    }

    /// 覆盖：长工程未完成时进度不会提前变成 100%（f32 四舍五入回归）。
    /// Covers: a long, unfinished render never reports 100% early (f32 rounding regression).
    #[test]
    fn progress_stays_below_one_until_finished() {
        let mut session = OfflineRenderSession {
            saved_project: Project::empty(),
            saved_position: 0.0,
            saved_playing: false,
            saved_auto_stop_enabled: true,
            saved_defer_master_clip: false,
            saved_sample_rate: 48_000.0,
            sample_rate: 48_000.0,
            channels: 2,
            total_frames: 20_000_000,
            rendered_frames: 19_999_999,
            buffers: Vec::new(),
            samples: Vec::new(),
            gpu: None,
            warning: None,
            finished: false,
        };
        assert!(session.progress() < 1.0);
        session.finished = true;
        assert_eq!(session.progress(), 1.0);
    }

    /// 覆盖：逐帧离线导出与一次性离线渲染输出一致，结束后恢复走带状态。
    /// Covers: step-driven export matches the batch render and restores the transport afterwards.
    #[test]
    fn step_offline_render_matches_batch_render() {
        let mut project = Project::default();
        project.bpm = 120.0;
        project.loop_start = 0.0;
        project.loop_end = 1.0;
        let mut track = Track::new(
            1,
            "Step",
            [0, 0, 0],
            InstrumentRef::Synth(SynthSettings::default()),
        );
        let mut clip = Clip::new(1, "clip", 0.0, 1.0);
        clip.notes.push(Note {
            start: 0.0,
            length: 0.5,
            pitch: 60,
            velocity: 120,
        });
        track.clips.push(clip);
        project.tracks.push(track);

        let options = OfflineRenderOptions {
            sample_rate: 48_000.0,
            layout: ChannelLayout::Stereo,
            range: RenderRange::Loop,
            tail_beats: 0.0,
            gpu: None,
        };
        let expected = Engine::render_offline_options(&project, &options).expect("batch render");

        let mut engine = Engine::new(48_000.0);
        let total = engine
            .begin_offline_render(&project, &options)
            .expect("begin");
        assert!(total > 0, "the loop range must contain frames");
        let mut steps = 0;
        loop {
            let progress = engine.render_offline_step(4).expect("step");
            steps += 1;
            assert!(steps < 10_000, "the step export never finished");
            if progress >= 0.999 {
                break;
            }
        }
        assert!(
            engine.offline_render_finished(),
            "the progress threshold must match the real finished state"
        );
        let stepped = engine.finish_offline_render().expect("finish");
        assert_eq!(stepped.channels, expected.channels);
        assert_eq!(stepped.samples.len(), expected.samples.len());
        assert!(
            stepped.samples.iter().any(|sample| sample.abs() > 1e-4),
            "the stepped export must contain audio"
        );
        assert!(
            !engine.is_playing(),
            "the transport must be restored after the export"
        );
        assert!(!engine.offline_render_active());
    }

    /// 覆盖：取消导出后丢弃会话并恢复播放状态。
    /// Covers: cancelling an export drops the session and restores the transport.
    #[test]
    fn cancel_offline_render_restores_transport() {
        let project = Project::demo();
        let options = OfflineRenderOptions {
            sample_rate: 48_000.0,
            layout: ChannelLayout::Stereo,
            range: RenderRange::Project,
            tail_beats: 0.0,
            gpu: None,
        };
        let mut engine = Engine::new(48_000.0);
        engine
            .begin_offline_render(&project, &options)
            .expect("begin");
        engine.render_offline_step(1).expect("step");
        engine.cancel_offline_render();
        assert!(!engine.offline_render_active());
        assert!(!engine.is_playing());
        assert!(engine.finish_offline_render().is_err());
    }

    /// 覆盖：5.1 离线渲染输出 6 声道交错样本。
    /// Covers: 5.1 offline rendering returns six interleaved channels.
    #[test]
    fn offline_render_supports_surround_layouts() {
        let project = Project::demo();
        let options = OfflineRenderOptions {
            sample_rate: 48_000.0,
            layout: ChannelLayout::Surround51,
            range: RenderRange::Loop,
            tail_beats: 0.0,
            gpu: None,
        };
        let render = Engine::render_offline_options(&project, &options).expect("render");
        assert_eq!(render.channels, 6);
        assert!(!render.samples.is_empty());
        assert_eq!(render.samples.len() % 6, 0);
    }

    /// 覆盖：开启 GPU 加速时离线渲染仍输出限幅后的样本，且没有回退到 CPU。
    /// Covers: GPU-accelerated offline render still returns limited samples without falling back.
    #[test]
    #[ignore = "需要可用的 GPU 与图形驱动"]
    fn gpu_offline_render_limits_the_master_bus() {
        let mut project = Project::default();
        project.bpm = 120.0;
        project.master_volume = 1.5;
        let mut track = Track::new(
            1,
            "GPU",
            [0, 0, 0],
            InstrumentRef::Synth(SynthSettings::default()),
        );
        let mut clip = Clip::new(1, "clip", 0.0, 1.0);
        clip.notes.push(Note {
            start: 0.0,
            length: 0.5,
            pitch: 60,
            velocity: 127,
        });
        track.clips.push(clip);
        project.tracks.push(track);

        let config = GpuRenderConfig::default();
        let render =
            Engine::render_offline_with(&project, 48_000.0, Some(&config)).expect("offline render");
        assert!(
            render.warning.is_none(),
            "GPU fallback: {:?}",
            render.warning
        );
        assert!(
            render.samples.iter().all(|sample| sample.abs() <= 1.0),
            "master bus must stay inside ±1.0"
        );
        assert!(
            render.samples.iter().any(|sample| sample.abs() > 1e-4),
            "the rendered clip should not be silent"
        );
    }
}
