//! 所属模块：surfing-ai-data / 资料库文件存储
//! 文件用途：
//! - 把 AI 资料库拆成“智能体 JSON + 技能 Markdown + 乐器 JSON + 顺序清单”写入资料库根目录。
//! - 读取上述文件并合并为完整的资料库 JSON；损坏条目跳过，不中断整体加载。
//! - 提供单条导入、单条导出与整类导出，供宿主 UI 调用。
//!
//! 函数清单：
//! - `load`：读取目录并合并为资料库 JSON。
//! - `save`：把资料库 JSON 拆分写入目录并清理过期文件。
//! - `import`：解析一个 `.json` 或 `.md` 文件为 `(类型, 条目)`。
//! - `export`：导出单条资料。
//! - `export_all`：导出某一类的全部资料。
//!
//! Module: surfing-ai-data / library file storage
//! Purpose:
//! - Split the AI library into agent JSON, skill Markdown, instrument JSON and an order manifest
//!   under the library root.
//! - Read those files back into one library JSON; broken entries are skipped without failing the load.
//! - Provide single-entry import/export and whole-category export for the host UI.
//!
//! Functions:
//! - `load`: read the directory and merge it into library JSON.
//! - `save`: split library JSON into files and remove stale entries.
//! - `import`: parse one `.json` or `.md` file into `(kind, entry)`.
//! - `export`: export one entry.
//! - `export_all`: export every entry of one kind.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::markdown;

/// 资料库清单文件名（保存结构版本与条目顺序）。
/// Library manifest file name (schema version and entry order).
const MANIFEST: &str = "library.json";
/// 智能体目录名。
/// Agent directory name.
const AGENTS_DIR: &str = "agents";
/// 技能根目录名。
/// Skill root directory name.
const SKILLS_DIR: &str = "skills";
/// 基础技能目录名。
/// Basic-skill directory name.
const BASICS_DIR: &str = "basics";
/// 编曲技巧目录名。
/// Technique directory name.
const TECHNIQUES_DIR: &str = "techniques";
/// 乐器资料目录名（位于 skills 下，保持分类层级）。
/// Instrument-note directory name (under skills to keep the category tree).
const INSTRUMENTS_DIR: &str = "instruments";
/// 缺少清单时的默认结构版本。
/// Default schema version when the manifest is missing.
const DEFAULT_VERSION: u64 = 3;

/// 所属模块：surfing-ai-data / 资料库文件存储
/// 功能：读取资料库根目录下的分类文件并合并为 `AiLibrary` 兼容的 JSON。
/// 输入：`root`：资料库根目录（由运行时给出，桌面为 `<安装目录>/ai`）。
/// 输出/副作用：资料库 JSON；只读文件，不创建目录。
/// 复杂度：时间 O(文件数 × 文件大小)，空间 O(资料库大小)。
///
/// Module: surfing-ai-data / library file storage
/// Purpose: Read the categorized files under the library root and merge them into
/// `AiLibrary`-shaped JSON.
/// Inputs: `root`: library root (supplied by the runtime; `<install>/ai` on desktop).
/// Outputs / side effects: library JSON; reads files only and creates nothing.
/// Complexity: O(files × file size) time and O(library size) space.
pub fn load(root: &Path) -> Result<Value, String> {
    let manifest = read_manifest(root);
    let agents = load_json_entries(&root.join(AGENTS_DIR), &manifest, "agents")?;
    let basics =
        load_markdown_entries(&root.join(SKILLS_DIR).join(BASICS_DIR), &manifest, "basics")?;
    let techniques = load_markdown_entries(
        &root.join(SKILLS_DIR).join(TECHNIQUES_DIR),
        &manifest,
        "techniques",
    )?;
    let instruments = load_json_entries(
        &root.join(SKILLS_DIR).join(INSTRUMENTS_DIR),
        &manifest,
        "instruments",
    )?;
    Ok(json!({
        "version": manifest.0,
        "agents": agents,
        "basics": basics,
        "techniques": techniques,
        "instruments": instruments,
    }))
}

/// 所属模块：surfing-ai-data / 资料库文件存储
/// 功能：把资料库 JSON 拆分为分类文件并写入资料库根目录，随后删除不再存在的条目文件。
/// 输入：`root` 资料库根目录；`library` 资料库 JSON。
/// 输出/副作用：创建目录并写文件；失败返回中文原因。
/// 复杂度：时间 O(条目数 × 条目大小)，空间 O(单条大小)。
///
/// Module: surfing-ai-data / library file storage
/// Purpose: Split library JSON into categorized files under the library root and delete files for
/// removed entries.
/// Inputs: `root`; `library`.
/// Outputs / side effects: creates directories and writes files; returns a Chinese error on failure.
/// Complexity: O(entries × entry size) time and O(one entry) space.
pub fn save(root: &Path, library: &Value) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| format!("创建 AI 资料目录失败: {error}"))?;
    let version = library
        .get("version")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_VERSION);

    let mut order = serde_json::Map::new();
    save_agents(root, library, &mut order)?;
    save_skills(
        &root.join(SKILLS_DIR).join(BASICS_DIR),
        library,
        "basics",
        "basic",
        &mut order,
    )?;
    save_skills(
        &root.join(SKILLS_DIR).join(TECHNIQUES_DIR),
        library,
        "techniques",
        "technique",
        &mut order,
    )?;
    save_instruments(root, library, &mut order)?;

    let manifest = json!({ "version": version, "order": order });
    write_text(
        &root.join(MANIFEST),
        &serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("序列化 AI 资料清单失败: {error}"))?,
    )
}

/// 所属模块：surfing-ai-data / 导入
/// 功能：解析一个技能 Markdown 或智能体/乐器 JSON。
/// 输入：`file` 源文件；`kind_hint` 当前页面类型（`agent` / `basic` / `technique` / `instrument`）。
/// 输出/副作用：`(类型, 条目 JSON)`；只读文件，不修改资料库。
/// 复杂度：时间 O(文件大小)，空间 O(文件大小)。
///
/// Module: surfing-ai-data / import
/// Purpose: Parse one skill Markdown or agent/instrument JSON file.
/// Inputs: `file`; `kind_hint` (the current page kind).
/// Outputs / side effects: `(kind, entry JSON)`; reads the file only.
/// Complexity: O(file size) time and space.
pub fn import(file: &Path, kind_hint: &str) -> Result<(String, Value), String> {
    let text = std::fs::read_to_string(file)
        .map_err(|error| format!("读取导入文件失败: {error}"))?
        .trim_start_matches('\u{feff}')
        .to_string();
    let extension = file
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let stem = file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("imported")
        .to_string();
    if extension == "md" || extension == "markdown" {
        let (header, body) = markdown::decode(&text);
        let kind = match header.kind.as_str() {
            "basic" => "basic".to_string(),
            "technique" => "technique".to_string(),
            _ => match kind_hint {
                "technique" => "technique".to_string(),
                _ => "basic".to_string(),
            },
        };
        let id = if header.id.trim().is_empty() {
            sanitize_id(&stem)
        } else {
            sanitize_id(&header.id)
        };
        let title = if header.title.trim().is_empty() {
            stem
        } else {
            header.title
        };
        return Ok((
            kind,
            json!({
                "id": id,
                "title": title,
                "summary": header.summary,
                "body": body,
            }),
        ));
    }
    if extension == "json" {
        let mut entry: Value =
            serde_json::from_str(&text).map_err(|error| format!("解析 JSON 失败: {error}"))?;
        let object = entry
            .as_object_mut()
            .ok_or_else(|| "JSON 文件必须是单个对象".to_string())?;
        if object.contains_key("agents") || object.contains_key("techniques") {
            return Err("暂不支持整库导入，请选择单个智能体或技能文件".into());
        }
        if object
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
        {
            object.insert("id".into(), Value::String(sanitize_id(&stem)));
        }
        let is_agent = object.contains_key("style")
            || object.contains_key("default_skill_ids")
            || object.contains_key("default_technique_ids");
        let is_instrument = object.contains_key("category")
            || object.contains_key("aliases")
            || object.contains_key("range");
        if is_agent {
            return Ok(("agent".into(), entry));
        }
        if is_instrument {
            return Ok(("instrument".into(), entry));
        }
        return Err(
            "无法识别 JSON 类型：智能体需要 style / default_skill_ids，乐器需要 category / aliases"
                .into(),
        );
    }
    Err("仅支持导入 .md（技能）或 .json（智能体 / 乐器）文件".into())
}

/// 所属模块：surfing-ai-data / 导出
/// 功能：把资料库中的一条记录导出到指定路径。
/// 输入：`library` 资料库 JSON；`kind` 类型；`id` 条目 id；`target` 目标文件。
/// 输出/副作用：写入目标文件；失败返回中文原因。
/// 复杂度：时间 O(条目大小)，空间 O(条目大小)。
///
/// Module: surfing-ai-data / export
/// Purpose: Export one library entry to a chosen path.
/// Inputs: `library`; `kind`; `id`; `target`.
/// Outputs / side effects: writes the target file; returns a Chinese error on failure.
/// Complexity: O(entry size) time and space.
pub fn export(library: &Value, kind: &str, id: &str, target: &Path) -> Result<(), String> {
    let entries = entries_of(library, kind)?;
    let entry = entries
        .iter()
        .find(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
        .ok_or_else(|| format!("未找到要导出的条目：{id}"))?;
    let text = entry_text(kind, entry)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建导出目录失败: {error}"))?;
    }
    write_text(target, &text)
}

/// 所属模块：surfing-ai-data / 导出
/// 功能：把某一类的全部记录导出到目标目录下的分类子目录。
/// 输入：`library` 资料库 JSON；`kind` 类型；`directory` 目标目录。
/// 输出/副作用：写入多个文件；返回导出数量。
/// 复杂度：时间 O(条目数 × 条目大小)，空间 O(单条大小)。
///
/// Module: surfing-ai-data / export
/// Purpose: Export every entry of one kind into the matching category sub-directory.
/// Inputs: `library`; `kind`; `directory`.
/// Outputs / side effects: writes several files; returns the exported count.
/// Complexity: O(entries × entry size) time and O(one entry) space.
pub fn export_all(library: &Value, kind: &str, directory: &Path) -> Result<usize, String> {
    let entries = entries_of(library, kind)?;
    let folder = directory.join(category_dir(kind));
    std::fs::create_dir_all(&folder).map_err(|error| format!("创建导出目录失败: {error}"))?;
    let mut used = BTreeSet::new();
    let mut count = 0;
    for entry in entries {
        let id = entry.get("id").and_then(Value::as_str).unwrap_or_default();
        if id.trim().is_empty() {
            continue;
        }
        let stem = unique_stem(&sanitize_id(id), &mut used);
        let target = folder.join(format!("{stem}.{}", extension(kind)));
        write_text(&target, &entry_text(kind, entry)?)?;
        count += 1;
    }
    Ok(count)
}

/// 所属模块：surfing-ai-data / 资料库文件存储
/// 功能：读取智能体目录并按清单顺序输出。
/// 输入：`root` 目录；`manifest` 清单；`key` 数组键。
/// 输出/副作用：条目数组；损坏文件跳过。
/// 复杂度：时间 O(文件数 × 文件大小)，空间 O(条目数)。
///
/// Module: surfing-ai-data / library file storage
/// Purpose: Read the agent directory and order entries by the manifest.
/// Inputs: `root`; `manifest`; `key`.
/// Outputs / side effects: entry array; broken files are skipped.
/// Complexity: O(files × file size) time and O(entries) space.
fn save_agents(
    root: &Path,
    library: &Value,
    order: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let directory = root.join(AGENTS_DIR);
    std::fs::create_dir_all(&directory).map_err(|error| format!("创建智能体目录失败: {error}"))?;
    let entries = array_of(library, "agents");
    let mut used = BTreeSet::new();
    let mut ids = Vec::new();
    for entry in entries {
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        if id.trim().is_empty() {
            continue;
        }
        let stem = unique_stem(&sanitize_id(id), &mut used);
        let text = serde_json::to_string_pretty(entry)
            .map_err(|error| format!("序列化智能体失败: {error}"))?;
        write_text(&directory.join(format!("{stem}.json")), &text)?;
        ids.push(Value::String(id.to_string()));
    }
    order.insert("agents".into(), Value::Array(ids));
    remove_stale(&directory, "json", &used);
    Ok(())
}

/// 所属模块：surfing-ai-data / 资料库文件存储
/// 功能：把基础技能或编曲技巧写成 Markdown，并按清单顺序记录 id。
/// 输入：`directory` 目标目录；`library`；`key` 数组键；`kind` 文件类型；`order` 顺序表。
/// 输出/副作用：写文件并清理过期 Markdown；失败返回原因。
/// 复杂度：时间 O(条目数 × 正文长度)，空间 O(单条正文)。
///
/// Module: surfing-ai-data / library file storage
/// Purpose: Write theory skills or techniques as Markdown and record their order.
/// Inputs: `directory`; `library`; `key`; `kind`; `order`.
/// Outputs / side effects: writes files and removes stale Markdown; returns an error on failure.
/// Complexity: O(entries × body length) time and O(one body) space.
fn save_skills(
    directory: &Path,
    library: &Value,
    key: &str,
    kind: &str,
    order: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|error| format!("创建技能目录失败: {error}"))?;
    let mut used = BTreeSet::new();
    let mut ids = Vec::new();
    for entry in array_of(library, key) {
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        if id.trim().is_empty() {
            continue;
        }
        let title = entry
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let summary = entry
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let body = entry
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let stem = unique_stem(&sanitize_id(id), &mut used);
        write_text(
            &directory.join(format!("{stem}.md")),
            &markdown::encode(id, title, summary, kind, body),
        )?;
        ids.push(Value::String(id.to_string()));
    }
    order.insert(key.into(), Value::Array(ids));
    remove_stale(directory, "md", &used);
    Ok(())
}

/// 所属模块：surfing-ai-data / 资料库文件存储
/// 功能：把乐器资料写成 JSON，并按清单顺序记录 id。
/// 输入：`root` 资料库根；`library`；`order` 顺序表。
/// 输出/副作用：写文件并清理过期 JSON；失败返回原因。
/// 复杂度：时间 O(乐器数 × 条目大小)，空间 O(单条大小)。
///
/// Module: surfing-ai-data / library file storage
/// Purpose: Write instrument notes as JSON and record their order.
/// Inputs: `root`; `library`; `order`.
/// Outputs / side effects: writes files and removes stale JSON; returns an error on failure.
/// Complexity: O(instruments × entry size) time and O(one entry) space.
fn save_instruments(
    root: &Path,
    library: &Value,
    order: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let directory = root.join(SKILLS_DIR).join(INSTRUMENTS_DIR);
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("创建乐器资料目录失败: {error}"))?;
    let mut used = BTreeSet::new();
    let mut ids = Vec::new();
    for entry in array_of(library, "instruments") {
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        if id.trim().is_empty() {
            continue;
        }
        let stem = unique_stem(&sanitize_id(id), &mut used);
        let text = serde_json::to_string_pretty(entry)
            .map_err(|error| format!("序列化乐器资料失败: {error}"))?;
        write_text(&directory.join(format!("{stem}.json")), &text)?;
        ids.push(Value::String(id.to_string()));
    }
    order.insert("instruments".into(), Value::Array(ids));
    remove_stale(&directory, "json", &used);
    Ok(())
}

/// 所属模块：surfing-ai-data / 资料库文件存储
/// 功能：读取 JSON 条目目录，并按清单中的 id 顺序排列。
/// 输入：`directory`；`manifest` 清单；`key` 数组键。
/// 输出/副作用：条目数组；目录不存在时返回空数组。
/// 复杂度：时间 O(文件数 × 文件大小)，空间 O(条目数)。
///
/// Module: surfing-ai-data / library file storage
/// Purpose: Read a JSON entry directory and order it by the manifest ids.
/// Inputs: `directory`; `manifest`; `key`.
/// Outputs / side effects: entry array; an absent directory yields an empty array.
/// Complexity: O(files × file size) time and O(entries) space.
fn load_json_entries(
    directory: &Path,
    manifest: &(u64, Value),
    key: &str,
) -> Result<Vec<Value>, String> {
    let mut entries = Vec::new();
    for path in sorted_files(directory, "json") {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else {
            continue;
        };
        if value.is_object() {
            entries.push(value);
        }
    }
    Ok(order_entries(entries, &manifest.1, key))
}

/// 所属模块：surfing-ai-data / 资料库文件存储
/// 功能：读取 Markdown 技能目录，解析头部并生成技能条目。
/// 输入：`directory`；`manifest`；`key` 数组键。
/// 输出/副作用：技能数组；没有头部的文件用文件名作为标题。
/// 复杂度：时间 O(文件数 × 文件大小)，空间 O(条目数)。
///
/// Module: surfing-ai-data / library file storage
/// Purpose: Read a Markdown skill directory, parse headers and build skill entries.
/// Inputs: `directory`; `manifest`; `key`.
/// Outputs / side effects: skill array; files without a header use the file stem as title.
/// Complexity: O(files × file size) time and O(entries) space.
fn load_markdown_entries(
    directory: &Path,
    manifest: &(u64, Value),
    key: &str,
) -> Result<Vec<Value>, String> {
    let mut entries = Vec::new();
    for path in sorted_files(directory, "md") {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let (header, body) = markdown::decode(&text);
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("entry");
        let id = if header.id.trim().is_empty() {
            sanitize_id(stem)
        } else {
            header.id
        };
        let title = if header.title.trim().is_empty() {
            stem.to_string()
        } else {
            header.title
        };
        entries.push(json!({
            "id": id,
            "title": title,
            "summary": header.summary,
            "body": body,
        }));
    }
    Ok(order_entries(entries, &manifest.1, key))
}

/// 所属模块：surfing-ai-data / 资料库文件存储
/// 功能：读取清单中的版本号与顺序表；缺失或损坏时返回默认值。
/// 输入：`root` 资料库根目录。
/// 输出/副作用：`(版本, 顺序 JSON)`；只读文件。
/// 复杂度：时间 O(清单大小)，空间 O(清单大小)。
///
/// Module: surfing-ai-data / library file storage
/// Purpose: Read the manifest version and order table; fall back when missing or broken.
/// Inputs: `root`.
/// Outputs / side effects: `(version, order JSON)`; reads a file only.
/// Complexity: O(manifest size) time and space.
fn read_manifest(root: &Path) -> (u64, Value) {
    let Ok(text) = std::fs::read_to_string(root.join(MANIFEST)) else {
        return (DEFAULT_VERSION, Value::Null);
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return (DEFAULT_VERSION, Value::Null);
    };
    let version = value
        .get("version")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_VERSION);
    let order = value.get("order").cloned().unwrap_or(Value::Null);
    (version, order)
}

/// 所属模块：surfing-ai-data / 资料库文件存储
/// 功能：按清单顺序重排条目；清单未列出的条目按文件顺序追加。
/// 输入：`entries` 条目；`order` 清单；`key` 数组键。
/// 输出/副作用：重排后的条目；不修改输入。
/// 复杂度：时间 O(条目数²)，空间 O(条目数)。
///
/// Module: surfing-ai-data / library file storage
/// Purpose: Reorder entries by the manifest and append unlisted ones in file order.
/// Inputs: `entries`; `order`; `key`.
/// Outputs / side effects: reordered entries; the inputs are unchanged.
/// Complexity: O(entries²) time and O(entries) space.
fn order_entries(entries: Vec<Value>, order: &Value, key: &str) -> Vec<Value> {
    let Some(ids) = order.get(key).and_then(Value::as_array) else {
        return entries;
    };
    let mut remaining = entries;
    let mut ordered = Vec::new();
    for id in ids.iter().filter_map(Value::as_str) {
        if let Some(index) = remaining
            .iter()
            .position(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
        {
            ordered.push(remaining.remove(index));
        }
    }
    ordered.extend(remaining);
    ordered
}

/// 所属模块：surfing-ai-data / 资料库文件存储
/// 功能：列出目录中指定扩展名的文件并按名称排序。
/// 输入：`directory`；`extension` 不带点。
/// 输出/副作用：路径列表；目录不存在时返回空列表。
/// 复杂度：时间 O(目录项数 log 目录项数)，空间 O(目录项数)。
///
/// Module: surfing-ai-data / library file storage
/// Purpose: List files with the given extension sorted by name.
/// Inputs: `directory`; `extension`.
/// Outputs / side effects: path list; an absent directory yields an empty list.
/// Complexity: O(entries log entries) time and O(entries) space.
fn sorted_files(directory: &Path, extension: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        })
        .collect();
    files.sort();
    files
}

/// 所属模块：surfing-ai-data / 资料库文件存储
/// 功能：删除目录中不在本次写入集合内的旧文件，保持文件与资料库一致。
/// 输入：`directory`；`extension`；`kept` 已写入的文件主名。
/// 输出/副作用：删除过期文件；删除失败被忽略（下次保存会重试）。
/// 复杂度：时间 O(目录项数)，空间 O(1)。
///
/// Module: surfing-ai-data / library file storage
/// Purpose: Remove old files whose stems were not written, keeping files and library consistent.
/// Inputs: `directory`; `extension`; `kept`.
/// Outputs / side effects: deletes stale files; failures are ignored and retried on the next save.
/// Complexity: O(directory entries) time and O(1) space.
fn remove_stale(directory: &Path, extension: &str, kept: &BTreeSet<String>) {
    for path in sorted_files(directory, extension) {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        if !kept.contains(stem) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// 所属模块：surfing-ai-data / 导出
/// 功能：返回资料库中某一类的数组。
/// 输入：`library`；`kind`。
/// 输出/副作用：数组引用；类型非法时返回中文错误。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: surfing-ai-data / export
/// Purpose: Return the array of one kind from the library.
/// Inputs: `library`; `kind`.
/// Outputs / side effects: array reference; a Chinese error for unknown kinds.
/// Complexity: O(1) time and space.
fn entries_of<'a>(library: &'a Value, kind: &str) -> Result<&'a Vec<Value>, String> {
    let key = match kind {
        "agent" => "agents",
        "basic" => "basics",
        "technique" => "techniques",
        "instrument" => "instruments",
        other => return Err(format!("未知的资料类型：{other}")),
    };
    library
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("资料库缺少数组字段 {key}"))
}

/// 所属模块：surfing-ai-data / 资料库文件存储
/// 功能：返回资料库中的数组；缺失时返回空切片。
/// 输入：`library`；`key`。
/// 输出/副作用：数组切片；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: surfing-ai-data / library file storage
/// Purpose: Return an array from the library; missing keys yield an empty slice.
/// Inputs: `library`; `key`.
/// Outputs / side effects: array slice; no state change.
/// Complexity: O(1) time and space.
fn array_of<'a>(library: &'a Value, key: &str) -> &'a [Value] {
    library
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// 所属模块：surfing-ai-data / 导出
/// 功能：按类型把条目编码为文件文本。
/// 输入：`kind`；`entry`。
/// 输出/副作用：文件文本；序列化失败返回中文错误。
/// 复杂度：时间 O(条目大小)，空间 O(条目大小)。
///
/// Module: surfing-ai-data / export
/// Purpose: Encode one entry into file text for its kind.
/// Inputs: `kind`; `entry`.
/// Outputs / side effects: file text; a Chinese error on serialization failure.
/// Complexity: O(entry size) time and space.
fn entry_text(kind: &str, entry: &Value) -> Result<String, String> {
    if matches!(kind, "basic" | "technique") {
        let id = entry.get("id").and_then(Value::as_str).unwrap_or_default();
        let title = entry
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let summary = entry
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let body = entry
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default();
        return Ok(markdown::encode(id, title, summary, kind, body));
    }
    serde_json::to_string_pretty(entry).map_err(|error| format!("序列化导出内容失败: {error}"))
}

/// 所属模块：surfing-ai-data / 导出
/// 功能：返回类型对应的分类子目录与扩展名。
/// 输入：`kind` 类型。
/// 输出/副作用：目录名或扩展名；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: surfing-ai-data / export
/// Purpose: Return the category directory or extension for a kind.
/// Inputs: `kind`.
/// Outputs / side effects: directory name or extension; no state change.
/// Complexity: O(1) time and space.
fn category_dir(kind: &str) -> &'static str {
    match kind {
        "agent" => AGENTS_DIR,
        "basic" => "skills/basics",
        "technique" => "skills/techniques",
        "instrument" => "skills/instruments",
        _ => "skills",
    }
}

/// 所属模块：surfing-ai-data / 导出
/// 功能：返回类型对应的文件扩展名。
/// 输入：`kind` 类型。
/// 输出/副作用：扩展名；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: surfing-ai-data / export
/// Purpose: Return the file extension for a kind.
/// Inputs: `kind`.
/// Outputs / side effects: extension; no state change.
/// Complexity: O(1) time and space.
fn extension(kind: &str) -> &'static str {
    if matches!(kind, "basic" | "technique") {
        "md"
    } else {
        "json"
    }
}

/// 所属模块：surfing-ai-data / 资料库文件存储
/// 功能：把 id 转为安全文件主名，只保留 ASCII 字母数字、`-` 与 `_`。
/// 输入：`id` 条目 id。
/// 输出/副作用：文件主名；为空时返回 `entry`。
/// 复杂度：时间 O(id 长度)，空间 O(id 长度)。
///
/// Module: surfing-ai-data / library file storage
/// Purpose: Turn an id into a safe file stem keeping ASCII alphanumerics, `-` and `_`.
/// Inputs: `id`.
/// Outputs / side effects: file stem; empty ids return `entry`.
/// Complexity: O(id length) time and space.
fn sanitize_id(id: &str) -> String {
    let mut stem = String::new();
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            stem.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() && !stem.is_empty() {
            stem.push('-');
        }
    }
    let stem = stem.trim_matches('-').to_string();
    if stem.is_empty() {
        "entry".into()
    } else {
        stem
    }
}

/// 所属模块：surfing-ai-data / 资料库文件存储
/// 功能：在已用集合中生成唯一文件主名（冲突时追加 `-2`、`-3`…）。
/// 输入：`base` 基础主名；`used` 已用集合。
/// 输出/副作用：唯一主名；同时写入 `used`。
/// 复杂度：时间 O(冲突数)，空间 O(1)。
///
/// Module: surfing-ai-data / library file storage
/// Purpose: Build a unique file stem inside the used set, appending `-2`, `-3`, ...
/// Inputs: `base`; `used`.
/// Outputs / side effects: unique stem; also inserts it into `used`.
/// Complexity: O(collisions) time and O(1) space.
fn unique_stem(base: &str, used: &mut BTreeSet<String>) -> String {
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    for suffix in 2..=10_000 {
        let candidate = format!("{base}-{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    base.to_string()
}

/// 所属模块：surfing-ai-data / 资料库文件存储
/// 功能：写入文本文件，自动创建父目录。
/// 输入：`path` 目标；`text` 内容。
/// 输出/副作用：创建目录并写文件；失败返回中文原因。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: surfing-ai-data / library file storage
/// Purpose: Write a text file and create its parent directory.
/// Inputs: `path`; `text`.
/// Outputs / side effects: creates directories and writes the file; returns a Chinese error.
/// Complexity: O(text length) time and O(1) space.
fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建目录失败: {error}"))?;
    }
    std::fs::write(path, text).map_err(|error| format!("写入文件失败: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：保存后再加载保持四个分类的条目、顺序与正文。
    /// Covers: saving then loading keeps entries, order and bodies of all four categories.
    #[test]
    fn save_and_load_round_trip() {
        let root =
            std::env::temp_dir().join(format!("surfing_ai_data_round_trip_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let library = json!({
            "version": 3,
            "agents": [
                {"id": "light-music", "name": "轻音乐", "description": "", "style": "柔和",
                 "default_skill_ids": ["music-theory"], "default_technique_ids": [],
                 "default_instrument_ids": []},
                {"id": "j-pop", "name": "J-Pop", "description": "", "style": "强 Hook"}
            ],
            "basics": [
                {"id": "music-theory", "title": "基础乐理", "summary": "和声", "body": "# 正文\n内容"}
            ],
            "techniques": [
                {"id": "groove-pocket", "title": "律动", "summary": " pocket ", "body": "正文"}
            ],
            "instruments": [
                {"id": "instrument-synth", "name": "合成器", "category": "合成器"}
            ]
        });
        save(&root, &library).expect("save");
        let loaded = load(&root).expect("load");
        assert_eq!(loaded["agents"][0]["id"], "light-music");
        assert_eq!(loaded["agents"][1]["id"], "j-pop");
        assert_eq!(loaded["basics"][0]["body"], "# 正文\n内容");
        assert_eq!(loaded["techniques"][0]["title"], "律动");
        assert_eq!(loaded["instruments"][0]["name"], "合成器");

        // 删除一条后再次保存，过期文件必须被清理。
        // Save again after removing an entry; its stale file must be removed.
        let mut trimmed = library;
        trimmed["basics"] = json!([]);
        save(&root, &trimmed).expect("save trimmed");
        assert!(
            load(&root).expect("reload")["basics"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 覆盖：导入 Markdown 技能时识别类型、标题与正文。
    /// Covers: importing Markdown recognizes kind, title and body.
    #[test]
    fn import_markdown_skill() {
        let path =
            std::env::temp_dir().join(format!("surfing_ai_data_import_{}.md", std::process::id()));
        std::fs::write(
            &path,
            markdown::encode("custom-skill", "自定义技能", "测试", "technique", "正文"),
        )
        .expect("write");
        let (kind, entry) = import(&path, "basic").expect("import");
        // 文件头部明确写了 kind=technique 时以文件为准，页面提示只作为缺省值。
        // An explicit kind=technique in the header wins; the page hint is only a fallback.
        assert_eq!(kind, "technique");
        assert_eq!(entry["id"], "custom-skill");
        assert_eq!(entry["title"], "自定义技能");
        let _ = std::fs::remove_file(path);
    }
}
