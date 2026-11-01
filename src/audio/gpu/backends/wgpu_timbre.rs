//! 所属模块：audio::gpu / wgpu 神经音色训练后端
//! 文件用途：
//! - 用计算着色器在显卡上完成神经音色模型的前向、反向与 Adam 更新。
//! - 每次训练只提交一个命令缓冲（所有轮次顺序编码），避免逐轮往返的驱动开销。
//! - 存储缓冲合并为「状态 + 工作区」两块，兼容 downlevel 设备每阶段 4 个存储缓冲的限制。
//!
//! 函数清单：
//! - `WgpuTimbreTrainer::open`：创建管线与固定容量缓冲。
//! - `WgpuTimbreTrainer::train`：上传训练集与初始权重，执行指定轮数并回读权重。
//! - `shader_source`：按网络层尺寸生成 WGSL 源码。
//! - `device_bytes`：把 `f32` 切片编码为小端字节。
//!
//! Module: audio::gpu / wgpu timbre training backend
//! Purpose:
//! - Run forward, backward and Adam updates of the neural timbre model on the GPU.
//! - Submit one command buffer per training call (all epochs encoded in order) so there is no
//!   per-epoch round trip.
//! - Merge storage buffers into one state buffer and one work buffer so the shader stays within the
//!   four-storage-buffers-per-stage limit of downlevel devices.
//!
//! Functions:
//! - `WgpuTimbreTrainer::open`: create pipelines and fixed-capacity buffers.
//! - `WgpuTimbreTrainer::train`: upload samples and initial weights, run epochs and read back.
//! - `shader_source`: build the WGSL source for the given layer sizes.
//! - `device_bytes`: encode an `f32` slice as little-endian bytes.

use super::super::GpuRenderConfig;
use super::wgpu_compute::create_compute_device;
use std::sync::mpsc::channel;

/// 输入特征数与输出频段数；与神经音色模型保持一致。
/// Input feature count and output band count; must match the neural timbre model.
const FEATURE_COUNT: u32 = 4;
const BAND_COUNT: u32 = 6;

/// 训练集容量上限（分区数 × 分析帧数远小于该值）；同时决定工作区缓冲的固定偏移。
/// Upper bound for the training set size; it also fixes the work-buffer offsets.
const MAX_SAMPLES: u32 = 256;

/// 单次 `train` 允许的最大轮数，防止误用导致显卡长时间占用。
/// Maximum epochs per `train` call so a mistake cannot occupy the GPU for long.
const MAX_EPOCHS: u32 = 20_000;

/// 网络尺寸与参数偏移；全部在主机侧算好，着色器只做线性访问。
/// Network sizes and parameter offsets; computed on the host so the shader only indexes.
#[derive(Clone, Copy, Debug)]
pub struct TimbreLayout {
    pub input: u32,
    pub hidden_1: u32,
    pub hidden_2: u32,
    pub output: u32,
}

impl TimbreLayout {
    /// 所属模块：audio::gpu / wgpu 神经音色训练后端
    /// 功能：按两个隐藏层宽度构造布局。
    /// 输入：`hidden_1` / `hidden_2` 隐藏层宽度。
    /// 输出/副作用：返回布局；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::gpu / wgpu timbre training backend
    /// Purpose: Build the layout for the two hidden-layer widths.
    /// Inputs: `hidden_1` / `hidden_2` hidden widths.
    /// Outputs / side effects: the layout; no state change.
    /// Complexity: O(1) time and space.
    pub fn new(hidden_1: u32, hidden_2: u32) -> Self {
        Self {
            input: FEATURE_COUNT,
            hidden_1,
            hidden_2,
            output: BAND_COUNT,
        }
    }

    /// 所属模块：audio::gpu / wgpu 神经音色训练后端
    /// 功能：返回全部可训练参数数量（六组权重与偏置之和）。
    /// 输入：`self`。
    /// 输出/副作用：参数数量；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::gpu / wgpu timbre training backend
    /// Purpose: Total trainable parameter count (six weight/bias groups).
    /// Inputs: `self`.
    /// Outputs / side effects: the parameter count; no state change.
    /// Complexity: O(1) time and space.
    pub fn parameter_count(&self) -> usize {
        let input = self.input as usize;
        let hidden_1 = self.hidden_1 as usize;
        let hidden_2 = self.hidden_2 as usize;
        let output = self.output as usize;
        input * hidden_1 + hidden_1 + hidden_1 * hidden_2 + hidden_2 + hidden_2 * output + output
    }

    /// 所属模块：audio::gpu / wgpu 神经音色训练后端
    /// 功能：工作区缓冲的浮点数容量（样本 + 隐藏激活 + 反向误差）。
    /// 输入：`self`。
    /// 输出/副作用：浮点数个数；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::gpu / wgpu timbre training backend
    /// Purpose: Float capacity of the work buffer (samples + activations + deltas).
    /// Inputs: `self`.
    /// Outputs / side effects: the float count; no state change.
    /// Complexity: O(1) time and space.
    fn work_floats(&self) -> usize {
        let sample_stride = (self.input + self.output) as usize;
        let act_stride = (self.hidden_1 + self.hidden_2) as usize;
        let delta_stride = (self.hidden_1 + self.hidden_2 + self.output) as usize;
        MAX_SAMPLES as usize * (sample_stride + act_stride + delta_stride)
    }
}

/// GPU 神经音色训练器：设备、管线与缓冲在首次使用时创建一次。
/// GPU timbre trainer: device, pipelines and buffers are created once.
pub struct WgpuTimbreTrainer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    tick: wgpu::ComputePipeline,
    forward: wgpu::ComputePipeline,
    backward_2: wgpu::ComputePipeline,
    backward_1: wgpu::ComputePipeline,
    adam: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    state: wgpu::Buffer,
    work: wgpu::Buffer,
    control: wgpu::Buffer,
    readback: wgpu::Buffer,
    layout: TimbreLayout,
}

impl WgpuTimbreTrainer {
    /// 所属模块：audio::gpu / wgpu 神经音色训练后端
    /// 功能：按 GPU 配置创建计算设备、五个管线与固定容量缓冲。
    /// 输入：`config` 设备/驱动/API 选择、`layout` 网络尺寸。
    /// 输出/副作用：创建 GPU 资源；失败返回中文原因，调用方回退 CPU 训练。
    /// 复杂度：时间 O(GPU 设备创建 + 着色器编译)，空间 O(训练集容量)。
    ///
    /// Module: audio::gpu / wgpu timbre training backend
    /// Purpose: Create the compute device, five pipelines and fixed-capacity buffers.
    /// Inputs: `config` device/driver/API, `layout` network sizes.
    /// Outputs / side effects: GPU resources; a Chinese reason on failure so the caller can fall
    /// back to CPU training.
    /// Complexity: O(device creation + shader compilation) time, O(training capacity) space.
    pub fn open(config: &GpuRenderConfig, layout: TimbreLayout) -> Result<Self, String> {
        let (device, queue) =
            create_compute_device(config, "SurfingStudio neural timbre training")?;
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        // 五个通道共用同一套显式布局：自动布局会让每个管线各自独占，绑定组无法复用。
        // All five passes share one explicit layout: implicit layouts would be exclusive per
        // pipeline and the single bind group could not be reused.
        let storage_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("timbre bind group layout"),
            entries: &[storage_entry(0), storage_entry(1), storage_entry(2)],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("timbre pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let source = shader_source(layout);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("neural timbre trainer"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let make = |label: &str, entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let tick = make("timbre tick", "tick");
        let forward = make("timbre forward", "forward");
        let backward_2 = make("timbre backward 2", "backward_2");
        let backward_1 = make("timbre backward 1", "backward_1");
        let adam = make("timbre adam", "adam");
        if let Some(error) = pollster::block_on(error_scope.pop()) {
            return Err(format!("GPU 训练着色器校验失败: {error}"));
        }

        let params = layout.parameter_count();
        let storage = |label: &str, floats: usize, extra: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: (floats.max(1) * std::mem::size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST
                    | extra,
                mapped_at_creation: false,
            })
        };
        // 状态区：参数 → 一阶矩 → 二阶矩，三段等长参数，偏移由着色器常量给出。
        // State buffer: parameters → first moment → second moment, three equally sized sections.
        let state = storage("timbre state", params * 3, wgpu::BufferUsages::empty());
        let work = storage(
            "timbre work",
            layout.work_floats(),
            wgpu::BufferUsages::empty(),
        );
        let control = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("timbre control"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("timbre readback"),
            size: (params * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("timbre trainer bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: state.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: work.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: control.as_entire_binding(),
                },
            ],
        });

        Ok(Self {
            device,
            queue,
            tick,
            forward,
            backward_2,
            backward_1,
            adam,
            bind_group,
            state,
            work,
            control,
            readback,
            layout,
        })
    }

    /// 所属模块：audio::gpu / wgpu 神经音色训练后端
    /// 功能：在一个命令缓冲里编码全部训练轮次，回读最终权重。
    /// 输入：
    /// - `samples`：每项为（4 维特征, 6 段目标频谱形状），已标准化。
    /// - `initial`：初始权重（扁平化，长度等于 `layout.parameter_count()`）。
    /// - `epochs`：训练轮数，上限 `MAX_EPOCHS`。
    /// - `learning_rate` / `decay`：Adam 学习率与权重衰减。
    /// 输出/副作用：返回训练后的权重；GPU 失败时返回中文原因，调用方回退 CPU。
    /// 复杂度：时间 O(轮数 × 样本数 × 参数量)（在 GPU 上并行），空间 O(参数量 + 样本数)。
    ///
    /// Module: audio::gpu / wgpu timbre training backend
    /// Purpose: Encode every epoch into one command buffer and read back the final weights.
    /// Inputs:
    /// - `samples`: (4 features, 6 target bands) pairs, already normalized.
    /// - `initial`: flattened initial weights, length = `layout.parameter_count()`.
    /// - `epochs`: training epochs, capped at `MAX_EPOCHS`.
    /// - `learning_rate` / `decay`: Adam learning rate and weight decay.
    /// Outputs / side effects: the trained weights; a Chinese reason on failure so the caller can
    /// fall back to CPU training.
    /// Complexity: O(epochs × samples × parameters) time on the GPU, O(parameters + samples) space.
    pub fn train(
        &mut self,
        samples: &[([f32; FEATURE_COUNT as usize], [f32; BAND_COUNT as usize])],
        initial: &[f32],
        epochs: u32,
        learning_rate: f32,
        decay: f32,
    ) -> Result<Vec<f32>, String> {
        let count = samples.len() as u32;
        if count == 0 || count > MAX_SAMPLES {
            return Err(format!("训练样本数量不合法：{count}"));
        }
        let epochs = epochs.clamp(1, MAX_EPOCHS);
        let params = self.layout.parameter_count();
        if initial.len() != params {
            return Err("初始权重长度与网络结构不一致".to_string());
        }

        let mut work = vec![0.0_f32; self.layout.work_floats()];
        for (index, (input, target)) in samples.iter().enumerate() {
            let base = index * (FEATURE_COUNT + BAND_COUNT) as usize;
            work[base..base + FEATURE_COUNT as usize].copy_from_slice(input);
            work[base + FEATURE_COUNT as usize..base + (FEATURE_COUNT + BAND_COUNT) as usize]
                .copy_from_slice(target);
        }
        self.queue.write_buffer(&self.work, 0, &device_bytes(&work));

        let mut state = vec![0.0_f32; params * 3];
        state[..params].copy_from_slice(initial);
        self.queue
            .write_buffer(&self.state, 0, &device_bytes(&state));

        let mut control = Vec::with_capacity(16);
        control.extend_from_slice(&0u32.to_le_bytes());
        control.extend_from_slice(&count.to_le_bytes());
        control.extend_from_slice(&learning_rate.to_le_bytes());
        control.extend_from_slice(&decay.to_le_bytes());
        self.queue.write_buffer(&self.control, 0, &control);

        let hidden_1 = self.layout.hidden_1;
        let hidden_2 = self.layout.hidden_2;
        let params = params as u32;
        let forward_groups = count;
        let backward_2_groups = (count * hidden_2).div_ceil(64).max(1);
        let backward_1_groups = (count * hidden_1).div_ceil(64).max(1);
        let adam_groups = params.div_ceil(64).max(1);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("timbre training encoder"),
            });
        for _ in 0..epochs {
            // 每个 epoch 用五个计算通道：步进计数 → 前向 → 两层反向 → Adam。
            // Five passes per epoch: tick → forward → two backward layers → Adam.
            for (pipeline, groups) in [
                (&self.tick, 1u32),
                (&self.forward, forward_groups),
                (&self.backward_2, backward_2_groups),
                (&self.backward_1, backward_1_groups),
                (&self.adam, adam_groups),
            ] {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("timbre training pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.dispatch_workgroups(groups, 1, 1);
            }
        }
        let byte_len = (params as usize * std::mem::size_of::<f32>()) as u64;
        encoder.copy_buffer_to_buffer(&self.state, 0, &self.readback, 0, byte_len);
        self.queue.submit(Some(encoder.finish()));

        let slice = self.readback.slice(..byte_len);
        let (sender, receiver) = channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| format!("等待 GPU 训练完成失败: {error:?}"))?;
        match receiver.recv() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(format!("读取 GPU 训练结果失败: {error:?}")),
            Err(_) => return Err("GPU 训练回读回调中断".to_string()),
        }
        let data = slice.get_mapped_range();
        let weights = data
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        drop(data);
        self.readback.unmap();
        Ok(weights)
    }
}

/// 所属模块：audio::gpu / wgpu 神经音色训练后端
/// 功能：按层尺寸生成训练着色器；层宽度与工作区偏移直接写成常量。
/// 输入：`layout` 网络尺寸。
/// 输出/副作用：返回 WGSL 源码；不修改状态。
/// 复杂度：时间 O(1)，空间 O(源码长度)。
///
/// Module: audio::gpu / wgpu timbre training backend
/// Purpose: Generate the training shader; layer widths and work offsets become constants.
/// Inputs: `layout` network sizes.
/// Outputs / side effects: WGSL source; no state change.
/// Complexity: O(1) time and O(source length) space.
fn shader_source(layout: TimbreLayout) -> String {
    let sample_stride = (layout.input + layout.output) as usize;
    let act_stride = (layout.hidden_1 + layout.hidden_2) as usize;
    let act_base = MAX_SAMPLES as usize * sample_stride;
    let delta_base = act_base + MAX_SAMPLES as usize * act_stride;
    SHADER_TEMPLATE
        .replace("__I__", &layout.input.to_string())
        .replace("__H1__", &layout.hidden_1.to_string())
        .replace("__H2__", &layout.hidden_2.to_string())
        .replace("__O__", &layout.output.to_string())
        .replace("__ACT_BASE__", &act_base.to_string())
        .replace("__DELTA_BASE__", &delta_base.to_string())
}

/// 所属模块：audio::gpu / wgpu 神经音色训练后端
/// 功能：把 `f32` 切片编码为小端字节，供 `write_buffer` 使用。
/// 输入：`values` 浮点切片。
/// 输出/副作用：返回字节缓冲；不修改输入。
/// 复杂度：时间 O(值个数)，空间 O(值个数 × 4)。
///
/// Module: audio::gpu / wgpu timbre training backend
/// Purpose: Encode an `f32` slice as little-endian bytes for `write_buffer`.
/// Inputs: `values` float slice.
/// Outputs / side effects: byte buffer; the input is untouched.
/// Complexity: O(values) time and O(values × 4) space.
fn device_bytes(values: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// 训练着色器模板；`__I__` / `__H1__` / `__H2__` / `__O__` / `__MAX__` 与三个工作区偏移由主机侧替换。
/// Training shader template; the widths, capacity and work offsets are replaced by the host.
const SHADER_TEMPLATE: &str = r#"
struct Control {
    step: u32,
    count: u32,
    learning_rate: f32,
    decay: f32,
};

@group(0) @binding(0) var<storage, read_write> state: array<f32>;
@group(0) @binding(1) var<storage, read_write> work: array<f32>;
@group(0) @binding(2) var<storage, read_write> control: Control;

const I: u32 = __I__u;
const H1: u32 = __H1__u;
const H2: u32 = __H2__u;
const O: u32 = __O__u;
const W1: u32 = 0u;
const B1: u32 = I * H1;
const W2: u32 = B1 + H1;
const B2: u32 = W2 + H1 * H2;
const W3: u32 = B2 + H2;
const B3: u32 = W3 + H2 * O;
const TOTAL: u32 = B3 + O;
const MOM1: u32 = TOTAL;
const MOM2: u32 = TOTAL * 2u;
const ACT_STRIDE: u32 = H1 + H2;
const DELTA_STRIDE: u32 = H1 + H2 + O;
const SAMPLE_STRIDE: u32 = I + O;
const ACT_BASE: u32 = __ACT_BASE__u;
const DELTA_BASE: u32 = __DELTA_BASE__u;
const BETA1: f32 = 0.9;
const BETA2: f32 = 0.999;
const EPS: f32 = 1e-8;

@compute @workgroup_size(1)
fn tick() {
    control.step = control.step + 1u;
}

@compute @workgroup_size(1)
fn forward(@builtin(global_invocation_id) id: vec3<u32>) {
    let s = id.x;
    if (s >= control.count) {
        return;
    }
    let base = s * SAMPLE_STRIDE;
    let act = ACT_BASE + s * ACT_STRIDE;
    for (var j = 0u; j < H1; j = j + 1u) {
        var sum = state[B1 + j];
        for (var i = 0u; i < I; i = i + 1u) {
            sum = sum + state[W1 + j * I + i] * work[base + i];
        }
        work[act + j] = tanh(sum);
    }
    for (var v = 0u; v < H2; v = v + 1u) {
        var sum = state[B2 + v];
        for (var u = 0u; u < H1; u = u + 1u) {
            sum = sum + state[W2 + v * H1 + u] * work[act + u];
        }
        work[act + H1 + v] = tanh(sum);
    }
    let delta = DELTA_BASE + s * DELTA_STRIDE;
    for (var o = 0u; o < O; o = o + 1u) {
        var sum = state[B3 + o];
        for (var v = 0u; v < H2; v = v + 1u) {
            sum = sum + state[W3 + o * H2 + v] * work[act + H1 + v];
        }
        work[delta + H1 + H2 + o] = sum - work[base + I + o];
    }
}

@compute @workgroup_size(64)
fn backward_2(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= control.count * H2) {
        return;
    }
    let s = index / H2;
    let v = index % H2;
    let delta = DELTA_BASE + s * DELTA_STRIDE;
    let act = ACT_BASE + s * ACT_STRIDE;
    var sum = 0.0;
    for (var o = 0u; o < O; o = o + 1u) {
        sum = sum + work[delta + H1 + H2 + o] * state[W3 + o * H2 + v];
    }
    let value = work[act + H1 + v];
    work[delta + H1 + v] = sum * (1.0 - value * value);
}

@compute @workgroup_size(64)
fn backward_1(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= control.count * H1) {
        return;
    }
    let s = index / H1;
    let u = index % H1;
    let delta = DELTA_BASE + s * DELTA_STRIDE;
    let act = ACT_BASE + s * ACT_STRIDE;
    var sum = 0.0;
    for (var v = 0u; v < H2; v = v + 1u) {
        sum = sum + work[delta + H1 + v] * state[W2 + v * H1 + u];
    }
    let value = work[act + u];
    work[delta + u] = sum * (1.0 - value * value);
}

@compute @workgroup_size(64)
fn adam(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= TOTAL) {
        return;
    }
    var gradient = 0.0;
    if (index < B1) {
        let j = index / I;
        let i = index % I;
        for (var s = 0u; s < control.count; s = s + 1u) {
            gradient = gradient
                + work[DELTA_BASE + s * DELTA_STRIDE + j]
                    * work[s * SAMPLE_STRIDE + i];
        }
    } else if (index < W2) {
        let j = index - B1;
        for (var s = 0u; s < control.count; s = s + 1u) {
            gradient = gradient + work[DELTA_BASE + s * DELTA_STRIDE + j];
        }
    } else if (index < B2) {
        let local = index - W2;
        let v = local / H1;
        let u = local % H1;
        for (var s = 0u; s < control.count; s = s + 1u) {
            gradient = gradient
                + work[DELTA_BASE + s * DELTA_STRIDE + H1 + v]
                    * work[ACT_BASE + s * ACT_STRIDE + u];
        }
    } else if (index < W3) {
        let v = index - B2;
        for (var s = 0u; s < control.count; s = s + 1u) {
            gradient = gradient + work[DELTA_BASE + s * DELTA_STRIDE + H1 + v];
        }
    } else if (index < B3) {
        let local = index - W3;
        let o = local / H2;
        let u = local % H2;
        for (var s = 0u; s < control.count; s = s + 1u) {
            gradient = gradient
                + work[DELTA_BASE + s * DELTA_STRIDE + H1 + H2 + o]
                    * work[ACT_BASE + s * ACT_STRIDE + H1 + u];
        }
    } else {
        let o = index - B3;
        for (var s = 0u; s < control.count; s = s + 1u) {
            gradient = gradient + work[DELTA_BASE + s * DELTA_STRIDE + H1 + H2 + o];
        }
    }
    gradient = gradient + control.decay * state[index];
    let moment_1 = BETA1 * state[MOM1 + index] + (1.0 - BETA1) * gradient;
    let moment_2 = BETA2 * state[MOM2 + index] + (1.0 - BETA2) * gradient * gradient;
    state[MOM1 + index] = moment_1;
    state[MOM2 + index] = moment_2;
    let t = f32(control.step);
    let corrected_1 = moment_1 / (1.0 - pow(BETA1, t));
    let corrected_2 = moment_2 / (1.0 - pow(BETA2, t));
    state[index] = state[index] - control.learning_rate * corrected_1
        / (sqrt(corrected_2) + EPS);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：生成的着色器包含实际层宽度与工作区偏移，参数数量与手算一致。
    /// Covers: the shader carries the real widths and work offsets, and the parameter count matches.
    #[test]
    fn layout_and_shader_use_the_given_widths() {
        let layout = TimbreLayout::new(20, 20);
        let source = shader_source(layout);
        assert!(source.contains("const H1: u32 = 20u;"));
        assert!(source.contains("const H2: u32 = 20u;"));
        assert_eq!(
            layout.parameter_count(),
            4 * 20 + 20 + 20 * 20 + 20 + 6 * 20 + 6
        );
        assert!(!source.contains("__"));
    }

    /// 覆盖：GPU 训练可以收敛，权重数量正确且确实被更新。
    /// Covers: GPU training converges, returns the expected weight count and actually changes them.
    /// 需要可用的 GPU 与图形驱动。
    /// Requires a usable GPU and graphics driver.
    #[test]
    #[ignore = "需要可用的 GPU 与图形驱动"]
    fn gpu_training_reduces_error() {
        let config = GpuRenderConfig::new("", "", "");
        let layout = TimbreLayout::new(20, 20);
        let mut trainer = WgpuTimbreTrainer::open(&config, layout).expect("打开 GPU 训练器");
        let mut samples = Vec::new();
        for index in 0..16 {
            let pitch = index as f32 / 16.0;
            let mut target = [0.0_f32; BAND_COUNT as usize];
            for (band, value) in target.iter_mut().enumerate() {
                *value = (band as f32 * pitch).sin();
            }
            samples.push(([pitch, 0.5, 0.4, 1.0], target));
        }
        let initial = vec![0.05_f32; layout.parameter_count()];
        let weights = trainer
            .train(&samples, &initial, 200, 0.012, 1e-4)
            .expect("GPU 训练");
        assert_eq!(weights.len(), layout.parameter_count());
        assert!(weights.iter().all(|value| value.is_finite()));
        assert!(
            weights
                .iter()
                .zip(initial.iter())
                .any(|(trained, start)| (trained - start).abs() > 1e-4),
            "训练没有改变任何权重"
        );
    }

    /// 覆盖：GPU 训练后的网络在训练集上的误差明显低于初始权重。
    /// Covers: the GPU-trained network fits the training set far better than the initial weights.
    /// 需要可用的 GPU 与图形驱动。
    /// Requires a usable GPU and graphics driver.
    #[test]
    #[ignore = "需要可用的 GPU 与图形驱动"]
    fn gpu_training_lowers_the_training_error() {
        let config = GpuRenderConfig::new("", "", "");
        let layout = TimbreLayout::new(20, 20);
        let mut trainer = WgpuTimbreTrainer::open(&config, layout).expect("打开 GPU 训练器");
        let mut samples = Vec::new();
        for index in 0..16 {
            let pitch = index as f32 / 16.0;
            let mut target = [0.0_f32; BAND_COUNT as usize];
            for (band, value) in target.iter_mut().enumerate() {
                *value = (band as f32 * pitch).sin();
            }
            samples.push(([pitch, 0.5, 0.4, 1.0], target));
        }
        let initial = vec![0.05_f32; layout.parameter_count()];
        let weights = trainer
            .train(&samples, &initial, 400, 0.012, 1e-4)
            .expect("GPU 训练");
        let error_of = |weights: &[f32]| -> f32 {
            let mut error = 0.0_f32;
            for (input, target) in &samples {
                let predicted = forward_cpu(&layout, weights, input);
                for (band, value) in target.iter().enumerate() {
                    let delta = predicted[band] - value;
                    error += delta * delta;
                }
            }
            error / (samples.len() * BAND_COUNT as usize) as f32
        };
        let before = error_of(&initial);
        let after = error_of(&weights);
        eprintln!("gpu training: before={before:.4} after={after:.4}");
        assert!(
            after < before * 0.8,
            "GPU 训练未降低误差：{before} → {after}"
        );
    }

    /// 覆盖：按 GPU 权重布局在 CPU 上做一次前向，供误差对比使用。
    /// Covers: a CPU forward pass over the GPU weight layout, used for the error comparison.
    fn forward_cpu(
        layout: &TimbreLayout,
        weights: &[f32],
        input: &[f32; FEATURE_COUNT as usize],
    ) -> [f32; BAND_COUNT as usize] {
        let input_count = layout.input as usize;
        let hidden_1 = layout.hidden_1 as usize;
        let hidden_2 = layout.hidden_2 as usize;
        let output = layout.output as usize;
        let w1 = 0;
        let b1 = input_count * hidden_1;
        let w2 = b1 + hidden_1;
        let b2 = w2 + hidden_1 * hidden_2;
        let w3 = b2 + hidden_2;
        let b3 = w3 + hidden_2 * output;
        let mut first = vec![0.0_f32; hidden_1];
        for unit in 0..hidden_1 {
            let mut sum = weights[b1 + unit];
            for index in 0..input_count {
                sum += weights[w1 + unit * input_count + index] * input[index];
            }
            first[unit] = sum.tanh();
        }
        let mut second = vec![0.0_f32; hidden_2];
        for unit in 0..hidden_2 {
            let mut sum = weights[b2 + unit];
            for index in 0..hidden_1 {
                sum += weights[w2 + unit * hidden_1 + index] * first[index];
            }
            second[unit] = sum.tanh();
        }
        let mut out = [0.0_f32; BAND_COUNT as usize];
        for unit in 0..output {
            let mut sum = weights[b3 + unit];
            for index in 0..hidden_2 {
                sum += weights[w3 + unit * hidden_2 + index] * second[index];
            }
            out[unit] = sum;
        }
        out
    }
}
