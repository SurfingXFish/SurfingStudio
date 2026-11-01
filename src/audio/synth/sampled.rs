//! 所属模块：audio / 采样乐器
//! 文件用途：
//! - 播放内置采样音色库：按音高与力度选择分区，做变速变调、AR 包络与简单低通。
//! - 首次使用时解码内嵌音频（Ogg Vorbis / WAV），并在进程内共享解码结果，避免多轨重复占用内存。
//!
//! 函数清单：
//! - `SampledInstrument::new` / `set_settings` / `set_sustain` / `set_pitch_bend`：运行时控制。
//! - `SampledInstrument::all_notes_off` / `note_on` / `note_off` / `render`：音符与实时渲染。
//! - `load_sample`：解码并缓存 WAV 为单声道浮点样本。
//! - `zone_for`：按音高/力度选择分区。
//!
//! Module: audio / sampled instrument
//! Purpose:
//! - Play the built-in sampled bank: select a zone by pitch and velocity, apply varispeed pitch
//!   shifting, an AR envelope and a simple low-pass.
//! - Decode embedded WAVs on first use and share the decoded data process-wide so multiple tracks
//!   do not duplicate memory.
//!
//! Functions:
//! - `SampledInstrument::new` / `set_settings` / `set_sustain` / `set_pitch_bend`: runtime control.
//! - `SampledInstrument::all_notes_off` / `note_on` / `note_off` / `render`: notes and rendering.
//! - `load_sample`: decode and cache embedded audio as mono floating-point samples.
//! - `zone_for`: choose a zone by pitch and velocity.

use super::{GateKind, SynthEvent};
use crate::core::model::{BankInstrument, SampleSettings, bank_sample_bytes};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex, OnceLock};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// 复音数；采样乐器每声部只读取一条采样。
/// Polyphony; each sampled voice reads a single sample.
const MAX_VOICES: usize = 24;

/// 输出余量：采样原始峰值约 0.9，直接叠加和弦会削波；留出余量后用 tanh 软限幅。
/// Output headroom: raw samples peak near 0.9 and polyphonic stacking would clip; leave headroom
/// and soft-limit with tanh.
const OUTPUT_HEADROOM: f32 = 0.55;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EnvStage {
    Off,
    Attack,
    Sustain,
    Release,
}

/// 已解码的采样分区。
/// A decoded sample zone.
struct LoadedZone {
    lo: u8,
    hi: u8,
    lo_vel: u8,
    hi_vel: u8,
    root: f32,
    sample_rate: f32,
    data: Arc<Vec<f32>>,
    /// 循环区间 `[loop_start, loop_end)`；`loop_end == 0` 表示不循环。
    /// Loop region `[loop_start, loop_end)`; `loop_end == 0` means no loop.
    loop_start: usize,
    loop_end: usize,
    /// 循环接缝的交错淡化长度（采样数）。
    /// Cross-fade length in samples at the loop seam.
    loop_fade: usize,
}

impl LoadedZone {
    fn matches(&self, pitch: u8, velocity: u8) -> bool {
        pitch >= self.lo && pitch <= self.hi && velocity >= self.lo_vel && velocity <= self.hi_vel
    }
}

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    note: u8,
    velocity: f32,
    zone: usize,
    position: f64,
    step: f64,
    env: f32,
    stage: EnvStage,
    pending_release: bool,
    age: u64,
    tone_state: f32,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            active: false,
            note: 60,
            velocity: 1.0,
            zone: 0,
            position: 0.0,
            step: 1.0,
            env: 0.0,
            stage: EnvStage::Off,
            pending_release: false,
            age: 0,
            tone_state: 0.0,
        }
    }
}

pub struct SampledInstrument {
    sample_rate: f32,
    settings: SampleSettings,
    zones: Vec<LoadedZone>,
    voices: Vec<Voice>,
    age_counter: u64,
    sustain_pedal: bool,
    pitch_bend_semitones: f32,
}

impl SampledInstrument {
    /// 所属模块：audio / 采样乐器
    /// 功能：按清单乐器创建运行时，并预解码该乐器全部可用分区。
    /// 输入：`instrument` 静态清单、`settings` 可调参数、`sample_rate` 引擎采样率。
    /// 输出/副作用：解码并缓存音频；缺失或损坏的分区被跳过。
    /// 复杂度：时间 O(分区数 × 样本数)，空间 O(解码样本)。
    ///
    /// Module: audio / sampled instrument
    /// Purpose: Create the runtime for a manifest instrument and pre-decode every usable zone.
    /// Inputs: `instrument` static manifest, `settings` adjustable parameters, `sample_rate`
    /// engine sample rate.
    /// Outputs / side effects: decodes and caches audio; missing or corrupt zones are skipped.
    /// Complexity: O(zones × samples) time and O(decoded samples) space.
    pub fn new(
        instrument: &'static BankInstrument,
        settings: SampleSettings,
        sample_rate: f32,
    ) -> Self {
        let mut zones = Vec::with_capacity(instrument.zones.len());
        for zone in &instrument.zones {
            let Some((data, source_rate)) = load_sample(&zone.file) else {
                continue;
            };
            let (loop_start, loop_end, loop_fade) =
                loop_points(instrument.looped, &data, source_rate);
            zones.push(LoadedZone {
                lo: zone.lo,
                hi: zone.hi,
                lo_vel: zone.lo_vel,
                hi_vel: zone.hi_vel,
                root: zone.root as f32,
                sample_rate: source_rate,
                data,
                loop_start,
                loop_end,
                loop_fade,
            });
        }
        Self {
            sample_rate: sample_rate.max(1.0),
            settings,
            zones,
            voices: (0..MAX_VOICES).map(|_| Voice::default()).collect(),
            age_counter: 0,
            sustain_pedal: false,
            pitch_bend_semitones: 0.0,
        }
    }

    pub fn set_settings(&mut self, settings: SampleSettings) {
        self.settings = settings;
    }

    pub fn set_sustain(&mut self, on: bool) {
        if self.sustain_pedal && !on {
            for voice in &mut self.voices {
                if voice.active && voice.pending_release {
                    voice.pending_release = false;
                    voice.stage = EnvStage::Release;
                }
            }
        }
        self.sustain_pedal = on;
    }

    pub fn set_pitch_bend(&mut self, semitones: f32) {
        self.pitch_bend_semitones = semitones.clamp(-24.0, 24.0);
    }

    pub fn all_notes_off(&mut self) {
        for voice in &mut self.voices {
            if voice.active {
                voice.pending_release = false;
                voice.stage = EnvStage::Release;
            }
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        let Some(zone_index) = zone_for(&self.zones, note, velocity) else {
            return;
        };
        let slot = self
            .voices
            .iter()
            .position(|voice| voice.active && voice.note == note)
            .or_else(|| self.voices.iter().position(|voice| !voice.active))
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, voice)| voice.age)
                    .map(|(index, _)| index)
                    .unwrap_or(0)
            });
        let zone = &self.zones[zone_index];
        let semitones = note as f32 - zone.root;
        let base_step = (zone.sample_rate / self.sample_rate) * 2.0_f32.powf(semitones / 12.0);
        self.age_counter = self.age_counter.wrapping_add(1);
        let voice = &mut self.voices[slot];
        voice.active = true;
        voice.note = note;
        voice.velocity = (velocity.max(1) as f32 / 127.0).powf(0.85);
        voice.zone = zone_index;
        voice.position = 0.0;
        voice.step = base_step as f64;
        voice.env = 0.0;
        voice.stage = EnvStage::Attack;
        voice.pending_release = false;
        voice.age = self.age_counter;
        voice.tone_state = 0.0;
    }

    fn note_off(&mut self, note: u8) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note {
                if self.sustain_pedal {
                    voice.pending_release = true;
                } else {
                    voice.stage = EnvStage::Release;
                }
            }
        }
    }

    /// 所属模块：audio / 采样乐器
    /// 功能：渲染一个 block 的单声道音频，并按帧偏移应用音符事件。
    /// 输入：`out` 输出缓冲、`events` 本块事件。
    /// 输出/副作用：覆盖写入 `out`；推进各 voice 的采样位置与包络。
    /// 复杂度：时间 O(帧数 × 发声 voice 数)，空间 O(1)。
    ///
    /// Module: audio / sampled instrument
    /// Purpose: Render one mono block and apply note events at their frame offsets.
    /// Inputs: `out` output buffer, `events` block events.
    /// Outputs / side effects: overwrites `out`; advances voice positions and envelopes.
    /// Complexity: O(frames × active voices) time and O(1) space.
    pub fn render(&mut self, out: &mut [f32], events: &[SynthEvent]) {
        out.fill(0.0);
        let mut event_index = 0;
        let sr = self.sample_rate.max(1.0);
        let settings = self.settings.clone();
        let attack_step = 1.0 / (settings.attack.max(0.0005) * sr);
        let release_step = 1.0 / (settings.release.max(0.005) * sr);
        let level = settings.level.clamp(0.0, 2.0);
        let velocity_amount = settings.velocity.clamp(0.0, 1.0);
        let bend_ratio = 2.0_f32.powf(self.pitch_bend_semitones / 12.0) as f64;
        // tone=1 时截止频率接近奈奎斯特，保持原始采样；tone=0 时约 500 Hz。
        // tone=1 puts the cutoff near Nyquist (original sample); tone=0 is about 500 Hz.
        let cutoff = 500.0 * (sr * 0.45 / 500.0).powf(settings.tone.clamp(0.0, 1.0) as f32);
        let tone_a = 1.0 - (-std::f32::consts::TAU * cutoff.min(sr * 0.45) / sr).exp();

        for (frame, sample_out) in out.iter_mut().enumerate() {
            while event_index < events.len() && events[event_index].frame <= frame {
                let event = events[event_index];
                match event.kind {
                    GateKind::On => self.note_on(event.pitch, event.velocity),
                    GateKind::Off => self.note_off(event.pitch),
                }
                event_index += 1;
            }

            let mut mix = 0.0;
            for voice in &mut self.voices {
                if !voice.active {
                    continue;
                }
                match voice.stage {
                    EnvStage::Attack => {
                        voice.env = (voice.env + attack_step).min(1.0);
                        if voice.env >= 1.0 {
                            voice.stage = EnvStage::Sustain;
                        }
                    }
                    EnvStage::Sustain => {}
                    EnvStage::Release => {
                        voice.env -= release_step;
                        if voice.env <= 0.0 {
                            voice.env = 0.0;
                            voice.stage = EnvStage::Off;
                            voice.active = false;
                            continue;
                        }
                    }
                    EnvStage::Off => {
                        voice.active = false;
                        continue;
                    }
                }

                let zone = &self.zones[voice.zone];
                if zone.data.is_empty() {
                    voice.active = false;
                    continue;
                }
                let index = voice.position as usize;
                if index >= zone.data.len() {
                    voice.active = false;
                    voice.stage = EnvStage::Off;
                    continue;
                }
                // 三次 Hermite 插值：采样率转换与变调不再使用最近采样，消除明显的量化杂音。
                // Cubic Hermite interpolation: sample-rate conversion and pitch shifting no longer
                // use nearest-neighbour reads, removing audible quantisation noise.
                let looped = zone.loop_end > zone.loop_start && zone.loop_fade > 0;
                let sample = if looped {
                    let span = (zone.loop_end - zone.loop_start) as f64;
                    let fade_start = (zone.loop_end - zone.loop_fade) as f64;
                    if voice.position >= fade_start {
                        // 循环接缝交错淡化：尾段逐渐让位给循环起点前的内容，回绕时没有跳变。
                        // Cross-fade the loop seam: the outgoing tail gives way to the audio just
                        // before the loop start, so wrapping cannot click.
                        let t = ((voice.position - fade_start) / zone.loop_fade as f64)
                            .clamp(0.0, 1.0) as f32;
                        let outgoing = interpolate(&zone.data, voice.position);
                        let incoming = interpolate(&zone.data, voice.position - span);
                        outgoing * (1.0 - t) + incoming * t
                    } else {
                        interpolate(&zone.data, voice.position)
                    }
                } else {
                    interpolate(&zone.data, voice.position)
                };
                // 一阶低通：音色参数只改变亮度，不改变采样长度。
                // One-pole low-pass: the tone control only changes brightness.
                voice.tone_state += tone_a * (sample - voice.tone_state);
                let gain = voice.env * (1.0 - velocity_amount + velocity_amount * voice.velocity);
                mix += voice.tone_state * gain;

                voice.position += voice.step * bend_ratio;
                if looped && voice.position >= zone.loop_end as f64 {
                    let span = (zone.loop_end - zone.loop_start) as f64;
                    voice.position -= span;
                }
            }

            *sample_out = (mix * level * OUTPUT_HEADROOM).tanh();
        }
    }
}

/// 所属模块：audio / 采样乐器
/// 功能：在浮点位置上做 4 点三次 Hermite 插值（边界夹取）。
/// 输入：`data` 采样、`position` 浮点位置。
/// 输出/副作用：返回插值后的样本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / sampled instrument
/// Purpose: Four-point cubic Hermite interpolation at a fractional position (edge-clamped).
/// Inputs: `data` samples, `position` fractional position.
/// Outputs / side effects: the interpolated sample; no state change.
/// Complexity: O(1) time and space.
fn interpolate(data: &[f32], position: f64) -> f32 {
    let index = position.floor() as isize;
    let t = (position - index as f64) as f32;
    let at = |offset: isize| -> f32 {
        let index = (index + offset).clamp(0, data.len() as isize - 1) as usize;
        data[index]
    };
    let p0 = at(-1);
    let p1 = at(0);
    let p2 = at(1);
    let p3 = at(2);
    let c0 = p1;
    let c1 = 0.5 * (p2 - p0);
    let c2 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
    let c3 = 0.5 * (p3 - p0) + 1.5 * (p1 - p2);
    ((c3 * t + c2) * t + c1) * t + c0
}

/// 所属模块：audio / 采样乐器
/// 功能：计算持续音的循环区间与接缝淡入长度，跳过提取时写入的尾部淡出。
/// 输入：`looped` 清单是否允许循环、`data` 分区样本、`source_rate` 源采样率。
/// 输出/副作用：返回 `(loop_start, loop_end, loop_fade)`；不允许循环或样本过短时全为 0。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / sampled instrument
/// Purpose: Compute the loop region and seam cross-fade length for sustained instruments,
/// skipping the fade-out baked into the extracted tail.
/// Inputs: `looped` manifest flag, `data` zone samples, `source_rate` source sample rate.
/// Outputs / side effects: `(loop_start, loop_end, loop_fade)`; all zero when looping is disabled
/// or the sample is too short.
/// Complexity: O(1) time and space.
fn loop_points(looped: bool, data: &[f32], source_rate: f32) -> (usize, usize, usize) {
    if !looped || data.is_empty() {
        return (0, 0, 0);
    }
    let rate = source_rate.max(1.0);
    // 提取脚本在结尾写了约 60 ms 的淡出，循环点放在淡出之前。
    // The extraction script writes a ~60 ms fade-out at the end; keep the loop before it.
    let end = data.len().saturating_sub((0.08 * rate) as usize);
    let min_len = ((0.25 * rate) as usize).max(128);
    let max_len = ((0.9 * rate) as usize).max(min_len);
    let len = (((end as f32) * 0.45) as usize).clamp(min_len, max_len);
    if end <= len + 128 {
        return (0, 0, 0);
    }
    let start = end - len;
    let fade = ((0.03 * rate) as usize).min(len / 4).min(start);
    if fade < 64 {
        return (0, 0, 0);
    }
    (start, end, fade)
}

/// 所属模块：audio / 采样乐器
/// 功能：选择覆盖指定音高与力度的分区；重叠时优先更窄的键位/力度范围。
/// 输入：`zones` 已加载分区、`pitch` 音高、`velocity` 力度。
/// 输出/副作用：返回分区下标；无匹配时返回 `None`。
/// 复杂度：时间 O(分区数)，空间 O(1)。
///
/// Module: audio / sampled instrument
/// Purpose: Choose a zone covering the pitch and velocity; narrower key/velocity spans win.
/// Inputs: `zones` loaded zones, `pitch`, `velocity`.
/// Outputs / side effects: the zone index, or `None`.
/// Complexity: O(zones) time and O(1) space.
fn zone_for(zones: &[LoadedZone], pitch: u8, velocity: u8) -> Option<usize> {
    zones
        .iter()
        .enumerate()
        .filter(|(_, zone)| zone.matches(pitch, velocity))
        .min_by_key(|(_, zone)| {
            let keys = zone.hi.saturating_sub(zone.lo);
            let layers = zone.hi_vel.saturating_sub(zone.lo_vel);
            (keys as u32) + (layers as u32)
        })
        .map(|(index, _)| index)
}

static CACHE: OnceLock<Mutex<HashMap<String, Option<(Arc<Vec<f32>>, f32)>>>> = OnceLock::new();

/// 所属模块：audio / 采样乐器
/// 功能：解码内嵌音频并缓存为单声道浮点样本。
/// 输入：`file` 清单相对路径。
/// 输出/副作用：返回样本与源采样率；缺失或解析失败时缓存 `None`。
/// 复杂度：首次 O(样本数)，之后 O(1) + 克隆 `Arc`。
///
/// Module: audio / sampled instrument
/// Purpose: Decode an embedded audio file and cache it as mono floating-point samples.
/// Inputs: `file` relative manifest path.
/// Outputs / side effects: samples and source rate; missing or invalid files cache `None`.
/// Complexity: O(samples) on first use, O(1) plus an `Arc` clone afterwards.
fn load_sample(file: &str) -> Option<(Arc<Vec<f32>>, f32)> {
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock() {
        if let Some(cached) = cache.get(file) {
            return cached.clone();
        }
    }
    let decoded = decode_audio(file);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(file.to_string(), decoded.clone());
    }
    decoded
}

/// 所属模块：audio / 采样乐器
/// 功能：按文件头选择解码器：WAV 用 hound，Ogg Vorbis / FLAC 用 Symphonia。
/// 输入：`file` 清单相对路径。
/// 输出/副作用：返回单声道样本与源采样率；找不到或无法解析时返回 `None`。
/// 复杂度：时间 O(样本数)，空间 O(样本数)。
///
/// Module: audio / sampled instrument
/// Purpose: Pick a decoder by file signature: hound for WAV, Symphonia for Ogg Vorbis / FLAC.
/// Inputs: `file` relative manifest path.
/// Outputs / side effects: mono samples and source rate; `None` when missing or unparsable.
/// Complexity: O(samples) time and space.
fn decode_audio(file: &str) -> Option<(Arc<Vec<f32>>, f32)> {
    let bytes = bank_sample_bytes(file)?;
    if bytes.starts_with(b"OggS") || bytes.starts_with(b"fLaC") {
        decode_symphonia(bytes)
    } else {
        decode_wav(bytes)
    }
}

/// 所属模块：audio / 采样乐器
/// 功能：用 Symphonia 从内存解码 Ogg Vorbis / FLAC。
/// 输入：`bytes` 内嵌音频字节。
/// 输出/副作用：返回单声道样本与源采样率；解码失败时返回 `None`。
/// 复杂度：时间 O(样本数)，空间 O(样本数)。
///
/// Module: audio / sampled instrument
/// Purpose: Decode Ogg Vorbis / FLAC from memory with Symphonia.
/// Inputs: `bytes` embedded audio bytes.
/// Outputs / side effects: mono samples and source rate; `None` on decode failure.
/// Complexity: O(samples) time and space.
fn decode_symphonia(bytes: &'static [u8]) -> Option<(Arc<Vec<f32>>, f32)> {
    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("ogg");
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .ok()?;
    let track = format.default_track(TrackType::Audio)?.clone();
    let params = track.codec_params.as_ref()?.audio()?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .ok()?;
    let track_id = track.id;
    let mut interleaved: Vec<f32> = Vec::new();
    let mut channels = 0usize;
    let mut sample_rate = 0.0_f32;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) | Err(Error::ResetRequired) => break,
            Err(_) => break,
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(buffer) => buffer,
            Err(Error::DecodeError(_)) | Err(Error::IoError(_)) => continue,
            Err(_) => break,
        };
        let spec = decoded.spec();
        if channels == 0 {
            channels = spec.channels().count();
            sample_rate = spec.rate() as f32;
        }
        let count = decoded.samples_interleaved();
        if count == 0 {
            continue;
        }
        let start = interleaved.len();
        interleaved.resize(start + count, 0.0);
        decoded.copy_to_slice_interleaved(&mut interleaved[start..]);
    }
    if interleaved.is_empty() || channels == 0 {
        return None;
    }
    let mono = downmix(&interleaved, channels);
    Some((Arc::new(trim_leading_silence(mono)), sample_rate.max(1.0)))
}

/// 所属模块：audio / 采样乐器
/// 功能：用 hound 读取内嵌 WAV，多声道求平均后转为 `f32`。
/// 输入：`bytes` 内嵌 WAV 字节。
/// 输出/副作用：返回样本与源采样率；无法解析时返回 `None`。
/// 复杂度：时间 O(样本数)，空间 O(样本数)。
///
/// Module: audio / sampled instrument
/// Purpose: Read embedded WAV bytes with hound and average channels into `f32`.
/// Inputs: `bytes` embedded WAV bytes.
/// Outputs / side effects: samples and source rate; `None` when unparsable.
/// Complexity: O(samples) time and space.
fn decode_wav(bytes: &'static [u8]) -> Option<(Arc<Vec<f32>>, f32)> {
    let reader = hound::WavReader::new(Cursor::new(bytes)).ok()?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let mono = match spec.sample_format {
        hound::SampleFormat::Float => {
            let mut frames = Vec::new();
            for sample in reader.into_samples::<f32>() {
                frames.push(sample.ok()?);
            }
            downmix(&frames, channels)
        }
        hound::SampleFormat::Int => {
            let mut frames = Vec::new();
            for sample in reader.into_samples::<i32>() {
                let value = sample.ok()?;
                let scale = 1.0 / (1i64 << (spec.bits_per_sample.saturating_sub(1))) as f32;
                frames.push(value as f32 * scale);
            }
            downmix(&frames, channels)
        }
    };
    if mono.is_empty() {
        return None;
    }
    Some((
        Arc::new(trim_leading_silence(mono)),
        spec.sample_rate as f32,
    ))
}

/// 所属模块：audio / 采样乐器
/// 功能：裁掉采样开头的静音/低电平残留，把起音对齐到真正的音符起始。
/// 输入：`samples` 单声道样本。
/// 输出/副作用：返回裁剪后的样本；全静音时原样返回。
/// 复杂度：时间 O(样本数)，空间 O(样本数)。
///
/// Module: audio / sampled instrument
/// Purpose: Trim leading silence/residue and align the attack with the real note onset.
/// Inputs: `samples` mono samples.
/// Outputs / side effects: trimmed samples; all-silent input is returned unchanged.
/// Complexity: O(samples) time and space.
fn trim_leading_silence(samples: Vec<f32>) -> Vec<f32> {
    let peak = samples
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    if peak <= 0.0 {
        return samples;
    }
    let silence_threshold = peak * 0.002;
    let Some(onset) = samples
        .iter()
        .position(|sample| sample.abs() >= silence_threshold)
    else {
        return samples;
    };
    // 提取片段时，部分分区会在音符前带上上一音符的低电平尾音；此时 0.2% 阈值会停在尾音上，
    // 让起音延后上百毫秒。若真正起音（5% 峰值）明显晚于首个非静音点，就把起点移到起音，
    // 保留少量预卷以避免切掉瞬态。
    // Some extracted zones begin with the low-level tail of a previous note. The 0.2 % threshold
    // then stops on that tail and delays the attack by up to hundreds of milliseconds. When the
    // real attack (5 % of peak) starts clearly later, move the start to the attack, keeping a
    // short pre-roll so the transient is not clipped.
    let attack = samples
        .iter()
        .position(|sample| sample.abs() >= peak * 0.05)
        .unwrap_or(onset);
    let start = if attack > onset + 1_024 {
        attack
    } else {
        onset
    };
    let pre_roll = 64;
    let start = start.saturating_sub(pre_roll);
    let mut trimmed = if start == 0 {
        samples
    } else {
        samples[start..].to_vec()
    };
    // 从起音位置直接开始播放会在第一个采样产生阶跃，听起来像每音一次的“噔”声。
    // 用约 3 ms 的半余弦淡入消除阶跃，同时不影响音头瞬态。
    // Starting directly on the attack leaves a step at the first sample, audible as a "thump".
    // A ~3 ms half-cosine fade-in removes the step without softening the transient noticeably.
    let fade = trimmed.len().min(128);
    for (index, sample) in trimmed[..fade].iter_mut().enumerate() {
        let phase = index as f32 / fade as f32;
        *sample *= 0.5 - 0.5 * (std::f32::consts::PI * phase).cos();
    }
    trimmed
}

/// 所属模块：audio / 采样乐器
/// 功能：把交错多声道样本平均为单声道。
/// 输入：`frames` 交错样本、`channels` 声道数。
/// 输出/副作用：返回单声道样本；不修改输入。
/// 复杂度：时间 O(样本数)，空间 O(样本数 / 声道数)。
///
/// Module: audio / sampled instrument
/// Purpose: Average interleaved multi-channel samples into mono.
/// Inputs: `frames` interleaved samples, `channels` channel count.
/// Outputs / side effects: mono samples; the input is untouched.
/// Complexity: O(samples) time and O(samples / channels) space.
fn downmix(frames: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return frames.to_vec();
    }
    frames
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{SampleSettings, bank_instrument};

    /// 覆盖：内嵌音频解码后的幅度与源采样率正确。
    /// Covers: decoded embedded audio amplitude and source sample rate.
    #[test]
    fn embedded_audio_decodes_at_full_scale() {
        let entry = bank_instrument("GrandPiano").expect("GrandPiano");
        let file = entry.zones[0].file.as_str();
        let (data, rate) = load_sample(file).expect("decode");
        assert_eq!(rate, entry.sample_rate as f32);
        assert_eq!(rate, 44_100.0);
        let peak = data
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        eprintln!("decoded {} samples, peak {peak}", data.len());
        assert!(peak > 0.5, "解码幅度过小：{peak}");
        assert!(peak <= 1.0, "解码幅度越界：{peak}");
    }

    /// 覆盖：钢琴采样在音符开启后能渲染出有界、非静音的音频。
    /// Covers: the piano sample renders bounded, non-silent audio after a note-on.
    #[test]
    fn grand_piano_renders_audible_audio() {
        let entry = bank_instrument("GrandPiano").expect("GrandPiano");
        let mut instrument = SampledInstrument::new(entry, SampleSettings::default(), 48_000.0);
        assert!(!instrument.zones.is_empty(), "应至少解码一个分区");
        let events = [SynthEvent {
            frame: 0,
            kind: GateKind::On,
            pitch: 60,
            velocity: 100,
        }];
        let mut out = vec![0.0; 4_800];
        instrument.render(&mut out, &events);
        let peak = out
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(peak > 0.02, "渲染输出过小：{peak}");
        assert!(peak <= 1.0, "渲染输出越界：{peak}");
        assert!(out.iter().all(|sample| sample.is_finite()));
    }

    /// 覆盖：音色库内每件乐器都能解码并渲染出非静音输出。
    /// Covers: every bank instrument decodes and renders non-silent output.
    #[test]
    fn every_bank_instrument_renders() {
        for entry in crate::core::model::bank_instruments() {
            let pitch = (entry.range[0] + entry.range[1]) / 2;
            let mut instrument = SampledInstrument::new(entry, SampleSettings::default(), 48_000.0);
            assert!(!instrument.zones.is_empty(), "{} 没有可用分区", entry.id);
            let events = [SynthEvent {
                frame: 0,
                kind: GateKind::On,
                pitch,
                velocity: 100,
            }];
            let mut out = vec![0.0; 9_600];
            instrument.render(&mut out, &events);
            let peak = out
                .iter()
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
            assert!(peak > 0.001, "{} 渲染输出过小：{peak}", entry.id);
            assert!(out.iter().all(|sample| sample.is_finite()));
        }
    }

    /// 覆盖：裁掉起音残留后样本从 0 开始淡入，不会在第一个采样产生“噔”声阶跃。
    /// Covers: after trimming, samples fade in from zero so the first sample cannot click.
    #[test]
    fn trimmed_attack_fades_in_from_zero() {
        let entry = bank_instrument("StringLeg").expect("StringLeg");
        for zone in &entry.zones {
            let (data, _) = load_sample(&zone.file).expect("decode zone");
            assert_eq!(data[0], 0.0, "{} 起音没有淡入", zone.file);
        }
    }

    /// 覆盖：持续类乐器超过采样长度后仍持续发声（交叉淡化循环），不会提前中断。
    /// Covers: sustained instruments keep sounding past the sample length via cross-faded looping.
    #[test]
    fn sustained_instrument_loops_past_sample_length() {
        let entry = bank_instrument("StringLeg").expect("StringLeg");
        assert!(entry.looped, "StringLeg 应开启循环");
        let mut instrument = SampledInstrument::new(entry, SampleSettings::default(), 48_000.0);
        assert!(
            instrument
                .zones
                .iter()
                .all(|zone| zone.loop_end > zone.loop_start && zone.loop_fade > 0),
            "持续类分区应带有循环点"
        );
        let events = [SynthEvent {
            frame: 0,
            kind: GateKind::On,
            pitch: 84,
            velocity: 100,
        }];
        let mut out = vec![0.0; 48_000 * 4];
        instrument.render(&mut out, &events);
        let tail = &out[out.len() - 4_800..];
        let rms =
            (tail.iter().map(|sample| sample * sample).sum::<f32>() / tail.len() as f32).sqrt();
        assert!(rms > 0.01, "长音末尾过小：{rms}");
        assert!(
            out.iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 1.0)
        );
    }

    /// 覆盖：超出所有分区音域时保持静音且不崩溃。
    /// Covers: pitches outside every zone stay silent without panicking.
    #[test]
    fn out_of_range_note_is_silent() {
        let entry = bank_instrument("GrandPiano").expect("GrandPiano");
        let mut instrument = SampledInstrument::new(entry, SampleSettings::default(), 48_000.0);
        let events = [SynthEvent {
            frame: 0,
            kind: GateKind::On,
            pitch: 0,
            velocity: 100,
        }];
        let mut out = vec![0.5; 128];
        instrument.render(&mut out, &events);
        assert!(out.iter().all(|sample| *sample == 0.0));
    }
}
