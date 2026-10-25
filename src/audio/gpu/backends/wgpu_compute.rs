//! 所属模块：audio::gpu / wgpu 计算后端
//! 文件用途：
//! - 通过 wgpu 枚举本机图形适配器（名称、驱动、API），并过滤掉纯软件设备。
//! - 使用 WGSL 计算着色器在 GPU 上执行主总线软限幅，供离线导出使用。
//! - 设备不可用或校验失败时返回中文错误，由调用方回退 CPU 渲染。
//!
//! 函数清单：
//! - `WgpuComputeBackend::new`：构造后端。
//! - `GpuBackend::info` / `devices` / `apply_master_limiter`：后端接口实现。
//! - `create_instance`：按 API 创建 wgpu 实例。
//! - `api_from_backend` / `backends_for_api`：API 与 wgpu 后端位标志互转。
//! - `device_type_label`：设备类型中文文本。
//! - `select_adapter`：按配置选择适配器。
//!
//! Module: audio::gpu / wgpu compute backend
//! Purpose:
//! - Enumerate native graphics adapters (name, driver, API) through wgpu, filtering software devices.
//! - Run the master-bus soft limiter on the GPU with a WGSL compute shader for offline export.
//! - Return Chinese errors when the device is unavailable or invalid, so the caller falls back to CPU.
//!
//! Functions:
//! - `WgpuComputeBackend::new`: construct the backend.
//! - `GpuBackend::info` / `devices` / `apply_master_limiter`: backend interface implementation.
//! - `create_instance`: create a wgpu instance for the requested APIs.
//! - `api_from_backend` / `backends_for_api`: convert between the API enum and wgpu backend flags.
//! - `device_type_label`: Chinese device-type text.
//! - `select_adapter`: pick an adapter for the config.

use super::super::{GpuApi, GpuBackend, GpuBackendInfo, GpuDeviceInfo, GpuRenderConfig};
use std::sync::mpsc::channel;

/// 单次向 GPU 提交的样本数上限（约 4 MiB）；长工程分块处理，避免超过存储缓冲上限。
/// Upper bound of samples per GPU submission (about 4 MiB); long exports are processed in chunks so
/// they never exceed the storage-buffer binding limits.
const CHUNK_SAMPLES: usize = 1 << 20;

/// 主总线软限幅着色器，与 `audio/engine/render.rs` 的 `soft_clip` 保持一致。
/// Master-bus soft limiter shader, matching `soft_clip` in `audio/engine/render.rs`.
const LIMITER_SHADER: &str = r#"
const KNEE: f32 = 0.95;

@group(0) @binding(0) var<storage, read_write> samples: array<f32>;

fn soft_clip(value: f32) -> f32 {
    let magnitude = abs(value);
    if (magnitude <= KNEE) {
        return value;
    }
    let sign = select(1.0, -1.0, value < 0.0);
    let range = 1.0 - KNEE;
    return sign * (KNEE + range * (1.0 - exp(-(magnitude - KNEE) / range)));
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (f32(index) >= f32(arrayLength(&samples))) {
        return;
    }
    samples[index] = soft_clip(samples[index]);
}
"#;

/// wgpu 计算后端。
/// wgpu compute backend.
pub struct WgpuComputeBackend;

impl WgpuComputeBackend {
    /// 所属模块：audio::gpu / wgpu 计算后端
    /// 功能：构造无状态后端；真正的设备在枚举或处理时才创建。
    /// 输入：无。
    /// 输出/副作用：返回后端实例；不触碰 GPU。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::gpu / wgpu compute backend
    /// Purpose: Construct the stateless backend; devices are created on enumeration or processing.
    /// Inputs: none.
    /// Outputs / side effects: the backend; no GPU work.
    /// Complexity: O(1) time and space.
    pub fn new() -> Self {
        Self
    }
}

impl GpuBackend for WgpuComputeBackend {
    /// 所属模块：audio::gpu / wgpu 计算后端
    /// 功能：返回后端能力描述。
    /// 输入：`self`。
    /// 输出/副作用：静态描述；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio::gpu / wgpu compute backend
    /// Purpose: Return the backend capability description.
    /// Inputs: `self`.
    /// Outputs / side effects: static description; no state change.
    /// Complexity: O(1) time and space.
    fn info(&self) -> GpuBackendInfo {
        GpuBackendInfo {
            id: "wgpu",
            name: "wgpu 计算着色器",
        }
    }

    /// 所属模块：audio::gpu / wgpu 计算后端
    /// 功能：枚举全部原生图形适配器并转换为设备信息。
    /// 输入：`self`。
    /// 输出/副作用：设备列表；没有可用适配器时返回中文错误。
    /// 复杂度：时间 O(枚举 API × 设备数)，空间 O(设备数)。
    ///
    /// Module: audio::gpu / wgpu compute backend
    /// Purpose: Enumerate every native graphics adapter and convert it to device info.
    /// Inputs: `self`.
    /// Outputs / side effects: device list; a Chinese error when no adapter is usable.
    /// Complexity: O(APIs × devices) time and O(devices) space.
    fn devices(&self) -> Result<Vec<GpuDeviceInfo>, String> {
        let instance = create_instance(None);
        let backends = host_backends();
        let adapters = pollster::block_on(instance.enumerate_adapters(backends));
        let mut counters = [0usize; 4];
        let mut devices = Vec::new();
        for adapter in adapters {
            let info = adapter.get_info();
            let Some(api) = api_from_backend(info.backend) else {
                continue;
            };
            // 纯 CPU 适配器（如操作系统软件光栅化）不属于 GPU 加速目标。
            // Pure-CPU adapters (OS software rasterisers) are not GPU acceleration targets.
            if info.device_type == wgpu::DeviceType::Cpu {
                continue;
            }
            let slot = api_slot(api);
            let index = counters[slot];
            counters[slot] += 1;
            devices.push(GpuDeviceInfo {
                api,
                index,
                name: info.name,
                driver: info.driver,
                driver_info: info.driver_info,
                vendor: info.vendor,
                device_type: device_type_label(info.device_type).to_string(),
            });
        }
        if devices.is_empty() {
            return Err("未检测到支持计算着色器的 GPU 设备".to_string());
        }
        Ok(devices)
    }

    /// 所属模块：audio::gpu / wgpu 计算后端
    /// 功能：在所选 GPU 上分块执行主总线软限幅，并回读结果。
    /// 输入：
    /// - `config`：设备/驱动/API 选择。
    /// - `samples`：交错立体声样本。
    /// 输出/副作用：原地改写 `samples`；失败时返回中文原因且内容可能未改动。
    /// 复杂度：时间 O(样本数)，空间 O(min(样本数, CHUNK_SAMPLES))。
    ///
    /// Module: audio::gpu / wgpu compute backend
    /// Purpose: Run the master-bus soft limiter on the chosen GPU in chunks and read the result back.
    /// Inputs:
    /// - `config`: device/driver/API selection.
    /// - `samples`: interleaved stereo samples.
    /// Outputs / side effects: rewrites `samples` in place; a Chinese reason on failure (contents may
    /// be untouched).
    /// Complexity: O(samples) time and O(min(samples, CHUNK_SAMPLES)) space.
    fn apply_master_limiter(
        &self,
        config: &GpuRenderConfig,
        samples: &mut [f32],
    ) -> Result<(), String> {
        if samples.is_empty() {
            return Ok(());
        }
        let instance = create_instance(config.api);
        let backends = match config.api {
            Some(api) => backends_for_api(api),
            None => host_backends(),
        };
        let adapters = pollster::block_on(instance.enumerate_adapters(backends));
        if adapters.is_empty() {
            return Err("未找到可用的 GPU 适配器".to_string());
        }
        let adapter = select_adapter(&adapters, config)
            .ok_or_else(|| "未找到设置中指定的 GPU 设备".to_string())?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("SurfingStudio GPU audio"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            ..Default::default()
        }))
        .map_err(|error| format!("创建 GPU 设备失败: {error:?}"))?;

        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("master limiter"),
            source: wgpu::ShaderSource::Wgsl(LIMITER_SHADER.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("master limiter pipeline"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let layout = pipeline.get_bind_group_layout(0);
        if let Some(error) = pollster::block_on(error_scope.pop()) {
            return Err(format!("GPU 着色器校验失败: {error}"));
        }

        for chunk in samples.chunks_mut(CHUNK_SAMPLES) {
            let byte_len = std::mem::size_of_val(chunk) as u64;
            let bytes: Vec<u8> = chunk.iter().flat_map(|value| value.to_le_bytes()).collect();
            let storage = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("master limiter samples"),
                size: byte_len,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&storage, 0, &bytes);
            let staging = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("master limiter readback"),
                size: byte_len,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("master limiter bind group"),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: storage.as_entire_binding(),
                }],
            });
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("master limiter encoder"),
            });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("master limiter pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                let groups = (chunk.len() as u64).div_ceil(64) as u32;
                pass.dispatch_workgroups(groups.max(1), 1, 1);
            }
            encoder.copy_buffer_to_buffer(&storage, 0, &staging, 0, byte_len);
            queue.submit(Some(encoder.finish()));

            let slice = staging.slice(..);
            let (tx, rx) = channel();
            slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
            device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|error| format!("等待 GPU 完成失败: {error:?}"))?;
            match rx.recv() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => return Err(format!("读取 GPU 结果失败: {error:?}")),
                Err(_) => return Err("GPU 回读回调中断".to_string()),
            }
            {
                let data = slice.get_mapped_range();
                for (index, chunk_bytes) in data.chunks_exact(4).enumerate() {
                    if index >= chunk.len() {
                        break;
                    }
                    chunk[index] = f32::from_le_bytes([
                        chunk_bytes[0],
                        chunk_bytes[1],
                        chunk_bytes[2],
                        chunk_bytes[3],
                    ]);
                }
            }
            staging.unmap();
        }
        Ok(())
    }
}

/// 所属模块：audio::gpu / wgpu 计算后端
/// 功能：按 API 创建 wgpu 实例；`None` 表示全部原生后端。
/// 输入：`api`：指定图形 API（可选）。
/// 输出/副作用：返回实例；不创建设备。
/// 复杂度：时间 O(已启用后端数)，空间 O(1)。
///
/// Module: audio::gpu / wgpu compute backend
/// Purpose: Create a wgpu instance for the requested API; `None` means every native backend.
/// Inputs: `api`: requested graphics API (optional).
/// Outputs / side effects: the instance; no device is created.
/// Complexity: O(enabled backends) time and O(1) space.
fn create_instance(api: Option<GpuApi>) -> wgpu::Instance {
    let backends = match api {
        Some(api) => backends_for_api(api),
        None => host_backends(),
    };
    wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        flags: wgpu::InstanceFlags::from_build_config(),
        backend_options: wgpu::BackendOptions::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        display: None,
    })
}

/// 所属模块：audio::gpu / wgpu 计算后端
/// 功能：返回当前平台可用的 wgpu 后端位标志。
/// 输入：无。
/// 输出/副作用：位标志；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio::gpu / wgpu compute backend
/// Purpose: Return the wgpu backend flags available on this platform.
/// Inputs: none.
/// Outputs / side effects: backend flags; no state change.
/// Complexity: O(1) time and space.
fn host_backends() -> wgpu::Backends {
    wgpu::Backends::VULKAN | wgpu::Backends::DX12 | wgpu::Backends::GL | wgpu::Backends::METAL
}

/// 所属模块：audio::gpu / wgpu 计算后端
/// 功能：把图形 API 转换为 wgpu 后端位标志。
/// 输入：`api`：图形 API。
/// 输出/副作用：位标志；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio::gpu / wgpu compute backend
/// Purpose: Convert a graphics API into wgpu backend flags.
/// Inputs: `api`: graphics API.
/// Outputs / side effects: backend flags; no state change.
/// Complexity: O(1) time and space.
fn backends_for_api(api: GpuApi) -> wgpu::Backends {
    match api {
        GpuApi::DirectX12 => wgpu::Backends::DX12,
        GpuApi::Vulkan => wgpu::Backends::VULKAN,
        GpuApi::OpenGl => wgpu::Backends::GL,
        GpuApi::Metal => wgpu::Backends::METAL,
    }
}

/// 所属模块：audio::gpu / wgpu 计算后端
/// 功能：把 wgpu 后端转换为图形 API。
/// 输入：`backend`：wgpu 后端。
/// 输出/副作用：对应 API；未映射的后端返回 `None`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio::gpu / wgpu compute backend
/// Purpose: Convert a wgpu backend into the graphics-API enum.
/// Inputs: `backend`: wgpu backend.
/// Outputs / side effects: the API, or `None` for an unmapped backend.
/// Complexity: O(1) time and space.
fn api_from_backend(backend: wgpu::Backend) -> Option<GpuApi> {
    match backend {
        wgpu::Backend::Dx12 => Some(GpuApi::DirectX12),
        wgpu::Backend::Vulkan => Some(GpuApi::Vulkan),
        wgpu::Backend::Gl => Some(GpuApi::OpenGl),
        wgpu::Backend::Metal => Some(GpuApi::Metal),
        _ => None,
    }
}

/// 所属模块：audio::gpu / wgpu 计算后端
/// 功能：返回 API 在计数器中的槽位。
/// 输入：`api`：图形 API。
/// 输出/副作用：0..3 的槽位；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio::gpu / wgpu compute backend
/// Purpose: Return the counter slot of an API.
/// Inputs: `api`: graphics API.
/// Outputs / side effects: a slot in 0..3; no state change.
/// Complexity: O(1) time and space.
fn api_slot(api: GpuApi) -> usize {
    match api {
        GpuApi::DirectX12 => 0,
        GpuApi::Vulkan => 1,
        GpuApi::OpenGl => 2,
        GpuApi::Metal => 3,
    }
}

/// 所属模块：audio::gpu / wgpu 计算后端
/// 功能：返回设备类型的中文文本。
/// 输入：`device_type`：wgpu 设备类型。
/// 输出/副作用：静态文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio::gpu / wgpu compute backend
/// Purpose: Return the Chinese text of a device type.
/// Inputs: `device_type`: wgpu device type.
/// Outputs / side effects: static text; no state change.
/// Complexity: O(1) time and space.
fn device_type_label(device_type: wgpu::DeviceType) -> &'static str {
    match device_type {
        wgpu::DeviceType::DiscreteGpu => "独立显卡",
        wgpu::DeviceType::IntegratedGpu => "集成显卡",
        wgpu::DeviceType::VirtualGpu => "虚拟 GPU",
        wgpu::DeviceType::Cpu => "软件渲染",
        wgpu::DeviceType::Other => "其他设备",
    }
}

/// 所属模块：audio::gpu / wgpu 计算后端
/// 功能：按设备键（其次 API + 名称 + 驱动）选择适配器，找不到时退回第一块非 CPU 适配器。
/// 输入：
/// - `adapters`：候选适配器。
/// - `config`：设备/驱动/API 选择。
/// 输出/副作用：选中的适配器；无候选时返回 `None`。
/// 复杂度：时间 O(适配器数)，空间 O(1)。
///
/// Module: audio::gpu / wgpu compute backend
/// Purpose: Pick an adapter by device key (then API + name + driver), falling back to the first
/// non-CPU adapter when nothing matches.
/// Inputs:
/// - `adapters`: candidate adapters.
/// - `config`: device/driver/API selection.
/// Outputs / side effects: the chosen adapter, or `None` when there is no candidate.
/// Complexity: O(adapters) time and O(1) space.
fn select_adapter(adapters: &[wgpu::Adapter], config: &GpuRenderConfig) -> Option<wgpu::Adapter> {
    let mut counters = [0usize; 4];
    let mut candidates: Vec<(wgpu::Adapter, GpuDeviceInfo)> = Vec::new();
    for adapter in adapters {
        let info = adapter.get_info();
        let Some(api) = api_from_backend(info.backend) else {
            continue;
        };
        if info.device_type == wgpu::DeviceType::Cpu {
            continue;
        }
        let slot = api_slot(api);
        let index = counters[slot];
        counters[slot] += 1;
        candidates.push((
            adapter.clone(),
            GpuDeviceInfo {
                api,
                index,
                name: info.name,
                driver: info.driver,
                driver_info: info.driver_info,
                vendor: info.vendor,
                device_type: device_type_label(info.device_type).to_string(),
            },
        ));
    }
    if !config.device_key.is_empty() {
        if let Some((adapter, _)) = candidates
            .iter()
            .find(|(_, device)| device.key() == config.device_key)
        {
            return Some(adapter.clone());
        }
    }
    if let Some((adapter, _)) = candidates
        .iter()
        .find(|(_, device)| config.api.map(|api| device.api == api).unwrap_or(true))
    {
        return Some(adapter.clone());
    }
    candidates.into_iter().next().map(|(adapter, _)| adapter)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：真实 GPU 上枚举设备并对超过拐点的样本做软限幅。
    /// Covers: enumerate a real GPU and soft-limit samples above the knee.
    #[test]
    #[ignore = "需要可用的 GPU 与图形驱动"]
    fn wgpu_backend_limits_samples_on_a_real_gpu() {
        let backend = WgpuComputeBackend::new();
        let devices = backend.devices().expect("GPU 设备枚举失败");
        let device = devices.first().expect("至少需要一个 GPU");
        let config = GpuRenderConfig {
            api: Some(device.api),
            device_key: device.key(),
            driver: device.driver_label(),
        };
        let mut samples = vec![0.0_f32, 1.5, -1.5, 0.5];
        backend
            .apply_master_limiter(&config, &mut samples)
            .expect("GPU 软限幅失败");
        assert!(
            samples[0].abs() < 1e-6,
            "linear sample changed: {}",
            samples[0]
        );
        assert!(
            samples[1] > 0.95 && samples[1] < 1.0,
            "positive limit: {}",
            samples[1]
        );
        assert!(
            samples[2] < -0.95 && samples[2] > -1.0,
            "negative limit: {}",
            samples[2]
        );
        assert!((samples[3] - 0.5).abs() < 1e-6, "quiet sample changed");
    }
}
