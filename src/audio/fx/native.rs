//! 所属模块：audio / 内置效果器
//! 文件用途：
//! - 实现过载、失真、法兹、合唱、相位、延时、混响与压缩八个实时效果器。
//! - 所有延迟线/梳状滤波器在创建时一次性分配，音频回调内不做堆分配。
//!
//! 函数清单：
//! - `NativeEffectProcessor::new`：按类型创建效果器并预分配状态。
//! - `NativeEffectProcessor::set_settings`：同步工程中的归一化参数。
//! - `NativeEffectProcessor::process`：逐采样处理立体声输入。
//! - `process_drive` / `process_chorus` / `process_phaser` / `process_delay`：
//!   失真类、调制类与延时类 DSP。
//! - `process_reverb` / `process_compressor`：混响与压缩 DSP。
//! - `Comb::new` / `Comb::process`：混响梳状滤波器。
//! - `Allpass::new` / `Allpass::process`：混响全通滤波器。
//! - `allpass` / `lfo` / `soft_clip` / `db_to_gain`：DSP 辅助函数。
//!
//! Module: audio / built-in effects
//! Purpose:
//! - Implement overdrive, distortion, fuzz, chorus, phaser, delay, reverb and compressor.
//! - All delay lines and comb filters are allocated once at construction; the audio
//!   callback never allocates.
//!
//! Functions:
//! - `NativeEffectProcessor::new`: create an effect and pre-allocate its state.
//! - `NativeEffectProcessor::set_settings`: sync normalized project parameters.
//! - `NativeEffectProcessor::process`: per-sample stereo processing.
//! - `process_drive` / `process_chorus` / `process_phaser` / `process_delay`:
//!   saturation, modulation and delay DSP.
//! - `process_reverb` / `process_compressor`: reverb and compressor DSP.
//! - `Comb::new` / `Comb::process`: reverb comb filter.
//! - `Allpass::new` / `Allpass::process`: reverb all-pass filter.
//! - `allpass` / `lfo` / `soft_clip` / `db_to_gain`: DSP helpers.

use crate::core::model::{NativeEffectKind, NativeEffectSettings};
use std::f32::consts::PI;

/// 合唱所需的最大延迟（60 ms 余量）。
/// Maximum chorus delay (60 ms with headroom).
const CHORUS_MAX_SECONDS: f32 = 0.06;
/// 延时线长度上限（约 1 秒，覆盖 900 ms 最大延时）。
/// Delay-line capacity (about 1 second, covering the 900 ms maximum delay).
const DELAY_MAX_SECONDS: f32 = 1.0;

/// 单个效果器的内部 DSP 状态。
/// Internal DSP state of one effect.
enum State {
    Drive {
        lowpass_l: f32,
        lowpass_r: f32,
    },
    Chorus {
        buffer_l: Vec<f32>,
        buffer_r: Vec<f32>,
        write: usize,
        phase: f32,
    },
    Phaser {
        stages_l: [f32; 4],
        stages_r: [f32; 4],
        feedback_l: f32,
        feedback_r: f32,
        phase: f32,
    },
    Delay {
        buffer_l: Vec<f32>,
        buffer_r: Vec<f32>,
        write: usize,
        damp_l: f32,
        damp_r: f32,
    },
    Reverb {
        combs_l: Vec<Comb>,
        combs_r: Vec<Comb>,
        allpass_l: Vec<Allpass>,
        allpass_r: Vec<Allpass>,
    },
    Compressor {
        envelope: f32,
        gain_l: f32,
        gain_r: f32,
    },
}

/// 混响梳状滤波器。
/// Reverb comb filter.
struct Comb {
    buffer: Vec<f32>,
    index: usize,
    filtered: f32,
}

impl Comb {
    /// 所属模块：audio / 内置效果器
    /// 功能：创建指定长度的梳状滤波器。
    /// 输入：`length` 采样数。
    /// 输出/副作用：返回滤波器；分配 `length` 个采样。
    /// 复杂度：时间 O(length)，空间 O(length)。
    ///
    /// Module: audio / built-in effects
    /// Purpose: Create a comb filter with the given length.
    /// Inputs: `length` in samples.
    /// Outputs / side effects: the filter; allocates `length` samples.
    /// Complexity: O(length) time and space.
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(2)],
            index: 0,
            filtered: 0.0,
        }
    }

    /// 所属模块：audio / 内置效果器
    /// 功能：处理一个混响梳状滤波采样。
    /// 输入：`input` 输入；`feedback` 反馈量；`damping` 高频阻尼。
    /// 输出/副作用：返回滤波输出并推进缓冲。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / built-in effects
    /// Purpose: Process one comb-filter sample.
    /// Inputs: `input`, `feedback`, `damping`.
    /// Outputs / side effects: filtered sample; advances the buffer.
    /// Complexity: O(1) time and space.
    fn process(&mut self, input: f32, feedback: f32, damping: f32) -> f32 {
        let output = self.buffer[self.index];
        self.filtered = output * (1.0 - damping) + self.filtered * damping;
        self.buffer[self.index] = input + self.filtered * feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

/// 混响全通滤波器。
/// Reverb all-pass filter.
struct Allpass {
    buffer: Vec<f32>,
    index: usize,
}

impl Allpass {
    /// 所属模块：audio / 内置效果器
    /// 功能：创建指定长度的全通滤波器。
    /// 输入：`length` 采样数。
    /// 输出/副作用：返回滤波器；分配 `length` 个采样。
    /// 复杂度：时间 O(length)，空间 O(length)。
    ///
    /// Module: audio / built-in effects
    /// Purpose: Create an all-pass filter with the given length.
    /// Inputs: `length` in samples.
    /// Outputs / side effects: the filter; allocates `length` samples.
    /// Complexity: O(length) time and space.
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(2)],
            index: 0,
        }
    }

    /// 所属模块：audio / 内置效果器
    /// 功能：处理一个全通滤波采样。
    /// 输入：`input` 输入。
    /// 输出/副作用：返回全通输出并推进缓冲。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / built-in effects
    /// Purpose: Process one all-pass sample.
    /// Inputs: `input`.
    /// Outputs / side effects: all-pass output; advances the buffer.
    /// Complexity: O(1) time and space.
    fn process(&mut self, input: f32) -> f32 {
        let delayed = self.buffer[self.index];
        let output = -input + delayed;
        self.buffer[self.index] = input + delayed * 0.5;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

/// 内置效果器运行时。
/// Native-effect runtime.
pub struct NativeEffectProcessor {
    kind: NativeEffectKind,
    settings: NativeEffectSettings,
    sample_rate: f32,
    state: State,
}

impl NativeEffectProcessor {
    /// 所属模块：audio / 内置效果器
    /// 功能：按类型创建效果器并预分配延迟线。
    /// 输入：
    /// - `kind`：效果器类型。
    /// - `settings`：归一化参数。
    /// - `sample_rate`：采样率。
    /// 输出/副作用：返回处理器；分配延迟/混响缓冲。
    /// 复杂度：时间 O(延迟线长度之和)，空间相同。
    ///
    /// Module: audio / built-in effects
    /// Purpose: Create an effect and pre-allocate its delay lines.
    /// Inputs:
    /// - `kind`: effect kind.
    /// - `settings`: normalized parameters.
    /// - `sample_rate`: sample rate.
    /// Outputs / side effects: the processor; allocates delay/reverb buffers.
    /// Complexity: O(total delay-line length) time and space.
    pub fn new(kind: NativeEffectKind, settings: NativeEffectSettings, sample_rate: f32) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let state = match kind {
            NativeEffectKind::Overdrive | NativeEffectKind::Distortion | NativeEffectKind::Fuzz => {
                State::Drive {
                    lowpass_l: 0.0,
                    lowpass_r: 0.0,
                }
            }
            NativeEffectKind::Chorus => {
                let length = (sample_rate * CHORUS_MAX_SECONDS).ceil() as usize + 4;
                State::Chorus {
                    buffer_l: vec![0.0; length],
                    buffer_r: vec![0.0; length],
                    write: 0,
                    phase: 0.0,
                }
            }
            NativeEffectKind::Phaser => State::Phaser {
                stages_l: [0.0; 4],
                stages_r: [0.0; 4],
                feedback_l: 0.0,
                feedback_r: 0.0,
                phase: 0.0,
            },
            NativeEffectKind::Delay => {
                let length = (sample_rate * DELAY_MAX_SECONDS).ceil() as usize + 4;
                State::Delay {
                    buffer_l: vec![0.0; length],
                    buffer_r: vec![0.0; length],
                    write: 0,
                    damp_l: 0.0,
                    damp_r: 0.0,
                }
            }
            NativeEffectKind::Reverb => {
                let scale = sample_rate / 44_100.0;
                let comb_lengths = [1116usize, 1188, 1277, 1356];
                let allpass_lengths = [556usize, 441];
                State::Reverb {
                    combs_l: comb_lengths
                        .iter()
                        .map(|length| Comb::new((*length as f32 * scale) as usize))
                        .collect(),
                    combs_r: comb_lengths
                        .iter()
                        .map(|length| Comb::new((*length as f32 * scale) as usize + 23))
                        .collect(),
                    allpass_l: allpass_lengths
                        .iter()
                        .map(|length| Allpass::new((*length as f32 * scale) as usize))
                        .collect(),
                    allpass_r: allpass_lengths
                        .iter()
                        .map(|length| Allpass::new((*length as f32 * scale) as usize + 23))
                        .collect(),
                }
            }
            NativeEffectKind::Compressor => State::Compressor {
                envelope: 0.0,
                gain_l: 1.0,
                gain_r: 1.0,
            },
        };
        Self {
            kind,
            settings,
            sample_rate,
            state,
        }
    }

    /// 所属模块：audio / 内置效果器
    /// 功能：同步工程中的归一化参数；延迟线状态保持不变。
    /// 输入：`settings` 新参数。
    /// 输出/副作用：替换参数；不重置 DSP 状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / built-in effects
    /// Purpose: Sync normalized project parameters without resetting DSP state.
    /// Inputs: `settings` new parameters.
    /// Outputs / side effects: replaces the settings; DSP state is kept.
    /// Complexity: O(1) time and space.
    pub fn set_settings(&mut self, settings: NativeEffectSettings) {
        self.settings = settings;
    }

    /// 所属模块：audio / 内置效果器
    /// 功能：处理一个 block 的立体声信号。
    /// 输入：
    /// - `frames`：本块帧数。
    /// - `in_l` / `in_r`：输入缓冲。
    /// - `out_l` / `out_r`：输出缓冲（可与输入相同，调用方保证长度）。
    /// 输出/副作用：写入输出并推进 DSP 状态。
    /// 复杂度：时间 O(帧数)，空间 O(1)。
    ///
    /// Module: audio / built-in effects
    /// Purpose: Process one block of stereo audio.
    /// Inputs:
    /// - `frames`: frames in this block.
    /// - `in_l` / `in_r`: input buffers.
    /// - `out_l` / `out_r`: output buffers (may alias inputs; lengths are validated).
    /// Outputs / side effects: writes the output and advances DSP state.
    /// Complexity: O(frames) time and O(1) space.
    pub fn process(
        &mut self,
        frames: usize,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
    ) {
        let frames = frames.min(in_l.len()).min(in_r.len());
        let frames = frames.min(out_l.len()).min(out_r.len());
        let settings = self.settings.clone();
        let sample_rate = self.sample_rate;
        match self.kind {
            NativeEffectKind::Overdrive | NativeEffectKind::Distortion | NativeEffectKind::Fuzz => {
                process_drive(
                    &settings,
                    self.kind,
                    sample_rate,
                    &mut self.state,
                    frames,
                    in_l,
                    in_r,
                    out_l,
                    out_r,
                )
            }
            NativeEffectKind::Chorus => process_chorus(
                &settings,
                sample_rate,
                &mut self.state,
                frames,
                in_l,
                in_r,
                out_l,
                out_r,
            ),
            NativeEffectKind::Phaser => process_phaser(
                &settings,
                sample_rate,
                &mut self.state,
                frames,
                in_l,
                in_r,
                out_l,
                out_r,
            ),
            NativeEffectKind::Delay => process_delay(
                &settings,
                sample_rate,
                &mut self.state,
                frames,
                in_l,
                in_r,
                out_l,
                out_r,
            ),
            NativeEffectKind::Reverb => {
                process_reverb(&settings, &mut self.state, frames, in_l, in_r, out_l, out_r)
            }
            NativeEffectKind::Compressor => process_compressor(
                &settings,
                sample_rate,
                &mut self.state,
                frames,
                in_l,
                in_r,
                out_l,
                out_r,
            ),
        }
    }
}

/// 所属模块：audio / 内置效果器
/// 功能：过载/失真/法兹的饱和处理与音色低通。
/// 输入：
/// - `settings`：参数。
/// - `kind`：失真类型。
/// - `state`：drive 状态。
/// - `frames` / `in_l` / `in_r` / `out_l` / `out_r`：音频数据。
/// 输出/副作用：写入输出并更新低通状态。
/// 复杂度：时间 O(帧数)，空间 O(1)。
///
/// Module: audio / built-in effects
/// Purpose: Saturation and tone low-pass for overdrive, distortion and fuzz.
/// Inputs:
/// - `settings`, `kind`, `state`, audio buffers.
/// Outputs / side effects: writes the output and updates the low-pass state.
/// Complexity: O(frames) time and O(1) space.
#[allow(clippy::too_many_arguments)]
fn process_drive(
    settings: &NativeEffectSettings,
    kind: NativeEffectKind,
    sample_rate: f32,
    state: &mut State,
    frames: usize,
    in_l: &[f32],
    in_r: &[f32],
    out_l: &mut [f32],
    out_r: &mut [f32],
) {
    let State::Drive {
        lowpass_l,
        lowpass_r,
    } = state
    else {
        return;
    };
    let drive = settings.drive.clamp(0.0, 1.0);
    let mix = settings.mix.clamp(0.0, 1.0);
    let level = settings.level.clamp(0.0, 1.5);
    let cutoff = (500.0 + settings.tone.clamp(0.0, 1.0) * 11_000.0).clamp(40.0, sample_rate * 0.45);
    let coefficient = 1.0 - (-2.0 * PI * cutoff / sample_rate).exp();
    for frame in 0..frames {
        let dry_l = in_l[frame];
        let dry_r = in_r[frame];
        let shaped_l = shape(kind, dry_l, drive);
        let shaped_r = shape(kind, dry_r, drive);
        *lowpass_l += (shaped_l - *lowpass_l) * coefficient;
        *lowpass_r += (shaped_r - *lowpass_r) * coefficient;
        let wet_l = *lowpass_l * level;
        let wet_r = *lowpass_r * level;
        out_l[frame] = dry_l * (1.0 - mix) + wet_l * mix;
        out_r[frame] = dry_r * (1.0 - mix) + wet_r * mix;
    }
}

/// 所属模块：audio / 内置效果器
/// 功能：按失真类型对单采样做非线性整形。
/// 输入：`kind` 类型；`sample` 输入；`drive` 0..1 驱动。
/// 输出/副作用：整形后的采样；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / built-in effects
/// Purpose: Apply the non-linear curve of one saturation type.
/// Inputs: `kind`, `sample`, `drive` in 0..1.
/// Outputs / side effects: shaped sample; no state change.
/// Complexity: O(1) time and space.
fn shape(kind: NativeEffectKind, sample: f32, drive: f32) -> f32 {
    match kind {
        NativeEffectKind::Overdrive => {
            let gain = 1.0 + drive * 7.0;
            (sample * gain).tanh() / gain.sqrt().max(1.0)
        }
        NativeEffectKind::Distortion => {
            let gain = 1.0 + drive * 24.0;
            let pushed = sample * gain;
            soft_clip(pushed) * 0.9
        }
        NativeEffectKind::Fuzz => {
            let gain = 1.0 + drive * 40.0;
            let biased = (sample * gain + 0.18 * drive).tanh() - (0.18 * drive).tanh();
            // 非对称削波产生法兹特有的偶次谐波。
            // Asymmetric clipping creates the even harmonics typical of fuzz.
            (biased * 1.4).tanh() * 0.7
        }
        _ => sample,
    }
}

/// 所属模块：audio / 内置效果器
/// 功能：调制延迟合唱，左右声道使用 90° 相位差的 LFO。
/// 输入：参数、采样率、状态与音频缓冲。
/// 输出/副作用：写入输出并推进延迟线。
/// 复杂度：时间 O(帧数)，空间 O(1)。
///
/// Module: audio / built-in effects
/// Purpose: Modulated-delay chorus with a 90° LFO offset between channels.
/// Inputs: settings, sample rate, state and audio buffers.
/// Outputs / side effects: writes the output and advances the delay line.
/// Complexity: O(frames) time and O(1) space.
#[allow(clippy::too_many_arguments)]
fn process_chorus(
    settings: &NativeEffectSettings,
    sample_rate: f32,
    state: &mut State,
    frames: usize,
    in_l: &[f32],
    in_r: &[f32],
    out_l: &mut [f32],
    out_r: &mut [f32],
) {
    let State::Chorus {
        buffer_l,
        buffer_r,
        write,
        phase,
    } = state
    else {
        return;
    };
    let rate = 0.05 + settings.rate.clamp(0.0, 1.0) * 7.95;
    let depth = settings.depth.clamp(0.0, 1.0);
    let mix = settings.mix.clamp(0.0, 1.0);
    let level = settings.level.clamp(0.0, 1.5);
    let base = 0.009 * sample_rate;
    let swing = 0.016 * sample_rate * depth;
    let length = buffer_l.len();
    for frame in 0..frames {
        let lfo_l = lfo(*phase);
        let lfo_r = lfo(*phase + 0.25);
        let delay_l = base + swing * (lfo_l * 0.5 + 0.5);
        let delay_r = base + swing * (lfo_r * 0.5 + 0.5);
        let read_l = read_delay(buffer_l, *write, delay_l);
        let read_r = read_delay(buffer_r, *write, delay_r);
        buffer_l[*write] = in_l[frame];
        buffer_r[*write] = in_r[frame];
        *write = (*write + 1) % length;
        *phase += rate / sample_rate;
        if *phase >= 1.0 {
            *phase -= 1.0;
        }
        out_l[frame] = in_l[frame] * (1.0 - mix) + read_l * mix * level;
        out_r[frame] = in_r[frame] * (1.0 - mix) + read_r * mix * level;
    }
}

/// 所属模块：audio / 内置效果器
/// 功能：四级全通相位器，带反馈与 LFO 扫频。
/// 输入：参数、采样率、状态与音频缓冲。
/// 输出/副作用：写入输出并推进全通状态。
/// 复杂度：时间 O(帧数 × 4)，空间 O(1)。
///
/// Module: audio / built-in effects
/// Purpose: Four-stage all-pass phaser with feedback and LFO sweep.
/// Inputs: settings, sample rate, state and audio buffers.
/// Outputs / side effects: writes the output and advances the all-pass state.
/// Complexity: O(frames × 4) time and O(1) space.
#[allow(clippy::too_many_arguments)]
fn process_phaser(
    settings: &NativeEffectSettings,
    sample_rate: f32,
    state: &mut State,
    frames: usize,
    in_l: &[f32],
    in_r: &[f32],
    out_l: &mut [f32],
    out_r: &mut [f32],
) {
    let State::Phaser {
        stages_l,
        stages_r,
        feedback_l,
        feedback_r,
        phase,
    } = state
    else {
        return;
    };
    let rate = 0.05 + settings.rate.clamp(0.0, 1.0) * 7.95;
    let depth = settings.depth.clamp(0.0, 1.0);
    let mix = settings.mix.clamp(0.0, 1.0);
    let feedback = settings.feedback.clamp(0.0, 0.95);
    let level = settings.level.clamp(0.0, 1.5);
    for frame in 0..frames {
        let lfo_l = lfo(*phase) * 0.5 + 0.5;
        let lfo_r = lfo(*phase + 0.25) * 0.5 + 0.5;
        let cutoff_l = 180.0 + depth * 1800.0 * lfo_l;
        let cutoff_r = 180.0 + depth * 1800.0 * lfo_r;
        let coefficient_l = phaser_coefficient(cutoff_l, sample_rate);
        let coefficient_r = phaser_coefficient(cutoff_r, sample_rate);
        let input_l = in_l[frame] + *feedback_l * feedback;
        let input_r = in_r[frame] + *feedback_r * feedback;
        let mut wet_l = input_l;
        let mut wet_r = input_r;
        for stage in 0..4 {
            wet_l = allpass(wet_l, coefficient_l, &mut stages_l[stage]);
            wet_r = allpass(wet_r, coefficient_r, &mut stages_r[stage]);
        }
        *feedback_l = wet_l;
        *feedback_r = wet_r;
        *phase += rate / sample_rate;
        if *phase >= 1.0 {
            *phase -= 1.0;
        }
        let wet_l = wet_l * level;
        let wet_r = wet_r * level;
        out_l[frame] = in_l[frame] * (1.0 - mix) + wet_l * mix;
        out_r[frame] = in_r[frame] * (1.0 - mix) + wet_r * mix;
    }
}

/// 所属模块：audio / 内置效果器
/// 功能：反馈延时，带高频阻尼。
/// 输入：参数、采样率、状态与音频缓冲。
/// 输出/副作用：写入输出并推进延迟线。
/// 复杂度：时间 O(帧数)，空间 O(1)。
///
/// Module: audio / built-in effects
/// Purpose: Feedback delay with high-frequency damping.
/// Inputs: settings, sample rate, state and audio buffers.
/// Outputs / side effects: writes the output and advances the delay line.
/// Complexity: O(frames) time and O(1) space.
#[allow(clippy::too_many_arguments)]
fn process_delay(
    settings: &NativeEffectSettings,
    sample_rate: f32,
    state: &mut State,
    frames: usize,
    in_l: &[f32],
    in_r: &[f32],
    out_l: &mut [f32],
    out_r: &mut [f32],
) {
    let State::Delay {
        buffer_l,
        buffer_r,
        write,
        damp_l,
        damp_r,
    } = state
    else {
        return;
    };
    let delay_seconds = 0.04 + settings.time.clamp(0.0, 1.0) * 0.86;
    let delay_samples = delay_seconds * sample_rate;
    let feedback = settings.feedback.clamp(0.0, 0.95);
    let mix = settings.mix.clamp(0.0, 1.0);
    let level = settings.level.clamp(0.0, 1.5);
    let tone = 0.05 + settings.tone.clamp(0.0, 1.0) * 0.94;
    let length = buffer_l.len();
    for frame in 0..frames {
        let read_l = read_delay(buffer_l, *write, delay_samples);
        let read_r = read_delay(buffer_r, *write, delay_samples);
        *damp_l += (read_l - *damp_l) * tone;
        *damp_r += (read_r - *damp_r) * tone;
        buffer_l[*write] = in_l[frame] + *damp_l * feedback;
        buffer_r[*write] = in_r[frame] + *damp_r * feedback;
        *write = (*write + 1) % length;
        out_l[frame] = in_l[frame] * (1.0 - mix) + read_l * mix * level;
        out_r[frame] = in_r[frame] * (1.0 - mix) + read_r * mix * level;
    }
}

/// 所属模块：audio / 内置效果器
/// 功能：Schroeder 风格立体声混响（梳状 + 全通）。
/// 输入：参数、状态与音频缓冲。
/// 输出/副作用：写入输出并推进梳状/全通缓冲。
/// 复杂度：时间 O(帧数 × 12)，空间 O(1)。
///
/// Module: audio / built-in effects
/// Purpose: Schroeder-style stereo reverb (comb + all-pass).
/// Inputs: settings, state and audio buffers.
/// Outputs / side effects: writes the output; advances comb/all-pass buffers.
/// Complexity: O(frames × 12) time and O(1) space.
fn process_reverb(
    settings: &NativeEffectSettings,
    state: &mut State,
    frames: usize,
    in_l: &[f32],
    in_r: &[f32],
    out_l: &mut [f32],
    out_r: &mut [f32],
) {
    let State::Reverb {
        combs_l,
        combs_r,
        allpass_l,
        allpass_r,
    } = state
    else {
        return;
    };
    let feedback = 0.62 + settings.time.clamp(0.0, 1.0) * 0.32;
    let damping = 0.18 + (1.0 - settings.tone.clamp(0.0, 1.0)) * 0.62;
    let mix = settings.mix.clamp(0.0, 1.0);
    let level = settings.level.clamp(0.0, 1.5);
    for frame in 0..frames {
        let input = (in_l[frame] + in_r[frame]) * 0.5;
        let mut wet_l = 0.0;
        let mut wet_r = 0.0;
        for comb in combs_l.iter_mut() {
            wet_l += comb.process(input, feedback, damping);
        }
        for comb in combs_r.iter_mut() {
            wet_r += comb.process(input, feedback, damping);
        }
        wet_l /= combs_l.len() as f32;
        wet_r /= combs_r.len() as f32;
        for stage in allpass_l.iter_mut() {
            wet_l = stage.process(wet_l);
        }
        for stage in allpass_r.iter_mut() {
            wet_r = stage.process(wet_r);
        }
        let wet_l = wet_l * level * 0.7;
        let wet_r = wet_r * level * 0.7;
        out_l[frame] = in_l[frame] * (1.0 - mix) + wet_l * mix;
        out_r[frame] = in_r[frame] * (1.0 - mix) + wet_r * mix;
    }
}

/// 所属模块：audio / 内置效果器
/// 功能：立体声联动峰值压缩器。
/// 输入：参数、采样率、状态与音频缓冲。
/// 输出/副作用：写入输出并更新包络与增益。
/// 复杂度：时间 O(帧数)，空间 O(1)。
///
/// Module: audio / built-in effects
/// Purpose: Stereo-linked peak compressor.
/// Inputs: settings, sample rate, state and audio buffers.
/// Outputs / side effects: writes the output and updates envelope/gain.
/// Complexity: O(frames) time and O(1) space.
#[allow(clippy::too_many_arguments)]
fn process_compressor(
    settings: &NativeEffectSettings,
    sample_rate: f32,
    state: &mut State,
    frames: usize,
    in_l: &[f32],
    in_r: &[f32],
    out_l: &mut [f32],
    out_r: &mut [f32],
) {
    let State::Compressor {
        envelope,
        gain_l,
        gain_r,
    } = state
    else {
        return;
    };
    let threshold_db = -48.0 + settings.threshold.clamp(0.0, 1.0) * 48.0;
    let ratio = 1.0 + settings.ratio.clamp(0.0, 1.0) * 19.0;
    let attack_seconds = 0.001 + settings.attack.clamp(0.0, 1.0) * 0.099;
    let release_seconds = 0.02 + settings.release.clamp(0.0, 1.0) * 1.98;
    let attack = (-1.0 / (attack_seconds * sample_rate)).exp();
    let release = (-1.0 / (release_seconds * sample_rate)).exp();
    let makeup = 0.4 + settings.level.clamp(0.0, 1.0) * 1.2;
    for frame in 0..frames {
        let peak = in_l[frame].abs().max(in_r[frame].abs());
        let coefficient = if peak > *envelope { attack } else { release };
        *envelope = peak + coefficient * (*envelope - peak);
        let envelope_db = 20.0 * envelope.max(1e-6).log10();
        let reduction_db = if envelope_db > threshold_db {
            (threshold_db + (envelope_db - threshold_db) / ratio) - envelope_db
        } else {
            0.0
        };
        let target_gain = db_to_gain(reduction_db);
        // 增益平滑，避免快速起音造成的“抽吸”爆音。
        // Smooth the gain to avoid pumping artefacts from fast attacks.
        let gain_coefficient = if target_gain < *gain_l {
            attack
        } else {
            release
        };
        *gain_l = target_gain + gain_coefficient * (*gain_l - target_gain);
        *gain_r = *gain_l;
        out_l[frame] = in_l[frame] * *gain_l * makeup;
        out_r[frame] = in_r[frame] * *gain_r * makeup;
    }
}

/// 所属模块：audio / 内置效果器
/// 功能：读取分数延迟采样。
/// 输入：
/// - `buffer`：延迟线。
/// - `write`：当前写入位置。
/// - `delay`：延迟采样数。
/// 输出/副作用：插值后的采样；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / built-in effects
/// Purpose: Read a fractional-delay sample.
/// Inputs:
/// - `buffer`: delay line.
/// - `write`: current write index.
/// - `delay`: delay in samples.
/// Outputs / side effects: interpolated sample; no state change.
/// Complexity: O(1) time and space.
fn read_delay(buffer: &[f32], write: usize, delay: f32) -> f32 {
    let length = buffer.len();
    let delay = delay.clamp(1.0, (length - 3) as f32);
    let read = write as f32 - delay;
    let read = if read < 0.0 {
        read + length as f32
    } else {
        read
    };
    let index = (read.floor() as usize) % length;
    let next = (index + 1) % length;
    let fraction = read - read.floor();
    buffer[index] * (1.0 - fraction) + buffer[next] * fraction
}

/// 所属模块：audio / 内置效果器
/// 功能：一阶全通滤波。
/// 输入：`input`、`coefficient`、`state`。
/// 输出/副作用：返回输出并更新状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / built-in effects
/// Purpose: First-order all-pass filter.
/// Inputs: `input`, `coefficient`, `state`.
/// Outputs / side effects: output and updated state.
/// Complexity: O(1) time and space.
fn allpass(input: f32, coefficient: f32, state: &mut f32) -> f32 {
    let output = -coefficient * input + *state;
    *state = input + coefficient * output;
    output
}

/// 所属模块：audio / 内置效果器
/// 功能：把相位器频率换算为一阶全通系数。
/// 输入：`cutoff` Hz、`sample_rate`。
/// 输出/副作用：全通系数；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / built-in effects
/// Purpose: Convert a phaser frequency into a first-order all-pass coefficient.
/// Inputs: `cutoff` in Hz, `sample_rate`.
/// Outputs / side effects: coefficient; no state change.
/// Complexity: O(1) time and space.
fn phaser_coefficient(cutoff: f32, sample_rate: f32) -> f32 {
    let tangent = (PI * cutoff.clamp(20.0, sample_rate * 0.45) / sample_rate).tan();
    (tangent - 1.0) / (tangent + 1.0)
}

/// 所属模块：audio / 内置效果器
/// 功能：正弦 LFO（相位 0..1）。
/// 输入：`phase` 归一化相位。
/// 输出/副作用：[-1, 1] 的值；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / built-in effects
/// Purpose: Sine LFO over a normalized phase.
/// Inputs: `phase` in 0..1.
/// Outputs / side effects: value in [-1, 1]; no state change.
/// Complexity: O(1) time and space.
fn lfo(phase: f32) -> f32 {
    (phase * 2.0 * PI).sin()
}

/// 所属模块：audio / 内置效果器
/// 功能：硬拐点软削波，避免数字过载。
/// 输入：`sample`。
/// 输出/副作用：限制后的采样；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / built-in effects
/// Purpose: Soft clipping to avoid digital overload.
/// Inputs: `sample`.
/// Outputs / side effects: limited sample; no state change.
/// Complexity: O(1) time and space.
fn soft_clip(sample: f32) -> f32 {
    sample / (1.0 + sample.abs())
}

/// 所属模块：audio / 内置效果器
/// 功能：分贝转线性增益。
/// 输入：`db` 分贝。
/// 输出/副作用：线性增益；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / built-in effects
/// Purpose: Convert decibels to linear gain.
/// Inputs: `db` decibels.
/// Outputs / side effects: linear gain; no state change.
/// Complexity: O(1) time and space.
fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：每种内置效果器都能处理信号且不产生 NaN/Inf。
    /// Covers: every native effect processes audio without NaN/Inf.
    #[test]
    fn every_native_effect_processes_without_nan() {
        for kind in NativeEffectKind::ALL {
            let settings = NativeEffectSettings::for_kind(kind);
            let mut processor = NativeEffectProcessor::new(kind, settings, 48_000.0);
            let frames = 1024;
            let input_l: Vec<f32> = (0..frames)
                .map(|index| (index as f32 * 0.05).sin() * 0.3)
                .collect();
            let input_r = input_l.clone();
            let mut out_l = vec![0.0; frames];
            let mut out_r = vec![0.0; frames];
            processor.process(frames, &input_l, &input_r, &mut out_l, &mut out_r);
            let peak = out_l
                .iter()
                .chain(out_r.iter())
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
            assert!(
                peak.is_finite(),
                "{} produced non-finite output",
                kind.label()
            );
            assert!(peak > 1e-6, "{} produced silence", kind.label());
        }
    }
}
