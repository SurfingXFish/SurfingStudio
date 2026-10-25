//! 所属模块：platforms/android / 安卓入口
//! 文件用途：
//! - NativeActivity 的 `android_main` 入口：把平台存储目录交给平台运行时，初始化诊断日志，
//!   然后用 eframe 创建根窗口（`WindowHost` 负责主页与工程窗口）。
//! - 业务逻辑全部位于 `surfing_studio` 库；本文件只做与桌面 `main.rs` 等价的初始化。
//!
//! 函数清单：
//! - `android_main`：安卓进程入口。
//!
//! Module: platforms/android / Android entry
//! Purpose:
//! - NativeActivity's `android_main`: hand the platform storage directories to the platform runtime,
//!   initialize the diagnostic log and create the eframe root window (`WindowHost` owns the home and
//!   project windows).
//! - All business logic lives in the `surfing_studio` library; this file mirrors desktop `main.rs`.
//!
//! Functions:
//! - `android_main`: Android process entry point.

#![cfg(target_os = "android")]

use android_activity::AndroidApp;
use eframe::egui;

/// 所属模块：platforms/android / 安卓入口
/// 功能：安卓进程入口：安装存储目录、初始化日志、创建 eframe 根窗口，退出时写日志尾帧并结束进程。
/// 输入：`app`：NativeActivity 提供的应用句柄（用于解析应用私有目录）。
/// 输出/副作用：打开音频设备、创建根窗口、写诊断日志；`run_native` 返回后终止进程。
/// 复杂度：时间 O(启动)，空间 O(设置 + 工程)。
///
/// Module: platforms/android / Android entry
/// Purpose: Android entry: install storage directories, initialize the log, create the eframe root
/// window and, after `run_native` returns, write the log footer and end the process.
/// Inputs: `app`: the application handle supplied by NativeActivity (resolves app-private paths).
/// Outputs / side effects: opens audio devices, creates the root window, writes the diagnostic log
/// and terminates after `run_native` returns.
/// Complexity: O(startup) time and O(settings + project) space.
#[no_mangle]
pub extern "C" fn android_main(app: AndroidApp) {
    // 安卓没有 `%APPDATA%`：设置、日志与插件缓存都必须写在应用沙箱内。
    // Android has no `%APPDATA%`: settings, logs and caches must live inside the app sandbox.
    let data = app.internal_data_path();
    let cache = data
        .as_ref()
        .and_then(|path| path.parent())
        .map(|parent| parent.join("cache"));
    surfing_studio::platform::install_storage(data, cache);
    // SAF 导入/导出需要 JVM 与 Activity：文件选择结果由 MainActivity 回传给 Rust。
    // SAF import/export needs the JVM and the Activity: file-picker results come back to Rust from
    // MainActivity.
    surfing_studio::platform::install_activity(app.vm_as_ptr(), app.activity_as_ptr());
    surfing_studio::diag::init(env!("CARGO_PKG_VERSION"));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SurfingStudio")
            // 安卓窗口由系统全屏铺满，不需要系统标题栏。
            // Android windows fill the screen and have no system title bar.
            .with_decorations(false),
        // 安卓使用 OpenGL ES 兼容路径（wgpu 渲染不在移动端启用）。
        // Android uses the OpenGL ES compatibility path (wgpu rendering is off on mobile).
        renderer: eframe::Renderer::Glow,
        android_app: Some(app),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "SurfingStudio",
        options,
        Box::new(|cc| Ok(Box::new(surfing_studio::WindowHost::new(cc)))),
    );
    surfing_studio::diag::request_exit("android_main_returned");
}
