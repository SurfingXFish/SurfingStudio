//! 所属模块：app / 全局设置
//! 文件用途：
//! - 定义可持久化的全局设置：界面缩放、项目列表、存储、音频、控制器、渲染、插件与收藏。
//! - 读写本机配置文件（`%APPDATA%/SurfingStudio/settings.json` 或对应平台目录）。
//! - 绘制独立的两栏设置窗口：左侧八类分类、右侧当前分类内容。
//!
//! 函数清单：
//! - `AppSettings::load` / `save`：读取与写入配置文件（缺失或损坏时回退默认值）。
//! - `AppSettings::push_recent` / `remove_recent`：维护项目列表。
//! - `AppSettings::audio_options` / `gpu_audio_config`：把设置转换为音频与 GPU 渲染选项。
//! - `SettingsPage::label`：设置分类名称。
//! - `GpuRenderApi::id` / `label` / `from_id`：GPU 界面渲染 API 标识。
//! - `gpu_render_startup`：启动阶段读取 GPU 界面渲染偏好。
//! - `settings_path` / `default_cache_dir`：配置与缓存目录。
//! - `SurfingApp::open_settings`：打开独立设置窗口并置前。
//! - `SurfingApp::ui_settings_window`：创建独立视口并绘制设置内容。
//! - `SurfingApp::ui_settings_content`：两栏内容与分类切换。
//! - `SurfingApp::ui_settings_interface` / `ui_settings_storage` / `ui_settings_audio` /
//!   `ui_settings_render` / `ui_settings_controller` / `ui_settings_plugins` /
//!   `ui_settings_shortcuts` / `ui_settings_about`：八个分类页。
//! - `SurfingApp::capture_hotkey_input`：录制并保存可自定义快捷键。
//! - `SurfingApp::open_settings_folder_picker` / `poll_folder_picker`：后台文件夹选择。
//! - `SurfingApp::settings_gpu_catalog`：GPU 设备枚举与缓存。
//! - `settings_title_bar` / `settings_title_button`：自绘标题栏与窗口按钮。
//! - `settings_nav_item` / `settings_section` / `settings_hint` / `settings_field_label`：通用布局。
//! - `settings_combo` / `settings_action_button` / `settings_switch`：统一风格的控件。
//!
//! Module: app / global settings
//! Purpose:
//! - Define persistable global settings: UI scale, project list, storage, audio, controller,
//!   rendering, plugins and favourites.
//! - Read and write the local configuration file.
//! - Draw the standalone two-column settings window with eight categories.
//!
//! Functions:
//! - `AppSettings::load` / `save`: read and write the configuration (defaults when missing/corrupt).
//! - `AppSettings::push_recent` / `remove_recent`: maintain the project list.
//! - `AppSettings::audio_options` / `gpu_audio_config`: convert settings into audio and GPU render
//!   options.
//! - `SettingsPage::label`: settings category label.
//! - `GpuRenderApi::id` / `label` / `from_id`: GPU UI-render API identity.
//! - `gpu_render_startup`: read the GPU UI-render preference at startup.
//! - `settings_path` / `default_cache_dir`: config and cache directories.
//! - `SurfingApp::open_settings`: open and focus the standalone settings window.
//! - `SurfingApp::ui_settings_window`: create the standalone viewport and paint settings.
//! - `SurfingApp::ui_settings_content`: two-column content and category switching.
//! - `SurfingApp::ui_settings_interface` / `ui_settings_storage` / `ui_settings_audio` /
//!   `ui_settings_render` / `ui_settings_controller` / `ui_settings_plugins` /
//!   `ui_settings_shortcuts` / `ui_settings_about`: the eight category pages.
//! - `SurfingApp::capture_hotkey_input`: record and persist customisable shortcuts.
//! - `SurfingApp::open_settings_folder_picker` / `poll_folder_picker`: background folder picker.
//! - `SurfingApp::settings_gpu_catalog`: GPU device enumeration and caching.
//! - `settings_title_bar` / `settings_title_button`: self-drawn title bar and window buttons.
//! - `settings_nav_item` / `settings_section` / `settings_hint` / `settings_field_label`: shared layout.
//! - `settings_combo` / `settings_action_button` / `settings_switch`: consistently styled controls.

use super::shortcuts::{Binding, ShortcutAction, ShortcutGroup};
use super::*;
use crate::audio::backend::{self, AudioOptions, AudioSystem};
use crate::plugin::ScannedPlugin;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 设置窗口标题；同时用于定位原生窗口，让文件对话框附着在设置窗口上。
/// Settings window title; also used to resolve the native window so dialogs attach to it.
const SETTINGS_WINDOW_TITLE: &str = "设置 · SurfingStudio";
/// 设置窗口默认尺寸（逻辑点）。
/// Default size of the settings window in logical points.
const SETTINGS_WINDOW_W: f32 = 760.0;
/// 设置窗口默认高度（逻辑点）。
/// Default height of the settings window in logical points.
const SETTINGS_WINDOW_H: f32 = 620.0;
/// 设置窗口最小尺寸（逻辑点）。
/// Minimum size of the settings window in logical points.
const SETTINGS_MIN_W: f32 = 620.0;
/// 设置窗口最小高度（逻辑点）。
/// Minimum height of the settings window in logical points.
const SETTINGS_MIN_H: f32 = 460.0;
/// 默认界面缩放；与 `ui::theme::setup` 的初始缩放保持一致。
/// Default UI scale; matches the initial zoom factor applied by `ui::theme::setup`.
const DEFAULT_UI_SCALE: f32 = 1.15;
/// 设置窗口左侧分类栏宽度（逻辑点）。
/// Width of the settings category column in logical points.
const SETTINGS_NAV_W: f32 = 152.0;
/// 设置窗口左侧分类项高度（逻辑点）。
/// Height of one settings category entry in logical points.
const SETTINGS_NAV_ROW_H: f32 = 32.0;
/// 设置窗口自绘标题栏高度（逻辑点）。
/// Height of the self-drawn settings title bar in logical points.
const SETTINGS_TITLE_H: f32 = 38.0;
/// 设置窗口标题栏按钮尺寸（逻辑点）。
/// Size of the settings title-bar buttons in logical points.
const SETTINGS_TITLE_BUTTON: [f32; 2] = [30.0, 28.0];
/// 设置字段标签宽度（逻辑点）；控件与操作按钮按此宽度对齐。
/// Width of a settings field label; controls and action buttons align to it.
const SETTINGS_FIELD_W: f32 = 88.0;
/// 默认自动保存间隔（秒）。
/// Default autosave interval in seconds.
const DEFAULT_AUTOSAVE_SECONDS: u64 = 60;
/// 默认自动备份保留数量。
/// Default number of retained backups.
const DEFAULT_BACKUP_RETENTION: usize = 10;
/// 默认采样率。
/// Default sample rate.
const DEFAULT_SAMPLE_RATE: u32 = 48_000;
/// 默认缓冲帧数。
/// Default buffer size in frames.
const DEFAULT_BUFFER_FRAMES: u32 = 512;
/// 自动保存间隔的可选值（秒）。
/// Selectable autosave intervals in seconds.
const AUTOSAVE_OPTIONS: [u64; 6] = [15, 30, 60, 120, 300, 600];
/// 自动备份保留数量的可选值。
/// Selectable backup-retention counts.
const BACKUP_RETENTION_OPTIONS: [usize; 5] = [3, 5, 10, 20, 50];
/// 可请求的采样率选项。
/// Selectable sample rates.
const SAMPLE_RATE_OPTIONS: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];
/// 可请求的缓冲帧数选项。
/// Selectable buffer sizes in frames.
const BUFFER_FRAMES_OPTIONS: [u32; 6] = [128, 256, 512, 1024, 2048, 4096];
/// GPU 界面渲染可选 API：持久化标识与展示文本。
/// Selectable GPU UI-render APIs: persisted id and display text.
const DEFAULT_GPU_RENDER_API: &str = "auto";

/// 设置窗口左侧分类。
/// Settings categories shown in the left column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsPage {
    #[default]
    Interface,
    Storage,
    Audio,
    Render,
    Controller,
    Plugins,
    Shortcuts,
    About,
}

impl SettingsPage {
    pub const ALL: [SettingsPage; 8] = [
        SettingsPage::Interface,
        SettingsPage::Storage,
        SettingsPage::Audio,
        SettingsPage::Render,
        SettingsPage::Controller,
        SettingsPage::Plugins,
        SettingsPage::Shortcuts,
        SettingsPage::About,
    ];

    /// 所属模块：app / 全局设置
    /// 功能：返回分类的中文名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / global settings
    /// Purpose: Return the Chinese label of a settings category.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            SettingsPage::Interface => "界面",
            SettingsPage::Storage => "存储",
            SettingsPage::Audio => "音频",
            SettingsPage::Render => "渲染",
            SettingsPage::Controller => "控制器",
            SettingsPage::Plugins => "插件",
            SettingsPage::Shortcuts => "快捷键",
            SettingsPage::About => "软件信息",
        }
    }
}

/// 后台文件夹选择的目标。
/// Target of the background folder picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsFolderTarget {
    /// 自定义 VST 扫描目录。
    /// Custom VST scan directory.
    VstPath,
    /// 缓存目录。
    /// Cache directory.
    CacheDir,
    /// 默认存储目录。
    /// Default project directory.
    ProjectDir,
}

/// 可持久化的全局设置。
/// Persistable global settings.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    /// 界面缩放（1.0 = 100%）。
    /// UI scale (1.0 = 100%).
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    /// 打开或保存过的工程路径，最新的在最前；主页按文件保存时间排序。
    /// Every opened or saved project path, newest first; the home page sorts by file mtime.
    #[serde(default)]
    pub recent_projects: Vec<PathBuf>,
    /// 乐器窗口中收藏的乐器键（`builtin:...` 或 `plugin:路径`）。
    /// Favourite instrument keys in the instrument window (`builtin:...` or `plugin:path`).
    #[serde(default)]
    pub favorite_instruments: Vec<String>,
    /// 效果器窗口中收藏的插件键（`plugin:路径`）。
    /// Favourite plugin keys in the effect window (`plugin:path`).
    #[serde(default)]
    pub favorite_effects: Vec<String>,
    /// 自定义 VST 扫描目录；启动增量扫描与手动扫描都会包含这些路径。
    /// Custom VST scan directories; both the incremental startup scan and manual scans include them.
    #[serde(default)]
    pub custom_vst_paths: Vec<PathBuf>,
    /// 自动保存间隔（秒）；`0` 表示关闭自动保存。
    /// Autosave interval in seconds; `0` disables autosave.
    #[serde(default = "default_autosave_seconds")]
    pub autosave_seconds: u64,
    /// 缓存目录；`None` 使用系统默认目录。
    /// Cache directory; `None` uses the platform default.
    #[serde(default)]
    pub cache_dir: Option<PathBuf>,
    /// 默认存储地址；保存对话框从这里开始。
    /// Default project directory; the save dialog starts here.
    #[serde(default)]
    pub default_project_dir: Option<PathBuf>,
    /// 是否在保存时自动保留历史版本。
    /// Whether historical versions are kept when saving.
    #[serde(default = "default_true")]
    pub auto_backup: bool,
    /// 每个工程保留的历史版本数量。
    /// Number of historical versions kept per project.
    #[serde(default = "default_backup_retention")]
    pub backup_retention: usize,
    /// 音频系统标识：`system` / `wasapi` / `asio`。
    /// Audio-system id: `system` / `wasapi` / `asio`.
    #[serde(default = "default_audio_system")]
    pub audio_system: String,
    /// 启用的输出设备；`None` 使用系统默认设备。
    /// Enabled output device; `None` uses the system default.
    #[serde(default)]
    pub audio_output_device: Option<String>,
    /// 启用的输入设备；`None` 表示不指定。
    /// Enabled input device; `None` means unspecified.
    #[serde(default)]
    pub audio_input_device: Option<String>,
    /// 请求的采样率（Hz）。
    /// Requested sample rate in Hz.
    #[serde(default = "default_sample_rate")]
    pub audio_sample_rate: u32,
    /// 请求的缓冲帧数。
    /// Requested buffer size in frames.
    #[serde(default = "default_buffer_frames")]
    pub audio_buffer_frames: u32,
    /// 请求的输出声道数：`0` 跟随设备，`2` 立体声，`6` 5.1，`8` 7.1。
    /// Requested output channel count: `0` follows the device, `2` stereo, `6` 5.1, `8` 7.1.
    #[serde(default)]
    pub audio_output_channels: u32,
    /// 是否启用 GPU 界面渲染加速；关闭时使用 OpenGL 兼容路径。
    /// Whether GPU UI rendering is accelerated; off uses the OpenGL compatibility path.
    #[serde(default)]
    pub gpu_render_enabled: bool,
    /// 界面渲染图形 API：`auto` / `dx12` / `vulkan` / `opengl`；重启后生效。
    /// UI-render graphics API: `auto` / `dx12` / `vulkan` / `opengl`; applied after a restart.
    #[serde(default = "default_gpu_render_api")]
    pub gpu_render_api: String,
    /// 是否使用 GPU 加速音频渲染（离线导出与主总线终段）。
    /// Whether GPU audio rendering is enabled (offline export and the master-bus final stage).
    #[serde(default)]
    pub gpu_audio_enabled: bool,
    /// GPU 音频渲染使用的图形 API 标识；空字符串表示自动。
    /// Graphics-API id used by GPU audio rendering; empty means automatic.
    #[serde(default)]
    pub gpu_audio_api: String,
    /// GPU 音频渲染使用的设备键。
    /// Device key used by GPU audio rendering.
    #[serde(default)]
    pub gpu_audio_device: String,
    /// GPU 音频渲染使用的驱动展示文本；空字符串表示自动。
    /// Driver display text used by GPU audio rendering; empty means automatic.
    #[serde(default)]
    pub gpu_audio_driver: String,
    /// 是否让 GPU 参与实时播放的主总线软限幅；失败或超时自动回退 CPU。
    /// Whether the GPU limits the master bus during realtime playback; failures/timeouts fall back
    /// to the CPU.
    #[serde(default)]
    pub gpu_audio_realtime_enabled: bool,
    /// 控制器延迟偏移（毫秒），用于补偿控制器的输入延迟。
    /// Controller latency offset in milliseconds, compensating controller input delay.
    #[serde(default)]
    pub controller_latency_offset_ms: f32,
    /// 控制器启用的输入设备（MIDI 输入端口名）。
    /// Enabled controller input device (MIDI input port name).
    #[serde(default)]
    pub controller_input_device: Option<String>,
    /// 控制器启用的输出设备（MIDI 输出端口名）。
    /// Enabled controller output device (MIDI output port name).
    #[serde(default)]
    pub controller_output_device: Option<String>,
    /// 自定义快捷键：动作 id → 绑定文本（多个绑定用逗号分隔；缺失表示使用默认值）。
    /// Custom shortcuts: action id → binding text (several bindings separated by commas; a missing
    /// entry means the default binding applies).
    #[serde(default)]
    pub shortcuts: BTreeMap<String, String>,
}

impl AppSettings {
    /// 所属模块：app / 全局设置
    /// 功能：把音频设置转换为输出流启动选项。
    /// 输入：`self`。
    /// 输出/副作用：返回 `AudioOptions`；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / global settings
    /// Purpose: Convert the audio settings into output-stream options.
    /// Inputs: `self`.
    /// Outputs / side effects: `AudioOptions`; no state change.
    /// Complexity: O(1) time and space.
    pub fn audio_options(&self) -> AudioOptions {
        AudioOptions {
            system: AudioSystem::from_id(&self.audio_system),
            output_device: self.audio_output_device.clone(),
            sample_rate: Some(self.audio_sample_rate).filter(|rate| *rate >= 8_000),
            buffer_frames: Some(self.audio_buffer_frames).filter(|frames| *frames >= 32),
            output_channels: match self.audio_output_channels {
                2 | 6 | 8 => Some(self.audio_output_channels as u16),
                _ => None,
            },
        }
    }

    /// 所属模块：app / 全局设置
    /// 功能：把 GPU 音频渲染设置转换为引擎可用的配置。
    /// 输入：`self`。
    /// 输出/副作用：未启用时返回 `None`；否则返回设备/驱动/API 选择。
    /// 复杂度：时间 O(1)，空间 O(传输文本长度)。
    ///
    /// Module: app / global settings
    /// Purpose: Convert the GPU audio-render settings into an engine-facing config.
    /// Inputs: `self`.
    /// Outputs / side effects: `None` when disabled; otherwise the device/driver/API selection.
    /// Complexity: O(1) time and O(text length) space.
    pub fn gpu_audio_config(&self) -> Option<crate::audio::gpu::GpuRenderConfig> {
        self.gpu_audio_enabled.then(|| {
            crate::audio::gpu::GpuRenderConfig::new(
                &self.gpu_audio_api,
                &self.gpu_audio_device,
                &self.gpu_audio_driver,
            )
        })
    }

    /// 所属模块：app / 全局设置
    /// 功能：返回实时播放使用的 GPU 限幅配置；未启用实时加速时为 `None`。
    /// 输入：`self`。
    /// 输出/副作用：配置或 `None`；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(设备键长度)。
    ///
    /// Module: app / global settings
    /// Purpose: Return the GPU limiter config for realtime playback; `None` when realtime
    /// acceleration is disabled.
    /// Inputs: `self`.
    /// Outputs / side effects: the config or `None`; no state change.
    /// Complexity: O(1) time and O(device-key length) space.
    pub fn gpu_realtime_config(&self) -> Option<crate::audio::gpu::GpuRenderConfig> {
        self.gpu_audio_realtime_enabled.then(|| {
            crate::audio::gpu::GpuRenderConfig::new(
                &self.gpu_audio_api,
                &self.gpu_audio_device,
                &self.gpu_audio_driver,
            )
        })
    }

    /// 所属模块：app / 全局设置
    /// 功能：读取本机配置文件；文件缺失、损坏或数值越界时回退到默认值。
    /// 输入：无。
    /// 输出/副作用：返回设置快照；只读文件系统，不创建文件。
    /// 复杂度：时间 O(配置文件大小)，空间 O(项目数)。
    ///
    /// Module: app / global settings
    /// Purpose: Read the local configuration; fall back to defaults when the file is missing,
    /// corrupt or out of range.
    /// Inputs: none.
    /// Outputs / side effects: returns a settings snapshot; reads the file system, creates nothing.
    /// Complexity: O(config size) time and O(projects) space.
    pub fn load() -> Self {
        let mut settings = settings_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            // 兼容记事本等编辑器写入的 UTF-8 BOM，避免整份设置被回退为默认值。
            // Tolerate a UTF-8 BOM written by editors such as Notepad so the file is not discarded.
            .and_then(|text| serde_json::from_str::<Self>(text.trim_start_matches('\u{feff}')).ok())
            .unwrap_or_default();
        // 缩放越界（例如手工编辑过配置）时回退，避免界面缩放到不可用尺寸。
        // Fall back on an out-of-range scale (e.g. hand-edited config) so the UI stays usable.
        if !(0.8..=2.0).contains(&settings.ui_scale) {
            settings.ui_scale = DEFAULT_UI_SCALE;
        }
        if !(15..=3_600).contains(&settings.autosave_seconds) && settings.autosave_seconds != 0 {
            settings.autosave_seconds = DEFAULT_AUTOSAVE_SECONDS;
        }
        if !(1..=100).contains(&settings.backup_retention) {
            settings.backup_retention = DEFAULT_BACKUP_RETENTION;
        }
        settings.controller_latency_offset_ms =
            settings.controller_latency_offset_ms.clamp(-200.0, 200.0);
        // 未知或旧版音频系统标识统一归一到 `system`，避免下拉框显示与存储值不一致。
        // Normalize unknown/older audio-system ids to `system` so the combo matches the stored value.
        settings.audio_system = AudioSystem::from_id(&settings.audio_system)
            .id()
            .to_string();
        // 未知的 GPU API 标识回退为自动，避免下拉框显示空白。
        // Unknown GPU API ids fall back to automatic so the combo never shows a blank value.
        if !GpuRenderApi::ALL
            .iter()
            .any(|api| api.id() == settings.gpu_render_api)
        {
            settings.gpu_render_api = DEFAULT_GPU_RENDER_API.to_string();
        }
        if crate::audio::gpu::GpuApi::from_id(&settings.gpu_audio_api).is_none() {
            settings.gpu_audio_api.clear();
        }
        // 输出声道数只接受 0（跟随设备）/ 2 / 6 / 8；其它值回到自动，避免设备协商失败。
        // Output channels accept only 0 (follow the device) / 2 / 6 / 8; anything else becomes auto.
        if !matches!(settings.audio_output_channels, 0 | 2 | 6 | 8) {
            settings.audio_output_channels = 0;
        }
        settings
    }

    /// 所属模块：app / 全局设置
    /// 功能：把当前设置写入本机配置文件；失败时静默忽略（设置不是关键业务数据）。
    /// 输入：`self`。
    /// 输出/副作用：创建配置目录并写盘；不修改内存状态。
    /// 复杂度：时间 O(设置大小)，空间 O(设置大小)。
    ///
    /// Module: app / global settings
    /// Purpose: Write the current settings to the local configuration file; failures are ignored
    /// because settings are not critical business data.
    /// Inputs: `self`.
    /// Outputs / side effects: creates the config directory and writes the file; no in-memory change.
    /// Complexity: O(settings size) time and space.
    pub fn save(&self) {
        let Some(path) = settings_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, text);
        }
    }

    /// 所属模块：app / 全局设置
    /// 功能：把项目路径加入项目列表：去重（大小写不敏感）并置顶；不再截断，主页展示全部项目。
    /// 输入：`path`：刚打开或保存的工程路径。
    /// 输出/副作用：修改 `recent_projects`；不写盘（由调用方决定何时保存）。
    /// 复杂度：时间 O(项目数)，空间 O(1)。
    ///
    /// Module: app / global settings
    /// Purpose: Add a project path to the project list: de-duplicate (case-insensitive) and move to
    /// the front; no truncation because the home page shows every project.
    /// Inputs: `path`: the project just opened or saved.
    /// Outputs / side effects: mutates `recent_projects`; does not write the file (the caller saves).
    /// Complexity: O(projects) time and O(1) space.
    pub fn push_recent(&mut self, path: &std::path::Path) {
        let key = path.to_string_lossy().to_lowercase();
        self.recent_projects
            .retain(|entry| entry.to_string_lossy().to_lowercase() != key);
        self.recent_projects.insert(0, path.to_path_buf());
    }

    /// 所属模块：app / 全局设置
    /// 功能：从项目列表移除一个路径（例如文件已被删除）。
    /// 输入：`path`：要移除的路径。
    /// 输出/副作用：修改 `recent_projects`；返回是否移除了条目。
    /// 复杂度：时间 O(项目数)，空间 O(1)。
    ///
    /// Module: app / global settings
    /// Purpose: Remove one path from the project list (e.g. the file was deleted).
    /// Inputs: `path`: the path to remove.
    /// Outputs / side effects: mutates `recent_projects`; returns whether an entry was removed.
    /// Complexity: O(projects) time and O(1) space.
    pub fn remove_recent(&mut self, path: &std::path::Path) -> bool {
        let key = path.to_string_lossy().to_lowercase();
        let before = self.recent_projects.len();
        self.recent_projects
            .retain(|entry| entry.to_string_lossy().to_lowercase() != key);
        self.recent_projects.len() != before
    }
}

/// GPU 界面渲染使用的图形 API。
/// Graphics API used by accelerated UI rendering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GpuRenderApi {
    /// 自动：优先 DirectX 12 / Vulkan。
    /// Automatic: prefer DirectX 12 / Vulkan.
    #[default]
    Auto,
    DirectX12,
    Vulkan,
    /// OpenGL 兼容路径（默认的 glow 渲染器）。
    /// OpenGL compatibility path (the default glow renderer).
    OpenGl,
}

impl GpuRenderApi {
    /// 全部可选值。
    /// Every selectable value.
    pub const ALL: [GpuRenderApi; 4] = [
        GpuRenderApi::Auto,
        GpuRenderApi::DirectX12,
        GpuRenderApi::Vulkan,
        GpuRenderApi::OpenGl,
    ];

    /// 所属模块：app / 全局设置 / GPU 渲染
    /// 功能：返回持久化标识。
    /// 输入：`self`。
    /// 输出/副作用：静态字符串；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / global settings / GPU rendering
    /// Purpose: Return the persisted id.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn id(self) -> &'static str {
        match self {
            GpuRenderApi::Auto => "auto",
            GpuRenderApi::DirectX12 => "dx12",
            GpuRenderApi::Vulkan => "vulkan",
            GpuRenderApi::OpenGl => "opengl",
        }
    }

    /// 所属模块：app / 全局设置 / GPU 渲染
    /// 功能：返回面向用户的名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / global settings / GPU rendering
    /// Purpose: Return the display name.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            GpuRenderApi::Auto => "自动（DirectX 12 / Vulkan）",
            GpuRenderApi::DirectX12 => "DirectX 12",
            GpuRenderApi::Vulkan => "Vulkan",
            GpuRenderApi::OpenGl => "OpenGL（兼容）",
        }
    }

    /// 所属模块：app / 全局设置 / GPU 渲染
    /// 功能：把持久化标识还原为 API。
    /// 输入：`id`：设置中保存的标识。
    /// 输出/副作用：未知标识回退为自动。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / global settings / GPU rendering
    /// Purpose: Restore the API from its persisted id.
    /// Inputs: `id`: the id stored in the settings.
    /// Outputs / side effects: automatic for an unknown id.
    /// Complexity: O(1) time and space.
    pub fn from_id(id: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|api| api.id() == id)
            .unwrap_or(GpuRenderApi::Auto)
    }
}

/// 启动阶段从配置文件读取的 GPU 界面渲染偏好。
/// GPU UI-render preference read from the config at startup.
#[derive(Clone, Copy, Debug, Default)]
pub struct GpuRenderStartup {
    /// 是否开启 GPU 渲染加速。
    /// Whether GPU rendering acceleration is enabled.
    pub enabled: bool,
    /// 选定的图形 API。
    /// Selected graphics API.
    pub api: GpuRenderApi,
}

/// 所属模块：app / 全局设置 / GPU 渲染
/// 功能：读取配置文件中的 GPU 界面渲染偏好（主函数在创建窗口前调用）。
/// 输入：无。
/// 输出/副作用：读取配置文件；不修改内存状态。
/// 复杂度：时间 O(配置文件大小)，空间 O(设置大小)。
///
/// Module: app / global settings / GPU rendering
/// Purpose: Read the GPU UI-render preference from the config file (called before window creation).
/// Inputs: none.
/// Outputs / side effects: reads the config file; no in-memory change.
/// Complexity: O(config size) time and space.
pub fn gpu_render_startup() -> GpuRenderStartup {
    let settings = AppSettings::load();
    GpuRenderStartup {
        enabled: settings.gpu_render_enabled,
        api: GpuRenderApi::from_id(&settings.gpu_render_api),
    }
}

/// 所属模块：app / 全局设置
/// 功能：`ui_scale` 的 serde 默认值。
/// 输入：无。
/// 输出/副作用：返回默认缩放；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: serde default for `ui_scale`.
/// Inputs: none.
/// Outputs / side effects: default scale; no state change.
/// Complexity: O(1) time and space.
fn default_ui_scale() -> f32 {
    DEFAULT_UI_SCALE
}

/// 所属模块：app / 全局设置
/// 功能：`autosave_seconds` 的 serde 默认值。
/// 输入：无。
/// 输出/副作用：返回默认间隔；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: serde default for `autosave_seconds`.
/// Inputs: none.
/// Outputs / side effects: default interval; no state change.
/// Complexity: O(1) time and space.
fn default_autosave_seconds() -> u64 {
    DEFAULT_AUTOSAVE_SECONDS
}

/// 所属模块：app / 全局设置
/// 功能：`auto_backup` 的 serde 默认值。
/// 输入：无。
/// 输出/副作用：返回 `true`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: serde default for `auto_backup`.
/// Inputs: none.
/// Outputs / side effects: `true`; no state change.
/// Complexity: O(1) time and space.
fn default_true() -> bool {
    true
}

/// 所属模块：app / 全局设置
/// 功能：`backup_retention` 的 serde 默认值。
/// 输入：无。
/// 输出/副作用：返回默认保留数；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: serde default for `backup_retention`.
/// Inputs: none.
/// Outputs / side effects: default retention; no state change.
/// Complexity: O(1) time and space.
fn default_backup_retention() -> usize {
    DEFAULT_BACKUP_RETENTION
}

/// 所属模块：app / 全局设置
/// 功能：`audio_system` 的 serde 默认值。
/// 输入：无。
/// 输出/副作用：返回 `system`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: serde default for `audio_system`.
/// Inputs: none.
/// Outputs / side effects: `system`; no state change.
/// Complexity: O(1) time and space.
fn default_audio_system() -> String {
    AudioSystem::System.id().to_string()
}

/// 所属模块：app / 全局设置
/// 功能：`audio_sample_rate` 的 serde 默认值。
/// 输入：无。
/// 输出/副作用：返回默认采样率；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: serde default for `audio_sample_rate`.
/// Inputs: none.
/// Outputs / side effects: default sample rate; no state change.
/// Complexity: O(1) time and space.
fn default_sample_rate() -> u32 {
    DEFAULT_SAMPLE_RATE
}

/// 所属模块：app / 全局设置
/// 功能：`audio_buffer_frames` 的 serde 默认值。
/// 输入：无。
/// 输出/副作用：返回默认缓冲帧数；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: serde default for `audio_buffer_frames`.
/// Inputs: none.
/// Outputs / side effects: default buffer frames; no state change.
/// Complexity: O(1) time and space.
fn default_buffer_frames() -> u32 {
    DEFAULT_BUFFER_FRAMES
}

/// 所属模块：app / 全局设置
/// 功能：`gpu_render_api` 的 serde 默认值。
/// 输入：无。
/// 输出/副作用：返回默认 API 标识；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: serde default for `gpu_render_api`.
/// Inputs: none.
/// Outputs / side effects: the default API id; no state change.
/// Complexity: O(1) time and space.
fn default_gpu_render_api() -> String {
    DEFAULT_GPU_RENDER_API.to_string()
}

impl Default for AppSettings {
    /// 所属模块：app / 全局设置
    /// 功能：构造默认设置。
    /// 输入：无。
    /// 输出/副作用：返回默认设置；不读盘、不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / global settings
    /// Purpose: Build the default settings.
    /// Inputs: none.
    /// Outputs / side effects: default settings; no disk access or state change.
    /// Complexity: O(1) time and space.
    fn default() -> Self {
        Self {
            ui_scale: DEFAULT_UI_SCALE,
            recent_projects: Vec::new(),
            favorite_instruments: Vec::new(),
            favorite_effects: Vec::new(),
            custom_vst_paths: Vec::new(),
            autosave_seconds: DEFAULT_AUTOSAVE_SECONDS,
            cache_dir: None,
            default_project_dir: None,
            auto_backup: true,
            backup_retention: DEFAULT_BACKUP_RETENTION,
            audio_system: default_audio_system(),
            audio_output_device: None,
            audio_input_device: None,
            audio_sample_rate: DEFAULT_SAMPLE_RATE,
            audio_buffer_frames: DEFAULT_BUFFER_FRAMES,
            audio_output_channels: 0,
            gpu_render_enabled: false,
            gpu_render_api: default_gpu_render_api(),
            gpu_audio_enabled: false,
            gpu_audio_api: String::new(),
            gpu_audio_device: String::new(),
            gpu_audio_driver: String::new(),
            gpu_audio_realtime_enabled: false,
            controller_latency_offset_ms: 0.0,
            controller_input_device: None,
            controller_output_device: None,
            shortcuts: BTreeMap::new(),
        }
    }
}

/// 所属模块：app / 全局设置
/// 功能：返回本机配置文件路径（由平台运行时给出配置目录）。
/// 输入：无。
/// 输出/副作用：返回路径或 `None`；不创建目录。
/// 复杂度：时间 O(1)，空间 O(路径长度)。
///
/// Module: app / global settings
/// Purpose: Return the local configuration path (the platform runtime supplies the config dir).
/// Inputs: none.
/// Outputs / side effects: the path, or `None` when no user config directory can be resolved.
/// Complexity: O(1) time and O(path length) space.
pub(super) fn settings_path() -> Option<PathBuf> {
    crate::platform::config_dir().map(|base| base.join("settings.json"))
}

/// 所属模块：app / 全局设置
/// 功能：返回系统默认缓存目录（由平台运行时给出）。
/// 输入：无。
/// 输出/副作用：目录路径或 `None`；不创建目录。
/// 复杂度：时间 O(1)，空间 O(路径长度)。
///
/// Module: app / global settings
/// Purpose: Return the platform default cache directory (supplied by the platform runtime).
/// Inputs: none.
/// Outputs / side effects: the directory path or `None`; no directory is created.
/// Complexity: O(1) time and O(path length) space.
pub(super) fn default_cache_dir() -> Option<PathBuf> {
    crate::platform::cache_dir()
}

impl SurfingApp {
    /// 所属模块：app / 全局设置
    /// 功能：打开独立设置窗口；已打开时把窗口置前。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：设置 `settings_open` 与一次性的置前标记。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / global settings
    /// Purpose: Open the standalone settings window, or raise it when already open.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: sets `settings_open` and a one-shot bring-to-front flag.
    /// Complexity: O(1) time and space.
    pub(super) fn open_settings(&mut self, ctx: &egui::Context) {
        if self.settings_open {
            // 窗口已存在时直接发送置前命令，避免下一帧再请求一次。
            // The window already exists, so raise it immediately instead of next frame.
            ctx.send_viewport_cmd_to(settings_viewport_id(), egui::ViewportCommand::Focus);
        }
        self.settings_open = true;
        self.settings_focus = true;
    }

    /// 所属模块：app / 全局设置
    /// 功能：绘制独立设置窗口；关闭按钮或系统关闭请求都会结束窗口。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：读写 `settings_open`；应用界面缩放并保存设置；可启动插件扫描。
    /// 复杂度：每帧 O(设置控件数 + 插件数)。
    ///
    /// Module: app / global settings
    /// Purpose: Draw the standalone settings window; the close button and the OS close request
    /// both end it.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: reads/writes `settings_open`; applies the UI scale and saves settings;
    /// may start plugin scans.
    /// Complexity: O(settings widgets + plugins) per frame.
    pub(super) fn ui_settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        if !crate::platform::caps().multi_window {
            self.ui_settings_modal(ctx);
            return;
        }
        let viewport_id = settings_viewport_id();
        if self.settings_focus {
            self.settings_focus = false;
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Focus);
        }
        let mut open = true;
        ctx.show_viewport_immediate(
            viewport_id,
            egui::ViewportBuilder::default()
                .with_title(SETTINGS_WINDOW_TITLE)
                .with_inner_size([SETTINGS_WINDOW_W, SETTINGS_WINDOW_H])
                .with_min_inner_size([SETTINGS_MIN_W, SETTINGS_MIN_H])
                .with_resizable(true)
                // 与工程窗口一致：不显示系统标题栏，改由应用自绘应用栏。
                // Like the project windows: no OS title bar; the app draws its own bar.
                .with_decorations(false),
            |ui, _class| {
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    open = false;
                    return;
                }
                ui.painter()
                    .rect_filled(ui.max_rect(), egui::CornerRadius::from(0), theme::PANEL);
                if settings_title_bar(ui) {
                    open = false;
                }
                self.ui_settings_content(ui);
            },
        );
        if !open {
            self.settings_open = false;
        }
    }

    /// 所属模块：app / 全局设置
    /// 功能：在单窗口平台把设置绘制成根窗口内的模态面板（安卓没有独立窗口）。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：读写 `settings_open`；应用缩放并保存设置；点击遮罩、按 Esc 或关闭按钮结束面板。
    /// 复杂度：每帧 O(设置控件数 + 插件数)。
    ///
    /// Module: app / global settings
    /// Purpose: On single-window platforms paint the settings as a modal panel inside the root
    /// window (Android has no standalone windows).
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: reads/writes `settings_open`; applies the UI scale and saves settings;
    /// a backdrop click, Esc or the close button ends the panel.
    /// Complexity: O(settings widgets + plugins) per frame.
    fn ui_settings_modal(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let screen = ctx.screen_rect();
        let width = SETTINGS_WINDOW_W.min((screen.width() - 32.0).max(320.0));
        let height = SETTINGS_WINDOW_H.min((screen.height() - 32.0).max(320.0));
        let response = egui::Modal::new(egui::Id::new("settings_modal")).show(ctx, |ui| {
            ui.set_width(width);
            ui.set_height(height);
            ui.painter()
                .rect_filled(ui.max_rect(), egui::CornerRadius::from(0), theme::PANEL);
            if settings_title_bar(ui) {
                open = false;
            }
            self.ui_settings_content(ui);
        });
        if response.should_close() {
            open = false;
        }
        if !open {
            self.settings_open = false;
        }
    }

    /// 所属模块：app / 全局设置
    /// 功能：绘制设置两栏内容：左侧分类、右侧当前分类的具体设置。
    /// 输入：`ui`：设置窗口的内容区。
    /// 输出/副作用：修改并保存设置、应用缩放、切换音频设备、维护插件路径。
    /// 复杂度：每帧 O(设置控件数 + 插件数)。
    ///
    /// Module: app / global settings
    /// Purpose: Draw the two-column settings content: categories on the left and the selected
    /// category's options on the right.
    /// Inputs: `ui`: content area of the settings window.
    /// Outputs / side effects: edits and saves settings, applies the UI scale, switches audio
    /// devices and manages custom plugin paths.
    /// Complexity: O(settings widgets + plugins) per frame.
    fn ui_settings_content(&mut self, ui: &mut egui::Ui) {
        // 文件夹选择可能在切换到其它分类后完成，因此在两栏内容入口统一轮询。
        // The folder picker may finish after switching categories, so poll it at the content entry.
        self.poll_folder_picker();
        let body_height = (ui.available_height() - 10.0).max(220.0);
        ui.add_space(10.0);
        let mut page = if self.settings_page == SettingsPage::Plugins
            && !crate::platform::caps().plugin_hosting
        {
            // 不支持插件宿主的平台（安卓）没有插件设置页，落回界面页。
            // Platforms without plugin hosting (Android) have no plugin page; fall back.
            SettingsPage::Interface
        } else {
            self.settings_page
        };
        ui.horizontal_top(|ui| {
            ui.add_space(16.0);
            ui.allocate_ui_with_layout(
                egui::Vec2::new(SETTINGS_NAV_W, body_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(SETTINGS_NAV_W);
                    for entry in SettingsPage::ALL {
                        // 插件设置只在支持插件宿主的平台出现。
                        // Plugin settings only exist on platforms with plugin hosting.
                        if entry == SettingsPage::Plugins && !crate::platform::caps().plugin_hosting
                        {
                            continue;
                        }
                        if settings_nav_item(ui, entry, page == entry) {
                            page = entry;
                        }
                    }
                },
            );
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(12.0);
            // 固定内容区尺寸，ScrollArea 才能拿到确定高度并完整绘制所有分区。
            // Give the content column a definite size so the ScrollArea gets a real height and
            // paints every section instead of clipping after the first one.
            let content_width = (ui.available_width() - 18.0).max(240.0);
            ui.allocate_ui_with_layout(
                egui::Vec2::new(content_width, body_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("settings_content_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.add_space(2.0);
                            match page {
                                SettingsPage::Interface => self.ui_settings_interface(ui),
                                SettingsPage::Storage => self.ui_settings_storage(ui),
                                SettingsPage::Audio => self.ui_settings_audio(ui),
                                SettingsPage::Render => self.ui_settings_render(ui),
                                SettingsPage::Controller => self.ui_settings_controller(ui),
                                SettingsPage::Plugins => self.ui_settings_plugins(ui),
                                SettingsPage::Shortcuts => self.ui_settings_shortcuts(ui),
                                SettingsPage::About => self.ui_settings_about(ui),
                            }
                            ui.add_space(12.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "SurfingStudio {} · 设置保存在用户配置目录",
                                    env!("CARGO_PKG_VERSION")
                                ))
                                .size(10.0)
                                .color(theme::TEXT_FAINT),
                            );
                            ui.add_space(8.0);
                        });
                },
            );
        });
        self.settings_page = page;
    }

    /// 所属模块：app / 全局设置 / 界面
    /// 功能：绘制界面页：全局缩放及其对所有窗口的说明。
    /// 输入：`ui`：设置内容区。
    /// 输出/副作用：修改并保存界面缩放，立即应用到所有窗口。
    /// 复杂度：每帧 O(1)。
    ///
    /// Module: app / global settings / interface
    /// Purpose: Draw the interface page: global scale and its effect on every window.
    /// Inputs: `ui`: settings content area.
    /// Outputs / side effects: edits and saves the UI scale, applied to every window immediately.
    /// Complexity: O(1) per frame.
    fn ui_settings_interface(&mut self, ui: &mut egui::Ui) {
        settings_section(ui, "界面", |ui| {
            ui.horizontal(|ui| {
                settings_field_label(ui, "界面缩放");
                let mut scale = self.settings.ui_scale;
                settings_combo(
                    ui,
                    "settings_ui_scale",
                    format!("{:.0}%", scale * 100.0),
                    |ui| {
                        for option in [1.0_f32, 1.15, 1.3, 1.45] {
                            ui.selectable_value(
                                &mut scale,
                                option,
                                format!("{:.0}%", option * 100.0),
                            );
                        }
                    },
                );
                if (scale - self.settings.ui_scale).abs() > 1e-3 {
                    self.settings.ui_scale = scale;
                    self.commit_settings();
                    ui.ctx().set_zoom_factor(scale);
                }
            });
            settings_hint(ui, "缩放会立即应用到所有窗口、浮动卷帘与插件窗口。");
        });

        settings_section(ui, "GPU 渲染", |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("GPU 渲染加速")
                        .size(11.5)
                        .color(theme::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut enabled = self.settings.gpu_render_enabled;
                    if settings_switch(ui, &mut enabled) {
                        self.settings.gpu_render_enabled = enabled;
                        self.commit_settings();
                    }
                });
            });
            settings_hint(
                ui,
                "开启后界面使用 wgpu 渲染后端（DirectX 12 / Vulkan）；关闭时使用 OpenGL 兼容路径。重启软件后生效。",
            );
            ui.horizontal(|ui| {
                settings_field_label(ui, "图形 API");
                let mut api = GpuRenderApi::from_id(&self.settings.gpu_render_api);
                settings_combo(ui, "settings_gpu_render_api", api.label(), |ui| {
                    for option in GpuRenderApi::ALL {
                        ui.selectable_value(&mut api, option, option.label());
                    }
                });
                if api.id() != self.settings.gpu_render_api {
                    self.settings.gpu_render_api = api.id().to_string();
                    self.commit_settings();
                }
            });
            settings_hint(
                ui,
                "DirectX 12 / Vulkan 走 GPU 加速路径；OpenGL 兼容路径不受显卡驱动限制。",
            );
        });
    }

    /// 所属模块：app / 全局设置 / 存储
    /// 功能：绘制存储页：自动保存间隔、缓存位置、默认存储地址、自动备份开关与保留数。
    /// 输入：`ui`：设置内容区。
    /// 输出/副作用：修改并保存设置；打开后端文件夹选择器。
    /// 复杂度：每帧 O(1)。
    ///
    /// Module: app / global settings / storage
    /// Purpose: Draw the storage page: autosave interval, cache location, default project folder,
    /// auto-backup switch and retention.
    /// Inputs: `ui`: settings content area.
    /// Outputs / side effects: edits and saves settings; starts the background folder picker.
    /// Complexity: O(1) per frame.
    fn ui_settings_storage(&mut self, ui: &mut egui::Ui) {
        let mut picker: Option<(SettingsFolderTarget, &'static str)> = None;
        settings_section(ui, "自动保存", |ui| {
            ui.horizontal(|ui| {
                settings_field_label(ui, "间隔");
                let mut seconds = self.settings.autosave_seconds;
                let label = if seconds == 0 {
                    "关闭".to_string()
                } else {
                    format!("每 {seconds} 秒")
                };
                settings_combo(ui, "settings_autosave_interval", label, |ui| {
                    ui.selectable_value(&mut seconds, 0, "关闭");
                    for option in AUTOSAVE_OPTIONS {
                        ui.selectable_value(&mut seconds, option, format!("每 {option} 秒"));
                    }
                });
                if seconds != self.settings.autosave_seconds {
                    self.settings.autosave_seconds = seconds;
                    self.commit_settings();
                }
            });
            settings_hint(ui, "仅对已有文件路径且存在未保存修改的工程生效。");
        });

        settings_section(ui, "位置", |ui| {
            settings_path_value(
                ui,
                "缓存位置",
                &self
                    .settings
                    .cache_dir
                    .clone()
                    .or_else(default_cache_dir)
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "系统默认".into()),
                true,
            );
            ui.horizontal(|ui| {
                ui.add_space(SETTINGS_FIELD_W + 9.0);
                if settings_action_button(ui, "更改缓存位置…", "选择一个文件夹作为缓存目录")
                {
                    picker = Some((SettingsFolderTarget::CacheDir, "选择缓存目录"));
                }
                if self.settings.cache_dir.is_some()
                    && settings_action_button(ui, "恢复默认", "使用系统默认缓存目录")
                {
                    self.settings.cache_dir = None;
                    self.commit_settings();
                }
            });
            settings_hint(
                ui,
                "用于插件缓存等本机数据；更改后立即把当前插件缓存写入新目录。",
            );
            ui.add_space(8.0);
            settings_path_value(
                ui,
                "默认存储地址",
                &self
                    .settings
                    .default_project_dir
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "未设置（使用上次位置）".into()),
                false,
            );
            ui.horizontal(|ui| {
                ui.add_space(SETTINGS_FIELD_W + 9.0);
                if settings_action_button(
                    ui,
                    "更改默认存储地址…",
                    "选择保存/打开对话框的默认起始目录",
                ) {
                    picker = Some((SettingsFolderTarget::ProjectDir, "选择默认存储目录"));
                }
                if self.settings.default_project_dir.is_some()
                    && settings_action_button(ui, "清除", "不使用默认存储地址")
                {
                    self.settings.default_project_dir = None;
                    self.commit_settings();
                }
            });
            settings_hint(ui, "保存对话框会从该目录开始；不会移动已有工程。");
        });

        settings_section(ui, "历史版本", |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("保存时自动保留历史版本")
                        .size(11.5)
                        .color(theme::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut enabled = self.settings.auto_backup;
                    if settings_switch(ui, &mut enabled) {
                        self.settings.auto_backup = enabled;
                        self.commit_settings();
                    }
                });
            });
            settings_hint(
                ui,
                "开启后每次保存前把现有文件复制到工程目录的 Backups 文件夹。",
            );
            ui.horizontal(|ui| {
                settings_field_label(ui, "保留数量");
                let mut retention = self.settings.backup_retention;
                settings_combo(
                    ui,
                    "settings_backup_retention",
                    format!("{retention} 个"),
                    |ui| {
                        for option in BACKUP_RETENTION_OPTIONS {
                            ui.selectable_value(&mut retention, option, format!("{option} 个"));
                        }
                    },
                );
                if retention != self.settings.backup_retention {
                    self.settings.backup_retention = retention;
                    self.commit_settings();
                }
            });
            settings_hint(
                ui,
                "超过保留数量的最旧版本会自动删除；主页项目下拉可查看历史版本。",
            );
        });

        if let Some((target, title)) = picker {
            self.open_settings_folder_picker(title, target);
        }
    }

    /// 所属模块：app / 全局设置 / 音频
    /// 功能：绘制音频页：音频系统、驱动/输出设备、采样率、缓冲、启用的输入与输出设备。
    /// 输入：`ui`：设置内容区。
    /// 输出/副作用：修改并保存设置；变更后立即重启音频输出；打开驱动选择。
    /// 复杂度：每帧 O(设备数)。
    ///
    /// Module: app / global settings / audio
    /// Purpose: Draw the audio page: audio system, driver/output device, sample rate, buffer and the
    /// enabled input/output devices.
    /// Inputs: `ui`: settings content area.
    /// Outputs / side effects: edits and saves settings; restarts the audio output after a change.
    /// Complexity: O(devices) per frame.
    fn ui_settings_audio(&mut self, ui: &mut egui::Ui) {
        let systems = self.settings_audio_systems();
        let system = AudioSystem::from_id(&self.settings.audio_system);
        let (outputs, inputs) = self.settings_audio_devices(system);
        let mut changed = false;
        let mut refresh = false;

        settings_section(ui, "音频系统", |ui| {
            ui.horizontal(|ui| {
                settings_field_label(ui, "音频系统");
                let mut selected = self.settings.audio_system.clone();
                let current_label = system.label();
                settings_combo(ui, "settings_audio_system", current_label, |ui| {
                    for info in &systems {
                        let label = if info.available {
                            info.system.label().to_string()
                        } else {
                            format!("{}（不可用）", info.system.label())
                        };
                        ui.add_enabled_ui(info.available, |ui| {
                            ui.selectable_value(&mut selected, info.system.id().to_string(), label);
                        });
                    }
                });
                if selected != self.settings.audio_system {
                    self.settings.audio_system = selected;
                    self.settings.audio_output_device = None;
                    self.commit_settings();
                    changed = true;
                }
                if settings_action_button(ui, "刷新设备", "重新枚举音频系统与设备") {
                    refresh = true;
                }
            });
            if let Some(info) = systems.iter().find(|info| info.system == system) {
                settings_hint(ui, &info.detail);
            }
            if system == AudioSystem::Asio {
                settings_hint(
                    ui,
                    "ASIO 直接加载本机注册的驱动；可在驱动自身的控制面板中调整缓冲。",
                );
            }
        });

        settings_section(ui, "输出", |ui| {
            ui.horizontal(|ui| {
                settings_field_label(ui, "驱动 / 设备");
                let mut selected = self
                    .settings
                    .audio_output_device
                    .clone()
                    .unwrap_or_else(|| "系统默认设备".to_string());
                settings_combo(ui, "settings_audio_output", selected.clone(), |ui| {
                    ui.selectable_value(&mut selected, "系统默认设备".to_string(), "系统默认设备");
                    for device in &outputs {
                        ui.selectable_value(&mut selected, device.clone(), device);
                    }
                });
                let mapped = (selected != "系统默认设备").then_some(selected);
                if mapped != self.settings.audio_output_device {
                    self.settings.audio_output_device = mapped;
                    self.commit_settings();
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                settings_field_label(ui, "采样率");
                let mut rate = self.settings.audio_sample_rate;
                settings_combo(ui, "settings_audio_rate", format!("{rate} Hz"), |ui| {
                    for option in SAMPLE_RATE_OPTIONS {
                        ui.selectable_value(&mut rate, option, format!("{option} Hz"));
                    }
                });
                if rate != self.settings.audio_sample_rate {
                    self.settings.audio_sample_rate = rate;
                    self.commit_settings();
                    changed = true;
                }
                ui.add_space(16.0);
                ui.label(egui::RichText::new("缓冲").size(11.5).color(theme::TEXT));
                let mut frames = self.settings.audio_buffer_frames;
                settings_combo(ui, "settings_audio_buffer", format!("{frames} 帧"), |ui| {
                    for option in BUFFER_FRAMES_OPTIONS {
                        ui.selectable_value(&mut frames, option, format!("{option} 帧"));
                    }
                });
                if frames != self.settings.audio_buffer_frames {
                    self.settings.audio_buffer_frames = frames;
                    self.commit_settings();
                    changed = true;
                }
            });
            settings_hint(
                ui,
                "缓冲越小延迟越低，但更依赖驱动稳定性；ASIO 驱动会按自身粒度取整。",
            );
            ui.horizontal(|ui| {
                settings_field_label(ui, "输出声道");
                let mut channels = self.settings.audio_output_channels;
                let label = match channels {
                    2 => "立体声（2.0）".to_string(),
                    6 => "环绕声（5.1）".to_string(),
                    8 => "环绕声（7.1）".to_string(),
                    _ => "跟随设备".to_string(),
                };
                settings_combo(ui, "settings_audio_channels", label, |ui| {
                    ui.selectable_value(&mut channels, 0, "跟随设备");
                    ui.selectable_value(&mut channels, 2, "立体声（2.0）");
                    ui.selectable_value(&mut channels, 6, "环绕声（5.1）");
                    ui.selectable_value(&mut channels, 8, "环绕声（7.1）");
                });
                if channels != self.settings.audio_output_channels {
                    self.settings.audio_output_channels = channels;
                    self.commit_settings();
                    changed = true;
                }
            });
            settings_hint(
                ui,
                "环绕声需要输出设备声明对应声道数；设备不支持时会自动退回立体声，工程仍按所选布局渲染并导出。",
            );
        });

        settings_section(ui, "启用的输入设备", |ui| {
            if inputs.is_empty() {
                settings_hint(ui, "当前音频系统没有可枚举的输入设备。");
            } else {
                let mut selected = self
                    .settings
                    .audio_input_device
                    .clone()
                    .unwrap_or_else(|| "不启用".to_string());
                ui.horizontal(|ui| {
                    settings_field_label(ui, "输入设备");
                    settings_combo(ui, "settings_audio_input", selected.clone(), |ui| {
                        ui.selectable_value(&mut selected, "不启用".to_string(), "不启用");
                        for device in &inputs {
                            ui.selectable_value(&mut selected, device.clone(), device);
                        }
                    });
                });
                let mapped = (selected != "不启用").then_some(selected);
                if mapped != self.settings.audio_input_device {
                    self.settings.audio_input_device = mapped;
                    self.commit_settings();
                }
            }
        });

        settings_section(ui, "当前状态", |ui| {
            ui.label(
                egui::RichText::new(&self.audio_status)
                    .size(11.5)
                    .color(theme::TEXT),
            );
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("应用音频设置")
                                .size(11.5)
                                .color(egui::Color32::WHITE),
                        )
                        .fill(theme::ACCENT)
                        .min_size(egui::vec2(112.0, 26.0)),
                    )
                    .on_hover_text("停止并重新打开输出设备")
                    .clicked()
                {
                    changed = true;
                }
            });
        });

        if changed || refresh {
            if refresh {
                self.settings_audio_devices = None;
                self.settings_audio_systems = None;
            }
            if changed {
                let status = self.shared.restart_audio(&self.settings);
                self.audio_status = status;
            }
        }
    }

    /// 所属模块：app / 全局设置 / 渲染
    /// 功能：绘制渲染页：GPU 音频渲染开关、GPU 设备、驱动与图形 API 选择。
    /// 输入：`ui`：设置内容区。
    /// 输出/副作用：修改并保存设置；枚举/刷新 GPU 设备。
    /// 复杂度：每帧 O(GPU 设备数)。
    ///
    /// Module: app / global settings / rendering
    /// Purpose: Draw the rendering page: the GPU audio-render switch plus GPU device, driver and
    /// graphics-API selection.
    /// Inputs: `ui`: settings content area.
    /// Outputs / side effects: edits and saves settings; enumerates/refreshes GPU devices.
    /// Complexity: O(GPU devices) per frame.
    fn ui_settings_render(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let mut refresh = false;
        // 实时限幅状态由引擎持有；锁忙时按未启用显示，避免设置页抢锁。
        // The engine owns the realtime limiter state; a busy lock reads as disabled so settings never
        // contend for the lock.
        let realtime_active = self
            .engine
            .try_lock()
            .map(|engine| engine.gpu_realtime_active())
            .unwrap_or(false);
        settings_section(ui, "GPU 音频渲染", |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("使用 GPU 加速音频渲染")
                        .size(11.5)
                        .color(theme::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut enabled = self.settings.gpu_audio_enabled;
                    if settings_switch(ui, &mut enabled) {
                        self.settings.gpu_audio_enabled = enabled;
                        changed = true;
                    }
                });
            });
            settings_hint(ui, "离线导出时把主总线软限幅交给所选 GPU 计算。");
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("实时播放使用 GPU 软限幅")
                        .size(11.5)
                        .color(theme::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut enabled = self.settings.gpu_audio_realtime_enabled;
                    if settings_switch(ui, &mut enabled) {
                        self.settings.gpu_audio_realtime_enabled = enabled;
                        changed = true;
                    }
                });
            });
            settings_hint(
                ui,
                "实时路径在每块最多等待 1 ms；显卡未及时返回或不可用时立即回退 CPU，不会阻塞音频线程。",
            );
        });

        // 设备枚举可能初始化图形后端，只在进入页面与手动刷新时执行一次。
        // Device enumeration may initialise graphics backends, so run it once per page entry/refresh.
        let catalog = self.settings_gpu_catalog();
        let devices = catalog.clone().unwrap_or_default();
        let active = devices
            .iter()
            .find(|device| device.key() == self.settings.gpu_audio_device)
            .or_else(|| devices.first())
            .cloned();

        settings_section(ui, "设备与驱动", |ui| {
            ui.horizontal(|ui| {
                settings_field_label(ui, "GPU");
                let current_label = active
                    .as_ref()
                    .map(|device| format!("{}（{}）", device.family_name(), device.device_type))
                    .unwrap_or_else(|| "未检测到 GPU".into());
                let mut pick: Option<String> = None;
                settings_combo(ui, "settings_gpu_audio_device", current_label, |ui| {
                    let mut seen: Vec<&str> = Vec::new();
                    for device in &devices {
                        let family = device.family_name();
                        if seen.contains(&family) {
                            continue;
                        }
                        seen.push(family);
                        let selected = active
                            .as_ref()
                            .map(|current| current.family_name() == family)
                            .unwrap_or(false);
                        let label = format!("{}（{}）", family, device.device_type);
                        if ui.selectable_label(selected, label).clicked() {
                            pick = Some(family.to_string());
                        }
                    }
                });
                if settings_action_button(ui, "刷新设备", "重新枚举 GPU、驱动与图形 API")
                {
                    refresh = true;
                }
                if let Some(name) = pick {
                    // 切换显卡时优先保留当前 API，找不到再退回该显卡的第一项。
                    // Keep the current API when switching cards, otherwise take the first entry.
                    let preferred = active.as_ref().map(|device| device.api);
                    let device = devices
                        .iter()
                        .find(|device| {
                            device.family_name() == name
                                && preferred.map(|api| device.api == api).unwrap_or(true)
                        })
                        .or_else(|| devices.iter().find(|device| device.family_name() == name));
                    if let Some(device) = device {
                        self.settings.gpu_audio_device = device.key();
                        self.settings.gpu_audio_api = device.api.id().to_string();
                        self.settings.gpu_audio_driver = device.driver_label();
                        changed = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                settings_field_label(ui, "驱动");
                let current_label = active
                    .as_ref()
                    .map(|device| device.driver_label())
                    .unwrap_or_else(|| "自动".into());
                let mut pick: Option<String> = None;
                settings_combo(ui, "settings_gpu_audio_driver", current_label, |ui| {
                    let mut seen: Vec<String> = Vec::new();
                    for device in &devices {
                        let Some(current) = active.as_ref() else {
                            break;
                        };
                        if device.family_name() != current.family_name() {
                            continue;
                        }
                        let label = device.driver_label();
                        if seen.contains(&label) {
                            continue;
                        }
                        seen.push(label.clone());
                        if ui
                            .selectable_label(current.driver_label() == label, &label)
                            .clicked()
                        {
                            pick = Some(label);
                        }
                    }
                });
                if let Some(label) = pick {
                    if let Some(device) = devices.iter().find(|device| {
                        active
                            .as_ref()
                            .map(|current| {
                                device.family_name() == current.family_name()
                                    && device.driver_label() == label
                            })
                            .unwrap_or(false)
                    }) {
                        self.settings.gpu_audio_device = device.key();
                        self.settings.gpu_audio_api = device.api.id().to_string();
                        self.settings.gpu_audio_driver = label;
                        changed = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                settings_field_label(ui, "图形 API");
                let current_label = active
                    .as_ref()
                    .map(|device| device.api.label())
                    .unwrap_or("自动");
                let mut pick = None;
                settings_combo(ui, "settings_gpu_audio_api", current_label, |ui| {
                    let mut seen: Vec<crate::audio::gpu::GpuApi> = Vec::new();
                    for device in &devices {
                        let Some(current) = active.as_ref() else {
                            break;
                        };
                        if device.family_name() != current.family_name()
                            || seen.contains(&device.api)
                        {
                            continue;
                        }
                        seen.push(device.api);
                        if ui
                            .selectable_label(current.api == device.api, device.api.label())
                            .clicked()
                        {
                            pick = Some(device.api);
                        }
                    }
                });
                if let Some(api) = pick {
                    if let Some(device) = devices.iter().find(|device| {
                        device.api == api
                            && active
                                .as_ref()
                                .map(|current| device.family_name() == current.family_name())
                                .unwrap_or(false)
                    }) {
                        self.settings.gpu_audio_device = device.key();
                        self.settings.gpu_audio_api = api.id().to_string();
                        self.settings.gpu_audio_driver = device.driver_label();
                        changed = true;
                    }
                }
            });
        });

        settings_section(ui, "状态", |ui| match &catalog {
            Ok(devices) if !devices.is_empty() => {
                if let Some(device) = active.as_ref() {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {} · {}",
                            device.name,
                            device.api.label(),
                            device.driver_label()
                        ))
                        .size(11.0)
                        .color(theme::TEXT),
                    );
                }
                settings_hint(
                        ui,
                        "导出时按所选设备与 API 运行计算着色器；设备不可用时自动回退 CPU 并在状态栏提示。",
                    );
                let realtime_label = if !self.settings.gpu_audio_realtime_enabled {
                    "未启用"
                } else if realtime_active {
                    "GPU"
                } else {
                    "CPU 回退"
                };
                ui.label(
                    egui::RichText::new(format!("实时限幅：{realtime_label}"))
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                );
            }
            Ok(_) => settings_hint(ui, "未检测到可用的 GPU 计算设备，导出会回退到 CPU 渲染。"),
            Err(error) => {
                ui.label(egui::RichText::new(error).size(10.5).color(theme::RED));
            }
        });

        if refresh {
            self.settings_gpu_devices = None;
            let _ = self.settings_gpu_catalog();
        }
        if changed {
            self.commit_settings();
            // 实时 GPU 限幅只影响主总线终段，可立即应用到当前窗口；配置未变化时不会重建设备。
            // The realtime GPU limiter only affects the master-bus final stage, so apply it right
            // away; an unchanged config never rebuilds the device.
            if let Ok(mut engine) = self.engine.try_lock() {
                engine.configure_gpu_realtime(self.settings.gpu_realtime_config());
            }
        }
    }

    /// 所属模块：app / 全局设置 / 控制器
    /// 功能：绘制控制器页：延迟偏移、启用的 MIDI 输入与输出设备。
    /// 输入：`ui`：设置内容区。
    /// 输出/副作用：修改并保存设置；输入设备变更时立即重连 MIDI。
    /// 复杂度：每帧 O(MIDI 端口数)。
    ///
    /// Module: app / global settings / controller
    /// Purpose: Draw the controller page: latency offset and enabled MIDI input/output devices.
    /// Inputs: `ui`: settings content area.
    /// Outputs / side effects: edits and saves settings; reconnects MIDI when the input changes.
    /// Complexity: O(MIDI ports) per frame.
    fn ui_settings_controller(&mut self, ui: &mut egui::Ui) {
        let (inputs, outputs) = self.settings_midi_ports();
        let mut reconnect = false;
        settings_section(ui, "延迟偏移", |ui| {
            ui.horizontal(|ui| {
                settings_field_label(ui, "偏移");
                let mut offset = self.settings.controller_latency_offset_ms;
                ui.add_sized(
                    [260.0, 24.0],
                    egui::Slider::new(&mut offset, -200.0..=200.0).suffix(" ms"),
                );
                if (offset - self.settings.controller_latency_offset_ms).abs() > 0.01 {
                    self.settings.controller_latency_offset_ms = offset;
                    self.commit_settings();
                }
            });
            settings_hint(ui, "正数延后控制器事件，负数提前；用于补偿设备输入延迟。");
        });

        settings_section(ui, "启用的输入设备", |ui| {
            if inputs.is_empty() {
                settings_hint(ui, "未发现 MIDI 输入设备。");
            } else {
                let mut selected = self
                    .settings
                    .controller_input_device
                    .clone()
                    .unwrap_or_else(|| "自动（第一个可用端口）".to_string());
                ui.horizontal(|ui| {
                    settings_field_label(ui, "输入端口");
                    settings_combo(ui, "settings_controller_input", selected.clone(), |ui| {
                        ui.selectable_value(
                            &mut selected,
                            "自动（第一个可用端口）".to_string(),
                            "自动（第一个可用端口）",
                        );
                        for port in &inputs {
                            ui.selectable_value(&mut selected, port.clone(), port);
                        }
                    });
                });
                let mapped = (selected != "自动（第一个可用端口）").then_some(selected);
                if mapped != self.settings.controller_input_device {
                    self.settings.controller_input_device = mapped;
                    self.commit_settings();
                    reconnect = true;
                }
            }
            ui.label(
                egui::RichText::new(&self.midi_status)
                    .size(10.5)
                    .color(theme::TEXT_DIM),
            );
        });

        settings_section(ui, "启用的输出设备", |ui| {
            if outputs.is_empty() {
                settings_hint(ui, "未发现 MIDI 输出设备（当前版本不发送 MIDI 输出）。");
            } else {
                let mut selected = self
                    .settings
                    .controller_output_device
                    .clone()
                    .unwrap_or_else(|| "不启用".to_string());
                ui.horizontal(|ui| {
                    settings_field_label(ui, "输出端口");
                    settings_combo(ui, "settings_controller_output", selected.clone(), |ui| {
                        ui.selectable_value(&mut selected, "不启用".to_string(), "不启用");
                        for port in &outputs {
                            ui.selectable_value(&mut selected, port.clone(), port);
                        }
                    });
                });
                let mapped = (selected != "不启用").then_some(selected);
                if mapped != self.settings.controller_output_device {
                    self.settings.controller_output_device = mapped;
                    self.commit_settings();
                }
                settings_hint(ui, "记录启用的输出端口；后续版本用于外部同步与控制器反馈。");
            }
        });

        if reconnect {
            self.midi_status = self
                .shared
                .reconnect_midi(self.settings.controller_input_device.as_deref());
        }
    }

    /// 所属模块：app / 全局设置 / 插件
    /// 功能：绘制插件页：扫描目录列表、重新扫描与已扫描插件列表。
    /// 输入：`ui`：设置内容区。
    /// 输出/副作用：增删自定义扫描目录、启动扫描；只读插件缓存。
    /// 复杂度：每帧 O(自定义路径数 + 插件数)。
    ///
    /// Module: app / global settings / plugins
    /// Purpose: Draw the plugin page: scan-directory list, rescan and the scanned plugin list.
    /// Inputs: `ui`: settings content area.
    /// Outputs / side effects: adds/removes custom directories, starts scans; reads the plugin cache.
    /// Complexity: O(custom paths + plugins) per frame.
    fn ui_settings_plugins(&mut self, ui: &mut egui::Ui) {
        let mut add_path = false;
        let mut remove_path: Option<usize> = None;
        let mut rescan = false;
        settings_section(ui, "扫描目录", |ui| {
            ui.horizontal(|ui| {
                let add_button = egui::Button::new(
                    egui::RichText::new("添加文件夹…")
                        .size(11.0)
                        .color(theme::TEXT),
                )
                .fill(theme::PANEL_2)
                .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                .min_size(egui::vec2(0.0, 24.0));
                if ui
                    .add_enabled(self.folder_rx.is_none(), add_button)
                    .on_hover_text("选择默认目录之外的 VST3 / VST2 插件文件夹")
                    .clicked()
                {
                    add_path = true;
                }
                let rescan_button = egui::Button::new(
                    egui::RichText::new("重新扫描")
                        .size(11.0)
                        .color(theme::TEXT),
                )
                .fill(theme::PANEL_2)
                .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                .min_size(egui::vec2(0.0, 24.0));
                if ui
                    .add_enabled(!self.scanning, rescan_button)
                    .on_hover_text("扫描默认目录与下方自定义路径")
                    .clicked()
                {
                    rescan = true;
                }
                if self.scanning || self.folder_rx.is_some() {
                    ui.add(egui::Spinner::new().size(14.0));
                }
            });
            settings_hint(
                ui,
                "系统默认的 VST3 / VST2 目录始终参与扫描，下面只列出额外目录。",
            );
            ui.add_space(6.0);
            if self.settings.custom_vst_paths.is_empty() {
                ui.label(
                    egui::RichText::new("尚未添加自定义扫描目录")
                        .size(10.5)
                        .color(theme::TEXT_FAINT),
                );
            } else {
                for (index, path) in self.settings.custom_vst_paths.iter().enumerate() {
                    let text = path.display().to_string();
                    egui::Frame::NONE
                        .fill(theme::PANEL_2)
                        .corner_radius(4)
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&text)
                                            .size(10.5)
                                            .monospace()
                                            .color(theme::TEXT_DIM),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(&text);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if settings_action_button(ui, "移除", "从扫描目录中移除")
                                        {
                                            remove_path = Some(index);
                                        }
                                    },
                                );
                            });
                        });
                    ui.add_space(4.0);
                }
            }
        });

        settings_section(ui, "插件列表", |ui| {
            let total = self.plugin_cache.len();
            let instruments = self
                .plugin_cache
                .iter()
                .filter(|plugin| plugin.kind == crate::plugin::PluginKind::Instrument)
                .count();
            let effects = self
                .plugin_cache
                .iter()
                .filter(|plugin| plugin.kind == crate::plugin::PluginKind::Effect)
                .count();
            ui.label(
                egui::RichText::new(format!(
                    "共 {total} 个：乐器 {instruments} · 效果器 {effects} · 未知 {}",
                    total.saturating_sub(instruments + effects)
                ))
                .size(10.5)
                .color(theme::TEXT_FAINT),
            );
            ui.add_space(4.0);
            if total == 0 {
                settings_hint(ui, "还没有插件记录，点击“重新扫描”开始扫描。");
            } else {
                let height = 260.0_f32.min(ui.available_height().max(120.0));
                egui::ScrollArea::vertical()
                    .id_salt("settings_plugin_list")
                    .max_height(height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for plugin in &self.plugin_cache {
                            settings_plugin_row(ui, plugin);
                        }
                    });
            }
        });

        if add_path && self.folder_rx.is_none() {
            self.open_settings_folder_picker("选择 VST 插件文件夹", SettingsFolderTarget::VstPath);
        }
        if let Some(index) = remove_path {
            if index < self.settings.custom_vst_paths.len() {
                let removed = self.settings.custom_vst_paths.remove(index);
                self.commit_settings();
                // 同时移除该目录下已入库的插件记录，避免它们继续出现在选择窗。
                // Drop cached plugins under that directory so they disappear from the pickers.
                self.prune_plugin_cache_path(&removed);
            }
        }
        if rescan {
            self.start_scan();
        }
    }

    /// 所属模块：app / 全局设置 / 快捷键
    /// 功能：绘制快捷键页：按功能分组列出全部动作；点击绑定按钮后按下新键即可修改。
    /// 输入：`ui`：设置内容区。
    /// 输出/副作用：可能进入录制状态、写入 `settings.shortcuts` 并写盘；不修改工程。
    /// 复杂度：每帧 O(快捷键条数)。
    ///
    /// Module: app / global settings / shortcuts
    /// Purpose: Draw the shortcuts page: every action grouped by function; clicking a binding and
    /// pressing a key rebinds it.
    /// Inputs: `ui`: settings content area.
    /// Outputs / side effects: may start recording, write `settings.shortcuts` and persist it; the
    /// project is untouched.
    /// Complexity: O(shortcuts) per frame.
    fn ui_settings_shortcuts(&mut self, ui: &mut egui::Ui) {
        self.capture_hotkey_input(ui.ctx());
        let mut record: Option<ShortcutAction> = None;
        let mut reset: Option<ShortcutAction> = None;
        let mut reset_all = false;
        settings_section(ui, "快捷键", |ui| {
            settings_hint(
                ui,
                "点击右侧按键后按下新的组合键即可修改；Esc 取消，Delete / Backspace 清除绑定。",
            );
            if let Some(hint) = &self.hotkey_hint {
                ui.add_space(2.0);
                ui.label(egui::RichText::new(hint).size(10.5).color(theme::ACCENT));
            }
            ui.add_space(6.0);
            if settings_action_button(ui, "全部恢复默认", "恢复所有快捷键的默认绑定")
            {
                reset_all = true;
            }
            for group in ShortcutGroup::ALL {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(group.label())
                        .size(11.5)
                        .strong()
                        .color(theme::ACCENT),
                );
                for action in ShortcutAction::ALL
                    .iter()
                    .copied()
                    .filter(|action| action.group() == group)
                {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(action.label())
                                .size(11.0)
                                .color(theme::TEXT_DIM),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let customized = self.settings.shortcuts.contains_key(action.id());
                            if customized
                                && settings_action_button(ui, "重置", "恢复此动作的默认快捷键")
                            {
                                reset = Some(action);
                            }
                            let recording = self.hotkey_recording == Some(action);
                            let text = if recording {
                                "请按新键…".to_string()
                            } else {
                                self.settings.shortcut_label(action)
                            };
                            let button = egui::Button::new(
                                egui::RichText::new(text).size(10.5).monospace().color(
                                    if recording {
                                        theme::ACCENT
                                    } else {
                                        theme::TEXT
                                    },
                                ),
                            )
                            .fill(theme::PANEL_2)
                            .stroke(egui::Stroke::new(
                                1.0_f32,
                                if recording {
                                    theme::ACCENT
                                } else {
                                    theme::LINE
                                },
                            ))
                            .min_size(egui::vec2(128.0, 22.0));
                            if ui
                                .add(button)
                                .on_hover_text("点击后按下新的快捷键")
                                .clicked()
                            {
                                record = Some(action);
                            }
                        });
                    });
                }
            }
        });
        if let Some(action) = record {
            self.hotkey_recording = Some(action);
            self.hotkey_hint = None;
        }
        if let Some(action) = reset {
            self.settings.reset_shortcut(action);
            self.hotkey_hint = Some(format!("「{}」已恢复默认", action.label()));
            self.commit_settings();
        }
        if reset_all {
            self.settings.reset_all_shortcuts();
            self.hotkey_recording = None;
            self.hotkey_hint = Some("全部快捷键已恢复默认".into());
            self.commit_settings();
        }
    }

    /// 所属模块：app / 全局设置 / 快捷键
    /// 功能：录制新快捷键：读取本帧按键，写入设置并处理冲突/取消/清除。
    /// 输入：`ctx`：设置窗口的 egui 上下文。
    /// 输出/副作用：可能修改 `settings.shortcuts`、`hotkey_recording` 与 `hotkey_hint`；写盘。
    /// 复杂度：时间 O(本帧事件数 + 动作数 × 绑定数)，空间 O(1)。
    ///
    /// Module: app / global settings / shortcuts
    /// Purpose: Record a new shortcut: read this frame's key, store it and handle conflicts,
    /// cancellation and clearing.
    /// Inputs: `ctx`: egui context of the settings window.
    /// Outputs / side effects: may modify `settings.shortcuts`, `hotkey_recording` and
    /// `hotkey_hint`; writes the settings file.
    /// Complexity: O(events + actions × bindings) time and O(1) space.
    fn capture_hotkey_input(&mut self, ctx: &egui::Context) {
        let Some(action) = self.hotkey_recording else {
            return;
        };
        let pressed = ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } => Some((*key, *modifiers)),
                _ => None,
            })
        });
        let Some((key, modifiers)) = pressed else {
            return;
        };
        let plain = !modifiers.alt && !modifiers.shift && !modifiers.command;
        if key == egui::Key::Escape && plain {
            self.hotkey_recording = None;
            self.hotkey_hint = Some("已取消录制".into());
            return;
        }
        if (key == egui::Key::Delete || key == egui::Key::Backspace) && plain {
            self.settings.set_shortcut(action, &[]);
            self.hotkey_recording = None;
            self.hotkey_hint = Some(format!("「{}」已清除绑定", action.label()));
            self.commit_settings();
            return;
        }
        let binding = Binding::new(key, modifiers.command, modifiers.shift, modifiers.alt);
        if binding.text().contains("Unknown") {
            self.hotkey_hint = Some("该按键不支持，请换一个组合键".into());
            return;
        }
        if let Some(other) = self.settings.shortcut_conflict(action, binding) {
            self.hotkey_hint = Some(format!(
                "{} 已被「{}」使用，请换一个组合键",
                binding.label(),
                other.label()
            ));
            return;
        }
        let label = binding.label();
        self.settings.set_shortcut(action, &[binding]);
        self.hotkey_recording = None;
        self.hotkey_hint = Some(format!("「{}」已设为 {}", action.label(), label));
        self.commit_settings();
    }

    /// 所属模块：app / 全局设置 / 软件信息
    /// 功能：绘制软件信息页：名称、版本、配置与缓存位置、功能摘要。
    /// 输入：`ui`：设置内容区。
    /// 输出/副作用：只读静态文案；不修改状态。
    /// 复杂度：每帧 O(1)。
    ///
    /// Module: app / global settings / about
    /// Purpose: Draw the about page: name, version, config/cache locations and feature summary.
    /// Inputs: `ui`: settings content area.
    /// Outputs / side effects: static text only; no state change.
    /// Complexity: O(1) per frame.
    fn ui_settings_about(&mut self, ui: &mut egui::Ui) {
        settings_section(ui, "SurfingStudio", |ui| {
            ui.label(
                egui::RichText::new(format!("版本 {}", env!("CARGO_PKG_VERSION")))
                    .size(12.0)
                    .strong()
                    .color(theme::TEXT),
            );
            settings_hint(
                ui,
                "以 MIDI 为核心的轻量音乐工作站，内置合成器、鼓机与原生乐器。",
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("插件：VST3 / VST2（原生编辑器、状态持久化）")
                    .size(10.5)
                    .color(theme::TEXT_DIM),
            );
            ui.label(
                egui::RichText::new("音频：系统默认 / WASAPI / ASIO，可配置采样率与缓冲")
                    .size(10.5)
                    .color(theme::TEXT_DIM),
            );
            ui.label(
                egui::RichText::new("工程：.sfsd（SFS2 分块容器），兼容导入 SoundBug .sndp")
                    .size(10.5)
                    .color(theme::TEXT_DIM),
            );
        });

        settings_section(ui, "位置", |ui| {
            settings_path_value(
                ui,
                "配置文件",
                &settings_path()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "不可用".into()),
                false,
            );
            settings_path_value(
                ui,
                "插件缓存",
                &self
                    .settings
                    .cache_dir
                    .clone()
                    .or_else(default_cache_dir)
                    .map(|path| path.join("plugin-cache.json").display().to_string())
                    .unwrap_or_else(|| "系统默认".into()),
                false,
            );
            settings_path_value(ui, "历史版本", "工程目录 / Backups", false);
        });

        settings_section(ui, "致谢", |ui| {
            settings_hint(
                ui,
                "基于 egui / eframe、cpal、midir、vst3-host 与 Rust 社区生态构建。",
            );
        });
    }

    /// 所属模块：app / 全局设置 / 文件夹选择
    /// 功能：在后台线程打开原生文件夹对话框，结果由 `poll_folder_picker` 应用。
    /// 输入：
    /// - `title`：对话框标题。
    /// - `target`：选中目录的用途。
    /// 输出/副作用：设置 `folder_rx` 与 `settings_folder_target`；不阻塞 UI。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / global settings / folder picker
    /// Purpose: Open the native folder dialog on a worker thread; `poll_folder_picker` applies it.
    /// Inputs:
    /// - `title`: dialog title.
    /// - `target`: what the selected directory is used for.
    /// Outputs / side effects: sets `folder_rx` and `settings_folder_target`; never blocks the UI.
    /// Complexity: O(1) time and space.
    pub(super) fn open_settings_folder_picker(
        &mut self,
        title: &str,
        target: SettingsFolderTarget,
    ) {
        if self.folder_rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        // 对话框挂在设置窗口上：否则平台对话框会把主窗口带到前台，看起来像“跳回主页”。
        // Parent the dialog to the settings window; otherwise the platform dialog raises the main
        // window, which looks like jumping back to the home page.
        let parent = crate::platform::parent_by_title(SETTINGS_WINDOW_TITLE)
            .or_else(|| self.dialog_parent());
        let title = title.to_string();
        std::thread::spawn(move || {
            let request = crate::platform::FileRequest {
                parent,
                ..crate::platform::FileRequest::titled(&title)
            };
            let _ = tx.send(crate::platform::pick_folder(&request));
        });
        self.folder_rx = Some(rx);
        self.settings_folder_target = Some(target);
    }

    /// 所属模块：app / 全局设置 / 文件夹选择
    /// 功能：非阻塞接收后台文件夹选择结果，并按目标写入设置。
    /// 输入：`self`。
    /// 输出/副作用：可能更新自定义 VST 路径、缓存目录或默认存储地址；必要时启动扫描。
    /// 复杂度：时间 O(自定义路径数)，空间 O(1)。
    ///
    /// Module: app / global settings / folder picker
    /// Purpose: Non-blocking receive of the background folder picker and apply it to the settings.
    /// Inputs: `self`.
    /// Outputs / side effects: may update custom VST paths, the cache directory or the default
    /// project directory; may start a scan.
    /// Complexity: O(custom paths) time and O(1) space.
    pub(super) fn poll_folder_picker(&mut self) {
        let outcome = match &self.folder_rx {
            None => return,
            Some(receiver) => match receiver.try_recv() {
                Ok(folder) => Some(folder),
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(None),
            },
        };
        self.folder_rx = None;
        let target = self.settings_folder_target.take();
        let Some(Some(folder)) = outcome else {
            self.status = "已取消选择文件夹".into();
            return;
        };
        match target {
            Some(SettingsFolderTarget::CacheDir) => {
                self.settings.cache_dir = Some(folder.clone());
                self.commit_settings();
                // 立即把当前插件缓存写到新目录，避免下次启动前缓存位置看起来没有生效。
                // Write the current plugin cache to the new directory so the change takes effect now.
                self.publish_plugin_cache();
                self.status = format!("缓存位置已设为 {}", folder.display());
            }
            Some(SettingsFolderTarget::ProjectDir) => {
                self.settings.default_project_dir = Some(folder.clone());
                self.commit_settings();
                self.status = format!("默认存储地址已设为 {}", folder.display());
            }
            _ => {
                let key = folder.to_string_lossy().to_lowercase();
                if !self
                    .settings
                    .custom_vst_paths
                    .iter()
                    .any(|path| path.to_string_lossy().to_lowercase() == key)
                {
                    self.settings.custom_vst_paths.push(folder);
                    self.commit_settings();
                }
                self.start_scan();
            }
        }
    }

    /// 所属模块：app / 全局设置 / 设备枚举
    /// 功能：返回音频系统列表；首次进入音频页时枚举并缓存，手动刷新会清空缓存。
    /// 输入：`self`。
    /// 输出/副作用：可能写入 `settings_audio_systems`；只读宿主能力与注册表。
    /// 复杂度：时间 O(音频系统数)，空间 O(音频系统数)。
    ///
    /// Module: app / global settings / device enumeration
    /// Purpose: Return the audio systems; enumerate and cache them on first use, cleared by refresh.
    /// Inputs: `self`.
    /// Outputs / side effects: may fill `settings_audio_systems`; reads host capabilities and the registry.
    /// Complexity: O(audio systems) time and space.
    fn settings_audio_systems(&mut self) -> Vec<crate::audio::backend::AudioSystemInfo> {
        if let Some(cached) = &self.settings_audio_systems {
            return cached.clone();
        }
        let systems = backend::audio_systems();
        self.settings_audio_systems = Some(systems.clone());
        systems
    }

    /// 所属模块：app / 全局设置 / 设备枚举
    /// 功能：返回指定音频系统的输出与输入设备；按系统缓存，切换系统时重新枚举。
    /// 输入：`system` 音频系统。
    /// 输出/副作用：可能写入 `settings_audio_devices`；只读设备枚举。
    /// 复杂度：时间 O(设备数)，空间 O(设备数)。
    ///
    /// Module: app / global settings / device enumeration
    /// Purpose: Return the output/input devices of an audio system; cached per system.
    /// Inputs: `system` — audio system.
    /// Outputs / side effects: may fill `settings_audio_devices`; reads device enumeration only.
    /// Complexity: O(devices) time and space.
    fn settings_audio_devices(&mut self, system: AudioSystem) -> (Vec<String>, Vec<String>) {
        if let Some((cached_system, outputs, inputs)) = &self.settings_audio_devices {
            if *cached_system == system {
                return (outputs.clone(), inputs.clone());
            }
        }
        let outputs = backend::output_devices(system).unwrap_or_default();
        let inputs = backend::input_devices(system).unwrap_or_default();
        self.settings_audio_devices = Some((system, outputs.clone(), inputs.clone()));
        (outputs, inputs)
    }

    /// 所属模块：app / 全局设置 / 设备枚举
    /// 功能：返回 MIDI 输入与输出端口名；首次进入控制器页时枚举并缓存。
    /// 输入：`self`。
    /// 输出/副作用：可能写入 `settings_midi_ports`；只读 MIDI 端口。
    /// 复杂度：时间 O(端口数)，空间 O(端口数)。
    ///
    /// Module: app / global settings / device enumeration
    /// Purpose: Return the MIDI input and output port names; enumerate and cache on first use.
    /// Inputs: `self`.
    /// Outputs / side effects: may fill `settings_midi_ports`; reads MIDI ports only.
    /// Complexity: O(ports) time and space.
    fn settings_midi_ports(&mut self) -> (Vec<String>, Vec<String>) {
        if let Some(cached) = &self.settings_midi_ports {
            return cached.clone();
        }
        let inputs = crate::core::midi_input::MidiInputManager::port_names();
        let outputs = crate::core::midi_input::MidiInputManager::output_port_names();
        self.settings_midi_ports = Some((inputs.clone(), outputs.clone()));
        (inputs, outputs)
    }

    /// 所属模块：app / 全局设置 / 设备枚举
    /// 功能：返回可用的 GPU 设备；进入渲染页时枚举一次并缓存，手动刷新时重建。
    /// 输入：`self`。
    /// 输出/副作用：可能写入 `settings_gpu_devices`；只读图形后端。
    /// 复杂度：时间 O(枚举 API × 设备数)，空间 O(设备数)。
    ///
    /// Module: app / global settings / device enumeration
    /// Purpose: Return the available GPU devices; enumerated once on entering the render page and
    /// rebuilt on manual refresh.
    /// Inputs: `self`.
    /// Outputs / side effects: may fill `settings_gpu_devices`; reads graphics backends only.
    /// Complexity: O(APIs × devices) time and O(devices) space.
    fn settings_gpu_catalog(&mut self) -> Result<Vec<crate::audio::gpu::GpuDeviceInfo>, String> {
        if let Some(cached) = &self.settings_gpu_devices {
            return cached.clone();
        }
        let result = crate::audio::gpu::GpuRuntime::standard().devices();
        self.settings_gpu_devices = Some(result.clone());
        result
    }
}

/// 所属模块：app / 全局设置 / 标题栏
/// 功能：绘制设置窗口的自绘标题栏：标题、最小化与关闭按钮，并支持拖动窗口。
/// 输入：`ui`：设置窗口内容区。
/// 输出/副作用：点击关闭返回 `true`；拖动时发送 `StartDrag`，最小化时发送视口命令。
/// 复杂度：每帧 O(1)。
///
/// Module: app / global settings / title bar
/// Purpose: Draw the self-painted settings title bar (title, minimize and close) and support
/// dragging the window.
/// Inputs: `ui`: settings content area.
/// Outputs / side effects: returns `true` when close was clicked; a drag sends `StartDrag`, and
/// minimize sends a viewport command.
/// Complexity: O(1) per frame.
fn settings_title_bar(ui: &mut egui::Ui) -> bool {
    let (rect, drag) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), SETTINGS_TITLE_H),
        egui::Sense::click_and_drag(),
    );
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    let painter = ui.painter();
    painter.text(
        egui::pos2(rect.left() + 16.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        "设置",
        egui::FontId::proportional(14.5),
        theme::TEXT,
    );
    painter.text(
        egui::pos2(rect.left() + 58.0, rect.center().y + 1.0),
        egui::Align2::LEFT_CENTER,
        "SurfingStudio",
        egui::FontId::proportional(10.5),
        theme::TEXT_FAINT,
    );
    painter.hline(
        rect.left()..=rect.right(),
        rect.bottom(),
        egui::Stroke::new(1.0_f32, theme::APP_BAR_LINE),
    );

    let [button_w, button_h] = SETTINGS_TITLE_BUTTON;
    let button_top = rect.center().y - button_h / 2.0;
    let close_rect = egui::Rect::from_min_size(
        egui::pos2(rect.right() - 6.0 - button_w, button_top),
        egui::vec2(button_w, button_h),
    );
    let minimize_rect = egui::Rect::from_min_size(
        egui::pos2(close_rect.left() - 2.0 - button_w, button_top),
        egui::vec2(button_w, button_h),
    );
    let close = settings_title_button(ui, close_rect, ButtonIcon::Close, "关闭");
    if settings_title_button(ui, minimize_rect, ButtonIcon::Minimize, "最小化") {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
    close
}

/// 设置窗口标题栏按钮的图标。
/// Icon of a settings title-bar button.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ButtonIcon {
    Close,
    Minimize,
}

/// 所属模块：app / 全局设置 / 标题栏
/// 功能：绘制一个标题栏按钮（悬停底色 + 细线图标），关闭按钮使用红色悬停。
/// 输入：
/// - `ui`：设置窗口内容区。
/// - `rect`：按钮矩形。
/// - `icon`：关闭或最小化。
/// - `tooltip`：悬停提示。
/// 输出/副作用：返回是否被点击；不修改其他状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global settings / title bar
/// Purpose: Draw one title-bar button (hover fill + thin icon); close uses a red hover.
/// Inputs:
/// - `ui`: settings content area.
/// - `rect`: button rect.
/// - `icon`: close or minimize.
/// - `tooltip`: hover text.
/// Outputs / side effects: whether it was clicked; no other state change.
/// Complexity: O(1) time and space.
fn settings_title_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    icon: ButtonIcon,
    tooltip: &str,
) -> bool {
    let response = ui.interact(
        rect,
        ui.id().with(("settings_title_button", tooltip)),
        egui::Sense::click(),
    );
    let hovered = response.hovered();
    if hovered {
        let fill = if icon == ButtonIcon::Close {
            theme::CLOSE_HOVER
        } else {
            theme::APP_BAR_HOVER
        };
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::from(4), fill);
    }
    let color = if hovered {
        egui::Color32::WHITE
    } else {
        theme::APP_BAR_TEXT_DIM
    };
    let stroke = egui::Stroke::new(1.1_f32, color);
    let center = rect.center();
    match icon {
        ButtonIcon::Close => {
            ui.painter().line_segment(
                [
                    egui::pos2(center.x - 4.0, center.y - 4.0),
                    egui::pos2(center.x + 4.0, center.y + 4.0),
                ],
                stroke,
            );
            ui.painter().line_segment(
                [
                    egui::pos2(center.x + 4.0, center.y - 4.0),
                    egui::pos2(center.x - 4.0, center.y + 4.0),
                ],
                stroke,
            );
        }
        ButtonIcon::Minimize => {
            ui.painter().line_segment(
                [
                    egui::pos2(center.x - 4.0, center.y),
                    egui::pos2(center.x + 4.0, center.y),
                ],
                stroke,
            );
        }
    }
    response
        .on_hover_text(tooltip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// 所属模块：app / 全局设置
/// 功能：绘制左侧一个分类项，选中时使用强调色背景。
/// 输入：
/// - `ui`：左侧分类栏。
/// - `page`：分类。
/// - `selected`：是否为当前分类。
/// 输出/副作用：绘制并返回是否被点击。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: Draw one category entry with an accent background when selected.
/// Inputs:
/// - `ui`: left category column.
/// - `page`: category.
/// - `selected`: whether it is the active category.
/// Outputs / side effects: paints and returns whether it was clicked.
/// Complexity: O(1) time and space.
fn settings_nav_item(ui: &mut egui::Ui, page: SettingsPage, selected: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::Vec2::new(SETTINGS_NAV_W - 6.0, SETTINGS_NAV_ROW_H),
        egui::Sense::click(),
    );
    let hovered = response.hovered();
    if selected {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::from(4), theme::ACCENT_DARK);
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::from(4),
            egui::Stroke::new(1.0_f32, theme::ACCENT),
            egui::StrokeKind::Inside,
        );
    } else if hovered {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::from(4), theme::PANEL_3);
    }
    ui.painter().text(
        egui::pos2(rect.left() + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        page.label(),
        egui::FontId::proportional(12.0),
        if selected {
            egui::Color32::WHITE
        } else if hovered {
            theme::TEXT
        } else {
            theme::TEXT_DIM
        },
    );
    response.clicked()
}

/// 所属模块：app / 全局设置
/// 功能：绘制一个设置分区卡片（标题 + 内容）。
/// 输入：
/// - `ui`：设置滚动区。
/// - `title`：分区标题。
/// - `add`：分区内容的绘制闭包。
/// 输出/副作用：绘制卡片；不修改应用状态（闭包内自行处理）。
/// 复杂度：每帧 O(1)。
///
/// Module: app / global settings
/// Purpose: Draw one settings section card (title + content).
/// Inputs:
/// - `ui`: settings scroll area.
/// - `title`: section title.
/// - `add`: closure that draws the section content.
/// Outputs / side effects: paints the card; no app state change (the closure owns its changes).
/// Complexity: O(1) per frame.
fn settings_section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(theme::PANEL)
        .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(title)
                    .size(12.5)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.add_space(8.0);
            add(ui);
        });
    ui.add_space(10.0);
}

/// 所属模块：app / 全局设置
/// 功能：绘制一行弱提示文本。
/// 输入：
/// - `ui`：设置内容区。
/// - `text`：提示文本。
/// 输出/副作用：只绘制文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: Draw one muted hint line.
/// Inputs:
/// - `ui`: settings content area.
/// - `text`: hint text.
/// Outputs / side effects: paints text only; no state change.
/// Complexity: O(1) time and space.
fn settings_hint(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(10.5).color(theme::TEXT_DIM));
}

/// 所属模块：app / 全局设置
/// 功能：绘制固定宽度的字段标签，让同一分区内的控件左侧对齐。
/// 输入：
/// - `ui`：设置内容区。
/// - `text`：字段名。
/// 输出/副作用：分配 88 点宽并绘制文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: Draw a fixed-width field label so controls in one section align on the left.
/// Inputs:
/// - `ui`: settings content area.
/// - `text`: field name.
/// Outputs / side effects: allocates 88 points and paints text; no state change.
/// Complexity: O(1) time and space.
fn settings_field_label(ui: &mut egui::Ui, text: &str) {
    ui.allocate_ui_with_layout(
        egui::Vec2::new(SETTINGS_FIELD_W, 24.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(SETTINGS_FIELD_W);
            ui.label(egui::RichText::new(text).size(11.5).color(theme::TEXT));
        },
    );
}

/// 所属模块：app / 全局设置
/// 功能：绘制一个带边框的设置操作按钮（添加、更改、恢复默认等）。
/// 输入：
/// - `ui`：设置内容区。
/// - `label`：按钮文本。
/// - `tooltip`：悬停提示。
/// 输出/副作用：返回是否被点击；不修改其他状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: Draw an outlined settings action button (add, change, restore defaults, …).
/// Inputs:
/// - `ui`: settings content area.
/// - `label`: button text.
/// - `tooltip`: hover text.
/// Outputs / side effects: whether it was clicked; no other state change.
/// Complexity: O(1) time and space.
fn settings_action_button(ui: &mut egui::Ui, label: &str, tooltip: &str) -> bool {
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(11.0).color(theme::TEXT))
            .fill(theme::PANEL_2)
            .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
            .min_size(egui::vec2(0.0, 24.0)),
    )
    .on_hover_text(tooltip)
    .on_hover_cursor(egui::CursorIcon::PointingHand)
    .clicked()
}

/// 所属模块：app / 全局设置
/// 功能：绘制带边框的下拉框，让它在 PANEL 分区卡片上始终可见。
/// 输入：
/// - `ui`：设置内容区。
/// - `id`：稳定的控件 id。
/// - `selected`：当前显示文本。
/// - `add`：下拉项绘制闭包。
/// 输出/副作用：绘制控件；选项由闭包处理。
/// 复杂度：时间 O(下拉项数)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: Draw an outlined combo box so it stays visible on a PANEL section card.
/// Inputs:
/// - `ui`: settings content area.
/// - `id`: stable widget id.
/// - `selected`: current text.
/// - `add`: closure that draws the dropdown items.
/// Outputs / side effects: paints the widget; the closure handles the options.
/// Complexity: O(items) time and O(1) space.
fn settings_combo(
    ui: &mut egui::Ui,
    id: &str,
    selected: impl Into<egui::WidgetText>,
    add: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::NONE
        .fill(theme::PANEL_2)
        .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            egui::ComboBox::from_id_salt(id)
                .selected_text(selected)
                .show_ui(ui, add);
        });
}

/// 所属模块：app / 全局设置
/// 功能：绘制 28×14 的开关，与效果器卡片启用开关保持一致。
/// 输入：
/// - `ui`：设置内容区。
/// - `value`：当前开关状态；点击时取反。
/// 输出/副作用：返回是否被点击；点击时修改 `value`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: Draw a 28×14 switch matching the effect-card enable toggle.
/// Inputs:
/// - `ui`: settings content area.
/// - `value`: current state; toggled on click.
/// Outputs / side effects: whether it was clicked; mutates `value` on click.
/// Complexity: O(1) time and space.
fn settings_switch(ui: &mut egui::Ui, value: &mut bool) -> bool {
    theme::toggle_switch(ui, value).clicked()
}

/// 所属模块：app / 全局设置
/// 功能：绘制“标签 + 路径值”一行。
/// 输入：
/// - `ui`：设置内容区。
/// - `label`：字段名。
/// - `value`：路径展示文本。
/// - `monospace`：是否用等宽字体（缓存/配置路径用）。
/// 输出/副作用：只绘制文本；不修改状态。
/// 复杂度：时间 O(路径长度)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: Draw one label + path value row.
/// Inputs:
/// - `ui`: settings content area.
/// - `label`: field name.
/// - `value`: path text.
/// - `monospace`: whether to use a monospace font (cache/config paths).
/// Outputs / side effects: paints text only; no state change.
/// Complexity: O(path length) time and O(1) space.
fn settings_path_value(ui: &mut egui::Ui, label: &str, value: &str, monospace: bool) {
    ui.horizontal(|ui| {
        settings_field_label(ui, label);
        let mut text = egui::RichText::new(value).size(10.5).color(theme::TEXT_DIM);
        if monospace {
            text = text.monospace();
        }
        // 长路径按剩余宽度截断，悬停显示完整路径，避免顶出卡片右边界。
        // Truncate long paths to the remaining width and show the full path on hover.
        ui.add(egui::Label::new(text).truncate())
            .on_hover_text(value);
    });
}

/// 所属模块：app / 全局设置
/// 功能：绘制一行插件记录：名称、类型、格式与厂商。
/// 输入：
/// - `ui`：插件列表。
/// - `plugin`：扫描记录。
/// 输出/副作用：分配整行并绘制；不修改状态。
/// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
///
/// Module: app / global settings
/// Purpose: Draw one plugin row: name, kind, format and vendor.
/// Inputs:
/// - `ui`: plugin list.
/// - `plugin`: scan record.
/// Outputs / side effects: allocates and paints the row; no state change.
/// Complexity: O(text length) time and space.
fn settings_plugin_row(ui: &mut egui::Ui, plugin: &ScannedPlugin) {
    let frame = egui::Frame::NONE
        .fill(theme::PANEL_2)
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                // 名称在前、标签在后，与主页插件列表保持一致。
                // Name first, tag afterwards, matching the home plugin list.
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&plugin.name)
                            .size(11.0)
                            .color(theme::TEXT),
                    )
                    .truncate(),
                );
                theme::badge(
                    ui,
                    plugin.kind.label(),
                    if plugin.kind == crate::plugin::PluginKind::Instrument {
                        theme::ACCENT
                    } else {
                        theme::AMBER
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(plugin.format.label())
                            .size(9.5)
                            .monospace()
                            .color(theme::TEXT_FAINT),
                    );
                    if !plugin.vendor.is_empty() {
                        ui.label(
                            egui::RichText::new(&plugin.vendor)
                                .size(9.5)
                                .color(theme::TEXT_FAINT),
                        );
                    }
                });
            });
        });
    frame
        .response
        .on_hover_text(plugin.path.display().to_string());
    ui.add_space(4.0);
}

/// 所属模块：app / 全局设置
/// 功能：返回设置窗口的视口 id。
/// 输入：无。
/// 输出/副作用：返回稳定的视口 id；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / global settings
/// Purpose: Return the settings window viewport id.
/// Inputs: none.
/// Outputs / side effects: stable viewport id; no state change.
/// Complexity: O(1) time and space.
fn settings_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("settings_window")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：旧版配置文件缺少新字段时按默认值补齐。
    /// Covers: an older settings file without the new fields falls back to the defaults.
    #[test]
    fn older_settings_files_receive_defaults() {
        let settings: AppSettings =
            serde_json::from_str(r#"{"ui_scale":1.0}"#).expect("parse old settings");
        assert!((settings.ui_scale - 1.0).abs() < 1e-6);
        assert_eq!(settings.autosave_seconds, DEFAULT_AUTOSAVE_SECONDS);
        assert_eq!(settings.backup_retention, DEFAULT_BACKUP_RETENTION);
        assert!(settings.auto_backup);
        assert_eq!(settings.audio_system, "system");
        assert_eq!(settings.audio_sample_rate, DEFAULT_SAMPLE_RATE);
        assert_eq!(settings.audio_buffer_frames, DEFAULT_BUFFER_FRAMES);
        assert!(settings.audio_output_device.is_none());
        assert!(settings.controller_input_device.is_none());
    }

    /// 覆盖：音频设置转换为启动选项时保留系统、设备、采样率与缓冲。
    /// Covers: audio options keep the system, device, sample rate and buffer.
    #[test]
    fn audio_options_follow_the_settings() {
        let mut settings = AppSettings::default();
        settings.audio_system = "asio".into();
        settings.audio_output_device = Some("ASIO4ALL v2".into());
        settings.audio_sample_rate = 96_000;
        settings.audio_buffer_frames = 256;
        let options = settings.audio_options();
        assert_eq!(options.system, AudioSystem::Asio);
        assert_eq!(options.output_device.as_deref(), Some("ASIO4ALL v2"));
        assert_eq!(options.sample_rate, Some(96_000));
        assert_eq!(options.buffer_frames, Some(256));
    }
}
