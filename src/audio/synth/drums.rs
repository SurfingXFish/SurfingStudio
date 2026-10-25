//! 所属模块：audio::synth / 内置鼓机
//! 文件用途：
//! - 用纯 DSP 合成底鼓、军鼓、闭/开镲、拍手、通鼓与其他打击乐。
//! - 音色参考 808/909 的合成方式：扫频正弦底鼓、双音模式军鼓、金属方波踩镲与多段噪声拍手。
//!
//! 函数清单：
//! - `drum_kind`：MIDI 音高到鼓件映射。
//! - `DrumKit::new` / `set_settings` / `set_sustain` / `set_pitch_bend` / `all_notes_off`：运行时控制。
//! - `DrumKit::trigger` / `render`：触发与逐采样合成。
//! - `white` / `wrap_phase` / `metallic` / `high_pass`：噪声、相位与滤波辅助。
//!
//! Module: audio::synth / built-in drum kit
//! Purpose:
//! - Synthesize kick, snare, closed/open hats, clap, toms and other percussion with pure DSP.
//! - Timbres follow 808/909 recipes: swept-sine kick, two-mode snare, metallic square hats and
//!   multi-burst clap.
//!
//! Functions:
//! - `drum_kind`: MIDI pitch to drum-piece mapping.
//! - `DrumKit::new` / `set_settings` / `set_sustain` / `set_pitch_bend` / `all_notes_off`: runtime control.
//! - `DrumKit::trigger` / `render`: triggering and per-sample synthesis.
//! - `white` / `wrap_phase` / `metallic` / `high_pass`: noise, phase and filter helpers.

use super::{GateKind, SynthEvent};
use crate::core::model::DrumSettings;
use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrumKind {
    Kick,
    Snare,
    Hat,
    OpenHat,
    Clap,
    Tom,
    Other,
}

fn drum_kind(pitch: u8) -> DrumKind {
    match pitch {
        35 | 36 => DrumKind::Kick,
        37 | 38 | 40 => DrumKind::Snare,
        42 | 44 => DrumKind::Hat,
        46 => DrumKind::OpenHat,
        39 => DrumKind::Clap,
        41 | 43 | 45 | 47 | 48 | 50 => DrumKind::Tom,
        _ => DrumKind::Other,
    }
}

#[derive(Clone, Copy)]
struct DrumVoice {
    active: bool,
    kind: DrumKind,
    note: u8,
    time: f32,
    amp: f32,
    velocity: f32,
    pitch_hz: f32,
    /// 触发时捕获的弯音倍率；鼓件为一次性发声，无法在发声中途改频。
    /// Pitch-bend ratio captured at trigger time; drums are one-shot voices.
    pitch_ratio: f32,
    noise: u32,
    hp_prev: f32,
    hp_state: f32,
    /// 主振荡器相位（底鼓/通鼓的扫频需要连续相位，避免用 `t × f` 产生相位跳变）。
    /// Main oscillator phase (swept kicks/toms need continuous phase instead of `t × f`).
    phase: f32,
    /// 第二振荡器相位（军鼓双音、金属噪声）。
    /// Second oscillator phase (snare two-tone and metallic noise).
    phase2: f32,
    age: u64,
}

impl Default for DrumVoice {
    fn default() -> Self {
        Self {
            active: false,
            kind: DrumKind::Kick,
            note: 36,
            time: 0.0,
            amp: 0.0,
            velocity: 1.0,
            pitch_hz: 50.0,
            pitch_ratio: 1.0,
            noise: 0x12345678,
            hp_prev: 0.0,
            hp_state: 0.0,
            phase: 0.0,
            phase2: 0.0,
            age: 0,
        }
    }
}

pub struct DrumKit {
    sample_rate: f32,
    settings: DrumSettings,
    voices: Vec<DrumVoice>,
    age_counter: u64,
    /// 延音踏板状态。鼓件为一次性发声，踏板不会延长已经触发的音符。
    /// Sustain-pedal state. Drum hits are one-shot, so the pedal never extends them.
    sustain_pedal: bool,
    pitch_bend_semitones: f32,
}

impl DrumKit {
    pub fn new(sample_rate: f32, settings: DrumSettings) -> Self {
        Self {
            sample_rate,
            settings,
            voices: (0..16).map(|_| DrumVoice::default()).collect(),
            age_counter: 0,
            sustain_pedal: false,
            pitch_bend_semitones: 0.0,
        }
    }

    pub fn set_settings(&mut self, settings: DrumSettings) {
        self.settings = settings;
    }

    /// 所属模块：audio::synth / 鼓机
    /// 功能：记录延音踏板状态（一次性鼓件无挂音，仅保持接口一致）。
    /// 输入：`on` 是否踩下。
    /// 输出/副作用：更新踏板状态；不修改正在发声的 voice。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::synth / drum kit
    /// Purpose: Record the sustain pedal (one-shot drums have no held notes; keeps the API
    /// consistent with other instruments).
    /// Inputs: `on` whether the pedal is down.
    /// Outputs / side effects: updates the pedal state; sounding voices are unchanged.
    /// Complexity: O(1) time and space.
    pub fn set_sustain(&mut self, on: bool) {
        self.sustain_pedal = on;
    }

    /// 所属模块：audio::synth / 鼓机
    /// 功能：设置弯音；在下次触发鼓件时改变音调类鼓件的频率。
    /// 输入：`semitones` 半音数。
    /// 输出/副作用：更新弯音值；不影响已触发的 voice。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::synth / drum kit
    /// Purpose: Set pitch bend; the next triggered tonal drum uses the new frequency.
    /// Inputs: `semitones` semitone offset.
    /// Outputs / side effects: updates the bend; already-triggered voices are unchanged.
    /// Complexity: O(1) time and space.
    pub fn set_pitch_bend(&mut self, semitones: f32) {
        self.pitch_bend_semitones = semitones.clamp(-24.0, 24.0);
    }

    pub fn all_notes_off(&mut self) {
        for voice in &mut self.voices {
            voice.active = false;
        }
    }

    /// 所属模块：audio::synth / 鼓机
    /// 功能：分配 voice 并触发一个鼓件，记录力度、音高、弯音与噪声种子。
    /// 输入：
    /// - `note`：MIDI 音高（决定鼓件）。
    /// - `velocity`：MIDI 力度，1..127。
    /// 输出/副作用：更新 voice 分配与初始状态；不修改全局设置。
    /// 复杂度：时间 O(voice 数)，空间 O(1)。
    ///
    /// Module: audio::synth / drum kit
    /// Purpose: Allocate a voice and trigger one drum piece, capturing velocity, pitch, bend and
    /// noise seed.
    /// Inputs:
    /// - `note`: MIDI pitch (selects the drum piece).
    /// - `velocity`: MIDI velocity, 1..127.
    /// Outputs / side effects: updates voice allocation and initial state.
    /// Complexity: O(voice count) time and O(1) space.
    fn trigger(&mut self, note: u8, velocity: u8) {
        let slot = self
            .voices
            .iter()
            .position(|v| !v.active)
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| v.age)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });
        self.age_counter = self.age_counter.wrapping_add(1);
        let kind = drum_kind(note);
        let freq = 440.0
            * 2.0_f32
                .powf((note as f32 - 69.0) / 12.0)
                .clamp(30.0, 1200.0);
        let voice = &mut self.voices[slot];
        voice.active = true;
        voice.kind = kind;
        voice.note = note;
        voice.time = 0.0;
        voice.amp = 1.0;
        voice.velocity = (velocity.max(1) as f32 / 127.0).powf(0.8);
        voice.pitch_hz = freq;
        voice.pitch_ratio = 2.0_f32.powf(self.pitch_bend_semitones / 12.0);
        voice.hp_prev = 0.0;
        voice.hp_state = 0.0;
        voice.phase = 0.0;
        voice.phase2 = 0.0;
        voice.age = self.age_counter;
    }

    /// 所属模块：audio::synth / 鼓机
    /// 功能：渲染一个 block：按帧偏移触发鼓件并逐采样合成全部活动 voice。
    /// 输入：
    /// - `out`：单声道输出缓冲；引擎会复制到右声道。
    /// - `events`：本块 Note On / Off 事件（鼓件只响应 Note On）。
    /// 输出/副作用：覆写 `out`；推进 voice 时间、相位与滤波状态。
    /// 复杂度：时间 O(帧数 × 活动 voice 数)，空间 O(1)。
    ///
    /// Module: audio::synth / drum kit
    /// Purpose: Render one block: trigger events at frame offsets and synthesize every active
    /// voice per sample.
    /// Inputs:
    /// - `out`: mono output buffer (the engine copies it to the right channel).
    /// - `events`: note on/off events for this block (drums only react to note-on).
    /// Outputs / side effects: overwrites `out`; advances voice time, phase and filters.
    /// Complexity: O(frames × active voices) time and O(1) space.
    pub fn render(&mut self, out: &mut [f32], events: &[SynthEvent]) {
        out.fill(0.0);
        let sr = self.sample_rate.max(1.0);
        let mut event_index = 0;
        let tone = self.settings.tone.clamp(0.0, 1.0);
        // 踩下延音踏板时让闭镲缓慢衰减，模拟鼓手持续踩踏板的效果。
        // With the sustain pedal down the closed hat decays more slowly.
        let hat_decay = if self.sustain_pedal { 26.0 } else { 45.0 };

        for (frame, sample_out) in out.iter_mut().enumerate() {
            while event_index < events.len() && events[event_index].frame <= frame {
                let event = events[event_index];
                if event.kind == GateKind::On {
                    self.trigger(event.pitch, event.velocity);
                }
                event_index += 1;
            }

            let mut mix = 0.0;
            for voice in &mut self.voices {
                if !voice.active {
                    continue;
                }
                let t = voice.time;
                let sample = match voice.kind {
                    DrumKind::Kick => {
                        // 808 风格：正弦从约 150 Hz 扫到 50 Hz，再叠加极短的高频点击。
                        // 808 style: a sine sweeping from ~150 Hz to 50 Hz plus a very short click.
                        let sweep = 50.0 + 110.0 * (-t * 32.0).exp();
                        let f = (sweep * voice.pitch_ratio).clamp(25.0, 400.0);
                        voice.phase = wrap_phase(voice.phase + 2.0 * PI * f / sr);
                        let body = voice.phase.sin() * (-t * 5.5).exp();
                        let click =
                            white(&mut voice.noise) * (-t * 420.0).exp() * (0.22 + tone * 0.3);
                        ((body * 1.15 + click).tanh()) * 0.9
                    }
                    DrumKind::Snare => {
                        // 909 风格：两个音调模式（约 185 / 330 Hz）加高通噪声。
                        // 909 style: two tonal modes (~185/330 Hz) plus high-passed noise.
                        let f1 = 185.0 * voice.pitch_ratio;
                        let f2 = 330.0 * voice.pitch_ratio;
                        voice.phase = wrap_phase(voice.phase + 2.0 * PI * f1 / sr);
                        voice.phase2 = wrap_phase(voice.phase2 + 2.0 * PI * f2 / sr);
                        let tone_part = voice.phase.sin() * 0.42 * (-t * 26.0).exp()
                            + voice.phase2.sin() * 0.2 * (-t * 36.0).exp();
                        let noise = white(&mut voice.noise);
                        let hp = high_pass(&mut voice.hp_prev, &mut voice.hp_state, noise, 0.78);
                        (tone_part + hp * 0.9 * (-t * 14.0).exp()) * 0.9
                    }
                    DrumKind::Hat | DrumKind::OpenHat => {
                        // 808 金属噪声：六个非整数比方波相加后高通，比纯噪声更像踩镲。
                        // 808 metallic noise: six inharmonic square waves summed and high-passed.
                        let metal = metallic(t, voice.pitch_ratio);
                        let noise = white(&mut voice.noise);
                        let hp = high_pass(
                            &mut voice.hp_prev,
                            &mut voice.hp_state,
                            metal * 0.35 + noise * 0.65,
                            0.86,
                        );
                        let decay = if voice.kind == DrumKind::Hat {
                            hat_decay * (0.8 + tone * 0.5)
                        } else {
                            11.0
                        };
                        hp * (-t * decay).exp() * 0.75
                    }
                    DrumKind::Clap => {
                        // 三次快速噪声爆发 + 尾音，模拟多人拍手的微小时间差。
                        // Three fast noise bursts plus a tail simulate several pairs of hands.
                        let noise = white(&mut voice.noise);
                        let hp = high_pass(&mut voice.hp_prev, &mut voice.hp_state, noise, 0.5);
                        let burst = if t < 0.012
                            || (0.018..0.030).contains(&t)
                            || (0.034..0.046).contains(&t)
                        {
                            1.0
                        } else {
                            0.35
                        };
                        hp * burst * (-t * 12.0).exp() * 0.95
                    }
                    DrumKind::Tom => {
                        let f =
                            (voice.pitch_hz * (0.55 + 0.9 * (-t * 22.0).exp())) * voice.pitch_ratio;
                        voice.phase = wrap_phase(voice.phase + 2.0 * PI * f.max(30.0) / sr);
                        let body = voice.phase.sin() * (-t * 9.0).exp();
                        let noise = white(&mut voice.noise) * (-t * 55.0).exp() * 0.08;
                        (body + noise) * 0.9
                    }
                    DrumKind::Other => {
                        let f = (voice.pitch_hz * voice.pitch_ratio).clamp(30.0, 4000.0);
                        voice.phase = wrap_phase(voice.phase + 2.0 * PI * f / sr);
                        voice.phase.sin() * (-t * 14.0).exp() * 0.6
                    }
                };
                let sample = sample * voice.velocity;
                mix += sample;
                voice.time += 1.0 / sr;
                voice.amp *= 0.9995;
                let silence = match voice.kind {
                    DrumKind::Kick => voice.time > 1.2,
                    DrumKind::Snare | DrumKind::Clap | DrumKind::Tom => voice.time > 0.8,
                    DrumKind::Hat => voice.time > 0.35,
                    DrumKind::OpenHat => voice.time > 1.0,
                    DrumKind::Other => voice.time > 0.9,
                };
                if silence || voice.amp < 0.0005 {
                    voice.active = false;
                }
            }

            mix *= self.settings.level * (0.8 + tone * 0.5);
            *sample_out += (mix * 1.1).tanh() * 0.7;
        }
    }
}

/// 所属模块：audio::synth / 鼓机
/// 功能：生成确定性白噪声。
/// 输入：`state` 线性同余状态。
/// 输出/副作用：返回 [-1, 1] 噪声并推进状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio::synth / drum kit
/// Purpose: Generate deterministic white noise.
/// Inputs: `state` LCG state.
/// Outputs / side effects: noise in [-1, 1] and advances the state.
/// Complexity: O(1) time and space.
fn white(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    ((*state >> 8) as f32 / 8_388_608.0) - 1.0
}

/// 所属模块：audio::synth / 鼓机
/// 功能：把弧度相位折回 `[0, 2π)`，保持扫频振荡器的连续性。
/// 输入：
/// - `phase`：当前相位，弧度。
/// 输出/副作用：折回后的相位；不修改外部状态。
/// 复杂度：时间 O(1)（单次增量远小于 2π），空间 O(1)。
///
/// Module: audio::synth / drum kit
/// Purpose: Wrap a radian phase back into `[0, 2π)` so swept oscillators stay continuous.
/// Inputs:
/// - `phase`: current phase in radians.
/// Outputs / side effects: the wrapped phase; no external change.
/// Complexity: O(1) time and space.
fn wrap_phase(phase: f32) -> f32 {
    let two_pi = 2.0 * PI;
    let mut wrapped = phase;
    while wrapped >= two_pi {
        wrapped -= two_pi;
    }
    if wrapped < 0.0 {
        wrapped += two_pi;
    }
    wrapped
}

/// 所属模块：audio::synth / 鼓机
/// 功能：用六个非整数比方波合成 808 风格金属噪声。
/// 输入：
/// - `seconds`：鼓件已发声时间，秒。
/// - `ratio`：弯音倍率。
/// 输出/副作用：返回金属噪声采样；不修改状态。
/// 复杂度：时间 O(1)（固定 6 个振荡器），空间 O(1)。
///
/// Module: audio::synth / drum kit
/// Purpose: Synthesize 808-style metallic noise with six inharmonic square waves.
/// Inputs:
/// - `seconds`: elapsed time of the hit in seconds.
/// - `ratio`: pitch-bend ratio.
/// Outputs / side effects: metallic noise sample; no state change.
/// Complexity: O(1) time (six fixed oscillators) and O(1) space.
fn metallic(seconds: f32, ratio: f32) -> f32 {
    const FREQUENCIES: [f32; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];
    let mut sum = 0.0;
    for frequency in FREQUENCIES {
        let phase = (seconds * frequency * ratio).fract();
        sum += if phase < 0.5 { 1.0 } else { -1.0 };
    }
    sum / 6.0
}

/// 所属模块：audio::synth / 鼓机
/// 功能：一阶高通差分，用于军鼓、踩镲与拍手的噪声塑形。
/// 输入：
/// - `prev_in`：上一输入采样。
/// - `state`：高通内部状态。
/// - `input`：当前输入采样。
/// - `coeff`：高通系数（0..1，越大越亮）。
/// 输出/副作用：返回高通采样并更新状态；不分配内存。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio::synth / drum kit
/// Purpose: One-pole high-pass difference used to shape snare, hat and clap noise.
/// Inputs:
/// - `prev_in`: previous input sample.
/// - `state`: internal high-pass state.
/// - `input`: current input sample.
/// - `coeff`: high-pass coefficient (0..1, larger is brighter).
/// Outputs / side effects: high-passed sample; updates the state.
/// Complexity: O(1) time and space.
fn high_pass(prev_in: &mut f32, state: &mut f32, input: f32, coeff: f32) -> f32 {
    // One-pole high-pass difference.
    let out = coeff * (*state + input - *prev_in);
    *prev_in = input;
    *state = out;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：全部鼓件在多块渲染后仍为有限值且不超出安全幅度。
    /// Covers: every drum piece stays finite and bounded after several rendered blocks.
    #[test]
    fn every_drum_piece_renders_finite_output() {
        for pitch in [
            35_u8, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 50, 60,
        ] {
            let mut kit = DrumKit::new(48_000.0, DrumSettings::default());
            let mut block = vec![0.0_f32; 512];
            kit.render(
                &mut block,
                &[SynthEvent {
                    frame: 0,
                    kind: GateKind::On,
                    pitch,
                    velocity: 112,
                }],
            );
            for _ in 0..24 {
                kit.render(&mut block, &[]);
                for sample in &block {
                    assert!(sample.is_finite(), "pitch {pitch} produced NaN");
                    assert!(
                        sample.abs() < 4.0,
                        "pitch {pitch} produced an unbounded sample: {sample}"
                    );
                }
            }
        }
    }
}
