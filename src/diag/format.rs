//! 所属模块：diag / 日志格式
//! 文件用途：
//! - `.sflog` v2 二进制格式的底层编码原语：varint、带长度字符串、本地时间与旧日志清理。
//! - 只做纯字节/文本处理，不持有文件句柄，便于单元测试与离线解码复用。
//!
//! 函数清单：
//! - `local_now`：返回本地日期时间与紧凑时间戳（文件名用）。
//! - `put_varint` / `get_varint`：LEB128 无符号变长整数编解码。
//! - `put_str` / `get_str`：长度前缀 UTF-8 字符串编解码。
//! - `prune_logs`：只保留最近的若干日志文件。
//! - `local_parts` / `civil_from_days`：本地时间分量与公历换算（内部）。
//!
//! Module: diag / log format
//! Purpose:
//! - Byte-level primitives for the `.sflog` v2 binary format: varints, length-prefixed strings,
//!   local timestamps and log pruning.
//! - Pure byte/text processing with no file handle, so it is testable and shared with the decoder.
//!
//! Functions:
//! - `local_now`: local date/time plus the compact stamp used in file names.
//! - `put_varint` / `get_varint`: LEB128 unsigned varint encoding/decoding.
//! - `put_str` / `get_str`: length-prefixed UTF-8 string encoding/decoding.
//! - `prune_logs`: keep only the newest log files.
//! - `local_parts` / `civil_from_days`: local time parts and civil-date conversion (internal).

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 本地时间格式化结果。
/// Local time formatting result.
pub(super) struct LocalNow {
    /// `YYYY-MM-DD HH:mm:ss.SSS`（本地时区）。
    /// `YYYY-MM-DD HH:mm:ss.SSS` in local time.
    pub datetime: String,
    /// `YYYYMMDD-HHmmss`（本地时区，用于文件名）。
    /// `YYYYMMDD-HHmmss` in local time, used in file names.
    pub compact: String,
}

/// 所属模块：diag / 日志格式
/// 功能：读取当前本地时间并生成两种时间文本。
/// 输入：无。
/// 输出/副作用：返回格式化文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: diag / log format
/// Purpose: Read the current local time and produce both timestamp texts.
/// Inputs: none.
/// Outputs / side effects: formatted text; no state change.
/// Complexity: O(1) time and space.
pub(super) fn local_now() -> LocalNow {
    let (year, month, day, hour, minute, second, millis) = local_parts();
    LocalNow {
        datetime: format!(
            "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{millis:03}"
        ),
        compact: format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}"),
    }
}

/// 所属模块：diag / 日志格式
/// 功能：把无符号整数按 LEB128 变长编码追加到缓冲。
/// 输入：
/// - `out`：目标缓冲。
/// - `value`：要编码的值。
/// 输出/副作用：向 `out` 追加 1~10 字节；不修改其他状态。
/// 复杂度：时间 O(字节数)，空间 O(1)。
///
/// Module: diag / log format
/// Purpose: Append an unsigned integer using LEB128 varint encoding.
/// Inputs:
/// - `out`: destination buffer.
/// - `value`: value to encode.
/// Outputs / side effects: appends 1-10 bytes to `out`; no other state change.
/// Complexity: O(bytes) time and O(1) space.
pub(super) fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// 所属模块：diag / 日志格式
/// 功能：从字节游标读取一个 LEB128 变长整数，并把游标前移。
/// 输入：`input`：剩余字节切片；读取后自动前移。
/// 输出/副作用：返回数值；数据截断或超过 10 字节时返回 `None`。
/// 复杂度：时间 O(字节数)，空间 O(1)。
///
/// Module: diag / log format
/// Purpose: Read one LEB128 varint from the byte cursor and advance it.
/// Inputs: `input`: remaining byte slice; advanced in place.
/// Outputs / side effects: the value, or `None` when truncated or longer than 10 bytes.
/// Complexity: O(bytes) time and O(1) space.
pub(super) fn get_varint(input: &mut &[u8]) -> Option<u64> {
    let mut value = 0_u64;
    let mut shift = 0_u32;
    for index in 0..10 {
        let byte = *input.first()?;
        *input = &input[1..];
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
        if index == 9 {
            break;
        }
    }
    None
}

/// 所属模块：diag / 日志格式
/// 功能：把 UTF-8 字符串按“变长长度 + 内容”写入缓冲。
/// 输入：
/// - `out`：目标缓冲。
/// - `text`：待写入文本。
/// 输出/副作用：追加长度与字节；不修改其他状态。
/// 复杂度：时间 O(文本长度)，空间 O(1) 额外。
///
/// Module: diag / log format
/// Purpose: Write a UTF-8 string as a varint length followed by its bytes.
/// Inputs:
/// - `out`: destination buffer.
/// - `text`: text to write.
/// Outputs / side effects: appends the length and bytes; no other state change.
/// Complexity: O(text length) time and O(1) extra space.
pub(super) fn put_str(out: &mut Vec<u8>, text: &str) {
    put_varint(out, text.len() as u64);
    out.extend_from_slice(text.as_bytes());
}

/// 所属模块：diag / 日志格式
/// 功能：从字节游标读取一个长度前缀 UTF-8 字符串，并把游标前移。
/// 输入：`input`：剩余字节切片。
/// 输出/副作用：返回字符串切片；长度非法或 UTF-8 无效时返回 `None`。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: diag / log format
/// Purpose: Read a length-prefixed UTF-8 string from the byte cursor and advance it.
/// Inputs: `input`: remaining byte slice.
/// Outputs / side effects: the string slice, or `None` on an invalid length or UTF-8.
/// Complexity: O(text length) time and O(1) space.
pub(super) fn get_str<'a>(input: &mut &'a [u8]) -> Option<&'a str> {
    let length = usize::try_from(get_varint(input)?).ok()?;
    if input.len() < length {
        return None;
    }
    let (text, rest) = input.split_at(length);
    *input = rest;
    std::str::from_utf8(text).ok()
}

/// 所属模块：diag / 日志格式
/// 功能：清理日志目录，只保留最近修改的 `keep` 个 `.sflog` 文件。
/// 输入：
/// - `dir`：日志目录。
/// - `keep`：保留数量，`0` 表示不清理。
/// 输出/副作用：删除较旧的日志文件；删除失败时忽略。
/// 复杂度：时间 O(日志文件数 × log 文件数)，空间 O(日志文件数)。
///
/// Module: diag / log format
/// Purpose: Prune the log directory, keeping only the newest `keep` `.sflog` files.
/// Inputs:
/// - `dir`: log directory.
/// - `keep`: number of files to keep; `0` disables pruning.
/// Outputs / side effects: removes older log files; failures are ignored.
/// Complexity: O(files × log files) time and O(files) space.
pub(super) fn prune_logs(dir: &Path, keep: usize) {
    if keep == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut logs: Vec<(SystemTime, std::path::PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().map(|ext| ext != "sflog").unwrap_or(true) {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .collect();
    if logs.len() <= keep {
        return;
    }
    logs.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in logs.into_iter().skip(keep) {
        let _ = std::fs::remove_file(path);
    }
}

/// 所属模块：diag / 日志格式
/// 功能：读取当前本地日期时间（含毫秒）；Windows 走时区换算，其他平台按 UTC。
/// 输入：无。
/// 输出/副作用：返回 `(年, 月, 日, 时, 分, 秒, 毫秒)`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: diag / log format
/// Purpose: Read the current local date/time including milliseconds; Windows applies the time
/// zone while other platforms use UTC.
/// Inputs: none.
/// Outputs / side effects: `(year, month, day, hour, minute, second, millisecond)`; no state change.
/// Complexity: O(1) time and space.
fn local_parts() -> (i32, u32, u32, u32, u32, u32, u32) {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    #[cfg(target_os = "windows")]
    {
        use winapi::shared::minwindef::FILETIME;
        use winapi::um::fileapi::FileTimeToLocalFileTime;
        use winapi::um::minwinbase::SYSTEMTIME;
        use winapi::um::timezoneapi::FileTimeToSystemTime;

        // 100 ns 为单位，起点为 1601-01-01；Unix 纪元偏移 11644473600 秒。
        // 100 ns units since 1601-01-01; the Unix epoch offset is 11_644_473_600 seconds.
        let ticks = (since_epoch.as_secs() + 11_644_473_600) * 10_000_000
            + since_epoch.subsec_nanos() as u64 / 100;
        let file_time = FILETIME {
            dwLowDateTime: (ticks & 0xffff_ffff) as u32,
            dwHighDateTime: (ticks >> 32) as u32,
        };
        let mut local = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut system_time = SYSTEMTIME {
            wYear: 0,
            wMonth: 0,
            wDayOfWeek: 0,
            wDay: 0,
            wHour: 0,
            wMinute: 0,
            wSecond: 0,
            wMilliseconds: 0,
        };
        // SAFETY: 两个转换函数只读写调用方提供的栈上结构体，句柄与指针均有效。
        // SAFETY: both conversion functions only read/write caller-provided stack structs.
        unsafe {
            if FileTimeToLocalFileTime(&file_time, &mut local) != 0
                && FileTimeToSystemTime(&local, &mut system_time) != 0
            {
                return (
                    system_time.wYear as i32,
                    system_time.wMonth as u32,
                    system_time.wDay as u32,
                    system_time.wHour as u32,
                    system_time.wMinute as u32,
                    system_time.wSecond as u32,
                    system_time.wMilliseconds as u32,
                );
            }
        }
    }
    let (year, month, day) = civil_from_days((since_epoch.as_secs() / 86_400) as i64);
    let seconds_of_day = since_epoch.as_secs() % 86_400;
    (
        year,
        month,
        day,
        (seconds_of_day / 3_600) as u32,
        ((seconds_of_day % 3_600) / 60) as u32,
        (seconds_of_day % 60) as u32,
        since_epoch.subsec_millis(),
    )
}

/// 所属模块：diag / 日志格式
/// 功能：把 Unix 天数换算为公历日期（Howard Hinnant 的 civil_from_days 算法）。
/// 输入：`days` 自 1970-01-01 起的天数。
/// 输出/副作用：返回 `(年, 月, 日)`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: diag / log format
/// Purpose: Convert days since the Unix epoch into a civil date (Howard Hinnant's algorithm).
/// Inputs: `days` — days since 1970-01-01.
/// Outputs / side effects: `(year, month, day)`; no state change.
/// Complexity: O(1) time and space.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year as i32, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：varint 对 0、单字节边界与 64 位最大值都能往返。
    /// Covers: varints round-trip for 0, the single-byte boundary and the 64-bit max.
    #[test]
    fn varints_round_trip() {
        for value in [0_u64, 1, 127, 128, 16_383, 16_384, u64::MAX] {
            let mut buffer = Vec::new();
            put_varint(&mut buffer, value);
            let mut input: &[u8] = &buffer;
            assert_eq!(get_varint(&mut input), Some(value), "value={value}");
            assert!(input.is_empty());
        }
    }

    /// 覆盖：字符串按长度前缀往返，截断数据返回 `None`。
    /// Covers: strings round-trip with a length prefix and truncated input returns `None`.
    #[test]
    fn strings_round_trip_and_reject_truncation() {
        let mut buffer = Vec::new();
        put_str(&mut buffer, "工程/轨道");
        put_str(&mut buffer, "");
        let mut input: &[u8] = &buffer;
        assert_eq!(get_str(&mut input), Some("工程/轨道"));
        assert_eq!(get_str(&mut input), Some(""));
        assert!(input.is_empty());

        let mut broken: &[u8] = &[5, b'a', b'b'];
        assert_eq!(get_str(&mut broken), None);
    }

    /// 覆盖：已知日期的公历换算。
    /// Covers: civil conversion of known dates.
    #[test]
    fn civil_dates_match_known_values() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_349), (2022, 12, 23));
    }
}
