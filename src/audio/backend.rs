//! 所属模块：audio / 系统输出
//! 文件用途：
//! - 枚举音频系统（系统默认 / WASAPI / ASIO）与输入、输出设备。
//! - 创建 cpal 输出流或 ASIO 输出流，适配采样格式并在断流时淡出/淡入。
//! - 用 `AudioRouter` 让多个工程窗口共用一个输出设备：只有当前激活的引擎会被渲染。
//!
//! 函数清单：
//! - `AudioSystem::id` / `label` / `from_id`：音频系统标识与展示文本。
//! - `audio_systems` / `output_devices` / `input_devices`：系统与设备枚举。
//! - `AudioRouter::new` / `register` / `activate` / `engine_slot` / `sample_rate` / `set_sample_rate`：多窗口音频路由。
//! - `start_output` / `start_output_with`：按选项选择采样率/缓冲并启动输出流。
//! - `start_cpal` / `start_asio`：cpal 与 ASIO 两条启动路径。
//! - `lock_engine`：在回调中短暂自旋等待引擎锁。
//! - `build<T>`：采样格式适配、回调渲染与断流淡入淡出。
//!
//! Module: audio / system output
//! Purpose:
//! - Enumerate audio systems (system default / WASAPI / ASIO) and input/output devices.
//! - Create either a cpal stream or an ASIO stream, adapt sample formats and fade across missed
//!   callbacks.
//! - Let several project windows share one output device through `AudioRouter`: only the active
//!   engine is rendered.
//!
//! Functions:
//! - `AudioSystem::id` / `label` / `from_id`: audio-system identity and labels.
//! - `audio_systems` / `output_devices` / `input_devices`: system and device enumeration.
//! - `AudioRouter::new` / `register` / `activate` / `engine_slot` / `sample_rate` / `set_sample_rate`: multi-window audio routing.
//! - `start_output` / `start_output_with`: pick sample rate/buffer according to options and start.
//! - `start_cpal` / `start_asio`: the cpal and ASIO launch paths.
//! - `lock_engine`: briefly spin for the engine lock inside the callback.
//! - `build<T>`: sample format conversion, callback rendering and dropout fades.

use crate::audio::engine::Engine;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

/// 采样率未知时使用的默认值（与 `Engine::new` 的默认采样率一致）。
/// Default sample rate used while the device rate is still unknown (matches `Engine::new`).
const DEFAULT_SAMPLE_RATE: f64 = 48_000.0;
/// 默认输出缓冲帧数。
/// Default output buffer size in frames.
const DEFAULT_BUFFER_FRAMES: u32 = 512;

/// 音频系统：决定使用哪一条宿主路径打开设备。
/// Audio system: selects which host path opens the device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AudioSystem {
    /// cpal 默认宿主（Windows 上为 WASAPI 默认设备）。
    /// cpal default host (the WASAPI default device on Windows).
    #[default]
    System,
    /// Windows Audio Session API。
    /// Windows Audio Session API.
    Wasapi,
    /// Steinberg ASIO：直接加载本机注册的驱动。
    /// Steinberg ASIO: loads locally registered drivers directly.
    Asio,
}

impl AudioSystem {
    /// 所属模块：audio / 系统输出 / 音频系统
    /// 功能：返回持久化用的系统标识。
    /// 输入：`self`。
    /// 输出/副作用：静态字符串；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / system output / audio system
    /// Purpose: Return the persisted system id.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn id(self) -> &'static str {
        match self {
            AudioSystem::System => "system",
            AudioSystem::Wasapi => "wasapi",
            AudioSystem::Asio => "asio",
        }
    }

    /// 所属模块：audio / 系统输出 / 音频系统
    /// 功能：返回界面展示名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / system output / audio system
    /// Purpose: Return the label shown in the settings window.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            AudioSystem::System => "系统默认",
            AudioSystem::Wasapi => "WASAPI（Windows）",
            AudioSystem::Asio => "ASIO（低延迟驱动）",
        }
    }

    /// 所属模块：audio / 系统输出 / 音频系统
    /// 功能：把持久化标识还原为音频系统；未知标识回退为系统默认。
    /// 输入：`id` 持久化标识。
    /// 输出/副作用：音频系统；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / system output / audio system
    /// Purpose: Restore the persisted id; unknown ids fall back to the system default.
    /// Inputs: `id` — persisted id.
    /// Outputs / side effects: the audio system; no state change.
    /// Complexity: O(1) time and space.
    pub fn from_id(id: &str) -> Self {
        match id {
            "wasapi" => AudioSystem::Wasapi,
            "asio" => AudioSystem::Asio,
            _ => AudioSystem::System,
        }
    }
}

/// 音频系统及其可用性。
/// Audio system and its availability.
#[derive(Clone, Debug)]
pub struct AudioSystemInfo {
    pub system: AudioSystem,
    pub available: bool,
    /// 不可用时给出原因或补充说明。
    /// Reason or note shown when unavailable.
    pub detail: String,
}

/// 输出流启动选项。
/// Options used to start the output stream.
#[derive(Clone, Debug, Default)]
pub struct AudioOptions {
    pub system: AudioSystem,
    /// 输出设备名称；`None` 使用系统默认设备。
    /// Output device name; `None` uses the system default.
    pub output_device: Option<String>,
    /// 期望采样率；`None` 使用设备默认值。
    /// Requested sample rate; `None` uses the device default.
    pub sample_rate: Option<u32>,
    /// 期望缓冲帧数；`None` 使用默认值。
    /// Requested buffer size in frames; `None` uses the default.
    pub buffer_frames: Option<u32>,
    /// 期望输出声道数（2 / 6 / 8）；`None` 或设备不支持时使用设备默认配置。
    /// Requested output channel count (2 / 6 / 8); `None` or an unsupported device falls back to the
    /// device default.
    pub output_channels: Option<u16>,
}

/// 输出流的后端句柄：cpal 流或 ASIO 流。
/// Backend handle of the output stream: a cpal stream or an ASIO stream.
// 只用于保活：字段从不读取，但一旦释放输出流就停止，因此保留句柄。
// Keep-alive only: the fields are never read, but dropping the handle stops the stream.
#[allow(dead_code)]
enum AudioStream {
    Cpal(cpal::Stream),
    #[cfg(target_os = "windows")]
    Asio(crate::audio::asio::AsioStream),
}

pub struct AudioOutput {
    /// Kept alive so the stream keeps running.
    _stream: AudioStream,
    pub description: String,
}

/// 所属模块：audio / 系统输出 / 音频系统
/// 功能：列出本机可选的音频系统：系统默认、WASAPI（仅 Windows）与 ASIO（存在注册驱动时）。
/// 输入：无。
/// 输出/副作用：返回系统列表；只读取宿主能力与注册表。
/// 复杂度：时间 O(音频系统数 + ASIO 驱动数)，空间 O(音频系统数)。
///
/// Module: audio / system output / audio system
/// Purpose: List the audio systems available on this machine: system default, WASAPI (Windows) and
/// ASIO when at least one driver is registered.
/// Inputs: none.
/// Outputs / side effects: the system list; reads host capabilities and the registry only.
/// Complexity: O(audio systems + ASIO drivers) time and O(audio systems) space.
pub fn audio_systems() -> Vec<AudioSystemInfo> {
    // WASAPI / ASIO 只在 Windows 追加，安卓与其它平台只有系统默认宿主。
    // WASAPI / ASIO are appended on Windows only; Android and other platforms keep the default host.
    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    let mut systems = vec![AudioSystemInfo {
        system: AudioSystem::System,
        available: true,
        detail: "使用操作系统默认音频宿主".into(),
    }];
    #[cfg(target_os = "windows")]
    {
        systems.push(AudioSystemInfo {
            system: AudioSystem::Wasapi,
            available: true,
            detail: "Windows 音频会话 API，兼容性最好".into(),
        });
        let drivers = crate::audio::asio::driver_names();
        let available = !drivers.is_empty();
        let detail = if available {
            format!("已检测到 {} 个 ASIO 驱动", drivers.len())
        } else {
            "未检测到已注册的 ASIO 驱动".into()
        };
        systems.push(AudioSystemInfo {
            system: AudioSystem::Asio,
            available,
            detail,
        });
    }
    systems
}

/// 所属模块：audio / 系统输出 / 设备枚举
/// 功能：返回指定音频系统的输出设备名称。
/// 输入：`system` 音频系统。
/// 输出/副作用：设备名称列表；ASIO 使用注册驱动名。
/// 复杂度：时间 O(设备数)，空间 O(设备数)。
///
/// Module: audio / system output / device enumeration
/// Purpose: Return output device names for the given audio system.
/// Inputs: `system` — audio system.
/// Outputs / side effects: device names; ASIO lists registered drivers.
/// Complexity: O(devices) time and space.
pub fn output_devices(system: AudioSystem) -> Result<Vec<String>, String> {
    #[cfg(target_os = "windows")]
    if system == AudioSystem::Asio {
        return Ok(crate::audio::asio::driver_names()
            .into_iter()
            .map(|driver| driver.description)
            .collect());
    }
    if system == AudioSystem::Asio {
        return Err("当前平台不支持 ASIO".into());
    }
    let host = host_for(system)?;
    let devices = host
        .output_devices()
        .map_err(|error| format!("枚举输出设备失败: {error}"))?;
    Ok(devices.map(|device| device.to_string()).collect())
}

/// 所属模块：audio / 系统输出 / 设备枚举
/// 功能：返回指定音频系统的输入设备名称。
/// 输入：`system` 音频系统。
/// 输出/副作用：设备名称列表；ASIO 不提供独立输入设备枚举。
/// 复杂度：时间 O(设备数)，空间 O(设备数)。
///
/// Module: audio / system output / device enumeration
/// Purpose: Return input device names for the given audio system.
/// Inputs: `system` — audio system.
/// Outputs / side effects: device names; ASIO has no separate input-device enumeration.
/// Complexity: O(devices) time and space.
pub fn input_devices(system: AudioSystem) -> Result<Vec<String>, String> {
    if system == AudioSystem::Asio {
        return Ok(Vec::new());
    }
    let host = host_for(system)?;
    let devices = host
        .input_devices()
        .map_err(|error| format!("枚举输入设备失败: {error}"))?;
    Ok(devices.map(|device| device.to_string()).collect())
}

/// 所属模块：audio / 系统输出 / 音频系统
/// 功能：把音频系统映射为 cpal 宿主。
/// 输入：`system` 音频系统。
/// 输出/副作用：返回宿主；不支持时返回中文错误。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / system output / audio system
/// Purpose: Map an audio system to a cpal host.
/// Inputs: `system` — audio system.
/// Outputs / side effects: the host, or a Chinese error when unsupported.
/// Complexity: O(1) time and space.
fn host_for(system: AudioSystem) -> Result<cpal::Host, String> {
    match system {
        AudioSystem::System => Ok(cpal::default_host()),
        AudioSystem::Wasapi => {
            #[cfg(target_os = "windows")]
            {
                cpal::host_from_id(cpal::HostId::Wasapi)
                    .map_err(|error| format!("WASAPI 不可用: {error}"))
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err("WASAPI 仅支持 Windows".into())
            }
        }
        AudioSystem::Asio => Err("ASIO 使用专用后端启动".into()),
    }
}

/// 多个工程窗口与单一输出设备之间的路由。
/// Router between several project windows and the single output device.
///
/// 每个窗口拥有独立 `Engine`；只有“激活”的窗口参与渲染，其余窗口暂停，避免多个工程同时
/// 输出到同一设备，也避免为未激活的工程消耗音频线程时间。
/// Each window owns its own `Engine`; only the activated window is rendered and the others are
/// paused, so two projects never mix on one device and inactive projects cost no audio time.
pub struct AudioRouter {
    /// 已注册引擎（弱引用，窗口关闭后自动失效）。
    /// Registered engines (weak, so closing a window retires its engine automatically).
    engines: Mutex<Vec<Weak<Mutex<Engine>>>>,
    /// 当前参与渲染的引擎。
    /// Engine currently rendered.
    active: Mutex<Option<Weak<Mutex<Engine>>>>,
    /// 输出设备采样率，单位 Hz。
    /// Output device sample rate in Hz.
    sample_rate: Mutex<f64>,
}

impl AudioRouter {
    /// 所属模块：audio / 系统输出 / 路由
    /// 功能：创建空路由，采样率先使用默认值。
    /// 输入：无。
    /// 输出/副作用：返回共享路由；不启动设备。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / system output / router
    /// Purpose: Create an empty router using the default sample rate.
    /// Inputs: none.
    /// Outputs / side effects: shared router; no device is opened.
    /// Complexity: O(1) time and space.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            engines: Mutex::new(Vec::new()),
            active: Mutex::new(None),
            sample_rate: Mutex::new(DEFAULT_SAMPLE_RATE),
        })
    }

    /// 所属模块：audio / 系统输出 / 路由
    /// 功能：注册一个窗口引擎并应用设备采样率；只有在没有存活激活引擎时才接管输出。
    /// 输入：`engine`：窗口新建的引擎。
    /// 输出/副作用：写入注册表；必要时设置激活弱引用；不会打断正在播放的其他工程。
    /// 复杂度：时间 O(已注册引擎数)，空间 O(1)。
    ///
    /// Module: audio / system output / router
    /// Purpose: Register a window engine and apply the device sample rate; take over the output only
    /// when no live active engine exists.
    /// Inputs: `engine`: the engine created for a window.
    /// Outputs / side effects: updates the registry; may set the active weak reference; never
    /// interrupts another playing project.
    /// Complexity: O(registered engines) time and O(1) space.
    pub fn register(&self, engine: &Arc<Mutex<Engine>>) {
        let rate = self.sample_rate();
        if let Ok(mut guard) = engine.lock() {
            guard.set_sample_rate(rate);
        }
        if let Ok(mut engines) = self.engines.lock() {
            engines.retain(|entry| entry.strong_count() > 0);
            engines.push(Arc::downgrade(engine));
        }
        if let Ok(mut active) = self.active.lock() {
            let has_live = active.as_ref().and_then(Weak::upgrade).is_some();
            if !has_live {
                *active = Some(Arc::downgrade(engine));
            }
        }
    }

    /// 所属模块：audio / 系统输出 / 路由
    /// 功能：把某个窗口设为唯一输出目标；暂停其他仍在播放的工程。
    /// 输入：`engine`：请求播放的窗口引擎。
    /// 输出/副作用：更新激活弱引用；以非阻塞方式暂停其他仍在播放的引擎。
    /// 复杂度：时间 O(已注册引擎数)，空间 O(1)。
    ///
    /// Module: audio / system output / router
    /// Purpose: Make one window the only output target and pause other projects that still play.
    /// Inputs: `engine`: the engine of the window that starts playback.
    /// Outputs / side effects: updates the active weak reference; pauses other transports.
    /// Complexity: O(registered engines) time and O(1) space.
    pub fn activate(&self, engine: &Arc<Mutex<Engine>>) {
        let target = Arc::as_ptr(engine);
        if let Ok(mut engines) = self.engines.lock() {
            engines.retain(|entry| entry.strong_count() > 0);
            if !engines.iter().any(|entry| {
                entry
                    .upgrade()
                    .is_some_and(|candidate| Arc::as_ptr(&candidate) == target)
            }) {
                engines.push(Arc::downgrade(engine));
            }
            for entry in engines.iter() {
                let Some(candidate) = entry.upgrade() else {
                    continue;
                };
                if Arc::as_ptr(&candidate) == target {
                    continue;
                }
                // 只做非阻塞暂停：其他窗口的后台插件同步可能长时间持有引擎锁，UI 不能等待。
                // Non-blocking pause only: a background plugin sync in another window may hold its
                // engine lock for seconds and the UI must not wait.
                if let Ok(mut guard) = candidate.try_lock() {
                    if guard.is_playing() {
                        guard.pause();
                    }
                };
            }
        }
        if let Ok(mut active) = self.active.lock() {
            *active = Some(Arc::downgrade(engine));
        }
    }

    /// 所属模块：audio / 系统输出 / 路由
    /// 功能：取消某个引擎的输出目标（仅当它正是当前激活引擎）。
    /// 输入：`engine`：准备关闭的窗口引擎。
    /// 输出/副作用：清空激活弱引用；不停止音频流。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / system output / router
    /// Purpose: Drop one engine as the output target when it is the active one.
    /// Inputs: `engine`: the engine of the window being closed.
    /// Outputs / side effects: clears the active weak reference; the audio stream keeps running.
    /// Complexity: O(1) time and space.
    pub fn deactivate(&self, engine: &Arc<Mutex<Engine>>) {
        let target = Arc::as_ptr(engine);
        if let Ok(mut active) = self.active.lock() {
            let is_target = active
                .as_ref()
                .and_then(Weak::upgrade)
                .is_some_and(|candidate| Arc::as_ptr(&candidate) == target);
            if is_target {
                *active = None;
            }
        }
    }

    /// 所属模块：audio / 系统输出 / 路由
    /// 功能：把某个引擎从注册表中移除（窗口关闭、引擎保留不析构时使用）。
    /// 输入：`engine`：要移除的窗口引擎。
    /// 输出/副作用：删除对应的弱引用，并清空激活槽位；不停止音频流、不析构引擎。
    /// 复杂度：时间 O(已注册引擎数)，空间 O(1)。
    ///
    /// Module: audio / system output / router
    /// Purpose: Remove one engine from the registry (used when a window closes while its engine is
    /// intentionally retained).
    /// Inputs: `engine`: the engine to remove.
    /// Outputs / side effects: drops the matching weak entry and clears the active slot; the audio
    /// stream keeps running and the engine is not destroyed.
    /// Complexity: O(registered engines) time and O(1) space.
    pub fn unregister(&self, engine: &Arc<Mutex<Engine>>) {
        let target = Arc::as_ptr(engine);
        if let Ok(mut engines) = self.engines.lock() {
            engines.retain(|entry| {
                entry
                    .upgrade()
                    .is_some_and(|candidate| Arc::as_ptr(&candidate) != target)
            });
        }
        self.deactivate(engine);
    }

    /// 所属模块：audio / 系统输出 / 路由
    /// 功能：读取激活引擎槽位；音频回调只做一次非阻塞尝试。
    /// 输入：`self`。
    /// 输出/副作用：外层 `None` 表示路由锁忙（沿用上一块）；内层 `None` 表示没有激活引擎
    /// （必须释放缓存的旧引擎，避免窗口关闭后仍然占用插件实例）。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / system output / router
    /// Purpose: Read the active engine slot with a single non-blocking attempt for the callback.
    /// Inputs: `self`.
    /// Outputs / side effects: outer `None` means the router lock was busy (keep the previous
    /// block); inner `None` means no active engine (the cached engine must be released so a closed
    /// window stops pinning its plugin instances).
    /// Complexity: O(1) time and space.
    pub fn engine_slot(&self) -> Option<Option<Arc<Mutex<Engine>>>> {
        let active = self.active.try_lock().ok()?;
        Some(active.as_ref().and_then(Weak::upgrade))
    }

    /// 所属模块：audio / 系统输出 / 路由
    /// 功能：返回输出设备采样率。
    /// 输入：`self`。
    /// 输出/副作用：采样率（Hz）；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / system output / router
    /// Purpose: Return the output device sample rate.
    /// Inputs: `self`.
    /// Outputs / side effects: sample rate in Hz; no state change.
    /// Complexity: O(1) time and space.
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
            .lock()
            .map(|rate| *rate)
            .unwrap_or(DEFAULT_SAMPLE_RATE)
    }

    /// 所属模块：audio / 系统输出 / 路由
    /// 功能：记录设备采样率并同步到所有已注册引擎。
    /// 输入：`sample_rate`：设备采样率（Hz）。
    /// 输出/副作用：更新采样率；可能设置各引擎采样率。
    /// 复杂度：时间 O(已注册引擎数)，空间 O(1)。
    ///
    /// Module: audio / system output / router
    /// Purpose: Store the device rate and forward it to every registered engine.
    /// Inputs: `sample_rate`: device rate in Hz.
    /// Outputs / side effects: updates the rate; may set each engine's rate.
    /// Complexity: O(registered engines) time and O(1) space.
    pub fn set_sample_rate(&self, sample_rate: f64) {
        if let Ok(mut rate) = self.sample_rate.lock() {
            *rate = sample_rate;
        }
        if let Ok(mut engines) = self.engines.lock() {
            engines.retain(|entry| entry.strong_count() > 0);
            for entry in engines.iter() {
                let Some(engine) = entry.upgrade() else {
                    continue;
                };
                if let Ok(mut guard) = engine.try_lock() {
                    guard.set_sample_rate(sample_rate);
                };
            }
        }
    }
}

/// 所属模块：audio / 系统输出 / 启动
/// 功能：按默认选项启动输出流（系统默认宿主、设备默认采样率、512 帧缓冲）。
/// 输入：`router` 多窗口音频路由。
/// 输出/副作用：打开音频设备并启动回调；失败时返回中文原因。
/// 复杂度：时间 O(设备初始化)，空间 O(缓冲帧数)。
///
/// Module: audio / system output / start
/// Purpose: Start the output stream with default options (default host, device rate, 512 frames).
/// Inputs: `router` — multi-window audio routing.
/// Outputs / side effects: opens the device and starts callbacks; returns a Chinese error on failure.
/// Complexity: O(device setup) time and O(buffer frames) space.
pub fn start_output(router: Arc<AudioRouter>) -> Result<AudioOutput, String> {
    start_output_with(router, &AudioOptions::default())
}

/// 所属模块：audio / 系统输出 / 启动
/// 功能：按音频系统、输出设备、采样率与缓冲启动输出流。
/// 输入：
/// - `router`：多窗口音频路由。
/// - `options`：音频系统、输出设备、采样率与缓冲帧数。
/// 输出/副作用：打开设备并启动回调；ASIO 走专用后端，其余走 cpal。
/// 复杂度：时间 O(设备初始化)，空间 O(缓冲帧数)。
///
/// Module: audio / system output / start
/// Purpose: Start the output stream for a given audio system, device, sample rate and buffer.
/// Inputs:
/// - `router`: multi-window audio routing.
/// - `options`: audio system, output device, sample rate and buffer frames.
/// Outputs / side effects: opens the device and starts callbacks; ASIO uses its own backend.
/// Complexity: O(device setup) time and O(buffer frames) space.
pub fn start_output_with(
    router: Arc<AudioRouter>,
    options: &AudioOptions,
) -> Result<AudioOutput, String> {
    match options.system {
        AudioSystem::Asio => start_asio(router, options),
        other => start_cpal(router, options, other),
    }
}

/// 所属模块：audio / 系统输出 / 启动
/// 功能：启动 ASIO 输出；非 Windows 平台直接返回不支持。
/// 输入：
/// - `router`：多窗口音频路由。
/// - `options`：包含驱动名、采样率与缓冲帧数。
/// 输出/副作用：加载驱动并启动回调；失败时返回中文原因。
/// 复杂度：时间 O(驱动初始化)，空间 O(缓冲帧数)。
///
/// Module: audio / system output / start
/// Purpose: Start the ASIO output; non-Windows platforms report unsupported.
/// Inputs:
/// - `router`: multi-window audio routing.
/// - `options`: driver name, sample rate and buffer frames.
/// Outputs / side effects: loads the driver and starts callbacks; returns a Chinese error on failure.
/// Complexity: O(driver setup) time and O(buffer frames) space.
fn start_asio(router: Arc<AudioRouter>, options: &AudioOptions) -> Result<AudioOutput, String> {
    #[cfg(target_os = "windows")]
    {
        let driver = crate::audio::asio::start(
            router,
            options.output_device.as_deref().unwrap_or(""),
            options.sample_rate,
            options.buffer_frames,
        )?;
        let description = driver.description.clone();
        Ok(AudioOutput {
            _stream: AudioStream::Asio(driver),
            description,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (router, options);
        Err("当前平台不支持 ASIO".into())
    }
}

/// 所属模块：audio / 系统输出 / 启动
/// 功能：用 cpal 启动输出流：选择设备、采样率与缓冲，并在驱动拒绝固定缓冲时回退默认值。
/// 输入：
/// - `router`：多窗口音频路由。
/// - `options`：输出设备、采样率与缓冲帧数。
/// - `system`：cpal 宿主（系统默认或 WASAPI）。
/// 输出/副作用：打开设备并启动回调；采样率写回路由。
/// 复杂度：时间 O(设备枚举 + 流创建)，空间 O(缓冲帧数)。
///
/// Module: audio / system output / start
/// Purpose: Start a cpal stream: pick the device, rate and buffer, falling back to the default
/// buffer when a driver rejects a fixed size.
/// Inputs:
/// - `router`: multi-window audio routing.
/// - `options`: output device, sample rate and buffer frames.
/// - `system`: cpal host (system default or WASAPI).
/// Outputs / side effects: opens the device and starts callbacks; the rate is stored on the router.
/// Complexity: O(device enumeration + stream creation) time and O(buffer frames) space.
fn start_cpal(
    router: Arc<AudioRouter>,
    options: &AudioOptions,
    system: AudioSystem,
) -> Result<AudioOutput, String> {
    let host = host_for(system)?;
    let device = match options.output_device.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => {
            let key = name.to_lowercase();
            host.output_devices()
                .map_err(|error| format!("枚举输出设备失败: {error}"))?
                .find(|device| device.to_string().to_lowercase() == key)
                .ok_or_else(|| format!("未找到输出设备「{name}」"))?
        }
        _ => host
            .default_output_device()
            .ok_or_else(|| "没有可用的音频输出设备".to_string())?,
    };
    let mut supported = device
        .default_output_config()
        .map_err(|e| format!("读取默认输出格式失败: {e}"))?;

    // 优先使用请求的采样率；未指定时沿用 48 kHz 的 DAW 常用值。
    // Prefer the requested rate; otherwise keep the DAW-friendly 48 kHz default.
    let requested_rate = options.sample_rate.unwrap_or(DEFAULT_SAMPLE_RATE as u32);
    // 环绕声输出优先匹配设置中的声道数（2 / 6 / 8）；设备不支持时退回任意 >= 2 声道的配置。
    // Surround output prefers the requested channel count (2 / 6 / 8); devices without it fall back
    // to any configuration with at least two channels.
    let requested_channels = options.output_channels.filter(|channels| *channels >= 2);
    if let Ok(ranges) = device.supported_output_configs() {
        let mut fallback: Option<cpal::SupportedStreamConfigRange> = None;
        let mut matched = false;
        for range in ranges {
            if range.sample_format() != cpal::SampleFormat::F32
                || range.min_sample_rate() > requested_rate
                || range.max_sample_rate() < requested_rate
                || range.channels() < 2
            {
                continue;
            }
            if requested_channels.is_none_or(|channels| range.channels() == channels) {
                supported = range.with_sample_rate(requested_rate);
                matched = true;
                break;
            }
            if fallback.is_none() {
                fallback = Some(range);
            }
        }
        if !matched {
            if let Some(range) = fallback {
                supported = range.with_sample_rate(requested_rate);
            }
        }
    }

    let sample_format = supported.sample_format();
    let mut config: cpal::StreamConfig = supported.config();
    let device_name = device.to_string();

    // 采样率属于设备而非某个窗口：写入路由后由所有已注册引擎共享。
    // The sample rate belongs to the device, not to one window: store it on the router so every
    // registered engine follows it.
    router.set_sample_rate(config.sample_rate as f64);

    let channels = config.channels as usize;
    let sample_rate = config.sample_rate;
    config.buffer_size =
        cpal::BufferSize::Fixed(options.buffer_frames.unwrap_or(DEFAULT_BUFFER_FRAMES));

    let stream = match sample_format {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config, router.clone(), channels),
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, router.clone(), channels),
        cpal::SampleFormat::U16 => build::<u16>(&device, &config, router.clone(), channels),
        other => Err(format!("不支持的采样格式: {other:?}")),
    }
    .or_else(|_| {
        // Some drivers reject a fixed buffer size; fall back to the default.
        config.buffer_size = cpal::BufferSize::Default;
        match sample_format {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, router.clone(), channels),
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, router.clone(), channels),
            cpal::SampleFormat::U16 => build::<u16>(&device, &config, router.clone(), channels),
            other => Err(format!("不支持的采样格式: {other:?}")),
        }
    })?;

    stream.play().map_err(|e| format!("启动音频流失败: {e}"))?;

    Ok(AudioOutput {
        _stream: AudioStream::Cpal(stream),
        description: format!("{device_name} · {sample_rate} Hz · {channels} ch"),
    })
}

/// 在音频回调中短暂等待引擎锁：后台同步线程只会短时持锁，稍等即可拿到，
/// 避免把一次短任务变成可听见的掉音。
/// Briefly wait for the engine lock in the audio callback: the background sync
/// thread only holds it briefly, so a short spin avoids an audible dropout.
///
/// 所属模块：audio / 系统输出 / 实时锁
/// 功能：在 2 ms 内自旋等待引擎锁，超时返回 `None`。
/// 输入：`engine` 窗口引擎。
/// 输出/副作用：返回锁守卫或 `None`；不修改引擎。
/// 复杂度：时间 O(1)（最多 2 ms），空间 O(1)。
///
/// Module: audio / system output / realtime lock
/// Purpose: Spin for the engine lock for up to 2 ms and return `None` on timeout.
/// Inputs: `engine` — window engine.
/// Outputs / side effects: the guard or `None`; the engine is untouched.
/// Complexity: O(1) time (at most 2 ms) and O(1) space.
pub(super) fn lock_engine(engine: &Mutex<Engine>) -> Option<std::sync::MutexGuard<'_, Engine>> {
    let deadline = Instant::now() + Duration::from_millis(2);
    loop {
        match engine.try_lock() {
            Ok(guard) => return Some(guard),
            Err(std::sync::TryLockError::Poisoned(poison)) => return Some(poison.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => {
                if Instant::now() >= deadline {
                    return None;
                }
                std::thread::yield_now();
            }
        }
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    router: Arc<AudioRouter>,
    channels: usize,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    // 每个设备声道一个缓冲：工程布局与设备声道数不同时由引擎完成下混或补零。
    // One buffer per device channel: the engine downmixes or pads when the project layout differs.
    let mut buffers: Vec<Vec<f32>> = (0..channels.max(1)).map(|_| Vec::new()).collect();
    let mut last: Vec<f32> = vec![0.0; channels.max(1)];
    // 回调缓存当前激活引擎，路由锁忙时继续使用上一次的引擎，避免实时线程等待。
    // Cache the active engine: when the router lock is busy, keep rendering the previous engine
    // instead of waiting on a lock inside the real-time callback.
    let mut active: Option<Arc<Mutex<Engine>>> = None;
    // Fade across a missed callback instead of cutting to silence, which is
    // heard as a click/crackle ("破音") when another thread briefly holds the
    // engine lock. `lock_engine` first spins for a couple of milliseconds so a
    // short background sync never turns into an audible dropout.
    let mut recovering = false;
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _info: &cpal::OutputCallbackInfo| {
                let frames = data.len() / channels.max(1);
                if frames == 0 {
                    return;
                }
                for buffer in buffers.iter_mut() {
                    if buffer.len() != frames {
                        buffer.resize(frames, 0.0);
                    }
                }
                // 外层 None 表示路由锁忙：沿用上一块缓存的引擎；内层 None 表示没有激活工程，
                // 释放缓存，避免已关闭窗口的引擎与插件实例被回调长期占用。
                // Outer None means the router lock was busy: keep the cached engine. Inner None
                // means no active project: release the cache so closed windows do not pin engines.
                if let Some(slot) = router.engine_slot() {
                    active = slot;
                }
                let mut ok = false;
                if let Some(engine) = &active {
                    if let Some(mut engine) = lock_engine(engine) {
                        engine.process_output(&mut buffers);
                        ok = true;
                    }
                }
                let fade = (frames / 8).max(1) as f32;
                if ok {
                    if recovering {
                        // Fade back in after a missed block.
                        for frame in 0..frames {
                            let gain = (frame as f32 / fade).min(1.0);
                            for buffer in buffers.iter_mut() {
                                buffer[frame] *= gain;
                            }
                        }
                        recovering = false;
                    }
                    for (channel, buffer) in buffers.iter().enumerate() {
                        last[channel] = buffer[frames.saturating_sub(1)];
                    }
                } else {
                    // Fall back to a short fade-out from the last sample of every channel.
                    // 取走 `last` 避免在回调中克隆分配；淡出后重新填充为零。
                    // Move `last` out to avoid a clone allocation in the callback; refill with zeros.
                    let start = std::mem::take(&mut last);
                    for frame in 0..frames {
                        let gain = (1.0 - frame as f32 / fade).max(0.0);
                        for (channel, buffer) in buffers.iter_mut().enumerate() {
                            buffer[frame] = start.get(channel).copied().unwrap_or(0.0) * gain;
                        }
                    }
                    last = start;
                    for sample in &mut last {
                        *sample = 0.0;
                    }
                    recovering = true;
                }
                if channels == 1 {
                    for frame in 0..frames {
                        data[frame] = T::from_sample(buffers[0][frame]);
                    }
                } else {
                    for frame in 0..frames {
                        let base = frame * channels;
                        for channel in 0..channels {
                            data[base + channel] = T::from_sample(buffers[channel][frame]);
                        }
                    }
                }
            },
            |err| eprintln!("音频输出错误: {err}"),
            None,
        )
        .map_err(|e| format!("创建音频流失败: {e}"))
}
