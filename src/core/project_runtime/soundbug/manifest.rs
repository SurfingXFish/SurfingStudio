//! 所属模块：core / 工程运行时 / SoundBug 后端 / 清单
//! 文件用途：
//! - 解析 `.sndp` 文件头、属性与媒体表。
//! - 解压 `comp.dat` 并解析媒体相对路径。
//!
//! 函数清单：
//! - `parse_sndp`：解析清单并返回工程名、媒体表与 `comp.dat` 路径。
//! - `parse_media`：读取媒体对象。
//! - `read_comp`：zlib 解压 `comp.dat`。
//! - `resolve_path`：解析相对媒体路径。
//!
//! Module: core / project runtime / SoundBug backend / manifest
//! Purpose:
//! - Parse the `.sndp` header, properties and media table.
//! - Decompress `comp.dat` and resolve relative media paths.
//!
//! Functions:
//! - `parse_sndp`: parse the manifest and return the name, media table and `comp.dat` path.
//! - `parse_media`: read media objects.
//! - `read_comp`: decompress `comp.dat` with zlib.
//! - `resolve_path`: resolve a relative media path.

use super::edit::{trim_nul, Reader};
use flate2::read::ZlibDecoder;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// `.sndp` 文件头的魔数。
/// Magic bytes at the start of an `.sndp` manifest.
const SNDP_MAGIC: &[u8; 4] = b"SNDP";

/// `comp.dat` 解压后的上限，避免损坏文件耗尽内存。
/// Upper bound for the decompressed `comp.dat`, protecting against corrupt files.
const MAX_COMP_BYTES: u64 = 256 * 1024 * 1024;

/// 单个清单属性允许的最大字节数。
/// Maximum size of a single manifest property.
const MAX_PROPERTY_BYTES: u32 = 16 * 1024 * 1024;

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 清单
/// 功能：解析 `.sndp` 文件头、属性与媒体对象。
/// 输入：
/// - `path`：`.sndp` 文件路径。
///
/// 输出/副作用：返回工程名、媒体表与 `comp.dat` 路径；只读文件。
///
/// 复杂度：时间 O(文件大小)，空间 O(媒体数量 + 属性大小)。
///
/// Module: core / project runtime / SoundBug backend / manifest
/// Purpose: Parse the `.sndp` header, properties and media objects.
/// Inputs:
/// - `path`: path to the `.sndp` file.
///
/// Outputs / side effects: returns the project name, media table and `comp.dat` path;
/// the file is only read.
///
/// Complexity: O(file size) time and O(media count + property bytes) space.
pub(super) fn parse_sndp(path: &Path) -> Result<SoundBugManifest, String> {
    let data = std::fs::read(path).map_err(|error| format!("无法读取 SoundBug 工程: {error}"))?;
    if data.len() < 16 || &data[0..4] != SNDP_MAGIC {
        return Err("不是有效的 SoundBug .sndp 工程文件".into());
    }
    let mut reader = Reader::new(&data);
    reader.pos = 4;
    // 头部的两个字段用途未公开，导入时跳过。
    // Two header fields are undocumented; skip them during import.
    let _unknown = reader.read_u32_le()?;
    let _flags = reader.read_u32_le()?;
    let property_count = reader.read_u32_le()? as usize;
    if property_count > 4096 {
        return Err("SoundBug 清单属性数量异常".into());
    }
    let mut properties = HashMap::new();
    for _ in 0..property_count {
        let key = reader.read_cstr()?;
        let length = reader.read_u32_le()?;
        if length > MAX_PROPERTY_BYTES {
            return Err(format!("SoundBug 清单属性过大: {key}"));
        }
        let value = reader.read_bytes(length as usize)?.to_vec();
        properties.insert(key, trim_nul(&value));
    }

    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let media = parse_media(&mut reader, base_dir)?;
    let comp_path = media
        .iter()
        .find(|media| media.kind.eq_ignore_ascii_case("edit"))
        .map(|media| media.path.clone());

    let name = properties
        .get("name")
        .map(|bytes| String::from_utf8_lossy(bytes).to_string())
        .unwrap_or_default();
    let duration_seconds = media
        .iter()
        .find(|media| media.kind.eq_ignore_ascii_case("edit"))
        .map(|media| media.duration_seconds)
        .unwrap_or(0.0);

    Ok(SoundBugManifest {
        name,
        media,
        comp_path,
        duration_seconds,
    })
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 清单
/// 功能：从清单尾部读取媒体对象（名称、类型、分类、相对路径、时长）。
/// 输入：
/// - `reader`：位于媒体表起始位置的读取器。
/// - `base_dir`：`.sndp` 所在目录，用于解析相对路径。
///
/// 输出/副作用：返回媒体列表并推进读取器；媒体对象损坏时返回错误。
///
/// 复杂度：时间 O(媒体数量 × 字符串长度)，空间 O(媒体数量)。
///
/// Module: core / project runtime / SoundBug backend / manifest
/// Purpose: Read media objects (name, kind, category, relative path, duration) from
/// the manifest tail.
/// Inputs:
/// - `reader`: reader positioned at the media table.
/// - `base_dir`: directory of the `.sndp` file, used to resolve relative paths.
///
/// Outputs / side effects: returns the media list and advances the reader; returns an
/// error when a media object is corrupt.
///
/// Complexity: O(media count × string length) time and O(media count) space.
fn parse_media(reader: &mut Reader<'_>, base_dir: &Path) -> Result<Vec<SoundBugMedia>, String> {
    let mut media = Vec::new();
    while reader.pos + 5 < reader.data.len() && reader.data[reader.pos] >= 0x20 {
        let name = reader.read_cstr()?;
        let kind = reader.read_cstr()?;
        let _category = reader.read_cstr()?;
        let relative = reader.read_cstr()?;
        let duration_seconds = reader.read_f64_le()?;
        if name.is_empty() || kind.is_empty() {
            return Err("SoundBug 媒体对象缺少名称或类型".into());
        }
        media.push(SoundBugMedia {
            name,
            kind,
            path: resolve_path(base_dir, &relative),
            duration_seconds,
        });
    }
    Ok(media)
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 清单
/// 功能：使用 zlib 解压 `comp.dat`，并限制解压后的最大体积。
/// 输入：
/// - `path`：`comp.dat` 路径。
///
/// 输出/副作用：返回解压后的字节；只读文件。
///
/// 复杂度：时间 O(解压后大小)，空间 O(解压后大小)。
///
/// Module: core / project runtime / SoundBug backend / manifest
/// Purpose: Decompress `comp.dat` with zlib and cap the decompressed size.
/// Inputs:
/// - `path`: path to `comp.dat`.
///
/// Outputs / side effects: returns the decompressed bytes; the file is only read.
///
/// Complexity: O(decompressed size) time and space.
pub(super) fn read_comp(path: &Path) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|error| format!("无法读取 comp.dat: {error}"))?;
    let mut decoder = ZlibDecoder::new(file).take(MAX_COMP_BYTES + 1);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|error| format!("解压 comp.dat 失败: {error}"))?;
    if out.len() as u64 > MAX_COMP_BYTES {
        return Err("comp.dat 解压后过大，已停止导入".into());
    }
    Ok(out)
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 清单
/// 功能：把相对媒体路径解析为绝对路径。
/// 输入：
/// - `base_dir`：工程目录。
/// - `relative`：清单中的相对路径。
///
/// 输出/副作用：返回解析后的路径。
///
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: core / project runtime / SoundBug backend / manifest
/// Purpose: Resolve a relative media path against the project directory.
/// Inputs:
/// - `base_dir`: project directory.
/// - `relative`: relative path from the manifest.
///
/// Outputs / side effects: returns the resolved path.
///
/// Complexity: O(path length) time and space.
fn resolve_path(base_dir: &Path, relative: &str) -> PathBuf {
    let path = Path::new(relative);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

/// SoundBug 清单中的媒体对象。
/// A media object from a SoundBug manifest.
pub(super) struct SoundBugMedia {
    pub(super) name: String,
    pub(super) kind: String,
    pub(super) path: PathBuf,
    pub(super) duration_seconds: f64,
}

/// SoundBug 清单解析结果。
/// Parsed SoundBug manifest.
pub(super) struct SoundBugManifest {
    pub(super) name: String,
    pub(super) media: Vec<SoundBugMedia>,
    pub(super) comp_path: Option<PathBuf>,
    pub(super) duration_seconds: f64,
}
