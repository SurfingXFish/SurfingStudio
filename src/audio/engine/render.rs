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
//! - `mix_audio_clips` / `apply_controller_state` / `scale_velocities` / `soft_clip`: mixing helpers.

use super::*;
use crate::audio::gpu::{GpuRenderConfig, GpuRuntime};

/// 离线渲染结果：交错立体声样本与可选回退提示。
/// Offline render result: interleaved stereo samples plus an optional fallback warning.
pub struct OfflineRender {
    /// 交错立体声样本。
    /// Interleaved stereo samples.
    pub samples: Vec<f32>,
    /// GPU 处理失败时说明已回退 CPU 渲染。
    /// Explains the CPU fallback when GPU processing failed.
    pub warning: Option<String>,
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
    /// 功能：处理一次音频回调的所有帧，实时渲染当前工程。
    /// 输入：
    /// - `out_l` / `out_r`：立体声输出缓冲。
    /// 输出/副作用：写入输出、推进播放位置、更新电平与 CPU 统计。
    /// 复杂度：时间 O(帧数 × 轨道数 + 本块事件数)，空间 O(1)（复用 scratch）。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Render one audio callback worth of frames from the live project.
    /// Inputs:
    /// - `out_l` / `out_r`: stereo output buffers.
    /// Outputs / side effects: writes the output, advances the playhead and updates
    /// levels and CPU statistics.
    /// Complexity: O(frames × tracks + events in block) time and O(1) space using scratch.
    pub fn process(&mut self, out_l: &mut [f32], out_r: &mut [f32]) {
        let frames = out_l.len().min(out_r.len());
        if frames == 0 {
            return;
        }
        let started = Instant::now();
        out_l.fill(0.0);
        out_r.fill(0.0);

        if !self.playing {
            if self.live_timeout > 0.0 {
                self.render_live(out_l, out_r, frames);
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

        self.render_block(out_l, out_r, frames);

        let elapsed = started.elapsed().as_secs_f32();
        let budget = frames as f32 / self.sample_rate as f32;
        if budget > 0.0 {
            self.cpu_load = self.cpu_load * 0.9 + (elapsed / budget) * 0.1;
        }
    }

    /// 所属模块：audio::engine / 实时渲染
    /// 功能：按固定 block 推进走带，处理循环、自动停止与块边界。
    /// 输入：
    /// - `out_l` / `out_r`：输出缓冲。
    /// - `frames`：本回调帧数。
    /// 输出/副作用：写入输出、推进 `position_beats`、可能在自动停止点暂停。
    /// 复杂度：时间 O(帧数 × 轨道数 + 本块事件数)，空间 O(1)。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Advance the transport in fixed blocks, handling loop wrap, auto-stop and
    /// block boundaries.
    /// Inputs:
    /// - `out_l` / `out_r`: output buffers.
    /// - `frames`: frames in this callback.
    /// Outputs / side effects: writes the output, advances `position_beats` and may pause
    /// at the auto-stop point.
    /// Complexity: O(frames × tracks + events in block) time and O(1) space.
    fn render_block(&mut self, out_l: &mut [f32], out_r: &mut [f32], frames: usize) {
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
            let (l, r) = (&mut out_l[offset..end], &mut out_r[offset..end]);
            self.render_chunk(l, r, true);
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
    /// - `out_l` / `out_r`：输出缓冲。
    /// - `frames`：本回调帧数。
    /// 输出/副作用：写入输出并更新电平；不推进走带。
    /// 复杂度：时间 O(帧数 × 轨道数)，空间 O(1)。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Render auditioned notes while the transport is stopped.
    /// Inputs:
    /// - `out_l` / `out_r`: output buffers.
    /// - `frames`: frames in this callback.
    /// Outputs / side effects: writes the output and updates levels; the playhead does not move.
    /// Complexity: O(frames × tracks) time and O(1) space.
    fn render_live(&mut self, out_l: &mut [f32], out_r: &mut [f32], frames: usize) {
        let mut offset = 0;
        while offset < frames {
            let end = (offset + self.block_size).min(frames);
            let (l, r) = (&mut out_l[offset..end], &mut out_r[offset..end]);
            self.render_chunk(l, r, false);
            offset = end;
        }
    }

    /// 所属模块：audio::engine / 实时渲染
    /// 功能：渲染单个 block：音符事件 → 乐器 → 音频片段 → 插入效果器 → 音量/声像 → 主总线。
    /// 输入：
    /// - `out_l` / `out_r`：本块输出缓冲。
    /// - `advance`：是否推进走带与时间轴事件（试听时为 `false`）。
    /// 输出/副作用：写入输出、更新轨道/主电平；`advance` 时推进 `position_beats`。
    /// 复杂度：时间 O(帧数 × 轨道数 + 本块事件数 + 帧数 × 重叠音频片段数)，
    /// 空间 O(1)（scratch 复用）。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Render one block: note events → instrument → audio clips → insert effects →
    /// volume/pan → master bus.
    /// Inputs:
    /// - `out_l` / `out_r`: block output buffers.
    /// - `advance`: whether to advance the transport and timeline events (`false` for audition).
    /// Outputs / side effects: writes the output and updates track/master levels; advances
    /// `position_beats` when `advance` is true.
    /// Complexity: O(frames × tracks + events in block + frames × overlapping audio clips) time
    /// and O(1) space with reused scratch buffers.
    fn render_chunk(&mut self, out_l: &mut [f32], out_r: &mut [f32], advance: bool) {
        let frames = out_l.len().min(out_r.len());
        if frames == 0 {
            return;
        }
        let sample_rate = self.sample_rate;
        let bps = self.project.bpm.max(1.0) / (60.0 * sample_rate);
        let block_start = self.position_beats;
        let block_end = block_start + frames as f64 * bps;
        let solo_active = self.any_solo;

        let mut master_l = std::mem::take(&mut self.master_scratch_l);
        let mut master_r = std::mem::take(&mut self.master_scratch_r);
        master_l.clear();
        master_l.resize(frames, 0.0);
        master_r.clear();
        master_r.resize(frames, 0.0);

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
            for frame in 0..frames {
                let l = track_l[frame] * gain_l;
                let r = track_r[frame] * gain_r;
                master_l[frame] += l;
                master_r[frame] += r;
                peak = peak.max(l.abs()).max(r.abs());
                peak_l = peak_l.max(l.abs());
                peak_r = peak_r.max(r.abs());
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

        // 主效果器同样支持停用（旁路），关闭时保留实例供再次启用。
        // The master effect supports the same bypass; its instance is kept for re-enabling.
        let master_enabled = self
            .project
            .master_effect
            .as_ref()
            .is_some_and(|effect| effect.enabled());
        if master_enabled {
            if let Some(effect) = &mut self.master_effect {
                let mut master_wet_l = std::mem::take(&mut self.wet_scratch_l);
                let mut master_wet_r = std::mem::take(&mut self.wet_scratch_r);
                master_wet_l.clear();
                master_wet_l.resize(frames, 0.0);
                master_wet_r.clear();
                master_wet_r.resize(frames, 0.0);
                effect.process(
                    sample_rate,
                    frames,
                    Some((&master_l, &master_r)),
                    &[],
                    &mut master_wet_l,
                    &mut master_wet_r,
                );
                std::mem::swap(&mut master_l, &mut master_wet_l);
                std::mem::swap(&mut master_r, &mut master_wet_r);
                self.wet_scratch_l = master_wet_l;
                self.wet_scratch_r = master_wet_r;
            }
        }

        let master_gain = self.project.master_volume_at(block_start);
        let mut peak = 0.0_f32;
        let mut peak_l = 0.0_f32;
        let mut peak_r = 0.0_f32;
        // GPU 导出时主总线只做增益，软限幅留到整段渲染完成后由计算着色器处理。
        // GPU export applies master gain only; the soft limiter runs on the compute shader after the
        // whole render finishes.
        let defer_clip = self.defer_master_clip;
        for i in 0..frames {
            let left = master_l[i] * master_gain;
            let right = master_r[i] * master_gain;
            out_l[i] = if defer_clip { left } else { soft_clip(left) };
            out_r[i] = if defer_clip { right } else { soft_clip(right) };
            peak = peak.max(out_l[i].abs()).max(out_r[i].abs());
            peak_l = peak_l.max(out_l[i].abs());
            peak_r = peak_r.max(out_r[i].abs());
        }
        self.master_peak = self.master_peak * 0.6 + peak * 0.4;
        self.master_peak_l = self.master_peak_l * 0.6 + peak_l * 0.4;
        self.master_peak_r = self.master_peak_r * 0.6 + peak_r * 0.4;
        self.master_scratch_l = master_l;
        self.master_scratch_r = master_r;

        if advance {
            self.position_beats += frames as f64 * bps;
        }
    }

    /// 所属模块：audio::engine / 实时渲染
    /// 功能：把当前工程离线渲染为交错立体声（不使用 GPU）。
    /// 输入：
    /// - `project`：工程快照（循环会被关闭）。
    /// - `sample_rate`：渲染采样率。
    /// 输出/副作用：返回交错立体声样本；不修改原工程。
    /// 复杂度：时间 O(渲染帧数 × 轨道数)，空间 O(渲染帧数 × 2)。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Render the project offline into interleaved stereo without the GPU.
    /// Inputs:
    /// - `project`: project snapshot (looping is disabled).
    /// - `sample_rate`: render sample rate.
    /// Outputs / side effects: interleaved stereo samples; the source project is unchanged.
    /// Complexity: O(rendered frames × tracks) time and O(rendered frames × 2) space.
    pub fn render_offline(project: &Project, sample_rate: f64) -> Result<Vec<f32>, String> {
        Ok(Self::render_offline_with(project, sample_rate, None)?.samples)
    }

    /// 所属模块：audio::engine / 实时渲染
    /// 功能：把当前工程离线渲染为交错立体声；可选把主总线软限幅交给 GPU。
    /// 输入：
    /// - `project`：工程快照（循环会被关闭）。
    /// - `sample_rate`：渲染采样率。
    /// - `gpu`：GPU 渲染配置；`None` 或执行失败时使用 CPU 软限幅。
    /// 输出/副作用：返回样本与可选的回退提示；不修改原工程。
    /// 复杂度：时间 O(渲染帧数 × 轨道数 + 样本数)，空间 O(渲染帧数 × 2 + 样本数)。
    ///
    /// Module: audio::engine / realtime render
    /// Purpose: Render the project offline into interleaved stereo, optionally handing the master
    /// soft limiter to the GPU.
    /// Inputs:
    /// - `project`: project snapshot (looping is disabled).
    /// - `sample_rate`: render sample rate.
    /// - `gpu`: GPU render config; `None` or a failed dispatch falls back to the CPU limiter.
    /// Outputs / side effects: samples plus an optional fallback warning; the source project is
    /// unchanged.
    /// Complexity: O(rendered frames × tracks + samples) time and O(rendered frames × 2 +
    /// samples) space.
    pub fn render_offline_with(
        project: &Project,
        sample_rate: f64,
        gpu: Option<&GpuRenderConfig>,
    ) -> Result<OfflineRender, String> {
        let mut render_project = project.clone();
        render_project.loop_enabled = false;
        let end_beat = render_project.content_end_beat().max(4.0) + 2.0;
        let total_frames =
            (end_beat * 60.0 / render_project.bpm.max(1.0) * sample_rate).ceil() as usize;
        let block = 512usize;

        let mut engine = Engine::new(sample_rate);
        engine.auto_stop_enabled = false;
        engine.defer_master_clip = gpu.is_some();
        engine.sync_project(&render_project);
        // 离线渲染不能等待 UI 的后台解码线程。
        // Offline rendering cannot wait for the UI's background decoder.
        engine.load_pending_audio_blocking();
        engine.play();

        let mut interleaved = Vec::with_capacity(total_frames * 2);
        let mut left = vec![0.0_f32; block];
        let mut right = vec![0.0_f32; block];
        let mut rendered = 0;
        while rendered < total_frames {
            let frames = block.min(total_frames - rendered);
            let l = &mut left[..frames];
            let r = &mut right[..frames];
            engine.process(l, r);
            for i in 0..frames {
                interleaved.push(l[i]);
                interleaved.push(r[i]);
            }
            rendered += frames;
        }
        let mut warning = None;
        if let Some(config) = gpu {
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
            warning,
        })
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
    use crate::core::model::{Clip, InstrumentRef, Note, SynthSettings};

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
