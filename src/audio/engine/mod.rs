//! 所属模块：audio::engine / 引擎状态与同步
//! 文件用途：
//! - 定义实时音频引擎状态：工程快照、轨道运行时、插件链、传输与电平。
//! - 同步工程到引擎，维护乐器/效果器实例与音频片段解码请求。
//! - 提供传输控制、插件 API 与 UI 遥测查询。
//!
//! 函数清单：
//! - `Engine::new` / `build` / `set_sample_rate`：引擎构造与采样率。
//! - `Engine::sync_project` / `sync_project_slice`：同步工程、重建轨道运行时与音符事件；
//!   后者按时间片补载乐器插件。
//! - `Engine::play` / `pause` / `seek` / `reset_playback_state` / `reset_note_state`：传输与挂音清理。
//! - `Engine::audition` / `is_auditioning`：试听。
//! - `Engine::stereo_levels` / `cpu_load` / `auto_stopped`：UI 遥测。
//! - `Engine::take_audio_requests` / `load_pending_audio_blocking` / `install_audio_file` / `fail_audio_file`：音频解码。
//! - `TrackEngine::ensure_instrument` / `ensure_effects` / `panic` / `poll_parameters`：轨道运行时维护。
//! - `format_position`：`bar.beat.tick` 格式化。
//!
//! Module: audio::engine / engine state and synchronisation
//! Purpose:
//! - Define the realtime engine state: project snapshot, track runtimes, plugin chains,
//!   transport and levels.
//! - Sync the project into the engine, maintaining instruments, effects and audio decode requests.
//! - Provide transport control, plugin APIs and UI telemetry queries.
//!
//! Functions:
//! - `Engine::new` / `build` / `set_sample_rate`: engine construction and sample rate.
//! - `Engine::sync_project` / `sync_project_slice`: sync the project, rebuild track runtimes and
//!   note events; the latter loads instrument plugins in time slices.
//! - `Engine::play` / `pause` / `seek` / `reset_playback_state` / `reset_note_state`: transport and note release.
//! - `Engine::audition` / `is_auditioning`: auditioning.
//! - `Engine::stereo_levels` / `cpu_load` / `auto_stopped`: UI telemetry.
//! - `Engine::take_audio_requests` / `load_pending_audio_blocking` / `install_audio_file` / `fail_audio_file`: audio decoding.
//! - `TrackEngine::ensure_instrument` / `ensure_effects` / `panic` / `poll_parameters`: track runtime maintenance.
//! - `format_position`: `bar.beat.tick` formatting.

mod plugins;
mod render;

pub use render::{OfflineRender, OfflineRenderOptions, OfflineRenderSession, RenderRange};

use crate::audio::fx::NativeEffectProcessor;
use crate::audio::gpu::GpuRenderConfig;
use crate::audio::io::{AudioFile, AudioRuntime};
use crate::audio::synth::{
    DrumKit, GateKind, ModeledInstrument, PolySynth, SampledInstrument, SynthEvent,
};
use crate::core::model::{
    ChannelLayout, ControllerKind, ControllerLanes, EffectRef, InstrumentRef, Project, Track,
    TrackId, evaluate_automation,
};
use crate::plugin::{ParamView, PluginHost, PluginKind, PluginRuntime};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SlotKind {
    Instrument,
    Effect(usize),
    Master,
}

/// 内部渲染块长：约 10 ms 音频，限制在 512 ~ 4096 帧。
/// 512 帧在 192 kHz 下只有 2.67 ms，每秒要调用插件 375 次（48 kHz 只有 94 次）；
/// 桥接类插件（VOCALOID / ACE Bridge）每次 `process` 都要与外部编辑器进程同步，
/// 高频调用会把它们和编辑器一起拖慢。按采样率放大块长后，高采样率下的调用频率
/// 回到与 48 kHz 同一量级，回调内的块长也保持恒定（避免 512/512/128 这类尾块）。
/// Internal render block: about 10 ms of audio, clamped to 512 ~ 4096 frames.
/// 512 frames is only 2.67 ms at 192 kHz, so plugins would be called 375 times a second
/// (94 times at 48 kHz). Bridge plugins (VOCALOID / ACE Bridge) synchronise with an external editor
/// process on every `process` call, and that rate drags both the plugin and its editor down.
/// Scaling the block with the sample rate brings the call rate back to the 48 kHz ballpark and keeps
/// the block length constant (no 512/512/128 tail blocks).
fn render_block_size(sample_rate: f64) -> usize {
    let frames = (sample_rate.max(1.0) * 0.010).round() as usize;
    frames.clamp(512, 4096)
}

pub struct Engine {
    pub sample_rate: f64,
    pub block_size: usize,
    project: Project,
    tracks: Vec<TrackEngine>,
    master_effect: Option<EffectRuntime>,
    master_effect_key: Option<String>,
    plugins_sample_rate: f64,
    last_tempo: f64,
    /// 上一次记录的“最慢链 / 主效果器”延迟（采样数），用于延迟日志去重。
    /// Last recorded slowest-chain / master-effect latency in samples, used to de-duplicate the log.
    last_max_latency: usize,
    last_master_latency: usize,
    last_parameter_poll: Instant,
    /// 参数轮询游标：每次只检查一条轨道，避免一次扫描全部插件阻塞 UI。
    /// Parameter poll cursor: check one track per tick so scanning a project
    /// with many plugins never blocks the UI thread.
    parameter_poll_cursor: usize,
    playing: bool,
    position_beats: f64,
    /// 插件链总延迟（采样）：实时渲染提前这么多，抵消“开启效果器后声音整体后延”。
    /// Total plugin-chain latency in samples: the realtime renderer runs this far ahead so enabling
    /// an effect no longer shifts the whole mix behind the playhead.
    render_ahead_samples: usize,
    /// 预卷剩余帧数：先用当前位置的内容填满补偿延迟线并丢弃输出，之后声音与播放头对齐。
    /// Remaining pre-roll frames: fill the compensation delay lines with the content at the current
    /// position and drop the output, after which playback lines up with the playhead.
    preroll_frames: usize,
    /// 预卷期间对外显示的播放位置。
    /// Playhead shown while pre-rolling.
    preroll_position_beats: f64,
    /// 是否启用前瞻补偿；离线导出关闭，导出文件保持从 0 开始的原始对齐。
    /// Whether look-ahead compensation is active; offline export turns it off so the file keeps its
    /// original alignment from frame zero.
    lookahead_enabled: bool,
    levels: Vec<f32>,
    levels_l: Vec<f32>,
    levels_r: Vec<f32>,
    master_peak: f32,
    master_peak_l: f32,
    master_peak_r: f32,
    cpu_load: f32,
    live_timeout: f32,
    auto_stop_enabled: bool,
    auto_stopped: bool,
    /// Cached per-sync project facts used by the audio callback.
    any_solo: bool,
    auto_stop_at: Option<f64>,
    /// Decoded audio-clip sources, keyed by file path.
    audio_files: HashMap<PathBuf, Arc<AudioFile>>,
    /// Sources currently being decoded by the background loader.
    audio_loading: HashSet<PathBuf>,
    /// Paths that `sync_project` wants the background loader to decode.
    audio_requests: Vec<PathBuf>,
    /// Decode errors discovered after the synchronous project sync.
    audio_warnings: Vec<String>,
    /// Playback cursor of each audio clip: (last block end beat, source sample
    /// position). Keeps pitch-bend changes from jumping in the source.
    audio_cursors: HashMap<(TrackId, u64), (f64, f64)>,
    /// Scratch buffers reused by the audio callback (no per-block allocation).
    audio_scratch_l: Vec<f32>,
    audio_scratch_r: Vec<f32>,
    wet_scratch_l: Vec<f32>,
    wet_scratch_r: Vec<f32>,
    /// 主效果器的湿缓冲（与轨道效果器分开，避免每块互换后丢失 scratch）。
    /// Master-effect wet buffers, kept separate from track effects so scratch is never lost.
    master_wet_scratch_l: Vec<f32>,
    master_wet_scratch_r: Vec<f32>,
    /// 主总线按工程声道布局渲染的声道缓冲；`sync_project` 按布局调整数量。
    /// Master-bus channel buffers rendered in the project layout; `sync_project` sizes them.
    master_channels: Vec<Vec<f32>>,
    /// 立体声便捷入口（`process`）使用的两声道缓冲。
    /// Two-channel buffers used by the stereo convenience entry (`process`).
    stereo_scratch: Vec<Vec<f32>>,
    /// GPU 实时限幅的交错上传/回读缓冲。
    /// Interleaved upload/read-back buffer for the realtime GPU limiter.
    gpu_interleave_scratch: Vec<f32>,
    /// 实时 GPU 主总线软限幅；`None` 表示使用 CPU。
    /// Realtime GPU master-bus limiter; `None` means the CPU path.
    gpu_realtime: Option<crate::audio::gpu::GpuRealtimeLimiter>,
    /// GPU 神经音色训练器；与实时限幅共用同一配置，失败时神经音色退回 CPU 训练。
    /// GPU neural timbre trainer; shares the realtime config and falls back to CPU training.
    gpu_timbre: Option<crate::audio::gpu::GpuTimbreTrainer>,
    /// 当前实时 GPU 配置；与设置比较，避免每次同步都重建设备。
    /// Current realtime GPU config; compared with the settings so devices are not rebuilt every sync.
    gpu_realtime_config: Option<GpuRenderConfig>,
    /// 最近一次实时 GPU 限幅失败原因；由 UI 取走后清空。
    /// Last realtime GPU limiter failure; cleared once the UI takes it.
    gpu_realtime_warning: Option<String>,
    /// 插件当前是否收到“正在播放”的走带状态；停止时只补发一次 kPlaying=false。
    /// Whether plugins were last told the transport is playing; stop is sent once.
    plugin_transport_playing: bool,
    /// 下一次插件渲染是否需要补发位置（开始播放、定位、循环回绕、新实例）。
    /// Whether the next plugin render must resend the position (play, seek, loop wrap, new instance).
    plugin_transport_seek: bool,
    /// Reused MIDI gate buffer for direct (live) playback.
    gates_scratch: Vec<SynthEvent>,
    /// 工程输出声道布局。
    /// Project output channel layout.
    output_layout: ChannelLayout,
    /// 离线渲染时把主总线软限幅推迟到 GPU：仅导出路径使用，实时回调始终为 `false`。
    /// Offline rendering defers the master soft limiter to the GPU; export-only and always `false`
    /// for the realtime callback.
    defer_master_clip: bool,
    /// 逐帧推进的导出会话；存在时实时回调输出静音，导出步骤直接驱动渲染。
    /// Step-driven export session; while present the realtime callback outputs silence and export
    /// steps drive rendering directly.
    offline: Option<render::OfflineRenderSession>,
}

/// 乐器插件实例化的时间片预算：一次同步只新建时间片内的插件，其余留到下一次调用。
/// Instrument plugin instantiation budget: one sync creates only the plugins that fit in the time
/// slice and leaves the rest for the next call.
struct InstrumentLoadBudget {
    /// 本轮允许创建插件的截止时刻；`None` 表示不限时（普通同步与后台同步）。
    /// Deadline for creating plugins in this pass; `None` means unlimited (regular and background
    /// syncs).
    deadline: Option<Instant>,
    /// 本轮已创建的插件数量；至少允许创建一个，保证每次调用都有进展。
    /// Plugins created in this pass; at least one is allowed so every call makes progress.
    created: usize,
    /// 因预算耗尽而推迟到下一次调用的乐器插件数量。
    /// Instrument plugins postponed to the next call because the budget ran out.
    pending: usize,
}

struct TrackEngine {
    id: TrackId,
    instrument_key: String,
    instrument: InstrumentRuntime,
    effect_keys: Vec<String>,
    effects: Vec<EffectRuntime>,
    live: Vec<SynthEvent>,
    /// Cached from the last project sync: avoids scanning all clips per block.
    has_audio_clips: bool,
    /// 预计算的音符开/关事件，按 beat 排序；实时回调只推进游标。
    /// Precomputed note on/off events sorted by beat; the audio callback only advances a cursor.
    events: Vec<render::NoteEvent>,
    /// 影响音符事件的轨道签名；未变化时跳过重建。
    /// Track signature affecting note events; unchanged signatures skip rebuilding.
    event_signature: u64,
    /// 下一个待处理事件的下标。
    /// Index of the next event to process.
    event_cursor: usize,
    /// 是否有可能仍在发声的 MIDI 音符；静音时用于决定是否需要 panic。
    /// Whether MIDI notes may still be sounding; used to decide when muting needs a panic.
    notes_active: bool,
    level: f32,
    level_l: f32,
    level_r: f32,
    last_sustain: Option<bool>,
    last_bend: Option<f32>,
    last_param_poll: Instant,
    last_full_param_poll: Instant,
    /// 本条链（乐器 + 效果器）上报的总延迟，同步时重新计算。
    /// Total latency reported by this chain (instrument + effects), recomputed on sync.
    chain_latency: usize,
    /// 延迟补偿：把本轨补到全工程最慢的一条链。
    /// Delay compensation: pads this track to the slowest chain in the project.
    comp_delay: CompDelay,
}

enum InstrumentRuntime {
    Synth(PolySynth),
    Drums(DrumKit),
    Native(ModeledInstrument),
    Sample(SampledInstrument),
    Plugin(PluginRuntime),
    Failed(String),
}

impl InstrumentRuntime {
    /// 所属模块：audio::engine / 乐器运行时
    /// 功能：返回插件乐器上报的处理延迟（采样数）；内置乐器为 0。
    /// 输入：`self`。
    /// 输出/副作用：延迟采样数；插件忙时返回上一次缓存值。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / instrument runtime
    /// Purpose: Reported latency of a plugin instrument in samples; `0` for built-in instruments.
    /// Inputs: `self`.
    /// Outputs / side effects: latency in samples; the last cached value while the plugin is busy.
    /// Complexity: O(1) time and space.
    fn latency_samples(&mut self) -> usize {
        match self {
            InstrumentRuntime::Plugin(plugin) => plugin.latency_samples(),
            _ => 0,
        }
    }
}

/// 效果链运行时：VST 插件或内置原生效果器。
/// Effect-chain runtime: either a VST plugin or a built-in native effect.
enum EffectRuntime {
    Plugin(PluginRuntime),
    Native(NativeEffectProcessor),
}

impl EffectRuntime {
    /// 所属模块：audio::engine / 效果链运行时
    /// 功能：关闭该效果器的挂音。
    /// 输入：`self`。
    /// 输出/副作用：VST 插件接收 panic；内置效果无挂音。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / effect runtime
    /// Purpose: Release hanging notes of this effect.
    /// Inputs: `self`.
    /// Outputs / side effects: VST plugins receive panic; native effects have no held notes.
    /// Complexity: O(1) time and space.
    fn panic(&mut self) {
        if let EffectRuntime::Plugin(plugin) = self {
            plugin.panic();
        }
    }

    /// 所属模块：audio::engine / 效果链运行时
    /// 功能：以 VST 插件形式访问（内置效果返回 `None`）。
    /// 输入：`self`。
    /// 输出/副作用：插件运行时；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / effect runtime
    /// Purpose: Access as a VST plugin runtime (`None` for native effects).
    /// Inputs: `self`.
    /// Outputs / side effects: plugin runtime; no state change.
    /// Complexity: O(1) time and space.
    fn plugin(&self) -> Option<&PluginRuntime> {
        match self {
            EffectRuntime::Plugin(plugin) => Some(plugin),
            EffectRuntime::Native(_) => None,
        }
    }

    /// 所属模块：audio::engine / 效果链运行时
    /// 功能：以可变 VST 插件形式访问。
    /// 输入：`self`。
    /// 输出/副作用：可变插件运行时；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / effect runtime
    /// Purpose: Access mutably as a VST plugin runtime.
    /// Inputs: `self`.
    /// Outputs / side effects: mutable plugin runtime; no state change.
    /// Complexity: O(1) time and space.
    fn plugin_mut(&mut self) -> Option<&mut PluginRuntime> {
        match self {
            EffectRuntime::Plugin(plugin) => Some(plugin),
            EffectRuntime::Native(_) => None,
        }
    }

    /// 所属模块：audio::engine / 效果链运行时
    /// 功能：返回插件上报的处理延迟（采样数）；内置效果为 0。
    /// 输入：`self`。
    /// 输出/副作用：延迟采样数；插件忙时返回上一次缓存值。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / effect runtime
    /// Purpose: Reported processing latency in samples; `0` for native effects.
    /// Inputs: `self`.
    /// Outputs / side effects: latency in samples; the last cached value while the plugin is busy.
    /// Complexity: O(1) time and space.
    fn latency_samples(&mut self) -> usize {
        match self {
            EffectRuntime::Plugin(plugin) => plugin.latency_samples(),
            EffectRuntime::Native(_) => 0,
        }
    }

    /// 所属模块：audio::engine / 效果链运行时
    /// 功能：处理一个 block 的效果音频。
    /// 输入：
    /// - `sample_rate` / `frames`：采样率与本块帧数。
    /// - `input`：立体声输入。
    /// - `gates`：MIDI 事件（内置效果忽略）。
    /// - `out_l` / `out_r`：输出缓冲。
    /// 输出/副作用：写入输出；内置效果无 MIDI 输入。
    /// 复杂度：时间 O(帧数)，空间 O(1)。
    ///
    /// Module: audio::engine / effect runtime
    /// Purpose: Process one block of audio through the effect.
    /// Inputs:
    /// - `sample_rate` / `frames`: sample rate and frames in this block.
    /// - `input`: stereo input.
    /// - `gates`: MIDI events (native effects ignore them).
    /// - `out_l` / `out_r`: output buffers.
    /// Outputs / side effects: writes the output; native effects take no MIDI.
    /// Complexity: O(frames) time and O(1) space.
    #[allow(clippy::too_many_arguments)]
    fn process(
        &mut self,
        sample_rate: f64,
        frames: usize,
        input: Option<(&[f32], &[f32])>,
        gates: &[SynthEvent],
        out_l: &mut [f32],
        out_r: &mut [f32],
    ) {
        match self {
            EffectRuntime::Plugin(plugin) => {
                plugin.process(sample_rate, frames, input, gates, out_l, out_r, None)
            }
            EffectRuntime::Native(native) => match input {
                Some((left, right)) => native.process(frames, left, right, out_l, out_r),
                None => {
                    out_l
                        .iter_mut()
                        .take(frames)
                        .for_each(|sample| *sample = 0.0);
                    out_r
                        .iter_mut()
                        .take(frames)
                        .for_each(|sample| *sample = 0.0);
                }
            },
        }
    }
}

/// 轨道延迟补偿：按插件上报的延迟给轨道补一段延迟，使所有轨道对齐到最慢的一条。
/// 没有补偿时，开启 Pro-MB（192 kHz 下 3840 采样 ≈ 20 ms）或 Pro-DS（2880 采样 ≈ 15 ms）
/// 会让该轨道整体晚于其他轨道，听感就是“开启效果器后声音延迟”。
/// Track delay compensation: pads a track by the latency its plugins report so every track aligns
/// to the slowest chain. Without it, enabling Pro-MB (3840 samples ≈ 20 ms at 192 kHz) or Pro-DS
/// (2880 samples ≈ 15 ms) shifts that track behind the others, heard as "enabling an effect adds
/// latency".
pub(super) struct CompDelay {
    left: Vec<f32>,
    right: Vec<f32>,
    /// 环形缓冲写指针。
    /// Ring-buffer write cursor.
    write: usize,
    /// 需要补偿的采样数；`0` 表示直通。
    /// Samples to delay; `0` bypasses the line.
    delay: usize,
    /// 缓冲内是否还有未播放的旧内容（用于静音时一次性清零）。
    /// Whether the line still holds unplayed audio (so muting can clear it once).
    dirty: bool,
}

impl CompDelay {
    fn new() -> Self {
        Self {
            left: Vec::new(),
            right: Vec::new(),
            write: 0,
            delay: 0,
            dirty: false,
        }
    }

    /// 所属模块：audio::engine / 延迟补偿
    /// 功能：设置补偿采样数；长度变化时才重新分配（只在同步线程调用）。
    /// 输入：`samples`：需要补偿的采样数，`0` 表示直通。
    /// 输出/副作用：可能重新分配缓冲区并清空；不修改其他轨道状态。
    /// 复杂度：时间 O(采样数)（仅长度变化时），空间 O(采样数)。
    ///
    /// Module: audio::engine / delay compensation
    /// Purpose: Set the compensation in samples; reallocates only when the length changes (sync
    /// thread only).
    /// Inputs: `samples`: samples to delay, `0` bypasses.
    /// Outputs / side effects: may reallocate and clear the buffers; nothing else changes.
    /// Complexity: O(samples) time only when the length changes, O(samples) space.
    fn set(&mut self, samples: usize) {
        if samples == 0 {
            self.left.clear();
            self.right.clear();
            self.write = 0;
            self.delay = 0;
            self.dirty = false;
            return;
        }
        let len = samples + 1;
        if self.left.len() != len {
            self.left.clear();
            self.left.resize(len, 0.0);
            self.right.clear();
            self.right.resize(len, 0.0);
            self.write = 0;
            self.dirty = false;
        }
        self.delay = samples;
    }

    /// 所属模块：audio::engine / 延迟补偿
    /// 功能：是否启用补偿。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：O(1)。
    ///
    /// Module: audio::engine / delay compensation
    /// Purpose: Whether compensation is active.
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1).
    fn is_active(&self) -> bool {
        self.delay > 0 && self.left.len() > self.delay
    }

    /// 所属模块：audio::engine / 延迟补偿
    /// 功能：丢弃缓冲内残留的音频（暂停、定位、静音时调用）。
    /// 输入：`self`。
    /// 输出/副作用：清零左右缓冲并复位写指针；不改变补偿长度。
    /// 复杂度：时间 O(采样数)（仅在确实有残留时）。
    ///
    /// Module: audio::engine / delay compensation
    /// Purpose: Drop audio left in the line (called on pause, seek and mute).
    /// Inputs: `self`.
    /// Outputs / side effects: clears both buffers and resets the write cursor; the delay length is
    /// unchanged.
    /// Complexity: O(samples) time only when something is still buffered.
    fn reset(&mut self) {
        if self.dirty {
            self.left.fill(0.0);
            self.right.fill(0.0);
            self.write = 0;
            self.dirty = false;
        }
    }

    /// 所属模块：audio::engine / 延迟补偿
    /// 功能：把一对声道延迟固定采样数。
    /// 输入：`left` / `right`：本块音频。
    /// 输出/副作用：原地改写为延迟后的音频；推进写指针。
    /// 复杂度：时间 O(帧数)，空间 O(1)。
    ///
    /// Module: audio::engine / delay compensation
    /// Purpose: Delay a stereo pair by a fixed number of samples.
    /// Inputs: `left` / `right`: this block's audio.
    /// Outputs / side effects: rewrites the block in place with the delayed audio and advances the
    /// write cursor.
    /// Complexity: O(frames) time and O(1) space.
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.is_active() {
            return;
        }
        let len = self.left.len();
        let delay = self.delay;
        let mut write = self.write;
        self.dirty = true;
        for index in 0..left.len() {
            let read = if write >= delay {
                write - delay
            } else {
                write + len - delay
            };
            let out_l = self.left[read];
            let out_r = self.right[read];
            self.left[write] = left[index];
            self.right[write] = right[index];
            left[index] = out_l;
            right[index] = out_r;
            write += 1;
            if write == len {
                write = 0;
            }
        }
        self.write = write;
    }
}

/// 所属模块：audio::engine / ARA
/// 功能：由轨道片段计算 ARA 播放区域的工程时间范围（秒）。
/// 输入：`track` 轨道、`bpm` 当前速度。
/// 输出/副作用：返回（起始秒、时长秒）；没有片段时用 0..4 拍。
/// 复杂度：时间 O(片段数)，空间 O(1)。
///
/// Module: audio::engine / ARA
/// Purpose: Compute the ARA playback region's project time range (seconds) from a track's clips.
/// Inputs: `track`, `bpm`.
/// Outputs / side effects: (start seconds, duration seconds); 0..4 beats when the track is empty.
/// Complexity: O(clips) time and O(1) space.
fn ara_span_seconds(track: &Track, bpm: f64) -> (f64, f64) {
    let mut start = f64::INFINITY;
    let mut end = f64::NEG_INFINITY;
    for clip in &track.clips {
        start = start.min(clip.start);
        end = end.max(clip.end());
    }
    if !start.is_finite() || end <= start {
        start = 0.0;
        end = 4.0;
    }
    let start = start.max(0.0);
    let seconds_per_beat = if bpm.is_finite() && bpm > 0.0 {
        60.0 / bpm
    } else {
        0.5
    };
    (
        start * seconds_per_beat,
        ((end - start) * seconds_per_beat).max(0.5),
    )
}

/// 所属模块：audio::engine / ARA
/// 功能：把轨道时间线与速度同步给插件效果器的 ARA 文档。
/// 输入：`runtime` 插件运行时、`track` 轨道、`bpm` 速度、`time_sig` 拍号。
/// 输出/副作用：只在值变化时调用插件；线程不符等错误被忽略（效果器可能在另一线程创建）。
/// 复杂度：时间 O(片段数)（仅在变化时），空间 O(1)。
///
/// Module: audio::engine / ARA
/// Purpose: Push the track timeline and tempo into a plug-in effect's ARA document.
/// Inputs: `runtime`, `track`, `bpm`, `time_sig`.
/// Outputs / side effects: calls the plug-in only when values changed; errors such as a thread
/// mismatch are ignored (an effect may have been created on another thread).
/// Complexity: O(clips) time (only when changed) and O(1) space.
fn sync_ara_timeline(runtime: &mut PluginRuntime, track: &Track, bpm: f64, time_sig: (u8, u8)) {
    let (start, length) = ara_span_seconds(track, bpm);
    let _ = runtime.set_ara_tempo(bpm, time_sig);
    let _ = runtime.set_ara_timeline(start, length);
    // 在控制器线程上按需导出归档：插件报告内容变化后才真正调用，避免每帧做一次大拷贝。
    // Export the archive on the controller thread when needed: the plug-in's content-change
    // counter gates the actual call so this is not a large copy every frame.
    let _ = runtime.refresh_ara_archive();
}

impl TrackEngine {
    fn new(id: TrackId) -> Self {
        Self {
            id,
            instrument_key: String::new(),
            instrument: InstrumentRuntime::Failed("未加载".into()),
            effect_keys: Vec::new(),
            effects: Vec::new(),
            live: Vec::new(),
            has_audio_clips: false,
            events: Vec::new(),
            event_signature: 0,
            event_cursor: 0,
            notes_active: false,
            level: 0.0,
            level_l: 0.0,
            level_r: 0.0,
            last_sustain: None,
            last_bend: None,
            last_param_poll: Instant::now(),
            last_full_param_poll: Instant::now(),
            chain_latency: 0,
            comp_delay: CompDelay::new(),
        }
    }

    /// 所属模块：audio::engine / 轨道运行时
    /// 功能：按乐器身份键增量维护乐器运行时；可延后插件乐器，并可按时间片推迟剩余插件。
    /// 输入：
    /// - `track`：轨道快照。
    /// - `sample_rate` / `block_size` / `bpm` / `time_sig`：插件实例参数。
    /// - `force_reload`：为真时丢弃旧实例（采样率变化）。
    /// - `defer_plugin_instruments`：为真时不在本线程创建 VST 乐器实例。
    /// - `budget`：本轮乐器插件实例化预算；时间片用尽后剩余插件推迟到下一次调用。
    /// - `warnings`：插件加载失败信息。
    /// 输出/副作用：更新 `instrument_key` 与 `instrument`；内置乐器只同步参数。
    /// 复杂度：时间 O(1 + 插件加载)，空间 O(1)。
    ///
    /// Module: audio::engine / track runtime
    /// Purpose: Incrementally maintain the instrument runtime by identity key; optionally defer
    /// plugin instruments, and postpone the rest when the time slice is exhausted.
    /// Inputs:
    /// - `track`: track snapshot.
    /// - `sample_rate` / `block_size` / `bpm` / `time_sig`: plugin instance parameters.
    /// - `force_reload`: drop the old instance when true (sample-rate change).
    /// - `defer_plugin_instruments`: when true, VST instrument instances are not created here.
    /// - `budget`: instrument instantiation budget for this pass; remaining plugins are postponed to
    ///   the next call once the slice is exhausted.
    /// - `warnings`: plugin load failures.
    /// Outputs / side effects: updates `instrument_key` and `instrument`; native instruments only
    /// sync their settings.
    /// Complexity: O(1 + plugin load) time and O(1) space.
    #[allow(clippy::too_many_arguments)]
    fn ensure_instrument(
        &mut self,
        track: &Track,
        sample_rate: f64,
        block_size: usize,
        force_reload: bool,
        defer_plugin_instruments: bool,
        budget: &mut InstrumentLoadBudget,
        bpm: f64,
        time_sig: (u8, u8),
        warnings: &mut Vec<String>,
        gpu: Option<&mut crate::audio::gpu::GpuTimbreTrainer>,
    ) {
        let desired = track.instrument.key();
        if desired != self.instrument_key || force_reload {
            // 后台同步线程不创建乐器插件实例：VOCALOID 等插件会在实例化线程创建
            // WPF 窗口，随后在另一个线程重建/析构时可能崩溃。KEY 保持为空，
            // 由 UI 线程完成加载后再建立实例。
            // The background sync thread never instantiates instrument plugins: plugins such as
            // VOCALOID create WPF windows on the instantiating thread, and rebuilding or destroying
            // them from another thread can crash. Keep the key empty so the UI thread loads them.
            if defer_plugin_instruments && matches!(track.instrument, InstrumentRef::Plugin(_)) {
                self.instrument_key.clear();
                self.instrument = InstrumentRuntime::Failed("乐器插件将在界面线程加载".into());
                return;
            }
            // 界面线程补载乐器插件时按时间片分批：本轮已创建过插件且时间片用尽，就把剩余
            // 轨道留到下一帧，避免一次同步让窗口长时间无响应。
            // UI-thread instrument loading is sliced by time: once a plugin has been created in
            // this pass and the slice is exhausted, remaining tracks wait for the next frame so the
            // window never blocks for long.
            let is_plugin_instrument = matches!(track.instrument, InstrumentRef::Plugin(_));
            if is_plugin_instrument {
                if let Some(deadline) = budget.deadline {
                    if budget.created > 0 && Instant::now() >= deadline {
                        self.instrument_key.clear();
                        self.instrument = InstrumentRuntime::Failed("乐器插件正在加载".into());
                        budget.pending += 1;
                        return;
                    }
                }
            }
            self.instrument_key = desired;
            self.instrument = match &track.instrument {
                InstrumentRef::Synth(settings) => {
                    InstrumentRuntime::Synth(PolySynth::new(sample_rate as f32, settings.clone()))
                }
                InstrumentRef::Drums(settings) => {
                    InstrumentRuntime::Drums(DrumKit::new(sample_rate as f32, settings.clone()))
                }
                InstrumentRef::Native(native) => InstrumentRuntime::Native(ModeledInstrument::new(
                    native.kind,
                    native.settings.clone(),
                    sample_rate as f32,
                )),
                InstrumentRef::Sample(sample) => match sample.entry() {
                    Some(entry) => InstrumentRuntime::Sample(SampledInstrument::new(
                        entry,
                        sample.settings.clone(),
                        sample_rate as f32,
                        gpu,
                    )),
                    None => {
                        let message = format!("未找到音色：{}", sample.bank);
                        warnings.push(format!("{}: {}", track.name, message));
                        InstrumentRuntime::Failed(message)
                    }
                },
                InstrumentRef::Plugin(pref) => {
                    // 逐插件记录加载开始/结束：插件在加载中崩溃时，最后一条 begin 就是现场。
                    // Log each instrument load: when a plugin crashes while loading, the last begin
                    // record identifies it.
                    let path_text = pref.path.display().to_string();
                    let span = crate::diag::Span::start_logged(
                        "app.engine",
                        "engine.instrument.load",
                        &[
                            ("name", &pref.name),
                            ("path", &path_text),
                            ("track", &track.name),
                        ],
                    );
                    match PluginHost::load_plugin(
                        pref.format,
                        &pref.path,
                        &pref.uid,
                        sample_rate,
                        block_size,
                        bpm,
                        time_sig,
                    ) {
                        Ok(mut runtime) => {
                            // 导入的未验证状态只保留数据，不自动恢复，避免插件崩溃；
                            // 此时用导入的程序号尽量还原预设选择。空状态同样回退到程序号，
                            // 否则“先选工厂预设、再打开编辑器”会让插件重置为默认音色。
                            // Unverified imported state is preserved but not applied; the imported
                            // program number is used to restore the preset selection instead. Empty
                            // state also falls back to the program so opening the editor after
                            // choosing a factory preset cannot reset the plug-in to its default.
                            let (state_bytes, ara_archive) =
                                crate::plugin::split_plugin_state(&pref.state);
                            if pref.restore_state && !state_bytes.is_empty() {
                                runtime.restore_state(state_bytes, &pref.params);
                            } else if let Some(program) = pref.program {
                                runtime.select_program(program);
                            }
                            if !ara_archive.is_empty() {
                                if let Err(error) = runtime.restore_ara_archive(ara_archive) {
                                    warnings.push(format!("{}: {}", track.name, error));
                                }
                            }
                            // ARA 插件的播放区域与速度跟随工程内容。
                            // ARA plug-ins follow the project's timeline and tempo.
                            let (ara_start, ara_length) = ara_span_seconds(track, bpm);
                            if let Err(error) = runtime.set_ara_timeline(ara_start, ara_length) {
                                warnings.push(format!("{}: {}", track.name, error));
                            }
                            let _ = runtime.set_ara_tempo(bpm, time_sig);
                            let _ = runtime.refresh_ara_archive();
                            span.end(&[("status", "ok")]);
                            InstrumentRuntime::Plugin(runtime)
                        }
                        Err(error) => {
                            span.end(&[("status", "error"), ("error", &error)]);
                            warnings.push(format!("{}: {}", track.name, error));
                            InstrumentRuntime::Failed(error)
                        }
                    }
                }
            };
            // 成功或失败都算一次实例化尝试，计入本轮时间片预算。
            // An attempt counts against this pass's slice whether it succeeded or failed.
            if is_plugin_instrument {
                budget.created += 1;
            }
        } else {
            match (&mut self.instrument, &track.instrument) {
                (InstrumentRuntime::Synth(synth), InstrumentRef::Synth(settings)) => {
                    synth.set_settings(settings.clone());
                }
                (InstrumentRuntime::Drums(drums), InstrumentRef::Drums(settings)) => {
                    drums.set_settings(settings.clone());
                }
                (InstrumentRuntime::Native(native), InstrumentRef::Native(instrument)) => {
                    native.set_settings(instrument.settings.clone());
                }
                (InstrumentRuntime::Sample(sample), InstrumentRef::Sample(reference)) => {
                    sample.set_settings(reference.settings.clone());
                }
                _ => {}
            }
            // 已加载的插件乐器：每帧同步 ARA 时间线/速度，内部只在变化时调用插件。
            // Already-loaded plugin instruments: sync the ARA timeline/tempo each pass; the
            // runtime only calls the plug-in when something actually changed.
            if let (InstrumentRuntime::Plugin(runtime), InstrumentRef::Plugin(_)) =
                (&mut self.instrument, &track.instrument)
            {
                let (ara_start, ara_length) = ara_span_seconds(track, bpm);
                let _ = runtime.set_ara_tempo(bpm, time_sig);
                if let Err(error) = runtime.set_ara_timeline(ara_start, ara_length) {
                    warnings.push(format!("{}: {}", track.name, error));
                }
            }
        }
    }

    /// 所属模块：audio::engine / 轨道运行时
    /// 功能：按效果器身份键增量维护插入效果链，支持 VST 与内置效果的混排。
    /// 输入：
    /// - `track`：轨道快照。
    /// - `sample_rate` / `block_size` / `bpm` / `time_sig`：插件实例参数。
    /// - `force_reload`：为真时丢弃全部旧实例。
    /// - `warnings`：插件加载失败信息。
    /// 输出/副作用：更新 `effect_keys` 与 `effects`；内置效果同步参数但不重建实例。
    /// 复杂度：时间 O(效果器数 + 新增插件加载)，空间 O(效果器数)。
    ///
    /// Module: audio::engine / track runtime
    /// Purpose: Incrementally maintain the insert chain by effect identity key, mixing VST and
    /// native effects.
    /// Inputs:
    /// - `track`: track snapshot.
    /// - `sample_rate` / `block_size` / `bpm` / `time_sig`: plugin instance parameters.
    /// - `force_reload`: drop every existing instance when true.
    /// - `warnings`: plugin load failures.
    /// Outputs / side effects: updates `effect_keys` and `effects`; native effects only sync
    /// parameters and keep their DSP state.
    /// Complexity: O(effect count + new plugin loads) time and O(effect count) space.
    fn ensure_effects(
        &mut self,
        track: &Track,
        sample_rate: f64,
        block_size: usize,
        force_reload: bool,
        bpm: f64,
        time_sig: (u8, u8),
        warnings: &mut Vec<String>,
    ) {
        let desired: Vec<String> = track.effects.iter().map(EffectRef::key).collect();
        if desired == self.effect_keys && !force_reload {
            // 参数变化不会改变身份键；内置效果需每帧同步参数，插件效果同步 ARA 时间线。
            // Parameter edits do not change the identity key: native effects sync settings every
            // pass and plug-in effects sync their ARA timeline.
            for (runtime, effect) in self.effects.iter_mut().zip(track.effects.iter()) {
                match (runtime, effect) {
                    (EffectRuntime::Native(native), EffectRef::Native(reference)) => {
                        native.set_settings(reference.settings.clone());
                    }
                    (EffectRuntime::Plugin(plugin), EffectRef::Plugin(_)) => {
                        sync_ara_timeline(plugin, track, bpm, time_sig);
                    }
                    _ => {}
                }
            }
            return;
        }
        let previous_keys = std::mem::replace(&mut self.effect_keys, desired.clone());
        let mut previous: Vec<Option<EffectRuntime>> = std::mem::take(&mut self.effects)
            .into_iter()
            .map(Some)
            .collect();
        let mut effects = Vec::with_capacity(track.effects.len());
        for (index, effect) in track.effects.iter().enumerate() {
            // 按效果器键复用已有运行时：新增/删除一个效果器时只加载新增的那个，
            // 不再重建整条链（重建 5 个插件会让界面卡住数秒）。
            // Reuse runtimes by key: adding or removing one effect must load only the
            // new one instead of rebuilding the whole chain (seconds of frozen UI).
            let reused = if force_reload {
                None
            } else {
                previous_keys
                    .iter()
                    .position(|key| *key == desired[index])
                    .and_then(|old| previous.get_mut(old).and_then(Option::take))
            };
            let runtime = reused.unwrap_or_else(|| match effect {
                EffectRef::Native(native) => EffectRuntime::Native(NativeEffectProcessor::new(
                    native.kind,
                    native.settings.clone(),
                    sample_rate as f32,
                )),
                EffectRef::Plugin(pref) => {
                    match PluginHost::load_plugin(
                        pref.format,
                        &pref.path,
                        &pref.uid,
                        sample_rate,
                        block_size,
                        bpm,
                        time_sig,
                    ) {
                        Ok(mut runtime) => {
                            let (state_bytes, ara_archive) =
                                crate::plugin::split_plugin_state(&pref.state);
                            if pref.restore_state && !state_bytes.is_empty() {
                                runtime.restore_state(state_bytes, &pref.params);
                            } else if let Some(program) = pref.program {
                                runtime.select_program(program);
                            }
                            if !ara_archive.is_empty() {
                                let _ = runtime.restore_ara_archive(ara_archive);
                            }
                            EffectRuntime::Plugin(runtime)
                        }
                        Err(error) => {
                            warnings.push(format!("{} 效果器: {}", track.name, error));
                            EffectRuntime::Plugin(PluginRuntime::Failed(error))
                        }
                    }
                }
            });
            effects.push(runtime);
        }
        self.effects = effects;
        for (runtime, effect) in self.effects.iter_mut().zip(track.effects.iter()) {
            match (runtime, effect) {
                (EffectRuntime::Native(native), EffectRef::Native(reference)) => {
                    native.set_settings(reference.settings.clone());
                }
                (EffectRuntime::Plugin(plugin), EffectRef::Plugin(_)) => {
                    sync_ara_timeline(plugin, track, bpm, time_sig);
                }
                _ => {}
            }
        }
    }

    fn panic(&mut self) {
        match &mut self.instrument {
            InstrumentRuntime::Synth(synth) => synth.all_notes_off(),
            InstrumentRuntime::Drums(drums) => drums.all_notes_off(),
            InstrumentRuntime::Native(native) => native.all_notes_off(),
            InstrumentRuntime::Sample(sample) => sample.all_notes_off(),
            InstrumentRuntime::Plugin(plugin) => plugin.panic(),
            InstrumentRuntime::Failed(_) => {}
        }
        for effect in &mut self.effects {
            effect.panic();
        }
        // 暂停/定位时补偿延迟里还留着上一段的音频，必须丢弃，否则恢复播放会先冒出旧声音。
        // Audio left in the compensation line must be dropped on pause/seek, otherwise resuming
        // replays the previous position.
        self.comp_delay.reset();
        self.live.clear();
        self.notes_active = false;
    }

    fn plugin(&self) -> Option<&PluginRuntime> {
        match &self.instrument {
            InstrumentRuntime::Plugin(plugin) => Some(plugin),
            _ => None,
        }
    }

    fn plugin_mut(&mut self) -> Option<&mut PluginRuntime> {
        match &mut self.instrument {
            InstrumentRuntime::Plugin(plugin) => Some(plugin),
            _ => None,
        }
    }

    fn poll_parameters(&mut self, playing: bool) -> bool {
        // Polling every VST2 parameter every frame is expensive; 5 Hz is
        // plenty to notice GUI edits and refresh the displayed values.
        if self.last_param_poll.elapsed() < std::time::Duration::from_millis(200) {
            return false;
        }
        self.last_param_poll = Instant::now();
        // The cheap gesture log can miss plugins that do not report edits; do a
        // full value comparison once per second as a fallback. While the
        // transport runs only the gesture log is used: a plugin whose values
        // drift internally must not trigger a full parameter refresh.
        let full_scan =
            !playing && self.last_full_param_poll.elapsed() >= std::time::Duration::from_secs(1);
        if full_scan {
            self.last_full_param_poll = Instant::now();
        }
        let mut changed = false;
        if let Some(plugin) = self.plugin_mut() {
            changed |= plugin.poll_parameters_changed(full_scan);
        }
        for effect in &mut self.effects {
            if let EffectRuntime::Plugin(plugin) = effect {
                changed |= plugin.poll_parameters_changed(full_scan);
            }
        }
        changed
    }
}

impl Engine {
    /// 所属模块：audio::engine / 构造
    /// 功能：创建实时渲染引擎。
    /// 输入：`sample_rate`：输出采样率。
    /// 输出/副作用：返回空工程引擎；自动停止默认启用。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / construction
    /// Purpose: Create the realtime rendering engine.
    /// Inputs: `sample_rate`: output sample rate.
    /// Outputs / side effects: engine with an empty project; auto-stop is enabled by default.
    /// Complexity: O(1) time and space.
    pub fn new(sample_rate: f64) -> Self {
        Self::build(sample_rate)
    }

    /// 所属模块：audio::engine / 构造
    /// 功能：初始化引擎字段。
    /// 输入：`sample_rate`：输出采样率。
    /// 输出/副作用：返回引擎；不启动线程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / construction
    /// Purpose: Initialise engine fields.
    /// Inputs: `sample_rate`: output sample rate.
    /// Outputs / side effects: returns the engine; no threads are started.
    /// Complexity: O(1) time and space.
    fn build(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            block_size: render_block_size(sample_rate),
            project: Project::default(),
            tracks: Vec::new(),
            master_effect: None,
            master_effect_key: None,
            plugins_sample_rate: 0.0,
            last_tempo: 0.0,
            last_max_latency: 0,
            last_master_latency: 0,
            last_parameter_poll: Instant::now(),
            parameter_poll_cursor: 0,
            playing: false,
            position_beats: 0.0,
            render_ahead_samples: 0,
            preroll_frames: 0,
            preroll_position_beats: 0.0,
            lookahead_enabled: true,
            levels: Vec::new(),
            levels_l: Vec::new(),
            levels_r: Vec::new(),
            master_peak: 0.0,
            master_peak_l: 0.0,
            master_peak_r: 0.0,
            cpu_load: 0.0,
            live_timeout: 0.0,
            auto_stop_enabled: true,
            auto_stopped: false,
            any_solo: false,
            auto_stop_at: None,
            audio_files: HashMap::new(),
            audio_loading: HashSet::new(),
            audio_requests: Vec::new(),
            audio_warnings: Vec::new(),
            audio_cursors: HashMap::new(),
            audio_scratch_l: Vec::new(),
            audio_scratch_r: Vec::new(),
            wet_scratch_l: Vec::new(),
            wet_scratch_r: Vec::new(),
            master_wet_scratch_l: Vec::new(),
            master_wet_scratch_r: Vec::new(),
            master_channels: vec![Vec::new(), Vec::new()],
            stereo_scratch: vec![Vec::new(), Vec::new()],
            gpu_interleave_scratch: Vec::new(),
            gpu_realtime: None,
            gpu_timbre: None,
            gpu_realtime_config: None,
            gpu_realtime_warning: None,
            plugin_transport_playing: false,
            plugin_transport_seek: true,
            gates_scratch: Vec::new(),
            output_layout: ChannelLayout::Stereo,
            defer_master_clip: false,
            offline: None,
        }
    }

    /// 所属模块：audio::engine / 采样率
    /// 功能：更新采样率；变化时强制下次同步重载插件。
    /// 输入：`sample_rate`：新采样率。
    /// 输出/副作用：更新 `sample_rate` 与 `plugins_sample_rate`；不重建工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / sample rate
    /// Purpose: Update the sample rate; a change forces plugin reload on the next sync.
    /// Inputs: `sample_rate`: new sample rate.
    /// Outputs / side effects: updates `sample_rate` and `plugins_sample_rate`; no project rebuild.
    /// Complexity: O(1) time and space.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        if (self.sample_rate - sample_rate).abs() > 1.0 {
            self.sample_rate = sample_rate;
            self.block_size = render_block_size(sample_rate);
            self.plugins_sample_rate = 0.0;
        }
    }

    /// 所属模块：audio::engine / 插件生命周期
    /// 功能：关闭工程前终止所有 VST3 插件组件，让 VOCALOID / ACE Bridge 等插件关闭它们拉起的外部应用
    /// 窗口；实例本身被泄漏而不是析构，避免在 UI 线程等待插件消息循环。
    /// 输入：`self`。
    /// 输出/副作用：停止处理、关闭编辑器视图并调用 `IComponent::terminate`；引擎之后不可再用于渲染。
    /// 复杂度：时间 O(插件数量 + 终止耗时)，空间 O(1)。
    ///
    /// Module: audio::engine / plugin lifecycle
    /// Purpose: Terminate every VST3 component before a project closes, letting plugins such as
    /// VOCALOID / ACE Bridge close the external app windows they spawned; instances are leaked
    /// instead of dropped so the UI thread never waits for a plugin message loop.
    /// Inputs: `self`.
    /// Outputs / side effects: stops processing, closes editor views and calls `IComponent::terminate`;
    /// the engine must not render afterwards.
    /// Complexity: O(plugin count + termination) time and O(1) space.
    pub fn shutdown_plugins(&mut self) {
        self.pause();
        for track in &mut self.tracks {
            let track_label = track.id.to_string();
            if let InstrumentRuntime::Plugin(plugin) = &mut track.instrument {
                Self::shutdown_plugin_logged(plugin, &track_label, "instrument");
            }
            // 清空身份键：已终止的实例只剩 `Failed` 外壳，下一次同步必须重建，
            // 否则重新打开同一工程时插件不会重新加载。
            // Clear the identity keys: a terminated instance is only a `Failed` shell, so the next
            // sync must rebuild it; otherwise reopening the same project would never reload it.
            track.instrument_key.clear();
            for (index, effect) in track.effects.iter_mut().enumerate() {
                if let EffectRuntime::Plugin(plugin) = effect {
                    let slot = index.to_string();
                    Self::shutdown_plugin_logged(plugin, &track_label, &slot);
                }
            }
            track.effect_keys.clear();
        }
        if let Some(EffectRuntime::Plugin(plugin)) = &mut self.master_effect {
            Self::shutdown_plugin_logged(plugin, "MASTER", "master");
        }
        self.master_effect_key = None;
    }

    /// 所属模块：audio::engine / 插件生命周期
    /// 功能：记录单个插件终止的开始/结束，插件在 terminate 中崩溃时最后一条 begin 即现场。
    /// 输入：`plugin`：插件运行时；`track` / `slot`：所属轨道与槽位。
    /// 输出/副作用：调用 `PluginRuntime::shutdown` 并写 `engine.plugin.shutdown` 日志。
    /// 复杂度：时间 O(1 + 插件终止)。
    ///
    /// Module: audio::engine / plugin lifecycle
    /// Purpose: Log one plugin termination; if it crashes, the last begin record identifies it.
    /// Inputs: `plugin`; `track` / `slot`: owning track and slot.
    /// Outputs / side effects: calls `PluginRuntime::shutdown` and writes
    /// `engine.plugin.shutdown`.
    /// Complexity: O(1 + termination) time.
    fn shutdown_plugin_logged(plugin: &mut PluginRuntime, track: &str, slot: &str) {
        let name = plugin.display_name().to_string();
        let span = crate::diag::Span::start_logged(
            "app.engine",
            "engine.plugin.shutdown",
            &[("name", &name), ("track", track), ("slot", slot)],
        );
        plugin.shutdown();
        span.end(&[("status", "ok")]);
    }

    /// 所属模块：audio::engine / GPU 实时渲染
    /// 功能：按设置启用或关闭实时主总线 GPU 软限幅；配置未变化时不重建设备。
    /// 输入：`config`：GPU 设备/驱动/API 选择；`None` 表示关闭并回到 CPU。
    /// 输出/副作用：可能创建 GPU 设备并把失败原因存入 `gpu_realtime_warning`；
    /// 成功后实时回调只在 GPU 已算完时使用其结果，否则本块回退 CPU，绝不等待。
    /// 复杂度：时间 O(GPU 设备创建)（配置变化时），空间 O(1)。
    ///
    /// Module: audio::engine / GPU realtime rendering
    /// Purpose: Enable or disable the realtime master-bus GPU soft limiter from the settings; an
    /// unchanged config never rebuilds the device.
    /// Inputs: `config`: GPU device/driver/API selection; `None` disables and returns to the CPU.
    /// Outputs / side effects: may create a GPU device and store a failure reason in
    /// `gpu_realtime_warning`; on success the callback uses the GPU result only when it is already
    /// available and otherwise limits that block on the CPU without waiting.
    /// Complexity: O(GPU device creation) time when the config changes, O(1) space.
    pub fn configure_gpu_realtime(&mut self, config: Option<GpuRenderConfig>) {
        // 配置相同即返回：失败后不会在每次同步时反复创建设备；用户切换设置可触发重试。
        // Return on an unchanged config: a failure never rebuilds the device on every sync; toggling
        // the setting retries.
        if config == self.gpu_realtime_config {
            return;
        }
        self.gpu_realtime = None;
        self.gpu_timbre = None;
        self.gpu_realtime_config = config.clone();
        let Some(config) = config else {
            return;
        };
        // 神经音色训练器与实时限幅共用配置：打不开就静默退回 CPU 训练，不影响播放。
        // The timbre trainer shares the config: when it cannot open, training silently stays on the
        // CPU and playback is unaffected.
        let (hidden_1, hidden_2) = crate::audio::synth::HIDDEN_WIDTHS;
        self.gpu_timbre = crate::audio::gpu::GpuRuntime::standard()
            .open_timbre_trainer(&config, hidden_1, hidden_2)
            .ok();
        match crate::audio::gpu::GpuRealtimeLimiter::open(&config) {
            Ok(limiter) => {
                self.gpu_realtime = Some(limiter);
                // 预先分配最大块（8 声道 × 4096 帧）的交错缓冲，避免首次回调内扩容。
                // Preallocate the largest block (8 channels × 4096 frames) so the first callback never
                // grows the buffer.
                self.gpu_interleave_scratch = vec![0.0; 8 * 4096];
                self.gpu_realtime_warning = None;
            }
            Err(error) => {
                self.gpu_realtime_warning =
                    Some(format!("实时 GPU 加速不可用，已回退 CPU：{error}"));
            }
        }
    }

    /// 所属模块：audio::engine / GPU 实时渲染
    /// 功能：返回实时 GPU 限幅是否处于可用状态。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / GPU realtime rendering
    /// Purpose: Return whether the realtime GPU limiter is available.
    /// Inputs: `self`.
    /// Outputs / side effects: a boolean; no state change.
    /// Complexity: O(1) time and space.
    pub fn gpu_realtime_active(&self) -> bool {
        self.gpu_realtime.is_some()
    }

    /// 所属模块：audio::engine / GPU 实时渲染
    /// 功能：取走最近一次实时 GPU 失败原因（取后清空，避免状态栏重复提示）。
    /// 输入：`self`。
    /// 输出/副作用：`Option<String>`；清空内部字段。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / GPU realtime rendering
    /// Purpose: Take the last realtime GPU failure (cleared afterwards so the status bar does not
    /// repeat it).
    /// Inputs: `self`.
    /// Outputs / side effects: `Option<String>`; clears the field.
    /// Complexity: O(1) time and space.
    pub fn take_gpu_realtime_warning(&mut self) -> Option<String> {
        self.gpu_realtime_warning.take()
    }

    /// 所属模块：audio::engine / 工程同步
    /// 功能：把工程快照同步到引擎，重建轨道运行时、插件链与音符事件。
    /// 输入：`project`：工程快照。
    /// 输出/副作用：更新工程、轨道运行时、音频解码请求与音符事件；返回插件/音频告警。
    /// 复杂度：时间 O(轨道数 + 片段数 + 音符数 log 音符数 + 插件加载)，空间 O(工程大小)。
    ///
    /// Module: audio::engine / project sync
    /// Purpose: Sync a project snapshot into the engine, rebuilding track runtimes,
    /// plugin chains and note events.
    /// Inputs: `project`: project snapshot.
    /// Outputs / side effects: updates the project, track runtimes, audio decode requests
    /// and note events; returns plugin/audio warnings.
    /// Complexity: O(tracks + clips + notes log notes + plugin load) time and O(project) space.
    pub fn sync_project(&mut self, project: &Project) -> Vec<String> {
        self.sync_project_with(project, false)
    }

    /// 所属模块：audio::engine / 同步
    /// 功能：同步工程；可选择延后乐器插件实例化，交由 UI 线程完成。
    /// 输入：
    /// - `project`：工程快照。
    /// - `defer_plugin_instruments`：为真时不在本线程创建 VST 乐器实例。
    /// 输出/副作用：重建轨道运行时、效果链、主效果器与音符事件；返回加载警告。
    /// 复杂度：时间 O(轨道数 + 片段数 + 音符数 log 音符数 + 新增插件加载)，空间 O(工程大小)。
    ///
    /// Module: audio::engine / sync
    /// Purpose: Sync the project, optionally deferring VST instrument instantiation to the UI thread.
    /// Inputs:
    /// - `project`: project snapshot.
    /// - `defer_plugin_instruments`: when true, VST instrument instances are not created here.
    /// Outputs / side effects: rebuilds track runtimes, effect chains, the master effect and note
    /// events; returns load warnings.
    /// Complexity: O(tracks + clips + notes log notes + new plugin loads) time and O(project) space.
    pub fn sync_project_with(
        &mut self,
        project: &Project,
        defer_plugin_instruments: bool,
    ) -> Vec<String> {
        self.sync_project_budgeted(project, defer_plugin_instruments, None)
            .0
    }

    /// 所属模块：audio::engine / 同步
    /// 功能：按时间片补载乐器插件：一次调用最多实例化时间片内的插件，其余推迟到下一次。
    /// 输入：
    /// - `project`：工程快照。
    /// - `budget`：本轮允许创建插件的时长；至少会创建第一个插件，保证每次调用都有进展。
    /// 输出/副作用：更新轨道运行时、效果链与音符事件，并取走音频解码请求。
    /// 复杂度：时间 O(轨道数 + 片段数 + 音符数 log 音符数 + 新增插件加载)，空间 O(工程大小)。
    ///
    /// Module: audio::engine / sync
    /// Purpose: Load instrument plugins in time slices: one call instantiates at most the plugins
    /// that fit in the slice and postpones the rest to the next call.
    /// Inputs:
    /// - `project`: project snapshot.
    /// - `budget`: how long this pass may spend creating plugins; the first plugin is always
    ///   created so every call makes progress.
    /// Outputs / side effects: rebuilds track runtimes, effect chains and note events, and takes
    /// the pending audio decode requests.
    /// Complexity: O(tracks + clips + notes log notes + new plugin loads) time and O(project) space.
    pub fn sync_project_slice(
        &mut self,
        project: &Project,
        budget: Duration,
    ) -> (Vec<String>, Vec<PathBuf>, usize) {
        let (warnings, pending) =
            self.sync_project_budgeted(project, false, Some(Instant::now() + budget));
        let requests = self.take_audio_requests();
        (warnings, requests, pending)
    }

    /// 所属模块：audio::engine / 同步
    /// 功能：同步工程；可选延后乐器插件实例化，并可按时间片限制本轮新建的插件数量。
    /// 输入：
    /// - `project`：工程快照。
    /// - `defer_plugin_instruments`：为真时不在本线程创建 VST 乐器实例。
    /// - `instrument_deadline`：本轮创建插件的截止时刻；`None` 表示不限时。
    /// 输出/副作用：重建轨道运行时、效果链、主效果器与音符事件；返回（加载警告、推迟的乐器数）。
    /// 复杂度：时间 O(轨道数 + 片段数 + 音符数 log 音符数 + 新增插件加载)，空间 O(工程大小)。
    ///
    /// Module: audio::engine / sync
    /// Purpose: Sync the project, optionally deferring VST instrument instantiation to the UI
    /// thread, and optionally limiting how many new plugins this pass may create.
    /// Inputs:
    /// - `project`: project snapshot.
    /// - `defer_plugin_instruments`: when true, VST instrument instances are not created here.
    /// - `instrument_deadline`: deadline for creating plugins in this pass; `None` means unlimited.
    /// Outputs / side effects: rebuilds track runtimes, effect chains, the master effect and note
    /// events; returns (load warnings, postponed instrument count).
    /// Complexity: O(tracks + clips + notes log notes + new plugin loads) time and O(project) space.
    fn sync_project_budgeted(
        &mut self,
        project: &Project,
        defer_plugin_instruments: bool,
        instrument_deadline: Option<Instant>,
    ) -> (Vec<String>, usize) {
        let mut warnings = Vec::new();
        let mut budget = InstrumentLoadBudget {
            deadline: instrument_deadline,
            created: 0,
            pending: 0,
        };
        let force_reload = (self.plugins_sample_rate - self.sample_rate).abs() > 1.0;
        if force_reload {
            self.plugins_sample_rate = self.sample_rate;
        }
        let tempo_changed = (self.last_tempo - project.bpm).abs() > 1e-6;
        self.last_tempo = project.bpm;

        self.project = project.clone();
        // 布局变化时按新声道数扩展主总线缓冲；不重建已有内容，下一块会自动清零复用。
        // A layout change resizes the master buffers; existing contents are simply cleared next block.
        self.output_layout = project.output_layout;
        self.master_channels
            .resize_with(project.output_layout.channels(), Vec::new);
        self.any_solo = project.any_solo();
        let auto_stop = project.auto_stop_beat();
        self.auto_stop_at = (auto_stop > 0.0).then_some(auto_stop);
        self.levels.resize(project.tracks.len(), 0.0);
        self.levels_l.resize(project.tracks.len(), 0.0);
        self.levels_r.resize(project.tracks.len(), 0.0);
        let time_sig = (project.time_sig_num, project.time_sig_den);

        let mut old_tracks = std::mem::take(&mut self.tracks);
        let mut needed_audio: HashSet<PathBuf> = HashSet::new();
        let mut alive_audio_clips: HashSet<(TrackId, u64)> = HashSet::new();
        for track in &project.tracks {
            let mut runtime = old_tracks
                .iter()
                .position(|t| t.id == track.id)
                .map(|index| old_tracks.remove(index))
                .unwrap_or_else(|| TrackEngine::new(track.id));
            runtime.ensure_instrument(
                track,
                self.sample_rate,
                self.block_size,
                force_reload,
                defer_plugin_instruments,
                &mut budget,
                project.bpm,
                time_sig,
                &mut warnings,
                self.gpu_timbre.as_mut(),
            );
            runtime.ensure_effects(
                track,
                self.sample_rate,
                self.block_size,
                force_reload,
                project.bpm,
                time_sig,
                &mut warnings,
            );
            runtime.has_audio_clips = track.has_audio_clips();
            // 插件上报的处理延迟：乐器加全部“启用中”的效果器（停用的效果器被旁路，不产生延迟）。
            // Reported processing latency: the instrument plus every enabled effect (bypassed
            // effects add no latency).
            let mut chain_latency = runtime.instrument.latency_samples();
            for (index, effect) in runtime.effects.iter_mut().enumerate() {
                if track
                    .effects
                    .get(index)
                    .is_some_and(|reference| reference.enabled())
                {
                    chain_latency = chain_latency.saturating_add(effect.latency_samples());
                }
            }
            runtime.chain_latency = chain_latency;
            // 只有片段内容/时间变化时才重建事件表，控制器或插件编辑不会重复排序。
            // Rebuild the event table only when clip content/timing changes; controller and
            // plugin edits do not re-sort notes.
            runtime.refresh_events(track, self.position_beats);
            for clip in &track.clips {
                if let Some(source) = &clip.audio {
                    needed_audio.insert(source.path.clone());
                    alive_audio_clips.insert((track.id, clip.id));
                }
            }
            self.tracks.push(runtime);
        }
        // Playback relies on `self.tracks[i]` matching `project.tracks[i]`.
        debug_assert_eq!(self.tracks.len(), project.tracks.len());

        let desired_master = project.master_effect.as_ref().map(EffectRef::key);
        if desired_master != self.master_effect_key || force_reload {
            self.master_effect_key = desired_master;
            self.master_effect = project
                .master_effect
                .as_ref()
                .and_then(|effect| match effect {
                    EffectRef::Native(native) => {
                        Some(EffectRuntime::Native(NativeEffectProcessor::new(
                            native.kind,
                            native.settings.clone(),
                            self.sample_rate as f32,
                        )))
                    }
                    EffectRef::Plugin(pref) => match PluginHost::load_plugin(
                        pref.format,
                        &pref.path,
                        &pref.uid,
                        self.sample_rate,
                        self.block_size,
                        project.bpm,
                        time_sig,
                    ) {
                        Ok(mut runtime) => {
                            let (state_bytes, ara_archive) =
                                crate::plugin::split_plugin_state(&pref.state);
                            if pref.restore_state && !state_bytes.is_empty() {
                                runtime.restore_state(state_bytes, &pref.params);
                            } else if let Some(program) = pref.program {
                                runtime.select_program(program);
                            }
                            if !ara_archive.is_empty() {
                                let _ = runtime.restore_ara_archive(ara_archive);
                            }
                            Some(EffectRuntime::Plugin(runtime))
                        }
                        Err(error) => {
                            warnings.push(format!("主效果器: {}", error));
                            Some(EffectRuntime::Plugin(PluginRuntime::Failed(error)))
                        }
                    },
                });
        } else if let (Some(EffectRuntime::Native(native)), Some(EffectRef::Native(reference))) =
            (&mut self.master_effect, &project.master_effect)
        {
            native.set_settings(reference.settings.clone());
        }

        // 延迟补偿：把所有轨道对齐到最慢的一条链。没有这一步时，开启 Pro-MB（192 kHz 下
        // 3840 采样 ≈ 20 ms）或 Pro-DS（2880 采样 ≈ 15 ms）会让该轨整体晚于其他轨道。
        // 主效果器的延迟对所有轨道相同，无法靠轨道补偿消除，只记录到日志。
        // Delay compensation: align every track to the slowest chain. Without it, enabling Pro-MB
        // (3840 samples ≈ 20 ms at 192 kHz) or Pro-DS (2880 samples ≈ 15 ms) shifts that track
        // behind the rest. Master-effect latency applies to everything equally and cannot be
        // compensated per track, so it is only reported.
        let master_latency = self
            .master_effect
            .as_mut()
            .map(EffectRuntime::latency_samples)
            .unwrap_or(0);
        let max_latency = self.apply_latency_compensation(master_latency);
        let slowest = self
            .tracks
            .iter()
            .enumerate()
            .max_by_key(|(_, track)| track.chain_latency)
            .and_then(|(index, _)| project.tracks.get(index))
            .map(|track| track.name.clone())
            .unwrap_or_default();
        self.report_plugin_latency(max_latency, master_latency, &slowest);

        if tempo_changed {
            for track in &mut self.tracks {
                if let Some(plugin) = track.plugin_mut() {
                    plugin.set_tempo(project.bpm, time_sig);
                }
                for effect in &mut track.effects {
                    if let EffectRuntime::Plugin(plugin) = effect {
                        plugin.set_tempo(project.bpm, time_sig);
                    }
                }
            }
            if let Some(EffectRuntime::Plugin(plugin)) = &mut self.master_effect {
                plugin.set_tempo(project.bpm, time_sig);
            }
        }

        // Audio-clip sources are decoded on a background worker. The audio
        // callback simply outputs silence for a clip until its file arrives,
        // which keeps opening a large project off the UI thread.
        self.audio_files
            .retain(|path, _| needed_audio.contains(path));
        self.audio_cursors
            .retain(|key, _| alive_audio_clips.contains(key));
        self.audio_requests.clear();
        for path in needed_audio {
            if self.audio_files.contains_key(&path) || self.audio_loading.contains(&path) {
                continue;
            }
            self.audio_loading.insert(path.clone());
            self.audio_requests.push(path);
        }

        // 本轮新建过插件实例时补发一次位置：新实例的 ProcessContext 还没有工程定位。
        // Resend the position after this pass created plugin instances: their ProcessContext has no
        // project position yet.
        if budget.created > 0 {
            self.plugin_transport_seek = true;
        }
        (warnings, budget.pending)
    }

    /// 所属模块：audio::engine / 延迟补偿
    /// 功能：插件上报的延迟变化时写一条 `engine.latency` 记录，便于定位“开启效果器后变晚”。
    /// 输入：
    /// - `max_latency`：全工程最慢链的延迟（采样数）。
    /// - `master_latency`：主效果器的延迟（采样数）。
    /// - `slowest`：最慢轨道的名字。
    /// 输出/副作用：延迟变化时写日志；不修改引擎状态（只更新记录用的缓存）。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / delay compensation
    /// Purpose: Log one `engine.latency` record whenever the reported plugin latency changes, so
    /// "sounds late after enabling an effect" can be traced.
    /// Inputs:
    /// - `max_latency`: latency of the slowest chain in the project, in samples.
    /// - `master_latency`: master-effect latency in samples.
    /// - `slowest`: name of the slowest track.
    /// Outputs / side effects: writes a log record when the values change; no engine state changes.
    /// Complexity: O(1) time and space.
    fn report_plugin_latency(&mut self, max_latency: usize, master_latency: usize, slowest: &str) {
        if max_latency == self.last_max_latency && master_latency == self.last_master_latency {
            return;
        }
        self.last_max_latency = max_latency;
        self.last_master_latency = master_latency;
        if max_latency == 0 && master_latency == 0 {
            return;
        }
        let rate = self.sample_rate.max(1.0);
        let samples_text = max_latency.to_string();
        let ms_text = (max_latency as f64 / rate * 1_000.0).round().to_string();
        let master_ms_text = (master_latency as f64 / rate * 1_000.0).round().to_string();
        crate::diag::info(
            "app.engine",
            "engine.latency",
            &[
                ("comp_samples", &samples_text),
                ("comp_ms", &ms_text),
                ("master_ms", &master_ms_text),
                ("slowest", slowest),
            ],
        );
    }

    /// 所属模块：audio::engine / 延迟补偿
    /// 功能：按当前各轨链延迟重新计算补偿量与实时前瞻量。
    /// 输入：`master_latency`：主效果器延迟（采样数）。
    /// 输出/副作用：更新每轨 `comp_delay` 与 `render_ahead_samples`；播放中前瞻量变化会重新预卷。
    /// 返回全工程最慢链的延迟。
    /// 复杂度：时间 O(轨道数)，空间 O(1)。
    ///
    /// Module: audio::engine / delay compensation
    /// Purpose: Recompute the per-track compensation and the realtime look-ahead from the current
    /// chain latencies.
    /// Inputs: `master_latency`: master-effect latency in samples.
    /// Outputs / side effects: updates each track's `comp_delay` and `render_ahead_samples`;
    /// a change during playback re-runs the pre-roll. Returns the slowest chain latency.
    /// Complexity: O(tracks) time and O(1) space.
    fn apply_latency_compensation(&mut self, master_latency: usize) -> usize {
        let max_latency = self
            .tracks
            .iter()
            .map(|track| track.chain_latency)
            .max()
            .unwrap_or(0);
        for track in &mut self.tracks {
            let compensation = max_latency.saturating_sub(track.chain_latency);
            track.comp_delay.set(compensation);
        }
        // 前瞻量 = 最慢轨道链 + 主效果器延迟：实时渲染提前这么多，听感就和播放头对齐。
        // 主效果器的延迟对所有轨道相同，只能靠整体前瞻抵消。
        // Look-ahead = slowest track chain + master-effect latency: rendering this far ahead makes
        // the audible result line up with the playhead. Master latency affects every track the same
        // way, so only a global look-ahead can cancel it.
        let ahead = max_latency.saturating_add(master_latency);
        if ahead != self.render_ahead_samples {
            let grew = ahead > self.render_ahead_samples;
            self.render_ahead_samples = ahead;
            // 只有变长时才需要重新预卷（把更长的延迟线填满）；变短时旧内容本来就在线里，
            // 再预卷反而会额外丢掉一段声音，听感更碎。
            // Only a longer look-ahead needs a pre-roll (to fill the longer delay lines). When it
            // shrinks the existing lines already hold the content, and pre-rolling again would drop
            // another chunk of audio and sound worse.
            if self.playing && grew {
                self.begin_preroll();
            }
        }
        max_latency
    }

    /// 所属模块：audio::engine / 延迟补偿
    /// 功能：轮询插件提出的宿主请求（`restartComponent`），把延迟变化立刻并入补偿。
    /// 输入：`self`。
    /// 输出/副作用：刷新每轨链延迟；有变化时重新分配补偿缓冲并写 `engine.latency`。
    /// 只在 UI 线程调用（插件锁用 `try_lock`，不会阻塞音频线程）。
    /// 复杂度：时间 O(轨道数 × 插件数)，空间 O(1)。
    ///
    /// Module: audio::engine / delay compensation
    /// Purpose: Poll the host requests plugins raise through `restartComponent` and fold latency
    /// changes into the compensation immediately.
    /// Inputs: `self`.
    /// Outputs / side effects: refreshes each chain's latency, reallocates the compensation lines
    /// and logs `engine.latency` when something changed. UI thread only (plugin locks use
    /// `try_lock`, so the audio thread never blocks).
    /// Complexity: O(tracks × plugins) time and O(1) space.
    pub fn refresh_plugin_latency(&mut self) {
        let mut changed = false;
        for (index, runtime) in self.tracks.iter_mut().enumerate() {
            let Some(track) = self.project.tracks.get(index) else {
                continue;
            };
            let mut latency = 0usize;
            if let InstrumentRuntime::Plugin(plugin) = &mut runtime.instrument {
                changed |= plugin.refresh_host_requests();
                latency = latency.saturating_add(plugin.latency_samples());
            }
            for (slot, effect) in runtime.effects.iter_mut().enumerate() {
                if !track
                    .effects
                    .get(slot)
                    .is_some_and(|reference| reference.enabled())
                {
                    continue;
                }
                if let EffectRuntime::Plugin(plugin) = effect {
                    changed |= plugin.refresh_host_requests();
                    latency = latency.saturating_add(plugin.latency_samples());
                }
            }
            if latency != runtime.chain_latency {
                runtime.chain_latency = latency;
                changed = true;
            }
        }
        if let Some(EffectRuntime::Plugin(plugin)) = &mut self.master_effect {
            changed |= plugin.refresh_host_requests();
        }
        if !changed {
            return;
        }
        let master_latency = self
            .master_effect
            .as_mut()
            .map(EffectRuntime::latency_samples)
            .unwrap_or(0);
        let max_latency = self.apply_latency_compensation(master_latency);
        let slowest = self
            .tracks
            .iter()
            .enumerate()
            .max_by_key(|(_, track)| track.chain_latency)
            .and_then(|(index, _)| self.project.tracks.get(index))
            .map(|track| track.name.clone())
            .unwrap_or_default();
        self.report_plugin_latency(max_latency, master_latency, &slowest);
    }

    /// 所属模块：audio::engine / 传输
    /// 功能：返回是否正在播放。
    /// 输入：`self`。
    /// 输出/副作用：播放状态；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / transport
    /// Purpose: Whether the transport is playing.
    /// Inputs: `self`.
    /// Outputs / side effects: playing state; no state change.
    /// Complexity: O(1) time and space.
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// 所属模块：audio::engine / 传输
    /// 功能：返回当前播放位置。
    /// 输入：`self`。
    /// 输出/副作用：播放位置，单位 beat；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / transport
    /// Purpose: Current playhead position.
    /// Inputs: `self`.
    /// Outputs / side effects: playhead in beats; no state change.
    /// Complexity: O(1) time and space.
    pub fn position_beats(&self) -> f64 {
        if self.playing && self.lookahead_enabled {
            if self.preroll_frames > 0 {
                // 预卷期间内部游标已经提前，显示值仍停在起播位置。
                // While pre-rolling the internal cursor is already ahead; keep showing the start.
                return self.preroll_position_beats;
            }
            // 内部游标（内容位置）比实际听到的声音早一个插件链延迟，显示值取听感位置。
            // The internal cursor (content position) runs one chain latency ahead of what is heard,
            // so the playhead reports the audible position.
            return (self.position_beats - self.render_ahead_beats()).max(0.0);
        }
        self.position_beats
    }

    /// 所属模块：audio::engine / 传输
    /// 功能：把插件链延迟（采样）换算成拍数。
    /// 输入：`self`。
    /// 输出/副作用：返回拍数；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / transport
    /// Purpose: Convert the plugin-chain latency in samples to beats.
    /// Inputs: `self`.
    /// Outputs / side effects: the beat count; no state change.
    /// Complexity: O(1) time and space.
    fn render_ahead_beats(&self) -> f64 {
        self.render_ahead_samples as f64 * self.project.bpm.max(1.0) / (60.0 * self.sample_rate)
    }

    /// 所属模块：audio::engine / 传输
    /// 功能：安排一次预卷：先把补偿延迟线填满，再开始输出。
    /// 输入：`self`。
    /// 输出/副作用：设置预卷帧数与显示位置；关闭前瞻时立即结束预卷。
    /// 复杂度：时间 O(轨道数)，空间 O(1)。
    ///
    /// Module: audio::engine / transport
    /// Purpose: Schedule a pre-roll so the compensation lines are filled before output starts.
    /// Inputs: `self`.
    /// Outputs / side effects: sets the pre-roll length and displayed position; a disabled
    /// look-ahead ends the pre-roll immediately.
    /// Complexity: O(tracks) time and O(1) space.
    fn begin_preroll(&mut self) {
        if !self.lookahead_enabled || self.render_ahead_samples == 0 {
            self.preroll_frames = 0;
            return;
        }
        self.preroll_frames = self.render_ahead_samples;
        self.preroll_position_beats = self.position_beats;
        for track in &mut self.tracks {
            track.comp_delay.reset();
        }
    }

    /// 所属模块：audio::engine / 传输
    /// 功能：开始播放，并从当前位置重建音符事件游标。
    /// 输入：`self`。
    /// 输出/副作用：`playing=true`；关闭全部音符、清空音频游标。
    /// 复杂度：时间 O(轨道数 + log 事件数)，空间 O(1)。
    ///
    /// Module: audio::engine / transport
    /// Purpose: Start playback and re-seek every track's note event cursor.
    /// Inputs: `self`.
    /// Outputs / side effects: `playing=true`; releases all notes and clears audio cursors.
    /// Complexity: O(tracks + log events) time and O(1) space.
    pub fn play(&mut self) {
        if !self.playing {
            self.playing = true;
            self.live_timeout = 0.0;
            self.auto_stopped = false;
            self.audio_cursors.clear();
            self.plugin_transport_seek = true;
            self.reset_note_state(self.position_beats);
            self.begin_preroll();
        }
    }

    /// 所属模块：audio::engine / 传输
    /// 功能：暂停播放并关闭全部音符。
    /// 输入：`self`。
    /// 输出/副作用：`playing=false`；关闭全部音符。
    /// 复杂度：时间 O(轨道数 + log 事件数)，空间 O(1)。
    ///
    /// Module: audio::engine / transport
    /// Purpose: Pause playback and release every note.
    /// Inputs: `self`.
    /// Outputs / side effects: `playing=false`; all notes released.
    /// Complexity: O(tracks + log events) time and O(1) space.
    pub fn pause(&mut self) {
        if self.playing {
            self.playing = false;
            // 前瞻时内部游标比听感位置早一个链延迟，暂停要把游标退回听感位置，
            // 否则播放头会突然向前跳一段，再次播放还会从更晚的地方开始。
            // With look-ahead the internal cursor is one chain latency ahead of what was heard;
            // pull it back on pause so the playhead does not jump and playback resumes from the
            // position the user actually heard.
            if self.lookahead_enabled && self.preroll_frames == 0 {
                self.position_beats = (self.position_beats - self.render_ahead_beats()).max(0.0);
            }
            self.preroll_frames = 0;
            self.reset_note_state(self.position_beats);
        }
    }

    /// 所属模块：audio::engine / 传输
    /// 功能：定位播放头，重建事件游标与音频源游标。
    /// 输入：`beat`：目标位置（会被收敛到非负）。
    /// 输出/副作用：更新 `position_beats`；关闭全部音符、清空音频游标。
    /// 复杂度：时间 O(轨道数 + log 事件数)，空间 O(1)。
    ///
    /// Module: audio::engine / transport
    /// Purpose: Seek the playhead and rebuild event and audio source cursors.
    /// Inputs: `beat`: target position (clamped to non-negative).
    /// Outputs / side effects: updates `position_beats`; releases all notes and clears audio cursors.
    /// Complexity: O(tracks + log events) time and O(1) space.
    pub fn seek(&mut self, beat: f64) {
        self.position_beats = beat.max(0.0);
        self.plugin_transport_seek = true;
        self.audio_cursors.clear();
        self.auto_stopped = false;
        self.reset_note_state(self.position_beats);
        if self.playing {
            // 定位后补偿延迟线里是旧位置的音频，重新预卷一次，避免定位后先冒出旧声音。
            // After a seek the compensation lines still hold the old position; pre-roll again so
            // stale audio cannot leak out.
            self.begin_preroll();
        }
    }

    /// 所属模块：audio::engine / 传输
    /// 功能：在工程被替换后重置播放状态（新建、加载、撤销/重做）。
    /// 输入：`self`。
    /// 输出/副作用：关闭全部音符、清空音频游标，并强制下次同步重建音符事件表。
    /// 复杂度：时间 O(轨道数 + log 事件数)，空间 O(1)。
    ///
    /// Module: audio::engine / transport
    /// Purpose: Reset playback state after the project is replaced (new, load, undo/redo).
    /// Inputs: `self`.
    /// Outputs / side effects: releases all notes, clears audio cursors and forces the next
    /// sync to rebuild every note event table; playhead unchanged.
    /// Complexity: O(tracks + log events) time and O(1) space.
    pub fn reset_playback_state(&mut self) {
        self.audio_cursors.clear();
        // 工程被替换时旧的前瞻预卷没有意义，直接取消，避免丢掉新工程的头部声音。
        // A pre-roll from the previous project is meaningless after a replacement; cancel it so the
        // new project never loses its first samples.
        self.preroll_frames = 0;
        self.reset_note_state(self.position_beats);
        // 工程内容可能被替换但 revision 未持久化；强制重建避免签名碰撞。
        // Project content can change while revisions are not persisted; force a rebuild
        // so identical signatures cannot reuse stale events.
        for track in &mut self.tracks {
            track.event_signature = u64::MAX;
        }
    }

    /// 所属模块：audio::engine / 传输
    /// 功能：关闭全部音符并把各轨事件游标定位到 `beat`。
    /// 输入：`beat`：目标位置，单位 beat。
    /// 输出/副作用：乐器/效果器挂音被关闭，`event_cursor` 更新。
    /// 复杂度：时间 O(轨道数 + log 事件数)，空间 O(1)。
    ///
    /// Module: audio::engine / transport
    /// Purpose: Release every note and position each track's event cursor at `beat`.
    /// Inputs: `beat`: target position in beats.
    /// Outputs / side effects: instruments/effects are released and `event_cursor` is updated.
    /// Complexity: O(tracks + log events) time and O(1) space.
    fn reset_note_state(&mut self, beat: f64) {
        for track in &mut self.tracks {
            track.panic();
            track.seek_events(beat);
        }
        if let Some(master) = &mut self.master_effect {
            master.panic();
        }
    }

    pub fn audition(&mut self, track_id: TrackId, pitch: u8, on: bool, velocity: u8) {
        let Some(track) = self.tracks.iter_mut().find(|t| t.id == track_id) else {
            return;
        };
        track.live.push(SynthEvent {
            frame: 0,
            kind: if on { GateKind::On } else { GateKind::Off },
            pitch,
            velocity,
        });
        if !self.playing {
            self.live_timeout = 8.0;
        }
    }

    /// Per-track left/right peak levels plus the master left/right peaks.
    pub fn stereo_levels(&self) -> (&[f32], &[f32], (f32, f32)) {
        (
            &self.levels_l,
            &self.levels_r,
            (self.master_peak_l, self.master_peak_r),
        )
    }

    /// Coarse peak envelope of a loaded audio clip, for arrangement drawing.
    pub fn audio_waveform(&self, path: &Path) -> Option<Vec<f32>> {
        self.audio_files.get(path).map(|file| file.peaks.clone())
    }

    /// Hand the pending audio decode requests to the application worker.
    pub fn take_audio_requests(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.audio_requests)
    }

    /// Decode every pending audio source on the calling thread. Offline
    /// rendering/export needs the samples immediately, unlike the interactive
    /// project load path which uses [`Self::take_audio_requests`].
    pub fn load_pending_audio_blocking(&mut self) -> Vec<String> {
        let paths = self.take_audio_requests();
        let mut warnings = Vec::new();
        for path in paths {
            match AudioRuntime::import_audio(&path) {
                Ok(file) => self.install_audio_file(path, Arc::new(file)),
                Err(error) => {
                    self.fail_audio_file(path, error.clone());
                    warnings.push(error);
                }
            }
        }
        warnings
    }

    pub fn install_audio_file(&mut self, path: PathBuf, file: Arc<AudioFile>) {
        self.audio_loading.remove(&path);
        self.audio_files.insert(path, file);
    }

    pub fn fail_audio_file(&mut self, path: PathBuf, error: String) {
        self.audio_loading.remove(&path);
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        self.audio_warnings.push(format!("{name}: {error}"));
    }

    pub fn take_audio_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.audio_warnings)
    }

    pub fn is_loading_audio(&self) -> bool {
        !self.audio_loading.is_empty()
    }

    pub fn cpu_load(&self) -> f32 {
        self.cpu_load
    }

    /// 所属模块：audio::engine / 状态查询
    /// 功能：返回自动停止标记。
    /// 输入：`self`。
    /// 输出/副作用：是否已在内容末尾自动停止；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::engine / state query
    /// Purpose: Return the auto-stop flag.
    /// Inputs: `self`.
    /// Outputs / side effects: whether the transport auto-stopped; no state change.
    /// Complexity: O(1) time and space.
    pub fn auto_stopped(&self) -> bool {
        self.auto_stopped
    }

    /// 所属模块：audio::engine / 试听
    /// 功能：是否正在试听（传输停止时仍有键盘/MIDI 音符发声）。
    /// 输入：`self`。
    /// 输出/副作用：试听状态；不修改状态。
    /// 复杂度：时间 O(轨道数)，空间 O(1)。
    ///
    /// Module: audio::engine / audition
    /// Purpose: Whether audition is producing sound while the transport is stopped.
    /// Inputs: `self`.
    /// Outputs / side effects: audition state; no state change.
    /// Complexity: O(track count) time and O(1) space.
    pub fn is_auditioning(&self) -> bool {
        self.live_timeout > 0.0 || self.tracks.iter().any(|track| !track.live.is_empty())
    }

    pub fn plugin_kind(&self, track_id: TrackId, slot: SlotKind) -> Option<PluginKind> {
        self.runtime_for(track_id, slot).map(PluginRuntime::kind)
    }
}

/// Format a beat position as `bar.beat.tick` (bars are 0-based).
pub fn format_position(beats: f64, beats_per_bar: f64) -> String {
    let beats = beats.max(0.0);
    let bar = (beats / beats_per_bar).floor() as u64;
    let beat_in_bar = beats % beats_per_bar;
    let beat = beat_in_bar.floor() as u64 + 1;
    let tick = ((beat_in_bar.fract()) * 960.0).round() as u64;
    format!("{bar}.{beat}.{tick:03}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::Clip;

    /// 覆盖：补偿延迟把音频整体后移固定采样数，暂停/静音清空后不再输出旧音频。
    /// Covers: the compensation line shifts audio by a fixed number of samples and drops leftovers
    /// once reset.
    #[test]
    fn compensation_delay_shifts_audio_and_resets() {
        let mut delay = CompDelay::new();
        delay.set(3);
        let mut left = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let mut right = left.clone();
        delay.process(&mut left, &mut right);
        assert_eq!(&left[..3], &[0.0, 0.0, 0.0]);
        assert_eq!(&left[3..], &[1.0, 2.0, 3.0]);
        assert_eq!(left, right, "both channels must be delayed identically");

        let mut next = vec![7.0, 8.0, 9.0];
        let mut next_r = next.clone();
        delay.process(&mut next, &mut next_r);
        assert_eq!(next, vec![4.0, 5.0, 6.0]);

        delay.reset();
        let mut after = vec![1.0, 1.0, 1.0];
        let mut after_r = after.clone();
        delay.process(&mut after, &mut after_r);
        assert_eq!(after, vec![0.0, 0.0, 0.0]);
    }

    /// 覆盖：补偿为 0 时缓冲直通，不改变音频。
    /// Covers: zero compensation bypasses the line and leaves the audio untouched.
    #[test]
    fn compensation_delay_with_zero_latency_is_transparent() {
        let mut delay = CompDelay::new();
        delay.set(0);
        let mut left = vec![0.25, -0.5, 0.75];
        let mut right = vec![-0.25, 0.5, -0.75];
        let expected = left.clone();
        delay.process(&mut left, &mut right);
        assert_eq!(left, expected);
        assert!(!delay.is_active());
    }

    /// 覆盖：内部块长按采样率放大到约 10 ms，低采样率不低于 512 帧。
    /// Covers: the internal block scales to roughly 10 ms and never drops below 512 frames.
    #[test]
    fn render_block_size_tracks_the_sample_rate() {
        assert_eq!(render_block_size(44_100.0), 512);
        assert_eq!(render_block_size(48_000.0), 512);
        assert_eq!(render_block_size(96_000.0), 960);
        assert_eq!(render_block_size(192_000.0), 1920);
        // 极端采样率也不会越界。
        assert_eq!(render_block_size(8_000.0), 512);
        assert_eq!(render_block_size(768_000.0), 4096);
    }

    /// 覆盖：Demo 工程可离线渲染出可听音频。
    /// Covers: the demo project renders audible audio offline.
    #[test]
    fn demo_project_renders_audio() {
        let project = Project::demo();
        let samples = Engine::render_offline(&project, 48_000.0).expect("offline render");
        assert!(!samples.is_empty());
        let peak = samples
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(peak > 0.05, "expected audible output, peak was {peak}");
    }

    /// 覆盖：原生乐器与内置效果器可以经由引擎渲染链产生输出。
    /// Covers: a native instrument and native effect render through the engine chain.
    #[test]
    fn native_instrument_and_effect_render_through_the_engine() {
        let mut project = Project::default();
        project.bpm = 120.0;
        let mut track = Track::new(
            1,
            "Native",
            [0, 0, 0],
            InstrumentRef::Native(crate::core::model::NativeInstrument::new(
                crate::core::model::NativeInstrumentKind::Piano,
            )),
        );
        track
            .effects
            .push(EffectRef::Native(crate::core::model::NativeEffect::new(
                crate::core::model::NativeEffectKind::Reverb,
            )));
        let mut clip = Clip::new(1, "clip", 0.0, 2.0);
        clip.notes.push(crate::core::model::Note {
            start: 0.0,
            length: 1.0,
            pitch: 60,
            velocity: 100,
        });
        track.clips.push(clip);
        project.tracks.push(track);

        let samples = Engine::render_offline(&project, 48_000.0).expect("offline render");
        let peak = samples
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(
            peak > 0.01,
            "native chain should produce audio, peak={peak}"
        );
    }

    /// 覆盖：采样音色库乐器可经由引擎渲染链产生输出。
    /// Covers: a sampled-bank instrument renders through the engine chain.
    #[test]
    fn sampled_instrument_renders_through_the_engine() {
        let mut project = Project::default();
        project.bpm = 120.0;
        let mut track = Track::new(
            1,
            "Sampled",
            [0, 0, 0],
            InstrumentRef::Sample(crate::core::model::SampleInstrument::new("GrandPiano")),
        );
        let mut clip = Clip::new(1, "clip", 0.0, 2.0);
        clip.notes.push(crate::core::model::Note {
            start: 0.0,
            length: 1.0,
            pitch: 60,
            velocity: 100,
        });
        track.clips.push(clip);
        project.tracks.push(track);

        let samples = Engine::render_offline(&project, 48_000.0).expect("offline render");
        let peak = samples
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(
            peak > 0.01,
            "sampled instrument should produce audio, peak={peak}"
        );
    }

    /// 覆盖：时间片同步能补齐原生乐器，并报告没有待补载的插件乐器。
    /// Covers: a sliced sync loads native instruments and reports no pending plugin instrument.
    #[test]
    fn sync_project_slice_loads_native_instruments_without_pending_plugins() {
        let mut project = Project::default();
        let track = Track::new(
            1,
            "Native",
            [0, 0, 0],
            InstrumentRef::Native(crate::core::model::NativeInstrument::new(
                crate::core::model::NativeInstrumentKind::Piano,
            )),
        );
        project.tracks.push(track);

        let mut engine = Engine::new(48_000.0);
        let (warnings, _requests, pending) =
            engine.sync_project_slice(&project, std::time::Duration::from_millis(1));
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        assert_eq!(pending, 0, "native instruments need no further slices");
        assert!(
            matches!(engine.tracks[0].instrument, InstrumentRuntime::Native(_)),
            "the native instrument should be instantiated by the sliced sync"
        );
    }

    /// 覆盖：块内音符起止能在实时渲染中触发。
    /// Covers: notes starting and stopping inside a block fire in the live renderer.
    #[test]
    fn notes_start_and_stop_within_a_block() {
        let mut project = Project::default();
        let mut track = Track::new(
            1,
            "Test",
            [0, 255, 0],
            InstrumentRef::Synth(crate::core::model::SynthSettings::default()),
        );
        let mut clip = Clip::new(1, "clip", 0.0, 2.0);
        clip.notes.push(crate::core::model::Note {
            start: 0.25,
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
        for _ in 0..48 {
            engine.process(&mut left, &mut right);
            audible |= left.iter().any(|sample| sample.abs() > 1e-4);
        }
        assert!(audible, "note starting at beat 0.25 should be audible");
        assert!(engine.position_beats() > 0.0);
    }

    /// 覆盖：启用前瞻时先做一次预卷（输出静音、播放头停在起点），之后播放头取听感位置。
    /// Covers: with look-ahead the engine pre-rolls first (silent output, playhead held at the
    /// start) and then reports the audible position.
    #[test]
    fn lookahead_prerolls_then_reports_audible_position() {
        let mut project = Project::default();
        let mut track = Track::new(
            1,
            "Test",
            [0, 255, 0],
            InstrumentRef::Synth(crate::core::model::SynthSettings::default()),
        );
        let mut clip = Clip::new(1, "clip", 0.0, 2.0);
        clip.notes.push(crate::core::model::Note {
            start: 0.0,
            length: 1.0,
            pitch: 60,
            velocity: 100,
        });
        track.clips.push(clip);
        project.tracks.push(track);

        let mut engine = Engine::new(48_000.0);
        engine.sync_project(&project);
        // 人为给 4800 采样（0.1 s）的前瞻量，等价于 192 kHz 下 Pro-MB + Pro-DS 的链延迟。
        // Pretend the chain latency is 4800 samples (0.1 s), the Pro-MB + Pro-DS total at 192 kHz.
        engine.render_ahead_samples = 4_800;
        engine.play();

        let mut left = vec![0.0_f32; 512];
        let mut right = vec![0.0_f32; 512];
        // 7 块 = 3584 帧 < 4800：仍在预卷中，输出必须静音，播放头停在 0。
        // 7 blocks = 3584 frames < 4800: still pre-rolling, so the output is silent and the
        // playhead stays at 0.
        for _ in 0..7 {
            engine.process(&mut left, &mut right);
            assert!(
                left.iter().all(|sample| *sample == 0.0),
                "pre-roll must not emit audio"
            );
            assert_eq!(engine.position_beats(), 0.0);
        }

        // 再推进到 13 块（6656 帧）：预卷已结束，声音开始输出，显示位置 = 内部游标 - 前瞻量。
        // Advance to 13 blocks (6656 frames): the pre-roll is over, audio starts and the reported
        // position is the internal cursor minus the look-ahead.
        let mut audible = false;
        for _ in 0..6 {
            engine.process(&mut left, &mut right);
            audible |= left.iter().any(|sample| sample.abs() > 1e-4);
        }
        assert!(audible, "audio must start once the chains are primed");
        let beats_per_frame = project.bpm.max(1.0) / 60.0 / 48_000.0;
        let expected = 13.0 * 512.0 * beats_per_frame - 4_800.0 * beats_per_frame;
        assert!(
            (engine.position_beats() - expected).abs() < 1e-9,
            "reported position should be {} but was {}",
            expected,
            engine.position_beats()
        );
    }

    /// 覆盖：插件延迟变化后重新计算补偿量与前瞻量（Pro-Q 4 切线性相位走这条路径）。
    /// Covers: a latency change recomputes the compensation and look-ahead (the path Pro-Q 4 takes
    /// when switching to linear phase).
    #[test]
    fn latency_change_recomputes_compensation_and_ahead() {
        let project = Project::demo();
        assert!(project.tracks.len() >= 2, "demo project needs two tracks");
        let mut engine = Engine::new(48_000.0);
        engine.sync_project(&project);
        assert_eq!(engine.render_ahead_samples, 0);

        // 人为把第一轨的链条延迟改成 4800 采样（Pro-Q 4 线性相位在 48 kHz 下的量级）。
        // Pretend the first track's chain reports 4800 samples (Pro-Q 4 linear phase at 48 kHz).
        engine.tracks[0].chain_latency = 4_800;
        let slowest = engine.apply_latency_compensation(0);

        assert_eq!(slowest, 4_800, "slowest chain should be the edited track");
        assert_eq!(engine.render_ahead_samples, 4_800);
        assert_eq!(
            engine.tracks[0].comp_delay.delay, 0,
            "the slowest track needs no extra compensation"
        );
        assert_eq!(
            engine.tracks[1].comp_delay.delay, 4_800,
            "other tracks must be padded to the slowest chain"
        );
    }

    /// 覆盖：传输位置格式化为 `bar.beat.tick`。
    /// Covers: transport positions format as `bar.beat.tick`.
    #[test]
    fn transport_position_formatting() {
        assert_eq!(format_position(0.0, 4.0), "0.1.000");
        assert_eq!(format_position(4.0, 4.0), "1.1.000");
        assert_eq!(format_position(5.5, 4.0), "1.2.480");
    }

    /// 覆盖：自动停止点为内容结束后两小节。
    /// Covers: auto-stop sits two bars after the content end.
    #[test]
    fn auto_stop_is_two_bars_after_content() {
        let project = Project::demo();
        let content_end = project.content_end_beat();
        assert_eq!(project.auto_stop_beat(), content_end + 8.0);
    }

    /// 覆盖：工程替换后即使片段签名碰撞，也会强制重建事件表。
    /// Covers: replacing a project forces an event rebuild even when signatures collide.
    #[test]
    fn reset_playback_state_forces_event_rebuild() {
        let mut project = Project::default();
        let mut track = Track::new(
            1,
            "Rebuild",
            [0, 0, 0],
            InstrumentRef::Synth(crate::core::model::SynthSettings::default()),
        );
        let mut clip = Clip::new(1, "clip", 0.0, 4.0);
        clip.notes.push(crate::core::model::Note {
            start: 0.0,
            length: 1.0,
            pitch: 60,
            velocity: 100,
        });
        track.clips.push(clip);
        project.tracks.push(track);

        let mut engine = Engine::new(48_000.0);
        engine.sync_project(&project);
        assert_eq!(engine.tracks[0].events.len(), 2);
        assert_eq!(engine.tracks[0].events[0].pitch, 60);

        // 模拟加载同 id/长度/音符数但音高不同的工程：revision 不持久化。
        // Simulate loading a project with identical ids/counts but different pitches;
        // revisions are not persisted, so the signature would collide.
        project.tracks[0].clips[0].notes[0].pitch = 72;
        engine.sync_project(&project);
        assert_eq!(engine.tracks[0].events[0].pitch, 60);

        engine.reset_playback_state();
        engine.sync_project(&project);
        assert_eq!(engine.tracks[0].events[0].pitch, 72);
    }

    /// 覆盖：同步为每个 MIDI 音符预计算开/关两条事件。
    /// Covers: sync precomputes one on and one off event per MIDI note.
    #[test]
    fn sync_precomputes_note_events() {
        let project = Project::demo();
        let mut engine = Engine::new(48_000.0);
        engine.sync_project(&project);

        for (track, runtime) in project.tracks.iter().zip(engine.tracks.iter()) {
            let expected = track
                .clips
                .iter()
                .filter(|clip| clip.audio.is_none())
                .map(|clip| clip.notes.len() * 2)
                .sum::<usize>();
            assert_eq!(
                runtime.events.len(),
                expected,
                "track {} should precompute two events per note",
                track.name
            );
        }
    }

    /// 覆盖：同音高轻微重叠的两个音符，后一个音符必须重新起音而不是被前一个 Note Off 掐断。
    /// Covers: a slightly overlapping same-pitch pair retriggers the later note instead of being
    /// silenced by the earlier Note Off.
    #[test]
    fn overlapping_same_pitch_notes_retrigger() {
        let mut project = Project::default();
        project.bpm = 120.0;
        let mut track = Track::new(
            1,
            "Overlap",
            [0, 255, 0],
            InstrumentRef::Synth(crate::core::model::SynthSettings::default()),
        );
        let mut clip = Clip::new(1, "clip", 0.0, 4.0);
        for start in [0.0, 0.99] {
            clip.notes.push(crate::core::model::Note {
                start,
                length: 1.0,
                pitch: 60,
                velocity: 100,
            });
        }
        track.clips.push(clip);
        project.tracks.push(track);

        let mut engine = Engine::new(48_000.0);
        engine.sync_project(&project);
        engine.play();
        let mut left = vec![0.0_f32; 512];
        let mut right = vec![0.0_f32; 512];
        let mut samples = Vec::new();
        for _ in 0..140 {
            left.fill(0.0);
            right.fill(0.0);
            engine.process(&mut left, &mut right);
            samples.extend_from_slice(&left);
        }
        // 120 BPM 下第二个音符在 0.99 beat ≈ 0.495 s 起音，取 0.50–0.60 s 的 RMS。
        // At 120 BPM the second note starts at 0.99 beat ≈ 0.495 s; measure 0.50–0.60 s RMS.
        let window = &samples[24_000..28_800];
        let rms =
            (window.iter().map(|sample| sample * sample).sum::<f32>() / window.len() as f32).sqrt();
        assert!(rms > 0.02, "the second note must be audible, rms={rms}");
    }
}
