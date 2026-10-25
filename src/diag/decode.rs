//! 所属模块：diag / 日志解码
//! 文件用途：
//! - 把 `.sflog` v2 二进制日志解码为便于阅读的文本（人工排障与外部工具参考实现）。
//! - 校验魔数、版本与每条记录的 CRC32；损坏或截断时给出明确标记而不是静默跳过。
//!
//! 函数清单：
//! - `decode_log_file`：读取并解码日志文件为文本。
//! - `export_text`：解码后写回同目录的 `.txt` 文件，返回目标路径。
//! - `decode_bytes` / `format_record` / `read_value`：内部解析实现。
//!
//! Module: diag / log decoding
//! Purpose:
//! - Decode a `.sflog` v2 binary log into readable text (human triage and a reference decoder).
//! - Validate magic, version and each record's CRC32; damaged/truncated data is flagged instead of
//!   silently skipped.
//!
//! Functions:
//! - `decode_log_file`: read and decode a log file into text.
//! - `export_text`: decode and write a `.txt` next to the log, returning the target path.
//! - `decode_bytes` / `format_record` / `read_value`: internal parsing.

use super::format::{get_str, get_varint};
use super::{
    Level, FIELD_STR, FORMAT_MAGIC, FORMAT_VERSION, HEADER_PREFIX_LEN, RECORD_EVENT, RECORD_FOOTER,
    RECORD_HEADER_LEN, RECORD_RESOURCE,
};
use std::path::{Path, PathBuf};

/// 所属模块：diag / 日志解码
/// 功能：读取并解码一个 `.sflog` 文件。
/// 输入：`path`：日志文件路径。
/// 输出/副作用：返回完整文本；文件不可读、格式不匹配或头部损坏时返回中文错误。
/// 复杂度：时间 O(文件大小)，空间 O(文件大小 + 文本长度)。
///
/// Module: diag / log decoding
/// Purpose: Read and decode one `.sflog` file.
/// Inputs: `path`: log file path.
/// Outputs / side effects: the decoded text; a Chinese error when the file is unreadable, has the
/// wrong format or a damaged header.
/// Complexity: O(file size) time and O(file size + text length) space.
pub fn decode_log_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("读取日志失败: {error}"))?;
    decode_bytes(&bytes)
}

/// 所属模块：diag / 日志解码
/// 功能：解码日志并把文本写到同目录的 `.txt` 文件。
/// 输入：`path`：日志文件路径。
/// 输出/副作用：创建/覆盖 `<日志名>.txt`；返回目标路径或写入错误。
/// 复杂度：时间 O(文件大小)，空间 O(文件大小)。
///
/// Module: diag / log decoding
/// Purpose: Decode a log and write the text next to it as `.txt`.
/// Inputs: `path`: log file path.
/// Outputs / side effects: creates/overwrites `<log name>.txt`; returns the target path or a write
/// error.
/// Complexity: O(file size) time and space.
pub fn export_text(path: &Path) -> Result<PathBuf, String> {
    let text = decode_log_file(path)?;
    let target = path.with_extension("txt");
    std::fs::write(&target, text).map_err(|error| format!("写入解码文本失败: {error}"))?;
    Ok(target)
}

/// 所属模块：diag / 日志解码
/// 功能：解码内存中的完整日志字节。
/// 输入：`bytes`：文件全部字节。
/// 输出/副作用：返回文本；格式不匹配时返回错误，记录损坏时在文本中标记。
/// 复杂度：时间 O(文件大小)，空间 O(文本长度)。
///
/// Module: diag / log decoding
/// Purpose: Decode a complete log held in memory.
/// Inputs: `bytes`: all file bytes.
/// Outputs / side effects: the text; a format error, or inline markers for damaged records.
/// Complexity: O(file size) time and O(text length) space.
fn decode_bytes(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() < HEADER_PREFIX_LEN {
        return Err("文件过短，不是有效的 .sflog".into());
    }
    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap_or_default());
    if magic != FORMAT_MAGIC {
        return Err("不是 .sflog 文件（魔数不匹配）".into());
    }
    let version = u16::from_le_bytes(bytes[4..6].try_into().unwrap_or_default());
    if version != FORMAT_VERSION {
        return Err(format!(
            "不支持的日志版本 {version}（当前支持 {FORMAT_VERSION}）"
        ));
    }
    let header_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap_or_default()) as usize;
    let pid = u32::from_le_bytes(bytes[12..16].try_into().unwrap_or_default());
    let start_unix_us = u64::from_le_bytes(bytes[16..24].try_into().unwrap_or_default());
    let header_end = HEADER_PREFIX_LEN
        .checked_add(header_len)
        .ok_or_else(|| "日志头长度溢出".to_string())?;
    if bytes.len() < header_end {
        return Err("日志头被截断".into());
    }
    let mut text = String::with_capacity(bytes.len());
    text.push_str(&format!(
        "# SFLG v{version} pid={pid} start_unix_us={start_unix_us}\n"
    ));
    let header = String::from_utf8_lossy(&bytes[HEADER_PREFIX_LEN..header_end]);
    for line in header.lines() {
        text.push_str("# ");
        text.push_str(line);
        text.push('\n');
    }
    let mut rest = &bytes[header_end..];
    while !rest.is_empty() {
        if rest.len() < RECORD_HEADER_LEN {
            text.push_str(&format!("# 截断的记录头：剩余 {} 字节\n", rest.len()));
            break;
        }
        let kind = rest[0];
        let level = Level::from_code(rest[1]);
        let seq = u32::from_le_bytes(rest[4..8].try_into().unwrap_or_default());
        let thread = u32::from_le_bytes(rest[8..12].try_into().unwrap_or_default());
        let t_us = u64::from_le_bytes(rest[12..20].try_into().unwrap_or_default());
        let frame = u32::from_le_bytes(rest[20..24].try_into().unwrap_or_default());
        let payload_len = u32::from_le_bytes(rest[24..28].try_into().unwrap_or_default()) as usize;
        let crc = u32::from_le_bytes(rest[28..32].try_into().unwrap_or_default());
        let record_end = RECORD_HEADER_LEN.saturating_add(payload_len);
        if rest.len() < record_end {
            text.push_str(&format!(
                "# 截断的记录 #{seq}：需要 {record_end} 字节，实际 {}\n",
                rest.len()
            ));
            break;
        }
        let payload = &rest[RECORD_HEADER_LEN..record_end];
        format_record(
            &mut text,
            seq,
            t_us,
            level,
            thread,
            frame,
            kind,
            payload,
            crc == crc32fast::hash(payload),
        );
        rest = &rest[record_end..];
    }
    Ok(text)
}

/// 所属模块：diag / 日志解码
/// 功能：把一条记录追加为一行文本。
/// 输入：
/// - `text`：输出缓冲。
/// - `seq` / `t_us` / `level` / `thread` / `frame`：记录头字段。
/// - `kind`：记录类型。
/// - `payload`：负载。
/// - `crc_ok`：CRC 是否匹配。
/// 输出/副作用：向 `text` 追加一行；解析失败时保留已有内容并标记。
/// 复杂度：时间 O(负载长度)，空间 O(行长度)。
///
/// Module: diag / log decoding
/// Purpose: Append one record as a text line.
/// Inputs:
/// - `text`: output buffer.
/// - `seq` / `t_us` / `level` / `thread` / `frame`: record-header fields.
/// - `kind`: record kind.
/// - `payload`: payload bytes.
/// - `crc_ok`: whether the CRC matches.
/// Outputs / side effects: appends one line; parse failures are marked without discarding output.
/// Complexity: O(payload length) time and O(line length) space.
fn format_record(
    text: &mut String,
    seq: u32,
    t_us: u64,
    level: Level,
    thread: u32,
    frame: u32,
    kind: u8,
    payload: &[u8],
    crc_ok: bool,
) {
    use std::fmt::Write as _;
    let _ = write!(
        text,
        "{seq:06} {:>12.3}ms {:5} tid={thread:<6} frame={frame:<6}",
        t_us as f64 / 1_000.0,
        level.label()
    );
    if !crc_ok {
        text.push_str(" !crc");
    }
    match kind {
        RECORD_EVENT => {
            let mut input = payload;
            let scope = get_str(&mut input).unwrap_or("?");
            let event = get_str(&mut input).unwrap_or("?");
            let _ = write!(text, "  {scope}/{event}");
            if let Some(duration) = get_varint(&mut input) {
                if duration != u64::MAX {
                    let _ = write!(text, "  dur={:.3}ms", duration as f64 / 1_000.0);
                }
            }
            let fields = get_varint(&mut input).unwrap_or(0);
            for _ in 0..fields {
                let Some(key) = get_str(&mut input) else {
                    text.push_str("  <坏字段>");
                    break;
                };
                let Some(value) = read_value(&mut input) else {
                    let _ = write!(text, "  {key}=<坏值>");
                    break;
                };
                let _ = write!(text, "  {key}={value}");
            }
        }
        RECORD_FOOTER => {
            let mut input = payload;
            let reason = get_str(&mut input).unwrap_or("?");
            let _ = write!(text, "  # end reason={reason}");
        }
        RECORD_RESOURCE => {
            if payload.len() >= 40 {
                let working_set = u64::from_le_bytes(payload[0..8].try_into().unwrap_or_default());
                let peak = u64::from_le_bytes(payload[8..16].try_into().unwrap_or_default());
                let private = u64::from_le_bytes(payload[16..24].try_into().unwrap_or_default());
                let cpu = u64::from_le_bytes(payload[24..32].try_into().unwrap_or_default());
                let handles = u32::from_le_bytes(payload[32..36].try_into().unwrap_or_default());
                let threads = u32::from_le_bytes(payload[36..40].try_into().unwrap_or_default());
                let _ = write!(
                    text,
                    "  resource ws={:.1}MiB peak={:.1}MiB private={:.1}MiB cpu={:.3}s handles={handles} threads={threads}",
                    working_set as f64 / 1_048_576.0,
                    peak as f64 / 1_048_576.0,
                    private as f64 / 1_048_576.0,
                    cpu as f64 / 10_000_000.0
                );
            } else {
                text.push_str("  resource <坏负载>");
            }
        }
        other => {
            let _ = write!(text, "  kind={other}  bytes={}", payload.len());
        }
    }
    text.push('\n');
}

/// 所属模块：diag / 日志解码
/// 功能：读取一个字段值；当前格式只写入字符串字段，其他类型保留占位。
/// 输入：`input`：负载游标。
/// 输出/副作用：返回展示文本；类型未知或数据截断时返回 `None`。
/// 复杂度：时间 O(值长度)，空间 O(值长度)。
///
/// Module: diag / log decoding
/// Purpose: Read one field value; the current writer only emits string fields, other types are
/// reserved.
/// Inputs: `input`: payload cursor.
/// Outputs / side effects: display text; `None` for unknown types or truncated data.
/// Complexity: O(value length) time and space.
fn read_value(input: &mut &[u8]) -> Option<String> {
    let kind = *input.first()?;
    *input = &input[1..];
    match kind {
        FIELD_STR => Some(get_str(input)?.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::format::{put_str, put_varint};
    use super::*;

    /// 覆盖：二进制头 + 一条事件 + 一条尾帧可被解码为文本。
    /// Covers: a header plus one event and one footer decode into text.
    #[test]
    fn decodes_header_event_and_footer() {
        let header_text = "app_version=0.1.0\nbuild=debug\n";
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&FORMAT_MAGIC.to_le_bytes());
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&(header_text.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&42_u32.to_le_bytes());
        bytes.extend_from_slice(&1_700_000_000_000_000_u64.to_le_bytes());
        bytes.extend_from_slice(header_text.as_bytes());

        let mut payload = Vec::new();
        put_str(&mut payload, "app.boot");
        put_str(&mut payload, "process.start");
        put_varint(&mut payload, u64::MAX);
        put_varint(&mut payload, 1);
        put_str(&mut payload, "pid");
        payload.push(FIELD_STR);
        put_str(&mut payload, "42");
        push_record(&mut bytes, 1, 2, 1, 1_500, 7, &payload);

        let mut footer = Vec::new();
        put_str(&mut footer, "home_closed");
        push_record(&mut bytes, 2, 2, 2, 2_000, 8, &footer);

        let text = decode_bytes(&bytes).expect("decode");
        assert!(text.contains("app_version=0.1.0"), "text={text}");
        assert!(text.contains("app.boot/process.start"), "text={text}");
        assert!(text.contains("pid=42"), "text={text}");
        assert!(text.contains("# end reason=home_closed"), "text={text}");
        assert!(!text.contains("!crc"), "text={text}");
    }

    /// 辅助：追加一条记录（测试用）。
    /// Helper: append one record (test only).
    fn push_record(
        bytes: &mut Vec<u8>,
        kind: u8,
        level: u8,
        seq: u32,
        t_us: u64,
        thread: u32,
        payload: &[u8],
    ) {
        let mut header = [0_u8; RECORD_HEADER_LEN];
        header[0] = kind;
        header[1] = level;
        header[4..8].copy_from_slice(&seq.to_le_bytes());
        header[8..12].copy_from_slice(&thread.to_le_bytes());
        header[12..20].copy_from_slice(&t_us.to_le_bytes());
        header[24..28].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        header[28..32].copy_from_slice(&crc32fast::hash(payload).to_le_bytes());
        bytes.extend_from_slice(&header);
        bytes.extend_from_slice(payload);
    }
}
