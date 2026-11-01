//! 所属模块：audio / 神经音色模型
//! 文件用途：
//! - 用采样音色库自身的数据训练一个极小的多层感知机（MLP），学习「音高 / 力度 / 时间 → 频谱形状」的映射。
//! - 实时渲染时按差值修正采样频谱，补偿变速变调的共振峰偏移、力度层不足与循环段“停滞”的听感。
//! - 提供逐 voice 的三段音色整形器，把模型输出折算成低架 / 中峰 / 高架增益。
//!
//! 函数清单：
//! - `NeuralTimbre::train`：从清单乐器的分区采样提取特征并训练网络。
//! - `NeuralTimbre::voice_gains`：实时推理给定音高/力度/时间的音色增益（dB，无分配）。
//! - `NeuralTimbre::correction`：推理 6 段相对频谱形状并返回相对参考点的差值。
//! - `NeuralTimbre::note_jitter`：按音符序号给出建模残差范围内的确定性抖动。
//! - `model_for`：按乐器 id 获取并缓存已训练模型。
//! - `ToneShaper::new` / `update` / `process` / `reset`：逐 voice 音色整形器。
//! - `Biquad::set_low_shelf` / `set_high_shelf` / `set_peaking` / `process`：RBJ 双二阶滤波器。
//! - `TinyMlp::forward` / `TinyMlp::train`：前向推理与带动量的 SGD 训练。
//! - `analyse_zone` / `band_shape` / `fft` / `hann` / `next_random` / `unit_hash`：特征提取与确定性随机。
//!
//! Module: audio / neural timbre model
//! Purpose:
//! - Train a tiny multi-layer perceptron (MLP) on the sampled bank's own data, learning the map
//!   from pitch / velocity / time to a spectral shape.
//! - At render time, correct the sample spectrum by the model difference so varispeed formant
//!   drift, missing velocity layers and the "frozen" loop section are compensated.
//! - Provide a per-voice three-band tone shaper that turns model output into low-shelf, peaking
//!   and high-shelf gains.
//!
//! Functions:
//! - `NeuralTimbre::train`: extract features from the manifest instrument's zones and train.
//! - `NeuralTimbre::voice_gains`: realtime inference of tone gains in dB without allocation.
//! - `NeuralTimbre::correction`: infer the six-band relative spectral shape and return the deltas.
//! - `NeuralTimbre::note_jitter`: deterministic per-note jitter scaled by the training residual.
//! - `model_for`: fetch and cache the trained model of an instrument id.
//! - `ToneShaper::new` / `update` / `process` / `reset`: per-voice tone shaper.
//! - `Biquad::set_low_shelf` / `set_high_shelf` / `set_peaking` / `process`: RBJ biquads.
//! - `TinyMlp::forward` / `TinyMlp::train`: forward inference and SGD training with momentum.
//! - `analyse_zone` / `band_shape` / `fft` / `hann` / `next_random` / `unit_hash`: features.

use super::sampled::load_sample;
use crate::core::model::BankInstrument;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// 频谱形状的频段数；训练目标与实时修正共用。
/// Number of spectral bands; shared by the training target and the realtime correction.
pub const BAND_COUNT: usize = 6;

/// 输入特征数：音高、力度、时间、偏置。
/// Input feature count: pitch, velocity, time, bias.
pub const FEATURE_COUNT: usize = 4;

/// 两个隐藏层的宽度；固定宽度让实时推理可以用栈上数组，完全无分配。
/// Width of both hidden layers; fixed so realtime inference can use stack arrays only.
const HIDDEN_1: usize = 10;
const HIDDEN_2: usize = 10;

/// 分析窗长度（2 的幂，供基 2 FFT 使用）。
/// Analysis window length (power of two for the radix-2 FFT).
const FFT_SIZE: usize = 256;

/// 每个分区取样的分析时间点（相对采样长度的比例）。
/// Analysis positions per zone sample, as a fraction of the sample length.
const ANALYSIS_POSITIONS: [f32; 4] = [0.08, 0.25, 0.45, 0.7];

/// 频段边界（FFT 序号）；随采样率变化只改变频率，不改变相对形状。
/// Band edges in FFT bins; the sample rate only changes frequencies, not relative shape.
const BAND_EDGES: [usize; BAND_COUNT + 1] = [1, 3, 6, 12, 24, 48, 96];

/// 训练轮数、Adam 超参数与权重衰减。
/// Training epochs, Adam hyper-parameters and weight decay.
const TRAIN_EPOCHS: usize = 700;
const ADAM_RATE: f32 = 0.012;
const ADAM_BETA_1: f32 = 0.9;
const ADAM_BETA_2: f32 = 0.999;
const ADAM_EPSILON: f32 = 1e-8;
const TRAIN_DECAY: f32 = 1e-4;

/// 相对频谱形状（对数能量差）到 dB 的换算系数。
/// Conversion factor from relative log band energy to decibels.
const BAND_TO_DB: f32 = 4.0;

/// 单段修正上限；超过该值说明模型外推过远，直接夹取比让滤波器失稳安全。
/// Per-band correction limit; clamping is safer than letting an extrapolating filter misbehave.
pub const MAX_GAIN_DB: f32 = 9.0;

/// 每个 voice 的增益平滑系数：每块向目标靠近一次，避免滤波器系数跳变产生“台阶”。
/// Per-block gain smoothing: coefficients must not jump, otherwise a block-rate step is audible.
const GAIN_SMOOTHING: f32 = 0.35;

/// 低架 / 中峰 / 高架的固定转折频率（Hz）。
/// Fixed corner frequencies for the low shelf, peaking band and high shelf.
const LOW_SHELF_HZ: f32 = 220.0;
const PEAK_HZ: f32 = 1_400.0;
const HIGH_SHELF_HZ: f32 = 5_200.0;
const PEAK_Q: f32 = 0.9;
/// 架式滤波器的品质因数（RBJ 推荐值）。
/// Quality factor of the shelf filters (RBJ recommendation).
const SHELF_Q: f32 = 0.7071;

/// 每个音符的最大亮度抖动（dB）；乘以训练残差后得到实际幅度。
/// Maximum per-note brightness jitter in dB; scaled by the training residual.
const JITTER_DB: f32 = 0.9;

/// 每个音符的最大音量抖动（dB）。
/// Maximum per-note level jitter in dB.
const JITTER_LEVEL_DB: f32 = 0.3;

/// 参考点特征：与分区根音重合时修正量为 0，原始采样音色不被改动。
/// Reference features: the correction is zero when the played note matches the root, so the
/// original sample timbre is preserved.
const REFERENCE_PITCH: f32 = 60.0;
const REFERENCE_VELOCITY: f32 = 0.62;
const REFERENCE_TIME: f32 = 0.35;

/// 时间特征的时间尺度（秒）。
/// Time scale of the time feature, in seconds.
const TIME_SCALE: f32 = 0.8;

/// 训练集最少帧数；不足时模型无意义，直接不使用。
/// Minimum training frames; below this the model is meaningless and is not built.
const MIN_TRAINING_FRAMES: usize = 8;

/// 一个极小的多层感知机：`FEATURE_COUNT → HIDDEN_1 → HIDDEN_2 → BAND_COUNT`。
/// A tiny MLP: `FEATURE_COUNT → HIDDEN_1 → HIDDEN_2 → BAND_COUNT`.
struct TinyMlp {
    w1: Vec<f32>,
    b1: Vec<f32>,
    w2: Vec<f32>,
    b2: Vec<f32>,
    w3: Vec<f32>,
    b3: Vec<f32>,
    /// 训练结束时的均方误差（对数频谱单位的平方）。
    /// Mean squared error after training (squared log-spectral units).
    residual: f32,
}

impl TinyMlp {
    /// 所属模块：audio / 神经音色模型
    /// 功能：用确定性的伪随机初值创建网络，避免每次启动得到不同音色。
    /// 输入：`seed` 伪随机种子。
    /// 输出/副作用：返回未训练的网络；不修改外部状态。
    /// 复杂度：时间 O(参数量)，空间 O(参数量)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Create a network with deterministic pseudo-random initial weights so the timbre is
    /// stable across runs.
    /// Inputs: `seed` pseudo-random seed.
    /// Outputs / side effects: an untrained network; no external state change.
    /// Complexity: O(parameters) time and space.
    fn new(seed: u32) -> Self {
        let mut state = seed | 1;
        let mut init = |count: usize, fan_in: usize| -> Vec<f32> {
            let scale = 1.0 / (fan_in as f32).sqrt();
            (0..count)
                .map(|_| (next_random(&mut state) * 2.0 - 1.0) * scale * 1.6)
                .collect()
        };
        Self {
            w1: init(HIDDEN_1 * FEATURE_COUNT, FEATURE_COUNT),
            b1: vec![0.0; HIDDEN_1],
            w2: init(HIDDEN_2 * HIDDEN_1, HIDDEN_1),
            b2: vec![0.0; HIDDEN_2],
            w3: init(BAND_COUNT * HIDDEN_2, HIDDEN_2),
            b3: vec![0.0; BAND_COUNT],
            residual: 0.0,
        }
    }

    /// 所属模块：audio / 神经音色模型
    /// 功能：一次完整前向计算，并把两层隐藏激活一并写出供反向传播复用。
    /// 输入：`input` 4 维特征、`hidden_1` / `hidden_2` 隐藏层输出、`out` 6 段频谱形状。
    /// 输出/副作用：写入三个输出缓冲；不分配内存、不修改网络，可实时调用。
    /// 复杂度：时间 O(FEATURE_COUNT×HIDDEN_1 + HIDDEN_1×HIDDEN_2 + HIDDEN_2×BAND_COUNT)，空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: One full forward pass that also writes both hidden activations for backprop.
    /// Inputs: `input` four features, `hidden_1` / `hidden_2` hidden outputs, `out` six-band shape.
    /// Outputs / side effects: writes the three buffers; no allocation and no mutation, realtime safe.
    /// Complexity: O(FEATURE_COUNT×HIDDEN_1 + HIDDEN_1×HIDDEN_2 + HIDDEN_2×BAND_COUNT) time,
    /// O(1) space.
    fn activations(
        &self,
        input: &[f32; FEATURE_COUNT],
        hidden_1: &mut [f32; HIDDEN_1],
        hidden_2: &mut [f32; HIDDEN_2],
        out: &mut [f32; BAND_COUNT],
    ) {
        for (unit, value) in hidden_1.iter_mut().enumerate() {
            let mut sum = self.b1[unit];
            for (index, feature) in input.iter().enumerate() {
                sum += self.w1[unit * FEATURE_COUNT + index] * feature;
            }
            *value = sum.tanh();
        }
        for (unit, value) in hidden_2.iter_mut().enumerate() {
            let mut sum = self.b2[unit];
            for (index, previous) in hidden_1.iter().enumerate() {
                sum += self.w2[unit * HIDDEN_1 + index] * previous;
            }
            *value = sum.tanh();
        }
        for (unit, value) in out.iter_mut().enumerate() {
            let mut sum = self.b3[unit];
            for (index, previous) in hidden_2.iter().enumerate() {
                sum += self.w3[unit * HIDDEN_2 + index] * previous;
            }
            *value = sum;
        }
    }

    /// 所属模块：audio / 神经音色模型
    /// 功能：只返回网络输出的前向推理，隐藏激活留在栈上。
    /// 输入：`input` 4 维特征、`out` 6 段频谱形状。
    /// 输出/副作用：写入 `out`；不分配内存、不修改网络。
    /// 复杂度：与 `activations` 相同。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Inference that only returns the network output, keeping activations on the stack.
    /// Inputs: `input` four features, `out` six-band shape.
    /// Outputs / side effects: writes `out`; no allocation and no mutation.
    /// Complexity: same as `activations`.
    fn forward(&self, input: &[f32; FEATURE_COUNT], out: &mut [f32; BAND_COUNT]) {
        let mut hidden_1 = [0.0_f32; HIDDEN_1];
        let mut hidden_2 = [0.0_f32; HIDDEN_2];
        self.activations(input, &mut hidden_1, &mut hidden_2, out);
    }

    /// 所属模块：audio / 神经音色模型
    /// 功能：在给定样本上做带动量的 SGD 训练：先按频段标准化目标，训练后再把输出层换算回原始量纲。
    /// 输入：`data`（特征, 目标频谱形状）样本、`seed` 洗牌种子。
    /// 输出/副作用：更新网络权重并记录最终均方误差；不修改样本。
    /// 复杂度：时间 O(训练轮数 × 样本数 × 参数量)，空间 O(参数量 + 样本数)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Train with momentum SGD: standardize the band targets first, then fold the
    /// normalisation back into the output layer so inference returns raw log-spectral units.
    /// Inputs: `data` (features, target shape) samples, `seed` shuffle seed.
    /// Outputs / side effects: updates the weights and stores the final MSE; samples are untouched.
    /// Complexity: O(epochs × samples × parameters) time, O(parameters + samples) space.
    fn train(&mut self, data: &[([f32; FEATURE_COUNT], [f32; BAND_COUNT])], seed: u32) {
        if data.is_empty() {
            return;
        }
        // 目标标准化：各频段对数能量差异很大，不标准化时学习率稍大就会发散。
        // Standardize targets: band log-energies differ a lot, and without normalisation even a
        // moderate learning rate diverges.
        let mut mean = [0.0_f32; BAND_COUNT];
        for (_, target) in data {
            for band in 0..BAND_COUNT {
                mean[band] += target[band];
            }
        }
        for value in &mut mean {
            *value /= data.len() as f32;
        }
        let mut scale = [1.0_f32; BAND_COUNT];
        for band in 0..BAND_COUNT {
            let mut variance = 0.0;
            for (_, target) in data {
                let delta = target[band] - mean[band];
                variance += delta * delta;
            }
            scale[band] = (variance / data.len() as f32).sqrt().max(1e-3);
        }
        let normalised: Vec<([f32; FEATURE_COUNT], [f32; BAND_COUNT])> = data
            .iter()
            .map(|(input, target)| {
                let mut scaled = [0.0_f32; BAND_COUNT];
                for band in 0..BAND_COUNT {
                    scaled[band] = (target[band] - mean[band]) / scale[band];
                }
                (*input, scaled)
            })
            .collect();
        let mut order: Vec<usize> = (0..normalised.len()).collect();
        let mut state = seed | 1;
        let mut adam_w1 = AdamState::new(self.w1.len());
        let mut adam_b1 = AdamState::new(self.b1.len());
        let mut adam_w2 = AdamState::new(self.w2.len());
        let mut adam_b2 = AdamState::new(self.b2.len());
        let mut adam_w3 = AdamState::new(self.w3.len());
        let mut adam_b3 = AdamState::new(self.b3.len());
        for _ in 0..TRAIN_EPOCHS {
            // Fisher-Yates 洗牌：固定种子保证同一乐器每次训练得到同样的模型。
            // Fisher-Yates shuffle: a fixed seed yields the same model for the same instrument.
            for index in (1..order.len()).rev() {
                let swap = (next_random(&mut state) * (index + 1) as f32) as usize;
                let swap = swap.min(index);
                order.swap(index, swap);
            }
            let mut loss = 0.0_f32;
            for &sample in &order {
                let (input, target) = normalised[sample];
                let mut hidden_1 = [0.0_f32; HIDDEN_1];
                let mut hidden_2 = [0.0_f32; HIDDEN_2];
                let mut output = [0.0_f32; BAND_COUNT];
                self.activations(&input, &mut hidden_1, &mut hidden_2, &mut output);
                let mut delta_out = [0.0_f32; BAND_COUNT];
                for band in 0..BAND_COUNT {
                    let error = output[band] - target[band];
                    loss += error * error;
                    delta_out[band] = error;
                }
                // 隐藏层用 tanh 导数回传；输出层线性，因此误差直接就是梯度。
                // Hidden layers backpropagate through the tanh derivative; the output layer is
                // linear, so the error itself is the gradient.
                let mut delta_2 = [0.0_f32; HIDDEN_2];
                for (unit, delta) in delta_2.iter_mut().enumerate() {
                    let mut sum = 0.0;
                    for band in 0..BAND_COUNT {
                        sum += delta_out[band] * self.w3[band * HIDDEN_2 + unit];
                    }
                    *delta = sum * (1.0 - hidden_2[unit] * hidden_2[unit]);
                }
                let mut delta_1 = [0.0_f32; HIDDEN_1];
                for (unit, delta) in delta_1.iter_mut().enumerate() {
                    let mut sum = 0.0;
                    for previous in 0..HIDDEN_2 {
                        sum += delta_2[previous] * self.w2[previous * HIDDEN_1 + unit];
                    }
                    *delta = sum * (1.0 - hidden_1[unit] * hidden_1[unit]);
                }

                // 每个样本推进一次优化步，六组参数共享同一步数与偏差修正。
                // Advance one optimisation step per sample; all six parameter sets share the step.
                adam_w1.advance();
                adam_b1.advance();
                adam_w2.advance();
                adam_b2.advance();
                adam_w3.advance();
                adam_b3.advance();
                adam_matrix(
                    &mut adam_w3,
                    &mut self.w3,
                    &delta_out,
                    &hidden_2,
                    BAND_COUNT,
                    HIDDEN_2,
                );
                adam_bias(&mut adam_b3, &mut self.b3, &delta_out);
                adam_matrix(
                    &mut adam_w2,
                    &mut self.w2,
                    &delta_2,
                    &hidden_1,
                    HIDDEN_2,
                    HIDDEN_1,
                );
                adam_bias(&mut adam_b2, &mut self.b2, &delta_2);
                adam_matrix(
                    &mut adam_w1,
                    &mut self.w1,
                    &delta_1,
                    &input,
                    HIDDEN_1,
                    FEATURE_COUNT,
                );
                adam_bias(&mut adam_b1, &mut self.b1, &delta_1);
            }
            loss /= normalised.len() as f32 * BAND_COUNT as f32;
            if loss.is_finite() && loss < 1e-4 {
                break;
            }
        }
        // 把标准化折回输出层：y = (W·h + b) × scale + mean。
        // Fold the normalisation into the output layer: y = (W·h + b) × scale + mean.
        for band in 0..BAND_COUNT {
            for unit in 0..HIDDEN_2 {
                self.w3[band * HIDDEN_2 + unit] *= scale[band];
            }
            self.b3[band] = self.b3[band] * scale[band] + mean[band];
        }
        // 残差按原始量纲统计，抖动幅度才与真实对数频谱误差对应。
        // Measure the residual in raw units so the jitter matches the real log-spectral error.
        let mut residual = 0.0_f32;
        for (input, target) in data {
            let mut output = [0.0_f32; BAND_COUNT];
            self.forward(input, &mut output);
            for band in 0..BAND_COUNT {
                let error = output[band] - target[band];
                residual += error * error;
            }
        }
        residual /= data.len() as f32 * BAND_COUNT as f32;
        self.residual = if residual.is_finite() {
            residual.max(0.0)
        } else {
            0.0
        };
    }
}

/// 所属模块：audio / 神经音色模型
/// 功能：一组参数的 Adam 优化器状态（一阶/二阶矩与偏差修正）。
/// 输入：由 `AdamState::new` 创建，`advance` 每个样本调用一次。
/// 输出/副作用：`update` 就地修改权重与矩估计。
/// 复杂度：`update` 时间 O(1)，空间 O(参数量)。
///
/// Module: audio / neural timbre model
/// Purpose: Adam optimizer state (first/second moments and bias correction) for one parameter set.
/// Inputs: created by `AdamState::new`; `advance` is called once per sample.
/// Outputs / side effects: `update` mutates the weights and moment estimates in place.
/// Complexity: O(1) time per `update`, O(parameters) space.
struct AdamState {
    first: Vec<f32>,
    second: Vec<f32>,
    correction_1: f32,
    correction_2: f32,
    step: u32,
}

impl AdamState {
    /// 所属模块：audio / 神经音色模型
    /// 功能：为指定参数数量创建零初始化的优化器状态。
    /// 输入：`count` 参数数量。
    /// 输出/副作用：返回状态；不修改外部状态。
    /// 复杂度：时间 O(count)，空间 O(count)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Create zero-initialised optimizer state for the given parameter count.
    /// Inputs: `count` number of parameters.
    /// Outputs / side effects: the state; no external change.
    /// Complexity: O(count) time and space.
    fn new(count: usize) -> Self {
        Self {
            first: vec![0.0; count],
            second: vec![0.0; count],
            correction_1: 1.0,
            correction_2: 1.0,
            step: 0,
        }
    }

    /// 所属模块：audio / 神经音色模型
    /// 功能：推进一次优化步并预计算偏差修正，避免每个参数重复求幂。
    /// 输入：无。
    /// 输出/副作用：更新 `step` 与两个修正系数。
    /// 复杂度：时间 O(1)（2 次整数幂），空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Advance one optimisation step and precompute bias corrections so each parameter
    /// update avoids repeated exponentiation.
    /// Inputs: none.
    /// Outputs / side effects: updates `step` and both correction factors.
    /// Complexity: O(1) time (two integer powers), O(1) space.
    fn advance(&mut self) {
        self.step = self.step.saturating_add(1);
        let step = self.step as i32;
        self.correction_1 = 1.0 - ADAM_BETA_1.powi(step);
        self.correction_2 = 1.0 - ADAM_BETA_2.powi(step);
    }

    /// 所属模块：audio / 神经音色模型
    /// 功能：用 Adam 规则更新单个参数。
    /// 输入：`index` 参数下标、`weight` 权重、`gradient` 梯度。
    /// 输出/副作用：就地更新权重与矩估计；不分配内存。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Update one parameter with the Adam rule.
    /// Inputs: `index`, `weight`, `gradient`.
    /// Outputs / side effects: updates the weight and moments in place; no allocation.
    /// Complexity: O(1) time and space.
    fn update(&mut self, index: usize, weight: &mut f32, gradient: f32) {
        self.first[index] = ADAM_BETA_1 * self.first[index] + (1.0 - ADAM_BETA_1) * gradient;
        self.second[index] =
            ADAM_BETA_2 * self.second[index] + (1.0 - ADAM_BETA_2) * gradient * gradient;
        let first = self.first[index] / self.correction_1;
        let second = self.second[index] / self.correction_2;
        *weight -= ADAM_RATE * first / (second.sqrt() + ADAM_EPSILON);
    }
}

/// 所属模块：audio / 神经音色模型
/// 功能：按层应用一次 Adam 更新（含权重衰减）。
/// 输入：`state` 优化器、`weights` 权重、每行 `delta`、每列 `input`、`rows`/`cols` 维度。
/// 输出/副作用：就地更新权重；不分配内存。
/// 复杂度：时间 O(rows × cols)，空间 O(1)。
///
/// Module: audio / neural timbre model
/// Purpose: Apply one Adam step for a layer, including weight decay.
/// Inputs: `state`, `weights`, per-row `delta`, per-column `input`, `rows`/`cols`.
/// Outputs / side effects: updates weights in place; no allocation.
/// Complexity: O(rows × cols) time, O(1) space.
fn adam_matrix(
    state: &mut AdamState,
    weights: &mut [f32],
    delta: &[f32],
    input: &[f32],
    rows: usize,
    cols: usize,
) {
    for row in 0..rows {
        for column in 0..cols {
            let index = row * cols + column;
            let gradient = delta[row] * input[column] + TRAIN_DECAY * weights[index];
            state.update(index, &mut weights[index], gradient);
        }
    }
}

/// 所属模块：audio / 神经音色模型
/// 功能：按层应用一次 Adam 偏置更新。
/// 输入：`state` 优化器、`bias` 偏置、`delta` 误差。
/// 输出/副作用：就地更新偏置；不分配内存。
/// 复杂度：时间 O(delta 长度)，空间 O(1)。
///
/// Module: audio / neural timbre model
/// Purpose: Apply one Adam step to a bias vector.
/// Inputs: `state`, `bias`, `delta`.
/// Outputs / side effects: updates the bias in place; no allocation.
/// Complexity: O(delta length) time, O(1) space.
fn adam_bias(state: &mut AdamState, bias: &mut [f32], delta: &[f32]) {
    for (index, value) in delta.iter().enumerate() {
        state.update(index, &mut bias[index], *value);
    }
}

/// 所属模块：audio / 神经音色模型
/// 功能：确定性线性同余随机数，返回 `[0, 1)`；训练初始化、洗牌与抖动都使用它。
/// 输入：`state` 可变种子。
/// 输出/副作用：就地推进种子并返回随机数。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / neural timbre model
/// Purpose: Deterministic LCG returning `[0, 1)`; used for init, shuffling and jitter.
/// Inputs: `state` mutable seed.
/// Outputs / side effects: advances the seed and returns the value.
/// Complexity: O(1) time and space.
fn next_random(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*state >> 8) as f32 / 16_777_216.0
}

/// 所属模块：audio / 神经音色模型
/// 功能：把无符号整数散列到 `[-1, 1]`，用于逐音符抖动（不需要保存随机状态）。
/// 输入：`value` 任意整数（音符序号）。
/// 输出/副作用：返回 `[-1, 1]`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / neural timbre model
/// Purpose: Hash a counter into `[-1, 1]` for per-note jitter without stored RNG state.
/// Inputs: `value` any integer (note counter).
/// Outputs / side effects: `[-1, 1]`; no state change.
/// Complexity: O(1) time and space.
fn unit_hash(value: u64) -> f32 {
    let mut x = value.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 29;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 32;
    let unit = (x & 0x00FF_FFFF) as f32 / 8_388_608.0;
    unit - 1.0
}

/// 所属模块：audio / 神经音色模型
/// 功能：训练好的音色模型：网络权重、参考点输出与训练残差。
/// 输入：由 `NeuralTimbre::train` 构造。
/// 输出/副作用：`correction` / `voice_gains` 只读推理，可在实时线程调用。
/// 复杂度：推理时间 O(参数量)，空间 O(1)。
///
/// Module: audio / neural timbre model
/// Purpose: A trained timbre model: network weights, reference output and training residual.
/// Inputs: built by `NeuralTimbre::train`.
/// Outputs / side effects: `correction` / `voice_gains` are read-only and realtime safe.
/// Complexity: O(parameters) inference time, O(1) space.
pub struct NeuralTimbre {
    net: TinyMlp,
    /// 参考点（分区根音、中力度、中段时间）的模型输出；差值保证在根音上修正量为 0。
    /// Model output at the reference point (root pitch, mid velocity, mid time); subtracting it
    /// keeps the correction at zero on the root note.
    reference: [f32; BAND_COUNT],
    /// 训练残差均方根，用于决定抖动幅度。
    /// RMS of the training residual, used to scale the jitter.
    residual: f32,
}

impl NeuralTimbre {
    /// 所属模块：audio / 神经音色模型
    /// 功能：从清单乐器的分区采样提取频谱特征并训练模型。
    /// 输入：`instrument` 静态清单乐器。
    /// 输出/副作用：返回训练好的模型；分区全部解码失败或样本过少时返回 `None`。
    /// 复杂度：时间 O(分区数 × 分析帧数 × FFT 长度 + 训练轮数 × 样本数 × 参数量)，空间 O(样本数)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Extract spectral features from the manifest instrument's zones and train a model.
    /// Inputs: `instrument` static manifest instrument.
    /// Outputs / side effects: the trained model, or `None` when no zone decodes or there are too
    /// few samples.
    /// Complexity: O(zones × frames × FFT length + epochs × samples × parameters) time,
    /// O(samples) space.
    pub fn train(instrument: &'static BankInstrument) -> Option<Self> {
        let mut data: Vec<([f32; FEATURE_COUNT], [f32; BAND_COUNT])> = Vec::new();
        for zone in &instrument.zones {
            analyse_zone(zone.root, zone.lo_vel, zone.hi_vel, &zone.file, &mut data);
        }
        if data.len() < MIN_TRAINING_FRAMES {
            return None;
        }
        let seed = 0x5EED_0000u32
            ^ instrument.id.bytes().fold(0u32, |hash, byte| {
                hash.wrapping_mul(31).wrapping_add(byte as u32)
            });
        let mut net = TinyMlp::new(seed);
        net.train(&data, seed.rotate_left(7));
        let mut reference = [0.0_f32; BAND_COUNT];
        // 参考点取中音区根音与中力度：与分区根音重合时修正量为 0，原采样音色不被改动。
        // The reference point is a mid-register root and mid velocity: on the root note the
        // correction is zero, so the original sample timbre is preserved.
        let reference_input = [
            pitch_feature(REFERENCE_PITCH),
            REFERENCE_VELOCITY,
            time_feature(REFERENCE_TIME),
            1.0,
        ];
        net.forward(&reference_input, &mut reference);
        let residual = net.residual.sqrt();
        Some(Self {
            net,
            reference,
            residual,
        })
    }

    /// 所属模块：audio / 神经音色模型
    /// 功能：推理 6 段相对频谱形状，并减去参考点得到修正量。
    /// 输入：`pitch` MIDI 音高、`velocity` 归一化力度、`time` 音符已持续时间（秒）、`out` 修正量。
    /// 输出/副作用：写入 `out`；不分配内存，可实时调用。
    /// 复杂度：时间 O(参数量)，空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Infer the six-band relative shape and subtract the reference to get the correction.
    /// Inputs: `pitch`, `velocity`, `time` in seconds, `out` correction values.
    /// Outputs / side effects: writes `out`; no allocation, realtime safe.
    /// Complexity: O(parameters) time, O(1) space.
    pub fn correction(&self, pitch: f32, velocity: f32, time: f32, out: &mut [f32; BAND_COUNT]) {
        let input = [
            pitch_feature(pitch),
            velocity.clamp(0.0, 1.0),
            time_feature(time),
            1.0,
        ];
        let mut shape = [0.0_f32; BAND_COUNT];
        self.net.forward(&input, &mut shape);
        for (band, value) in out.iter_mut().enumerate() {
            *value = shape[band] - self.reference[band];
        }
    }

    /// 所属模块：audio / 神经音色模型
    /// 功能：把模型修正折算为三段滤波器增益（dB）并叠加抖动。
    /// 输入：`pitch`、`velocity`、`time`、`jitter` 逐音符抖动、`out` 三段增益。
    /// 输出/副作用：写入 `out`；不分配内存，可实时调用。
    /// 复杂度：时间 O(参数量)，空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Convert the correction into three filter gains in dB and add the jitter.
    /// Inputs: `pitch`, `velocity`, `time`, `jitter`, `out` three gains.
    /// Outputs / side effects: writes `out`; no allocation, realtime safe.
    /// Complexity: O(parameters) time, O(1) space.
    pub fn voice_gains(
        &self,
        pitch: f32,
        velocity: f32,
        time: f32,
        jitter: f32,
        out: &mut [f32; 3],
    ) {
        let mut bands = [0.0_f32; BAND_COUNT];
        self.correction(pitch, velocity, time, &mut bands);
        let jitter_gain = jitter * self.residual.min(1.0) * JITTER_DB;
        out[0] = ((bands[0] + bands[1]) * 0.5 * BAND_TO_DB).clamp(-MAX_GAIN_DB, MAX_GAIN_DB);
        out[1] = ((bands[2] + bands[3]) * 0.5 * BAND_TO_DB + jitter_gain)
            .clamp(-MAX_GAIN_DB, MAX_GAIN_DB);
        out[2] = ((bands[4] + bands[5]) * 0.5 * BAND_TO_DB + jitter_gain)
            .clamp(-MAX_GAIN_DB, MAX_GAIN_DB);
    }

    /// 所属模块：audio / 神经音色模型
    /// 功能：按音符序号生成确定性的亮度抖动与音量抖动。
    /// 输入：`counter` 递增的音符序号。
    /// 输出/副作用：返回 `(亮度抖动 [-1,1], 音量增益倍数)`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Deterministic brightness and level jitter keyed by the note counter.
    /// Inputs: `counter` incremented note counter.
    /// Outputs / side effects: `(brightness jitter in [-1,1], level gain multiplier)`.
    /// Complexity: O(1) time and space.
    pub fn note_jitter(&self, counter: u64) -> (f32, f32) {
        let brightness = unit_hash(counter ^ 0x51ED_2701);
        let level = unit_hash(counter.wrapping_add(0x2545_F491));
        let level_db = level * self.residual.min(1.0) * JITTER_LEVEL_DB;
        (brightness, 10.0_f32.powf(level_db / 20.0))
    }
}

/// 所属模块：audio / 神经音色模型
/// 功能：把 MIDI 音高映射到 `[0, 1]` 特征。
/// 输入：`pitch` MIDI 音高（0~127）。
/// 输出/副作用：返回归一化音高；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / neural timbre model
/// Purpose: Map a MIDI note to a `[0, 1]` feature.
/// Inputs: `pitch` MIDI note (0..=127).
/// Outputs / side effects: normalized pitch; no state change.
/// Complexity: O(1) time and space.
fn pitch_feature(pitch: f32) -> f32 {
    ((pitch - 21.0) / 87.0).clamp(0.0, 1.0)
}

/// 所属模块：audio / 神经音色模型
/// 功能：把音符已持续秒数映射到 `[0, 1)`；用 `t/(t+TIME_SCALE)` 让前几百毫秒变化最快、长音趋于饱和。
/// 输入：`seconds` 音符已持续时间（秒）。
/// 输出/副作用：返回归一化时间；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / neural timbre model
/// Purpose: Map elapsed seconds to `[0, 1)`; `t/(t+TIME_SCALE)` changes fastest in the first few
/// hundred milliseconds and saturates for long notes.
/// Inputs: `seconds` elapsed time.
/// Outputs / side effects: normalized time; no state change.
/// Complexity: O(1) time and space.
fn time_feature(seconds: f32) -> f32 {
    let seconds = seconds.max(0.0);
    (seconds / (seconds + TIME_SCALE)).clamp(0.0, 1.0)
}

/// 所属模块：audio / 神经音色模型
/// 功能：分析一个分区的若干时间点，把（特征, 频谱形状）样本追加到训练集。
/// 输入：`root` 根音、`lo_vel`/`hi_vel` 力度层范围、`file` 采样文件、`data` 训练集。
/// 输出/副作用：向 `data` 追加样本；解码失败时什么也不做。
/// 复杂度：时间 O(分析帧数 × FFT 长度)，空间 O(FFT 长度)。
///
/// Module: audio / neural timbre model
/// Purpose: Analyse several positions of one zone and append (features, shape) samples.
/// Inputs: `root`, `lo_vel`/`hi_vel`, `file`, `data` training set.
/// Outputs / side effects: appends samples; a decode failure adds nothing.
/// Complexity: O(frames × FFT length) time, O(FFT length) space.
fn analyse_zone(
    root: u8,
    lo_vel: u8,
    hi_vel: u8,
    file: &str,
    data: &mut Vec<([f32; FEATURE_COUNT], [f32; BAND_COUNT])>,
) {
    let Some((samples, rate)) = load_sample(file) else {
        return;
    };
    if samples.len() < FFT_SIZE * 2 {
        return;
    }
    let duration = samples.len() as f32 / rate.max(1.0);
    let velocity = ((lo_vel as f32 + hi_vel as f32) * 0.5 / 127.0).clamp(0.0, 1.0);
    let mut window = [0.0_f32; FFT_SIZE];
    let mut real = [0.0_f32; FFT_SIZE];
    let mut imaginary = [0.0_f32; FFT_SIZE];
    for position in ANALYSIS_POSITIONS {
        let start = ((samples.len() as f32 * position) as usize).min(samples.len() - FFT_SIZE);
        for (index, slot) in window.iter_mut().enumerate() {
            *slot = samples[start + index] * hann(index);
        }
        real.copy_from_slice(&window);
        imaginary.fill(0.0);
        fft(&mut real, &mut imaginary);
        let shape = band_shape(&real, &imaginary);
        // 时间特征必须与实时推理同尺度：用真实秒数而不是采样内比例。
        // The time feature must match realtime inference, so use real seconds rather than the
        // position inside the sample.
        let features = [
            pitch_feature(root as f32),
            velocity,
            time_feature(duration * position),
            1.0,
        ];
        data.push((features, shape));
    }
}

/// 所属模块：audio / 神经音色模型
/// 功能：把 FFT 结果按对数频段汇总为相对频谱形状（各段对数能量减去均值）。
/// 输入：`real` / `imaginary` FFT 实部与虚部。
/// 输出/副作用：返回 6 段相对形状；不修改输入。
/// 复杂度：时间 O(频段数 × 每段 bin 数)，空间 O(1)。
///
/// Module: audio / neural timbre model
/// Purpose: Fold FFT output into a relative spectral shape (log band energy minus the mean).
/// Inputs: `real` / `imaginary` FFT parts.
/// Outputs / side effects: six relative band values; inputs untouched.
/// Complexity: O(bands × bins per band) time, O(1) space.
fn band_shape(real: &[f32; FFT_SIZE], imaginary: &[f32; FFT_SIZE]) -> [f32; BAND_COUNT] {
    let mut bands = [0.0_f32; BAND_COUNT];
    for band in 0..BAND_COUNT {
        let mut energy = 0.0_f32;
        let end = BAND_EDGES[band + 1].min(FFT_SIZE / 2);
        for bin in BAND_EDGES[band]..end {
            energy += real[bin] * real[bin] + imaginary[bin] * imaginary[bin];
        }
        bands[band] = (energy + 1e-9).ln();
    }
    let mean = bands.iter().sum::<f32>() / BAND_COUNT as f32;
    for value in &mut bands {
        *value = (*value - mean).clamp(-4.0, 4.0);
    }
    bands
}

/// 所属模块：audio / 神经音色模型
/// 功能：Hann 窗系数。
/// 输入：`index` 窗内序号。
/// 输出/副作用：返回窗值；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / neural timbre model
/// Purpose: Hann window coefficient.
/// Inputs: `index` position inside the window.
/// Outputs / side effects: the window value; no state change.
/// Complexity: O(1) time and space.
fn hann(index: usize) -> f32 {
    let phase = std::f32::consts::TAU * index as f32 / FFT_SIZE as f32;
    0.5 - 0.5 * phase.cos()
}

/// 所属模块：audio / 神经音色模型
/// 功能：原地基 2 Cooley-Tukey FFT（长度必须为 2 的幂）。
/// 输入：`real` / `imaginary` 时域数据，长度相同且为 2 的幂。
/// 输出/副作用：就地写入频域结果；不分配内存。
/// 复杂度：时间 O(n log n)，空间 O(1)，`n` = 输入长度。
///
/// Module: audio / neural timbre model
/// Purpose: In-place radix-2 Cooley-Tukey FFT (length must be a power of two).
/// Inputs: `real` / `imaginary` time-domain data of equal power-of-two length.
/// Outputs / side effects: writes the spectrum in place; no allocation.
/// Complexity: O(n log n) time, O(1) space, `n` = input length.
fn fft(real: &mut [f32], imaginary: &mut [f32]) {
    let length = real.len();
    let mut target = 0usize;
    for index in 1..length {
        let mut bit = length >> 1;
        while target & bit != 0 {
            target ^= bit;
            bit >>= 1;
        }
        target |= bit;
        if index < target {
            real.swap(index, target);
            imaginary.swap(index, target);
        }
    }
    let mut span = 2usize;
    while span <= length {
        let angle = -std::f32::consts::TAU / span as f32;
        let (step_real, step_imaginary) = (angle.cos(), angle.sin());
        let mut start = 0usize;
        while start < length {
            let (mut twiddle_real, mut twiddle_imaginary) = (1.0_f32, 0.0_f32);
            for offset in 0..span / 2 {
                let even = start + offset;
                let odd = even + span / 2;
                let odd_real = real[odd] * twiddle_real - imaginary[odd] * twiddle_imaginary;
                let odd_imaginary = real[odd] * twiddle_imaginary + imaginary[odd] * twiddle_real;
                real[odd] = real[even] - odd_real;
                imaginary[odd] = imaginary[even] - odd_imaginary;
                real[even] += odd_real;
                imaginary[even] += odd_imaginary;
                let next_real = twiddle_real * step_real - twiddle_imaginary * step_imaginary;
                twiddle_imaginary = twiddle_real * step_imaginary + twiddle_imaginary * step_real;
                twiddle_real = next_real;
            }
            start += span;
        }
        span <<= 1;
    }
}

/// 所属模块：audio / 神经音色模型
/// 功能：按乐器 id 获取已训练模型，首次调用时训练并缓存。
/// 输入：`instrument` 静态清单乐器。
/// 输出/副作用：返回共享模型；训练失败时缓存 `None`，避免每次同步都重试。
/// 复杂度：首次 O(训练代价)，之后 O(1)；`Mutex` 只在同步阶段短暂持有。
///
/// Module: audio / neural timbre model
/// Purpose: Get the trained model of an instrument id, training and caching it on first use.
/// Inputs: `instrument` static manifest instrument.
/// Outputs / side effects: the shared model; failures cache `None` so sync never retries.
/// Complexity: O(training cost) on first use, O(1) afterwards; the `Mutex` is held only briefly
/// during the sync phase.
pub fn model_for(instrument: &'static BankInstrument) -> Option<Arc<NeuralTimbre>> {
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock() {
        if let Some(cached) = cache.get(instrument.id.as_str()) {
            return cached.clone();
        }
    }
    let trained = NeuralTimbre::train(instrument).map(Arc::new);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(instrument.id.clone(), trained.clone());
    }
    trained
}

static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<NeuralTimbre>>>>> = OnceLock::new();

/// 所属模块：audio / 神经音色模型
/// 功能：单个 voice 的三段音色整形器：低架 → 中峰 → 高架，参数按块平滑更新。
/// 输入：由 `ToneShaper::new` / `update` 提供。
/// 输出/副作用：`process` 逐采样处理并推进滤波器状态。
/// 复杂度：`process` 时间 O(1)，空间 O(1)。
///
/// Module: audio / neural timbre model
/// Purpose: Per-voice three-band tone shaper (low shelf → peaking → high shelf) with block-wise
/// smoothed parameters.
/// Inputs: provided by `ToneShaper::new` / `update`.
/// Outputs / side effects: `process` runs sample by sample and advances the filter state.
/// Complexity: O(1) time and space per sample.
#[derive(Clone, Copy)]
pub struct ToneShaper {
    sample_rate: f32,
    low: Biquad,
    mid: Biquad,
    high: Biquad,
    gains: [f32; 3],
}

impl ToneShaper {
    /// 所属模块：audio / 神经音色模型
    /// 功能：按采样率创建旁路状态的整形器。
    /// 输入：`sample_rate` 引擎采样率（Hz）。
    /// 输出/副作用：返回增益为 0 dB 的整形器；不修改外部状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Create a bypassed shaper for the given sample rate.
    /// Inputs: `sample_rate` engine rate in Hz.
    /// Outputs / side effects: a 0 dB shaper; no external state change.
    /// Complexity: O(1) time and space.
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            low: Biquad::default(),
            mid: Biquad::default(),
            high: Biquad::default(),
            gains: [0.0; 3],
        }
    }

    /// 所属模块：audio / 神经音色模型
    /// 功能：把目标增益平滑后重算滤波器系数；每块调用一次，逐采样路径不做除法与三角函数。
    /// 输入：`gains` 三段目标增益（dB）。
    /// 输出/副作用：更新滤波器系数；`gains` 为 0 时三个滤波器都退化为直通。
    /// 复杂度：时间 O(1)（含 3 次三角函数），空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Smooth the target gains and recompute coefficients once per block so the
    /// per-sample path has no divisions or trigonometry.
    /// Inputs: `gains` three target gains in dB.
    /// Outputs / side effects: updates coefficients; all-zero gains degrade to pass-through.
    /// Complexity: O(1) time (three trig calls), O(1) space.
    pub fn update(&mut self, gains: [f32; 3]) {
        for (current, target) in self.gains.iter_mut().zip(gains.iter()) {
            *current += (target - *current) * GAIN_SMOOTHING;
        }
        let sample_rate = self.sample_rate;
        self.low
            .set_low_shelf(sample_rate, LOW_SHELF_HZ, self.gains[0]);
        self.mid
            .set_peaking(sample_rate, PEAK_HZ, PEAK_Q, self.gains[1]);
        self.high
            .set_high_shelf(sample_rate, HIGH_SHELF_HZ, self.gains[2]);
    }

    /// 所属模块：audio / 神经音色模型
    /// 功能：逐采样串行应用三个滤波器。
    /// 输入：`sample` 输入采样。
    /// 输出/副作用：返回整形后的采样；推进滤波器状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Run the three filters in series for one sample.
    /// Inputs: `sample` input value.
    /// Outputs / side effects: the shaped value; advances filter state.
    /// Complexity: O(1) time and space.
    pub fn process(&mut self, sample: f32) -> f32 {
        let value = self.low.process(sample);
        let value = self.mid.process(value);
        self.high.process(value)
    }

    /// 所属模块：audio / 神经音色模型
    /// 功能：清空滤波器状态与增益，供新音符复用 voice。
    /// 输入：无。
    /// 输出/副作用：状态归零；不修改采样率。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Clear filter state and gains so a stolen voice starts clean.
    /// Inputs: none.
    /// Outputs / side effects: the state resets; the sample rate is kept.
    /// Complexity: O(1) time and space.
    pub fn reset(&mut self) {
        let sample_rate = self.sample_rate;
        *self = Self::new(sample_rate);
    }
}

/// 所属模块：audio / 神经音色模型
/// 功能：RBJ 双二阶滤波器，只实现音色整形需要的低架 / 峰值 / 高架。
/// 输入：由 `set_*` 写入系数，由 `process` 逐采样运行。
/// 输出/副作用：`process` 推进内部状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / neural timbre model
/// Purpose: RBJ biquad implementing only the low shelf, peaking and high shelf the shaper needs.
/// Inputs: coefficients via `set_*`, samples via `process`.
/// Outputs / side effects: `process` advances internal state.
/// Complexity: O(1) time and space.
#[derive(Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Default for Biquad {
    /// 所属模块：audio / 神经音色模型
    /// 功能：默认必须是直通而不是全零；全零滤波器会把尚未更新系数的 voice 直接静音。
    /// 输入：无。
    /// 输出/副作用：返回直通系数；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: The default must pass audio through rather than being all zeros; an all-zero
    /// filter would silence a voice whose coefficients have not been updated yet.
    /// Inputs: none.
    /// Outputs / side effects: pass-through coefficients; no state change.
    /// Complexity: O(1) time and space.
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }
}

impl Biquad {
    /// 所属模块：audio / 神经音色模型
    /// 功能：写入直通系数（`b0 = 1`，其余为 0）。
    /// 输入：无。
    /// 输出/副作用：滤波器变为直通并保留状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Write pass-through coefficients (`b0 = 1`, everything else 0).
    /// Inputs: none.
    /// Outputs / side effects: the filter passes audio through and keeps its state.
    /// Complexity: O(1) time and space.
    fn set_passthrough(&mut self) {
        self.b0 = 1.0;
        self.b1 = 0.0;
        self.b2 = 0.0;
        self.a1 = 0.0;
        self.a2 = 0.0;
    }

    /// 所属模块：audio / 神经音色模型
    /// 功能：写入低架系数；增益为 0 时退化为直通。
    /// 输入：`sample_rate` Hz、`frequency` 转折频率 Hz、`gain_db` 增益 dB。
    /// 输出/副作用：更新系数；不改动滤波器状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Write low-shelf coefficients; a 0 dB gain becomes pass-through.
    /// Inputs: `sample_rate` in Hz, `frequency` in Hz, `gain_db` in dB.
    /// Outputs / side effects: updates coefficients; filter state is kept.
    /// Complexity: O(1) time and space.
    fn set_low_shelf(&mut self, sample_rate: f32, frequency: f32, gain_db: f32) {
        if gain_db.abs() < 0.01 {
            self.set_passthrough();
            return;
        }
        let (cosine, alpha, amplitude) = shelf_terms(sample_rate, frequency, SHELF_Q, gain_db);
        let two_root_amplitude_alpha = 2.0 * amplitude.sqrt() * alpha;
        let a0 = (amplitude + 1.0) + (amplitude - 1.0) * cosine + two_root_amplitude_alpha;
        self.b0 = amplitude
            * ((amplitude + 1.0) - (amplitude - 1.0) * cosine + two_root_amplitude_alpha)
            / a0;
        self.b1 = 2.0 * amplitude * ((amplitude - 1.0) - (amplitude + 1.0) * cosine) / a0;
        self.b2 = amplitude
            * ((amplitude + 1.0) - (amplitude - 1.0) * cosine - two_root_amplitude_alpha)
            / a0;
        self.a1 = -2.0 * ((amplitude - 1.0) + (amplitude + 1.0) * cosine) / a0;
        self.a2 = ((amplitude + 1.0) + (amplitude - 1.0) * cosine - two_root_amplitude_alpha) / a0;
    }

    /// 所属模块：audio / 神经音色模型
    /// 功能：写入高架系数；增益为 0 时退化为直通。
    /// 输入：`sample_rate` Hz、`frequency` 转折频率 Hz、`gain_db` 增益 dB。
    /// 输出/副作用：更新系数；不改动滤波器状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Write high-shelf coefficients; a 0 dB gain becomes pass-through.
    /// Inputs: `sample_rate` in Hz, `frequency` in Hz, `gain_db` in dB.
    /// Outputs / side effects: updates coefficients; filter state is kept.
    /// Complexity: O(1) time and space.
    fn set_high_shelf(&mut self, sample_rate: f32, frequency: f32, gain_db: f32) {
        if gain_db.abs() < 0.01 {
            self.set_passthrough();
            return;
        }
        let (cosine, alpha, amplitude) = shelf_terms(sample_rate, frequency, SHELF_Q, gain_db);
        let two_root_amplitude_alpha = 2.0 * amplitude.sqrt() * alpha;
        let a0 = (amplitude + 1.0) - (amplitude - 1.0) * cosine + two_root_amplitude_alpha;
        self.b0 = amplitude
            * ((amplitude + 1.0) + (amplitude - 1.0) * cosine + two_root_amplitude_alpha)
            / a0;
        self.b1 = -2.0 * amplitude * ((amplitude - 1.0) + (amplitude + 1.0) * cosine) / a0;
        self.b2 = amplitude
            * ((amplitude + 1.0) + (amplitude - 1.0) * cosine - two_root_amplitude_alpha)
            / a0;
        self.a1 = 2.0 * ((amplitude - 1.0) - (amplitude + 1.0) * cosine) / a0;
        self.a2 = ((amplitude + 1.0) - (amplitude - 1.0) * cosine - two_root_amplitude_alpha) / a0;
    }

    /// 所属模块：audio / 神经音色模型
    /// 功能：写入峰值（钟形）系数；增益为 0 时退化为直通。
    /// 输入：`sample_rate` Hz、`frequency` 中心频率 Hz、`q` 品质因数、`gain_db` 增益 dB。
    /// 输出/副作用：更新系数；不改动滤波器状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Write peaking coefficients; a 0 dB gain becomes pass-through.
    /// Inputs: `sample_rate` in Hz, `frequency` in Hz, `q` quality factor, `gain_db` in dB.
    /// Outputs / side effects: updates coefficients; filter state is kept.
    /// Complexity: O(1) time and space.
    fn set_peaking(&mut self, sample_rate: f32, frequency: f32, q: f32, gain_db: f32) {
        if gain_db.abs() < 0.01 {
            self.set_passthrough();
            return;
        }
        let (cosine, alpha, amplitude) = shelf_terms(sample_rate, frequency, q, gain_db);
        let a0 = 1.0 + alpha / amplitude;
        self.b0 = (1.0 + alpha * amplitude) / a0;
        self.b1 = -2.0 * cosine / a0;
        self.b2 = (1.0 - alpha * amplitude) / a0;
        self.a1 = -2.0 * cosine / a0;
        self.a2 = (1.0 - alpha / amplitude) / a0;
    }

    /// 所属模块：audio / 神经音色模型
    /// 功能：转置直接 II 型逐采样处理。
    /// 输入：`sample` 输入采样。
    /// 输出/副作用：返回滤波结果并推进状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / neural timbre model
    /// Purpose: Transposed direct form II, one sample at a time.
    /// Inputs: `sample` input value.
    /// Outputs / side effects: filtered value; advances state.
    /// Complexity: O(1) time and space.
    fn process(&mut self, sample: f32) -> f32 {
        let value = self.b0 * sample + self.z1;
        self.z1 = self.b1 * sample - self.a1 * value + self.z2;
        self.z2 = self.b2 * sample - self.a2 * value;
        value
    }
}

/// 所属模块：audio / 神经音色模型
/// 功能：计算 RBJ 双二阶公共项（归一化余弦、alpha 与幅度），并把频率限制在可用范围内。
/// 输入：`sample_rate` Hz、`frequency` Hz、`q` 品质因数、`gain_db` dB。
/// 输出/副作用：返回 `(cos, alpha, amplitude)`；不修改状态。
/// 复杂度：时间 O(1)（3 次三角函数），空间 O(1)。
///
/// Module: audio / neural timbre model
/// Purpose: Compute the shared RBJ terms (normalized cosine, alpha, amplitude) with the frequency
/// clamped into the usable range.
/// Inputs: `sample_rate` in Hz, `frequency` in Hz, `q` quality factor, `gain_db` in dB.
/// Outputs / side effects: `(cos, alpha, amplitude)`; no state change.
/// Complexity: O(1) time (three trig calls), O(1) space.
fn shelf_terms(sample_rate: f32, frequency: f32, q: f32, gain_db: f32) -> (f32, f32, f32) {
    let nyquist = sample_rate * 0.45;
    let frequency = frequency.clamp(20.0, nyquist);
    let amplitude = 10.0_f32.powf(gain_db / 40.0);
    let omega = std::f32::consts::TAU * frequency / sample_rate;
    let sine = omega.sin();
    let cosine = omega.cos();
    let alpha = sine / (2.0 * q.max(0.1));
    (cosine, alpha, amplitude)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::bank_instrument;

    /// 覆盖：前向推理确定、有界，且不随重复调用漂移。
    /// Covers: forward inference is deterministic, bounded and stable across calls.
    #[test]
    fn forward_inference_is_deterministic_and_bounded() {
        let model = NeuralTimbre::train(bank_instrument("GrandPiano").expect("GrandPiano"))
            .expect("trained model");
        let mut first = [0.0_f32; 3];
        let mut second = [0.0_f32; 3];
        model.voice_gains(60.0, 0.8, 0.2, 0.0, &mut first);
        model.voice_gains(60.0, 0.8, 0.2, 0.0, &mut second);
        assert_eq!(first, second, "同一输入必须得到同一输出");
        assert!(first.iter().all(|gain| gain.abs() <= MAX_GAIN_DB));
    }

    /// 覆盖：训练会让均方误差相对未训练网络显著下降，说明模型确实从采样数据中学习。
    /// Covers: training lowers the mean squared error well below the untrained network, proving the
    /// model learns from the sample data.
    #[test]
    fn training_reduces_error() {
        let instrument = bank_instrument("GrandPiano").expect("GrandPiano");
        let mut data = Vec::new();
        for zone in &instrument.zones {
            analyse_zone(zone.root, zone.lo_vel, zone.hi_vel, &zone.file, &mut data);
        }
        let mut net = TinyMlp::new(7);
        let mut before = 0.0_f32;
        for (input, target) in &data {
            let mut output = [0.0_f32; BAND_COUNT];
            net.forward(input, &mut output);
            for band in 0..BAND_COUNT {
                let error = output[band] - target[band];
                before += error * error;
            }
        }
        before /= (data.len() * BAND_COUNT) as f32;
        net.train(&data, 11);
        eprintln!(
            "frames={} mse_before={before:.3} mse_after={:.3}",
            data.len(),
            net.residual
        );
        assert!(net.residual.is_finite());
        assert!(
            net.residual < before * 0.5,
            "训练必须显著降低误差：before={before}, after={}",
            net.residual
        );
    }

    /// 覆盖：根音、中力度、中段时间上的修正量为 0，原始采样音色不被改动。
    /// Covers: on the root note at mid velocity and mid time the correction is zero.
    #[test]
    fn correction_is_zero_at_reference_point() {
        let model = NeuralTimbre::train(bank_instrument("GrandPiano").expect("GrandPiano"))
            .expect("trained model");
        let mut bands = [0.0_f32; BAND_COUNT];
        model.correction(
            REFERENCE_PITCH,
            REFERENCE_VELOCITY,
            REFERENCE_TIME,
            &mut bands,
        );
        assert!(
            bands.iter().all(|value| value.abs() < 1e-5),
            "参考点修正量必须为 0：{bands:?}"
        );
    }

    /// 覆盖：模型按乐器缓存，同一乐器两次获取得到同一份权重。
    /// Covers: models are cached per instrument, so two lookups share the same weights.
    #[test]
    fn models_are_cached_per_instrument() {
        let instrument = bank_instrument("GrandPiano").expect("GrandPiano");
        let first = model_for(instrument).expect("model");
        let second = model_for(instrument).expect("model");
        assert!(Arc::ptr_eq(&first, &second));
    }

    /// 覆盖：整形器在 0 dB 时是直通，正增益会提升高频能量。
    /// Covers: the shaper is transparent at 0 dB and boosts high-frequency energy for positive gains.
    #[test]
    fn tone_shaper_boosts_high_band() {
        let mut shaper = ToneShaper::new(48_000.0);
        shaper.update([0.0; 3]);
        let silence = shaper.process(0.0);
        assert!(silence.abs() < 1e-9, "0 dB 时应为直通");

        let mut boosted = ToneShaper::new(48_000.0);
        boosted.update([0.0, 0.0, 8.0]);
        let mut energy = 0.0_f32;
        for index in 0..2_000 {
            let sample = (std::f32::consts::TAU * 8_000.0 * index as f32 / 48_000.0).sin() * 0.25;
            let value = boosted.process(sample);
            assert!(value.is_finite());
            if index > 1_000 {
                energy += value * value;
            }
        }
        assert!(energy > 20.0, "高架提升后能量应显著增加：{energy}");
    }
}
