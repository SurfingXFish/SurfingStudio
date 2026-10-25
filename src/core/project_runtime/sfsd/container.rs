//! 所属模块：core / 工程运行时 / SurfingStudio 后端 / 容器
//! 文件用途：
//! - 提供 SFS2 分块容器的最小读写原语：文件头、目录表、数据块与 CRC 校验。
//! - 只负责字节层完整性，不感知 `Project` 结构。
//!
//! 函数清单：
//! - `ContainerWriter::create` / `write_chunk` / `finish`：容器写入状态机。
//! - `ContainerReader::open` / `entry` / `find` / `read_chunk`：容器读取与校验。
//! - `ChunkEntry::encode_into` / `decode`：目录条目编解码。
//! - `io_error`：统一文件错误文本。
//!
//! Module: core / project runtime / SurfingStudio backend / container
//! Purpose:
//! - Provide the minimal SFS2 chunk-container primitives: header, TOC, chunks and CRC.
//! - Handle byte-level integrity only; the `Project` structure is unknown here.
//!
//! Functions:
//! - `ContainerWriter::create` / `write_chunk` / `finish`: container write state machine.
//! - `ContainerReader::open` / `entry` / `find` / `read_chunk`: container read and validation.
//! - `ChunkEntry::encode_into` / `decode`: TOC entry encoding and decoding.
//! - `io_error`: normalize file error text.

use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub(super) const MAGIC: [u8; 4] = *b"SFS2";
pub(super) const FORMAT_VERSION: u16 = 2;
pub(super) const HEADER_LEN: u64 = 32;
pub(super) const ENTRY_LEN: usize = 36;
pub(super) const MAX_CHUNKS: u32 = 1_000_000;
pub(super) const MAX_CHUNK_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub(super) const ZSTD_LEVEL: i32 = 3;

pub(super) const TAG_META: [u8; 4] = *b"META";
pub(super) const TAG_PLUG: [u8; 4] = *b"PLUG";
pub(super) const TAG_PEAK: [u8; 4] = *b"PEAK";

pub(super) const CODEC_RAW: u8 = 0;
pub(super) const CODEC_ZSTD: u8 = 1;

/// 所属模块：core / 工程运行时 / SurfingStudio 后端 / 容器
/// 功能：把底层 IO 错误转换为统一的中文错误文本。
/// 输入：`error` 原始 IO 错误。
///
/// 输出/副作用：返回错误文本；不修改状态。
///
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / project runtime / SurfingStudio backend / container
/// Purpose: Normalize low-level IO errors into Chinese context text.
/// Inputs: `error` — the original IO error.
///
/// Outputs / side effects: returns the error text; no state change.
///
/// Complexity: O(1) time and space.
pub(super) fn io_error(error: std::io::Error) -> String {
    format!("文件读写失败: {error}")
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ChunkEntry {
    pub(super) tag: [u8; 4],
    pub(super) codec: u8,
    pub(super) offset: u64,
    pub(super) stored_len: u64,
    pub(super) raw_len: u64,
    pub(super) crc32: u32,
}

impl ChunkEntry {
    /// 所属模块：core / 工程运行时 / SurfingStudio 后端 / 容器
    /// 功能：把目录条目编码为固定长度的字节。
    /// 输入：`out` 输出缓冲。
    ///
    /// 输出/副作用：向 `out` 追加 `ENTRY_LEN` 字节。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / SurfingStudio backend / container
    /// Purpose: Encode a TOC entry into fixed-length bytes.
    /// Inputs: `out` — output buffer.
    ///
    /// Outputs / side effects: appends `ENTRY_LEN` bytes to `out`.
    ///
    /// Complexity: O(1) time and space.
    pub(super) fn encode_into(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.tag);
        out.push(self.codec);
        out.extend_from_slice(&[0, 0, 0]);
        out.extend_from_slice(&self.offset.to_le_bytes());
        out.extend_from_slice(&self.stored_len.to_le_bytes());
        out.extend_from_slice(&self.raw_len.to_le_bytes());
        out.extend_from_slice(&self.crc32.to_le_bytes());
    }

    /// 所属模块：core / 工程运行时 / SurfingStudio 后端 / 容器
    /// 功能：解码一个固定长度的目录条目。
    /// 输入：`bytes` 长度为 `ENTRY_LEN` 的字节切片。
    ///
    /// 输出/副作用：返回条目；长度不符时为 `None`。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / SurfingStudio backend / container
    /// Purpose: Decode one fixed-length TOC entry.
    /// Inputs: `bytes` — a slice of exactly `ENTRY_LEN` bytes.
    ///
    /// Outputs / side effects: returns the entry, or `None` on a length mismatch.
    ///
    /// Complexity: O(1) time and space.
    pub(super) fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != ENTRY_LEN {
            return None;
        }
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&bytes[0..4]);
        Some(Self {
            tag,
            codec: bytes[4],
            offset: u64::from_le_bytes(bytes[8..16].try_into().ok()?),
            stored_len: u64::from_le_bytes(bytes[16..24].try_into().ok()?),
            raw_len: u64::from_le_bytes(bytes[24..32].try_into().ok()?),
            crc32: u32::from_le_bytes(bytes[32..36].try_into().ok()?),
        })
    }
}

/// SFS2 容器写入器。
/// SFS2 container writer.
pub(super) struct ContainerWriter {
    file: BufWriter<File>,
    offset: u64,
    entries: Vec<ChunkEntry>,
}

impl ContainerWriter {
    /// 所属模块：core / 工程运行时 / SurfingStudio 后端 / 容器
    /// 功能：创建容器文件并预留文件头。
    /// 输入：`path` 目标文件路径。
    ///
    /// 输出/副作用：创建/截断目标文件；返回写入器。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / SurfingStudio backend / container
    /// Purpose: Create the container file and reserve the header.
    /// Inputs: `path` — destination path.
    ///
    /// Outputs / side effects: creates/truncates the destination; returns the writer.
    ///
    /// Complexity: O(1) time and space.
    pub(super) fn create(path: &Path) -> Result<Self, String> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(|error| format!("无法写入 {}: {error}", path.display()))?;
        let mut writer = Self {
            file: BufWriter::new(file),
            offset: 0,
            entries: Vec::new(),
        };
        let zeros = [0u8; HEADER_LEN as usize];
        writer.file.write_all(&zeros).map_err(io_error)?;
        writer.offset = HEADER_LEN;
        Ok(writer)
    }

    /// 所属模块：core / 工程运行时 / SurfingStudio 后端 / 容器
    /// 功能：写入一个数据块并记录目录条目。
    /// 输入：
    /// - `tag`：四字节块标签。
    /// - `codec`：`CODEC_RAW` 或 `CODEC_ZSTD`。
    /// - `raw`：未压缩的块数据。
    ///
    /// 输出/副作用：写入文件；返回块索引。
    ///
    /// 复杂度：时间 O(块大小)（压缩另计），空间 O(压缩后大小)。
    ///
    /// Module: core / project runtime / SurfingStudio backend / container
    /// Purpose: Write one chunk and record its TOC entry.
    /// Inputs:
    /// - `tag`: four-byte chunk tag.
    /// - `codec`: `CODEC_RAW` or `CODEC_ZSTD`.
    /// - `raw`: uncompressed chunk data.
    ///
    /// Outputs / side effects: writes the file; returns the chunk index.
    ///
    /// Complexity: O(chunk size) time (compression excluded) and O(compressed size) space.
    pub(super) fn write_chunk(
        &mut self,
        tag: [u8; 4],
        codec: u8,
        raw: &[u8],
    ) -> Result<u32, String> {
        let index = self.entries.len() as u32;
        let offset = self.offset;
        let crc = crc32fast::hash(raw);
        let stored_len = match codec {
            CODEC_ZSTD => {
                let compressed =
                    zstd::encode_all(raw, ZSTD_LEVEL).map_err(|e| format!("压缩失败: {e}"))?;
                self.file.write_all(&compressed).map_err(io_error)?;
                compressed.len() as u64
            }
            _ => {
                self.file.write_all(raw).map_err(io_error)?;
                raw.len() as u64
            }
        };
        self.offset += stored_len;
        self.entries.push(ChunkEntry {
            tag,
            codec,
            offset,
            stored_len,
            raw_len: raw.len() as u64,
            crc32: crc,
        });
        Ok(index)
    }

    /// 所属模块：core / 工程运行时 / SurfingStudio 后端 / 容器
    /// 功能：追写目录表与文件头并同步到磁盘。
    /// 输入：`self`。
    ///
    /// 输出/副作用：完成文件写入；失败时返回错误。
    ///
    /// 复杂度：时间 O(块数)，空间 O(块数)。
    ///
    /// Module: core / project runtime / SurfingStudio backend / container
    /// Purpose: Append the TOC and header, then sync to disk.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: finalizes the file; errors are returned.
    ///
    /// Complexity: O(chunk count) time and space.
    pub(super) fn finish(mut self) -> Result<(), String> {
        let toc_offset = self.offset;
        let mut toc = Vec::with_capacity(4 + self.entries.len() * ENTRY_LEN + 4);
        toc.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for entry in &self.entries {
            entry.encode_into(&mut toc);
        }
        let toc_crc = crc32fast::hash(&toc);
        toc.extend_from_slice(&toc_crc.to_le_bytes());
        self.file.write_all(&toc).map_err(io_error)?;
        let file_size = toc_offset + toc.len() as u64;

        let mut header = [0u8; HEADER_LEN as usize];
        header[0..4].copy_from_slice(&MAGIC);
        header[4..6].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        header[6..8].copy_from_slice(&0u16.to_le_bytes());
        header[8..12].copy_from_slice(&(self.entries.len() as u32).to_le_bytes());
        header[12..20].copy_from_slice(&toc_offset.to_le_bytes());
        header[20..28].copy_from_slice(&file_size.to_le_bytes());
        let header_crc = crc32fast::hash(&header[..28]);
        header[28..32].copy_from_slice(&header_crc.to_le_bytes());

        self.file.seek(SeekFrom::Start(0)).map_err(io_error)?;
        self.file.write_all(&header).map_err(io_error)?;
        self.file.flush().map_err(io_error)?;
        self.file.get_ref().sync_all().map_err(io_error)?;
        Ok(())
    }
}

/// SFS2 容器读取器。
/// SFS2 container reader.
pub(super) struct ContainerReader {
    file: BufReader<File>,
    entries: Vec<ChunkEntry>,
}

impl ContainerReader {
    /// 所属模块：core / 工程运行时 / SurfingStudio 后端 / 容器
    /// 功能：打开并校验容器文件头与目录表。
    /// 输入：`path` 工程文件路径。
    ///
    /// 输出/副作用：返回读取器；校验失败时返回错误。
    ///
    /// 复杂度：时间 O(TOC 大小)，空间 O(块数)。
    ///
    /// Module: core / project runtime / SurfingStudio backend / container
    /// Purpose: Open and validate the container header and TOC.
    /// Inputs: `path` — project file path.
    ///
    /// Outputs / side effects: returns the reader; validation failures are returned.
    ///
    /// Complexity: O(TOC size) time and O(chunk count) space.
    pub(super) fn open(path: &Path) -> Result<Self, String> {
        let mut file = BufReader::new(
            File::open(path).map_err(|error| format!("无法读取 {}: {error}", path.display()))?,
        );
        let mut header = [0u8; HEADER_LEN as usize];
        file.read_exact(&mut header).map_err(io_error)?;
        if header[0..4] != MAGIC {
            return Err("不是 SurfingStudio 工程文件（缺少 SFS2 标识）".into());
        }
        let format_version = u16::from_le_bytes([header[4], header[5]]);
        if format_version != FORMAT_VERSION {
            return Err(format!(
                "不支持的工程格式版本 {format_version}（当前需要 {FORMAT_VERSION}）"
            ));
        }
        let header_crc = u32::from_le_bytes([header[28], header[29], header[30], header[31]]);
        if crc32fast::hash(&header[..28]) != header_crc {
            return Err("工程文件头校验失败".into());
        }
        let count = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
        if count > MAX_CHUNKS {
            return Err("工程文件块数量异常".into());
        }
        let toc_offset = u64::from_le_bytes([
            header[12], header[13], header[14], header[15], header[16], header[17], header[18],
            header[19],
        ]);
        let file_size = u64::from_le_bytes([
            header[20], header[21], header[22], header[23], header[24], header[25], header[26],
            header[27],
        ]);
        if let Ok(metadata) = std::fs::metadata(path) {
            if metadata.len() != file_size {
                return Err("工程文件不完整（大小与文件头不一致）".into());
            }
        }

        let toc_len = 4 + count as usize * ENTRY_LEN + 4;
        if toc_offset
            .checked_add(toc_len as u64)
            .map_or(true, |end| end > file_size)
        {
            return Err("工程目录表位置无效".into());
        }
        file.seek(SeekFrom::Start(toc_offset)).map_err(io_error)?;
        let mut toc = vec![0u8; toc_len];
        file.read_exact(&mut toc).map_err(io_error)?;
        let (payload, crc_bytes) = toc.split_at(toc_len - 4);
        let expected_crc =
            u32::from_le_bytes([crc_bytes[0], crc_bytes[1], crc_bytes[2], crc_bytes[3]]);
        if crc32fast::hash(payload) != expected_crc {
            return Err("工程目录表校验失败".into());
        }
        let entry_count =
            u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
        if entry_count != count as usize {
            return Err("工程目录表损坏".into());
        }
        let entries: Vec<ChunkEntry> = (0..entry_count)
            .filter_map(|index| {
                let start = 4 + index * ENTRY_LEN;
                ChunkEntry::decode(&payload[start..start + ENTRY_LEN])
            })
            .collect();
        if entries.len() != entry_count {
            return Err("工程目录表损坏".into());
        }
        Ok(Self { file, entries })
    }

    /// 所属模块：core / 工程运行时 / SurfingStudio 后端 / 容器
    /// 功能：按目录表索引取数据块条目。
    /// 输入：`index` 条目索引。
    ///
    /// 输出/副作用：返回条目副本；越界时为 `None`。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / SurfingStudio backend / container
    /// Purpose: Look up a chunk entry by TOC index.
    /// Inputs: `index` — entry index.
    ///
    /// Outputs / side effects: returns a copy of the entry, or `None` when out of range.
    ///
    /// Complexity: O(1) time and space.
    pub(super) fn entry(&self, index: usize) -> Option<ChunkEntry> {
        self.entries.get(index).copied()
    }

    /// 所属模块：core / 工程运行时 / SurfingStudio 后端 / 容器
    /// 功能：按块标签查找第一个数据块条目。
    /// 输入：`tag` 四字节块标签。
    ///
    /// 输出/副作用：返回条目副本；不存在时为 `None`。
    ///
    /// 复杂度：时间 O(块数)，空间 O(1)。
    ///
    /// Module: core / project runtime / SurfingStudio backend / container
    /// Purpose: Find the first chunk entry with the given tag.
    /// Inputs: `tag` — four-byte chunk tag.
    ///
    /// Outputs / side effects: returns a copy of the entry, or `None` when absent.
    ///
    /// Complexity: O(chunk count) time and O(1) space.
    pub(super) fn find(&self, tag: [u8; 4]) -> Option<ChunkEntry> {
        self.entries.iter().find(|entry| entry.tag == tag).copied()
    }

    /// 所属模块：core / 工程运行时 / SurfingStudio 后端 / 容器
    /// 功能：读取并校验单个数据块（解压、长度、CRC）。
    /// 输入：`entry` 目标块条目。
    ///
    /// 输出/副作用：返回未压缩数据；校验失败时返回错误。
    ///
    /// 复杂度：时间 O(块大小)，空间 O(块大小)。
    ///
    /// Module: core / project runtime / SurfingStudio backend / container
    /// Purpose: Read and validate one chunk (decompression, length, CRC).
    /// Inputs: `entry` — target chunk entry.
    ///
    /// Outputs / side effects: returns uncompressed bytes; validation failures are returned.
    ///
    /// Complexity: O(chunk size) time and space.
    pub(super) fn read_chunk(&mut self, entry: &ChunkEntry) -> Result<Vec<u8>, String> {
        if entry.stored_len > MAX_CHUNK_BYTES || entry.raw_len > MAX_CHUNK_BYTES {
            return Err("工程数据块过大".into());
        }
        self.file
            .seek(SeekFrom::Start(entry.offset))
            .map_err(io_error)?;
        let mut stored = vec![0u8; entry.stored_len as usize];
        self.file.read_exact(&mut stored).map_err(io_error)?;
        let raw = match entry.codec {
            CODEC_RAW => stored,
            CODEC_ZSTD => zstd::decode_all(&stored[..])
                .map_err(|error| format!("工程数据解压失败: {error}"))?,
            other => return Err(format!("未知的工程数据编码 {other}")),
        };
        if raw.len() as u64 != entry.raw_len {
            return Err("工程数据块长度不一致".into());
        }
        if crc32fast::hash(&raw) != entry.crc32 {
            return Err("工程数据块校验失败".into());
        }
        Ok(raw)
    }
}
