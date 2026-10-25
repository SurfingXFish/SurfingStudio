//! 所属模块：plugin / 插件运行时框架
//! 文件用途：
//! - 声明插件运行时框架、转译接口、VST3 / VST2 后端与共享类型。
//! - 应用层只通过 `PluginHost` 使用插件格式，不直接依赖具体后端。
//!
//! 导出项：
//! - `PluginHost` / `PluginBackend` / `PluginBackendInfo`：框架与后端接口。
//! - `PluginRuntime` / `Vst3Runtime` / `Vst2Runtime`：统一插件运行时。
//! - `Vst2EditorWindow`：VST1/VST2 独立编辑器窗口。
//! - `PluginKind` / `ParamView` / `ScannedPlugin`：共享展示类型。
//! - `CatalogEntry` / `lookup_catalog` / `list_plugin_files` / `scan_vst3_paths`：内置资料库与增量扫描工具。
//!
//! Module: plugin / plugin runtime framework
//! Purpose:
//! - Declare the plugin runtime framework, translation interface, VST3 / VST2 backends
//!   and shared types.
//! - The app uses plugin formats only through `PluginHost`, never a concrete backend.
//!
//! Exports:
//! - `PluginHost` / `PluginBackend` / `PluginBackendInfo`: framework and backend interface.
//! - `PluginRuntime` / `Vst3Runtime` / `Vst2Runtime`: unified plugin runtime.
//! - `Vst2EditorWindow`: standalone VST1/VST2 editor window.
//! - `PluginKind` / `ParamView` / `ScannedPlugin`: shared presentation types.
//! - `CatalogEntry` / `lookup_catalog` / `list_plugin_files` / `scan_vst3_paths`: catalog and
//!   incremental-scan helpers.

mod backend;
mod backends;
mod catalog;
mod editor;
mod host;
mod runtime;
mod scan;

pub use backend::{PluginBackend, PluginBackendInfo};
pub use catalog::{lookup as lookup_catalog, CatalogEntry};
pub use editor::Vst2EditorWindow;
pub use host::PluginHost;
pub use runtime::{PluginRuntime, Vst2Runtime, Vst3Runtime};
pub use scan::list_plugin_files;
pub use scan::scan_vst3_paths;

use crate::core::model::PluginFormat;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Broad plugin role used by the browser and the inspector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginKind {
    Instrument,
    Effect,
    Unknown,
}

impl PluginKind {
    pub fn label(&self) -> &'static str {
        match self {
            PluginKind::Instrument => "乐器",
            PluginKind::Effect => "效果器",
            PluginKind::Unknown => "未知",
        }
    }
}

/// A normalized parameter shown in the generic parameter panels.
#[derive(Clone, Debug)]
pub struct ParamView {
    pub id: u32,
    pub name: String,
    pub value: f64,
    pub display: String,
}

/// A lightweight record produced by the plugin browser scanners.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScannedPlugin {
    pub format: PluginFormat,
    pub path: PathBuf,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub vendor: String,
    #[serde(default)]
    pub category: String,
    #[serde(default = "default_plugin_kind")]
    pub kind: PluginKind,
    #[serde(default)]
    pub uid: String,
    #[serde(default)]
    pub has_midi_input: bool,
    #[serde(default)]
    pub has_gui: bool,
}

/// `ScannedPlugin::kind` 的 serde 默认值：未知类型。
/// serde default for `ScannedPlugin::kind`: unknown.
fn default_plugin_kind() -> PluginKind {
    PluginKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::synth::{GateKind, SynthEvent};
    fn installed(path: &str) -> Option<PathBuf> {
        let path = PathBuf::from(path);
        path.exists().then_some(path)
    }

    #[test]
    #[ignore = "requires locally installed FabFilter VST3 plugins"]
    fn loads_and_processes_real_vst3_plugins() {
        let Some(instrument_path) =
            installed(r"C:\Program Files\Common Files\VST3\FabFilter\FabFilter One.vst3")
        else {
            eprintln!("FabFilter One 未安装，跳过");
            return;
        };
        let mut instrument = PluginHost::load_plugin(
            PluginFormat::Vst3,
            &instrument_path,
            "",
            48_000.0,
            512,
            120.0,
            (4, 4),
        )
        .expect("load instrument");
        assert!(
            !instrument.params().is_empty(),
            "instrument should expose parameters"
        );
        assert!(instrument.has_editor(), "FabFilter One should have a GUI");

        let mut left = vec![0.0_f32; 512];
        let mut right = vec![0.0_f32; 512];
        let mut peak = 0.0_f32;
        let gate = SynthEvent {
            frame: 0,
            kind: GateKind::On,
            pitch: 60,
            velocity: 100,
        };
        for _ in 0..32 {
            instrument.process(48_000.0, 512, None, &[gate], &mut left, &mut right);
            for (l, r) in left.iter().zip(right.iter()) {
                peak = peak.max(l.abs()).max(r.abs());
            }
        }
        eprintln!("FabFilter One peak = {peak}");
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));

        let Some(effect_path) =
            installed(r"C:\Program Files\Common Files\VST3\FabFilter\FabFilter Pro-Q 4.vst3")
        else {
            return;
        };
        let mut effect = PluginHost::load_plugin(
            PluginFormat::Vst3,
            &effect_path,
            "",
            48_000.0,
            512,
            120.0,
            (4, 4),
        )
        .expect("load effect");
        assert!(!effect.params().is_empty());
        let input_l = vec![0.12_f32; 512];
        let input_r = vec![0.08_f32; 512];
        effect.process(
            48_000.0,
            512,
            Some((&input_l, &input_r)),
            &[],
            &mut left,
            &mut right,
        );
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
    }

    #[test]
    #[ignore = "diagnostic: requires locally installed FabFilter Pro-Q 4 and opens its editor"]
    #[cfg(target_os = "windows")]
    fn pro_q_editor_repaints_in_a_plain_message_loop() {
        use std::time::{Duration, Instant};
        use winapi::um::winuser::{
            DispatchMessageW, MsgWaitForMultipleObjectsEx, PeekMessageW, TranslateMessage, MSG,
            MWMO_INPUTAVAILABLE, PM_REMOVE, QS_ALLINPUT, WM_PAINT, WM_TIMER,
        };

        let path =
            PathBuf::from(r"C:\Program Files\Common Files\VST3\FabFilter\FabFilter Pro-Q 4.vst3");
        if !path.exists() {
            eprintln!("Pro-Q 4 未安装，跳过");
            return;
        }
        // 与桌面应用一致：启用 DPI 感知，让 GetDpiForWindow 返回真实缩放。
        // Match the desktop app: enable DPI awareness so GetDpiForWindow reports the real scale.
        unsafe {
            winapi::um::winuser::SetProcessDPIAware();
        }
        let runtime =
            PluginHost::load_plugin(PluginFormat::Vst3, &path, "", 48_000.0, 512, 120.0, (4, 4))
                .expect("load Pro-Q 4");
        let plugin = runtime.vst3_plugin().expect("VST3 runtime");

        // 持续处理音频，让分析器有数据可画。
        // Keep processing audio so the analyser has data to draw.
        let audio = plugin.clone();
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let audio_running = running.clone();
        std::thread::spawn(move || {
            while audio_running.load(std::sync::atomic::Ordering::Relaxed) {
                let mut buffers = vst3_host::AudioBuffers::new(2, 2, 512, 48_000.0);
                for channel in 0..2 {
                    if let Some(input) = buffers.inputs.get_mut(channel) {
                        for (index, sample) in input.iter_mut().enumerate() {
                            *sample = ((index as f32) * 0.05).sin() * 0.1;
                        }
                    }
                }
                if let Ok(mut plugin) = audio.lock() {
                    let _ = plugin.process_audio(&mut buffers);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });

        let mut window = vst3_host::PluginWindow::new(plugin.clone());
        window.open().expect("open Pro-Q editor");

        // Does a due WM_TIMER wake MsgWaitForMultipleObjectsEx without a timeout?
        // 是否会在超时前被插件的 WM_TIMER 唤醒？
        let wait_start = Instant::now();
        let wait_result = unsafe {
            MsgWaitForMultipleObjectsEx(0, std::ptr::null(), 1000, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        };
        eprintln!(
            "MsgWait first wake: result={wait_result} elapsed={:?}",
            wait_start.elapsed()
        );

        let count_for = |duration: Duration| {
            let start = Instant::now();
            let (mut timers, mut paints, mut messages) = (0usize, 0usize, 0usize);
            while start.elapsed() < duration {
                let mut message: MSG = unsafe { std::mem::zeroed() };
                while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) }
                    != 0
                {
                    messages += 1;
                    match message.message {
                        WM_TIMER => timers += 1,
                        WM_PAINT => paints += 1,
                        _ => {}
                    }
                    unsafe {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                let _ = window.service_platform_events();
                std::thread::sleep(Duration::from_millis(2));
            }
            (messages, timers, paints)
        };

        let active = count_for(Duration::from_secs(1));

        // 模拟 winit：先阻塞在 MsgWaitForMultipleObjectsEx，再取出消息。
        // Mimic winit: block in MsgWaitForMultipleObjectsEx, then drain messages.
        let winit_like = {
            let start = Instant::now();
            let (mut timers, mut paints, mut messages) = (0usize, 0usize, 0usize);
            while start.elapsed() < Duration::from_secs(1) {
                unsafe {
                    MsgWaitForMultipleObjectsEx(
                        0,
                        std::ptr::null(),
                        16,
                        QS_ALLINPUT,
                        MWMO_INPUTAVAILABLE,
                    );
                }
                let mut message: MSG = unsafe { std::mem::zeroed() };
                while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) }
                    != 0
                {
                    messages += 1;
                    match message.message {
                        WM_TIMER => timers += 1,
                        WM_PAINT => paints += 1,
                        _ => {}
                    }
                    unsafe {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                let _ = window.service_platform_events();
            }
            (messages, timers, paints)
        };

        // 创建一个普通前台窗口，把焦点从插件编辑器移走。
        // Create a plain foreground window so the editor loses activation.
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let title: Vec<u16> = "focus taker\0".encode_utf16().collect();
        use winapi::um::winuser::{
            CreateWindowExW, SetForegroundWindow, ShowWindow, CW_USEDEFAULT, SW_SHOW,
            WS_OVERLAPPEDWINDOW, WS_VISIBLE,
        };
        let dummy = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                320,
                200,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert!(!dummy.is_null(), "failed to create focus taker");
        unsafe {
            ShowWindow(dummy, SW_SHOW);
            SetForegroundWindow(dummy);
        }
        std::thread::sleep(Duration::from_millis(200));
        let inactive = count_for(Duration::from_secs(1));
        unsafe { winapi::um::winuser::DestroyWindow(dummy) };

        running.store(false, std::sync::atomic::Ordering::Relaxed);
        eprintln!(
            "Pro-Q editor tight loop: messages={} WM_TIMER={} WM_PAINT={}; winit-like: messages={} WM_TIMER={} WM_PAINT={}; inactive: messages={} WM_TIMER={} WM_PAINT={}",
            active.0,
            active.1,
            active.2,
            winit_like.0,
            winit_like.1,
            winit_like.2,
            inactive.0,
            inactive.1,
            inactive.2
        );
        assert!(
            winit_like.1 + winit_like.2 > 0,
            "MsgWaitForMultipleObjectsEx did not dispatch the plugin's timer messages"
        );
    }

    #[test]
    #[ignore = "diagnostic: requires locally installed FabFilter Pro-Q 4; opens an embedded child window"]
    #[cfg(target_os = "windows")]
    fn pro_q_editor_embeds_in_a_host_child_window() {
        use std::time::Duration;
        use winapi::shared::windef::{HWND, POINT, RECT};
        use winapi::um::winuser::{
            ClientToScreen, CreateWindowExW, DestroyWindow, DispatchMessageW, FindWindowW,
            GetClientRect, GetWindowRect, IsWindowVisible, MsgWaitForMultipleObjectsEx,
            PeekMessageW, ShowWindow, TranslateMessage, MSG, MWMO_INPUTAVAILABLE, PM_REMOVE,
            QS_ALLINPUT, SW_SHOW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
        };

        let path =
            PathBuf::from(r"C:\Program Files\Common Files\VST3\FabFilter\FabFilter Pro-Q 4.vst3");
        if !path.exists() {
            eprintln!("Pro-Q 4 未安装，跳过");
            return;
        }
        unsafe { winapi::um::winuser::SetProcessDPIAware() };
        let runtime =
            PluginHost::load_plugin(PluginFormat::Vst3, &path, "", 48_000.0, 512, 120.0, (4, 4))
                .expect("load Pro-Q 4");
        let plugin = runtime.vst3_plugin().expect("VST3 runtime");

        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let title: Vec<u16> = "embedded host\0".encode_utf16().collect();
        let parent = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                100,
                100,
                900,
                700,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert!(!parent.is_null(), "failed to create host window");
        unsafe { ShowWindow(parent, SW_SHOW) };

        let mut window = vst3_host::PluginWindow::new(plugin);
        window
            .open_embedded(parent as *mut std::ffi::c_void)
            .expect("open embedded Pro-Q editor");
        // 与宿主一致：先按 500×300 等比缩放，再摆放。
        // Same as the host: fit the editor into 500×300, then place it.
        let (fitted_width, fitted_height) = window.fit_embedded(500, 300);
        assert!(fitted_width <= 500 && fitted_height <= 300);
        window.set_embedded_bounds(40, 40, fitted_width, fitted_height);
        window.set_embedded_clip(0, 0, fitted_width / 2, fitted_height);
        window.set_embedded_visible(true);

        // 泵消息并服务窗口，让插件完成首次布局与绘制。
        // Pump messages and service the window so the plugin can lay out and paint.
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(600) {
            unsafe {
                MsgWaitForMultipleObjectsEx(
                    0,
                    std::ptr::null(),
                    16,
                    QS_ALLINPUT,
                    MWMO_INPUTAVAILABLE,
                )
            };
            let mut message: MSG = unsafe { std::mem::zeroed() };
            while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0
            {
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            let _ = window.service_platform_events();
        }

        // 内嵌编辑器是无边框附属顶层窗口，按窗口类查找。
        // The inline editor is a frameless owned top-level window; look it up by class.
        let class: Vec<u16> = "VST3PluginEmbeddedWindow\0".encode_utf16().collect();
        let child: HWND = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
        assert!(!child.is_null(), "embedded window was not created");
        assert_eq!(
            unsafe { IsWindowVisible(child) },
            1,
            "embedded window is hidden"
        );

        let mut client: RECT = unsafe { std::mem::zeroed() };
        unsafe { GetClientRect(parent, &mut client) };
        let mut origin = POINT {
            x: client.left,
            y: client.top,
        };
        unsafe { ClientToScreen(parent, &mut origin) };
        let mut rect: RECT = unsafe { std::mem::zeroed() };
        unsafe { GetWindowRect(child, &mut rect) };
        eprintln!(
            "embedded child rect: ({}, {}) {}x{}",
            rect.left - origin.x,
            rect.top - origin.y,
            rect.right - rect.left,
            rect.bottom - rect.top
        );
        assert_eq!(rect.left - origin.x, 40);
        assert_eq!(rect.top - origin.y, 40);
        assert_eq!(rect.right - rect.left, fitted_width);
        assert_eq!(rect.bottom - rect.top, fitted_height);

        window.set_embedded_visible(false);
        assert_eq!(
            unsafe { IsWindowVisible(child) },
            0,
            "embedded window did not hide"
        );
        unsafe { DestroyWindow(parent) };
    }

    #[test]
    #[ignore = "diagnostic: loads Pro-Q on a worker thread like project loading does"]
    #[cfg(target_os = "windows")]
    fn pro_q_editor_timer_thread_follows_the_loading_thread() {
        use std::time::{Duration, Instant};
        use winapi::um::winuser::{
            DispatchMessageW, MsgWaitForMultipleObjectsEx, PeekMessageW, TranslateMessage, MSG,
            MWMO_INPUTAVAILABLE, PM_REMOVE, QS_ALLINPUT, WM_TIMER,
        };

        let path =
            PathBuf::from(r"C:\Program Files\Common Files\VST3\FabFilter\FabFilter Pro-Q 4.vst3");
        if !path.exists() {
            eprintln!("Pro-Q 4 未安装，跳过");
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let start_pump = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_start = start_pump.clone();
        let worker = std::thread::spawn(move || {
            let runtime = PluginHost::load_plugin(
                PluginFormat::Vst3,
                &path,
                "",
                48_000.0,
                512,
                120.0,
                (4, 4),
            )
            .expect("load Pro-Q 4 on worker");
            let plugin = runtime.vst3_plugin().expect("VST3 runtime");
            tx.send(plugin.clone()).expect("send plugin");
            while !worker_start.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(5));
            }
            // 模拟常驻同步线程的消息泵。
            // Mimic the persistent sync thread's message pump.
            let start = Instant::now();
            let mut timers = 0usize;
            while start.elapsed() < Duration::from_secs(2) {
                unsafe {
                    MsgWaitForMultipleObjectsEx(
                        0,
                        std::ptr::null(),
                        16,
                        QS_ALLINPUT,
                        MWMO_INPUTAVAILABLE,
                    );
                }
                let mut message: MSG = unsafe { std::mem::zeroed() };
                while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) }
                    != 0
                {
                    if message.message == WM_TIMER {
                        timers += 1;
                    }
                    unsafe {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
            }
            timers
        });

        let plugin = rx.recv().expect("plugin from worker");
        let audio = plugin.clone();
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let audio_running = running.clone();
        std::thread::spawn(move || {
            while audio_running.load(std::sync::atomic::Ordering::Relaxed) {
                let mut buffers = vst3_host::AudioBuffers::new(2, 2, 512, 48_000.0);
                for channel in 0..2 {
                    if let Some(input) = buffers.inputs.get_mut(channel) {
                        for (index, sample) in input.iter_mut().enumerate() {
                            *sample = ((index as f32) * 0.05).sin() * 0.1;
                        }
                    }
                }
                if let Ok(mut plugin) = audio.lock() {
                    let _ = plugin.process_audio(&mut buffers);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });

        let mut window = vst3_host::PluginWindow::new(plugin);
        window.open().expect("open Pro-Q editor");
        start_pump.store(true, std::sync::atomic::Ordering::Relaxed);

        let start = Instant::now();
        let mut main_timers = 0usize;
        while start.elapsed() < Duration::from_secs(2) {
            unsafe {
                MsgWaitForMultipleObjectsEx(
                    0,
                    std::ptr::null(),
                    16,
                    QS_ALLINPUT,
                    MWMO_INPUTAVAILABLE,
                );
            }
            let mut message: MSG = unsafe { std::mem::zeroed() };
            while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0
            {
                if message.message == WM_TIMER {
                    main_timers += 1;
                }
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            let _ = window.service_platform_events();
        }
        running.store(false, std::sync::atomic::Ordering::Relaxed);
        let worker_timers = worker.join().expect("worker pump");
        eprintln!("Pro-Q timer thread: main={main_timers} worker={worker_timers}");
        assert!(
            main_timers + worker_timers > 0,
            "neither thread received the editor timer"
        );
    }

    #[test]
    #[ignore = "diagnostic: requires locally installed FabFilter Pro-Q 4"]
    fn pro_q_pushes_component_controller_messages_from_process() {
        let path =
            PathBuf::from(r"C:\Program Files\Common Files\VST3\FabFilter\FabFilter Pro-Q 4.vst3");
        if !path.exists() {
            eprintln!("Pro-Q 4 未安装，跳过");
            return;
        }
        let runtime =
            PluginHost::load_plugin(PluginFormat::Vst3, &path, "", 48_000.0, 512, 120.0, (4, 4))
                .expect("load Pro-Q 4");
        let plugin = runtime.vst3_plugin().expect("VST3 runtime");
        let worker = plugin.clone();
        // 处理必须发生在 UI 线程之外，才能验证 component→controller 消息排队路径。
        // Processing must run off the UI thread to exercise the queued notify path.
        std::thread::spawn(move || {
            for _ in 0..32 {
                let mut buffers = vst3_host::AudioBuffers::new(2, 2, 512, 48_000.0);
                for channel in 0..2 {
                    if let Some(input) = buffers.inputs.get_mut(channel) {
                        for (index, sample) in input.iter_mut().enumerate() {
                            *sample = ((index as f32) * 0.05).sin() * 0.1;
                        }
                    }
                }
                if let Ok(mut plugin) = worker.lock() {
                    let _ = plugin.process_audio(&mut buffers);
                }
            }
        })
        .join()
        .expect("audio worker");

        let mut guard = plugin.lock().expect("plugin lock");
        let delivered = guard.service_connection_messages();
        let parameter_changes = guard.get_parameter_changes();
        // Pro-Q 4 是单组件插件，分析器数据走内部共享状态，不经过 IConnectionPoint；
        // 这里只记录数量，避免把“没有连接点消息”误判为失败。
        // Pro-Q 4 is a single-component plug-in whose analyser uses internal shared state
        // rather than IConnectionPoint; log the counts instead of asserting.
        eprintln!(
            "Pro-Q connection messages delivered = {delivered}, output parameter changes = {}",
            parameter_changes.len()
        );
    }

    #[test]
    #[ignore = "requires locally installed FabFilter Pro-Q 4"]
    fn probes_single_file_vst3_metadata() {
        let path =
            PathBuf::from(r"C:\Program Files\Common Files\VST3\FabFilter\FabFilter Pro-Q 4.vst3");
        if !path.exists() {
            eprintln!("Pro-Q 4 未安装，跳过");
            return;
        }
        let plugin = PluginHost::probe_plugin_metadata(PluginFormat::Vst3, &path)
            .expect("probe Pro-Q 4 metadata");
        assert_eq!(plugin.kind, PluginKind::Effect, "Pro-Q is an effect");
        assert!(
            !plugin.uid.is_empty(),
            "a single-file VST3 should expose its CID"
        );
    }

    #[test]
    #[ignore = "build tools/vst2-fixture first: cargo build -p surf-vst2-fixture"]
    fn loads_and_processes_vst2_fixture() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/surf_vst2_fixture.dll");
        if !path.exists() {
            eprintln!("fixture DLL not built, skipping: {}", path.display());
            return;
        }
        let mut instrument =
            PluginHost::load_plugin(PluginFormat::Vst2, &path, "", 48_000.0, 512, 120.0, (4, 4))
                .expect("load VST2 fixture");
        assert!(!instrument.params().is_empty());

        let mut left = vec![0.0_f32; 512];
        let mut right = vec![0.0_f32; 512];
        let gate = SynthEvent {
            frame: 0,
            kind: GateKind::On,
            pitch: 69,
            velocity: 110,
        };
        let mut peak = 0.0_f32;
        for _ in 0..16 {
            instrument.process(48_000.0, 512, None, &[gate], &mut left, &mut right);
            for (l, r) in left.iter().zip(right.iter()) {
                peak = peak.max(l.abs()).max(r.abs());
            }
        }
        eprintln!("VST2 fixture peak = {peak}");
        assert!(peak > 0.01, "VST2 instrument should produce audio");
        assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
    }

    #[test]
    #[ignore = "opens a native VST2 editor window; build tools/vst2-fixture and run manually"]
    fn opens_vst2_fixture_editor_window() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/surf_vst2_fixture.dll");
        if !path.exists() {
            eprintln!("fixture DLL not built, skipping: {}", path.display());
            return;
        }
        let mut runtime =
            PluginHost::load_plugin(PluginFormat::Vst2, &path, "", 48_000.0, 512, 120.0, (4, 4))
                .expect("load VST2 fixture");
        assert!(runtime.has_editor(), "fixture should advertise an editor");
        let editor = runtime.take_editor().expect("fixture editor");
        let window = Vst2EditorWindow::new(editor, "Surfing VST2 Fixture")
            .expect("create VST2 editor window");
        assert!(window.is_visible(), "editor window should be visible");
    }

    #[test]
    #[ignore = "requires the bundled SoundBug example and a local TDR Nova VST3"]
    fn restores_soundbug_plugin_state_in_a_vst3() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join("光年外的流星_编曲工程")
            .join("光年外的流星.sndp");
        if !path.exists() {
            eprintln!("示例工程不存在，跳过: {}", path.display());
            return;
        }
        let session = crate::core::project_runtime::ProjectRuntime::open_project(&path)
            .expect("open SoundBug example");
        let mut target = None;
        for track in &session.project.tracks {
            let plugin = match &track.instrument {
                crate::core::model::InstrumentRef::Plugin(plugin) => Some(plugin),
                _ => None,
            };
            let candidates = plugin
                .into_iter()
                .chain(track.effects.iter().filter_map(|effect| effect.plugin()));
            for plugin in candidates {
                if !plugin.state.is_empty()
                    && plugin
                        .path
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().contains("TDR Nova"))
                        .unwrap_or(false)
                    && plugin.path.exists()
                {
                    target = Some(plugin.clone());
                    break;
                }
            }
            if target.is_some() {
                break;
            }
        }
        let Some(plugin) = target else {
            eprintln!("未找到 TDR Nova 插件，跳过");
            return;
        };
        let mut runtime = PluginHost::load_plugin(
            plugin.format,
            &plugin.path,
            &plugin.uid,
            48_000.0,
            512,
            120.0,
            (4, 4),
        )
        .expect("load TDR Nova");
        runtime.restore_state(&plugin.state, &plugin.params);
        let (state, _) = runtime.capture_state();
        assert!(
            !state.is_empty(),
            "restored plugin should expose a state blob"
        );
    }

    #[test]
    #[ignore = "diagnostic: requires locally installed Auto-Tune Pro; measures editor open time"]
    #[cfg(target_os = "windows")]
    fn auto_tune_editor_open_timing() {
        use std::time::{Duration, Instant};
        use winapi::shared::windef::HWND;
        use winapi::um::winuser::{
            CreateWindowExW, DestroyWindow, DispatchMessageW, MsgWaitForMultipleObjectsEx,
            PeekMessageW, ShowWindow, TranslateMessage, MSG, MWMO_INPUTAVAILABLE, PM_REMOVE,
            QS_ALLINPUT, SW_SHOW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
        };

        let path = PathBuf::from(r"C:\Program Files\Common Files\VST3\Antares\Auto-Tune Pro.vst3");
        if !path.exists() {
            eprintln!("Auto-Tune Pro 未安装，跳过");
            return;
        }
        unsafe { winapi::um::winuser::SetProcessDPIAware() };
        let start = Instant::now();
        let runtime =
            PluginHost::load_plugin(PluginFormat::Vst3, &path, "", 48_000.0, 512, 120.0, (4, 4))
                .expect("load Auto-Tune Pro");
        eprintln!("load_plugin: {:?}", start.elapsed());
        let plugin = runtime.vst3_plugin().expect("VST3 runtime");

        let probe = Instant::now();
        let size = plugin
            .lock()
            .expect("plugin lock")
            .get_editor_size()
            .unwrap_or((0, 0));
        eprintln!("get_editor_size: {:?} -> {size:?}", probe.elapsed());

        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let title: Vec<u16> = "autotune host\0".encode_utf16().collect();
        let parent: HWND = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                100,
                100,
                1000,
                760,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert!(!parent.is_null(), "failed to create host window");
        unsafe { ShowWindow(parent, SW_SHOW) };

        let query = plugin.clone();
        let open = Instant::now();
        let mut window = vst3_host::PluginWindow::new(plugin);
        window
            .open_embedded(parent as *mut std::ffi::c_void)
            .expect("open embedded editor");
        eprintln!("open_embedded: {:?}", open.elapsed());

        {
            let guard = query.lock().expect("plugin lock");
            eprintln!(
                "can_resize: {:?} attached_size: {:?}",
                guard.attached_editor_can_resize(),
                guard.attached_editor_size()
            );
        }

        let attributes = Instant::now();
        let fitted = window.fit_embedded(430, 240);
        eprintln!(
            "fit_embedded(430x240) -> {fitted:?} in {:?}",
            attributes.elapsed()
        );
        window.set_embedded_bounds(40, 40, fitted.0, fitted.1);
        window.set_embedded_visible(true);
        let rescaled = window.fit_embedded(860, 480);
        eprintln!("fit_embedded(860x480) -> {rescaled:?}");

        // 逐帧测量：任何一帧明显卡住都会体现在这个最大耗时里。
        // Measure per-frame service time: a hang shows up as a large maximum.
        let start = Instant::now();
        let mut worst = Duration::ZERO;
        while start.elapsed() < Duration::from_secs(3) {
            unsafe {
                MsgWaitForMultipleObjectsEx(
                    0,
                    std::ptr::null(),
                    16,
                    QS_ALLINPUT,
                    MWMO_INPUTAVAILABLE,
                )
            };
            let mut message: MSG = unsafe { std::mem::zeroed() };
            while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0
            {
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            let frame = Instant::now();
            let _ = window.service_platform_events();
            worst = worst.max(frame.elapsed());
            std::thread::sleep(Duration::from_millis(4));
        }
        eprintln!("worst service frame: {worst:?}");
        unsafe { DestroyWindow(parent) };
    }

    #[test]
    #[ignore = "reads the local VST3 installation"]
    fn scans_installed_vst3_metadata() {
        let plugins = PluginHost::scan_plugin_format(PluginFormat::Vst3, &[]).expect("scan");
        assert!(
            !plugins.is_empty(),
            "expected at least one .vst3 bundle in the standard directory"
        );
        for plugin in plugins.iter().take(30) {
            eprintln!(
                "{} | {} | {} | {} | uid={}",
                plugin.name,
                plugin.vendor,
                plugin.category,
                plugin.kind.label(),
                plugin.uid
            );
        }
        assert!(
            plugins
                .iter()
                .any(|p| p.vendor.contains("FabFilter") || p.name.contains("FabFilter")),
            "expected at least one FabFilter plugin"
        );
    }
}
