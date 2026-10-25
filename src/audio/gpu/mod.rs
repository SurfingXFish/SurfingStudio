//! 所属模块：audio / GPU 渲染
//! 文件用途：
//! - 提供 GPU 渲染运行时框架：设备与驱动枚举、图形 API 选择以及主总线 GPU 处理入口。
//! - 定义 `GpuApi` / `GpuDeviceInfo` / `GpuRenderConfig` 等跨层数据契约。
//! - 只在离线渲染与导出路径使用；实时音频回调内不做任何 GPU 调用。
//!
//! 函数清单：
//! - `GpuApi::ALL` / `id` / `label` / `from_id`：图形 API 标识与展示文本。
//! - `GpuDeviceInfo::key` / `driver_label` / `device_type_label`：设备去重键与展示文本。
//! - `GpuRenderConfig::new`：把设置中的字符串转换为渲染配置。
//! - `GpuRuntime::standard` / `devices` / `apply_master_limiter`：注册后端、枚举设备、执行 GPU 处理。
//!
//! Module: audio / GPU rendering
//! Purpose:
//! - Provide the GPU rendering runtime framework: device/driver enumeration, graphics-API selection
//!   and the master-bus GPU processing entry point.
//! - Define the shared contracts `GpuApi` / `GpuDeviceInfo` / `GpuRenderConfig`.
//! - Used by offline rendering and export only; the realtime audio callback never touches the GPU.
//!
//! Functions:
//! - `GpuApi::ALL` / `id` / `label` / `from_id`: graphics-API identity and display text.
//! - `GpuDeviceInfo::key` / `driver_label` / `device_type_label`: de-duplication key and display text.
//! - `GpuRenderConfig::new`: convert the persisted setting strings into a render config.
//! - `GpuRuntime::standard` / `devices` / `apply_master_limiter`: register backends, enumerate
//!   devices and run GPU processing.

mod backends;

use backends::wgpu_compute::WgpuComputeBackend;

/// 图形 API 标识；与 `wgpu::Backend` 一一对应。
/// Graphics-API identity; maps one-to-one onto `wgpu::Backend`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GpuApi {
    DirectX12,
    Vulkan,
    OpenGl,
    Metal,
}

impl GpuApi {
    /// 全部图形 API；UI 下拉框按此顺序展示。
    /// Every graphics API; the settings dropdown lists them in this order.
    pub const ALL: [GpuApi; 4] = [
        GpuApi::DirectX12,
        GpuApi::Vulkan,
        GpuApi::OpenGl,
        GpuApi::Metal,
    ];

    /// 所属模块：audio / GPU 渲染
    /// 功能：返回持久化用的稳定标识。
    /// 输入：`self`。
    /// 输出/副作用：静态字符串；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / GPU rendering
    /// Purpose: Return the stable id used by persisted settings.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn id(self) -> &'static str {
        match self {
            GpuApi::DirectX12 => "dx12",
            GpuApi::Vulkan => "vulkan",
            GpuApi::OpenGl => "opengl",
            GpuApi::Metal => "metal",
        }
    }

    /// 所属模块：audio / GPU 渲染
    /// 功能：返回面向用户的 API 名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / GPU rendering
    /// Purpose: Return the user-facing API name.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            GpuApi::DirectX12 => "DirectX 12",
            GpuApi::Vulkan => "Vulkan",
            GpuApi::OpenGl => "OpenGL",
            GpuApi::Metal => "Metal",
        }
    }

    /// 所属模块：audio / GPU 渲染
    /// 功能：把持久化标识还原为图形 API。
    /// 输入：`id`：设置中保存的标识。
    /// 输出/副作用：匹配的 API；未知标识返回 `None`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / GPU rendering
    /// Purpose: Restore a graphics API from its persisted id.
    /// Inputs: `id`: the id stored in the settings.
    /// Outputs / side effects: the matching API, or `None` for an unknown id.
    /// Complexity: O(1) time and space.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|api| api.id() == id)
    }
}

/// 一个可用的 GPU 设备（同一块显卡在不同 API 下会各出现一次）。
/// One usable GPU device (the same card appears once per graphics API).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuDeviceInfo {
    /// 图形 API。
    /// Graphics API.
    pub api: GpuApi,
    /// 同一 API 内的枚举序号，用于区分同名设备。
    /// Enumeration index inside one API, distinguishing identically named devices.
    pub index: usize,
    /// 设备名称。
    /// Device name.
    pub name: String,
    /// 驱动名称。
    /// Driver name.
    pub driver: String,
    /// 驱动版本/附加信息。
    /// Driver version / additional info.
    pub driver_info: String,
    /// PCI 厂商 id。
    /// PCI vendor id.
    pub vendor: u32,
    /// 设备类型文本（独显 / 集显 / 虚拟 GPU 等）。
    /// Device-type text (discrete / integrated / virtual GPU, …).
    pub device_type: String,
}

impl GpuDeviceInfo {
    /// 所属模块：audio / GPU 渲染
    /// 功能：返回设置中保存的设备键（API + 名称 + 序号）。
    /// 输入：`self`。
    /// 输出/副作用：设备键字符串；不修改状态。
    /// 复杂度：时间 O(名称长度)，空间 O(名称长度)。
    ///
    /// Module: audio / GPU rendering
    /// Purpose: Return the device key persisted in the settings (API + name + index).
    /// Inputs: `self`.
    /// Outputs / side effects: key text; no state change.
    /// Complexity: O(name length) time and space.
    pub fn key(&self) -> String {
        format!("{}|{}|{}", self.api.id(), self.name, self.index)
    }

    /// 所属模块：audio / GPU 渲染
    /// 功能：返回同一块显卡在多个 API 下共用的名称（去掉 OpenGL 的 "/PCIe/SSE2" 等后缀）。
    /// 输入：`self`。
    /// 输出/副作用：显卡族名称；不修改状态。
    /// 复杂度：时间 O(名称长度)，空间 O(1)。
    ///
    /// Module: audio / GPU rendering
    /// Purpose: Return the card name shared across APIs (strips OpenGL suffixes such as
    /// "/PCIe/SSE2").
    /// Inputs: `self`.
    /// Outputs / side effects: the card family name; no state change.
    /// Complexity: O(name length) time and O(1) space.
    pub fn family_name(&self) -> &str {
        self.name.split('/').next().unwrap_or(&self.name).trim()
    }

    /// 所属模块：audio / GPU 渲染
    /// 功能：返回驱动展示文本（驱动名 + 版本）。
    /// 输入：`self`。
    /// 输出/副作用：展示文本；不修改状态。
    /// 复杂度：时间 O(驱动文本长度)，空间 O(驱动文本长度)。
    ///
    /// Module: audio / GPU rendering
    /// Purpose: Return the driver display text (name + version).
    /// Inputs: `self`.
    /// Outputs / side effects: display text; no state change.
    /// Complexity: O(driver text) time and space.
    pub fn driver_label(&self) -> String {
        if self.driver_info.is_empty() {
            self.driver.clone()
        } else if self.driver.is_empty() {
            self.driver_info.clone()
        } else {
            format!("{} ({})", self.driver, self.driver_info)
        }
    }
}

/// GPU 音频渲染配置；由全局设置构造，解析设备时按此匹配。
/// GPU audio-render config built from the global settings and used to match a device.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GpuRenderConfig {
    /// 指定的图形 API；`None` 表示自动。
    /// Requested graphics API; `None` means automatic.
    pub api: Option<GpuApi>,
    /// 设备键；空字符串表示自动选择第一块可用 GPU。
    /// Device key; an empty string selects the first available GPU.
    pub device_key: String,
    /// 期望的驱动展示文本；空字符串表示不限制。
    /// Expected driver display text; an empty string means no restriction.
    pub driver: String,
}

impl GpuRenderConfig {
    /// 所属模块：audio / GPU 渲染
    /// 功能：由设置字符串构造渲染配置。
    /// 输入：
    /// - `api`：图形 API 标识（未知或空表示自动）。
    /// - `device_key`：设备键。
    /// - `driver`：驱动展示文本。
    /// 输出/副作用：返回配置；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(传输文本长度)。
    ///
    /// Module: audio / GPU rendering
    /// Purpose: Build a render config from the persisted setting strings.
    /// Inputs:
    /// - `api`: graphics-API id (unknown/empty means automatic).
    /// - `device_key`: device key.
    /// - `driver`: driver display text.
    /// Outputs / side effects: the config; no state change.
    /// Complexity: O(1) time and O(text length) space.
    pub fn new(api: &str, device_key: &str, driver: &str) -> Self {
        Self {
            api: GpuApi::from_id(api),
            device_key: device_key.to_string(),
            driver: driver.to_string(),
        }
    }
}

/// GPU 后端能力描述。
/// Capability description of a GPU backend.
#[derive(Clone, Debug)]
pub struct GpuBackendInfo {
    /// 稳定标识。
    /// Stable id.
    pub id: &'static str,
    /// 展示名称。
    /// Display name.
    pub name: &'static str,
}

/// GPU 后端接口：枚举设备并执行主总线处理。
/// GPU backend interface: enumerate devices and process the master bus.
pub trait GpuBackend {
    /// 所属模块：audio / GPU 渲染
    /// 功能：返回后端能力描述。
    /// 输入：`self`。
    /// 输出/副作用：静态描述；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / GPU rendering
    /// Purpose: Return the backend capability description.
    /// Inputs: `self`.
    /// Outputs / side effects: static description; no state change.
    /// Complexity: O(1) time and space.
    fn info(&self) -> GpuBackendInfo;

    /// 所属模块：audio / GPU 渲染
    /// 功能：枚举当前机器上可用于计算着色器的 GPU。
    /// 输入：`self`。
    /// 输出/副作用：设备列表；失败时返回中文原因。
    /// 复杂度：时间 O(枚举 API × 设备数)，空间 O(设备数)。
    ///
    /// Module: audio / GPU rendering
    /// Purpose: Enumerate GPUs able to run compute shaders on this machine.
    /// Inputs: `self`.
    /// Outputs / side effects: device list; a Chinese reason on failure.
    /// Complexity: O(APIs × devices) time and O(devices) space.
    fn devices(&self) -> Result<Vec<GpuDeviceInfo>, String>;

    /// 所属模块：audio / GPU 渲染
    /// 功能：在 GPU 上执行主总线软限幅，原地改写交错立体声样本。
    /// 输入：
    /// - `config`：设备/驱动/API 选择。
    /// - `samples`：交错立体声样本，长度会被截断到 4 字节对齐部分。
    /// 输出/副作用：原地写入 `samples`；失败时不保证内容。
    /// 复杂度：时间 O(样本数)，空间 O(样本数)（GPU 缓冲与回读各一份）。
    ///
    /// Module: audio / GPU rendering
    /// Purpose: Run the master-bus soft limiter on the GPU, rewriting interleaved samples in place.
    /// Inputs:
    /// - `config`: device/driver/API selection.
    /// - `samples`: interleaved stereo samples; only the 4-byte aligned prefix is processed.
    /// Outputs / side effects: rewrites `samples` in place; contents are unspecified on failure.
    /// Complexity: O(samples) time and O(samples) space (GPU buffer plus read-back).
    fn apply_master_limiter(
        &self,
        config: &GpuRenderConfig,
        samples: &mut [f32],
    ) -> Result<(), String>;
}

/// GPU 渲染运行时：持有后端注册表，向上层暴露唯一入口。
/// GPU rendering runtime: owns the backend registry and exposes the single upper-layer entry.
pub struct GpuRuntime {
    backends: Vec<Box<dyn GpuBackend>>,
}

impl GpuRuntime {
    /// 所属模块：audio / GPU 渲染
    /// 功能：构造注册了全部后端（当前为 wgpu 计算后端）的运行时。
    /// 输入：无。
    /// 输出/副作用：返回运行时；不触碰 GPU 设备。
    /// 复杂度：时间 O(1)，空间 O(后端数)。
    ///
    /// Module: audio / GPU rendering
    /// Purpose: Build the runtime with every backend (currently the wgpu compute backend).
    /// Inputs: none.
    /// Outputs / side effects: the runtime; no GPU device is touched.
    /// Complexity: O(1) time and O(backends) space.
    pub fn standard() -> Self {
        Self {
            backends: vec![Box::new(WgpuComputeBackend::new())],
        }
    }

    /// 所属模块：audio / GPU 渲染
    /// 功能：合并所有后端枚举到的设备，按 API 与名称排序。
    /// 输入：`self`。
    /// 输出/副作用：设备列表；所有后端都失败时返回最后一条错误。
    /// 复杂度：时间 O(后端数 × 设备数 log 设备数)，空间 O(设备数)。
    ///
    /// Module: audio / GPU rendering
    /// Purpose: Merge the devices of every backend and sort them by API and name.
    /// Inputs: `self`.
    /// Outputs / side effects: device list; the last error when every backend fails.
    /// Complexity: O(backends × devices log devices) time and O(devices) space.
    pub fn devices(&self) -> Result<Vec<GpuDeviceInfo>, String> {
        let mut devices = Vec::new();
        let mut last_error: Option<String> = None;
        for backend in &self.backends {
            match backend.devices() {
                Ok(mut found) => devices.append(&mut found),
                Err(error) => last_error = Some(error),
            }
        }
        if devices.is_empty() {
            if let Some(error) = last_error {
                return Err(error);
            }
        }
        devices.sort_by(|a, b| a.api.id().cmp(b.api.id()).then(a.name.cmp(&b.name)));
        Ok(devices)
    }

    /// 所属模块：audio / GPU 渲染
    /// 功能：把主总线软限幅交给第一个能处理的后端执行。
    /// 输入：
    /// - `config`：设备/驱动/API 选择。
    /// - `samples`：交错立体声样本。
    /// 输出/副作用：原地改写 `samples`；所有后端失败时返回最后一条错误。
    /// 复杂度：时间 O(后端数 × 样本数)，空间 O(样本数)。
    ///
    /// Module: audio / GPU rendering
    /// Purpose: Let the first capable backend apply the master-bus soft limiter.
    /// Inputs:
    /// - `config`: device/driver/API selection.
    /// - `samples`: interleaved stereo samples.
    /// Outputs / side effects: rewrites `samples` in place; the last error when every backend fails.
    /// Complexity: O(backends × samples) time and O(samples) space.
    pub fn apply_master_limiter(
        &self,
        config: &GpuRenderConfig,
        samples: &mut [f32],
    ) -> Result<(), String> {
        let mut last_error: Option<String> = None;
        for backend in &self.backends {
            match backend.apply_master_limiter(config, samples) {
                Ok(()) => return Ok(()),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| "没有可用的 GPU 渲染后端".to_string()))
    }
}
