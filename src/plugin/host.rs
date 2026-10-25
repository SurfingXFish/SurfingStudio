//! 所属模块：plugin / 插件运行时框架
//! 文件用途：
//! - 提供插件宿主运行时 `PluginHost`：应用层只调用框架方法。
//! - 维护插件格式后端注册表，统一分派加载与扫描。
//! - VST3 / VST2 作为后端，只实现各自格式的转译。
//!
//! 函数清单：
//! - `PluginHost::new` / `standard` / `Default::default`：运行时构造与默认后端注册。
//! - `PluginHost::backends` / `backend_for`：后端查询。
//! - `PluginHost::load` / `scan` / `scan_all`：框架核心方法。
//! - `PluginHost::load_plugin` / `scan_plugin_format` / `scan_plugins`：进程级默认入口。
//!
//! Module: plugin / plugin runtime framework
//! Purpose:
//! - Provide the plugin host runtime `PluginHost`: the app only calls framework methods.
//! - Maintain a registry of plugin format backends and dispatch loading/scanning.
//! - VST3 / VST2 are backends that only translate their own format.
//!
//! Functions:
//! - `PluginHost::new` / `standard` / `Default::default`: runtime construction and
//!   default backend registration.
//! - `PluginHost::backends` / `backend_for`: backend queries.
//! - `PluginHost::load` / `scan` / `scan_all`: core framework methods.
//! - `PluginHost::load_plugin` / `scan_plugin_format` / `scan_plugins`: process-wide entries.

use super::backend::{PluginBackend, PluginBackendInfo};
use super::backends::{Vst2Backend, Vst3Backend};
use super::{PluginRuntime, ScannedPlugin};
use crate::core::model::PluginFormat;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 插件运行时框架：持有格式后端注册表并暴露统一操作。
/// Plugin runtime framework: owns the format backend registry and unified operations.
pub struct PluginHost {
    backends: Vec<Box<dyn PluginBackend>>,
}

impl PluginHost {
    /// 所属模块：plugin / 插件运行时 / 构造
    /// 功能：使用给定的后端创建插件运行时。
    /// 输入：`backends` 插件格式后端列表。
    ///
    /// 输出/副作用：返回运行时；不修改外部状态。
    ///
    /// 复杂度：时间 O(后端数量)，空间 O(后端数量)。
    ///
    /// Module: plugin / plugin runtime / construction
    /// Purpose: Create a plugin runtime from the given backends.
    /// Inputs: `backends` — plugin format backend list.
    ///
    /// Outputs / side effects: returns the runtime; no external state change.
    ///
    /// Complexity: O(backend count) time and space.
    pub fn new(backends: Vec<Box<dyn PluginBackend>>) -> Self {
        Self { backends }
    }

    /// 所属模块：plugin / 插件运行时 / 构造
    /// 功能：创建标准插件运行时：注册 `Vst3Backend` 与 `Vst2Backend`。
    /// 输入：无。
    ///
    /// 输出/副作用：返回标准运行时。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / plugin runtime / construction
    /// Purpose: Create the standard plugin runtime with `Vst3Backend` and `Vst2Backend`.
    /// Inputs: none.
    ///
    /// Outputs / side effects: returns the standard runtime.
    ///
    /// Complexity: O(1) time and space.
    pub fn standard() -> Self {
        Self::new(vec![Box::new(Vst3Backend), Box::new(Vst2Backend)])
    }

    /// 所属模块：plugin / 插件运行时 / 查询
    /// 功能：返回全部后端能力描述。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回能力列表；不修改状态。
    ///
    /// 复杂度：时间 O(后端数量)，空间 O(后端数量)。
    ///
    /// Module: plugin / plugin runtime / query
    /// Purpose: Return capability descriptors of all backends.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the capability list; no state change.
    ///
    /// Complexity: O(backend count) time and space.
    pub fn backends(&self) -> Vec<PluginBackendInfo> {
        self.backends.iter().map(|backend| backend.info()).collect()
    }

    /// 所属模块：plugin / 插件运行时 / 查询
    /// 功能：按格式查找后端。
    /// 输入：`format` 插件格式。
    ///
    /// 输出/副作用：返回后端引用；不支持该格式时为 `None`。
    ///
    /// 复杂度：时间 O(后端数量)，空间 O(1)。
    ///
    /// Module: plugin / plugin runtime / query
    /// Purpose: Find the backend for a plugin format.
    /// Inputs: `format` — plugin format.
    ///
    /// Outputs / side effects: returns a backend reference, or `None` when unsupported.
    ///
    /// Complexity: O(backend count) time and O(1) space.
    fn backend_for(&self, format: PluginFormat) -> Option<&dyn PluginBackend> {
        self.backends
            .iter()
            .find(|backend| backend.info().format == format)
            .map(|backend| backend.as_ref())
    }

    /// 所属模块：plugin / 插件运行时 / 加载
    /// 功能：按格式分派加载插件。
    /// 输入：同 `PluginBackend::load`。
    ///
    /// 输出/副作用：返回统一插件运行时；不支持的格式返回错误。
    ///
    /// 复杂度：时间 O(插件初始化)，空间 O(插件状态)。
    ///
    /// Module: plugin / plugin runtime / loading
    /// Purpose: Dispatch plugin loading by format.
    /// Inputs: same as `PluginBackend::load`.
    ///
    /// Outputs / side effects: returns the unified runtime; unsupported formats error.
    ///
    /// Complexity: O(plugin initialization) time and O(plugin state) space.
    pub fn load(
        &self,
        format: PluginFormat,
        path: &Path,
        uid: &str,
        sample_rate: f64,
        block_size: usize,
        bpm: f64,
        time_sig: (u8, u8),
    ) -> Result<PluginRuntime, String> {
        let Some(backend) = self.backend_for(format) else {
            return Err(format!("不支持的插件格式: {}", format.label()));
        };
        backend
            .load(path, uid, sample_rate, block_size, bpm, time_sig)
            .map_err(|error| format!("{} 加载失败: {error}", backend.info().label))
    }

    /// 所属模块：plugin / 插件运行时 / 扫描
    /// 功能：扫描指定格式的插件。
    /// 输入：
    /// - `format`：插件格式。
    /// - `custom_paths`：自定义扫描目录。
    ///
    /// 输出/副作用：返回扫描结果；不支持的格式返回错误。
    ///
    /// 复杂度：时间 O(目录项数 × 最大递归深度)。
    ///
    /// Module: plugin / plugin runtime / scanning
    /// Purpose: Scan plugins of one format.
    /// Inputs:
    /// - `format`: plugin format.
    /// - `custom_paths`: custom scan directories.
    ///
    /// Outputs / side effects: returns scan results; unsupported formats error.
    ///
    /// Complexity: O(directory entries × max recursion depth) time.
    pub fn scan(
        &self,
        format: PluginFormat,
        custom_paths: &[PathBuf],
    ) -> Result<Vec<ScannedPlugin>, String> {
        let Some(backend) = self.backend_for(format) else {
            return Err(format!("不支持的插件格式: {}", format.label()));
        };
        backend
            .scan(custom_paths)
            .map_err(|error| format!("{} 扫描失败: {error}", backend.info().label))
    }

    /// 所属模块：plugin / 插件运行时 / 扫描
    /// 功能：按格式补齐单个插件的分类元数据。
    /// 输入：
    /// - `format`：插件格式。
    /// - `path`：插件路径。
    ///
    /// 输出/副作用：返回完整扫描记录；不支持或读取失败时为 `None`。
    ///
    /// 复杂度：时间 O(插件元数据读取)，空间 O(1)。
    ///
    /// Module: plugin / plugin runtime / scanning
    /// Purpose: Fill classification metadata for one plugin by format.
    /// Inputs:
    /// - `format`: plugin format.
    /// - `path`: plugin path.
    ///
    /// Outputs / side effects: a complete scan record, or `None` when unsupported/failed.
    ///
    /// Complexity: O(plugin metadata read) time and O(1) space.
    pub fn probe(&self, format: PluginFormat, path: &Path) -> Option<ScannedPlugin> {
        self.backend_for(format)?.probe(path)
    }

    /// 所属模块：plugin / 插件运行时 / 扫描
    /// 功能：扫描全部后端；至少一个后端成功时返回合并结果。
    /// 输入：`custom_paths` 自定义扫描目录。
    ///
    /// 输出/副作用：返回合并后的扫描结果；全部失败时返回汇总错误。
    ///
    /// 复杂度：时间 O(全部后端的目录扫描)，空间 O(插件数量)。
    ///
    /// Module: plugin / plugin runtime / scanning
    /// Purpose: Scan every backend; merge results when at least one backend succeeds.
    /// Inputs: `custom_paths` — custom scan directories.
    ///
    /// Outputs / side effects: returns merged scan results; aggregates errors when every
    /// backend fails.
    ///
    /// Complexity: O(total directory scanning) time and O(plugin count) space.
    pub fn scan_all(&self, custom_paths: &[PathBuf]) -> Result<Vec<ScannedPlugin>, String> {
        let mut plugins = Vec::new();
        let mut errors = Vec::new();
        for backend in &self.backends {
            if !backend.info().can_scan {
                continue;
            }
            match backend.scan(custom_paths) {
                Ok(mut found) => plugins.append(&mut found),
                Err(error) => errors.push(format!("{}: {error}", backend.info().label)),
            }
        }
        if plugins.is_empty() && !errors.is_empty() {
            Err(errors.join(" · "))
        } else {
            Ok(plugins)
        }
    }

    /// 所属模块：plugin / 插件运行时 / 进程级入口
    /// 功能：使用进程级默认运行时加载插件。
    /// 输入：同 `PluginHost::load`。
    ///
    /// 输出/副作用：返回统一插件运行时。
    ///
    /// 复杂度：时间 O(插件初始化)，空间 O(插件状态)。
    ///
    /// Module: plugin / plugin runtime / process-wide entry
    /// Purpose: Load a plugin through the process-wide default runtime.
    /// Inputs: same as `PluginHost::load`.
    ///
    /// Outputs / side effects: returns the unified plugin runtime.
    ///
    /// Complexity: O(plugin initialization) time and O(plugin state) space.
    #[allow(clippy::too_many_arguments)]
    pub fn load_plugin(
        format: PluginFormat,
        path: &Path,
        uid: &str,
        sample_rate: f64,
        block_size: usize,
        bpm: f64,
        time_sig: (u8, u8),
    ) -> Result<PluginRuntime, String> {
        standard_host().load(format, path, uid, sample_rate, block_size, bpm, time_sig)
    }

    /// 所属模块：plugin / 插件运行时 / 进程级入口
    /// 功能：使用进程级默认运行时扫描指定格式。
    /// 输入：
    /// - `format`：插件格式。
    /// - `custom_paths`：自定义扫描目录。
    ///
    /// 输出/副作用：返回扫描结果。
    ///
    /// 复杂度：时间 O(目录项数 × 最大递归深度)。
    ///
    /// Module: plugin / plugin runtime / process-wide entry
    /// Purpose: Scan one format through the process-wide default runtime.
    /// Inputs:
    /// - `format`: plugin format.
    /// - `custom_paths`: custom scan directories.
    ///
    /// Outputs / side effects: returns scan results.
    ///
    /// Complexity: O(directory entries × max recursion depth) time.
    pub fn scan_plugin_format(
        format: PluginFormat,
        custom_paths: &[PathBuf],
    ) -> Result<Vec<ScannedPlugin>, String> {
        standard_host().scan(format, custom_paths)
    }

    /// 所属模块：plugin / 插件运行时 / 进程级入口
    /// 功能：使用进程级默认运行时补齐单个插件的分类元数据。
    /// 输入：
    /// - `format`：插件格式。
    /// - `path`：插件路径。
    ///
    /// 输出/副作用：返回完整扫描记录；不支持或读取失败时为 `None`。
    ///
    /// 复杂度：时间 O(插件元数据读取)，空间 O(1)。
    ///
    /// Module: plugin / plugin runtime / process-wide entry
    /// Purpose: Fill one plugin's classification through the process-wide runtime.
    /// Inputs:
    /// - `format`: plugin format.
    /// - `path`: plugin path.
    ///
    /// Outputs / side effects: a complete scan record, or `None` when unsupported/failed.
    ///
    /// Complexity: O(plugin metadata read) time and O(1) space.
    pub fn probe_plugin_metadata(format: PluginFormat, path: &Path) -> Option<ScannedPlugin> {
        standard_host().probe(format, path)
    }

    /// 所属模块：plugin / 插件运行时 / 进程级入口
    /// 功能：使用进程级默认运行时扫描全部格式。
    /// 输入：`custom_paths` 自定义扫描目录。
    ///
    /// 输出/副作用：返回合并后的扫描结果。
    ///
    /// 复杂度：时间 O(全部后端的目录扫描)，空间 O(插件数量)。
    ///
    /// Module: plugin / plugin runtime / process-wide entry
    /// Purpose: Scan every format through the process-wide default runtime.
    /// Inputs: `custom_paths` — custom scan directories.
    ///
    /// Outputs / side effects: returns merged scan results.
    ///
    /// Complexity: O(total directory scanning) time and O(plugin count) space.
    pub fn scan_plugins(custom_paths: &[PathBuf]) -> Result<Vec<ScannedPlugin>, String> {
        standard_host().scan_all(custom_paths)
    }
}

impl Default for PluginHost {
    /// 所属模块：plugin / 插件运行时 / 构造
    /// 功能：创建标准插件运行时。
    /// 输入：无。
    ///
    /// 输出/副作用：返回标准运行时。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / plugin runtime / construction
    /// Purpose: Create the standard plugin runtime.
    /// Inputs: none.
    ///
    /// Outputs / side effects: returns the standard runtime.
    ///
    /// Complexity: O(1) time and space.
    fn default() -> Self {
        Self::standard()
    }
}

/// 所属模块：plugin / 插件运行时 / 进程级入口
/// 功能：返回进程级默认插件运行时，首次调用时注册标准后端。
/// 输入：无。
///
/// 输出/副作用：返回静态运行时引用；只初始化一次。
///
/// 复杂度：时间 O(1)（首次 O(1)），空间 O(1)。
///
/// Module: plugin / plugin runtime / process-wide entry
/// Purpose: Return the process-wide default runtime, registering standard backends on
/// first use.
/// Inputs: none.
///
/// Outputs / side effects: returns a static runtime reference; initialized once.
///
/// Complexity: O(1) time (O(1) on first use) and O(1) space.
fn standard_host() -> &'static PluginHost {
    static HOST: OnceLock<PluginHost> = OnceLock::new();
    HOST.get_or_init(PluginHost::standard)
}
