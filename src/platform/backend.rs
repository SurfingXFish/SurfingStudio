//! 所属模块：platform / 后端接口
//! 文件用途：
//! - 定义平台后端接口 `PlatformBackend` 与能力描述 `PlatformCaps`。
//! - 定义文件对话框请求 `FileRequest`，让应用层不感知具体对话框实现。
//! - 后端只负责单一系统，不感知应用状态。
//!
//! 函数清单：
//! - `PlatformBackend::info` / `caps`：后端标识与能力。
//! - `PlatformBackend::config_dir` / `cache_dir` / `data_dir` / `log_dir` / `temp_dir`：存储目录。
//! - `PlatformBackend::pick_file` / `pick_folder` / `save_file`：文件对话框。
//! - `PlatformBackend::exit_process`：确定性退出。
//!
//! Module: platform / backend interface
//! Purpose:
//! - Define the `PlatformBackend` interface and the `PlatformCaps` capability descriptor.
//! - Define `FileRequest` so the app layer never sees a concrete dialog implementation.
//! - A backend covers exactly one OS and knows nothing about the application state.
//!
//! Functions:
//! - `PlatformBackend::info` / `caps`: backend identity and capabilities.
//! - `PlatformBackend::config_dir` / `cache_dir` / `data_dir` / `log_dir` / `temp_dir`: storage.
//! - `PlatformBackend::pick_file` / `pick_folder` / `save_file`: file dialogs.
//! - `PlatformBackend::exit_process`: deterministic exit.

use super::DialogParent;
use std::path::{Path, PathBuf};

/// 平台后端标识。
/// Platform backend identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformInfo {
    /// 持久化/日志用的稳定标识。
    /// Stable id used in logs and persisted data.
    pub id: &'static str,
    /// 界面展示名称。
    /// Label shown in the UI.
    pub label: &'static str,
}

/// 平台能力集合：应用层据此隐藏或降级不可用功能。
/// Capability set: the app layer hides or degrades unsupported features.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformCaps {
    /// 是否支持多窗口（egui 视口 / 独立窗口）。
    /// Whether multiple OS windows (egui viewports) are supported.
    pub multi_window: bool,
    /// 是否支持拖动自绘应用栏移动窗口。
    /// Whether the hand-drawn app bar can drag the OS window.
    pub window_drag: bool,
    /// 是否提供系统原生文件对话框。
    /// Whether native file dialogs exist.
    pub native_file_dialog: bool,
    /// 是否支持 VST 插件宿主。
    /// Whether VST plugin hosting is available.
    pub plugin_hosting: bool,
    /// 是否支持插件原生编辑器窗口。
    /// Whether native plugin editor windows are available.
    pub native_plugin_editor: bool,
    /// 是否提供多种音频系统（WASAPI / ASIO 等）。
    /// Whether several audio systems (WASAPI / ASIO, ...) are offered.
    pub system_audio_systems: bool,
    /// 是否支持 GPU 界面渲染（wgpu 后端）。
    /// Whether GPU UI rendering (wgpu backend) is supported.
    pub gpu_ui_render: bool,
    /// 是否安装未处理异常过滤器（Windows 结构化异常）。
    /// Whether an unhandled-exception filter (Windows SEH) is installed.
    pub crash_trap: bool,
}

impl PlatformCaps {
    /// 所属模块：platform / 后端接口 / 能力
    /// 功能：返回全功能桌面能力（Windows / macOS / Linux 共用）。
    /// 输入：无。
    /// 输出/副作用：能力结构；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / backend interface / capabilities
    /// Purpose: Return the full desktop capability set (shared by Windows / macOS / Linux).
    /// Inputs: none.
    /// Outputs / side effects: the descriptor; no state change.
    /// Complexity: O(1) time and space.
    pub const DESKTOP: Self = Self {
        multi_window: true,
        window_drag: true,
        native_file_dialog: true,
        plugin_hosting: true,
        native_plugin_editor: true,
        system_audio_systems: true,
        gpu_ui_render: true,
        crash_trap: true,
    };

    /// 所属模块：platform / 后端接口 / 能力
    /// 功能：返回安卓能力：单窗口、无系统文件对话框、无 VST 宿主、固定系统音频。
    /// 输入：无。
    /// 输出/副作用：能力结构；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / backend interface / capabilities
    /// Purpose: Return Android capabilities: one window, no native file dialog, no VST host and a
    /// single system audio path.
    /// Inputs: none.
    /// Outputs / side effects: the descriptor; no state change.
    /// Complexity: O(1) time and space.
    pub const ANDROID: Self = Self {
        multi_window: false,
        window_drag: false,
        native_file_dialog: false,
        plugin_hosting: false,
        native_plugin_editor: false,
        system_audio_systems: false,
        gpu_ui_render: false,
        crash_trap: false,
    };
}

/// 文件对话框请求：把标题、过滤器、起始目录与父窗口打包成平台无关的输入。
/// File-dialog request: packs title, filters, start directory and parent window into a
/// platform-neutral input.
#[derive(Clone, Copy, Debug)]
pub struct FileRequest<'a> {
    /// 对话框标题。
    /// Dialog title.
    pub title: &'a str,
    /// 过滤器：`(显示名, 扩展名列表)`；扩展名不带点。
    /// Filters: `(label, extensions)`; extensions carry no dot.
    pub filters: &'a [(&'a str, &'a [&'a str])],
    /// 起始目录。
    /// Initial directory.
    pub start_dir: Option<&'a Path>,
    /// 另存为的默认文件名。
    /// Default file name for save-as.
    pub default_name: Option<&'a str>,
    /// 原生父窗口；`None` 表示不受理模态归属。
    /// Native parent window; `None` means no modality owner.
    pub parent: Option<DialogParent>,
}

impl<'a> FileRequest<'a> {
    /// 所属模块：platform / 后端接口 / 对话框
    /// 功能：构造一个只有标题的最小请求。
    /// 输入：`title` 对话框标题。
    /// 输出/副作用：请求结构；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / backend interface / dialogs
    /// Purpose: Build a minimal request that only carries a title.
    /// Inputs: `title`: dialog title.
    /// Outputs / side effects: the request; no state change.
    /// Complexity: O(1) time and space.
    pub fn titled(title: &'a str) -> Self {
        Self {
            title,
            filters: &[],
            start_dir: None,
            default_name: None,
            parent: None,
        }
    }
}

/// 平台后端：把各系统的存储、对话框、窗口与退出差异收敛到同一接口。
/// Platform backend: unify per-OS storage, dialogs, window and exit differences.
pub trait PlatformBackend: Send + Sync {
    /// 所属模块：platform / 后端接口
    /// 功能：返回后端标识。
    /// 输入：`self`。
    /// 输出/副作用：标识结构；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / backend interface
    /// Purpose: Return the backend identity.
    /// Inputs: `self`.
    /// Outputs / side effects: the descriptor; no state change.
    /// Complexity: O(1) time and space.
    fn info(&self) -> PlatformInfo;

    /// 所属模块：platform / 后端接口
    /// 功能：返回平台能力。
    /// 输入：`self`。
    /// 输出/副作用：能力结构；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / backend interface
    /// Purpose: Return the platform capabilities.
    /// Inputs: `self`.
    /// Outputs / side effects: the descriptor; no state change.
    /// Complexity: O(1) time and space.
    fn caps(&self) -> PlatformCaps;

    /// 所属模块：platform / 后端接口 / 存储
    /// 功能：返回用户配置目录。
    /// 输入：`self`。
    /// 输出/副作用：目录路径或 `None`；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / backend interface / storage
    /// Purpose: Return the user config directory.
    /// Inputs: `self`.
    /// Outputs / side effects: the path or `None`; nothing is created.
    /// Complexity: O(path length) time and space.
    fn config_dir(&self) -> Option<PathBuf>;

    /// 所属模块：platform / 后端接口 / 存储
    /// 功能：返回缓存目录。
    /// 输入：`self`。
    /// 输出/副作用：目录路径或 `None`；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / backend interface / storage
    /// Purpose: Return the cache directory.
    /// Inputs: `self`.
    /// Outputs / side effects: the path or `None`; nothing is created.
    /// Complexity: O(path length) time and space.
    fn cache_dir(&self) -> Option<PathBuf>;

    /// 所属模块：platform / 后端接口 / 存储
    /// 功能：返回应用私有数据目录。
    /// 输入：`self`。
    /// 输出/副作用：目录路径或 `None`；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / backend interface / storage
    /// Purpose: Return the app-private data directory.
    /// Inputs: `self`.
    /// Outputs / side effects: the path or `None`; nothing is created.
    /// Complexity: O(path length) time and space.
    fn data_dir(&self) -> Option<PathBuf>;

    /// 所属模块：platform / 后端接口 / 存储
    /// 功能：返回诊断日志目录。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / backend interface / storage
    /// Purpose: Return the diagnostic log directory.
    /// Inputs: `self`.
    /// Outputs / side effects: the path; nothing is created.
    /// Complexity: O(path length) time and space.
    fn log_dir(&self) -> PathBuf;

    /// 所属模块：platform / 后端接口 / 存储
    /// 功能：返回系统临时目录。
    /// 输入：`self`。
    /// 输出/副作用：目录路径；不创建目录。
    /// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
    ///
    /// Module: platform / backend interface / storage
    /// Purpose: Return the system temp directory.
    /// Inputs: `self`.
    /// Outputs / side effects: the path; nothing is created.
    /// Complexity: O(path length) time and space.
    fn temp_dir(&self) -> PathBuf;

    /// 所属模块：platform / 后端接口 / 对话框
    /// 功能：打开“选择文件”对话框；不支持时返回 `None`。
    /// 输入：`request` 对话框请求。
    /// 输出/副作用：阻塞直到用户选择；返回路径。
    /// 复杂度：时间取决于用户操作，空间 O(路径长度)。
    ///
    /// Module: platform / backend interface / dialogs
    /// Purpose: Show an open-file dialog; returns `None` when unsupported.
    /// Inputs: `request`: dialog request.
    /// Outputs / side effects: blocks until the user picks; returns the path.
    /// Complexity: user-bound time, O(path length) space.
    fn pick_file(&self, request: &FileRequest<'_>) -> Option<PathBuf>;

    /// 所属模块：platform / 后端接口 / 导入
    /// 功能：让用户选择一个文件用于导入，返回**可由 `std::fs` 读取的本地路径**。
    /// 输入：`request` 对话框请求（标题、过滤器、默认文件名）。
    /// 输出/副作用：桌面直接返回所选路径；安卓用 SAF 选择后复制到应用沙箱。
    /// 复杂度：时间取决于用户操作，空间 O(文件大小)（安卓复制时）。
    ///
    /// Module: platform / backend interface / import
    /// Purpose: Let the user pick a file to import and return a **local path readable by
    /// `std::fs`**.
    /// Inputs: `request`: dialog request (title, filters, default name).
    /// Outputs / side effects: desktop returns the picked path; Android copies the SAF document into
    /// the app sandbox.
    /// Complexity: user-bound time; O(file size) space on Android while copying.
    fn import_file(&self, request: &FileRequest<'_>) -> Result<Option<PathBuf>, String> {
        Ok(self.pick_file(request))
    }

    /// 所属模块：platform / 后端接口 / 导出
    /// 功能：把本地文件复制到用户选择的位置（公共目录）；取消时返回 `Ok(None)`。
    /// 输入：
    /// - `source`：本地源文件（应用沙箱内）。
    /// - `request`：对话框请求（默认文件名与过滤器决定 MIME）。
    /// 输出/副作用：桌面走“另存为”后复制；安卓走 SAF 新建文档后写入。
    /// 复杂度：时间取决于用户操作 + O(文件大小)，空间 O(缓冲区)。
    ///
    /// Module: platform / backend interface / export
    /// Purpose: Copy a local file to a user-chosen public location; `Ok(None)` means cancelled.
    /// Inputs:
    /// - `source`: local source file (inside the app sandbox).
    /// - `request`: dialog request (the default name and filters decide the MIME type).
    /// Outputs / side effects: desktop saves-as then copies; Android creates a SAF document then
    /// writes into it.
    /// Complexity: user-bound time + O(file size); O(buffer) space.
    fn export_copy(
        &self,
        source: &Path,
        request: &FileRequest<'_>,
    ) -> Result<Option<String>, String> {
        match self.save_file(request) {
            None => Ok(None),
            Some(target) => {
                std::fs::copy(source, &target).map_err(|error| format!("复制文件失败: {error}"))?;
                Ok(Some(target.display().to_string()))
            }
        }
    }

    /// 所属模块：platform / 后端接口 / 对话框
    /// 功能：打开“选择文件夹”对话框；不支持时返回 `None`。
    /// 输入：`request` 对话框请求。
    /// 输出/副作用：阻塞直到用户选择；返回目录。
    /// 复杂度：时间取决于用户操作，空间 O(路径长度)。
    ///
    /// Module: platform / backend interface / dialogs
    /// Purpose: Show a pick-folder dialog; returns `None` when unsupported.
    /// Inputs: `request`: dialog request.
    /// Outputs / side effects: blocks until the user picks; returns the directory.
    /// Complexity: user-bound time, O(path length) space.
    fn pick_folder(&self, request: &FileRequest<'_>) -> Option<PathBuf>;

    /// 所属模块：platform / 后端接口 / 对话框
    /// 功能：打开“另存为”对话框；不支持时返回 `None`。
    /// 输入：`request` 对话框请求。
    /// 输出/副作用：阻塞直到用户确认；返回目标路径。
    /// 复杂度：时间取决于用户操作，空间 O(路径长度)。
    ///
    /// Module: platform / backend interface / dialogs
    /// Purpose: Show a save-as dialog; returns `None` when unsupported.
    /// Inputs: `request`: dialog request.
    /// Outputs / side effects: blocks until the user confirms; returns the target path.
    /// Complexity: user-bound time, O(path length) space.
    fn save_file(&self, request: &FileRequest<'_>) -> Option<PathBuf>;

    /// 所属模块：platform / 后端接口 / 退出
    /// 功能：立即结束进程。
    /// 输入：`code` 进程退出码。
    /// 输出/副作用：结束进程；正常情况下不返回。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: platform / backend interface / exit
    /// Purpose: End the process immediately.
    /// Inputs: `code`: process exit code.
    /// Outputs / side effects: ends the process; normally never returns.
    /// Complexity: O(1) time and space.
    fn exit_process(&self, code: i32) -> !;
}
