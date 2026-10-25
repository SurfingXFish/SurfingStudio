//! 所属模块：audio::synth / 原生乐器
//! 文件用途：
//! - 用纯 DSP 实现钢琴、木吉他、电吉他、失真电吉他、贝斯、小提琴与三种弦乐奏法。
//! - 拨弦类使用 Karplus-Strong 延迟线（拨弦位置梳状激励、音高相关阻尼、琴箱/拾音器共振）；
//!   钢琴使用音高相关非谐分音、击弦噪声与音板共振；弓弦类使用失谐振荡器、体腔共振与弓毛噪声。
//! - 全部乐器支持音量、声像、力度（由引擎统一处理）以及延音踏板与 ±2 半音弯音。
//!
//! 函数清单：
//! - `ModeledInstrumentKind::is_plucked` / `is_piano` / `is_bowed`：奏法分类。
//! - `Voice::new` / `start_pluck` / `start_piano` / `start_bow`：voice 初始化。
//! - `Voice::start_release` / `render_pluck` / `render_piano` / `render_bow`：单采样渲染。
//! - `ModeledInstrument::new` / `set_settings` / `set_sustain` / `set_pitch_bend`：运行时控制。
//! - `ModeledInstrument::all_notes_off` / `note_on` / `note_off` / `render`：音符与实时渲染。
//! - `Resonator::new` / `set` / `process`：琴箱与体腔共振。
//! - `noise` / `decay_coefficient`：噪声与衰减系数辅助。
//!
//! Module: audio::synth / native instruments
//! Purpose:
//! - Implement piano, acoustic/electric/distorted guitar, bass, violin and three string
//!   articulations with pure DSP.
//! - Plucked models use Karplus-Strong delay lines (pluck-position comb, pitch-dependent damping
//!   and body/pickup resonators); the piano uses pitch-dependent inharmonic partials, hammer noise
//!   and soundboard resonances; bowed models use detuned oscillators, cavity formants and bow noise.
//! - Every instrument supports volume, pan and velocity (handled by the engine) plus the
//!   sustain pedal and ±2 semitone pitch bend.
//!
//! Functions:
//! - `ModeledInstrumentKind::is_plucked` / `is_piano` / `is_bowed`: articulation families.
//! - `Voice::new` / `start_pluck` / `start_piano` / `start_bow`: voice initialisation.
//! - `Voice::start_release` / `render_pluck` / `render_piano` / `render_bow`: per-sample render.
//! - `ModeledInstrument::new` / `set_settings` / `set_sustain` / `set_pitch_bend`: runtime control.
//! - `ModeledInstrument::all_notes_off` / `note_on` / `note_off` / `render`: notes and rendering.
//! - `Resonator::new` / `set` / `process`: body and cavity resonators.
//! - `noise` / `decay_coefficient`: noise and decay helpers.

use super::{GateKind, SynthEvent};
use crate::core::model::{NativeInstrumentKind, NativeInstrumentSettings};
use std::f32::consts::PI;

/// 每个 voice 的延迟线长度：8192 采样可覆盖约 5.9 Hz 以上的最低音。
/// Delay-line length per voice: 8192 samples covers the lowest notes above about 5.9 Hz.
const DELAY_LEN: usize = 8192;
/// 复音数；钢琴与弦乐需要比合成器更多的同时发声数。
/// Polyphony; the piano and strings need more simultaneous voices than the synth.
const MAX_VOICES: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EnvStage {
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

impl NativeInstrumentKind {
    /// 所属模块：audio::synth / 原生乐器
    /// 功能：是否为拨弦类（延迟线激励）。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Whether this kind is plucked (delay-line excitation).
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1) time and space.
    fn is_plucked(self) -> bool {
        matches!(
            self,
            NativeInstrumentKind::AcousticGuitar
                | NativeInstrumentKind::ElectricGuitar
                | NativeInstrumentKind::DistortedGuitar
                | NativeInstrumentKind::BassGuitar
                | NativeInstrumentKind::StringPizzicato
        )
    }
}

impl NativeInstrumentKind {
    /// 所属模块：audio::synth / 原生乐器
    /// 功能：是否为钢琴（非谐分音叠加）。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Whether this kind is the piano (inharmonic additive model).
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1) time and space.
    fn is_piano(self) -> bool {
        self == NativeInstrumentKind::Piano
    }

    /// 所属模块：audio::synth / 原生乐器
    /// 功能：是否为弓弦类（多振荡器 + ADSR）。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Whether this kind is bowed (detuned oscillators + ADSR).
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1) time and space.
    fn is_bowed(self) -> bool {
        matches!(
            self,
            NativeInstrumentKind::Violin
                | NativeInstrumentKind::StringStaccato
                | NativeInstrumentKind::StringLegato
        )
    }
}

/// 二阶共振器：模拟钢琴音板、吉他琴箱与小提琴体腔的固定共振峰。
/// Second-order resonator used for soundboard, guitar-body and violin-cavity formants.
struct Resonator {
    b1: f32,
    b2: f32,
    gain: f32,
    y1: f32,
    y2: f32,
}

impl Resonator {
    /// 所属模块：audio::synth / 原生乐器 / 共振器
    /// 功能：创建静默共振器。
    /// 输入：无。
    /// 输出/副作用：返回零状态共振器；不修改外部状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments / resonator
    /// Purpose: Create a silent resonator.
    /// Inputs: none.
    /// Outputs / side effects: a zero-state resonator; no external change.
    /// Complexity: O(1) time and space.
    fn new() -> Self {
        Self {
            b1: 0.0,
            b2: 0.0,
            gain: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// 所属模块：audio::synth / 原生乐器 / 共振器
    /// 功能：按中心频率、带宽与增益配置共振峰；频率自动限制在奈奎斯特以下。
    /// 输入：
    /// - `freq_hz`：共振中心频率，Hz。
    /// - `bandwidth_hz`：-3 dB 带宽，Hz。
    /// - `gain`：共振峰混合增益（通常 0.05 ~ 0.35）。
    /// - `sample_rate`：采样率。
    /// 输出/副作用：写入极点和增益并清空状态；不修改其他 voice。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments / resonator
    /// Purpose: Configure a formant from centre frequency, bandwidth and gain; the frequency is
    /// clamped below Nyquist.
    /// Inputs:
    /// - `freq_hz`: centre frequency in Hz.
    /// - `bandwidth_hz`: -3 dB bandwidth in Hz.
    /// - `gain`: formant mix gain (typically 0.05 ~ 0.35).
    /// - `sample_rate`: sample rate.
    /// Outputs / side effects: writes poles and gain and clears the state.
    /// Complexity: O(1) time and space.
    fn set(&mut self, freq_hz: f32, bandwidth_hz: f32, gain: f32, sample_rate: f32) {
        let sample_rate = sample_rate.max(1.0);
        let freq = freq_hz.clamp(20.0, sample_rate * 0.45);
        let bandwidth = bandwidth_hz.clamp(2.0, sample_rate * 0.4);
        // 极点半径决定共振锐度；1-r 归一化避免不同带宽导致音量跳变。
        // The pole radius sets the sharpness; 1-r normalisation avoids level jumps.
        let radius = (-PI * bandwidth / sample_rate).exp().min(0.9999);
        let theta = 2.0 * PI * freq / sample_rate;
        self.b1 = 2.0 * radius * theta.cos();
        self.b2 = -radius * radius;
        // 极点越靠近单位圆、频率越低，峰值增益越大；用 |1 - r e^{-2jθ}| 归一化到设定增益。
        // Lower frequencies and narrower bandwidths raise the peak gain; normalise with
        // |1 - r e^{-2jθ}| so the peak equals the requested gain.
        let denominator = (1.0 - 2.0 * radius * (2.0 * theta).cos() + radius * radius)
            .sqrt()
            .max(1e-4);
        self.gain = gain * (1.0 - radius) * denominator;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    /// 所属模块：audio::synth / 原生乐器 / 共振器
    /// 功能：处理一个采样。
    /// 输入：`input` 单声道采样。
    /// 输出/副作用：返回共振输出并推进内部状态；不分配内存。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments / resonator
    /// Purpose: Process one sample.
    /// Inputs: `input` mono sample.
    /// Outputs / side effects: the resonant output; advances internal state; no allocation.
    /// Complexity: O(1) time and space.
    fn process(&mut self, input: f32) -> f32 {
        let out = input * self.gain + self.b1 * self.y1 + self.b2 * self.y2;
        self.y2 = self.y1;
        self.y1 = out;
        out
    }
}

/// 单个原生乐器 voice。延迟线在构造时一次性分配，渲染期间不再分配。
/// One native-instrument voice. Delay lines are allocated once at construction.
struct Voice {
    active: bool,
    note: u8,
    velocity: f32,
    age: u64,
    pending_release: bool,
    releasing: bool,
    env_stage: EnvStage,
    env: f32,
    // 拨弦：延迟线与环路状态。
    // Pluck: delay line and loop state.
    delay: Vec<f32>,
    base_delay: f32,
    write: usize,
    damp: f32,
    loop_decay: f32,
    release_decay: f32,
    last_filtered: f32,
    click_env: f32,
    click_decay: f32,
    click_noise: u32,
    // 钢琴：六个非谐分音、击弦噪声与琴弦同音群拍频。
    // Piano: six inharmonic partials, hammer noise and unison beating.
    partial_phase: [f32; 6],
    partial_inc: [f32; 6],
    partial_amp: [f32; 6],
    partial_decay: [f32; 6],
    hammer_env: f32,
    hammer_decay: f32,
    hammer_noise: u32,
    unison_phase: f32,
    release_gain: f32,
    // 弓弦：三个失谐振荡器、弓噪与颤音渐入。
    // Bowed: three detuned oscillators, bow noise and vibrato fade-in.
    phase: [f32; 3],
    inc: [f32; 3],
    lowpass: f32,
    vibrato_phase: f32,
    vibrato_age: f32,
    bow_noise: u32,
    // 体腔共振（拨弦琴箱、钢琴音板、弓弦体腔）。
    // Body resonances (plucked body, piano soundboard, bowed cavity).
    body1: Resonator,
    body2: Resonator,
    body3: Resonator,
}

impl Voice {
    /// 所属模块：audio::synth / 原生乐器
    /// 功能：创建一个空闲 voice，并分配固定长度的延迟线。
    /// 输入：无。
    /// 输出/副作用：返回 voice；分配 `DELAY_LEN` 个采样。
    /// 复杂度：时间 O(DELAY_LEN)，空间 O(DELAY_LEN)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Create an idle voice and allocate its fixed-length delay line.
    /// Inputs: none.
    /// Outputs / side effects: the voice; allocates `DELAY_LEN` samples.
    /// Complexity: O(DELAY_LEN) time and space.
    fn new() -> Self {
        Self {
            active: false,
            note: 60,
            velocity: 1.0,
            age: 0,
            pending_release: false,
            releasing: false,
            env_stage: EnvStage::Off,
            env: 0.0,
            delay: vec![0.0; DELAY_LEN],
            base_delay: 100.0,
            write: 0,
            damp: 0.6,
            loop_decay: 0.999,
            release_decay: 0.99,
            last_filtered: 0.0,
            click_env: 0.0,
            click_decay: 0.0,
            click_noise: 0x1234_5678,
            partial_phase: [0.0; 6],
            partial_inc: [0.0; 6],
            partial_amp: [0.0; 6],
            partial_decay: [0.999; 6],
            hammer_env: 0.0,
            hammer_decay: 0.0,
            hammer_noise: 0x9e37_79b9,
            unison_phase: 0.0,
            release_gain: 1.0,
            phase: [0.0; 3],
            inc: [0.0; 3],
            lowpass: 0.0,
            vibrato_phase: 0.0,
            vibrato_age: 0.0,
            bow_noise: 0x2545_f491,
            body1: Resonator::new(),
            body2: Resonator::new(),
            body3: Resonator::new(),
        }
    }

    /// 所属模块：audio::synth / 原生乐器
    /// 功能：初始化拨弦 voice：噪声激励、拨弦位置梳状相减、环路阻尼、拨片瞬态与琴箱共振。
    /// 输入：
    /// - `kind`：乐器类型。
    /// - `settings`：音色参数。
    /// - `freq`：基频 Hz。
    /// - `sample_rate`：采样率。
    /// - `velocity`：1..127 力度。
    /// 输出/副作用：重置延迟线、包络、环路、瞬态与共振器状态。
    /// 复杂度：时间 O(DELAY_LEN)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Initialise a plucked voice: noise excitation, pluck-position comb, loop damping,
    /// pick transient and body resonances.
    /// Inputs:
    /// - `kind`: instrument kind.
    /// - `settings`: tone parameters.
    /// - `freq`: fundamental in Hz.
    /// - `sample_rate`: sample rate.
    /// - `velocity`: MIDI velocity 1..127.
    /// Outputs / side effects: resets the delay line, envelope, loop, transient and resonators.
    /// Complexity: O(DELAY_LEN) time and O(1) space.
    fn start_pluck(
        &mut self,
        kind: NativeInstrumentKind,
        settings: &NativeInstrumentSettings,
        freq: f32,
        sample_rate: f32,
        velocity: u8,
    ) {
        // 参考弦乐器物理建模的 T60 范围：低音弦余音最长，失真吉他因高增益更持久。
        // T60 ranges follow physical-model references: bass strings ring longest and a
        // high-gain distorted guitar sustains longer than a clean one.
        let t60_seconds = match kind {
            NativeInstrumentKind::BassGuitar => 3.8,
            NativeInstrumentKind::AcousticGuitar => 4.5,
            NativeInstrumentKind::ElectricGuitar => 5.2,
            NativeInstrumentKind::DistortedGuitar => 6.0,
            NativeInstrumentKind::StringPizzicato => 0.8,
            _ => 2.0,
        };
        // 拨弦位置决定被削弱的谐波：靠近琴桥时高频更亮，靠近琴颈时更温暖。
        // Pluck position notches harmonics: near the bridge is brighter, near the neck warmer.
        let pick_position = match kind {
            NativeInstrumentKind::BassGuitar => 0.12,
            NativeInstrumentKind::AcousticGuitar => 0.18,
            NativeInstrumentKind::ElectricGuitar => 0.24,
            NativeInstrumentKind::DistortedGuitar => 0.3,
            NativeInstrumentKind::StringPizzicato => 0.1,
            _ => 0.2,
        };
        let release_seconds = settings.release.clamp(0.05, 3.0);
        let brightness = settings.tone.clamp(0.0, 1.0);
        // 环路低通截止随音高上移，高音才不会因为固定阻尼而发闷。
        // The loop-filter cutoff follows the pitch so high notes are not dulled by a fixed damping.
        let cutoff = (freq * (4.0 + brightness * 8.0)).clamp(600.0, sample_rate * 0.45);
        self.damp = 1.0 - (-2.0 * PI * cutoff / sample_rate.max(1.0)).exp();
        // 低通在环路中引入额外相位延迟；从延迟线长度中扣除，避免基频偏低。
        // The loop low-pass adds phase delay; subtract it from the delay length so the fundamental
        // is not flat.
        let filter_delay = ((1.0 - self.damp) / self.damp.max(0.05)).clamp(0.0, 4.0);
        self.base_delay =
            (sample_rate / freq.max(20.0) - filter_delay).clamp(2.0, (DELAY_LEN - 4) as f32);
        let length = self.base_delay.round() as usize;
        self.delay.fill(0.0);
        // 激励噪声带一阶低通；低音区更暗，避免拨弦瞬间过于刺耳。
        // The excitation noise is low-passed; the bass register is darker to avoid a harsh attack.
        let excitation = match kind {
            NativeInstrumentKind::BassGuitar => 0.12 + brightness * 0.38,
            _ => 0.22 + brightness * 0.68,
        };
        let mut state = 0x2545_f491u32 ^ (self.note as u32).wrapping_mul(2654435761);
        let mut previous = 0.0_f32;
        for index in 0..length {
            let white = noise(&mut state);
            previous += (white - previous) * excitation;
            self.delay[index] = previous;
        }
        // 梳状相减模拟拨弦位置：延迟副本相减产生真实弦上缺失的谐波。
        // A comb subtraction models the pluck position: the delayed copy removes missing harmonics.
        let pick_offset = ((length as f32 * pick_position).round() as usize).max(1);
        for index in (0..length).rev() {
            let delayed = if index >= pick_offset {
                self.delay[index - pick_offset]
            } else {
                0.0
            };
            let taper = 1.0 - index as f32 / length.max(1) as f32;
            self.delay[index] = (self.delay[index] - delayed * 0.9) * taper * 0.85;
        }
        self.write = length % DELAY_LEN;
        self.loop_decay = decay_coefficient(t60_seconds, freq.max(20.0));
        self.release_decay = decay_coefficient(release_seconds, sample_rate);
        self.last_filtered = 0.0;
        self.velocity = (velocity.max(1) as f32 / 127.0).powf(0.8);
        // 拨片/指甲的瞬态噪声，电声乐器更明显，是“像真实乐器”的关键线索。
        // A pick/fingernail transient is more obvious on electric instruments and is key to realism.
        self.click_env = self.velocity.powf(1.4)
            * if kind == NativeInstrumentKind::BassGuitar {
                0.10
            } else {
                0.22
            };
        self.click_decay = (-1.0 / (0.006 * sample_rate.max(1.0))).exp();
        self.click_noise = 0x9e37_79b9 ^ (self.note as u32).wrapping_mul(2246822519);
        // 琴箱/拾音器共振：木吉他是低频木质共鸣，电吉他加入拾音器中高频峰。
        // Body/pickup resonances: woody low modes for acoustic, a pickup peak for electric guitars.
        match kind {
            NativeInstrumentKind::AcousticGuitar => {
                self.body1.set(105.0, 24.0, 0.30, sample_rate);
                self.body2.set(205.0, 34.0, 0.22, sample_rate);
                self.body3.set(410.0, 60.0, 0.12, sample_rate);
            }
            NativeInstrumentKind::ElectricGuitar => {
                self.body1.set(160.0, 45.0, 0.16, sample_rate);
                self.body2.set(900.0, 240.0, 0.18, sample_rate);
                self.body3.set(2800.0, 700.0, 0.20, sample_rate);
            }
            NativeInstrumentKind::DistortedGuitar => {
                self.body1.set(110.0, 50.0, 0.12, sample_rate);
                self.body2.set(700.0, 220.0, 0.16, sample_rate);
                self.body3.set(3200.0, 900.0, 0.14, sample_rate);
            }
            NativeInstrumentKind::BassGuitar => {
                self.body1.set(62.0, 18.0, 0.34, sample_rate);
                self.body2.set(125.0, 26.0, 0.24, sample_rate);
                self.body3.set(400.0, 90.0, 0.10, sample_rate);
            }
            _ => {
                self.body1.set(240.0, 70.0, 0.18, sample_rate);
                self.body2.set(520.0, 120.0, 0.14, sample_rate);
                self.body3.set(1200.0, 260.0, 0.08, sample_rate);
            }
        }
        self.env = 1.0;
        self.release_gain = 1.0;
        self.releasing = false;
        self.pending_release = false;
        self.env_stage = EnvStage::Sustain;
    }

    /// 所属模块：audio::synth / 原生乐器
    /// 功能：初始化钢琴 voice 的六个非谐分音、击弦噪声与音板共振。
    /// 输入：
    /// - `settings`：音色参数。
    /// - `freq`：基频 Hz。
    /// - `sample_rate`：采样率。
    /// - `velocity`：MIDI 力度。
    /// 输出/副作用：重置分音相位、幅度、衰减、击弦噪声与共振器。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Initialise the six inharmonic partials, hammer noise and soundboard resonances.
    /// Inputs:
    /// - `settings`: tone parameters.
    /// - `freq`: fundamental in Hz.
    /// - `sample_rate`: sample rate.
    /// - `velocity`: MIDI velocity.
    /// Outputs / side effects: resets partial phases/amplitudes/decays, hammer noise and resonators.
    /// Complexity: O(1) time and space.
    fn start_piano(
        &mut self,
        settings: &NativeInstrumentSettings,
        freq: f32,
        sample_rate: f32,
        velocity: u8,
    ) {
        const AMPS: [f32; 6] = [1.0, 0.62, 0.40, 0.24, 0.15, 0.10];
        const DECAY_SCALE: [f32; 6] = [1.0, 0.72, 0.54, 0.42, 0.33, 0.26];
        let velocity_gain = (velocity.max(1) as f32 / 127.0).powf(0.75);
        // 钢琴弦的非谐性随音高增大：低音 B 约 3e-4，高音可到 3e-2（见文献值）。
        // Piano-string inharmonicity grows with pitch: B ≈ 3e-4 in the bass and up to 3e-2
        // in the treble (literature values).
        let note = self.note as f32;
        let inharmonicity = 0.0003 * 1.055_f32.powf((note - 21.0).clamp(0.0, 87.0));
        // 低音区余音更长；release 参数控制整体弦长感。
        // The bass register rings longer; `release` shapes the overall perceived string length.
        let register = ((60.0 - note) / 48.0).clamp(-0.35, 0.6);
        let base_t60 = ((2.4 + settings.release.clamp(0.15, 3.0) * 4.0) * (1.0 + register * 0.7))
            .clamp(0.8, 9.0);
        let brightness = ((0.55 + settings.tone.clamp(0.0, 1.0) * 0.7)
            * (0.6 + velocity_gain * 0.6))
            .clamp(0.35, 1.15);
        for index in 0..6 {
            self.partial_phase[index] = 0.0;
            let harmonic = (index + 1) as f32;
            let ratio = harmonic * (1.0 + inharmonicity * harmonic * harmonic).sqrt();
            let increment = freq * ratio / sample_rate;
            self.partial_inc[index] = increment;
            // 高音区的高次分音可能超过奈奎斯特频率，直接静音而不是产生混叠。
            // Upper partials of high notes can exceed Nyquist; mute them instead of aliasing.
            if increment >= 0.5 {
                self.partial_amp[index] = 0.0;
                self.partial_decay[index] = 0.0;
                continue;
            }
            // 力度越大高次分音越明显，tone 控制整体亮度。
            // Harder velocities excite more upper partials; `tone` shapes overall brightness.
            let rolloff = brightness.powf(index as f32 * 0.55);
            self.partial_amp[index] = AMPS[index] * rolloff * velocity_gain;
            self.partial_decay[index] =
                decay_coefficient(base_t60 * DECAY_SCALE[index], sample_rate);
        }
        // 击弦噪声：极短的高频瞬态，让钢琴不像纯加法合成器。
        // Hammer noise: a very short high-frequency transient that distinguishes a piano from an
        // additive organ.
        self.hammer_env = (0.08 + velocity_gain.powf(1.5) * 0.26) * (0.7 + brightness * 0.4);
        self.hammer_decay = (-1.0 / (0.008 * sample_rate.max(1.0))).exp();
        self.hammer_noise = 0x9e37_79b9 ^ (self.note as u32).wrapping_mul(3266489917);
        self.unison_phase = 0.0;
        // 音板低阶共振，给钢琴一点箱体体积感。
        // Low-order soundboard resonances add cabinet body to the piano.
        self.body1.set(92.0, 40.0, 0.10, sample_rate);
        self.body2.set(186.0, 70.0, 0.07, sample_rate);
        self.body3.set(380.0, 140.0, 0.05, sample_rate);
        self.velocity = velocity_gain;
        self.env = 0.0;
        self.env_stage = EnvStage::Attack;
        self.release_gain = 1.0;
        self.releasing = false;
        self.pending_release = false;
    }

    /// 所属模块：audio::synth / 原生乐器
    /// 功能：初始化弓弦 voice：三个轻微失谐的振荡器、体腔共振与颤音。
    /// 输入：
    /// - `kind`：乐器类型（小提琴 / 弦乐断奏 / 弦乐联奏）。
    /// - `settings`：音色参数。
    /// - `freq`：基频 Hz。
    /// - `sample_rate`：采样率。
    /// - `velocity`：MIDI 力度。
    /// 输出/副作用：重置振荡器相位、包络、颤音与共振器状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Initialise a bowed voice with three slightly detuned oscillators, cavity
    /// formants and vibrato.
    /// Inputs:
    /// - `kind`: instrument kind (violin / string staccato / string legato).
    /// - `settings`: tone parameters.
    /// - `freq`: fundamental in Hz.
    /// - `sample_rate`: sample rate.
    /// - `velocity`: MIDI velocity.
    /// Outputs / side effects: resets oscillator phases, envelope, vibrato and resonators.
    /// Complexity: O(1) time and space.
    fn start_bow(
        &mut self,
        kind: NativeInstrumentKind,
        settings: &NativeInstrumentSettings,
        freq: f32,
        sample_rate: f32,
        velocity: u8,
    ) {
        let (detune, formants): ([f32; 3], [(f32, f32, f32); 3]) = match kind {
            NativeInstrumentKind::StringStaccato => (
                [-0.002, 0.0, 0.0025],
                [
                    (320.0, 90.0, 0.28),
                    (760.0, 160.0, 0.20),
                    (1800.0, 340.0, 0.13),
                ],
            ),
            NativeInstrumentKind::StringLegato => (
                // 弦乐群奏用更宽的失谐制造合唱感。
                // The string ensemble uses wider detuning to create a chorus.
                [-0.007, -0.001, 0.008],
                [
                    (250.0, 100.0, 0.26),
                    (520.0, 200.0, 0.22),
                    (1200.0, 420.0, 0.16),
                ],
            ),
            _ => (
                [-0.003, 0.0, 0.004],
                [
                    (300.0, 80.0, 0.30),
                    (700.0, 140.0, 0.22),
                    (1600.0, 320.0, 0.14),
                ],
            ),
        };
        let tone = settings.tone.clamp(0.0, 1.0);
        for index in 0..3 {
            self.phase[index] = index as f32 * 0.13;
            self.inc[index] = freq * (1.0 + detune[index] * (0.4 + tone)) / sample_rate;
        }
        self.body1
            .set(formants[0].0, formants[0].1, formants[0].2, sample_rate);
        self.body2
            .set(formants[1].0, formants[1].1, formants[1].2, sample_rate);
        self.body3
            .set(formants[2].0, formants[2].1, formants[2].2, sample_rate);
        self.bow_noise = 0x2545_f491 ^ (self.note as u32).wrapping_mul(2246822519);
        self.velocity = (velocity.max(1) as f32 / 127.0).powf(0.7);
        self.env = 0.0;
        self.env_stage = EnvStage::Attack;
        self.lowpass = 0.0;
        self.vibrato_phase = 0.0;
        self.vibrato_age = 0.0;
        self.release_gain = 1.0;
        self.releasing = false;
        self.pending_release = false;
    }

    /// 所属模块：audio::synth / 原生乐器
    /// 功能：进入释放阶段；拨弦类改为快速阻尼。
    /// 输入：`plucked` 是否为拨弦类。
    /// 输出/副作用：更新 `releasing` / `env_stage`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Enter release; plucked models switch to fast damping.
    /// Inputs: `plucked` whether the model is plucked.
    /// Outputs / side effects: updates `releasing` / `env_stage`.
    /// Complexity: O(1) time and space.
    fn start_release(&mut self, plucked: bool) {
        self.releasing = true;
        self.pending_release = false;
        if plucked {
            self.env_stage = EnvStage::Release;
        } else if self.env_stage != EnvStage::Off {
            self.env_stage = EnvStage::Release;
        }
    }

    /// 所属模块：audio::synth / 原生乐器
    /// 功能：渲染一个拨弦采样。
    /// 输入：
    /// - `kind`：乐器类型。
    /// - `settings`：音色参数。
    /// - `bend_ratio`：弯音频率倍率。
    /// 输出/副作用：返回单声道采样；推进延迟线、包络、拨片瞬态与琴箱共振。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Render one plucked sample.
    /// Inputs:
    /// - `kind`: instrument kind.
    /// - `settings`: tone parameters.
    /// - `bend_ratio`: pitch-bend frequency multiplier.
    /// Outputs / side effects: one mono sample; advances the delay line, envelope, pick transient
    /// and body resonators.
    /// Complexity: O(1) time and space.
    fn render_pluck(
        &mut self,
        kind: NativeInstrumentKind,
        settings: &NativeInstrumentSettings,
        bend_ratio: f32,
    ) -> f32 {
        if self.releasing {
            self.env *= self.release_decay;
        }
        if self.env < 1e-4 {
            self.active = false;
            self.env_stage = EnvStage::Off;
            return 0.0;
        }
        let delay = (self.base_delay / bend_ratio).clamp(2.0, (DELAY_LEN - 3) as f32);
        let read = self.write as f32 - delay;
        let read = if read < 0.0 {
            read + DELAY_LEN as f32
        } else {
            read
        };
        let index = read.floor() as usize % DELAY_LEN;
        let next = (index + 1) % DELAY_LEN;
        let frac = read - read.floor();
        let raw = self.delay[index] * (1.0 - frac) + self.delay[next] * frac;
        self.last_filtered += (raw - self.last_filtered) * self.damp;
        // 环路衰减只作用于写回值；松键包络负责收尾，避免双重衰减把余音切得过短。
        // The loop decay only applies to the written value; the release envelope handles the tail
        // so the note is not shortened twice.
        self.delay[self.write] = self.last_filtered * self.loop_decay;
        self.write = (self.write + 1) % DELAY_LEN;

        let mut sample = self.last_filtered * self.env;
        // 拨片瞬态：与弦体信号并行，让起音有清晰的“咔”声。
        // The pick transient runs in parallel with the string body and gives a clear attack click.
        if self.click_env > 1e-4 {
            sample += noise(&mut self.click_noise) * self.click_env;
            self.click_env *= self.click_decay;
        }
        // 琴箱/拾音器共振与干声混合，避免共振峰把基频吞掉。
        // Mix body/pickup resonances with the dry string so the fundamental is not swallowed.
        let body = self.body1.process(sample) * 0.9
            + self.body2.process(sample) * 0.65
            + self.body3.process(sample) * 0.4;
        sample = sample * 0.72 + body;
        // 电吉他/失真电吉他的拾音器与音箱染色。
        // Pickup and amplifier colouring for the electric guitars.
        match kind {
            NativeInstrumentKind::DistortedGuitar => {
                let drive = 2.0 + settings.drive.clamp(0.0, 1.0) * 12.0;
                sample = (sample * drive).tanh() * 0.65;
                sample = sample - sample * sample * sample * 0.12;
            }
            NativeInstrumentKind::ElectricGuitar => {
                let drive = 1.0 + settings.drive.clamp(0.0, 1.0) * 3.0;
                sample = (sample * drive).tanh() * 0.85;
            }
            NativeInstrumentKind::AcousticGuitar => {
                sample += sample * sample * 0.16;
            }
            _ => {}
        }
        sample * self.velocity
    }

    /// 所属模块：audio::synth / 原生乐器
    /// 功能：渲染一个钢琴采样。
    /// 输入：
    /// - `attack_step`：已换算为每采样的起音增量。
    /// - `bend_ratio`：弯音频率倍率。
    /// - `release_decay`：松键后的整体衰减系数。
    /// - `sample_rate`：采样率，用于同音群拍频。
    /// 输出/副作用：返回单声道采样；推进分音相位、击弦噪声与音板共振。
    /// 复杂度：时间 O(分音数)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Render one piano sample.
    /// Inputs:
    /// - `attack_step`: attack increment per sample.
    /// - `bend_ratio`: pitch-bend frequency multiplier.
    /// - `release_decay`: overall decay after note-off.
    /// - `sample_rate`: sample rate, used by the unison beating.
    /// Outputs / side effects: one mono sample; advances partial phases, hammer noise and the
    /// soundboard resonators.
    /// Complexity: O(partials) time and O(1) space.
    fn render_piano(
        &mut self,
        attack_step: f32,
        bend_ratio: f32,
        release_decay: f32,
        sample_rate: f32,
    ) -> f32 {
        if self.releasing {
            self.release_gain *= release_decay;
        }
        if self.release_gain < 1e-4 || self.partial_amp[0] < 1e-5 {
            self.active = false;
            self.env_stage = EnvStage::Off;
            return 0.0;
        }
        if self.env_stage == EnvStage::Attack {
            self.env += attack_step;
            if self.env >= 1.0 {
                self.env = 1.0;
                self.env_stage = EnvStage::Sustain;
            }
        }
        let mut sum = 0.0;
        for index in 0..6 {
            self.partial_phase[index] += self.partial_inc[index] * bend_ratio;
            if self.partial_phase[index] >= 1.0 {
                self.partial_phase[index] -= 1.0;
            }
            sum += (self.partial_phase[index] * 2.0 * PI).sin() * self.partial_amp[index];
            self.partial_amp[index] *= self.partial_decay[index];
        }
        // 同音群三根弦的轻微拍频，给钢琴增加自然的不稳定感。
        // Slight beating between the unison strings adds natural instability to the piano.
        self.unison_phase += 0.55 / sample_rate.max(1.0);
        if self.unison_phase >= 1.0 {
            self.unison_phase -= 1.0;
        }
        sum *= 1.0 + (self.unison_phase * 2.0 * PI).sin() * 0.04;
        if self.hammer_env > 1e-4 {
            sum += noise(&mut self.hammer_noise) * self.hammer_env * 0.7;
            self.hammer_env *= self.hammer_decay;
        }
        let body = self.body1.process(sum) * 0.9
            + self.body2.process(sum) * 0.6
            + self.body3.process(sum) * 0.35;
        (sum * 0.8 + body) * self.env * self.release_gain
    }

    /// 所属模块：audio::synth / 原生乐器
    /// 功能：渲染一个弓弦采样。
    /// 输入：
    /// - `settings`：音色与包络参数。
    /// - `bend_ratio`：弯音频率倍率。
    /// - `attack_step` / `decay_step` / `release_step`：包络每采样增量。
    /// - `sustain`：延音电平。
    /// - `sample_rate`：采样率。
    /// 输出/副作用：返回单声道采样；推进振荡器、包络、颤音、低通、体腔共振与弓毛噪声。
    /// 复杂度：时间 O(振荡器数)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Render one bowed sample.
    /// Inputs:
    /// - `settings`: tone and envelope parameters.
    /// - `bend_ratio`: pitch-bend frequency multiplier.
    /// - `attack_step` / `decay_step` / `release_step`: per-sample envelope increments.
    /// - `sustain`: sustain level.
    /// - `sample_rate`: sample rate.
    /// Outputs / side effects: one mono sample; advances oscillators, envelope, vibrato, LPF,
    /// cavity formants and bow noise.
    /// Complexity: O(oscillators) time and O(1) space.
    #[allow(clippy::too_many_arguments)]
    fn render_bow(
        &mut self,
        settings: &NativeInstrumentSettings,
        bend_ratio: f32,
        attack_step: f32,
        decay_step: f32,
        release_step: f32,
        sustain: f32,
        sample_rate: f32,
    ) -> f32 {
        match self.env_stage {
            EnvStage::Attack => {
                self.env += attack_step;
                if self.env >= 1.0 {
                    self.env = 1.0;
                    self.env_stage = EnvStage::Decay;
                }
            }
            EnvStage::Decay => {
                self.env -= decay_step;
                if self.env <= sustain {
                    self.env = sustain;
                    self.env_stage = EnvStage::Sustain;
                }
            }
            EnvStage::Sustain => self.env = sustain,
            EnvStage::Release => {
                self.env -= release_step;
                if self.env <= 0.0 {
                    self.env = 0.0;
                    self.env_stage = EnvStage::Off;
                    self.active = false;
                    return 0.0;
                }
            }
            EnvStage::Off => {
                self.active = false;
                return 0.0;
            }
        }

        self.vibrato_phase += 5.2 / sample_rate.max(1.0);
        if self.vibrato_phase >= 1.0 {
            self.vibrato_phase -= 1.0;
        }
        self.vibrato_age += 1.0 / sample_rate.max(1.0);
        // 颤音在 0.35 秒内渐入，模仿演奏者起弓后才加入揉弦。
        // Vibrato fades in over 0.35 s, like a player adding vibrato after the bow starts.
        let vibrato_depth =
            settings.vibrato.clamp(0.0, 1.0) * (self.vibrato_age / 0.35).clamp(0.0, 1.0);
        let vibrato = (self.vibrato_phase * 2.0 * PI).sin() * vibrato_depth * 0.28;
        let ratio = bend_ratio * 2.0_f32.powf(vibrato / 12.0);
        let mut sum = 0.0;
        for index in 0..3 {
            let increment = (self.inc[index] * ratio).clamp(0.0, 0.45);
            let value = 2.0 * self.phase[index]
                - 1.0
                - super::poly::poly_blep(self.phase[index], increment);
            self.phase[index] += increment;
            if self.phase[index] >= 1.0 {
                self.phase[index] -= 1.0;
            }
            sum += value;
        }
        // 体腔共振是弦乐与合成锯齿最明显的区别：小提琴在 300/700/1600 Hz 附近有固定峰。
        // Cavity formants distinguish a bowed string from a raw saw: the violin has fixed peaks
        // near 300/700/1600 Hz.
        let body = self.body1.process(sum * 0.33) * 0.9
            + self.body2.process(sum * 0.33) * 0.65
            + self.body3.process(sum * 0.33) * 0.35;
        // 弓毛摩擦噪声；音量很小，但能显著提升“真实弓弦”的质感。
        // Bow-hair friction noise is quiet but adds a convincing bowed-string texture.
        let bow = noise(&mut self.bow_noise) * self.env * self.velocity * 0.018;
        let cutoff = 350.0 + settings.tone.clamp(0.0, 1.0).powi(2) * 9000.0;
        let coefficient = 1.0 - (-2.0 * PI * cutoff / sample_rate).exp();
        self.lowpass += (sum * 0.33 - self.lowpass) * coefficient;
        (self.lowpass * 0.72 + body + bow) * self.env * self.velocity * 0.5
    }
}

/// 所属模块：audio::synth / 原生乐器
/// 功能：把 T60（衰减 60 dB 所需秒数）换算为每个更新周期的衰减系数。
/// 输入：
/// - `t60_seconds`：衰减时间，秒。
/// - `rate_per_second`：每秒更新次数（采样时为采样率，延迟线环路时为基频）。
/// 输出/副作用：每周期乘数；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio::synth / native instruments
/// Purpose: Convert a T60 (seconds to decay 60 dB) into a per-update multiplier.
/// Inputs:
/// - `t60_seconds`: decay time in seconds.
/// - `rate_per_second`: updates per second (sample rate for per-sample envelopes, fundamental
///   frequency for delay-line loops).
/// Outputs / side effects: per-update multiplier; no state change.
/// Complexity: O(1) time and space.
fn decay_coefficient(t60_seconds: f32, rate_per_second: f32) -> f32 {
    let t60 = t60_seconds.clamp(0.02, 20.0);
    10.0_f32.powf(-3.0 / (t60 * rate_per_second.max(1.0)))
}

/// 所属模块：audio::synth / 原生乐器
/// 功能：生成确定性白噪声。
/// 输入：`state` 线性同余状态。
/// 输出/副作用：返回 [-1, 1] 噪声并推进状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio::synth / native instruments
/// Purpose: Generate deterministic white noise.
/// Inputs: `state` LCG state.
/// Outputs / side effects: noise in [-1, 1] and advances the state.
/// Complexity: O(1) time and space.
fn noise(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    ((*state >> 8) as f32 / 8_388_608.0) - 1.0
}

/// 原生乐器运行时。
/// Native-instrument runtime.
pub struct ModeledInstrument {
    sample_rate: f32,
    kind: NativeInstrumentKind,
    settings: NativeInstrumentSettings,
    voices: Vec<Voice>,
    age_counter: u64,
    sustain_pedal: bool,
    pitch_bend_semitones: f32,
}

impl ModeledInstrument {
    /// 所属模块：audio::synth / 原生乐器
    /// 功能：创建原生乐器运行时，一次性分配全部 voice。
    /// 输入：
    /// - `kind`：乐器类型。
    /// - `settings`：音色参数。
    /// - `sample_rate`：采样率。
    /// 输出/副作用：返回运行时；分配 `MAX_VOICES × DELAY_LEN` 采样。
    /// 复杂度：时间 O(voice 数 × DELAY_LEN)，空间相同。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Create the runtime and allocate every voice once.
    /// Inputs:
    /// - `kind`: instrument kind.
    /// - `settings`: tone parameters.
    /// - `sample_rate`: sample rate.
    /// Outputs / side effects: the runtime; allocates `MAX_VOICES × DELAY_LEN` samples.
    /// Complexity: O(voice count × DELAY_LEN) time and space.
    pub fn new(
        kind: NativeInstrumentKind,
        settings: NativeInstrumentSettings,
        sample_rate: f32,
    ) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            kind,
            settings,
            voices: (0..MAX_VOICES).map(|_| Voice::new()).collect(),
            age_counter: 0,
            sustain_pedal: false,
            pitch_bend_semitones: 0.0,
        }
    }

    /// 所属模块：audio::synth / 原生乐器
    /// 功能：更新音色参数，保持正在发声的 voice 状态。
    /// 输入：`settings` 新参数。
    /// 输出/副作用：替换参数；不重置 voice。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Update tone parameters while keeping sounding voices alive.
    /// Inputs: `settings` new parameters.
    /// Outputs / side effects: replaces the settings; voices are not reset.
    /// Complexity: O(1) time and space.
    pub fn set_settings(&mut self, settings: NativeInstrumentSettings) {
        self.settings = settings;
    }

    /// 所属模块：audio::synth / 原生乐器
    /// 功能：设置延音踏板；抬起时释放被踏板挂起的音符。
    /// 输入：`on` 是否踩下。
    /// 输出/副作用：释放 `pending_release` 的 voice。
    /// 复杂度：时间 O(voice 数)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Set the sustain pedal; releasing it releases pedal-held voices.
    /// Inputs: `on` whether the pedal is down.
    /// Outputs / side effects: releases voices marked `pending_release`.
    /// Complexity: O(voice count) time and O(1) space.
    pub fn set_sustain(&mut self, on: bool) {
        if self.sustain_pedal && !on {
            let plucked = self.kind.is_plucked();
            for voice in &mut self.voices {
                if voice.active && voice.pending_release {
                    voice.start_release(plucked);
                }
            }
        }
        self.sustain_pedal = on;
    }

    /// 所属模块：audio::synth / 原生乐器
    /// 功能：设置弯音，范围 ±24 半音（引擎实际传入 ±2）。
    /// 输入：`semitones` 半音数。
    /// 输出/副作用：更新弯音值；不修改 voice。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Set pitch bend, clamped to ±24 semitones (the engine sends ±2).
    /// Inputs: `semitones` semitone offset.
    /// Outputs / side effects: updates the bend; voices are unchanged.
    /// Complexity: O(1) time and space.
    pub fn set_pitch_bend(&mut self, semitones: f32) {
        self.pitch_bend_semitones = semitones.clamp(-24.0, 24.0);
    }

    /// 所属模块：audio::synth / 原生乐器
    /// 功能：关闭全部音符（进入释放阶段，避免爆音）。
    /// 输入：`self`。
    /// 输出/副作用：所有活动 voice 进入释放；不做硬切断。
    /// 复杂度：时间 O(voice 数)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Release every note (release stage, no hard cut).
    /// Inputs: `self`.
    /// Outputs / side effects: every active voice enters release.
    /// Complexity: O(voice count) time and O(1) space.
    pub fn all_notes_off(&mut self) {
        let plucked = self.kind.is_plucked();
        for voice in &mut self.voices {
            if voice.active {
                voice.start_release(plucked);
            }
        }
    }

    /// 所属模块：audio::synth / 原生乐器
    /// 功能：分配 voice 并触发一个音符。
    /// 输入：
    /// - `note`：MIDI 音高。
    /// - `velocity`：MIDI 力度。
    /// 输出/副作用：更新 voice 分配、音高与激励；不修改其他状态。
    /// 复杂度：时间 O(voice 数 + DELAY_LEN)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Allocate a voice and trigger one note.
    /// Inputs:
    /// - `note`: MIDI pitch.
    /// - `velocity`: MIDI velocity.
    /// Outputs / side effects: updates voice allocation, pitch and excitation.
    /// Complexity: O(voice count + DELAY_LEN) time and O(1) space.
    fn note_on(&mut self, note: u8, velocity: u8) {
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
        self.age_counter = self.age_counter.wrapping_add(1);
        let semitones = note as f32 - 69.0;
        let freq = 440.0 * 2.0_f32.powf(semitones / 12.0);
        let kind = self.kind;
        let settings = self.settings.clone();
        let sample_rate = self.sample_rate;
        let voice = &mut self.voices[slot];
        voice.active = true;
        voice.note = note;
        voice.age = self.age_counter;
        match kind {
            _ if kind.is_piano() => voice.start_piano(&settings, freq, sample_rate, velocity),
            _ if kind.is_plucked() => {
                voice.start_pluck(kind, &settings, freq, sample_rate, velocity)
            }
            _ => voice.start_bow(kind, &settings, freq, sample_rate, velocity),
        }
    }

    /// 所属模块：audio::synth / 原生乐器
    /// 功能：结束一个音符；踩下延音时挂起释放。
    /// 输入：`note` MIDI 音高。
    /// 输出/副作用：相应 voice 进入释放或标记 `pending_release`。
    /// 复杂度：时间 O(voice 数)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: End a note; defer release while the sustain pedal is down.
    /// Inputs: `note` MIDI pitch.
    /// Outputs / side effects: voices enter release or are marked `pending_release`.
    /// Complexity: O(voice count) time and O(1) space.
    fn note_off(&mut self, note: u8) {
        let plucked = self.kind.is_plucked();
        for voice in &mut self.voices {
            if voice.active && voice.note == note {
                if self.sustain_pedal {
                    voice.pending_release = true;
                } else {
                    voice.start_release(plucked);
                }
            }
        }
    }

    /// 所属模块：audio::synth / 原生乐器
    /// 功能：渲染一个 block；事件按帧偏移触发。
    /// 输入：
    /// - `out`：单声道输出缓冲（引擎会复制到右声道）。
    /// - `events`：本块 Note On / Off 事件。
    /// 输出/副作用：覆写 `out`；推进 voice 状态。
    /// 复杂度：时间 O(帧数 × voice 数 × 分音/振荡器数)，空间 O(1)。
    ///
    /// Module: audio::synth / native instruments
    /// Purpose: Render one block, applying events at their frame offsets.
    /// Inputs:
    /// - `out`: mono output buffer (the engine copies it to the right channel).
    /// - `events`: note on/off events for this block.
    /// Outputs / side effects: overwrites `out`; advances voice state.
    /// Complexity: O(frames × voices × partials/oscillators) time and O(1) space.
    pub fn render(&mut self, out: &mut [f32], events: &[SynthEvent]) {
        out.fill(0.0);
        let mut event_index = 0;
        let settings = self.settings.clone();
        let sample_rate = self.sample_rate;
        let attack_step = 1.0 / (settings.attack.max(0.001) * sample_rate);
        let decay_step =
            (1.0 - settings.sustain.clamp(0.0, 1.0)) / (settings.release.max(0.005) * sample_rate);
        let release_step = 1.0 / (settings.release.max(0.005) * sample_rate);
        let sustain = settings.sustain.clamp(0.0, 1.0);
        let release_decay = decay_coefficient(settings.release.max(0.05), sample_rate);
        let bend_ratio = 2.0_f32.powf(self.pitch_bend_semitones / 12.0);
        let piano = self.kind.is_piano();
        let bowed = self.kind.is_bowed();
        let plucked = self.kind.is_plucked();
        let level = settings.level.clamp(0.0, 1.5);

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
                if piano {
                    mix += voice.render_piano(attack_step, bend_ratio, release_decay, sample_rate);
                } else if plucked {
                    mix += voice.render_pluck(self.kind, &settings, bend_ratio);
                } else if bowed {
                    mix += voice.render_bow(
                        &settings,
                        bend_ratio,
                        attack_step,
                        decay_step,
                        release_step,
                        sustain,
                        sample_rate,
                    );
                }
            }

            mix *= level;
            *sample_out += mix.tanh() * 0.85;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：每种原生乐器在收到 Note On 后都能产生可听输出。
    /// Covers: every native instrument produces audible output after a note on.
    #[test]
    fn every_native_instrument_renders_audible_output() {
        for kind in NativeInstrumentKind::ALL {
            let settings = NativeInstrumentSettings::for_kind(kind);
            let mut instrument = ModeledInstrument::new(kind, settings, 48_000.0);
            let mut block = vec![0.0_f32; 512];
            let mut peak = 0.0_f32;
            for index in 0..40 {
                let events = if index == 0 {
                    vec![SynthEvent {
                        frame: 0,
                        kind: GateKind::On,
                        pitch: 60,
                        velocity: 100,
                    }]
                } else {
                    Vec::new()
                };
                instrument.render(&mut block, &events);
                peak = block
                    .iter()
                    .fold(peak, |peak, sample| peak.max(sample.abs()));
            }
            assert!(peak > 1e-4, "{} produced silence", kind.label());
        }
    }

    /// 覆盖：延音踏板会挂住 Note Off，抬起踏板后进入释放。
    /// Covers: the sustain pedal defers note-off until the pedal is released.
    #[test]
    fn sustain_pedal_defers_note_release() {
        let settings = NativeInstrumentSettings::for_kind(NativeInstrumentKind::StringLegato);
        let mut instrument =
            ModeledInstrument::new(NativeInstrumentKind::StringLegato, settings, 48_000.0);
        instrument.set_sustain(true);
        let mut block = vec![0.0_f32; 512];
        instrument.render(
            &mut block,
            &[SynthEvent {
                frame: 0,
                kind: GateKind::On,
                pitch: 60,
                velocity: 100,
            }],
        );
        instrument.render(
            &mut block,
            &[SynthEvent {
                frame: 0,
                kind: GateKind::Off,
                pitch: 60,
                velocity: 0,
            }],
        );
        instrument.render(&mut block, &[]);
        let held_peak = block
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(held_peak > 1e-5, "pedal should hold the note");
        instrument.set_sustain(false);
        for _ in 0..200 {
            instrument.render(&mut block, &[]);
        }
        let released_peak = block
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(
            released_peak < held_peak,
            "note should release after the pedal"
        );
    }

    /// 覆盖：全部原生乐器长音渲染保持有限值，共振器不会自激。
    /// Covers: long renders of every native instrument stay finite; resonators never self-oscillate.
    #[test]
    fn native_instruments_stay_finite_and_bounded() {
        for kind in NativeInstrumentKind::ALL {
            let settings = NativeInstrumentSettings::for_kind(kind);
            let mut instrument = ModeledInstrument::new(kind, settings, 48_000.0);
            let mut block = vec![0.0_f32; 512];
            for index in 0..200 {
                let events = if index == 0 {
                    vec![SynthEvent {
                        frame: 0,
                        kind: GateKind::On,
                        pitch: 60,
                        velocity: 110,
                    }]
                } else if index == 90 {
                    vec![SynthEvent {
                        frame: 0,
                        kind: GateKind::Off,
                        pitch: 60,
                        velocity: 0,
                    }]
                } else {
                    Vec::new()
                };
                instrument.render(&mut block, &events);
                for sample in &block {
                    assert!(sample.is_finite(), "{} produced NaN", kind.label());
                    assert!(
                        sample.abs() < 4.0,
                        "{} produced an unbounded sample: {sample}",
                        kind.label()
                    );
                }
            }
        }
    }
}
