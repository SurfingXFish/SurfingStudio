//! 所属模块：audio / ASIO 输出
//! 文件用途：
//! - 通过 COM 直接加载本机注册的 ASIO 驱动，枚举驱动并创建低延迟输出流。
//! - 把 ASIO 双缓冲回调接入 `AudioRouter`，并把立体声结果写入驱动声明的输出缓冲。
//!
//! 函数清单：
//! - `driver_names` / `driver_available`：枚举注册表中的 ASIO 驱动。
//! - `start`：创建 COM 实例、协商采样率与缓冲、创建缓冲并启动回调。
//! - `AsioStream::drop`：停止驱动、释放缓冲与 COM 接口。
//! - `buffer_switch` / `render_block` / `write_channel` / `write_aligned_int32`：实时回调与采样格式转换。
//! - `parse_clsid` / `clsid_string` / `registry_string`：注册表与 GUID 解析。
//!
//! Module: audio / ASIO output
//! Purpose:
//! - Load locally registered ASIO drivers through COM and enumerate them.
//! - Bridge the ASIO double-buffer callback into `AudioRouter` and write the stereo result into
//!   the output buffers declared by the driver.
//!
//! Functions:
//! - `driver_names` / `driver_available`: enumerate ASIO drivers from the registry.
//! - `start`: create the COM instance, negotiate sample rate/buffer, create buffers and start.
//! - `AsioStream::drop`: stop the driver, dispose buffers and release the COM interface.
//! - `buffer_switch` / `render_block` / `write_channel` / `write_aligned_int32`: realtime callback and sample conversion.
//! - `parse_clsid` / `registry_string`: registry and GUID parsing.

use crate::audio::backend::{lock_engine, AudioRouter};
use crate::audio::engine::Engine;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};
use winapi::ctypes::c_void;
use winapi::shared::guiddef::GUID;
use winapi::shared::minwindef::{DWORD, HKEY};
use winapi::um::combaseapi::{CoCreateInstance, CoInitializeEx};
use winapi::um::winnt::{KEY_READ, REG_SZ};
use winapi::um::winreg::{
    RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY_LOCAL_MACHINE,
};

/// ASIO 成功返回码。
/// ASIO success code.
const ASE_OK: i32 = 0;
/// ASIO 注册表根路径。
/// Registry root of ASIO drivers.
const ASIO_REGISTRY_PATH: &str = "SOFTWARE\\ASIO";
/// 缓冲区大小请求的上下限（采样帧）。
/// Lower/upper bound of the requested buffer size in frames.
const BUFFER_MIN_FRAMES: i32 = 32;
/// 缓冲区大小请求的上限（采样帧）。
/// Upper bound of the requested buffer size in frames.
const BUFFER_MAX_FRAMES: i32 = 4096;
/// 采样格式：16 位小端整数。
/// Sample type: 16-bit little-endian integer.
const ASIO_ST_INT16_LSB: i32 = 16;
/// 采样格式：24 位小端整数（3 字节打包）。
/// Sample type: 24-bit little-endian integer (packed into 3 bytes).
const ASIO_ST_INT24_LSB: i32 = 17;
/// 采样格式：32 位小端整数（完整 32 位）。
/// Sample type: 32-bit little-endian integer (full 32-bit range).
const ASIO_ST_INT32_LSB: i32 = 18;
/// 采样格式：32 位小端浮点。
/// Sample type: 32-bit little-endian float.
const ASIO_ST_FLOAT32_LSB: i32 = 19;
/// 采样格式：64 位小端浮点。
/// Sample type: 64-bit little-endian float.
const ASIO_ST_FLOAT64_LSB: i32 = 20;
/// 采样格式：32 位容器内左对齐的 16 位整数。
/// Sample type: 16-bit integer left-aligned inside a 32-bit container.
const ASIO_ST_INT32_LSB16: i32 = 24;
/// 采样格式：32 位容器内左对齐的 18 位整数。
/// Sample type: 18-bit integer left-aligned inside a 32-bit container.
const ASIO_ST_INT32_LSB18: i32 = 25;
/// 采样格式：32 位容器内左对齐的 20 位整数。
/// Sample type: 20-bit integer left-aligned inside a 32-bit container.
const ASIO_ST_INT32_LSB20: i32 = 26;
/// 采样格式：32 位容器内左对齐的 24 位整数。
/// Sample type: 24-bit integer left-aligned inside a 32-bit container.
const ASIO_ST_INT32_LSB24: i32 = 27;
/// 可接受的最低设备采样率；低于该值视为驱动未初始化或返回占位值。
/// Lowest plausible device sample rate; lower values mean an uninitialized or placeholder driver.
const MIN_REASONABLE_RATE: f64 = 32_000.0;

/// 当前实时回调使用的上下文；同一进程只允许一个 ASIO 输出。
/// Context used by the realtime callback; only one ASIO output exists per process.
static ASIO_CONTEXT: AtomicPtr<AsioCallbackContext> = AtomicPtr::new(std::ptr::null_mut());

/// ASIO 驱动信息。
/// ASIO driver descriptor.
#[derive(Clone, Debug)]
pub struct AsioDriverInfo {
    /// 注册表键名，用于 `init` 与 `start` 匹配。
    /// Registry key name, used to match the driver when starting.
    pub name: String,
    /// 驱动描述（缺失时等同注册表键名）。
    /// Driver description (falls back to the registry key name).
    pub description: String,
}

/// ASIO 输出流句柄：持有驱动接口与实时上下文，析构时停止并释放。
/// ASIO output handle: owns the driver interface and realtime context; dropping it stops and releases.
pub struct AsioStream {
    /// COM 接口指针；为空表示创建失败后的占位。
    /// COM interface pointer; null for a placeholder after a failed start.
    driver: *mut IAsio,
    /// 实时回调上下文，生命周期与输出流一致。
    /// Realtime callback context; lives as long as the output stream.
    context: *mut AsioCallbackContext,
    /// 驱动说明，供状态栏显示。
    /// Driver description shown in the status bar.
    pub description: String,
}

// 安全性：ASIO 回调只通过 `ASIO_CONTEXT` 原子指针访问上下文，接口方法只在创建与析构的
// UI 线程调用；`stop()` 返回后驱动保证不再回调，因此跨线程移动句柄是安全的。
// Safety: the ASIO callback only reaches the context through the `ASIO_CONTEXT` atomic pointer and
// interface methods are called on the UI thread during creation/destruction. `stop()` guarantees no
// further callbacks, so moving the handle across threads is safe.
unsafe impl Send for AsioStream {}
unsafe impl Sync for AsioStream {}

impl Drop for AsioStream {
    /// 所属模块：audio / ASIO 输出
    /// 功能：停止驱动、清空回调上下文、释放缓冲与 COM 接口。
    /// 输入：`self`。
    /// 输出/副作用：驱动停止回调；释放 COM 引用与上下文内存。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / ASIO output
    /// Purpose: Stop the driver, clear the callback context and release buffers/COM.
    /// Inputs: `self`.
    /// Outputs / side effects: the driver stops calling back; COM references and context memory are freed.
    /// Complexity: O(1) time and space.
    fn drop(&mut self) {
        unsafe {
            if !self.driver.is_null() {
                let table = (*self.driver).table;
                ((*table).stop)(self.driver as *mut c_void);
                // 驱动停止后才允许释放上下文：ASIOStop 返回时保证回调已全部结束。
                // Free the context only after stopping: ASIOStop returns once all callbacks finished.
                ASIO_CONTEXT.store(std::ptr::null_mut(), Ordering::Release);
                ((*table).dispose_buffers)(self.driver as *mut c_void);
                ((*table).release)(self.driver as *mut c_void);
                self.driver = std::ptr::null_mut();
            } else {
                ASIO_CONTEXT.store(std::ptr::null_mut(), Ordering::Release);
            }
            if !self.context.is_null() {
                drop(Box::from_raw(self.context));
                self.context = std::ptr::null_mut();
            }
        }
    }
}

/// 所属模块：audio / ASIO 输出 / 采样格式
/// 功能：判断驱动声明的采样类型是否可由宿主写入。
/// 输入：`sample_type`：ASIO 采样类型。
/// 输出/副作用：布尔值；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / ASIO output / sample format
/// Purpose: Whether the driver's sample type can be written by the host.
/// Inputs: `sample_type` — ASIO sample type.
/// Outputs / side effects: boolean; no state change.
/// Complexity: O(1) time and space.
fn is_supported_sample_type(sample_type: i32) -> bool {
    matches!(
        sample_type,
        ASIO_ST_INT16_LSB
            | ASIO_ST_INT24_LSB
            | ASIO_ST_INT32_LSB
            | ASIO_ST_FLOAT32_LSB
            | ASIO_ST_FLOAT64_LSB
            | ASIO_ST_INT32_LSB16
            | ASIO_ST_INT32_LSB18
            | ASIO_ST_INT32_LSB20
            | ASIO_ST_INT32_LSB24
    )
}

/// 实时回调上下文。
/// Realtime callback context.
struct AsioCallbackContext {
    router: Arc<AudioRouter>,
    /// 当前激活引擎缓存：路由锁忙时沿用上一块。
    /// Cached active engine; reused when the router lock is busy.
    active: Option<Arc<Mutex<Engine>>>,
    left: Vec<f32>,
    right: Vec<f32>,
    /// 输出通道的双缓冲地址（最多两个通道）。
    /// Double-buffer addresses of the output channels (at most two).
    output_buffers: Vec<[*mut c_void; 2]>,
    /// 每个输出通道的 ASIO 采样格式。
    /// ASIO sample type of every output channel.
    output_types: Vec<i32>,
    frames: usize,
    last_l: f32,
    last_r: f32,
    recovering: bool,
    /// 交给驱动的回调表：必须与上下文同生命周期。
    /// 部分驱动会保存该指针并在 `start()` 返回后继续调用，放在栈上会导致野指针。
    /// Callback table handed to the driver; it must live as long as the stream. Some drivers keep
    /// the pointer and call it after `start()` returns, so a stack-local table would dangle.
    callbacks: AsioCallbacks,
}

/// ASIO COM 接口（IASIO）。字段顺序必须与 SDK 的 `iasiodrv.h` 完全一致。
/// ASIO COM interface (IASIO). The field order must match the SDK's `iasiodrv.h`.
#[repr(C)]
struct IAsio {
    table: *const IAsioVtbl,
}

#[repr(C)]
struct IAsioVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    init: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32,
    get_driver_name: unsafe extern "system" fn(*mut c_void, *mut i8),
    get_driver_version: unsafe extern "system" fn(*mut c_void) -> i32,
    get_error_message: unsafe extern "system" fn(*mut c_void, *mut i8),
    start: unsafe extern "system" fn(*mut c_void) -> i32,
    stop: unsafe extern "system" fn(*mut c_void) -> i32,
    get_channels: unsafe extern "system" fn(*mut c_void, *mut i32, *mut i32) -> i32,
    get_latencies: unsafe extern "system" fn(*mut c_void, *mut i32, *mut i32) -> i32,
    get_buffer_size:
        unsafe extern "system" fn(*mut c_void, *mut i32, *mut i32, *mut i32, *mut i32) -> i32,
    can_sample_rate: unsafe extern "system" fn(*mut c_void, f64) -> i32,
    get_sample_rate: unsafe extern "system" fn(*mut c_void, *mut f64) -> i32,
    set_sample_rate: unsafe extern "system" fn(*mut c_void, f64) -> i32,
    get_clock_sources:
        unsafe extern "system" fn(*mut c_void, *mut AsioClockSource, *mut i32) -> i32,
    set_clock_source: unsafe extern "system" fn(*mut c_void, i32) -> i32,
    get_sample_position: unsafe extern "system" fn(*mut c_void, *mut i64, *mut i64) -> i32,
    get_channel_info: unsafe extern "system" fn(*mut c_void, *mut AsioChannelInfo) -> i32,
    create_buffers: unsafe extern "system" fn(
        *mut c_void,
        *mut AsioBufferInfo,
        i32,
        i32,
        *mut AsioCallbacks,
    ) -> i32,
    dispose_buffers: unsafe extern "system" fn(*mut c_void) -> i32,
    control_panel: unsafe extern "system" fn(*mut c_void) -> i32,
    future: unsafe extern "system" fn(*mut c_void, i32, *mut c_void) -> i32,
    output_ready: unsafe extern "system" fn(*mut c_void) -> i32,
}

#[repr(C)]
struct AsioBufferInfo {
    is_input: i32,
    channel_num: i32,
    buffers: [*mut c_void; 2],
}

#[repr(C)]
struct AsioChannelInfo {
    channel: i32,
    is_input: i32,
    is_active: i32,
    channel_group: i32,
    sample_type: i32,
    name: [i8; 32],
}

#[repr(C)]
struct AsioClockSource {
    index: i32,
    associated_channel: i32,
    associated_group: i32,
    is_current_source: i32,
    name: [i8; 32],
}

#[repr(C)]
struct AsioCallbacks {
    buffer_switch: Option<unsafe extern "system" fn(i32, i32)>,
    sample_rate_did_change: Option<unsafe extern "system" fn(f64)>,
    asio_message: Option<unsafe extern "system" fn(i32, i32, *mut c_void, *mut f64) -> i32>,
    buffer_switch_time_info:
        Option<unsafe extern "system" fn(*mut c_void, i32, i32) -> *mut c_void>,
}

/// 所属模块：audio / ASIO 输出
/// 功能：返回本机注册的 ASIO 驱动列表；未注册任何驱动时返回空列表。
/// 输入：无。
/// 输出/副作用：只读注册表；不加载驱动。
/// 复杂度：时间 O(驱动数 × 键名长度)，空间 O(驱动数)。
///
/// Module: audio / ASIO output
/// Purpose: Return the locally registered ASIO drivers; empty when none is registered.
/// Inputs: none.
/// Outputs / side effects: reads the registry only; drivers are not loaded.
/// Complexity: O(driver count × key length) time and O(driver count) space.
pub fn driver_names() -> Vec<AsioDriverInfo> {
    enumerate_drivers()
        .into_iter()
        .map(|(name, _, description)| AsioDriverInfo { name, description })
        .collect()
}

/// 所属模块：audio / ASIO 输出
/// 功能：返回是否存在可用的 ASIO 驱动。
/// 输入：无。
/// 输出/副作用：布尔值；只读注册表。
/// 复杂度：时间 O(驱动数)，空间 O(1)。
///
/// Module: audio / ASIO output
/// Purpose: Whether at least one ASIO driver is registered.
/// Inputs: none.
/// Outputs / side effects: boolean; reads the registry only.
/// Complexity: O(driver count) time and O(1) space.
pub fn driver_available() -> bool {
    !enumerate_drivers().is_empty()
}

/// 所属模块：audio / ASIO 输出
/// 功能：按驱动名创建并启动 ASIO 输出流。
/// 输入：
/// - `router`：多窗口音频路由。
/// - `driver_name`：注册表键名或描述；空字符串表示第一个驱动。
/// - `sample_rate`：期望采样率；`None` 使用驱动当前采样率。
/// - `buffer_frames`：期望缓冲帧数；`None` 使用驱动首选值。
/// 输出/副作用：加载驱动 DLL、创建缓冲并开始回调；失败时释放全部资源并返回中文原因。
/// 复杂度：时间 O(驱动初始化 + 缓冲创建)，空间 O(缓冲帧数)。
///
/// Module: audio / ASIO output
/// Purpose: Create and start an ASIO output stream for the given driver.
/// Inputs:
/// - `router`: multi-window audio routing.
/// - `driver_name`: registry key or description; an empty string means the first driver.
/// - `sample_rate`: requested rate; `None` keeps the driver rate.
/// - `buffer_frames`: requested buffer size; `None` uses the driver preference.
/// Outputs / side effects: loads the driver DLL, creates buffers and starts callbacks; on failure
/// every resource is released and a Chinese reason is returned.
/// Complexity: O(driver setup + buffer creation) time and O(buffer frames) space.
pub fn start(
    router: Arc<AudioRouter>,
    driver_name: &str,
    sample_rate: Option<u32>,
    buffer_frames: Option<u32>,
) -> Result<AsioStream, String> {
    let drivers = enumerate_drivers();
    if drivers.is_empty() {
        return Err("未检测到 ASIO 驱动".into());
    }
    let key = driver_name.trim().to_lowercase();
    let selected = drivers
        .iter()
        .find(|(name, _, description)| {
            key.is_empty()
                || name.to_lowercase() == key
                || description.to_lowercase() == key
                || name.to_lowercase().contains(&key)
        })
        .ok_or_else(|| format!("未找到 ASIO 驱动「{driver_name}」"))?;
    let clsid = selected.1;
    let display_name = if selected.2.is_empty() {
        selected.0.clone()
    } else {
        selected.2.clone()
    };

    unsafe {
        // STA 初始化失败通常表示线程已初始化过其他模式，此时仍可创建驱动实例。
        // A failed STA init usually means the thread already uses another mode; loading still works.
        // COINIT_APARTMENTTHREADED = 0x2；CLSCTX_INPROC_SERVER = 0x1。
        // COINIT_APARTMENTTHREADED = 0x2; CLSCTX_INPROC_SERVER = 0x1.
        const COINIT_APARTMENTTHREADED: DWORD = 0x2;
        const CLSCTX_INPROC_SERVER: DWORD = 0x1;
        CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);

        let mut driver: *mut IAsio = std::ptr::null_mut();
        let hr = CoCreateInstance(
            &clsid,
            std::ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &clsid,
            &mut driver as *mut *mut IAsio as *mut *mut c_void,
        );
        if hr < 0 || driver.is_null() {
            return Err(format!("创建 ASIO 驱动实例失败（HRESULT 0x{hr:08X}）"));
        }
        let table = (*driver).table;
        if ((*table).init)(driver as *mut c_void, std::ptr::null_mut()) == 0 {
            ((*table).release)(driver as *mut c_void);
            return Err(format!("ASIO 驱动「{display_name}」初始化失败"));
        }

        // 从这里开始任何失败都必须释放 COM 接口。
        // Any failure below must release the COM interface.
        let mut output_channels = 0_i32;
        let mut input_channels = 0_i32;
        if ((*table).get_channels)(
            driver as *mut c_void,
            &mut input_channels,
            &mut output_channels,
        ) != ASE_OK
            || output_channels <= 0
        {
            ((*table).release)(driver as *mut c_void);
            return Err(format!("ASIO 驱动「{display_name}」没有可用输出通道"));
        }

        let mut current_rate = 0.0_f64;
        let _ = ((*table).get_sample_rate)(driver as *mut c_void, &mut current_rate);
        let requested_rate = sample_rate
            .map(|rate| rate as f64)
            .filter(|rate| *rate >= 8_000.0)
            .unwrap_or(current_rate);
        // 驱动可以拒绝采样率（例如 ASIO4ALL 在未打开控制面板前只接受当前设备速率）：
        // 此时沿用驱动当前采样率，而不是让整个输出不可用。
        // The driver may reject a rate (ASIO4ALL only accepts the device rate until its panel is
        // opened); keep the current rate instead of making the whole output unusable.
        if requested_rate >= 8_000.0
            && ((*table).can_sample_rate)(driver as *mut c_void, requested_rate) != 0
            && ((*table).set_sample_rate)(driver as *mut c_void, requested_rate) == ASE_OK
        {
            current_rate = requested_rate;
        }
        if current_rate < MIN_REASONABLE_RATE {
            // 驱动未报告有效采样率时按 48 kHz 处理，并尽力让驱动切到该速率。
            // Treat an invalid reported rate as 48 kHz and best-effort switch the driver to it.
            current_rate = 48_000.0;
            let _ = ((*table).set_sample_rate)(driver as *mut c_void, current_rate);
        }

        let mut min_size = 0_i32;
        let mut max_size = 0_i32;
        let mut preferred_size = 0_i32;
        let mut granularity = 0_i32;
        if ((*table).get_buffer_size)(
            driver as *mut c_void,
            &mut min_size,
            &mut max_size,
            &mut preferred_size,
            &mut granularity,
        ) != ASE_OK
        {
            ((*table).release)(driver as *mut c_void);
            return Err(format!("ASIO 驱动「{display_name}」无法读取缓冲大小"));
        }
        let requested_frames = buffer_frames
            .map(|frames| frames as i32)
            .unwrap_or(preferred_size)
            .clamp(
                min_size.max(BUFFER_MIN_FRAMES),
                max_size.min(BUFFER_MAX_FRAMES).max(min_size),
            );
        let buffer_size = if granularity > 0 {
            ((requested_frames / granularity) * granularity).clamp(min_size, max_size)
        } else {
            requested_frames
        }
        .max(1);

        let channels = output_channels.min(2) as usize;
        let mut output_types = Vec::with_capacity(channels);
        for channel in 0..channels {
            let mut info = AsioChannelInfo {
                channel: channel as i32,
                is_input: 0,
                is_active: 0,
                channel_group: 0,
                sample_type: 0,
                name: [0; 32],
            };
            if ((*table).get_channel_info)(driver as *mut c_void, &mut info) != ASE_OK {
                ((*table).release)(driver as *mut c_void);
                return Err(format!(
                    "ASIO 驱动「{display_name}」无法读取输出通道 {channel} 的格式"
                ));
            }
            if !is_supported_sample_type(info.sample_type) {
                ((*table).release)(driver as *mut c_void);
                return Err(format!(
                    "ASIO 驱动「{display_name}」使用不支持的采样格式 {}",
                    info.sample_type
                ));
            }
            output_types.push(info.sample_type);
        }

        let mut buffer_infos: Vec<AsioBufferInfo> = (0..channels)
            .map(|channel| AsioBufferInfo {
                is_input: 0,
                channel_num: channel as i32,
                buffers: [std::ptr::null_mut(); 2],
            })
            .collect();
        let callbacks = AsioCallbacks {
            buffer_switch: Some(buffer_switch),
            sample_rate_did_change: Some(sample_rate_changed),
            asio_message: Some(asio_message),
            // 部分驱动不看 `buffer_switch` 而直接调用时间信息回调：必须提供真实函数，
            // 否则会出现空指针跳转（0xc0000005）。
            // Some drivers ignore `buffer_switch` and call the time-info callback directly, so a
            // real function must be provided to avoid a null-pointer jump (0xc0000005).
            buffer_switch_time_info: Some(buffer_switch_time_info),
        };

        let frames = buffer_size.max(1) as usize;
        let mut context = Box::new(AsioCallbackContext {
            router: router.clone(),
            active: None,
            left: vec![0.0; frames],
            right: vec![0.0; frames],
            output_buffers: Vec::with_capacity(channels),
            output_types,
            frames,
            last_l: 0.0,
            last_r: 0.0,
            recovering: false,
            callbacks,
        });
        // 上下文必须在 createBuffers 之前就位：部分驱动会立刻回调一次。
        // The context must be in place before createBuffers: some drivers call back immediately.
        let context_ptr = &mut *context as *mut AsioCallbackContext;
        ASIO_CONTEXT.store(context_ptr, Ordering::Release);

        if ((*table).create_buffers)(
            driver as *mut c_void,
            buffer_infos.as_mut_ptr(),
            channels as i32,
            buffer_size,
            &mut (*context_ptr).callbacks,
        ) != ASE_OK
        {
            ASIO_CONTEXT.store(std::ptr::null_mut(), Ordering::Release);
            ((*table).release)(driver as *mut c_void);
            return Err(format!("ASIO 驱动「{display_name}」创建缓冲失败"));
        }
        for info in &buffer_infos {
            context.output_buffers.push(info.buffers);
        }

        if ((*table).start)(driver as *mut c_void) != ASE_OK {
            ASIO_CONTEXT.store(std::ptr::null_mut(), Ordering::Release);
            ((*table).dispose_buffers)(driver as *mut c_void);
            ((*table).release)(driver as *mut c_void);
            return Err(format!("ASIO 驱动「{display_name}」启动失败"));
        }

        // 启动后再次读取采样率：部分驱动只有真正开始运行后才报告设备速率。
        // Read the rate again after starting: some drivers only report the device rate once running.
        let mut actual_rate = 0.0_f64;
        let _ = ((*table).get_sample_rate)(driver as *mut c_void, &mut actual_rate);
        if actual_rate >= MIN_REASONABLE_RATE {
            current_rate = actual_rate;
        }
        router.set_sample_rate(current_rate);
        Ok(AsioStream {
            driver,
            context: Box::into_raw(context),
            description: format!(
                "{display_name} (ASIO) · {} Hz · {} ch · {} frames",
                current_rate.round() as i32,
                channels,
                buffer_size
            ),
        })
    }
}

/// 所属模块：audio / ASIO 输出 / 回调
/// 功能：ASIO 双缓冲切换回调：渲染当前激活工程并写入驱动缓冲。
/// 输入：
/// - `double_buffer_index`：驱动当前使用的缓冲半区（0 或 1）。
/// - `_direct_process`：驱动建议是否立即处理；宿主总是立即处理。
/// 输出/副作用：写入驱动输出缓冲；推进工程走带与电平统计。
/// 复杂度：时间 O(帧数 × 轨道数 + 本块事件数)，空间 O(1)。
///
/// Module: audio / ASIO output / callback
/// Purpose: ASIO double-buffer switch: render the active project and write the driver buffers.
/// Inputs:
/// - `double_buffer_index`: buffer half currently used by the driver (0 or 1).
/// - `_direct_process`: whether immediate processing is suggested; the host always processes now.
/// Outputs / side effects: writes the driver output buffers; advances transport and levels.
/// Complexity: O(frames × tracks + events in block) time and O(1) space.
unsafe extern "system" fn buffer_switch(double_buffer_index: i32, _direct_process: i32) {
    let context_ptr = ASIO_CONTEXT.load(Ordering::Acquire);
    if context_ptr.is_null() {
        return;
    }
    let context = &mut *context_ptr;
    let index = (double_buffer_index.clamp(0, 1)) as usize;
    render_block(context);
    for (channel, buffers) in context.output_buffers.iter().enumerate() {
        let sample_type = context.output_types.get(channel).copied().unwrap_or(0);
        let data = if channel == 0 {
            &context.left
        } else {
            &context.right
        };
        write_channel(buffers[index], sample_type, data);
    }
}

/// 所属模块：audio / ASIO 输出 / 回调
/// 功能：ASIO 2.x 时间信息版缓冲切换回调：忽略时间信息，复用 `buffer_switch` 渲染。
/// 输入：
/// - `params`：驱动提供的时间信息指针；原样返回。
/// - `double_buffer_index`：驱动当前使用的缓冲半区（0 或 1）。
/// - `direct_process`：驱动建议是否立即处理。
/// 输出/副作用：写入驱动输出缓冲；返回 `params`。
/// 复杂度：时间 O(帧数 × 轨道数)，空间 O(1)。
///
/// Module: audio / ASIO output / callback
/// Purpose: ASIO 2.x time-info buffer switch: ignore the time info and reuse `buffer_switch`.
/// Inputs:
/// - `params`: time info supplied by the driver; returned unchanged.
/// - `double_buffer_index`: buffer half currently used by the driver (0 or 1).
/// - `direct_process`: whether immediate processing is suggested.
/// Outputs / side effects: writes the driver output buffers; returns `params`.
/// Complexity: O(frames × tracks) time and O(1) space.
unsafe extern "system" fn buffer_switch_time_info(
    params: *mut c_void,
    double_buffer_index: i32,
    direct_process: i32,
) -> *mut c_void {
    buffer_switch(double_buffer_index, direct_process);
    params
}

/// 所属模块：audio / ASIO 输出 / 回调
/// 功能：渲染一块音频：读取激活引擎，失败时用短淡出避免爆音。
/// 输入：`context` 实时上下文。
/// 输出/副作用：写入 `left` / `right`；更新淡入淡出状态。
/// 复杂度：时间 O(帧数 × 轨道数 + 本块事件数)，空间 O(1)。
///
/// Module: audio / ASIO output / callback
/// Purpose: Render one block from the active engine, fading out on a missed lock to avoid clicks.
/// Inputs: `context` realtime context.
/// Outputs / side effects: writes `left` / `right`; updates fade state.
/// Complexity: O(frames × tracks + events in block) time and O(1) space.
unsafe fn render_block(context: &mut AsioCallbackContext) {
    if let Some(slot) = context.router.engine_slot() {
        context.active = slot;
    }
    let frames = context.frames;
    let mut processed = false;
    if let Some(engine) = &context.active {
        if let Some(mut engine) = lock_engine(engine) {
            engine.process(&mut context.left, &mut context.right);
            processed = true;
        }
    }
    let fade = (frames / 8).max(1) as f32;
    if processed {
        if context.recovering {
            for frame in 0..frames {
                let gain = (frame as f32 / fade).min(1.0);
                context.left[frame] *= gain;
                context.right[frame] *= gain;
            }
            context.recovering = false;
        }
        context.last_l = context.left[frames.saturating_sub(1)];
        context.last_r = context.right[frames.saturating_sub(1)];
    } else {
        let start_l = context.last_l;
        let start_r = context.last_r;
        for frame in 0..frames {
            let gain = (1.0 - frame as f32 / fade).max(0.0);
            context.left[frame] = start_l * gain;
            context.right[frame] = start_r * gain;
        }
        context.last_l = 0.0;
        context.last_r = 0.0;
        context.recovering = true;
    }
}

/// 所属模块：audio / ASIO 输出 / 采样格式
/// 功能：按驱动声明的采样格式把一个声道的浮点数据写入 ASIO 缓冲。
/// 输入：
/// - `buffer`：驱动缓冲起始地址。
/// - `sample_type`：ASIO 采样格式。
/// - `data`：归一化浮点样本（-1..1）。
/// 输出/副作用：写入驱动内存；格式不支持时不写入。
/// 复杂度：时间 O(帧数)，空间 O(1)。
///
/// Module: audio / ASIO output / sample format
/// Purpose: Write one channel of normalized floats into the ASIO buffer using the driver format.
/// Inputs:
/// - `buffer`: driver buffer start address.
/// - `sample_type`: ASIO sample type.
/// - `data`: normalized samples (-1..1).
/// Outputs / side effects: writes driver memory; unsupported formats write nothing.
/// Complexity: O(frames) time and O(1) space.
unsafe fn write_channel(buffer: *mut c_void, sample_type: i32, data: &[f32]) {
    if buffer.is_null() {
        return;
    }
    match sample_type {
        ASIO_ST_FLOAT32_LSB => {
            let out = buffer as *mut f32;
            for (frame, sample) in data.iter().enumerate() {
                *out.add(frame) = sample.clamp(-1.0, 1.0);
            }
        }
        ASIO_ST_FLOAT64_LSB => {
            let out = buffer as *mut f64;
            for (frame, sample) in data.iter().enumerate() {
                *out.add(frame) = sample.clamp(-1.0, 1.0) as f64;
            }
        }
        ASIO_ST_INT24_LSB => {
            // 24 位样本按 3 字节小端打包；取 i32 的最低 3 字节即为二补码 24 位值。
            // 24-bit samples are packed into 3 little-endian bytes; the low 3 bytes of an i32 are
            // the two's-complement 24-bit value.
            let out = buffer as *mut u8;
            for (frame, sample) in data.iter().enumerate() {
                let value = (sample.clamp(-1.0, 1.0) * 8_388_607.0) as i32;
                let bytes = value.to_le_bytes();
                let base = frame * 3;
                *out.add(base) = bytes[0];
                *out.add(base + 1) = bytes[1];
                *out.add(base + 2) = bytes[2];
            }
        }
        ASIO_ST_INT32_LSB => {
            let out = buffer as *mut i32;
            for (frame, sample) in data.iter().enumerate() {
                *out.add(frame) = (sample.clamp(-1.0, 1.0) * 2_147_483_647.0) as i32;
            }
        }
        ASIO_ST_INT32_LSB16 => {
            write_aligned_int32(buffer, data, 32_767.0, 16);
        }
        ASIO_ST_INT32_LSB18 => {
            write_aligned_int32(buffer, data, 131_071.0, 14);
        }
        ASIO_ST_INT32_LSB20 => {
            write_aligned_int32(buffer, data, 524_287.0, 12);
        }
        ASIO_ST_INT32_LSB24 => {
            write_aligned_int32(buffer, data, 8_388_607.0, 8);
        }
        ASIO_ST_INT16_LSB => {
            let out = buffer as *mut i16;
            for (frame, sample) in data.iter().enumerate() {
                *out.add(frame) = (sample.clamp(-1.0, 1.0) * 32_767.0) as i16;
            }
        }
        _ => {}
    }
}

/// 所属模块：audio / ASIO 输出 / 采样格式
/// 功能：把浮点样本写入“左对齐到 32 位容器”的整数缓冲：先按有效位数缩放，再左移对齐。
/// 输入：
/// - `buffer`：驱动缓冲起始地址。
/// - `data`：归一化浮点样本（-1..1）。
/// - `full_scale`：有效位数的满量程（如 24 位为 8388607）。
/// - `shift`：左移位数（32 - 有效位数）。
/// 输出/副作用：写入驱动内存；不修改输入。
/// 复杂度：时间 O(帧数)，空间 O(1)。
///
/// Module: audio / ASIO output / sample format
/// Purpose: Write floats into a 32-bit container with the valid bits left-aligned: scale to the
/// effective bit depth, then shift left.
/// Inputs:
/// - `buffer`: driver buffer start address.
/// - `data`: normalized samples (-1..1).
/// - `full_scale`: full-scale value of the effective depth (e.g. 8_388_607 for 24 bits).
/// - `shift`: left shift (32 minus the effective bit depth).
/// Outputs / side effects: writes driver memory; inputs are untouched.
/// Complexity: O(frames) time and O(1) space.
unsafe fn write_aligned_int32(buffer: *mut c_void, data: &[f32], full_scale: f32, shift: u32) {
    let out = buffer as *mut i32;
    for (frame, sample) in data.iter().enumerate() {
        let value = (sample.clamp(-1.0, 1.0) * full_scale) as i32;
        *out.add(frame) = value << shift;
    }
}

/// 所属模块：audio / ASIO 输出 / 回调
/// 功能：驱动采样率变化回调；宿主不主动重启流，只记录到驱动说明。
/// 输入：`_rate` 新采样率。
/// 输出/副作用：无；避免在实时线程做任何分配或日志。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / ASIO output / callback
/// Purpose: Driver sample-rate change; the host does not restart the stream and only ignores it.
/// Inputs: `_rate` new sample rate.
/// Outputs / side effects: none; no allocation or logging on the realtime thread.
/// Complexity: O(1) time and space.
unsafe extern "system" fn sample_rate_changed(_rate: f64) {}

/// 所属模块：audio / ASIO 输出 / 回调
/// 功能：响应驱动的通用消息；不支持时间信息与时间码，返回 0。
/// 输入：`_selector` / `_value` / `_message` / `_opt`。
/// 输出/副作用：返回 0；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / ASIO output / callback
/// Purpose: Answer generic driver messages; time info and timecode are unsupported, so return 0.
/// Inputs: `_selector` / `_value` / `_message` / `_opt`.
/// Outputs / side effects: returns 0; no state change.
/// Complexity: O(1) time and space.
unsafe extern "system" fn asio_message(
    _selector: i32,
    _value: i32,
    _message: *mut c_void,
    _opt: *mut f64,
) -> i32 {
    0
}

/// 所属模块：audio / ASIO 输出 / 注册表
/// 功能：枚举注册表 `HKLM\SOFTWARE\ASIO` 下的驱动：键名、CLSID 与描述。
/// 输入：无。
/// 输出/副作用：返回 `(键名, CLSID, 描述)`；只读注册表。
/// 复杂度：时间 O(驱动数 × 键名长度)，空间 O(驱动数)。
///
/// Module: audio / ASIO output / registry
/// Purpose: Enumerate drivers under `HKLM\SOFTWARE\ASIO`: key name, CLSID and description.
/// Inputs: none.
/// Outputs / side effects: `(key name, CLSID, description)` entries; reads the registry only.
/// Complexity: O(driver count × key length) time and O(driver count) space.
fn enumerate_drivers() -> Vec<(String, GUID, String)> {
    let mut drivers = Vec::new();
    let path: Vec<u16> = ASIO_REGISTRY_PATH
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_LOCAL_MACHINE, path.as_ptr(), 0, KEY_READ, &mut key) != 0 {
            return drivers;
        }
        let mut index = 0_u32;
        loop {
            let mut name = [0_u16; 256];
            let mut name_len = name.len() as u32;
            let result = RegEnumKeyExW(
                key,
                index,
                name.as_mut_ptr(),
                &mut name_len,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if result != 0 {
                break;
            }
            index += 1;
            let key_name = String::from_utf16_lossy(&name[..name_len as usize]);
            let Some(clsid) =
                registry_string(key, &key_name, "CLSID").and_then(|text| parse_clsid(&text))
            else {
                continue;
            };
            let description =
                registry_string(key, &key_name, "Description").unwrap_or_else(|| key_name.clone());
            drivers.push((key_name, clsid, description));
        }
        RegCloseKey(key);
    }
    drivers.sort_by(|left, right| left.2.cmp(&right.2));
    drivers
}

/// 所属模块：audio / ASIO 输出 / 注册表
/// 功能：读取驱动子键下的字符串值。
/// 输入：
/// - `root`：已打开的 ASIO 根键。
/// - `sub_key`：驱动子键名。
/// - `value_name`：值名（如 `CLSID` / `Description`）。
/// 输出/副作用：返回字符串；缺失或类型不匹配时为 `None`。
/// 复杂度：时间 O(值长度)，空间 O(值长度)。
///
/// Module: audio / ASIO output / registry
/// Purpose: Read a string value under one driver sub-key.
/// Inputs:
/// - `root`: opened ASIO root key.
/// - `sub_key`: driver sub-key name.
/// - `value_name`: value name (`CLSID` / `Description`).
/// Outputs / side effects: the string, or `None` when missing or of another type.
/// Complexity: O(value length) time and space.
unsafe fn registry_string(root: HKEY, sub_key: &str, value_name: &str) -> Option<String> {
    let path: Vec<u16> = sub_key.encode_utf16().chain(std::iter::once(0)).collect();
    let name: Vec<u16> = value_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut key: HKEY = std::ptr::null_mut();
    if RegOpenKeyExW(root, path.as_ptr(), 0, KEY_READ, &mut key) != 0 {
        return None;
    }
    let mut value_type: DWORD = 0;
    let mut size = 0_u32;
    let query = RegQueryValueExW(
        key,
        name.as_ptr(),
        std::ptr::null_mut(),
        &mut value_type,
        std::ptr::null_mut(),
        &mut size,
    );
    if query != 0 || value_type != REG_SZ || size == 0 {
        RegCloseKey(key);
        return None;
    }
    let mut buffer = vec![0_u8; size as usize];
    let query = RegQueryValueExW(
        key,
        name.as_ptr(),
        std::ptr::null_mut(),
        &mut value_type,
        buffer.as_mut_ptr(),
        &mut size,
    );
    RegCloseKey(key);
    if query != 0 {
        return None;
    }
    let wide: Vec<u16> = buffer
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    Some(String::from_utf16_lossy(&wide))
}

/// 所属模块：audio / ASIO 输出 / GUID
/// 功能：解析注册表中的 CLSID 字符串（`{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`）。
/// 输入：`text` CLSID 文本。
/// 输出/副作用：返回 GUID；格式错误时为 `None`。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: audio / ASIO output / GUID
/// Purpose: Parse a CLSID string from the registry.
/// Inputs: `text` — CLSID text.
/// Outputs / side effects: the GUID, or `None` when malformed.
/// Complexity: O(text length) time and O(1) space.
fn parse_clsid(text: &str) -> Option<GUID> {
    let trimmed = text.trim().trim_start_matches('{').trim_end_matches('}');
    let mut parts = trimmed.split('-');
    let data1 = u32::from_str_radix(parts.next()?, 16).ok()?;
    let data2 = u16::from_str_radix(parts.next()?, 16).ok()?;
    let data3 = u16::from_str_radix(parts.next()?, 16).ok()?;
    let high = parts.next()?;
    let low = parts.next()?;
    if high.len() != 4 || low.len() != 12 {
        return None;
    }
    let mut data4 = [0_u8; 8];
    let high_bytes = u16::from_str_radix(high, 16).ok()?.to_be_bytes();
    data4[0] = high_bytes[0];
    data4[1] = high_bytes[1];
    for index in 0..6 {
        data4[index + 2] = u8::from_str_radix(&low[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(GUID {
        Data1: data1,
        Data2: data2,
        Data3: data3,
        Data4: data4,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：注册表 CLSID 文本能被正确解析为 GUID。
    /// Covers: a registry CLSID string parses into the expected GUID.
    #[test]
    fn clsid_text_parses_into_a_guid() {
        let guid = parse_clsid("{188135E1-D565-11D2-854F-00A0C99F5D19}").expect("parse");
        assert_eq!(guid.Data1, 0x1881_35E1);
        assert_eq!(guid.Data2, 0xD565);
        assert_eq!(guid.Data3, 0x11D2);
        assert_eq!(guid.Data4, [0x85, 0x4F, 0x00, 0xA0, 0xC9, 0x9F, 0x5D, 0x19]);
    }

    /// 覆盖：非法 CLSID 文本被拒绝。
    /// Covers: malformed CLSID text is rejected.
    #[test]
    fn malformed_clsid_text_is_rejected() {
        assert!(parse_clsid("not-a-guid").is_none());
        assert!(parse_clsid("{188135E1-D565-11D2-854F-00A0C99F5D1}").is_none());
    }

    /// 覆盖：只接受可写采样格式。
    /// Covers: only writable sample formats are accepted.
    #[test]
    fn only_writable_sample_types_are_supported() {
        assert!(is_supported_sample_type(ASIO_ST_FLOAT32_LSB));
        assert!(is_supported_sample_type(ASIO_ST_INT32_LSB));
        assert!(is_supported_sample_type(ASIO_ST_INT16_LSB));
        assert!(is_supported_sample_type(ASIO_ST_INT24_LSB));
        assert!(is_supported_sample_type(ASIO_ST_INT32_LSB24));
        assert!(!is_supported_sample_type(32));
    }

    /// 覆盖：本机存在 ASIO 驱动时能创建并启动输出流（诊断用，默认跳过）。
    /// Covers: local ASIO drivers can be opened and started (diagnostic, ignored by default).
    #[test]
    #[ignore = "requires a local ASIO driver and audio device; opens real output streams"]
    fn opens_local_asio_drivers() {
        let drivers = driver_names();
        eprintln!("ASIO drivers: {drivers:?}");
        let filter = std::env::var("ASIO_TEST_DRIVER").unwrap_or_default();
        for driver in &drivers {
            if !filter.is_empty() && !driver.description.eq_ignore_ascii_case(&filter) {
                continue;
            }
            let router = AudioRouter::new();
            match start(router, &driver.description, Some(48_000), Some(512)) {
                Ok(stream) => {
                    eprintln!("{} started: {}", driver.description, stream.description);
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    drop(stream);
                    eprintln!("{} dropped", driver.description);
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
                Err(error) => eprintln!("{} start failed: {error}", driver.description),
            }
        }
        eprintln!("asio driver loop done");
    }
}
