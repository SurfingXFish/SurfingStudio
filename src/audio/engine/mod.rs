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
use crate::audio::synth::{DrumKit, GateKind, ModeledInstrument, PolySynth, SynthEvent};
use crate::core::model::{
    evaluate_automation, ChannelLayout, ControllerKind, ControllerLanes, EffectRef, InstrumentRef,
    Project, Track, TrackId,
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

pub struct Engine {
    pub sample_rate: f64,
    pub block_size: usize,
    project: Project,
    tracks: Vec<TrackEngine>,
    master_effect: Option<EffectRuntime>,
    master_effect_key: Option<String>,
    plugins_sample_rate: f64,
    last_tempo: f64,
    last_parameter_poll: Instant,
    /// 参数轮询游标：每次只检查一条轨道，避免一次扫描全部插件阻塞 UI。
    /// Parameter poll cursor: check one track per tick so scanning a project
    /// with many plugins never blocks the UI thread.
    parameter_poll_cursor: usize,
    playing: bool,
    position_beats: f64,
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
    /// 当前实时 GPU 配置；与设置比较，避免每次同步都重建设备。
    /// Current realtime GPU config; compared with the settings so devices are not rebuilt every sync.
    gpu_realtime_config: Option<GpuRenderConfig>,
    /// 最近一次实时 GPU 限幅失败原因；由 UI 取走后清空。
    /// Last realtime GPU limiter failure; cleared once the UI takes it.
    gpu_realtime_warning: Option<String>,
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
}

enum InstrumentRuntime {
    Synth(PolySynth),
    Drums(DrumKit),
    Native(ModeledInstrument),
    Plugin(PluginRuntime),
    Failed(String),
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
                plugin.process(sample_rate, frames, input, gates, out_l, out_r)
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
                InstrumentRef::Plugin(pref) => {
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
                            // 导入的未验证状态只保留数据，不自动恢复，避免插件崩溃。
                            // Unverified imported state is preserved but not applied.
                            if pref.restore_state {
                                runtime.restore_state(&pref.state, &pref.params);
                            }
                            InstrumentRuntime::Plugin(runtime)
                        }
                        Err(error) => {
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
                _ => {}
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
            // 参数变化不会改变身份键；内置效果需每帧同步参数。
            // Parameter edits do not change the identity key, so native effects sync settings.
            for (runtime, effect) in self.effects.iter_mut().zip(track.effects.iter()) {
                if let (EffectRuntime::Native(native), EffectRef::Native(reference)) =
                    (runtime, effect)
                {
                    native.set_settings(reference.settings.clone());
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
                            if pref.restore_state {
                                runtime.restore_state(&pref.state, &pref.params);
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
            if let (EffectRuntime::Native(native), EffectRef::Native(reference)) = (runtime, effect)
            {
                native.set_settings(reference.settings.clone());
            }
        }
    }

    fn panic(&mut self) {
        match &mut self.instrument {
            InstrumentRuntime::Synth(synth) => synth.all_notes_off(),
            InstrumentRuntime::Drums(drums) => drums.all_notes_off(),
            InstrumentRuntime::Native(native) => native.all_notes_off(),
            InstrumentRuntime::Plugin(plugin) => plugin.panic(),
            InstrumentRuntime::Failed(_) => {}
        }
        for effect in &mut self.effects {
            effect.panic();
        }
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
            block_size: 512,
            project: Project::default(),
            tracks: Vec::new(),
            master_effect: None,
            master_effect_key: None,
            plugins_sample_rate: 0.0,
            last_tempo: 0.0,
            last_parameter_poll: Instant::now(),
            parameter_poll_cursor: 0,
            playing: false,
            position_beats: 0.0,
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
            gpu_realtime_config: None,
            gpu_realtime_warning: None,
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
            if let InstrumentRuntime::Plugin(plugin) = &mut track.instrument {
                plugin.shutdown();
            }
            // 清空身份键：已终止的实例只剩 `Failed` 外壳，下一次同步必须重建，
            // 否则重新打开同一工程时插件不会重新加载。
            // Clear the identity keys: a terminated instance is only a `Failed` shell, so the next
            // sync must rebuild it; otherwise reopening the same project would never reload it.
            track.instrument_key.clear();
            for effect in &mut track.effects {
                if let EffectRuntime::Plugin(plugin) = effect {
                    plugin.shutdown();
                }
            }
            track.effect_keys.clear();
        }
        if let Some(EffectRuntime::Plugin(plugin)) = &mut self.master_effect {
            plugin.shutdown();
        }
        self.master_effect_key = None;
    }

    /// 所属模块：audio::engine / GPU 实时渲染
    /// 功能：按设置启用或关闭实时主总线 GPU 软限幅；配置未变化时不重建设备。
    /// 输入：`config`：GPU 设备/驱动/API 选择；`None` 表示关闭并回到 CPU。
    /// 输出/副作用：可能创建 GPU 设备并把失败原因存入 `gpu_realtime_warning`；
    /// 成功后实时回调在 `GPU_REALTIME_TIMEOUT_MS` 内等待结果，超时自动回退 CPU。
    /// 复杂度：时间 O(GPU 设备创建)（配置变化时），空间 O(1)。
    ///
    /// Module: audio::engine / GPU realtime rendering
    /// Purpose: Enable or disable the realtime master-bus GPU soft limiter from the settings; an
    /// unchanged config never rebuilds the device.
    /// Inputs: `config`: GPU device/driver/API selection; `None` disables and returns to the CPU.
    /// Outputs / side effects: may create a GPU device and store a failure reason in
    /// `gpu_realtime_warning`; on success the callback waits at most `GPU_REALTIME_TIMEOUT_MS` and
    /// falls back to the CPU on timeout.
    /// Complexity: O(GPU device creation) time when the config changes, O(1) space.
    pub fn configure_gpu_realtime(&mut self, config: Option<GpuRenderConfig>) {
        // 配置相同即返回：失败后不会在每次同步时反复创建设备；用户切换设置可触发重试。
        // Return on an unchanged config: a failure never rebuilds the device on every sync; toggling
        // the setting retries.
        if config == self.gpu_realtime_config {
            return;
        }
        self.gpu_realtime = None;
        self.gpu_realtime_config = config.clone();
        let Some(config) = config else {
            return;
        };
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
                            if pref.restore_state {
                                runtime.restore_state(&pref.state, &pref.params);
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

        (warnings, budget.pending)
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
        self.position_beats
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
            self.reset_note_state(self.position_beats);
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
        self.audio_cursors.clear();
        self.auto_stopped = false;
        self.reset_note_state(self.position_beats);
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
}
