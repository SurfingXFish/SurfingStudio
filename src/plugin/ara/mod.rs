//! 所属模块：plugin / ARA2 宿主
//! 文件用途：
//! - 在 VST3 插件实例上发现 ARA 工厂（`ARA Main Factory Class` 或
//!   `ARA::IPlugInEntryPoint`）。
//! - 建立最小可用的 ARA 文档模型：文档、音乐上下文（速度/拍号）、区域序列、
//!   静音音频源、音频修改与播放区域，并把插件实例绑定到该文档控制器。
//! - 提供宿主侧回调：音频数据（静音）、内容读取（速度/拍号）、模型更新通知、
//!   归档（暂不支持）与播放控制请求。
//!
//! 设计约束：
//! - ARA 文档控制器及其全部模型对象必须在同一个线程上创建和销毁；本模块记录创建
//!   线程，在错误的线程析构时只记录日志并跳过销毁，避免让插件在自己的线程模型里崩溃。
//! - VST3 侧必须在首次 `setActive(true)` 之前完成绑定，因此加载器使用
//!   `vst3_host` 的延迟激活选项。
//! - 所有回调都捕获 panic，绝不让 Rust 的 unwind 穿过 C ABI。
//!
//! Module: plugin / ARA2 host
//! Purpose:
//! - Discover the ARA factory on a VST3 instance (`ARA Main Factory Class` or
//!   `ARA::IPlugInEntryPoint`).
//! - Build a minimal ARA document model: document, musical context (tempo / time signature),
//!   region sequence, silent audio source, audio modification and playback region, then bind
//!   the plug-in instance to that document controller.
//! - Provide host callbacks: audio samples (silence), content reading (tempo / bar signatures),
//!   model-update notifications, archiving (not supported yet) and playback requests.
//!
//! Design constraints:
//! - The ARA document controller and every model object must be created and destroyed on the
//!   same thread. This module records the creating thread and skips destruction (logging
//!   instead) when dropped elsewhere, rather than crashing inside the plug-in's thread model.
//! - VST3 requires binding before the first `setActive(true)`, so loading goes through
//!   `vst3_host`'s deferred-activation option.
//! - Every callback catches panics: Rust unwinding must never cross the C ABI.

// 本模块几乎全部是 FFI 回调与裸指针操作；每个 unsafe fn 的调用约定都在函数文档里写明，
// 逐行再包一层 unsafe 只会淹没真正的边界检查。
// Nearly everything here is FFI callbacks and raw pointers; each unsafe fn documents its
// contract, and per-line unsafe blocks would only obscure the actual boundary checks.
#![allow(unsafe_op_in_unsafe_fn)]

pub mod ffi;

use self::ffi::*;
use crate::diag;
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::ThreadId;
use vst3::Steinberg::{FUnknown, kResultOk, kResultTrue};
use vst3::com_scrape_types::Guid;
use vst3_host::RawPluginHandles;

/// 把 VST3 的 `DECLARE_CLASS_IID` 四个 32 位整数转换成 16 字节 GUID。
/// Convert the four 32-bit integers of VST3's `DECLARE_CLASS_IID` into a 16-byte GUID.
///
/// 注意不能手写“大端拼接”：VST3 的 FUID 采用混合字节序，直接用 `vst3::uid` 保证与
/// 官方绑定里的接口 ID 完全一致。
/// Do not hand-write big-endian byte order: VST3's FUID uses mixed endianness, so `vst3::uid`
/// is used to match the official bindings exactly.
const fn ara_uid(a: u32, b: u32, c: u32, d: u32) -> [u8; 16] {
    let tuid = vst3::uid(a, b, c, d);
    let mut out = [0u8; 16];
    let mut index = 0;
    while index < 16 {
        out[index] = tuid[index] as u8;
        index += 1;
    }
    out
}

/// `ARA::IMainFactory` 的类 ID（来自 `ARAVST3.h` 的 `DECLARE_CLASS_IID`）。
/// Class ID of `ARA::IMainFactory` (from `DECLARE_CLASS_IID` in `ARAVST3.h`).
const IID_ARA_MAIN_FACTORY: [u8; 16] = ara_uid(0xDB2A1669, 0xFAFD42A5, 0xA82F864F, 0x7B6872EA);
/// `ARA::IPlugInEntryPoint` 的类 ID。
/// Class ID of `ARA::IPlugInEntryPoint`.
const IID_ARA_PLUGIN_ENTRY_POINT: [u8; 16] =
    ara_uid(0x12814E54, 0xA1CE4076, 0x82B96813, 0x16950BD6);
/// `ARA::IPlugInEntryPoint2` 的类 ID（带角色绑定）。
/// Class ID of `ARA::IPlugInEntryPoint2` (role-aware binding).
const IID_ARA_PLUGIN_ENTRY_POINT2: [u8; 16] =
    ara_uid(0xCD9A5913, 0xC9EB46D7, 0x96CA53AD, 0xD1DB89F5);

/// 扫描 ARA 标记时读取的二进制上限：超过该大小的文件只读前 64 MiB。
/// Upper bound when scanning binaries for ARA markers: only the first 64 MiB is read.
const ARA_MARKER_SCAN_LIMIT: u64 = 64 * 1024 * 1024;

/// 静音音频源的通道数。
/// Channel count of the silent audio source.
const SILENT_SOURCE_CHANNELS: i32 = 2;

/// `IMainFactory` 的 vtable（`FUnknown` 之后一个 `getFactory`）。
/// `IMainFactory` vtable (`getFactory` after `FUnknown`).
#[repr(C)]
struct IMainFactoryVtbl {
    base: vst3::Steinberg::FUnknownVtbl,
    get_factory: unsafe extern "system" fn(*mut c_void) -> *const AraFactory,
}

/// `IPlugInEntryPoint` 的 vtable。
/// `IPlugInEntryPoint` vtable.
#[repr(C)]
struct IPlugInEntryPointVtbl {
    base: vst3::Steinberg::FUnknownVtbl,
    get_factory: unsafe extern "system" fn(*mut c_void) -> *const AraFactory,
    bind_to_document_controller:
        unsafe extern "system" fn(*mut c_void, AraRef) -> *const AraPlugInExtensionInstance,
}

/// `IPlugInEntryPoint2` 的 vtable（追加角色绑定）。
/// `IPlugInEntryPoint2` vtable (adds role-aware binding).
#[repr(C)]
struct IPlugInEntryPoint2Vtbl {
    base: vst3::Steinberg::FUnknownVtbl,
    get_factory: unsafe extern "system" fn(*mut c_void) -> *const AraFactory,
    bind_to_document_controller:
        unsafe extern "system" fn(*mut c_void, AraRef) -> *const AraPlugInExtensionInstance,
    bind_to_document_controller_with_roles:
        unsafe extern "system" fn(
            *mut c_void,
            AraRef,
            i32,
            i32,
        ) -> *const AraPlugInExtensionInstance,
}

/// 插件通过 `IPlugInEntryPoint` 暴露的 ARA 工厂信息。
/// ARA factory metadata exposed by the plug-in through `IPlugInEntryPoint`.
#[derive(Clone, Debug, Default)]
pub struct AraFactoryInfo {
    pub plugin_name: String,
    pub manufacturer: String,
    pub version: String,
    pub factory_id: String,
    pub document_archive_id: String,
    pub lowest_generation: i32,
    pub highest_generation: i32,
    pub analyzeable_content_types: Vec<i32>,
    pub transformation_flags: i32,
}

/// 插件向宿主发出的播放控制请求（宿主可择机执行）。
/// Playback control request issued by the plug-in (the host applies it when convenient).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlaybackRequest {
    Start,
    Stop,
    Seek(f64),
    CycleRange(f64, f64),
    EnableCycle(bool),
}

/// 宿主侧音乐上下文内容：恒定速度与拍号。
/// Host musical context content: constant tempo and time signature.
#[derive(Clone, Copy, Debug)]
struct TempoMap {
    bpm: f64,
    numerator: i32,
    denominator: i32,
}

impl Default for TempoMap {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            numerator: 4,
            denominator: 4,
        }
    }
}

/// 宿主控制器状态：所有回调拿到的 `hostRef` 都指向这个对象的稳定地址。
/// Host controller state: every callback's `hostRef` points at this stable object.
struct HostState {
    audio_access: AraAudioAccessControllerInterface,
    archiving: AraArchivingControllerInterface,
    content_access: AraContentAccessControllerInterface,
    model_update: AraModelUpdateControllerInterface,
    playback: AraPlaybackControllerInterface,
    audio_access_host_ref: AraHostRef,
    archiving_host_ref: AraHostRef,
    content_access_host_ref: AraHostRef,
    model_update_host_ref: AraHostRef,
    playback_host_ref: AraHostRef,
    tempo: Mutex<TempoMap>,
    archive_id: CString,
    /// 指向断言函数指针的稳定地址（必须活到 `uninitializeARA`）。
    /// Stable address of the assert function pointer (must live until `uninitializeARA`).
    assert_function: Box<AraAssertFunction>,
    requests: Mutex<Vec<PlaybackRequest>>,
    content_changes: AtomicU64,
}

/// 宿主侧音频读取器：当前只提供静音数据。
/// Host-side audio reader: silence only for now.
struct SilentAudioReader {
    channels: i32,
    use_64_bit: bool,
}

/// 宿主侧内容读取器（速度表或拍号表）。
/// Host-side content reader (tempo map or bar signatures).
struct HostContentReader {
    kind: i32,
    tempo: Vec<AraContentTempoEntry>,
    bars: Vec<AraContentBarSignature>,
    /// 最近一次返回给插件的事件指针（仅供插件在下次调用前读取）。
    /// Last event pointer handed to the plug-in (valid until the next call).
    #[allow(dead_code)]
    last: *const c_void,
}

/// 文档内的模型对象引用（插件侧不透明指针）。
/// Model object references inside the document (opaque plug-in pointers).
#[derive(Default, Clone, Copy)]
struct ModelObjects {
    musical_context: AraRef,
    region_sequence: AraRef,
    audio_source: AraRef,
    audio_modification: AraRef,
    playback_region: AraRef,
}

/// 一个已绑定的 ARA 会话。
/// One bound ARA session.
pub struct AraSession {
    info: AraFactoryInfo,
    factory: *const AraFactory,
    controller_ref: AraRef,
    controller: *const AraDocumentControllerInterface,
    host: Box<HostState>,
    /// 传给插件的宿主实例表；仅需保活（插件持有其中的指针）。
    /// Host instance table passed to the plug-in; kept alive because the plug-in holds pointers.
    _host_instance: Box<AraDocumentControllerHostInstance>,
    /// 插件侧模型对象。
    /// Plug-in side model objects.
    objects: ModelObjects,
    /// 宿主侧不透明引用（音频源 / 音乐上下文等），保证地址稳定。
    /// Host-side opaque references (audio source, musical context, ...), kept address-stable.
    markers: Vec<Box<u8>>,
    /// 供属性结构体引用的 C 字符串。
    /// C strings referenced by the property structs.
    strings: Box<SessionStrings>,
    /// 当前播放区域覆盖的时间范围（秒）。
    /// Time range covered by the current playback region (seconds).
    timeline: Option<(f64, f64)>,
    sample_rate: f64,
    /// 已绑定的插件扩展实例（插件生命周期内有效）。
    /// Bound plug-in extension instance (valid for the plug-in's lifetime).
    extension: *const AraPlugInExtensionInstance,
    bound_roles: i32,
    controller_thread: ThreadId,
}

/// 会话持有的 C 字符串。
/// C strings owned by the session.
struct SessionStrings {
    document_name: CString,
    musical_context_name: CString,
    region_sequence_name: CString,
    audio_source_name: CString,
    audio_modification_name: CString,
    playback_region_name: CString,
    source_persistent_id: CString,
    modification_persistent_id: CString,
}

// 会话跨线程传递是安全的：内部只有 Arc/Mutex 与稳定的堆上对象；所有 ARA 调用仍然
// 只在控制器线程上发生（调用方负责），Drop 在错误的线程会直接跳过。
// Moving the session across threads is safe: it only owns heap objects and mutexes; every ARA
// call still happens on the controller thread (the caller's responsibility) and Drop skips
// destruction on the wrong thread.
unsafe impl Send for AraSession {}

/// 所属模块：plugin / ARA2 宿主
/// 功能：判断是否启用 ARA 绑定（默认关闭，环境变量 `SURFING_ARA_HOST=1` 开启）。
/// 输入：无。
/// 输出/副作用：返回布尔值；读取进程环境变量。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: plugin / ARA2 host
/// Purpose: Whether ARA binding is enabled (off by default; `SURFING_ARA_HOST=1` enables it).
/// Inputs: none.
/// Outputs / side effects: a boolean; reads the process environment.
/// Complexity: O(1) time and space.
///
/// 默认关闭的原因：部分插件（实测 VOCALOID Bridge 6.5）在角色绑定时会在自己的代码里崩溃，
/// 宿主无法拦截；开启前请确认插件版本。
/// Off by default because some plug-ins (VOCALOID Bridge 6.5 observed) crash inside their own
/// role-binding code, which a host cannot intercept.
pub fn host_enabled() -> bool {
    match std::env::var("SURFING_ARA_HOST") {
        Ok(value) => {
            let value = value.trim();
            value == "1" || value.eq_ignore_ascii_case("true")
        }
        Err(_) => false,
    }
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：只读取插件的 ARA 工厂信息，不创建文档、不绑定。
/// 输入：`handles` 插件原始 VST3 指针。
/// 输出/副作用：返回工厂信息；没有 ARA 支持时为 `None`。
/// 复杂度：时间 O(类枚举)，空间 O(1)。
///
/// Module: plugin / ARA2 host
/// Purpose: Read the plug-in's ARA factory metadata without creating a document or binding.
/// Inputs: `handles` — raw VST3 pointers of the plug-in.
/// Outputs / side effects: the factory metadata, or `None` when ARA is unsupported.
/// Complexity: O(class enumeration) time and O(1) space.
pub unsafe fn probe_factory(handles: RawPluginHandles) -> Option<AraFactoryInfo> {
    let factory = discover_factory(handles).ok()?;
    Some(read_factory_info(factory))
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：判断插件二进制是否声明了 ARA 支持（只读文件，不实例化插件）。
/// 输入：`path` VST3 单文件或 bundle 目录。
/// 输出/副作用：命中 `ARA Main Factory Class` 等标记时返回 `true`；只读文件系统。
/// 复杂度：时间 O(读取的字节数)，空间 O(单次读取上限)。
///
/// Module: plugin / ARA2 host
/// Purpose: Whether the plug-in binary declares ARA support (read-only, no instantiation).
/// Inputs: `path` — VST3 single file or bundle directory.
/// Outputs / side effects: `true` when an ARA marker such as `ARA Main Factory Class` is found;
/// the filesystem is read-only.
/// Complexity: O(bytes read) time and O(read limit) space.
pub fn declares_ara(path: &Path) -> bool {
    const MARKERS: [&[u8]; 2] = [b"ARA Main Factory Class", b"arafactory"];
    let mut files = Vec::new();
    if path.is_dir() {
        collect_binaries(path, 0, &mut files);
    } else {
        files.push(path.to_path_buf());
    }
    for file in files {
        let Ok(metadata) = std::fs::metadata(&file) else {
            continue;
        };
        if metadata.len() == 0 || metadata.len() > ARA_MARKER_SCAN_LIMIT * 4 {
            continue;
        }
        let Ok(bytes) = std::fs::read(&file) else {
            continue;
        };
        if MARKERS
            .iter()
            .any(|marker| bytes.windows(marker.len()).any(|window| window == *marker))
        {
            return true;
        }
    }
    false
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：递归收集 bundle 内的二进制候选文件（跳过资源目录）。
/// 输入：`dir` 目录、`depth` 当前深度、`out` 结果列表。
/// 输出/副作用：追加文件路径；只读文件系统。
/// 复杂度：时间 O(目录项数)，空间 O(文件数)。
///
/// Module: plugin / ARA2 host
/// Purpose: Recursively collect binary candidates inside a bundle (resource folders skipped).
/// Inputs: `dir`, `depth`, `out`.
/// Outputs / side effects: appends file paths; the filesystem is read-only.
/// Complexity: O(directory entries) time and O(files) space.
fn collect_binaries(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if name == "resources" {
                continue;
            }
            collect_binaries(&path, depth + 1, out);
        } else if let Ok(metadata) = entry.metadata() {
            if metadata.is_file() && metadata.len() > 4096 {
                out.push(path);
            }
        }
    }
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：从插件实例发现 ARA 工厂。
/// 输入：`handles` 插件原始 VST3 指针。
/// 输出/副作用：返回工厂指针；不支持的插件返回错误。
/// 复杂度：时间 O(类枚举)，空间 O(1)。
///
/// Module: plugin / ARA2 host
/// Purpose: Discover the ARA factory on a plug-in instance.
/// Inputs: `handles` — raw VST3 pointers of the plug-in.
/// Outputs / side effects: the factory pointer, or an error for unsupported plug-ins.
/// Complexity: O(class enumeration) time and O(1) space.
unsafe fn discover_factory(handles: RawPluginHandles) -> Result<*const AraFactory, String> {
    // 首选处理器上的 IPlugInEntryPoint：它描述的正是这个实例对应的 ARA 工厂。
    // Prefer IPlugInEntryPoint on the processor: it describes the factory of this instance.
    if !handles.processor.is_null() {
        if let Some(factory) = plugin_entry_point_factory(handles.processor) {
            return Ok(factory);
        }
    }
    // 回退到模块工厂的 "ARA Main Factory Class"。
    // Fall back to the module factory's "ARA Main Factory Class".
    if !handles.factory.is_null() {
        if let Some(factory) = main_factory_from_vst3(handles.factory) {
            return Ok(factory);
        }
    }
    Err("未找到 ARA 工厂".to_string())
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：通过处理器的 `IPlugInEntryPoint` 取 ARA 工厂。
/// 输入：`processor` `IAudioProcessor` 指针。
/// 输出/副作用：返回工厂指针；接口不存在时为 `None`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: plugin / ARA2 host
/// Purpose: Get the ARA factory through the processor's `IPlugInEntryPoint`.
/// Inputs: `processor` — `IAudioProcessor` pointer.
/// Outputs / side effects: the factory pointer, or `None` when the interface is absent.
/// Complexity: O(1) time and space.
unsafe fn plugin_entry_point_factory(processor: *mut c_void) -> Option<*const AraFactory> {
    let entry = query_interface(processor, &IID_ARA_PLUGIN_ENTRY_POINT)?;
    let vtbl = *(entry as *mut *const IPlugInEntryPointVtbl);
    let factory = ((*vtbl).get_factory)(entry);
    release(entry);
    (!factory.is_null()).then_some(factory)
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：从 VST3 工厂枚举 `ARA Main Factory Class` 并取 ARA 工厂。
/// 输入：`factory` `IPluginFactory` 指针。
/// 输出/副作用：返回工厂指针；没有该类时为 `None`。
/// 复杂度：时间 O(类数)，空间 O(1)。
///
/// Module: plugin / ARA2 host
/// Purpose: Enumerate the VST3 factory's `ARA Main Factory Class` and get the ARA factory.
/// Inputs: `factory` — `IPluginFactory` pointer.
/// Outputs / side effects: the factory pointer, or `None` when the class is absent.
/// Complexity: O(class count) time and O(1) space.
unsafe fn main_factory_from_vst3(factory: *mut c_void) -> Option<*const AraFactory> {
    use vst3::Steinberg::IPluginFactory;
    let factory = factory as *mut IPluginFactory;
    let vtbl = (*factory).vtbl;
    let count = ((*vtbl).countClasses)(factory);
    for index in 0..count {
        let mut info: vst3::Steinberg::PClassInfo = std::mem::zeroed();
        if ((*vtbl).getClassInfo)(factory, index, &mut info) != kResultOk {
            continue;
        }
        let category = c_array_to_string(&info.category);
        if category != "ARA Main Factory Class" {
            continue;
        }
        let mut obj: *mut c_void = std::ptr::null_mut();
        let iid = IID_ARA_MAIN_FACTORY;
        let result = ((*vtbl).createInstance)(
            factory,
            info.cid.as_ptr() as *const c_char,
            iid.as_ptr() as *const c_char,
            &mut obj,
        );
        if (result != kResultOk && result != kResultTrue) || obj.is_null() {
            continue;
        }
        let main_vtbl = *(obj as *mut *const IMainFactoryVtbl);
        let ara = ((*main_vtbl).get_factory)(obj);
        release(obj);
        if !ara.is_null() {
            return Some(ara);
        }
    }
    None
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：在 COM 对象上查询接口。
/// 输入：`object` 对象指针、`iid` 16 字节接口 ID。
/// 输出/副作用：返回接口指针（已 addRef）；不支持时为 `None`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: plugin / ARA2 host
/// Purpose: Query an interface on a COM object.
/// Inputs: `object`, `iid` (16-byte interface id).
/// Outputs / side effects: the interface pointer (addRef'd), or `None`.
/// Complexity: O(1) time and space.
unsafe fn query_interface(object: *mut c_void, iid: &[u8; 16]) -> Option<*mut c_void> {
    if object.is_null() {
        return None;
    }
    let unknown = object as *mut FUnknown;
    let guid: Guid = *iid;
    let mut out: *mut c_void = std::ptr::null_mut();
    let vtbl = (*unknown).vtbl;
    let result = ((*vtbl).queryInterface)(unknown, guid.as_ptr() as *const _, &mut out);
    if (result == kResultOk || result == kResultTrue) && !out.is_null() {
        Some(out)
    } else {
        None
    }
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：释放一个 COM 引用。
/// 输入：`object` 对象指针。
/// 输出/副作用：调用 release；空指针忽略。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: plugin / ARA2 host
/// Purpose: Release a COM reference.
/// Inputs: `object`.
/// Outputs / side effects: calls release; null is ignored.
/// Complexity: O(1) time and space.
unsafe fn release(object: *mut c_void) {
    if object.is_null() {
        return;
    }
    let unknown = object as *mut FUnknown;
    let vtbl = (*unknown).vtbl;
    ((*vtbl).release)(unknown);
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：把固定长度 C 字符数组转成字符串（去掉尾部 NUL）。
/// 输入：`array` 字符数组。
/// 输出/副作用：返回 Rust 字符串。
/// 复杂度：时间 O(长度)，空间 O(长度)。
///
/// Module: plugin / ARA2 host
/// Purpose: Convert a fixed-size C char array to a string (trailing NULs trimmed).
/// Inputs: `array`.
/// Outputs / side effects: a Rust string.
/// Complexity: O(length) time and space.
fn c_array_to_string(array: &[c_char]) -> String {
    let end = array.iter().position(|c| *c == 0).unwrap_or(array.len());
    let bytes: Vec<u8> = array[..end].iter().map(|c| *c as u8).collect();
    String::from_utf8_lossy(&bytes).to_string()
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：读取 C 字符串。
/// 输入：`text` 字符串指针，可为空。
/// 输出/副作用：返回 Rust 字符串；空指针为空串。
/// 复杂度：时间 O(长度)，空间 O(长度)。
///
/// Module: plugin / ARA2 host
/// Purpose: Read a C string.
/// Inputs: `text` — may be null.
/// Outputs / side effects: a Rust string; null becomes empty.
/// Complexity: O(length) time and space.
unsafe fn read_c_string(text: *const c_char) -> String {
    if text.is_null() {
        return String::new();
    }
    CStr::from_ptr(text).to_string_lossy().to_string()
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：读取 ARA 工厂的展示信息。
/// 输入：`factory` 工厂指针。
/// 输出/副作用：返回信息结构；只读插件内存。
/// 复杂度：时间 O(字符串长度 + 内容类型数)，空间 O(内容类型数)。
///
/// Module: plugin / ARA2 host
/// Purpose: Read the ARA factory's descriptive metadata.
/// Inputs: `factory`.
/// Outputs / side effects: the metadata; only reads plug-in memory.
/// Complexity: O(string lengths + content types) time and O(content types) space.
unsafe fn read_factory_info(factory: *const AraFactory) -> AraFactoryInfo {
    let f = &*factory;
    let mut analyzeable = Vec::new();
    let count = f.analyzeable_content_types_count;
    let types = f.analyzeable_content_types;
    if !types.is_null() {
        for index in 0..count.min(32) {
            analyzeable.push(*types.add(index));
        }
    }
    AraFactoryInfo {
        plugin_name: read_c_string(f.plug_in_name),
        manufacturer: read_c_string(f.manufacturer_name),
        version: read_c_string(f.version),
        factory_id: read_c_string(f.factory_id),
        document_archive_id: read_c_string(f.document_archive_id),
        lowest_generation: f.lowest_supported_api_generation,
        highest_generation: f.highest_supported_api_generation,
        analyzeable_content_types: analyzeable,
        transformation_flags: f.supported_playback_transformation_flags,
    }
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：把 `hostRef` 还原为宿主状态引用。
/// 输入：`host_ref` 回调收到的宿主引用。
/// 输出/副作用：返回状态引用；空指针为 `None`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: plugin / ARA2 host
/// Purpose: Turn a callback's `hostRef` back into the host state.
/// Inputs: `host_ref`.
/// Outputs / side effects: the state reference, or `None` for null.
/// Complexity: O(1) time and space.
unsafe fn host_state<'a>(host_ref: AraHostRef) -> Option<&'a HostState> {
    if host_ref.is_null() {
        return None;
    }
    Some(&*(host_ref as *const HostState))
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：执行回调体并吞掉 panic。
/// 输入：`body` 回调逻辑。
/// 输出/副作用：panic 被记录到诊断日志，不跨 C ABI。
/// 复杂度：时间 O(回调)，空间 O(1)。
///
/// Module: plugin / ARA2 host
/// Purpose: Run a callback body and swallow panics.
/// Inputs: `body`.
/// Outputs / side effects: panics are logged; nothing crosses the C ABI.
/// Complexity: O(callback) time and O(1) space.
fn guard(scope: &'static str, body: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).is_err() {
        diag::error("plugin.ara", "ara.callback_panic", &[("callback", scope)]);
    }
}

unsafe extern "C" fn cb_create_audio_reader(
    _controller: AraHostRef,
    _source: AraHostRef,
    use64bit: AraBool,
) -> AraHostRef {
    Box::into_raw(Box::new(SilentAudioReader {
        channels: SILENT_SOURCE_CHANNELS,
        use_64_bit: use64bit != 0,
    })) as AraHostRef
}

unsafe extern "C" fn cb_read_audio_samples(
    _controller: AraHostRef,
    reader: AraHostRef,
    _position: AraSampleCount,
    samples: AraSampleCount,
    buffers: *const *mut c_void,
) -> AraBool {
    if reader.is_null() || buffers.is_null() || samples <= 0 {
        return ARA_FALSE;
    }
    let reader = &*(reader as *const SilentAudioReader);
    let bytes_per_sample = if reader.use_64_bit { 8 } else { 4 };
    for channel in 0..reader.channels.max(0) {
        let target = *buffers.add(channel as usize);
        if target.is_null() {
            continue;
        }
        std::ptr::write_bytes(target, 0, samples as usize * bytes_per_sample);
    }
    ARA_TRUE
}

unsafe extern "C" fn cb_destroy_audio_reader(_controller: AraHostRef, reader: AraHostRef) {
    if !reader.is_null() {
        drop(Box::from_raw(reader as *mut SilentAudioReader));
    }
}

unsafe extern "C" fn cb_get_archive_size(_controller: AraHostRef, _reader: AraHostRef) -> AraSize {
    0
}

unsafe extern "C" fn cb_read_bytes_from_archive(
    _controller: AraHostRef,
    _reader: AraHostRef,
    _position: AraSize,
    _length: AraSize,
    _buffer: *mut u8,
) -> AraBool {
    ARA_FALSE
}

unsafe extern "C" fn cb_write_bytes_to_archive(
    _controller: AraHostRef,
    _writer: AraHostRef,
    _position: AraSize,
    _length: AraSize,
    _buffer: *const u8,
) -> AraBool {
    ARA_FALSE
}

unsafe extern "C" fn cb_archiving_progress(_controller: AraHostRef, value: f32) {
    guard("archiving_progress", || {
        let value = format!("{value:.2}");
        diag::info("plugin.ara", "ara.archiving_progress", &[("value", &value)]);
    });
}

unsafe extern "C" fn cb_get_document_archive_id(
    controller: AraHostRef,
    _reader: AraHostRef,
) -> AraPersistentId {
    host_state(controller)
        .map(|state| state.archive_id.as_ptr())
        .unwrap_or(std::ptr::null())
}

unsafe extern "C" fn cb_is_mc_content_available(
    _controller: AraHostRef,
    _musical_context: AraHostRef,
    content_type: AraInt32,
) -> AraBool {
    if content_type == ARA_CONTENT_TYPE_TEMPO_ENTRIES
        || content_type == ARA_CONTENT_TYPE_BAR_SIGNATURES
    {
        ARA_TRUE
    } else {
        ARA_FALSE
    }
}

unsafe extern "C" fn cb_mc_content_grade(
    _controller: AraHostRef,
    _musical_context: AraHostRef,
    _content_type: AraInt32,
) -> AraInt32 {
    // kARAContentGradeAdjusted
    2
}

unsafe extern "C" fn cb_create_mc_content_reader(
    controller: AraHostRef,
    _musical_context: AraHostRef,
    content_type: AraInt32,
    _range: *const AraContentTimeRange,
) -> AraHostRef {
    let Some(state) = host_state(controller) else {
        return std::ptr::null_mut();
    };
    let tempo = state.tempo.lock().map(|t| *t).unwrap_or_default();
    let mut reader = HostContentReader {
        kind: content_type,
        tempo: Vec::new(),
        bars: Vec::new(),
        last: std::ptr::null(),
    };
    match content_type {
        ARA_CONTENT_TYPE_TEMPO_ENTRIES => {
            // 恒定速度：两个同步点即可覆盖整条时间轴。
            // Constant tempo: two sync points cover the whole timeline.
            reader.tempo.push(AraContentTempoEntry {
                time_position: 0.0,
                quarter_position: 0.0,
            });
            reader.tempo.push(AraContentTempoEntry {
                time_position: 60.0,
                quarter_position: tempo.bpm,
            });
        }
        ARA_CONTENT_TYPE_BAR_SIGNATURES => {
            reader.bars.push(AraContentBarSignature {
                numerator: tempo.numerator,
                denominator: tempo.denominator,
                position: 0.0,
            });
        }
        _ => return std::ptr::null_mut(),
    }
    Box::into_raw(Box::new(reader)) as AraHostRef
}

unsafe extern "C" fn cb_is_as_content_available(
    _controller: AraHostRef,
    _source: AraHostRef,
    _content_type: AraInt32,
) -> AraBool {
    ARA_FALSE
}

unsafe extern "C" fn cb_as_content_grade(
    _controller: AraHostRef,
    _source: AraHostRef,
    _content_type: AraInt32,
) -> AraInt32 {
    0
}

unsafe extern "C" fn cb_create_as_content_reader(
    _controller: AraHostRef,
    _source: AraHostRef,
    _content_type: AraInt32,
    _range: *const AraContentTimeRange,
) -> AraHostRef {
    std::ptr::null_mut()
}

unsafe extern "C" fn cb_content_reader_event_count(
    _controller: AraHostRef,
    reader: AraHostRef,
) -> AraInt32 {
    if reader.is_null() {
        return 0;
    }
    let reader = &*(reader as *const HostContentReader);
    match reader.kind {
        ARA_CONTENT_TYPE_TEMPO_ENTRIES => reader.tempo.len() as AraInt32,
        ARA_CONTENT_TYPE_BAR_SIGNATURES => reader.bars.len() as AraInt32,
        _ => 0,
    }
}

unsafe extern "C" fn cb_content_reader_data(
    _controller: AraHostRef,
    reader: AraHostRef,
    index: AraInt32,
) -> *const c_void {
    if reader.is_null() || index < 0 {
        return std::ptr::null();
    }
    let reader = &mut *(reader as *mut HostContentReader);
    let data = match reader.kind {
        ARA_CONTENT_TYPE_TEMPO_ENTRIES => reader
            .tempo
            .get(index as usize)
            .map(|entry| entry as *const _ as *const c_void),
        ARA_CONTENT_TYPE_BAR_SIGNATURES => reader
            .bars
            .get(index as usize)
            .map(|entry| entry as *const _ as *const c_void),
        _ => None,
    }
    .unwrap_or(std::ptr::null());
    reader.last = data;
    data
}

unsafe extern "C" fn cb_destroy_content_reader(_controller: AraHostRef, reader: AraHostRef) {
    if !reader.is_null() {
        drop(Box::from_raw(reader as *mut HostContentReader));
    }
}

unsafe extern "C" fn cb_analysis_progress(
    _controller: AraHostRef,
    _source: AraHostRef,
    state: AraInt32,
    value: f32,
) {
    guard("analysis_progress", || {
        let state = state.to_string();
        let value = format!("{value:.2}");
        diag::info(
            "plugin.ara",
            "ara.analysis_progress",
            &[("state", &state), ("value", &value)],
        );
    });
}

unsafe extern "C" fn cb_as_content_changed(
    controller: AraHostRef,
    _source: AraHostRef,
    _range: *const AraContentTimeRange,
    flags: AraInt32,
) {
    guard("as_content_changed", || {
        let flags = flags.to_string();
        if let Some(state) = host_state(controller) {
            state.content_changes.fetch_add(1, Ordering::Relaxed);
        }
        diag::info(
            "plugin.ara",
            "ara.audio_source_content_changed",
            &[("flags", &flags)],
        );
    });
}

unsafe extern "C" fn cb_am_content_changed(
    controller: AraHostRef,
    _modification: AraHostRef,
    _range: *const AraContentTimeRange,
    flags: AraInt32,
) {
    guard("am_content_changed", || {
        let flags = flags.to_string();
        if let Some(state) = host_state(controller) {
            state.content_changes.fetch_add(1, Ordering::Relaxed);
        }
        diag::info(
            "plugin.ara",
            "ara.audio_modification_content_changed",
            &[("flags", &flags)],
        );
    });
}

unsafe extern "C" fn cb_pr_content_changed(
    controller: AraHostRef,
    _region: AraHostRef,
    _range: *const AraContentTimeRange,
    flags: AraInt32,
) {
    guard("pr_content_changed", || {
        let flags = flags.to_string();
        if let Some(state) = host_state(controller) {
            state.content_changes.fetch_add(1, Ordering::Relaxed);
        }
        diag::info(
            "plugin.ara",
            "ara.playback_region_content_changed",
            &[("flags", &flags)],
        );
    });
}

unsafe extern "C" fn cb_document_data_changed(controller: AraHostRef) {
    guard("document_data_changed", || {
        if let Some(state) = host_state(controller) {
            state.content_changes.fetch_add(1, Ordering::Relaxed);
        }
        diag::info("plugin.ara", "ara.document_data_changed", &[]);
    });
}

unsafe extern "C" fn cb_request_start(controller: AraHostRef) {
    push_request(controller, PlaybackRequest::Start);
}

unsafe extern "C" fn cb_request_stop(controller: AraHostRef) {
    push_request(controller, PlaybackRequest::Stop);
}

unsafe extern "C" fn cb_request_seek(controller: AraHostRef, position: AraTimePosition) {
    push_request(controller, PlaybackRequest::Seek(position));
}

unsafe extern "C" fn cb_request_cycle_range(
    controller: AraHostRef,
    start: AraTimePosition,
    duration: AraTimePosition,
) {
    push_request(controller, PlaybackRequest::CycleRange(start, duration));
}

unsafe extern "C" fn cb_request_enable_cycle(controller: AraHostRef, enable: AraBool) {
    push_request(controller, PlaybackRequest::EnableCycle(enable != 0));
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：把插件的播放控制请求放入宿主队列并记录日志。
/// 输入：`controller` 宿主引用、`request` 请求。
/// 输出/副作用：追加到状态队列；记录诊断日志。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: plugin / ARA2 host
/// Purpose: Queue a plug-in playback request on the host and log it.
/// Inputs: `controller`, `request`.
/// Outputs / side effects: appends to the state queue and logs.
/// Complexity: O(1) time and space.
unsafe fn push_request(controller: AraHostRef, request: PlaybackRequest) {
    guard("playback_request", || {
        if let Some(state) = host_state(controller) {
            if let Ok(mut queue) = state.requests.lock() {
                queue.push(request);
            }
        }
        let text = format!("{request:?}");
        diag::info("plugin.ara", "ara.playback_request", &[("request", &text)]);
    });
}

impl AraSession {
    /// 所属模块：plugin / ARA2 宿主
    /// 功能：在插件实例上建立并绑定 ARA 文档。
    /// 输入：
    /// - `handles`：插件原始 VST3 指针。
    /// - `sample_rate`：工程采样率。
    /// - `bpm` / `time_sig`：初始速度与拍号。
    /// - `document_name`：工程名（用于文档属性）。
    ///
    /// 输出/副作用：创建 ARA 文档、音乐上下文与区域序列，并绑定插件实例；失败时返回中文
    /// 上下文错误且不留下半初始化状态。
    /// 复杂度：时间 O(插件初始化)，空间 O(文档模型)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Create and bind an ARA document on a plug-in instance.
    /// Inputs:
    /// - `handles`: raw VST3 pointers of the plug-in.
    /// - `sample_rate`: project sample rate.
    /// - `bpm` / `time_sig`: initial tempo and time signature.
    /// - `document_name`: project name for the document properties.
    /// Outputs / side effects: creates the ARA document, musical context and region sequence,
    /// then binds the plug-in instance. Failures return a Chinese message and leave nothing
    /// half-initialized.
    /// Complexity: O(plug-in initialization) time and O(document model) space.
    pub unsafe fn create(
        handles: RawPluginHandles,
        sample_rate: f64,
        bpm: f64,
        time_sig: (u8, u8),
        document_name: &str,
    ) -> Result<Self, String> {
        let factory = discover_factory(handles)?;
        let info = read_factory_info(factory);

        // 选择双方都支持的 API 世代：优先 2.0 Final，否则用插件支持的最高世代。
        // Pick a generation both sides support: prefer 2.0 Final, else the plug-in's highest.
        let generation = if info.lowest_generation <= ARA_API_GENERATION_2_0_FINAL
            && ARA_API_GENERATION_2_0_FINAL <= info.highest_generation
        {
            ARA_API_GENERATION_2_0_FINAL
        } else {
            info.highest_generation
        };
        if generation < 2 {
            return Err(format!(
                "插件只支持 ARA API 世代 {}..{}，无法作为 ARA 宿主使用",
                info.lowest_generation, info.highest_generation
            ));
        }

        let strings = Box::new(SessionStrings {
            document_name: c_string(document_name, "SurfingStudio"),
            musical_context_name: c_string("SurfingStudio", "Context"),
            region_sequence_name: c_string("SurfingStudio", "Track"),
            audio_source_name: c_string("SurfingStudio", "Source"),
            audio_modification_name: c_string("SurfingStudio", "Modification"),
            playback_region_name: c_string("SurfingStudio", "Region"),
            source_persistent_id: CString::new("com.surfingstudio.ara.source").unwrap(),
            modification_persistent_id: CString::new("com.surfingstudio.ara.modification").unwrap(),
        });

        let archive_id = CString::new(info.document_archive_id.clone())
            .unwrap_or_else(|_| CString::new("com.surfingstudio.ara.archive").unwrap());
        let tempo = TempoMap {
            bpm: if bpm.is_finite() && bpm > 0.0 {
                bpm
            } else {
                120.0
            },
            numerator: time_sig.0.max(1) as i32,
            denominator: time_sig.1.max(1) as i32,
        };

        let mut host = Box::new(HostState::new(tempo, archive_id));
        let host_ptr = (&mut *host) as *mut HostState as AraHostRef;
        host.bind_refs(host_ptr);

        let configuration = AraInterfaceConfiguration {
            struct_size: std::mem::size_of::<AraInterfaceConfiguration>(),
            desired_api_generation: generation,
            assert_function_address: host.assert_function.as_mut() as *mut AraAssertFunction
                as *mut c_void,
        };
        if let Some(initialize) = (*factory).initialize_ara_with_configuration {
            initialize(&configuration);
        }

        let host_instance = Box::new(AraDocumentControllerHostInstance {
            struct_size: std::mem::size_of::<AraDocumentControllerHostInstance>(),
            audio_access_controller_host_ref: host.audio_access_host_ref,
            audio_access_controller_interface: &host.audio_access,
            archiving_controller_host_ref: host.archiving_host_ref,
            archiving_controller_interface: &host.archiving,
            content_access_controller_host_ref: host.content_access_host_ref,
            content_access_controller_interface: &host.content_access,
            model_update_controller_host_ref: host.model_update_host_ref,
            model_update_controller_interface: &host.model_update,
            playback_controller_host_ref: host.playback_host_ref,
            playback_controller_interface: &host.playback,
        });

        let document_properties = AraDocumentProperties {
            struct_size: std::mem::size_of::<AraDocumentProperties>(),
            name: strings.document_name.as_ptr(),
        };
        let Some(create_document) = (*factory).create_document_controller_with_document else {
            if let Some(uninitialize) = (*factory).uninitialize_ara {
                uninitialize();
            }
            return Err("插件未提供 createDocumentControllerWithDocument".to_string());
        };
        let instance = create_document(&*host_instance, &document_properties);
        if instance.is_null() {
            if let Some(uninitialize) = (*factory).uninitialize_ara {
                uninitialize();
            }
            return Err("插件拒绝创建 ARA 文档控制器".to_string());
        }
        let instance_ref = &*instance;
        let controller_ref = instance_ref.document_controller_ref;
        let controller = instance_ref.document_controller_interface;
        if controller_ref.is_null() || controller.is_null() {
            if let Some(uninitialize) = (*factory).uninitialize_ara {
                uninitialize();
            }
            return Err("插件返回了空的文档控制器".to_string());
        }

        let mut session = Self {
            info: info.clone(),
            factory,
            controller_ref,
            controller,
            host,
            _host_instance: host_instance,
            objects: ModelObjects::default(),
            markers: Vec::new(),
            strings,
            timeline: None,
            sample_rate,
            extension: std::ptr::null(),
            bound_roles: 0,
            controller_thread: std::thread::current().id(),
        };

        // 所有模型图编辑必须包在 beginEditing()/endEditing() 之间。
        // Every model graph edit must be guarded by beginEditing()/endEditing().
        let dc = &*controller;
        if let Some(begin) = dc.begin_editing {
            begin(controller_ref);
        }
        if let Some(update) = dc.update_document_properties {
            update(controller_ref, &document_properties);
        }
        let setup = session
            .create_musical_context()
            .and_then(|_| session.create_region_sequence());
        if let Some(end) = dc.end_editing {
            end(controller_ref);
        }
        if let Some(error) = setup.err() {
            if let Some(notify) = dc.notify_model_updates {
                notify(controller_ref);
            }
            return Err(error);
        }
        if let Some(notify) = dc.notify_model_updates {
            notify(controller_ref);
        }

        let (extension, roles) = bind_plugin(handles.processor, controller_ref)?;
        session.extension = extension;
        session.bound_roles = roles;

        diag::info(
            "plugin.ara",
            "ara.session_created",
            &[
                ("plugin", &info.plugin_name),
                ("factory", &info.factory_id),
                ("generation", &generation.to_string()),
                ("roles", &roles.to_string()),
                (
                    "content_types",
                    &info
                        .analyzeable_content_types
                        .iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<_>>()
                        .join(","),
                ),
            ],
        );
        Ok(session)
    }

    /// 所属模块：plugin / ARA2 宿主
    /// 功能：返回插件工厂信息。
    /// 输入：`self`。
    /// 输出/副作用：返回只读信息。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Return the plug-in factory metadata.
    /// Inputs: `self`.
    /// Outputs / side effects: read-only metadata.
    /// Complexity: O(1) time and space.
    pub fn info(&self) -> &AraFactoryInfo {
        &self.info
    }

    /// 所属模块：plugin / ARA2 宿主
    /// 功能：返回已绑定的角色位。
    /// 输入：`self`。
    /// 输出/副作用：返回角色位掩码。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Return the bound role bits.
    /// Inputs: `self`.
    /// Outputs / side effects: the role mask.
    /// Complexity: O(1) time and space.
    pub fn bound_roles(&self) -> i32 {
        self.bound_roles
    }

    /// 所属模块：plugin / ARA2 宿主
    /// 功能：返回插件累计上报的内容变更次数。
    /// 输入：`self`。
    /// 输出/副作用：返回计数。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Return how many content changes the plug-in reported.
    /// Inputs: `self`.
    /// Outputs / side effects: the counter.
    /// Complexity: O(1) time and space.
    pub fn content_change_count(&self) -> u64 {
        self.host.content_changes.load(Ordering::Relaxed)
    }

    /// 所属模块：plugin / ARA2 宿主
    /// 功能：取走插件发出的播放控制请求。
    /// 输入：`self`。
    /// 输出/副作用：清空并返回队列；可在任意线程调用。
    /// 复杂度：时间 O(请求数)，空间 O(请求数)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Drain playback requests issued by the plug-in.
    /// Inputs: `self`.
    /// Outputs / side effects: clears and returns the queue; callable from any thread.
    /// Complexity: O(requests) time and space.
    pub fn take_playback_requests(&self) -> Vec<PlaybackRequest> {
        self.host
            .requests
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }

    /// 所属模块：plugin / ARA2 宿主
    /// 功能：更新音乐上下文的速度与拍号。
    /// 输入：`bpm` 速度、`time_sig` 拍号。
    /// 输出/副作用：修改宿主内容表并通知插件重新读取内容。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Update the musical context's tempo and time signature.
    /// Inputs: `bpm`, `time_sig`.
    /// Outputs / side effects: updates the host content table and notifies the plug-in.
    /// Complexity: O(1) time and space.
    pub fn set_tempo(&mut self, bpm: f64, time_sig: (u8, u8)) -> Result<(), String> {
        self.ensure_controller_thread("set_tempo")?;
        let tempo = TempoMap {
            bpm: if bpm.is_finite() && bpm > 0.0 {
                bpm
            } else {
                120.0
            },
            numerator: time_sig.0.max(1) as i32,
            denominator: time_sig.1.max(1) as i32,
        };
        let changed = self
            .host
            .tempo
            .lock()
            .map(|mut current| {
                let changed = (*current).bpm != tempo.bpm
                    || current.numerator != tempo.numerator
                    || current.denominator != tempo.denominator;
                *current = tempo;
                changed
            })
            .unwrap_or(false);
        if !changed || self.objects.musical_context.is_null() {
            return Ok(());
        }
        unsafe {
            let dc = &*self.controller;
            if let Some(begin) = dc.begin_editing {
                begin(self.controller_ref);
            }
            if let Some(update) = dc.update_musical_context_content {
                update(
                    self.controller_ref,
                    self.objects.musical_context,
                    std::ptr::null(),
                    0,
                );
            }
            if let Some(end) = dc.end_editing {
                end(self.controller_ref);
            }
            if let Some(notify) = dc.notify_model_updates {
                notify(self.controller_ref);
            }
        }
        Ok(())
    }

    /// 所属模块：plugin / ARA2 宿主
    /// 功能：把播放区域更新到给定的工程时间范围（秒）。
    /// 输入：`start` 起始秒、`duration` 时长秒。
    /// 输出/副作用：首次调用创建静音音频源、音频修改与播放区域；之后更新区域属性。
    /// 复杂度：时间 O(模型对象数)，空间 O(1)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Move the playback region to the given project time range (seconds).
    /// Inputs: `start`, `duration`.
    /// Outputs / side effects: the first call creates a silent audio source, audio modification
    /// and playback region; later calls update the region properties.
    /// Complexity: O(model objects) time and O(1) space.
    pub fn set_timeline(&mut self, start: f64, duration: f64) -> Result<(), String> {
        self.ensure_controller_thread("set_timeline")?;
        if !start.is_finite() || !duration.is_finite() || duration <= 0.0 {
            return Err("ARA 播放区域的时间范围无效".to_string());
        }
        if self.timeline.is_some_and(|(old_start, old_duration)| {
            (old_start - start).abs() < 1e-6 && (old_duration - duration).abs() < 1e-6
        }) {
            return Ok(());
        }
        unsafe {
            let dc = &*self.controller;
            if let Some(begin) = dc.begin_editing {
                begin(self.controller_ref);
            }
            if self.objects.audio_source.is_null() {
                self.create_audio_graph(start, duration)?;
            } else {
                self.update_audio_graph(start, duration)?;
            }
            if let Some(end) = dc.end_editing {
                end(self.controller_ref);
            }
            if let Some(notify) = dc.notify_model_updates {
                notify(self.controller_ref);
            }
        }
        self.timeline = Some((start, duration));
        Ok(())
    }

    /// 所属模块：plugin / ARA2 宿主
    /// 功能：创建音乐上下文并声明恒定速度内容。
    /// 输入：`self`。
    /// 输出/副作用：写入 `objects.musical_context`；失败时返回错误。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Create the musical context and announce constant-tempo content.
    /// Inputs: `self`.
    /// Outputs / side effects: stores `objects.musical_context`; errors on failure.
    /// Complexity: O(1) time and space.
    unsafe fn create_musical_context(&mut self) -> Result<(), String> {
        let dc = &*self.controller;
        let Some(create) = dc.create_musical_context else {
            return Err("插件未提供 createMusicalContext".to_string());
        };
        let marker = Box::new(0u8);
        let host_ref = (&*marker) as *const u8 as AraHostRef;
        self.markers.push(marker);
        let properties = AraMusicalContextProperties {
            struct_size: std::mem::size_of::<AraMusicalContextProperties>(),
            name: self.strings.musical_context_name.as_ptr(),
            order_index: 0,
            color: std::ptr::null(),
        };
        let context = create(self.controller_ref, host_ref, &properties);
        if context.is_null() {
            return Err("插件拒绝创建音乐上下文".to_string());
        }
        self.objects.musical_context = context;
        if let Some(update) = dc.update_musical_context_content {
            update(self.controller_ref, context, std::ptr::null(), 0);
        }
        Ok(())
    }

    /// 所属模块：plugin / ARA2 宿主
    /// 功能：创建区域序列（ARA 2 的“轨道”分组）。
    /// 输入：`self`。
    /// 输出/副作用：写入 `objects.region_sequence`；失败时返回错误。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Create the region sequence (ARA 2's "track" grouping).
    /// Inputs: `self`.
    /// Outputs / side effects: stores `objects.region_sequence`; errors on failure.
    /// Complexity: O(1) time and space.
    unsafe fn create_region_sequence(&mut self) -> Result<(), String> {
        let dc = &*self.controller;
        let Some(create) = dc.create_region_sequence else {
            // ARA 1 插件没有区域序列；播放区域里的音乐上下文引用即可。
            // ARA 1 plug-ins have no region sequence; the deprecated context reference suffices.
            return Ok(());
        };
        let marker = Box::new(0u8);
        let host_ref = (&*marker) as *const u8 as AraHostRef;
        self.markers.push(marker);
        let properties = AraRegionSequenceProperties {
            struct_size: std::mem::size_of::<AraRegionSequenceProperties>(),
            name: self.strings.region_sequence_name.as_ptr(),
            order_index: 0,
            musical_context_ref: self.objects.musical_context,
            color: std::ptr::null(),
        };
        let sequence = create(self.controller_ref, host_ref, &properties);
        if sequence.is_null() {
            return Err("插件拒绝创建区域序列".to_string());
        }
        self.objects.region_sequence = sequence;
        Ok(())
    }

    /// 所属模块：plugin / ARA2 宿主
    /// 功能：创建静音音频源、音频修改与播放区域。
    /// 输入：`start` / `duration` 播放时间（秒）。
    /// 输出/副作用：写入 `objects`，并把播放区域登记到插件渲染器。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Create the silent audio source, audio modification and playback region.
    /// Inputs: `start` / `duration` in playback seconds.
    /// Outputs / side effects: fills `objects` and registers the region on the plug-in renderers.
    /// Complexity: O(1) time and space.
    unsafe fn create_audio_graph(&mut self, start: f64, duration: f64) -> Result<(), String> {
        let dc = &*self.controller;
        let Some(create_source) = dc.create_audio_source else {
            return Err("插件未提供 createAudioSource".to_string());
        };
        let source_marker = Box::new(0u8);
        let source_host_ref = (&*source_marker) as *const u8 as AraHostRef;
        self.markers.push(source_marker);
        let source_properties = AraAudioSourceProperties {
            struct_size: std::mem::size_of::<AraAudioSourceProperties>(),
            name: self.strings.audio_source_name.as_ptr(),
            persistent_id: self.strings.source_persistent_id.as_ptr(),
            sample_count: (duration * self.sample_rate).round() as AraSampleCount,
            sample_rate: self.sample_rate,
            channel_count: SILENT_SOURCE_CHANNELS,
            merits64_bit_samples: ARA_FALSE,
            channel_arrangement_data_type: 0,
            channel_arrangement: std::ptr::null(),
        };
        let source = create_source(self.controller_ref, source_host_ref, &source_properties);
        if source.is_null() {
            return Err("插件拒绝创建音频源".to_string());
        }
        self.objects.audio_source = source;

        let Some(create_modification) = dc.create_audio_modification else {
            return Err("插件未提供 createAudioModification".to_string());
        };
        let modification_marker = Box::new(0u8);
        let modification_host_ref = (&*modification_marker) as *const u8 as AraHostRef;
        self.markers.push(modification_marker);
        let modification_properties = AraAudioModificationProperties {
            struct_size: std::mem::size_of::<AraAudioModificationProperties>(),
            name: self.strings.audio_modification_name.as_ptr(),
            persistent_id: self.strings.modification_persistent_id.as_ptr(),
        };
        let modification = create_modification(
            self.controller_ref,
            source,
            modification_host_ref,
            &modification_properties,
        );
        if modification.is_null() {
            return Err("插件拒绝创建音频修改".to_string());
        }
        self.objects.audio_modification = modification;

        self.create_playback_region(start, duration)?;

        if let Some(enable) = dc.enable_audio_source_samples_access {
            enable(self.controller_ref, source, ARA_TRUE);
        }
        Ok(())
    }

    /// 所属模块：plugin / ARA2 宿主
    /// 功能：在现有音频修改上创建播放区域。
    /// 输入：`start` / `duration` 播放时间（秒）。
    /// 输出/副作用：写入 `objects.playback_region` 并登记到渲染器接口。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Create a playback region on the existing audio modification.
    /// Inputs: `start` / `duration` in playback seconds.
    /// Outputs / side effects: stores `objects.playback_region` and registers it on the renderers.
    /// Complexity: O(1) time and space.
    unsafe fn create_playback_region(&mut self, start: f64, duration: f64) -> Result<(), String> {
        let dc = &*self.controller;
        let Some(create) = dc.create_playback_region else {
            return Err("插件未提供 createPlaybackRegion".to_string());
        };
        let marker = Box::new(0u8);
        let host_ref = (&*marker) as *const u8 as AraHostRef;
        self.markers.push(marker);
        let properties = AraPlaybackRegionProperties {
            struct_size: std::mem::size_of::<AraPlaybackRegionProperties>(),
            transformation_flags: 0,
            start_in_modification_time: 0.0,
            duration_in_modification_time: duration,
            start_in_playback_time: start,
            duration_in_playback_time: duration,
            musical_context_ref: self.objects.musical_context,
            region_sequence_ref: self.objects.region_sequence,
            name: self.strings.playback_region_name.as_ptr(),
            color: std::ptr::null(),
        };
        let region = create(
            self.controller_ref,
            self.objects.audio_modification,
            host_ref,
            &properties,
        );
        if region.is_null() {
            return Err("插件拒绝创建播放区域".to_string());
        }
        self.objects.playback_region = region;
        self.register_region_on_renderers(region, true);
        Ok(())
    }

    /// 所属模块：plugin / ARA2 宿主
    /// 功能：更新时间线上已有对象的属性（音频源长度、区域位置）。
    /// 输入：`start` / `duration` 播放时间（秒）。
    /// 输出/副作用：调用插件的 update* 接口；不新建对象。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Update the existing objects' properties (source length, region position).
    /// Inputs: `start` / `duration` in playback seconds.
    /// Outputs / side effects: calls the plug-in's update* functions; creates nothing.
    /// Complexity: O(1) time and space.
    unsafe fn update_audio_graph(&mut self, start: f64, duration: f64) -> Result<(), String> {
        let dc = &*self.controller;
        let source_properties = AraAudioSourceProperties {
            struct_size: std::mem::size_of::<AraAudioSourceProperties>(),
            name: self.strings.audio_source_name.as_ptr(),
            persistent_id: self.strings.source_persistent_id.as_ptr(),
            sample_count: (duration * self.sample_rate).round() as AraSampleCount,
            sample_rate: self.sample_rate,
            channel_count: SILENT_SOURCE_CHANNELS,
            merits64_bit_samples: ARA_FALSE,
            channel_arrangement_data_type: 0,
            channel_arrangement: std::ptr::null(),
        };
        if let Some(update) = dc.update_audio_source_properties {
            update(
                self.controller_ref,
                self.objects.audio_source,
                &source_properties,
            );
        }
        let region_properties = AraPlaybackRegionProperties {
            struct_size: std::mem::size_of::<AraPlaybackRegionProperties>(),
            transformation_flags: 0,
            start_in_modification_time: 0.0,
            duration_in_modification_time: duration,
            start_in_playback_time: start,
            duration_in_playback_time: duration,
            musical_context_ref: self.objects.musical_context,
            region_sequence_ref: self.objects.region_sequence,
            name: self.strings.playback_region_name.as_ptr(),
            color: std::ptr::null(),
        };
        if let Some(update) = dc.update_playback_region_properties {
            update(
                self.controller_ref,
                self.objects.playback_region,
                &region_properties,
            );
        }
        Ok(())
    }

    /// 所属模块：plugin / ARA2 宿主
    /// 功能：把播放区域加入插件的播放/编辑器渲染器。
    /// 输入：`region` 播放区域、`add` 为真时添加、为假时移除。
    /// 输出/副作用：调用插件扩展接口；扩展缺失时忽略。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Add or remove a playback region on the plug-in's playback/editor renderers.
    /// Inputs: `region`, `add`.
    /// Outputs / side effects: calls the plug-in extension; missing extensions are ignored.
    /// Complexity: O(1) time and space.
    unsafe fn register_region_on_renderers(&self, region: AraRef, add: bool) {
        if self.extension.is_null() || region.is_null() {
            return;
        }
        let extension = &*self.extension;
        let playback = extension.playback_renderer_interface;
        if !playback.is_null() && !extension.playback_renderer_ref.is_null() {
            let callback = if add {
                (*playback).add_playback_region
            } else {
                (*playback).remove_playback_region
            };
            if let Some(callback) = callback {
                callback(extension.playback_renderer_ref, region);
            }
        }
        let editor = extension.editor_renderer_interface;
        if !editor.is_null() && !extension.editor_renderer_ref.is_null() {
            let callback = if add {
                (*editor).add_playback_region
            } else {
                (*editor).remove_playback_region
            };
            if let Some(callback) = callback {
                callback(extension.editor_renderer_ref, region);
            }
        }
    }

    /// 所属模块：plugin / ARA2 宿主
    /// 功能：确认调用发生在创建文档控制器的线程上。
    /// 输入：`operation` 操作名（用于错误信息）。
    /// 输出/副作用：线程不符时返回错误；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Ensure the call happens on the document controller's thread.
    /// Inputs: `operation` — used in the error message.
    /// Outputs / side effects: an error on a thread mismatch; no state change.
    /// Complexity: O(1) time and space.
    fn ensure_controller_thread(&self, operation: &str) -> Result<(), String> {
        if std::thread::current().id() == self.controller_thread {
            Ok(())
        } else {
            Err(format!("ARA {operation} 必须在创建文档的线程调用"))
        }
    }
}

impl Drop for AraSession {
    fn drop(&mut self) {
        // 销毁必须回到创建线程；其它线程只记录日志，避免插件在自己的线程模型里崩溃。
        // Destruction belongs to the creating thread; elsewhere just log, so the plug-in's thread
        // model is never violated.
        if std::thread::current().id() != self.controller_thread {
            diag::warn(
                "plugin.ara",
                "ara.session_drop_off_thread",
                &[("plugin", &self.info.plugin_name)],
            );
            return;
        }
        unsafe {
            self.register_region_on_renderers(self.objects.playback_region, false);
            let dc = &*self.controller;
            if let Some(destroy) = dc.destroy_playback_region {
                if !self.objects.playback_region.is_null() {
                    destroy(self.controller_ref, self.objects.playback_region);
                }
            }
            if let Some(destroy) = dc.destroy_audio_modification {
                if !self.objects.audio_modification.is_null() {
                    destroy(self.controller_ref, self.objects.audio_modification);
                }
            }
            if let Some(destroy) = dc.destroy_audio_source {
                if !self.objects.audio_source.is_null() {
                    destroy(self.controller_ref, self.objects.audio_source);
                }
            }
            if let Some(destroy) = dc.destroy_region_sequence {
                if !self.objects.region_sequence.is_null() {
                    destroy(self.controller_ref, self.objects.region_sequence);
                }
            }
            if let Some(destroy) = dc.destroy_musical_context {
                if !self.objects.musical_context.is_null() {
                    destroy(self.controller_ref, self.objects.musical_context);
                }
            }
            if let Some(destroy) = dc.destroy_document_controller {
                destroy(self.controller_ref);
            }
            if let Some(uninitialize) = (*self.factory).uninitialize_ara {
                uninitialize();
            }
        }
        diag::info(
            "plugin.ara",
            "ara.session_destroyed",
            &[("plugin", &self.info.plugin_name)],
        );
    }
}

impl HostState {
    /// 所属模块：plugin / ARA2 宿主
    /// 功能：构造宿主控制器状态（接口表 + 数据）。
    /// 输入：`tempo` 初始速度、`archive_id` 文档归档 ID、`log_name` 插件名。
    /// 输出/副作用：返回初始化的状态；不调用插件。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Build the host controller state (interface tables + data).
    /// Inputs: `tempo`, `archive_id`.
    /// Outputs / side effects: an initialized state; no plug-in calls.
    /// Complexity: O(1) time and space.
    fn new(tempo: TempoMap, archive_id: CString) -> Self {
        Self {
            audio_access: AraAudioAccessControllerInterface {
                struct_size: std::mem::size_of::<AraAudioAccessControllerInterface>(),
                create_audio_reader_for_source: Some(cb_create_audio_reader),
                read_audio_samples: Some(cb_read_audio_samples),
                destroy_audio_reader: Some(cb_destroy_audio_reader),
            },
            archiving: AraArchivingControllerInterface {
                struct_size: std::mem::size_of::<AraArchivingControllerInterface>(),
                get_archive_size: Some(cb_get_archive_size),
                read_bytes_from_archive: Some(cb_read_bytes_from_archive),
                write_bytes_to_archive: Some(cb_write_bytes_to_archive),
                notify_document_archiving_progress: Some(cb_archiving_progress),
                notify_document_unarchiving_progress: Some(cb_archiving_progress),
                get_document_archive_id: Some(cb_get_document_archive_id),
            },
            content_access: AraContentAccessControllerInterface {
                struct_size: std::mem::size_of::<AraContentAccessControllerInterface>(),
                is_musical_context_content_available: Some(cb_is_mc_content_available),
                get_musical_context_content_grade: Some(cb_mc_content_grade),
                create_musical_context_content_reader: Some(cb_create_mc_content_reader),
                is_audio_source_content_available: Some(cb_is_as_content_available),
                get_audio_source_content_grade: Some(cb_as_content_grade),
                create_audio_source_content_reader: Some(cb_create_as_content_reader),
                get_content_reader_event_count: Some(cb_content_reader_event_count),
                get_content_reader_data_for_event: Some(cb_content_reader_data),
                destroy_content_reader: Some(cb_destroy_content_reader),
            },
            model_update: AraModelUpdateControllerInterface {
                struct_size: std::mem::size_of::<AraModelUpdateControllerInterface>(),
                notify_audio_source_analysis_progress: Some(cb_analysis_progress),
                notify_audio_source_content_changed: Some(cb_as_content_changed),
                notify_audio_modification_content_changed: Some(cb_am_content_changed),
                notify_playback_region_content_changed: Some(cb_pr_content_changed),
                notify_document_data_changed: Some(cb_document_data_changed),
            },
            playback: AraPlaybackControllerInterface {
                struct_size: std::mem::size_of::<AraPlaybackControllerInterface>(),
                request_start_playback: Some(cb_request_start),
                request_stop_playback: Some(cb_request_stop),
                request_set_playback_position: Some(cb_request_seek),
                request_set_cycle_range: Some(cb_request_cycle_range),
                request_enable_cycle: Some(cb_request_enable_cycle),
            },
            audio_access_host_ref: std::ptr::null_mut(),
            archiving_host_ref: std::ptr::null_mut(),
            content_access_host_ref: std::ptr::null_mut(),
            model_update_host_ref: std::ptr::null_mut(),
            playback_host_ref: std::ptr::null_mut(),
            tempo: Mutex::new(tempo),
            archive_id,
            assert_function: Box::new(Some(ara_assert)),
            requests: Mutex::new(Vec::new()),
            content_changes: AtomicU64::new(0),
        }
    }

    /// 所属模块：plugin / ARA2 宿主
    /// 功能：把所有宿主引用指向状态自身的稳定地址。
    /// 输入：`pointer` 状态对象的堆地址。
    /// 输出/副作用：写入五个 hostRef 字段。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / ARA2 host
    /// Purpose: Point every host reference at the state's own stable address.
    /// Inputs: `pointer` — heap address of the state.
    /// Outputs / side effects: fills the five hostRef fields.
    /// Complexity: O(1) time and space.
    fn bind_refs(&mut self, pointer: AraHostRef) {
        self.audio_access_host_ref = pointer;
        self.archiving_host_ref = pointer;
        self.content_access_host_ref = pointer;
        self.model_update_host_ref = pointer;
        self.playback_host_ref = pointer;
    }
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：把插件实例绑定到文档控制器（优先使用 ARA 2 的角色绑定）。
/// 输入：`processor` 处理器指针、`controller_ref` 文档控制器引用。
/// 输出/副作用：返回扩展实例指针与已绑定角色位；失败时返回错误。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: plugin / ARA2 host
/// Purpose: Bind the plug-in instance to a document controller (role-aware ARA 2 preferred).
/// Inputs: `processor`, `controller_ref`.
/// Outputs / side effects: the extension instance pointer and bound role bits; errors on failure.
/// Complexity: O(1) time and space.
unsafe fn bind_plugin(
    processor: *mut c_void,
    controller_ref: AraRef,
) -> Result<(*const AraPlugInExtensionInstance, i32), String> {
    // 优先 ARA 2 的角色绑定：插件需要显式知道宿主支持哪些角色。
    // Prefer ARA 2 role-aware binding: the plug-in needs to know which roles the host supports.
    if let Some(entry) = query_interface(processor, &IID_ARA_PLUGIN_ENTRY_POINT2) {
        let vtbl = *(entry as *mut *const IPlugInEntryPoint2Vtbl);
        let extension = ((*vtbl).bind_to_document_controller_with_roles)(
            entry,
            controller_ref,
            ARA_ROLE_ALL,
            ARA_ROLE_ALL,
        );
        release(entry);
        if !extension.is_null() {
            return Ok((extension, ARA_ROLE_ALL));
        }
    }
    // 回退到 ARA 1 的绑定调用（等价于全部角色）。
    // Fall back to the ARA 1 binding call (equivalent to all roles).
    if let Some(entry) = query_interface(processor, &IID_ARA_PLUGIN_ENTRY_POINT) {
        let vtbl = *(entry as *mut *const IPlugInEntryPointVtbl);
        let extension = ((*vtbl).bind_to_document_controller)(entry, controller_ref);
        release(entry);
        if !extension.is_null() {
            return Ok((extension, ARA_ROLE_ALL));
        }
    }
    Err("插件实例未接受 ARA 绑定".to_string())
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：把可选的用户字符串转成 C 字符串（含 NUL 的输入被替换）。
/// 输入：`text` 原字符串、`fallback` 回退值。
/// 输出/副作用：返回 C 字符串。
/// 复杂度：时间 O(长度)，空间 O(长度)。
///
/// Module: plugin / ARA2 host
/// Purpose: Convert an optional user string into a C string (NULs replaced).
/// Inputs: `text`, `fallback`.
/// Outputs / side effects: a C string.
/// Complexity: O(length) time and space.
fn c_string(text: &str, fallback: &str) -> CString {
    let cleaned = text.replace('\0', " ");
    CString::new(cleaned)
        .or_else(|_| CString::new(fallback))
        .unwrap_or_else(|_| CString::new("SurfingStudio").unwrap())
}

/// 所属模块：plugin / ARA2 宿主
/// 功能：ARA 断言回调：只记录日志，不终止进程。
/// 输入：`category` 断言类别、`argument` 相关参数、`text` 断言文本。
/// 输出/副作用：写入诊断日志。
/// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
///
/// Module: plugin / ARA2 host
/// Purpose: ARA assert callback: log only, never abort.
/// Inputs: `category`, `argument`, `text`.
/// Outputs / side effects: writes a diagnostic entry.
/// Complexity: O(text length) time and space.
unsafe extern "C" fn ara_assert(
    category: AraAssertCategory,
    _argument: *const c_void,
    text: *const c_char,
) {
    let category = category.to_string();
    let text = read_c_string(text);
    diag::warn(
        "plugin.ara",
        "ara.assert",
        &[("category", &category), ("text", &text)],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ara_marker_scan_is_case_sensitive_to_the_real_binary() {
        // 目录中没有任何文件时不应命中。
        // An empty directory must not match.
        let dir = std::env::temp_dir().join("surfing-studio-ara-marker-empty");
        let _ = std::fs::create_dir_all(&dir);
        assert!(!declares_ara(&dir));
    }

    #[test]
    fn c_array_conversion_trims_trailing_nuls() {
        let mut raw = [0 as c_char; 8];
        for (index, byte) in b"Ab\0".iter().enumerate() {
            raw[index] = *byte as c_char;
        }
        assert_eq!(c_array_to_string(&raw), "Ab");
    }

    /// 诊断用：在真实插件上验证 ARA 工厂发现。
    /// Diagnostic: verify ARA factory discovery on a real plug-in.
    ///
    /// 注意：`SURFING_ARA_HOST=1` 时 VOCALOID Bridge 会在自身的角色绑定代码里崩溃
    /// （访问冲突），因此这里只验证发现与元数据。
    /// Note: with `SURFING_ARA_HOST=1`, VOCALOID Bridge crashes inside its own role-binding code
    /// (access violation), so this test only verifies discovery and metadata.
    #[test]
    #[ignore = "requires a locally installed VOCALOIDBridge.vst3"]
    fn probes_real_vocaloid_ara_factory() {
        let path = PathBuf::from(r"C:\Program Files\Common Files\VST3\VOCALOIDBridge.vst3");
        if !path.exists() {
            eprintln!("VOCALOIDBridge.vst3 未安装，跳过");
            return;
        }
        assert!(declares_ara(&path), "binary should declare ARA support");
        let mut host = vst3_host::Vst3Host::builder()
            .sample_rate(48_000.0)
            .block_size(512)
            .input_channels(2)
            .output_channels(2)
            .build()
            .expect("build host");
        let plugin = host.load_plugin(&path).expect("load VOCALOIDBridge");
        let handles = plugin.raw_handles().expect("raw handles");
        let info = unsafe { probe_factory(handles) }.expect("ARA factory");
        eprintln!("ARA factory: {info:?}");
        assert_eq!(info.plugin_name, "VOCALOID Bridge");
        assert_eq!(info.factory_id, "com.yamaha.vocaloidbridge.arafactory");
    }
}
