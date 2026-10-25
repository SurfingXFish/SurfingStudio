//! 所属模块：diag / 诊断日志
//! 文件用途：
//! - 进程级诊断日志：以自定义 `.sflog` **v2 二进制格式**记录启动、工程、插件、引擎、资源占用与退出过程。
//! - 每条记录固定头部 + 变长负载，包含序号、线程 id、UI 帧号、相对时间、级别、耗时与键值字段。
//! - 周期性采样进程资源（内存/句柄/线程/CPU），并聚合 UI 帧耗时；记录 panic 与未处理异常。
//! - 提供确定性的进程退出入口：写尾帧后直接结束进程，不依赖 eframe 的析构链。
//! - 日志目录：`%LOCALAPPDATA%\SurfingStudio\logs`，不可用时回退系统临时目录。
//! - 格式说明见 `doc/日志规范.md`；实时音频回调内禁止调用本模块。
//!
//! 函数清单：
//! - `init`：创建日志文件、写入二进制头、安装 panic 钩子并启动看门狗线程。
//! - `info` / `warn` / `error`：写入一条记录。
//! - `Span::start` / `Span::start_logged` / `Span::end` / `Span::end_slow`：耗时区间记录。
//! - `note_frame` / `note_frame_duration` / `set_activity`：UI 帧心跳、慢帧、帧耗时聚合与当前阶段。
//! - `request_exit`：写尾帧并立即终止进程（带退出看门狗）。
//! - `crash`：崩溃路径的 FATAL 记录与尾帧（使用 `try_lock`，可在异常过滤器内调用）。
//! - `thread_id`：返回当前 Win32 线程 id，用于标注跨线程记录。
//! - `decode_log_file` / `export_text`：把二进制日志解码为文本（重新导出供人工查看）。
//! - `watchdog_loop` / `install_panic_hook` / `open_sink` / `log_dir` / `terminate_now` / `now_us` /
//!   `activity` / `write_line` / `write_record` / `write_frame` / `write_footer` / `write_resource` /
//!   `encode_event_payload` / `sample_resources` / `finish` / `start_unix_us`：内部实现。
//!
//! Module: diag / diagnostic log
//! Purpose:
//! - Process-wide diagnostic log in a custom `.sflog` **v2 binary format**: startup, projects,
//!   plugins, engine, resource usage and shutdown.
//! - Every record has a fixed header plus a variable payload: sequence, thread id, UI frame,
//!   relative time, level, duration and key/value fields.
//! - Periodically samples process resources (memory/handles/threads/CPU) and aggregates UI frame
//!   timing; records panics and unhandled exceptions.
//! - A deterministic exit entry: write the footer, then terminate without eframe's teardown chain.
//! - Log directory: `%LOCALAPPDATA%\SurfingStudio\logs`, falling back to the temp directory.
//! - The format is documented in `doc/日志规范.md`; never call this module from the audio callback.
//!
//! Functions:
//! - `init`: create the log file, write the binary header, install the panic hook, start the watchdog.
//! - `info` / `warn` / `error`: write one record.
//! - `Span::start` / `Span::start_logged` / `Span::end` / `Span::end_slow`: duration spans.
//! - `note_frame` / `note_frame_duration` / `set_activity`: frame heartbeat, slow frames, frame stats.
//! - `request_exit`: write the footer and terminate immediately (with a watchdog).
//! - `crash`: FATAL record plus footer for crash paths (`try_lock`, safe inside an exception filter).
//! - `thread_id`: current Win32 thread id for cross-thread records.
//! - `decode_log_file` / `export_text`: decode a binary log into text for human inspection.
//! - `watchdog_loop` / `install_panic_hook` / `open_sink` / `log_dir` / `terminate_now` / `now_us` /
//!   `activity` / `write_line` / `write_record` / `write_frame` / `write_footer` / `write_resource` /
//!   `encode_event_payload` / `sample_resources` / `finish` / `start_unix_us`: internals.

mod decode;
mod format;

pub use decode::{decode_log_file, export_text};

use format::{local_now, prune_logs, put_str, put_varint};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 文件头魔数：小端字节为 `SFLG`。
/// File-header magic: little-endian bytes spell `SFLG`.
const FORMAT_MAGIC: u32 = 0x474C_4653;
/// `.sflog` 格式版本；布局变更时递增。
/// `.sflog` format version; bump when the layout changes.
const FORMAT_VERSION: u16 = 2;
/// 文件头固定前缀长度（魔数 + 版本 + 标志 + 头长度 + PID + 启动时间）。
/// Fixed file-header prefix length (magic + version + flags + header length + PID + start time).
const HEADER_PREFIX_LEN: usize = 24;
/// 记录固定头部长度。
/// Fixed record-header length.
const RECORD_HEADER_LEN: usize = 32;
/// 记录类型：事件（scope/event/耗时/字段）。
/// Record kind: event (scope/event/duration/fields).
const RECORD_EVENT: u8 = 1;
/// 记录类型：尾帧（退出原因）。
/// Record kind: footer (exit reason).
const RECORD_FOOTER: u8 = 2;
/// 记录类型：进程资源采样。
/// Record kind: process resource sample.
const RECORD_RESOURCE: u8 = 3;
/// 字段值类型：UTF-8 字符串。
/// Field value type: UTF-8 string.
const FIELD_STR: u8 = 3;
/// 日志文件保留数量：超出后删除最旧的文件。
/// Number of log files to keep; older files are deleted.
const LOG_KEEP_FILES: usize = 20;
/// 看门狗轮询间隔。
/// Watchdog polling interval.
const WATCHDOG_TICK_MS: u64 = 250;
/// UI 帧间隔超过该值视为慢帧（毫秒）。
/// A UI frame longer than this is a slow frame (milliseconds).
const UI_SLOW_FRAME_MS: u64 = 120;
/// 连续慢帧日志的最小间隔：避免播放中刷屏（毫秒）。
/// Minimum interval between slow-frame records, so playback cannot flood the log (milliseconds).
const UI_SLOW_FRAME_REPEAT_MS: u64 = 1_000;
/// 每累计这么多帧写入一条帧耗时聚合记录。
/// Write one aggregated frame-timing record every N frames.
const UI_FRAME_STATS_FRAMES: u64 = 600;
/// UI 线程超过该时长没有执行新帧即判定为卡死（毫秒）。
/// The UI thread is considered stalled after this long without a new frame (milliseconds).
const UI_STALL_MS: u64 = 3_000;
/// 同一次卡死重复上报的最小间隔（毫秒）。
/// Minimum interval for repeated stall reports of one episode (milliseconds).
const UI_STALL_REPEAT_MS: u64 = 15_000;
/// 进程资源采样间隔（毫秒）。
/// Process resource sampling interval (milliseconds).
const RESOURCE_SAMPLE_INTERVAL_MS: u64 = 5_000;
/// 退出看门狗：调用终止后仍存活超过该时长则再次强制结束（毫秒）。
/// Exit watchdog: terminate again when the process outlives this after an exit request (ms).
const EXIT_WATCHDOG_MS: u64 = 3_000;

/// 日志级别。
/// Log level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// 高频细节，默认不写入。
    /// High-frequency detail, not written by default.
    Trace,
    /// 调试信息，默认不写入。
    /// Debug information, not written by default.
    Debug,
    /// 正常流程节点。
    /// Normal flow milestones.
    Info,
    /// 可疑但可继续的状态（卡顿、回退）。
    /// Suspicious but recoverable states (stalls, fallbacks).
    Warn,
    /// 操作失败（保底回退后仍不可用）。
    /// An operation failed even after fallbacks.
    Error,
    /// 致命：进程即将结束。
    /// Fatal: the process is about to end.
    Fatal,
}

impl Level {
    /// 所属模块：diag / 日志级别
    /// 功能：返回记录头中使用的级别字节。
    /// 输入：`self`。
    /// 输出/副作用：`0..=5`；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: diag / log level
    /// Purpose: Return the level byte stored in the record header.
    /// Inputs: `self`.
    /// Outputs / side effects: `0..=5`; no state change.
    /// Complexity: O(1) time and space.
    fn code(self) -> u8 {
        match self {
            Level::Trace => 0,
            Level::Debug => 1,
            Level::Info => 2,
            Level::Warn => 3,
            Level::Error => 4,
            Level::Fatal => 5,
        }
    }

    /// 所属模块：diag / 日志级别
    /// 功能：返回级别文本，供解码器展示。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: diag / log level
    /// Purpose: Return the level label used by the decoder.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub(super) fn label(self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
            Level::Fatal => "FATAL",
        }
    }

    /// 所属模块：diag / 日志级别
    /// 功能：把记录头中的级别字节还原为枚举（解码器使用）。
    /// 输入：`code` 级别字节。
    /// 输出/副作用：未知值按 `Info` 处理；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: diag / log level
    /// Purpose: Restore a level from its header byte (decoder use).
    /// Inputs: `code` — level byte.
    /// Outputs / side effects: unknown values map to `Info`; no state change.
    /// Complexity: O(1) time and space.
    pub(super) fn from_code(code: u8) -> Self {
        match code {
            0 => Level::Trace,
            1 => Level::Debug,
            2 => Level::Info,
            3 => Level::Warn,
            4 => Level::Error,
            5 => Level::Fatal,
            _ => Level::Info,
        }
    }
}

/// 一个已打开的日志文件。
/// An open log file.
struct LogSink {
    file: File,
    path: PathBuf,
    seq: u32,
    finished: bool,
}

/// 全局日志句柄；`None` 表示初始化失败或尚未初始化。
/// Global log handle; `None` before initialization or when it failed.
static LOG: Mutex<Option<LogSink>> = Mutex::new(None);
/// 进程起点，用于记录相对时间戳。
/// Process start, used for relative timestamps.
static START: OnceLock<Instant> = OnceLock::new();
/// 是否已完成初始化。
/// Whether initialization already ran.
static INITIALIZED: AtomicBool = AtomicBool::new(false);
/// UI 线程最近一帧的开始时间（微秒，相对进程起点）；`0` 表示尚未产生帧。
/// Start time of the latest UI frame (microseconds since start); `0` means no frame yet.
static LAST_FRAME_US: AtomicU64 = AtomicU64::new(0);
/// UI 帧计数，用于判断看门狗报告期间是否仍在刷新。
/// UI frame counter, used to confirm the loop is still alive.
static FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
/// 本次卡死的起点（微秒）；`0` 表示未在卡死中。
/// Start of the current stall (microseconds); `0` means not stalled.
static STALL_START_US: AtomicU64 = AtomicU64::new(0);
/// 下一次允许上报卡死的时间（微秒）。
/// Next time a stall may be reported (microseconds).
static NEXT_STALL_REPORT_US: AtomicU64 = AtomicU64::new(0);
/// 上一次慢帧记录时间（微秒）。
/// Time of the last slow-frame record (microseconds).
static LAST_SLOW_FRAME_US: AtomicU64 = AtomicU64::new(0);
/// 帧耗时聚合：累计微秒。
/// Frame-timing aggregate: total microseconds.
static FRAME_SUM_US: AtomicU64 = AtomicU64::new(0);
/// 帧耗时聚合：最大单帧微秒。
/// Frame-timing aggregate: max frame microseconds.
static FRAME_MAX_US: AtomicU64 = AtomicU64::new(0);
/// 帧耗时聚合：已累计帧数。
/// Frame-timing aggregate: frames counted.
static FRAME_STATS_COUNT: AtomicU64 = AtomicU64::new(0);
/// 下一次资源采样的截止时间（微秒）。
/// Deadline of the next resource sample (microseconds).
static NEXT_RESOURCE_SAMPLE_US: AtomicU64 = AtomicU64::new(0);
/// 退出看门狗的截止时间（微秒）；`0` 表示未请求退出。
/// Exit watchdog deadline (microseconds); `0` means no exit requested.
static EXIT_DEADLINE_US: AtomicU64 = AtomicU64::new(0);
/// 是否已经进入退出流程。
/// Whether the exit flow already started.
static EXITING: AtomicBool = AtomicBool::new(false);
/// UI 当前阶段，供卡死/慢帧记录定位。
/// Current UI phase, used by stall/slow-frame records.
static ACTIVITY: Mutex<&'static str> = Mutex::new("boot");

/// 所属模块：diag / 初始化
/// 功能：创建日志文件并写入二进制头，安装 panic 钩子与看门狗线程。
/// 输入：`app_version`：写入头部的软件版本号。
/// 输出/副作用：在日志目录创建 `.sflog` 文件；清理旧日志；写入首条记录与首个资源采样；
/// 启动常驻看门狗线程。
/// 复杂度：时间 O(日志文件数 + 目录创建 + 资源采样)，空间 O(1)。
///
/// Module: diag / initialization
/// Purpose: Create the log file, write the binary header, install the panic hook and start the
/// watchdog.
/// Inputs: `app_version`: version written into the header.
/// Outputs / side effects: creates a `.sflog` file in the log directory; prunes old logs; writes the
/// first record and resource sample; starts a persistent watchdog thread.
/// Complexity: O(log files + directory creation + resource sampling) time and O(1) space.
pub fn init(app_version: &str) {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = START.set(Instant::now());
    let Some(mut sink) = open_sink() else {
        install_panic_hook();
        return;
    };
    let now = local_now();
    let header_text = format!(
        "app_version={}\nos={}\narch={}\nbuild={}\nstarted={}\nlog_path={}\n",
        app_version,
        std::env::consts::OS,
        std::env::consts::ARCH,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        now.datetime,
        sink.path.to_string_lossy()
    );
    let mut header = Vec::with_capacity(HEADER_PREFIX_LEN + header_text.len());
    header.extend_from_slice(&FORMAT_MAGIC.to_le_bytes());
    header.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    header.extend_from_slice(&1_u16.to_le_bytes());
    header.extend_from_slice(&(header_text.len() as u32).to_le_bytes());
    header.extend_from_slice(&std::process::id().to_le_bytes());
    header.extend_from_slice(&start_unix_us().to_le_bytes());
    header.extend_from_slice(header_text.as_bytes());
    let _ = sink.file.write_all(&header);
    let path_text = sink.path.to_string_lossy().to_string();
    let pid_text = std::process::id().to_string();
    *LOG.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(sink);
    write_line(
        Level::Info,
        "app.boot",
        "process.start",
        None,
        &[
            ("pid", &pid_text),
            ("version", app_version),
            ("os", std::env::consts::OS),
            ("arch", std::env::consts::ARCH),
            (
                "build",
                if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                },
            ),
        ],
    );
    write_line(
        Level::Info,
        "app.boot",
        "log.ready",
        None,
        &[("path", &path_text)],
    );
    NEXT_RESOURCE_SAMPLE_US.store(
        now_us() + RESOURCE_SAMPLE_INTERVAL_MS * 1_000,
        Ordering::Relaxed,
    );
    write_resource_sample();
    install_panic_hook();
    let _ = std::thread::Builder::new()
        .name("diag-watchdog".into())
        .spawn(watchdog_loop);
}

/// 所属模块：diag / 记录
/// 功能：写入一条 INFO 记录。
/// 输入：
/// - `scope`：领域标识，如 `app.project`。
/// - `event`：事件名，如 `project.load`。
/// - `fields`：键值对，可为空。
/// 输出/副作用：追加一条二进制记录；未初始化时忽略。
/// 复杂度：时间 O(字段总长度)，空间 O(载荷长度)。
///
/// Module: diag / records
/// Purpose: Write one INFO record.
/// Inputs:
/// - `scope`: domain id such as `app.project`.
/// - `event`: event name such as `project.load`.
/// - `fields`: key/value pairs, may be empty.
/// Outputs / side effects: appends one binary record; ignored before initialization.
/// Complexity: O(total field length) time and O(payload length) space.
pub fn info(scope: &str, event: &str, fields: &[(&str, &str)]) {
    write_line(Level::Info, scope, event, None, fields);
}

/// 所属模块：diag / 记录
/// 功能：写入一条 WARN 记录。
/// 输入：`scope` / `event` / `fields`，含义同 `info`。
/// 输出/副作用：追加一条二进制记录；未初始化时忽略。
/// 复杂度：时间 O(字段总长度)，空间 O(载荷长度)。
///
/// Module: diag / records
/// Purpose: Write one WARN record.
/// Inputs: `scope` / `event` / `fields` as in `info`.
/// Outputs / side effects: appends one binary record; ignored before initialization.
/// Complexity: O(total field length) time and O(payload length) space.
pub fn warn(scope: &str, event: &str, fields: &[(&str, &str)]) {
    write_line(Level::Warn, scope, event, None, fields);
}

/// 所属模块：diag / 记录
/// 功能：写入一条 ERROR 记录。
/// 输入：`scope` / `event` / `fields`，含义同 `info`。
/// 输出/副作用：追加一条二进制记录；未初始化时忽略。
/// 复杂度：时间 O(字段总长度)，空间 O(载荷长度)。
///
/// Module: diag / records
/// Purpose: Write one ERROR record.
/// Inputs: `scope` / `event` / `fields` as in `info`.
/// Outputs / side effects: appends one binary record; ignored before initialization.
/// Complexity: O(total field length) time and O(payload length) space.
pub fn error(scope: &str, event: &str, fields: &[(&str, &str)]) {
    write_line(Level::Error, scope, event, None, fields);
}

/// 耗时间隔。
/// A duration span.
pub struct Span {
    scope: &'static str,
    event: &'static str,
    start: Instant,
}

impl Span {
    /// 所属模块：diag / 耗时
    /// 功能：开始一个静默区间：只有结束时才写日志（适合高频、通常很快的操作）。
    /// 输入：
    /// - `scope` / `event`：区间标识。
    /// 输出/副作用：返回区间句柄；此时不写日志。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: diag / durations
    /// Purpose: Start a silent span: only the end is logged (for frequent, usually fast work).
    /// Inputs:
    /// - `scope` / `event`: span identifiers.
    /// Outputs / side effects: span handle; nothing is logged yet.
    /// Complexity: O(1) time and space.
    pub fn start(scope: &'static str, event: &'static str) -> Self {
        Self {
            scope,
            event,
            start: Instant::now(),
        }
    }

    /// 所属模块：diag / 耗时
    /// 功能：开始一个区间并立即写入 `phase=begin` 记录；卡死时最后一条 begin 就是现场。
    /// 输入：
    /// - `scope` / `event`：区间标识。
    /// - `fields`：附加字段。
    /// 输出/副作用：写入一条 INFO 记录并返回区间句柄。
    /// 复杂度：时间 O(字段总长度)，空间 O(载荷长度)。
    ///
    /// Module: diag / durations
    /// Purpose: Start a span and log `phase=begin` immediately, so a hang leaves a visible marker.
    /// Inputs:
    /// - `scope` / `event`: span identifiers.
    /// - `fields`: extra fields.
    /// Outputs / side effects: writes one INFO record and returns the span handle.
    /// Complexity: O(total field length) time and O(payload length) space.
    pub fn start_logged(scope: &'static str, event: &'static str, fields: &[(&str, &str)]) -> Self {
        let mut combined: Vec<(&str, &str)> = fields.to_vec();
        combined.push(("phase", "begin"));
        write_line(Level::Info, scope, event, None, &combined);
        Self {
            scope,
            event,
            start: Instant::now(),
        }
    }

    /// 所属模块：diag / 耗时
    /// 功能：结束区间并写入 `phase=end` 记录（含 `dur_us`）。
    /// 输入：`fields`：附加字段，通常包含 `status`。
    /// 输出/副作用：写入一条 INFO 记录；不修改其他状态。
    /// 复杂度：时间 O(字段总长度)，空间 O(载荷长度)。
    ///
    /// Module: diag / durations
    /// Purpose: End the span and log `phase=end` including `dur_us`.
    /// Inputs: `fields`: extra fields, usually `status`.
    /// Outputs / side effects: writes one INFO record; no other state change.
    /// Complexity: O(total field length) time and O(payload length) space.
    pub fn end(self, fields: &[(&str, &str)]) {
        let mut combined: Vec<(&str, &str)> = fields.to_vec();
        combined.push(("phase", "end"));
        write_line(
            Level::Info,
            self.scope,
            self.event,
            Some(self.start.elapsed().as_micros() as u64),
            &combined,
        );
    }

    /// 所属模块：diag / 耗时
    /// 功能：结束区间，仅当耗时达到阈值时记为 WARN `phase=end`（用于高频轮询）。
    /// 输入：
    /// - `threshold_ms`：触发记录的耗时下限（毫秒）。
    /// - `fields`：附加字段。
    /// 输出/副作用：超过阈值时写入一条 WARN 记录；否则完全不写日志。
    /// 复杂度：时间 O(字段总长度)，空间 O(载荷长度)。
    ///
    /// Module: diag / durations
    /// Purpose: End the span and log a WARN `phase=end` only when it exceeded the threshold
    /// (used by high-frequency polling).
    /// Inputs:
    /// - `threshold_ms`: minimum duration to log (milliseconds).
    /// - `fields`: extra fields.
    /// Outputs / side effects: writes one WARN record when exceeded, otherwise nothing.
    /// Complexity: O(total field length) time and O(payload length) space.
    pub fn end_slow(self, threshold_ms: u64, fields: &[(&str, &str)]) {
        let elapsed = self.start.elapsed();
        if (elapsed.as_millis() as u64) < threshold_ms {
            return;
        }
        let mut combined: Vec<(&str, &str)> = fields.to_vec();
        combined.push(("phase", "end"));
        write_line(
            Level::Warn,
            self.scope,
            self.event,
            Some(elapsed.as_micros() as u64),
            &combined,
        );
    }
}

/// 所属模块：diag / UI 心跳
/// 功能：标记一帧开始；若上一段卡死刚恢复，写入 `ui.resume` 记录。
/// 输入：无。
/// 输出/副作用：更新帧时间戳与计数；可能写入一条 INFO 记录。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: diag / UI heartbeat
/// Purpose: Mark the start of a frame and log `ui.resume` when a reported stall just recovered.
/// Inputs: none.
/// Outputs / side effects: updates the frame timestamp/counter; may write one INFO record.
/// Complexity: O(1) time and space.
pub fn note_frame() {
    let now = now_us();
    LAST_FRAME_US.store(now, Ordering::Relaxed);
    FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
    let stall_start = STALL_START_US.swap(0, Ordering::Relaxed);
    if stall_start != 0 {
        let stall_ms = now.saturating_sub(stall_start) / 1_000;
        let stall_text = stall_ms.to_string();
        info(
            "app.ui",
            "ui.resume",
            &[("stall_ms", &stall_text), ("phase", activity())],
        );
    }
}

/// 所属模块：diag / UI 心跳
/// 功能：累计帧耗时；超过阈值时写受节流的慢帧记录，每 N 帧写一条聚合统计。
/// 输入：`duration`：本帧从进入到绘制完成的耗时。
/// 输出/副作用：可能写入 WARN（慢帧）或 INFO（`ui.frames`）记录；不修改 UI 状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: diag / UI heartbeat
/// Purpose: Accumulate frame timing; write throttled slow-frame records and an aggregated
/// `ui.frames` record every N frames.
/// Inputs: `duration`: time from frame entry to the end of painting.
/// Outputs / side effects: may write one WARN (slow frame) or INFO (`ui.frames`) record.
/// Complexity: O(1) time and space.
pub fn note_frame_duration(duration: Duration) {
    let micros = duration.as_micros() as u64;
    FRAME_SUM_US.fetch_add(micros, Ordering::Relaxed);
    FRAME_MAX_US.fetch_max(micros, Ordering::Relaxed);
    let counted = FRAME_STATS_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if counted >= UI_FRAME_STATS_FRAMES {
        FRAME_STATS_COUNT.store(0, Ordering::Relaxed);
        let total = FRAME_SUM_US.swap(0, Ordering::Relaxed);
        let peak = FRAME_MAX_US.swap(0, Ordering::Relaxed);
        let count_text = counted.to_string();
        let average_text = (total / counted.max(1)).to_string();
        let peak_text = peak.to_string();
        info(
            "app.ui",
            "ui.frames",
            &[
                ("frames", &count_text),
                ("avg_us", &average_text),
                ("max_us", &peak_text),
                ("phase", activity()),
            ],
        );
    }
    let millis = micros / 1_000;
    if millis < UI_SLOW_FRAME_MS {
        return;
    }
    let now = now_us();
    let last = LAST_SLOW_FRAME_US.load(Ordering::Relaxed);
    if now.saturating_sub(last) < UI_SLOW_FRAME_REPEAT_MS * 1_000 {
        return;
    }
    LAST_SLOW_FRAME_US.store(now, Ordering::Relaxed);
    let millis_text = millis.to_string();
    let frames_text = FRAME_COUNT.load(Ordering::Relaxed).to_string();
    warn(
        "app.ui",
        "ui.frame.slow",
        &[
            ("dur_ms", &millis_text),
            ("phase", activity()),
            ("frames", &frames_text),
        ],
    );
}

/// 所属模块：diag / UI 心跳
/// 功能：记录 UI 当前阶段，供卡死与慢帧记录定位。
/// 输入：`phase`：阶段名（静态字符串，避免热路径分配）。
/// 输出/副作用：更新全局阶段；不阻塞。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: diag / UI heartbeat
/// Purpose: Record the current UI phase so stall and slow-frame records can locate the work.
/// Inputs: `phase`: phase name (static, so the hot path never allocates).
/// Outputs / side effects: updates the global phase; never blocks for long.
/// Complexity: O(1) time and space.
pub fn set_activity(phase: &'static str) {
    let mut guard = ACTIVITY.lock().unwrap_or_else(|poison| poison.into_inner());
    *guard = phase;
}

/// 所属模块：diag / 退出
/// 功能：写入退出记录与尾帧，然后立即终止进程。
/// 输入：`reason`：退出原因（如 `home_closed`、`on_exit`）。
/// 输出/副作用：写日志并结束进程；若终止调用异常则回退 `std::process::exit`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: diag / exit
/// Purpose: Write the shutdown record and footer, then terminate the process immediately.
/// Inputs: `reason`: exit reason such as `home_closed` or `on_exit`.
/// Outputs / side effects: writes the log and ends the process; falls back to `std::process::exit`
/// when termination fails.
/// Complexity: O(1) time and space.
pub fn request_exit(reason: &str) -> ! {
    // 先设定看门狗截止时间：即使终止调用本身卡住，看门狗也会再次强制结束。
    // Arm the watchdog first so a blocked termination is retried by the watchdog thread.
    EXIT_DEADLINE_US.store(now_us() + EXIT_WATCHDOG_MS * 1_000, Ordering::Relaxed);
    if !EXITING.swap(true, Ordering::SeqCst) {
        let uptime_text = (now_us() / 1_000).to_string();
        write_line(
            Level::Info,
            "app.exit",
            "process.exit",
            None,
            &[("reason", reason), ("uptime_ms", &uptime_text)],
        );
        finish(reason);
    }
    terminate_now()
}

/// 所属模块：diag / 崩溃
/// 功能：在崩溃路径写入 FATAL 记录与尾帧；可在 Windows 未处理异常过滤器内调用。
/// 输入：
/// - `reason`：尾帧原因，如 `unhandled_exception`。
/// - `scope` / `event` / `fields`：FATAL 记录内容。
/// 输出/副作用：`try_lock` 成功时写日志并 flush；失败时静默放弃，绝不阻塞崩溃路径。
/// 复杂度：时间 O(字段总长度)，空间 O(载荷长度)。
///
/// Module: diag / crash
/// Purpose: Write a FATAL record and footer on crash paths; callable from the Windows unhandled
/// exception filter.
/// Inputs:
/// - `reason`: footer reason such as `unhandled_exception`.
/// - `scope` / `event` / `fields`: the FATAL record.
/// Outputs / side effects: writes and flushes when `try_lock` succeeds; silently gives up otherwise,
/// so the crash path never blocks.
/// Complexity: O(total field length) time and O(payload length) space.
pub fn crash(reason: &str, scope: &str, event: &str, fields: &[(&str, &str)]) {
    let Ok(mut guard) = LOG.try_lock() else {
        return;
    };
    let Some(sink) = guard.as_mut() else {
        return;
    };
    write_record(sink, Level::Fatal, scope, event, None, fields);
    write_footer(sink, reason);
    let _ = sink.file.flush();
}

/// 所属模块：diag / 线程
/// 功能：返回当前线程的 Win32 线程 id，供跨线程日志标注来源。
/// 输入：无。
/// 输出/副作用：线程 id；非 Windows 返回 0。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: diag / threads
/// Purpose: Return the Win32 thread id of the current thread so cross-thread records can be told
/// apart.
/// Inputs: none.
/// Outputs / side effects: the thread id; `0` off Windows.
/// Complexity: O(1) time and space.
pub fn thread_id() -> u32 {
    #[cfg(target_os = "windows")]
    {
        // SAFETY: `GetCurrentThreadId` 只读取当前线程的 id，不涉及指针或所有权。
        // SAFETY: `GetCurrentThreadId` only reads the current thread id.
        return unsafe { winapi::um::processthreadsapi::GetCurrentThreadId() };
    }
    #[cfg(not(target_os = "windows"))]
    {
        0
    }
}

/// 所属模块：diag / 内部
/// 功能：看门狗线程：检测 UI 卡死与退出超时，并周期性采样进程资源。
/// 输入：无。
/// 输出/副作用：可能写入 WARN/FATAL 与资源记录；退出超时后终止进程；线程常驻。
/// 复杂度：每次 tick 时间 O(1)（每 5 秒一次资源采样 O(线程数)）。
///
/// Module: diag / internals
/// Purpose: Watchdog thread: detect UI stalls and exit timeouts, and sample process resources
/// periodically.
/// Inputs: none.
/// Outputs / side effects: may write WARN/FATAL and resource records; terminates the process after an
/// exit timeout; the thread lives for the process lifetime.
/// Complexity: O(1) per tick (one O(threads) resource sample every 5 s).
fn watchdog_loop() {
    loop {
        std::thread::sleep(Duration::from_millis(WATCHDOG_TICK_MS));
        let now = now_us();
        let deadline = EXIT_DEADLINE_US.load(Ordering::Relaxed);
        if deadline != 0 && now >= deadline {
            EXIT_DEADLINE_US.store(0, Ordering::Relaxed);
            crash(
                "exit_timeout",
                "app.exit",
                "process.exit.timeout",
                &[("reason", "watchdog"), ("phase", activity())],
            );
            terminate_now();
        }
        if now >= NEXT_RESOURCE_SAMPLE_US.load(Ordering::Relaxed) {
            NEXT_RESOURCE_SAMPLE_US
                .store(now + RESOURCE_SAMPLE_INTERVAL_MS * 1_000, Ordering::Relaxed);
            write_resource_sample();
        }
        let last = LAST_FRAME_US.load(Ordering::Relaxed);
        if last == 0 {
            continue;
        }
        let stalled_ms = now.saturating_sub(last) / 1_000;
        if stalled_ms < UI_STALL_MS {
            continue;
        }
        if now < NEXT_STALL_REPORT_US.load(Ordering::Relaxed) {
            continue;
        }
        NEXT_STALL_REPORT_US.store(now + UI_STALL_REPEAT_MS * 1_000, Ordering::Relaxed);
        STALL_START_US.store(last, Ordering::Relaxed);
        let stalled_text = stalled_ms.to_string();
        let frames_text = FRAME_COUNT.load(Ordering::Relaxed).to_string();
        warn(
            "app.ui",
            "ui.stall",
            &[
                ("stall_ms", &stalled_text),
                ("phase", activity()),
                ("frames", &frames_text),
            ],
        );
        write_resource_sample();
    }
}

/// 所属模块：diag / 内部
/// 功能：安装 panic 钩子：先写 FATAL 记录，再交给原钩子输出。
/// 输入：无。
/// 输出/副作用：替换全局 panic 钩子；panic 时写日志。
/// 复杂度：时间 O(1)（安装），空间 O(1)。
///
/// Module: diag / internals
/// Purpose: Install a panic hook that writes a FATAL record before the previous hook runs.
/// Inputs: none.
/// Outputs / side effects: replaces the global panic hook; writes on panic.
/// Complexity: O(1) time and space.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = if let Some(text) = info.payload().downcast_ref::<&str>() {
            (*text).to_string()
        } else if let Some(text) = info.payload().downcast_ref::<String>() {
            text.clone()
        } else {
            "未知 panic / unknown panic".to_string()
        };
        let location = info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "未知位置 / unknown location".to_string());
        if let Ok(mut guard) = LOG.try_lock() {
            if let Some(sink) = guard.as_mut() {
                write_record(
                    sink,
                    Level::Fatal,
                    "app.panic",
                    "panic",
                    None,
                    &[("message", &message), ("location", &location)],
                );
                let _ = sink.file.flush();
            }
        }
        previous(info);
    }));
}

/// 所属模块：diag / 内部
/// 功能：按顺序写入一条事件记录；未初始化或已结束时不写。
/// 输入：`level` / `scope` / `event` / `duration_us` / `fields`。
/// 输出/副作用：向日志文件追加一条记录。
/// 复杂度：时间 O(字段总长度)，空间 O(载荷长度)。
///
/// Module: diag / internals
/// Purpose: Append one event record; nothing is written before initialization or after the footer.
/// Inputs: `level` / `scope` / `event` / `duration_us` / `fields`.
/// Outputs / side effects: appends one record to the log file.
/// Complexity: O(total field length) time and O(payload length) space.
fn write_line(
    level: Level,
    scope: &str,
    event: &str,
    duration_us: Option<u64>,
    fields: &[(&str, &str)],
) {
    let mut guard = LOG.lock().unwrap_or_else(|poison| poison.into_inner());
    let Some(sink) = guard.as_mut() else {
        return;
    };
    write_record(sink, level, scope, event, duration_us, fields);
}

/// 所属模块：diag / 内部
/// 功能：把一条事件记录写入已打开的日志文件（调用方持有锁）。
/// 输入：
/// - `sink`：日志文件。
/// - `level` / `scope` / `event` / `duration_us` / `fields`：记录内容。
/// 输出/副作用：递增序号并追加一条记录；已写尾帧后忽略。
/// 复杂度：时间 O(字段总长度)，空间 O(载荷长度)。
///
/// Module: diag / internals
/// Purpose: Write one event record into the open file (the caller holds the lock).
/// Inputs:
/// - `sink`: log file.
/// - `level` / `scope` / `event` / `duration_us` / `fields`: record content.
/// Outputs / side effects: increments the sequence and appends one record; ignored after the footer.
/// Complexity: O(total field length) time and O(payload length) space.
fn write_record(
    sink: &mut LogSink,
    level: Level,
    scope: &str,
    event: &str,
    duration_us: Option<u64>,
    fields: &[(&str, &str)],
) {
    if sink.finished {
        return;
    }
    let payload = encode_event_payload(scope, event, duration_us, fields);
    sink.seq = sink.seq.wrapping_add(1);
    write_frame(sink, RECORD_EVENT, level, &payload);
}

/// 所属模块：diag / 内部
/// 功能：编码事件负载：scope、event、耗时（`u64::MAX` 表示无）与键值字段。
/// 输入：`scope` / `event` / `duration_us` / `fields`。
/// 输出/副作用：返回负载字节；不写盘。
/// 复杂度：时间 O(字段总长度)，空间 O(载荷长度)。
///
/// Module: diag / internals
/// Purpose: Encode an event payload: scope, event, duration (`u64::MAX` means none) and fields.
/// Inputs: `scope` / `event` / `duration_us` / `fields`.
/// Outputs / side effects: payload bytes; nothing is written to disk.
/// Complexity: O(total field length) time and O(payload length) space.
fn encode_event_payload(
    scope: &str,
    event: &str,
    duration_us: Option<u64>,
    fields: &[(&str, &str)],
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(64);
    put_str(&mut payload, scope);
    put_str(&mut payload, event);
    put_varint(&mut payload, duration_us.unwrap_or(u64::MAX));
    put_varint(&mut payload, fields.len() as u64);
    for (key, value) in fields {
        put_str(&mut payload, key);
        payload.push(FIELD_STR);
        put_str(&mut payload, value);
    }
    payload
}

/// 所属模块：diag / 内部
/// 功能：写出 32 字节记录头与负载，并计算负载 CRC32。
/// 输入：
/// - `sink`：日志文件（序号由调用方递增）。
/// - `kind`：记录类型。
/// - `level`：级别。
/// - `payload`：负载字节。
/// 输出/副作用：一次 `write_all` 追加整条记录；不 flush。
/// 复杂度：时间 O(载荷长度 + CRC)，空间 O(载荷长度)。
///
/// Module: diag / internals
/// Purpose: Write the 32-byte record header plus payload and compute the payload CRC32.
/// Inputs:
/// - `sink`: log file (the caller increments the sequence).
/// - `kind`: record kind.
/// - `level`: level.
/// - `payload`: payload bytes.
/// Outputs / side effects: appends the whole record with one `write_all`; does not flush.
/// Complexity: O(payload length + CRC) time and O(payload length) space.
fn write_frame(sink: &mut LogSink, kind: u8, level: Level, payload: &[u8]) {
    let mut header = [0_u8; RECORD_HEADER_LEN];
    header[0] = kind;
    header[1] = level.code();
    header[4..8].copy_from_slice(&sink.seq.to_le_bytes());
    header[8..12].copy_from_slice(&thread_id().to_le_bytes());
    header[12..20].copy_from_slice(&now_us().to_le_bytes());
    header[20..24].copy_from_slice(&(FRAME_COUNT.load(Ordering::Relaxed) as u32).to_le_bytes());
    header[24..28].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    header[28..32].copy_from_slice(&crc32fast::hash(payload).to_le_bytes());
    let mut buffer = Vec::with_capacity(RECORD_HEADER_LEN + payload.len());
    buffer.extend_from_slice(&header);
    buffer.extend_from_slice(payload);
    let _ = sink.file.write_all(&buffer);
}

/// 所属模块：diag / 内部
/// 功能：写入尾帧（退出/崩溃原因）并标记日志结束。
/// 输入：
/// - `sink`：日志文件。
/// - `reason`：原因文本。
/// 输出/副作用：追加一条 FOOTER 记录；之后不再接受新记录。
/// 复杂度：时间 O(原因长度)，空间 O(原因长度)。
///
/// Module: diag / internals
/// Purpose: Write the footer (exit/crash reason) and mark the log as finished.
/// Inputs:
/// - `sink`: log file.
/// - `reason`: reason text.
/// Outputs / side effects: appends one FOOTER record; later records are ignored.
/// Complexity: O(reason length) time and space.
fn write_footer(sink: &mut LogSink, reason: &str) {
    let mut payload = Vec::with_capacity(reason.len() + 2);
    put_str(&mut payload, reason);
    sink.seq = sink.seq.wrapping_add(1);
    write_frame(sink, RECORD_FOOTER, Level::Info, &payload);
    sink.finished = true;
}

/// 所属模块：diag / 内部
/// 功能：采样当前进程资源并写入一条 RESOURCE 记录。
/// 输入：无。
/// 输出/副作用：追加记录（不可用时忽略）；不修改其他状态。
/// 复杂度：时间 O(进程线程数)，空间 O(1)。
///
/// Module: diag / internals
/// Purpose: Sample current process resources and append one RESOURCE record.
/// Inputs: none.
/// Outputs / side effects: appends a record (ignored when sampling is unavailable); no other state.
/// Complexity: O(process threads) time and O(1) space.
fn write_resource_sample() {
    let Some(sample) = sample_resources() else {
        return;
    };
    let mut guard = LOG.lock().unwrap_or_else(|poison| poison.into_inner());
    let Some(sink) = guard.as_mut() else {
        return;
    };
    if sink.finished {
        return;
    }
    let mut payload = Vec::with_capacity(40);
    payload.extend_from_slice(&sample.working_set.to_le_bytes());
    payload.extend_from_slice(&sample.peak_working_set.to_le_bytes());
    payload.extend_from_slice(&sample.private_bytes.to_le_bytes());
    payload.extend_from_slice(&sample.cpu_100ns.to_le_bytes());
    payload.extend_from_slice(&sample.handles.to_le_bytes());
    payload.extend_from_slice(&sample.threads.to_le_bytes());
    sink.seq = sink.seq.wrapping_add(1);
    write_frame(sink, RECORD_RESOURCE, Level::Info, &payload);
}

/// 进程资源采样结果。
/// One process resource sample.
struct ResourceSample {
    working_set: u64,
    peak_working_set: u64,
    private_bytes: u64,
    cpu_100ns: u64,
    handles: u32,
    threads: u32,
}

/// 所属模块：diag / 内部
/// 功能：读取当前进程的内存、句柄、线程与 CPU 累计时间。
/// 输入：无。
/// 输出/副作用：返回采样结果；非 Windows 或调用失败返回 `None`。
/// 复杂度：时间 O(进程线程数)，空间 O(1)。
///
/// Module: diag / internals
/// Purpose: Read the current process memory, handle count, thread count and accumulated CPU time.
/// Inputs: none.
/// Outputs / side effects: the sample; `None` off Windows or when a call fails.
/// Complexity: O(process threads) time and O(1) space.
#[cfg(target_os = "windows")]
fn sample_resources() -> Option<ResourceSample> {
    use winapi::shared::minwindef::FILETIME;
    use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
    use winapi::um::processthreadsapi::{
        GetCurrentProcess, GetProcessHandleCount, GetProcessTimes,
    };
    use winapi::um::psapi::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use winapi::um::tlhelp32::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };

    // SAFETY: 所有调用只读取当前进程/系统快照；结构体均已初始化并设置大小字段。
    // SAFETY: every call only reads the current process or a system snapshot; all structs are
    // zero-initialised and their size fields are set.
    unsafe {
        let process = GetCurrentProcess();
        let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(process, &mut counters, counters.cb) == 0 {
            return None;
        }
        let mut handles = 0_u32;
        let _ = GetProcessHandleCount(process, &mut handles);
        let mut creation: FILETIME = std::mem::zeroed();
        let mut exit: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        let cpu_100ns =
            if GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) != 0 {
                filetime_ticks(kernel) + filetime_ticks(user)
            } else {
                0
            };
        let mut threads = 0_u32;
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot != INVALID_HANDLE_VALUE && !snapshot.is_null() {
            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
            let own_pid = std::process::id();
            let mut next = Thread32First(snapshot, &mut entry) != 0;
            while next {
                if entry.th32OwnerProcessID == own_pid {
                    threads += 1;
                }
                next = Thread32Next(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
        }
        Some(ResourceSample {
            working_set: counters.WorkingSetSize as u64,
            peak_working_set: counters.PeakWorkingSetSize as u64,
            private_bytes: counters.PagefileUsage as u64,
            cpu_100ns,
            handles,
            threads,
        })
    }
}

/// 所属模块：diag / 内部
/// 功能：非 Windows 平台不提供资源采样。
/// 输入：无。
/// 输出/副作用：返回 `None`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: diag / internals
/// Purpose: Resource sampling is unavailable off Windows.
/// Inputs: none.
/// Outputs / side effects: `None`.
/// Complexity: O(1) time and space.
#[cfg(not(target_os = "windows"))]
fn sample_resources() -> Option<ResourceSample> {
    None
}

/// 所属模块：diag / 内部
/// 功能：把 `FILETIME` 转换为 100 ns 计数。
/// 输入：`time` Win32 时间结构。
/// 输出/副作用：返回 64 位计数；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: diag / internals
/// Purpose: Convert a `FILETIME` into 100 ns ticks.
/// Inputs: `time` — Win32 time struct.
/// Outputs / side effects: the 64-bit tick count; no state change.
/// Complexity: O(1) time and space.
#[cfg(target_os = "windows")]
fn filetime_ticks(time: winapi::shared::minwindef::FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

/// 所属模块：diag / 内部
/// 功能：写入尾帧并 flush。
/// 输入：`reason`：退出原因。
/// 输出/副作用：追加 FOOTER 记录并 flush；重复调用无效果。
/// 复杂度：时间 O(原因长度)，空间 O(原因长度)。
///
/// Module: diag / internals
/// Purpose: Write the footer and flush.
/// Inputs: `reason`: exit reason.
/// Outputs / side effects: appends the FOOTER record and flushes; repeated calls do nothing.
/// Complexity: O(reason length) time and space.
fn finish(reason: &str) {
    let mut guard = LOG.lock().unwrap_or_else(|poison| poison.into_inner());
    let Some(sink) = guard.as_mut() else {
        return;
    };
    if sink.finished {
        return;
    }
    write_footer(sink, reason);
    let _ = sink.file.flush();
}

/// 所属模块：diag / 内部
/// 功能：创建本次运行的日志文件并清理超出保留数量的旧日志。
/// 输入：无。
/// 输出/副作用：创建目录与文件；删除旧日志；返回日志句柄（失败为 `None`）。
/// 复杂度：时间 O(日志文件数)，空间 O(1)。
///
/// Module: diag / internals
/// Purpose: Create this run's log file and prune logs beyond the retention limit.
/// Inputs: none.
/// Outputs / side effects: creates the directory and file; removes old logs; returns the sink
/// (`None` on failure).
/// Complexity: O(log files) time and O(1) space.
fn open_sink() -> Option<LogSink> {
    let dir = log_dir();
    std::fs::create_dir_all(&dir).ok()?;
    // 先清理旧文件再创建本次文件：保证新建的日志不会被自己清理掉。
    // Prune before creating this run's file so the new log can never delete itself.
    prune_logs(&dir, LOG_KEEP_FILES.saturating_sub(1));
    let stamp = local_now().compact;
    let path = dir.join(format!("surfing-{stamp}-p{}.sflog", std::process::id()));
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    Some(LogSink {
        file,
        path,
        seq: 0,
        finished: false,
    })
}

/// 所属模块：diag / 内部
/// 功能：返回日志目录（由平台运行时按系统约定给出）。
/// 输入：无。
/// 输出/副作用：目录路径；不创建目录。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: diag / internals
/// Purpose: Return the log directory (the platform runtime supplies the per-OS convention).
/// Inputs: none.
/// Outputs / side effects: directory path; the directory is not created here.
/// Complexity: O(path length) time and space.
fn log_dir() -> PathBuf {
    crate::platform::log_dir()
}

/// 所属模块：diag / 内部
/// 功能：立即终止当前进程（由平台运行时选择 `TerminateProcess` 或 `process::exit`）。
/// 输入：无。
/// 输出/副作用：结束进程；正常情况下不返回。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: diag / internals
/// Purpose: Terminate the current process immediately (the platform runtime picks
/// `TerminateProcess` or `process::exit`).
/// Inputs: none.
/// Outputs / side effects: ends the process; normally never returns.
/// Complexity: O(1) time and space.
fn terminate_now() -> ! {
    crate::platform::exit_process(0)
}

/// 所属模块：diag / 内部
/// 功能：返回相对进程起点的微秒数。
/// 输入：无。
/// 输出/副作用：微秒数；首次调用会记录进程起点。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: diag / internals
/// Purpose: Return microseconds since the process start.
/// Inputs: none.
/// Outputs / side effects: the microsecond timestamp; the first call records the start.
/// Complexity: O(1) time and space.
fn now_us() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_micros() as u64
}

/// 所属模块：diag / 内部
/// 功能：返回当前 Unix 时间（微秒），写入文件头。
/// 输入：无。
/// 输出/副作用：Unix 微秒；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: diag / internals
/// Purpose: Return the current Unix time in microseconds for the file header.
/// Inputs: none.
/// Outputs / side effects: Unix microseconds; no state change.
/// Complexity: O(1) time and space.
fn start_unix_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_micros() as u64
}

/// 所属模块：diag / 内部
/// 功能：返回 UI 当前阶段；锁被占用时返回 `unknown`，绝不阻塞看门狗。
/// 输入：无。
/// 输出/副作用：阶段名；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: diag / internals
/// Purpose: Return the current UI phase; `unknown` when the lock is busy so the watchdog never
/// blocks.
/// Inputs: none.
/// Outputs / side effects: the phase name; no state change.
/// Complexity: O(1) time and space.
fn activity() -> &'static str {
    match ACTIVITY.try_lock() {
        Ok(guard) => *guard,
        Err(_) => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::format::{get_str, get_varint};
    use super::*;

    /// 覆盖：事件负载编码后可被解码器使用同一套原语还原。
    /// Covers: an encoded event payload can be reconstructed with the shared primitives.
    #[test]
    fn event_payload_round_trips() {
        let payload = encode_event_payload(
            "app.ui",
            "ui.stall",
            Some(1_500),
            &[("phase", "paint"), ("note", "a;b|c")],
        );
        let mut input: &[u8] = &payload;
        assert_eq!(get_str(&mut input), Some("app.ui"));
        assert_eq!(get_str(&mut input), Some("ui.stall"));
        assert_eq!(get_varint(&mut input), Some(1_500));
        let field_count = get_varint(&mut input).expect("field count");
        assert_eq!(field_count, 2);
        assert_eq!(get_str(&mut input), Some("phase"));
        assert_eq!(input[0], FIELD_STR);
        input = &input[1..];
        assert_eq!(get_str(&mut input), Some("paint"));
        assert_eq!(get_str(&mut input), Some("note"));
        assert_eq!(input[0], FIELD_STR);
        input = &input[1..];
        assert_eq!(get_str(&mut input), Some("a;b|c"));
        assert!(input.is_empty());
    }

    /// 覆盖：无耗时用 `u64::MAX` 表示，字段为空时字段数量为 0。
    /// Covers: a missing duration uses `u64::MAX` and an empty field list stores zero fields.
    #[test]
    fn event_payload_marks_missing_duration() {
        let payload = encode_event_payload("app.boot", "process.start", None, &[]);
        let mut input: &[u8] = &payload;
        let _ = get_str(&mut input);
        let _ = get_str(&mut input);
        assert_eq!(get_varint(&mut input), Some(u64::MAX));
        assert_eq!(get_varint(&mut input), Some(0));
    }

    /// 覆盖：级别编码与解码互为逆运算。
    /// Covers: level encoding and decoding are inverse operations.
    #[test]
    fn level_codes_round_trip() {
        for level in [
            Level::Trace,
            Level::Debug,
            Level::Info,
            Level::Warn,
            Level::Error,
            Level::Fatal,
        ] {
            assert_eq!(Level::from_code(level.code()), level);
        }
    }
}
