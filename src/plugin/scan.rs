//! 所属模块：plugin / 插件扫描
//! 文件用途：
//! - 共享插件发现工具：VST3 bundle 元数据读取、VST2 DLL 探测与目录枚举。
//! - 用内置插件资料库补全缺失的厂商、分类与乐器/效果器角色。
//! 函数清单：
//! - `enrich_from_catalog`：资料库补全。
//! - `From<vst3_host::PluginInfo>`：VST3 探测结果转换。
//! - `scan_vst3` / `scan_legacy_vst`：按格式扫描。
//! - `list_plugin_files`：只枚举插件文件（增量扫描使用）。
//! - `collect_vst3_files` / `collect_vst2_files`：目录递归枚举。
//! - `scan_vst2_dir` / `probe_vst2_kind`：VST2 扫描与类型探测。
//! - `standard_vst3_dirs` / `standard_vst2_dirs`：标准插件目录。
//! - `scan_vst3_dir` / `add_vst3_bundle` / `add_vst3_bundle_file`：VST3 目录扫描与记录构造。
//! - `probe_vst3_path` / `probe_vst3_bundle`：单插件工厂元数据探测。
//! - `classify_vst3` / `kind_from_category` / `read_moduleinfo` / `sub_categories` / `json_string` / `dedup_key` / `vendor_from_parent`：分类与元数据解析。
//!
//! Module: plugin / plugin scanning
//! Purpose:
//! - Shared discovery helpers: VST3 bundle metadata, VST2 DLL probing and directory enumeration.
//! - Fill missing vendor, category and instrument/effect role from the built-in catalog.
//! Functions:
//! - `enrich_from_catalog`: catalog enrichment.
//! - `From<vst3_host::PluginInfo>`: convert a VST3 probe result.
//! - `scan_vst3` / `scan_legacy_vst`: per-format scans.
//! - `list_plugin_files`: enumerate plugin files only (used by incremental scans).
//! - `collect_vst3_files` / `collect_vst2_files`: recursive directory enumeration.
//! - `scan_vst2_dir` / `probe_vst2_kind`: VST2 scanning and role probing.
//! - `standard_vst3_dirs` / `standard_vst2_dirs`: standard plugin directories.
//! - `scan_vst3_dir` / `add_vst3_bundle` / `add_vst3_bundle_file`: VST3 directory scanning and records.
//! - `probe_vst3_path` / `probe_vst3_bundle`: per-plugin factory metadata probing.
//! - `classify_vst3` / `kind_from_category` / `read_moduleinfo` / `sub_categories` / `json_string` / `dedup_key` / `vendor_from_parent`: classification and metadata parsing.

use super::backends::Vst2Host;
use super::{PluginKind, ScannedPlugin};
use crate::core::model::PluginFormat;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use vst::host::PluginLoader;
use vst::plugin::Plugin as VstPluginTrait;

/// 所属模块：plugin / 内置插件资料库
/// 功能：用内置资料表补全缺失的厂商、分类与乐器/效果器类型，不覆盖已有的可信元数据。
/// 输入：`plugin` 扫描记录。
/// 输出/副作用：原地补全 `vendor` / `category` / `kind` / `has_midi_input`。
/// 复杂度：时间 O(资料条数 × 名称长度)，空间 O(名称长度)。
///
/// Module: plugin / built-in plugin catalog
/// Purpose: Fill missing vendor, category and role from the built-in catalog without overriding
/// trustworthy metadata.
/// Inputs: `plugin` — one scan record.
/// Outputs / side effects: updates `vendor` / `category` / `kind` / `has_midi_input` in place.
/// Complexity: O(catalog entries × name length) time and O(name length) space.
fn enrich_from_catalog(plugin: &mut ScannedPlugin) {
    let Some(entry) = super::catalog::lookup(&plugin.name) else {
        return;
    };
    if plugin.vendor.is_empty() {
        plugin.vendor = entry.vendor.to_string();
    }
    let generic_category = plugin.category.is_empty()
        || plugin.category.eq_ignore_ascii_case("VST3")
        || plugin.category.eq_ignore_ascii_case("VST / VST2");
    if generic_category {
        plugin.category = entry.category.to_string();
    }
    if plugin.kind == PluginKind::Unknown {
        plugin.kind = if entry.is_instrument {
            PluginKind::Instrument
        } else {
            PluginKind::Effect
        };
    }
    if entry.is_instrument && plugin.kind == PluginKind::Instrument {
        plugin.has_midi_input = true;
    }
}

impl From<vst3_host::PluginInfo> for ScannedPlugin {
    fn from(info: vst3_host::PluginInfo) -> Self {
        let kind = if info.has_midi_input {
            PluginKind::Instrument
        } else if info.category.to_lowercase().contains("fx")
            || info.category.to_lowercase().contains("effect")
        {
            PluginKind::Effect
        } else {
            PluginKind::Unknown
        };
        let mut plugin = Self {
            format: PluginFormat::Vst3,
            path: info.path,
            name: info.name,
            vendor: info.vendor,
            category: info.category,
            kind,
            uid: info.uid,
            has_midi_input: info.has_midi_input,
            has_gui: info.has_gui,
        };
        enrich_from_catalog(&mut plugin);
        plugin
    }
}

/// Scan VST3 bundles using their `moduleinfo.json` metadata only.
///
/// Deliberately avoids instantiating plugins while scanning: many commercial
/// plugins run licensing code during initialization and can abort the host
/// process. Loading is deferred until the user actually picks a plugin.
pub fn scan_vst3(custom_paths: &[PathBuf]) -> Result<Vec<ScannedPlugin>, String> {
    let mut roots = standard_vst3_dirs();
    roots.extend(custom_paths.iter().cloned());

    let mut results = Vec::new();
    let mut seen = HashSet::new();
    for root in roots {
        scan_vst3_dir(&root, 0, &mut results, &mut seen);
    }
    results.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.vendor.cmp(&b.vendor))
    });
    Ok(results)
}

/// Scan legacy VST / VST2 `.dll` files. Each plugin is instantiated briefly to
/// read its name and category so instruments and effects can be separated.
pub fn scan_legacy_vst(custom_paths: &[PathBuf]) -> Result<Vec<ScannedPlugin>, String> {
    let mut roots = standard_vst2_dirs();
    roots.extend(custom_paths.iter().cloned());
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    for root in roots {
        scan_vst2_dir(&root, 0, &mut results, &mut seen);
    }
    results.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.vendor.cmp(&b.vendor))
    });
    Ok(results)
}

/// 所属模块：plugin / 插件扫描
/// 功能：只读取指定 VST3 bundle/单文件的 `moduleinfo.json`（缺失元数据时按文件名记为未知），
/// 用于增量扫描中新增或变化的 VST3 文件。
/// 输入：`paths` VST3 bundle 目录或单文件 `.vst3`。
/// 输出/副作用：返回扫描记录；只读文件系统，不实例化插件。
/// 复杂度：时间 O(文件数 × 元数据读取)，空间 O(文件数)。
///
/// Module: plugin / plugin scanning
/// Purpose: Read `moduleinfo.json` only for the given VST3 bundles/single files (missing metadata
/// becomes an unknown record named after the file), used for new or changed VST3 files.
/// Inputs: `paths` — VST3 bundle directories or single-file `.vst3` entries.
/// Outputs / side effects: scan records; read-only filesystem, no plugin instantiation.
/// Complexity: O(files × metadata reads) time and O(files) space.
pub fn scan_vst3_paths(paths: &[PathBuf]) -> Vec<ScannedPlugin> {
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    for path in paths {
        if path.is_dir() {
            add_vst3_bundle(path, &mut results, &mut seen);
        } else {
            add_vst3_bundle_file(path, &mut results, &mut seen);
        }
    }
    results
}

/// 所属模块：plugin / 插件扫描
/// 功能：枚举标准目录与自定义路径中的插件文件（VST3 bundle/单文件与 VST2 DLL），不读取元数据、
/// 不实例化，供启动时增量扫描与缓存比对使用。
/// 输入：`custom_paths` 额外扫描目录或单个插件文件。
/// 输出/副作用：返回 `(格式, 路径)` 列表；只读文件系统。
/// 复杂度：时间 O(目录项数 × 最大递归深度)，空间 O(插件文件数)。
///
/// Module: plugin / plugin scanning
/// Purpose: Enumerate plugin files (VST3 bundles/single files and VST2 DLLs) in the standard
/// directories and custom paths without reading metadata or instantiating, for incremental scans.
/// Inputs: `custom_paths` — extra scan directories or single plugin files.
/// Outputs / side effects: returns `(format, path)` pairs; the filesystem is read-only.
/// Complexity: O(directory entries × max recursion depth) time and O(plugin files) space.
pub fn list_plugin_files(custom_paths: &[PathBuf]) -> Vec<(PluginFormat, PathBuf)> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for root in standard_vst3_dirs() {
        collect_vst3_files(&root, 0, &mut found, &mut seen);
    }
    for root in standard_vst2_dirs() {
        collect_vst2_files(&root, 0, &mut found, &mut seen);
    }
    for root in custom_paths {
        if root.is_dir() {
            collect_vst3_files(root, 0, &mut found, &mut seen);
            collect_vst2_files(root, 0, &mut found, &mut seen);
            continue;
        }
        let extension = root
            .extension()
            .map(|ext| ext.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let format = match extension.as_str() {
            "vst3" => PluginFormat::Vst3,
            "dll" => PluginFormat::Vst2,
            _ => continue,
        };
        if seen.insert(dedup_key(root)) {
            found.push((format, root.clone()));
        }
    }
    found
}

/// 递归收集 VST3 文件；bundle 目录与单文件 `.vst3` 都算一个插件。
/// Collect VST3 files recursively; both bundle directories and single-file `.vst3` count.
fn collect_vst3_files(
    dir: &Path,
    depth: usize,
    found: &mut Vec<(PluginFormat, PathBuf)>,
    seen: &mut HashSet<String>,
) {
    if depth > 5 || !dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_vst3 = path
            .extension()
            .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("vst3"))
            .unwrap_or(false);
        if is_vst3 {
            if seen.insert(dedup_key(&path)) {
                found.push((PluginFormat::Vst3, path));
            }
        } else if path.is_dir() {
            collect_vst3_files(&path, depth + 1, found, seen);
        }
    }
}

/// 递归收集 VST2 `.dll` 文件。
/// Collect VST2 `.dll` files recursively.
fn collect_vst2_files(
    dir: &Path,
    depth: usize,
    found: &mut Vec<(PluginFormat, PathBuf)>,
    seen: &mut HashSet<String>,
) {
    if depth > 5 || !dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_vst2_files(&path, depth + 1, found, seen);
            continue;
        }
        let is_dll = path
            .extension()
            .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("dll"))
            .unwrap_or(false);
        if is_dll && seen.insert(dedup_key(&path)) {
            found.push((PluginFormat::Vst2, path));
        }
    }
}

fn standard_vst2_dirs() -> Vec<PathBuf> {
    // 安卓没有标准 VST2 目录，`dirs` 保持为空，因此该绑定在安卓上不需要 mut。
    // Android has no standard VST2 directory, so the binding needs no mut there.
    #[cfg_attr(target_os = "android", allow(unused_mut))]
    let mut dirs = Vec::new();
    #[cfg(target_os = "windows")]
    {
        for variable in ["PROGRAMFILES", "PROGRAMFILES(X86)"] {
            if let Ok(base) = std::env::var(variable) {
                let base = PathBuf::from(base);
                dirs.push(base.join("VSTPlugins"));
                dirs.push(base.join("Steinberg").join("VstPlugins"));
                dirs.push(base.join("VST"));
            }
        }
        for variable in ["COMMONPROGRAMFILES", "COMMONPROGRAMFILES(X86)"] {
            if let Ok(base) = std::env::var(variable) {
                let base = PathBuf::from(base);
                dirs.push(base.join("VST2"));
                dirs.push(base.join("VSTPlugins"));
            }
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("Programs").join("VSTPlugins"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/Library/Audio/Plug-Ins/VST"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(home).join("Library/Audio/Plug-Ins/VST"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/lib/vst"));
        dirs.push(PathBuf::from("/usr/local/lib/vst"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(home).join(".vst"));
        }
    }
    dirs
}

fn scan_vst2_dir(
    dir: &Path,
    depth: usize,
    results: &mut Vec<ScannedPlugin>,
    seen: &mut HashSet<String>,
) {
    if depth > 5 || !dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_vst2_dir(&path, depth + 1, results, seen);
            continue;
        }
        let is_dll = path
            .extension()
            .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("dll"))
            .unwrap_or(false);
        if !is_dll {
            continue;
        }
        let key = dedup_key(&path);
        if !seen.insert(key) {
            continue;
        }
        let fallback_name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "VST 插件".into());
        // Best-effort type detection: instantiate the plugin briefly so the
        // browser can separate instruments from effects.
        let (name, kind) = match probe_vst2_kind(&path) {
            Some((reported, kind)) => (
                if reported.is_empty() {
                    fallback_name
                } else {
                    reported
                },
                kind,
            ),
            None => (fallback_name, PluginKind::Unknown),
        };
        results.push(ScannedPlugin {
            format: PluginFormat::Vst2,
            path: path.clone(),
            name,
            vendor: vendor_from_parent(&path),
            kind,
            category: "VST / VST2".into(),
            uid: String::new(),
            has_midi_input: false,
            has_gui: true,
        });
        if let Some(plugin) = results.last_mut() {
            enrich_from_catalog(plugin);
        }
    }
}

/// Load a VST2 long enough to read its reported name and category.
fn probe_vst2_kind(path: &Path) -> Option<(String, PluginKind)> {
    let host = Arc::new(Mutex::new(Vst2Host::default()));
    let mut loader = PluginLoader::load(path, host).ok()?;
    let instance = loader.instance().ok()?;
    let info = instance.get_info();
    let kind = match info.category {
        vst::plugin::Category::Synth | vst::plugin::Category::Generator => PluginKind::Instrument,
        vst::plugin::Category::Unknown => PluginKind::Unknown,
        _ => PluginKind::Effect,
    };
    Some((info.name, kind))
}

fn standard_vst3_dirs() -> Vec<PathBuf> {
    // 安卓没有标准 VST3 目录，`dirs` 保持为空，因此该绑定在安卓上不需要 mut。
    // Android has no standard VST3 directory, so the binding needs no mut there.
    #[cfg_attr(target_os = "android", allow(unused_mut))]
    let mut dirs = Vec::new();
    #[cfg(target_os = "windows")]
    {
        for variable in ["COMMONPROGRAMFILES", "COMMONPROGRAMFILES(X86)"] {
            if let Ok(base) = std::env::var(variable) {
                dirs.push(PathBuf::from(base).join("VST3"));
            }
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            dirs.push(
                PathBuf::from(&local)
                    .join("Programs")
                    .join("Common")
                    .join("VST3"),
            );
            dirs.push(PathBuf::from(local).join("VST3"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(home).join("Library/Audio/Plug-Ins/VST3"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/lib/vst3"));
        dirs.push(PathBuf::from("/usr/local/lib/vst3"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(home).join(".vst3"));
        }
    }
    dirs
}

fn scan_vst3_dir(
    dir: &Path,
    depth: usize,
    results: &mut Vec<ScannedPlugin>,
    seen: &mut HashSet<String>,
) {
    if depth > 5 || !dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_vst3 = path
            .extension()
            .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("vst3"))
            .unwrap_or(false);
        if is_vst3 {
            if path.is_dir() {
                add_vst3_bundle(&path, results, seen);
            } else {
                add_vst3_bundle_file(&path, results, seen);
            }
        } else if path.is_dir() {
            scan_vst3_dir(&path, depth + 1, results, seen);
        }
    }
}

fn dedup_key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

fn add_vst3_bundle(path: &Path, results: &mut Vec<ScannedPlugin>, seen: &mut HashSet<String>) {
    let key = dedup_key(path);
    if !seen.insert(key) {
        return;
    }
    let fallback = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "VST3 插件".into());
    // 标准 bundle 的 moduleinfo.json 位于 Contents 下；部分厂商（如 Serum 2）
    // 放在 Contents/Resources 中，需要一并探测。
    // Standard bundles keep moduleinfo.json under Contents; some vendors (e.g.
    // Serum 2) place it in Contents/Resources, so probe both locations.
    let metadata = read_moduleinfo(&path.join("Contents").join("moduleinfo.json")).or_else(|| {
        read_moduleinfo(
            &path
                .join("Contents")
                .join("Resources")
                .join("moduleinfo.json"),
        )
    });
    let info = match metadata {
        Some(info) => info,
        None => {
            // 快速扫描不做实例化探测：先按未知归类，由应用层后台补齐。
            // Fast scan does not probe: mark the plugin unknown and let the app
            // classify it in the background.
            Vst3ClassInfo {
                name: fallback.clone(),
                vendor: String::new(),
                uid: String::new(),
                category: "VST3".into(),
                is_instrument: false,
                audio_inputs: 0,
            }
        }
    };
    let kind = classify_vst3(&info.category, info.is_instrument, info.audio_inputs);
    let mut vendor = info.vendor;
    if vendor.is_empty() {
        vendor = vendor_from_parent(path);
    }
    results.push(ScannedPlugin {
        format: PluginFormat::Vst3,
        path: path.to_path_buf(),
        name: if info.name.is_empty() {
            fallback
        } else {
            info.name
        },
        vendor,
        kind,
        category: info.category,
        uid: info.uid,
        has_midi_input: info.is_instrument,
        has_gui: true,
    });
    if let Some(plugin) = results.last_mut() {
        enrich_from_catalog(plugin);
    }
}

fn add_vst3_bundle_file(path: &Path, results: &mut Vec<ScannedPlugin>, seen: &mut HashSet<String>) {
    let key = dedup_key(path);
    if !seen.insert(key) {
        return;
    }
    let fallback = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "VST3 插件".into());
    // Windows 允许把 VST3 打包成单个 `.vst3` DLL，这种文件不含 moduleinfo.json；
    // 快速扫描先按文件名与目录归类为未知，类型由应用层后台补齐。
    // Windows allows single-file `.vst3` DLLs without moduleinfo.json; the fast scan
    // labels them unknown by file name/folder and the app classifies them later.
    results.push(ScannedPlugin {
        format: PluginFormat::Vst3,
        path: path.to_path_buf(),
        name: fallback,
        vendor: vendor_from_parent(path),
        kind: PluginKind::Unknown,
        category: "VST3".into(),
        uid: String::new(),
        has_midi_input: false,
        has_gui: true,
    });
    if let Some(plugin) = results.last_mut() {
        enrich_from_catalog(plugin);
    }
}

/// 通过工厂元数据补齐单个 VST3 插件的名称、CID 与分类（后台识别使用）。
/// Fill one VST3 plugin's name, CID and classification from factory metadata (used by
/// deferred background classification).
pub(super) fn probe_vst3_path(path: &Path) -> Option<ScannedPlugin> {
    let info = probe_vst3_bundle(path)?;
    let fallback = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "VST3 插件".into());
    let kind = classify_vst3(&info.category, info.is_instrument, info.audio_inputs);
    let mut vendor = info.vendor;
    if vendor.is_empty() {
        vendor = vendor_from_parent(path);
    }
    Some(ScannedPlugin {
        format: PluginFormat::Vst3,
        path: path.to_path_buf(),
        name: if info.name.is_empty() {
            fallback
        } else {
            info.name
        },
        vendor,
        kind,
        category: info.category,
        uid: info.uid,
        has_midi_input: info.is_instrument,
        has_gui: true,
    })
    .map(|mut plugin| {
        enrich_from_catalog(&mut plugin);
        plugin
    })
}

/// VST3 类元数据：moduleinfo.json 与实例化探测共用的中间结构。
/// VST3 class metadata shared by moduleinfo.json parsing and instantiation probes.
struct Vst3ClassInfo {
    name: String,
    vendor: String,
    uid: String,
    category: String,
    /// 是否乐器；moduleinfo 只能看 SubCategories，探测还能结合通道布局。
    /// Whether this is an instrument; moduleinfo only exposes SubCategories while a
    /// probe can also inspect the bus layout.
    is_instrument: bool,
    /// 音频输入通道数；moduleinfo 不提供时为 `0`。
    /// Audio input count; `0` when moduleinfo does not provide it.
    audio_inputs: u32,
}

/// Load a VST3 module's factory metadata (no component/controller instance) to read its
/// class name, CID and category when `moduleinfo.json` is unavailable.
fn probe_vst3_bundle(path: &Path) -> Option<Vst3ClassInfo> {
    let classes = vst3_host::read_factory_classes(path).ok()?;
    let mut chosen: Option<&vst3_host::FactoryClassInfo> = None;
    for class in &classes {
        if !class.category.contains("Audio Module Class") {
            continue;
        }
        let is_instrument = class
            .sub_categories
            .iter()
            .any(|value| value.eq_ignore_ascii_case("Instrument"));
        if chosen.is_none() || is_instrument {
            chosen = Some(class);
        }
        if is_instrument {
            // Bundle 同时导出乐器与效果器类时优先列出乐器。
            // Prefer the instrument class when a bundle exports both roles.
            break;
        }
    }
    let class = chosen?;
    let is_instrument = class
        .sub_categories
        .iter()
        .any(|value| value.eq_ignore_ascii_case("Instrument"));
    let category = if class.sub_categories.is_empty() {
        if class.category.is_empty() {
            "VST3".to_string()
        } else {
            class.category.clone()
        }
    } else {
        class.sub_categories.join(" / ")
    };
    Some(Vst3ClassInfo {
        name: class.name.clone(),
        vendor: class.vendor.clone(),
        uid: class.class_id.clone(),
        category,
        is_instrument,
        // 工厂元数据不包含总线布局；分类只依赖子类别即可。
        // Factory metadata has no bus layout; sub-categories are enough to classify.
        audio_inputs: 0,
    })
}

/// Combine category keywords with the channel layout for robust classification.
pub(super) fn classify_vst3(category: &str, has_midi: bool, audio_inputs: u32) -> PluginKind {
    let kind = kind_from_category(category, has_midi);
    if kind != PluginKind::Unknown {
        return kind;
    }
    if audio_inputs == 0 && has_midi {
        PluginKind::Instrument
    } else if audio_inputs > 0 {
        PluginKind::Effect
    } else if has_midi {
        PluginKind::Instrument
    } else {
        PluginKind::Unknown
    }
}

fn kind_from_category(category: &str, has_midi: bool) -> PluginKind {
    let lower = category.to_lowercase();
    const INSTRUMENT: &[&str] = &[
        "instrument",
        "synth",
        "sampler",
        "sample player",
        "drum machine",
        "generator",
        "piano",
        "organ",
    ];
    const EFFECT: &[&str] = &[
        "fx",
        "effect",
        "eq",
        "filter",
        "dynamics",
        "compressor",
        "limiter",
        "reverb",
        "delay",
        "modulation",
        "distortion",
        "saturation",
        "chorus",
        "flanger",
        "phaser",
        "pitch",
        "vocoder",
        "utility",
        "analyzer",
        "mastering",
        "restoration",
        "ambience",
        "spatial",
        "surround",
    ];
    if INSTRUMENT.iter().any(|keyword| lower.contains(keyword)) {
        PluginKind::Instrument
    } else if EFFECT.iter().any(|keyword| lower.contains(keyword)) {
        PluginKind::Effect
    } else if has_midi {
        PluginKind::Instrument
    } else {
        PluginKind::Unknown
    }
}

/// Vendor folders are a common convention on Windows (`FabFilter/Pro-Q.vst3`).
fn vendor_from_parent(path: &Path) -> String {
    let Some(parent) = path.parent().and_then(|p| p.file_name()) else {
        return String::new();
    };
    let parent = parent.to_string_lossy();
    if parent.eq_ignore_ascii_case("VST3")
        || parent.eq_ignore_ascii_case("VstPlugins")
        || parent.is_empty()
    {
        String::new()
    } else {
        parent.to_string()
    }
}

/// Parse the standard VST3 `moduleinfo.json` without loading the plugin.
fn read_moduleinfo(path: &Path) -> Option<Vst3ClassInfo> {
    let text = std::fs::read_to_string(path).ok()?;
    let root: serde_json::Value = serde_json::from_str(&text).ok()?;
    let classes = root
        .get("Classes")
        .or_else(|| root.get("classes"))
        .and_then(|value| value.as_array());

    let mut chosen: Option<(&serde_json::Value, bool)> = None;
    if let Some(classes) = classes {
        for class in classes {
            let category = json_string(class, &["Category", "category"]).unwrap_or_default();
            if !category.contains("Audio Module Class") {
                continue;
            }
            let sub_categories = sub_categories(class);
            let is_instrument = sub_categories
                .iter()
                .any(|value| value.eq_ignore_ascii_case("Instrument"));
            if chosen.is_none() || is_instrument {
                chosen = Some((class, is_instrument));
            }
            if is_instrument {
                // Prefer the instrument class over an effect sibling.
                break;
            }
        }
    }

    let (class, is_instrument) = match chosen {
        Some(chosen) => chosen,
        None => {
            // A bundle without a usable class list: use the top-level metadata.
            let name = json_string(&root, &["Name", "name"])?;
            let vendor = json_string(&root, &["Vendor", "vendor"]).unwrap_or_default();
            return Some(Vst3ClassInfo {
                name,
                vendor,
                uid: String::new(),
                category: "VST3".into(),
                is_instrument: false,
                audio_inputs: 0,
            });
        }
    };

    let name = json_string(class, &["Name", "name"])
        .or_else(|| json_string(&root, &["Name", "name"]))
        .unwrap_or_else(|| "VST3 插件".into());
    let vendor = json_string(class, &["Vendor", "vendor"])
        .or_else(|| json_string(&root, &["Vendor", "vendor"]))
        .unwrap_or_default();
    let uid = json_string(class, &["CID", "cid"]).unwrap_or_default();
    let category = {
        let values = sub_categories(class);
        if values.is_empty() {
            "VST3".to_string()
        } else {
            values.join(" / ")
        }
    };

    Some(Vst3ClassInfo {
        name,
        vendor,
        uid,
        category,
        is_instrument,
        audio_inputs: 0,
    })
}

/// 读取 VST3 类声明的 SubCategories，兼容不同 SDK 版本的键名（含带空格的写法）。
/// Read the SubCategories declared by a VST3 class, accepting the key spellings used
/// by different SDK versions (including the spaced form).
fn sub_categories(class: &serde_json::Value) -> Vec<String> {
    class
        .get("SubCategories")
        .or_else(|| class.get("subCategories"))
        .or_else(|| class.get("subcategories"))
        .or_else(|| class.get("Sub Categories"))
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn json_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|v| v.as_str()))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：Resources 下的 moduleinfo 使用带空格的 `Sub Categories`，乐器与效果器都能正确分类。
    /// Covers: moduleinfo under Resources with a spaced `Sub Categories` key classifies both roles.
    #[test]
    fn moduleinfo_reads_spaced_sub_categories_and_classifies_roles() {
        let dir = std::env::temp_dir().join("surfing-studio-scan-classify");
        let resources = dir.join("Contents").join("Resources");
        std::fs::create_dir_all(&resources).expect("create resources dir");
        let path = resources.join("moduleinfo.json");

        let instrument = r#"{
            "Name": "Demo Synth",
            "Classes": [
                {
                    "CID": "0123456789ABCDEF0123456789ABCDEF",
                    "Category": "Audio Module Class",
                    "Name": "Demo Synth",
                    "Vendor": "Demo Vendor",
                    "Sub Categories": ["Instrument", "Synth"]
                }
            ]
        }"#;
        std::fs::write(&path, instrument).expect("write instrument moduleinfo");
        let info = read_moduleinfo(&path).expect("parse instrument moduleinfo");
        assert!(info.is_instrument, "instrument class must be detected");
        assert_eq!(
            classify_vst3(&info.category, info.is_instrument, info.audio_inputs),
            PluginKind::Instrument
        );
        assert_eq!(info.uid, "0123456789ABCDEF0123456789ABCDEF");

        let effect = r#"{
            "Name": "Demo EQ",
            "Classes": [
                {
                    "CID": "FEDCBA9876543210FEDCBA9876543210",
                    "Category": "Audio Module Class",
                    "Name": "Demo EQ",
                    "Vendor": "Demo Vendor",
                    "Sub Categories": ["Fx", "EQ"]
                }
            ]
        }"#;
        std::fs::write(&path, effect).expect("write effect moduleinfo");
        let info = read_moduleinfo(&path).expect("parse effect moduleinfo");
        assert!(
            !info.is_instrument,
            "effect class must not be an instrument"
        );
        assert_eq!(
            classify_vst3(&info.category, info.is_instrument, info.audio_inputs),
            PluginKind::Effect
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
