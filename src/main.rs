#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! 所属模块：main / 二进制入口
//! 文件用途：
//! - 创建 eframe 根窗口并安装未处理异常兜底；业务逻辑全部在 `surfing_studio` 库中。
//! - 初始化诊断日志，并在所有退出路径上写尾帧后直接结束进程。
//! - `--decode-log <文件>`：把二进制 `.sflog` 解码为同目录 `.txt` 后退出（不创建窗口）。
//!
//! 函数清单：
//! - `terminate_on_unhandled_exception`：记录异常并直接终止进程。
//! - `native_options`：按启动设置选择 OpenGL 兼容渲染器或 wgpu（DirectX 12 / Vulkan）。
//! - `decode_log_command`：处理 `--decode-log` 命令行模式。
//! - `main`：初始化日志、安装异常过滤器、创建根窗口并在退出时兜底终止进程。
//!
//! Module: main / binary entry
//! Purpose:
//! - Create the eframe root window and install the unhandled-exception fallback; all business logic
//!   lives in the `surfing_studio` library.
//! - Initialize the diagnostic log and end the process right after writing the footer on every exit
//!   path.
//! - `--decode-log <file>`: decode a binary `.sflog` into a `.txt` next to it and exit (no window).
//!
//! Functions:
//! - `terminate_on_unhandled_exception`: log the exception and terminate the process.
//! - `native_options`: pick the OpenGL-compatible renderer or wgpu (DirectX 12 / Vulkan) from the
//!   startup settings.
//! - `decode_log_command`: handle the `--decode-log` command-line mode.
//! - `main`: initialize logging, install the exception filter, create the root window and terminate
//!   as a fallback.

#[cfg(not(target_os = "android"))]
use std::path::PathBuf;

#[cfg(not(target_os = "android"))]
use surfing_studio::app::{gpu_render_startup, GpuRenderApi, GpuRenderStartup};
#[cfg(not(target_os = "android"))]
use surfing_studio::audio::gpu::{GpuApi, GpuRuntime};
#[cfg(not(target_os = "android"))]
use surfing_studio::diag;
#[cfg(not(target_os = "android"))]
use surfing_studio::WindowHost;

/// 所属模块：main / 退出兜底
/// 功能：未处理异常（含插件内部的原生/.NET 异常）发生时记录 FATAL 并立即结束进程。
/// 输入：`info`：Windows 异常信息；可能为空指针。
/// 输出/副作用：写诊断日志尾帧并调用 `TerminateProcess`；正常情况下不会返回。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: main / exit fallback
/// Purpose: Log a FATAL record and terminate immediately on an unhandled exception (including
/// native/.NET exceptions raised inside plugins).
/// Inputs: `info`: Windows exception information; may be null.
/// Outputs / side effects: writes the log footer and calls `TerminateProcess`; normally never
/// returns.
/// Complexity: O(1) time and space.
#[cfg(target_os = "windows")]
unsafe extern "system" fn terminate_on_unhandled_exception(
    info: *mut winapi::um::winnt::EXCEPTION_POINTERS,
) -> i32 {
    // 插件的未处理异常会触发 Windows 错误报告，进程可能长时间停在崩溃对话框里，
    // 外部 taskkill 也会因为进程正在终止而返回“拒绝访问”。先记日志，再直接结束进程。
    // An unhandled plugin exception opens Windows Error Reporting; the process can stay in the
    // crash dialog and external taskkill fails with "access denied" while it is terminating.
    // Log first, then terminate directly.
    if !info.is_null() {
        if let Some(record) = (*info).ExceptionRecord.as_ref() {
            let code = format!("0x{:08X}", record.ExceptionCode);
            diag::crash(
                "unhandled_exception",
                "app.crash",
                "unhandled_exception",
                &[("exception_code", &code), ("phase", "native")],
            );
        }
    }
    winapi::um::processthreadsapi::TerminateProcess(
        winapi::um::processthreadsapi::GetCurrentProcess(),
        1,
    );
    // EXCEPTION_EXECUTE_HANDLER（1）：让系统继续执行处理器；进程已在上一行终止。
    // EXCEPTION_EXECUTE_HANDLER (1): the process was already terminated above.
    1
}

/// 所属模块：main / 日志解码
/// 功能：处理 `--decode-log <文件>`：把二进制日志解码为同目录 `.txt` 后退出。
/// 输入：`args`：完整命令行参数（含程序名）。
/// 输出/副作用：匹配时写 `.txt` 并结束进程（成功 0，失败 2）；不匹配时返回 `false`。
/// 复杂度：时间 O(日志大小)，空间 O(日志大小)。
///
/// Module: main / log decoding
/// Purpose: Handle `--decode-log <file>`: decode a binary log into a `.txt` next to it, then exit.
/// Inputs: `args`: full command line (including the program name).
/// Outputs / side effects: writes the `.txt` and exits (0 on success, 2 on failure); returns `false`
/// when the arguments do not match.
/// Complexity: O(log size) time and space.
#[cfg(not(target_os = "android"))]
fn decode_log_command(args: &[String]) -> bool {
    if args.len() != 3 || args[1] != "--decode-log" {
        return false;
    }
    match diag::export_text(&PathBuf::from(&args[2])) {
        Ok(target) => {
            println!("已解码到 {}", target.display());
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("解码日志失败: {error}");
            std::process::exit(2);
        }
    }
}

/// 所属模块：main / 窗口创建
/// 功能：按设置选择渲染器：开启 GPU 加速、选择 DirectX 12 / Vulkan 且该 API 有可用 GPU 时使用
/// wgpu，否则使用 OpenGL 兼容路径。
/// 输入：`startup`：启动阶段读取的 GPU 渲染偏好。
/// 输出/副作用：返回 eframe 原生选项与是否使用 wgpu；可能枚举 GPU 设备，不创建窗口。
/// 复杂度：时间 O(GPU 设备数)（仅在开启加速时枚举），空间 O(设备数)。
///
/// Module: main / window creation
/// Purpose: Pick the renderer from the settings: wgpu when GPU acceleration is enabled, DirectX 12 /
/// Vulkan is selected and that API has a usable GPU; OpenGL compatibility otherwise.
/// Inputs: `startup`: the GPU preference read at startup.
/// Outputs / side effects: eframe options plus whether wgpu is used; may enumerate GPU devices, never
/// creates a window.
/// Complexity: O(GPU devices) time when acceleration is enabled, O(devices) space.
#[cfg(not(target_os = "android"))]
fn native_options(startup: GpuRenderStartup) -> (eframe::NativeOptions, bool) {
    let mut options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1100.0, 700.0])
            .with_title("SurfingStudio")
            // 关闭系统标题栏，改用应用内自绘应用栏（图标、菜单、项目栏、窗口按钮）。
            // Disable the OS title bar; the app draws its own app bar instead.
            .with_decorations(false),
        // 默认使用 glow（OpenGL）：更稳定，也更适合嵌入原生插件子窗口。
        // Default to glow (OpenGL): more stable and better with native plugin child windows.
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    if !startup.enabled {
        return (options, false);
    }
    let (apis, backends) = match startup.api {
        GpuRenderApi::Auto => (
            vec![GpuApi::DirectX12, GpuApi::Vulkan],
            eframe::wgpu::Backends::DX12 | eframe::wgpu::Backends::VULKAN,
        ),
        GpuRenderApi::DirectX12 => (vec![GpuApi::DirectX12], eframe::wgpu::Backends::DX12),
        GpuRenderApi::Vulkan => (vec![GpuApi::Vulkan], eframe::wgpu::Backends::VULKAN),
        // OpenGL 兼容路径继续走 glow，不切换到 wgpu。
        // The OpenGL compatibility path keeps using glow instead of switching to wgpu.
        GpuRenderApi::OpenGl => return (options, false),
    };
    // 先确认所选 API 至少有一块可用 GPU：驱动缺失时直接留在 OpenGL，避免启动失败。
    // Confirm the API has a usable GPU first: without a driver the app stays on OpenGL instead of
    // failing to start.
    let available = GpuRuntime::standard()
        .devices()
        .map(|devices| devices.iter().any(|device| apis.contains(&device.api)))
        .unwrap_or(false);
    if !available {
        return (options, false);
    }
    options.renderer = eframe::Renderer::Wgpu;
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = backends;
    options.wgpu_options.wgpu_setup = setup.into();
    (options, true)
}

/// 所属模块：main / 安卓入口占位
/// 功能：安卓的入口是 `platforms/android` 里的 `android_main`（NativeActivity 不会调用 `main`），
/// 这里只保留一个空实现，让同一个包在安卓目标下也能通过编译检查。
/// 输入：无。
/// 输出/副作用：无。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: main / Android entry placeholder
/// Purpose: Android starts from `android_main` in `platforms/android` (NativeActivity never calls
/// `main`); this stub only keeps the package compiling for the Android target.
/// Inputs: none.
/// Outputs / side effects: none.
/// Complexity: O(1) time and space.
#[cfg(target_os = "android")]
fn main() {}

/// 所属模块：main / 入口
/// 功能：初始化日志与异常兜底，创建 eframe 根窗口，并在所有退出路径上确定性地结束进程。
/// 输入：无。
/// 输出/副作用：创建窗口与音频设备；退出时写日志尾帧并终止进程。
/// 复杂度：时间 O(启动)，空间 O(设置 + 工程)。
///
/// Module: main / entry
/// Purpose: Initialize logging and the exception fallback, create the eframe root window and end the
/// process deterministically on every exit path.
/// Inputs: none.
/// Outputs / side effects: opens windows and audio devices; writes the log footer and terminates on
/// exit.
/// Complexity: O(startup) time and O(settings + project) space.
#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result<()> {
    // 命令行解码模式先于日志初始化：解码旧日志时不能再创建新日志。
    // The command-line decoder runs before logging init: decoding an old log must not create a new one.
    let args: Vec<String> = std::env::args().collect();
    let _ = decode_log_command(&args);
    // 日志必须最先初始化：启动耗时、panic 与退出原因都要落在同一个文件里。
    // Logging is initialized first so startup timings, panics and the exit reason share one file.
    diag::init(env!("CARGO_PKG_VERSION"));
    // 在创建任何窗口之前安装顶层异常过滤器：插件在自己的窗口过程或 .NET 消息循环中抛出的
    // 未处理异常不会留在“幽灵进程”里。
    // Install the top-level exception filter before any window exists, so an unhandled exception
    // raised inside a plugin's window procedure or .NET message loop cannot leave a zombie process.
    #[cfg(target_os = "windows")]
    unsafe {
        winapi::um::errhandlingapi::SetUnhandledExceptionFilter(Some(
            terminate_on_unhandled_exception,
        ));
    }
    let startup = gpu_render_startup();
    let (options, uses_wgpu) = native_options(startup);
    let result = eframe::run_native(
        "SurfingStudio",
        options,
        // 根窗口由 `WindowHost` 管理：初始显示主页，第一个工程可接管该窗口，其余工程各自视口。
        // The root window is managed by `WindowHost`: it starts on the home page, the first project
        // may take it over, and further projects get their own viewports.
        Box::new(|cc| Ok(Box::new(WindowHost::new(cc)))),
    );
    // wgpu 初始化仍然失败时回退 OpenGL，避免软件完全无法启动。
    // Fall back to OpenGL when wgpu still fails to initialise, so the app can always start.
    let result = match result {
        Err(_) if uses_wgpu => eframe::run_native(
            "SurfingStudio",
            native_options(GpuRenderStartup::default()).0,
            Box::new(|cc| Ok(Box::new(WindowHost::new(cc)))),
        ),
        other => other,
    };
    // 退出阶段不能走正常的 CRT/DLL 卸载：插件 DLL 的 DllMain(DETACH) 可能等待
    // 已停止的 UI 线程（Auto-Tune 等），导致进程永不结束。先写日志尾帧，再直接终止进程。
    // 正常关闭路径已在 WindowHost 内（关闭根窗口 / on_exit）终止进程；这里兜底覆盖
    // `run_native` 直接返回的罕见路径。
    // Do not run the normal CRT/DLL detach on exit: plugin DllMain(DETACH) may wait for a UI
    // thread that no longer runs (Auto-Tune), so the process never ends. Write the log footer and
    // terminate directly. The normal close path already terminates inside `WindowHost` (root
    // window close / `on_exit`); this covers the rare case where `run_native` returns by itself.
    let _ = result;
    diag::request_exit("run_native_returned")
}
