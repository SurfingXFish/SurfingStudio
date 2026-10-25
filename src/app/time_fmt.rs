//! 所属模块：app / 本地时间格式化
//! 文件用途：
//! - 把 `SystemTime` 转换为本地日期时间，供主页项目列表、历史版本与备份文件命名使用。
//! - Windows 使用 Win32 的 FILETIME 换算（含时区），其他平台按 UTC 显示。
//!
//! 函数清单：
//! - `local_parts`：返回本地（或 UTC）年/月/日/时/分/秒。
//! - `format_datetime`：`YYYY-MM-DD HH:mm`。
//! - `format_compact`：`YYYYMMDD-HHmmss`，用于备份文件名。
//! - `format_relative`：`刚刚` / `N 分钟前` / `N 小时前` / 完整日期。
//!
//! Module: app / local time formatting
//! Purpose:
//! - Convert `SystemTime` into local date/time for the home project list, history versions and
//!   backup file names.
//! - Windows uses Win32 FILETIME conversion (with time zone); other platforms show UTC.
//!
//! Functions:
//! - `local_parts`: year/month/day/hour/minute/second in local time (or UTC).
//! - `format_datetime`: `YYYY-MM-DD HH:mm`.
//! - `format_compact`: `YYYYMMDD-HHmmss`, used for backup names.
//! - `format_relative`: `刚刚` / `N 分钟前` / `N 小时前` / the absolute date.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 所属模块：app / 本地时间格式化
/// 功能：把时间转换为本地日期时间分量；Windows 走时区换算，其他平台使用 UTC。
/// 输入：`time` 系统时间。
/// 输出/副作用：返回 `(年, 月, 日, 时, 分, 秒)`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / local time formatting
/// Purpose: Convert a timestamp into local date/time parts; Windows applies the time zone while
/// other platforms use UTC.
/// Inputs: `time` — system time.
/// Outputs / side effects: `(year, month, day, hour, minute, second)`; no state change.
/// Complexity: O(1) time and space.
pub(super) fn local_parts(time: SystemTime) -> (i32, u32, u32, u32, u32, u32) {
    let since_epoch = time.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    #[cfg(target_os = "windows")]
    {
        use winapi::shared::minwindef::FILETIME;
        use winapi::um::fileapi::FileTimeToLocalFileTime;
        use winapi::um::minwinbase::SYSTEMTIME;
        use winapi::um::timezoneapi::FileTimeToSystemTime;

        // 100 ns 为单位，起点为 1601-01-01；Unix 纪元偏移 11644473600 秒。
        // 100 ns units since 1601-01-01; the Unix epoch offset is 11_644_473_600 seconds.
        let ticks = (since_epoch.as_secs() as u64 + 11_644_473_600) * 10_000_000
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
    )
}

/// 所属模块：app / 本地时间格式化
/// 功能：把 Unix 天数换算为公历日期（Howard Hinnant 的 civil_from_days 算法）。
/// 输入：`days` 自 1970-01-01 起的天数。
/// 输出/副作用：返回 `(年, 月, 日)`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / local time formatting
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

/// 所属模块：app / 本地时间格式化
/// 功能：格式化为 `YYYY-MM-DD HH:mm`。
/// 输入：`time` 系统时间。
/// 输出/副作用：返回展示文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / local time formatting
/// Purpose: Format as `YYYY-MM-DD HH:mm`.
/// Inputs: `time` — system time.
/// Outputs / side effects: display text; no state change.
/// Complexity: O(1) time and space.
pub(super) fn format_datetime(time: SystemTime) -> String {
    let (year, month, day, hour, minute, _) = local_parts(time);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

/// 所属模块：app / 本地时间格式化
/// 功能：格式化为 `YYYYMMDD-HHmmss`，用于备份文件名。
/// 输入：`time` 系统时间。
/// 输出/副作用：返回紧凑时间文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / local time formatting
/// Purpose: Format as `YYYYMMDD-HHmmss` for backup file names.
/// Inputs: `time` — system time.
/// Outputs / side effects: compact time text; no state change.
/// Complexity: O(1) time and space.
pub(super) fn format_compact(time: SystemTime) -> String {
    let (year, month, day, hour, minute, second) = local_parts(time);
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// 所属模块：app / 本地时间格式化
/// 功能：返回相对时间描述：1 分钟内“刚刚”，1 小时内“N 分钟前”，24 小时内“N 小时前”，
/// 其余显示完整日期时间。
/// 输入：`time` 系统时间。
/// 输出/副作用：返回展示文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / local time formatting
/// Purpose: Return a relative description: “just now” under a minute, “N minutes ago” under an
/// hour, “N hours ago” under a day, otherwise the absolute date.
/// Inputs: `time` — system time.
/// Outputs / side effects: display text; no state change.
/// Complexity: O(1) time and space.
pub(super) fn format_relative(time: SystemTime) -> String {
    let elapsed = SystemTime::now()
        .duration_since(time)
        .unwrap_or(Duration::ZERO);
    if elapsed.as_secs() < 60 {
        "刚刚".into()
    } else if elapsed.as_secs() < 3_600 {
        format!("{} 分钟前", elapsed.as_secs() / 60)
    } else if elapsed.as_secs() < 86_400 {
        format!("{} 小时前", elapsed.as_secs() / 3_600)
    } else {
        format_datetime(time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：Unix 纪元与已知日期能正确换算为公历日期。
    /// Covers: the Unix epoch and known dates convert to the expected civil dates.
    #[test]
    fn civil_dates_match_known_values() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_349), (2022, 12, 23));
    }

    /// 覆盖：紧凑时间格式固定为 `YYYYMMDD-HHmmss`。
    /// Covers: the compact format is always `YYYYMMDD-HHmmss`.
    #[test]
    fn compact_format_has_a_stable_shape() {
        let text = format_compact(UNIX_EPOCH + Duration::from_secs(1_700_000_000));
        assert_eq!(text.len(), 15, "text={text}");
        assert_eq!(&text[8..9], "-");
    }
}
