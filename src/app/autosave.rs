//! 所属模块：app / 自动保存与崩溃恢复
//! 文件用途：
//! - 按固定间隔把已保存过的工程自动写回原文件，避免长时间编辑后忘记保存。
//! - 独立崩溃恢复线程：持有最新工程快照并周期性写入恢复文件；工程从未保存时写入临时目录。
//! - 关闭窗口前的未保存确认对话框（保存并关闭 / 不保存 / 取消）。
//!
//! 函数清单：
//! - `SurfingApp::poll_autosave`：每帧轮询恢复结果、推送快照并按间隔触发自动保存。
//! - `SurfingApp::tick_autosave` / `push_recovery_snapshot` / `poll_recovery`：自动保存与恢复线程调度。
//! - `SurfingApp::remove_recovery_file` / `reset_autosave_state`：恢复文件清理与工程切换重置。
//! - `SurfingApp::has_unsaved_changes` / `ui_close_prompt`：关闭确认状态与对话框。
//! - `RecoveryWorker::spawn` / `send` / `try_recv` / `Drop`：崩溃恢复线程句柄。
//! - `run_recovery_worker` / `recovery_target_path` / `sanitize_file_name`：线程主体与恢复文件命名。
//!
//! Module: app / autosave and crash recovery
//! Purpose:
//! - Periodically write a previously saved project back to its file so long editing sessions are
//!   never lost.
//! - Standalone crash-recovery thread: holds the newest project snapshot and periodically writes a
//!   recovery file; projects that were never saved go to the temporary directory.
//! - Unsaved-changes confirmation dialog shown before a window closes (save and close / discard /
//!   cancel).
//!
//! Functions:
//! - `SurfingApp::poll_autosave`: per frame, poll recovery results, push snapshots and trigger the
//!   interval autosave.
//! - `SurfingApp::tick_autosave` / `push_recovery_snapshot` / `poll_recovery`: autosave and
//!   recovery-thread scheduling.
//! - `SurfingApp::remove_recovery_file` / `reset_autosave_state`: recovery-file cleanup and
//!   project-switch reset.
//! - `SurfingApp::has_unsaved_changes` / `ui_close_prompt`: close-confirmation state and dialog.
//! - `RecoveryWorker::spawn` / `send` / `try_recv` / `Drop`: crash-recovery thread handle.
//! - `run_recovery_worker` / `recovery_target_path` / `sanitize_file_name`: thread body and recovery
//!   file naming.

use super::*;
use crate::core::project_runtime::WaveformPeak;
use std::path::{Path, PathBuf};

/// 自动保存间隔的可接受范围（秒）；0 表示关闭，范围外回退默认值。
/// Accepted autosave interval range in seconds; 0 disables it and out-of-range values fall back.
pub(super) const AUTOSAVE_MIN_SECONDS: u64 = 15;
/// 崩溃恢复线程的写盘间隔：崩溃时最多丢失该时长内的编辑。
/// Disk-write interval of the crash-recovery thread; a crash loses at most this much editing.
const RECOVERY_SAVE_INTERVAL: Duration = Duration::from_secs(20);
/// 崩溃恢复快照的最小推送间隔：避免每次编辑都克隆整个工程。
/// Minimum interval between recovery snapshots, so editing does not clone the project every frame.
const RECOVERY_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(5);
/// 未保存工程恢复文件所在的临时目录名。
/// Name of the temporary directory holding recovery files of never-saved projects.
const RECOVERY_DIR_NAME: &str = "SurfingStudio";
/// 恢复文件后缀：紧邻工程文件，明确区分自动生成的备份。
/// Recovery-file suffix; it sits next to the project file and marks an automatically generated copy.
const RECOVERY_SUFFIX: &str = "autosave";

/// 关闭确认对话框中的用户选择。
/// User choice in the close-confirmation dialog.
enum ClosePromptAction {
    Save,
    Discard,
    Cancel,
}

/// 交给崩溃恢复线程的工程快照。
/// Project snapshot handed to the crash-recovery thread.
struct RecoverySnapshot {
    project: Project,
    peaks: Vec<WaveformPeak>,
    target: PathBuf,
    edit_seq: u64,
}

/// 恢复线程收到的请求；`Discard` 用于删除已无意义的恢复文件，`Shutdown` 用于退出线程。
/// Request received by the recovery thread; `Discard` removes an obsolete recovery file and
/// `Shutdown` exits the thread.
enum RecoveryRequest {
    Snapshot(Box<RecoverySnapshot>),
    Discard(PathBuf),
    Shutdown,
}

/// 恢复线程回传的写盘结果，由 UI 线程轮询用于提示。
/// Write result reported by the recovery thread and polled by the UI thread for status text.
struct RecoveryOutcome {
    target: PathBuf,
    result: Result<(), String>,
}

/// 崩溃恢复线程句柄：每个工程窗口一个，独立于 UI 线程与后台保存线程。
/// Crash-recovery thread handle: one per project window, independent from the UI and save threads.
pub(super) struct RecoveryWorker {
    requests: Sender<RecoveryRequest>,
    outcomes: Receiver<RecoveryOutcome>,
}

impl RecoveryWorker {
    /// 所属模块：app / 自动保存与崩溃恢复 / 线程
    /// 功能：启动常驻恢复线程；线程只在收到快照或关闭请求时被唤醒，不做忙等。
    /// 输入：无。
    /// 输出/副作用：创建线程并返回句柄；线程持有最新快照直至进程结束。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / autosave and crash recovery / thread
    /// Purpose: Start the persistent recovery thread; it wakes only for snapshots or shutdown.
    /// Inputs: none.
    /// Outputs / side effects: creates the thread and returns its handle; the thread keeps the
    /// newest snapshot until the process exits.
    /// Complexity: O(1) time and space.
    pub(super) fn spawn() -> Self {
        let (requests, request_rx) = channel();
        let (outcome_tx, outcomes) = channel();
        std::thread::spawn(move || run_recovery_worker(request_rx, outcome_tx));
        Self { requests, outcomes }
    }

    /// 所属模块：app / 自动保存与崩溃恢复 / 线程
    /// 功能：把最新工程快照交给恢复线程；线程内只保留最后一份。
    /// 输入：`snapshot` 工程、峰值、目标路径与编辑序号。
    /// 输出/副作用：写入发送队列；线程已退出时返回错误文本。
    /// 复杂度：时间 O(1)（不含快照克隆，克隆在调用方完成），空间 O(1)。
    ///
    /// Module: app / autosave and crash recovery / thread
    /// Purpose: Hand the newest project snapshot to the recovery thread, which keeps only the last.
    /// Inputs: `snapshot` — project, peaks, target path and edit sequence.
    /// Outputs / side effects: enqueues the snapshot; returns an error when the thread exited.
    /// Complexity: O(1) time (the clone happens in the caller) and O(1) space.
    fn send(&self, snapshot: RecoverySnapshot) -> Result<(), String> {
        self.requests
            .send(RecoveryRequest::Snapshot(Box::new(snapshot)))
            .map_err(|_| "崩溃恢复线程已停止".to_string())
    }

    /// 所属模块：app / 自动保存与崩溃恢复 / 线程
    /// 功能：请求删除恢复文件；由恢复线程在完成当前写盘后执行，避免“刚删又被写回”。
    /// 输入：`path` 要删除的恢复文件路径。
    /// 输出/副作用：写入发送队列；线程已退出时静默忽略。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / autosave and crash recovery / thread
    /// Purpose: Ask the recovery thread to delete a recovery file after its current write finishes,
    /// so a file is never re-created right after deletion.
    /// Inputs: `path` — recovery file to remove.
    /// Outputs / side effects: enqueues the request; ignored when the thread already exited.
    /// Complexity: O(1) time and space.
    fn discard(&self, path: PathBuf) {
        let _ = self.requests.send(RecoveryRequest::Discard(path));
    }

    /// 所属模块：app / 自动保存与崩溃恢复 / 线程
    /// 功能：非阻塞读取一条写盘结果。
    /// 输入：`self`。
    /// 输出/副作用：返回结果或 `None`；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / autosave and crash recovery / thread
    /// Purpose: Non-blocking read of one write result.
    /// Inputs: `self`.
    /// Outputs / side effects: the result or `None`; no state change.
    /// Complexity: O(1) time and space.
    fn try_recv(&self) -> Option<RecoveryOutcome> {
        self.outcomes.try_recv().ok()
    }
}

impl Drop for RecoveryWorker {
    /// 所属模块：app / 自动保存与崩溃恢复 / 线程
    /// 功能：窗口关闭时通知恢复线程退出，避免它继续持有工程快照。
    /// 输入：`self`。
    /// 输出/副作用：发送 `Shutdown`；线程在当前写盘结束后退出。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / autosave and crash recovery / thread
    /// Purpose: Ask the recovery thread to exit when the window closes, so it no longer holds the
    /// project snapshot.
    /// Inputs: `self`.
    /// Outputs / side effects: sends `Shutdown`; the thread exits after the current write.
    /// Complexity: O(1) time and space.
    fn drop(&mut self) {
        let _ = self.requests.send(RecoveryRequest::Shutdown);
    }
}

/// 所属模块：app / 自动保存与崩溃恢复 / 线程
/// 功能：恢复线程主体：缓存最新快照，按间隔写盘；内容未变化时不重复写。
/// 输入：
/// - `requests`：快照与退出请求。
/// - `outcomes`：写盘结果回传通道。
/// 输出/副作用：在后台写恢复文件；不接触 UI 与引擎。
/// 复杂度：时间 O(快照数 + 写盘次数 × 工程大小)，空间 O(最新工程大小)。
///
/// Module: app / autosave and crash recovery / thread
/// Purpose: Recovery-thread body: cache the newest snapshot and write it on an interval, skipping
/// writes when the content did not change.
/// Inputs:
/// - `requests`: snapshots and shutdown requests.
/// - `outcomes`: channel that reports write results.
/// Outputs / side effects: writes recovery files in the background; never touches the UI or engine.
/// Complexity: O(snapshot count + write count × project size) time and O(newest project) space.
fn run_recovery_worker(requests: Receiver<RecoveryRequest>, outcomes: Sender<RecoveryOutcome>) {
    let mut pending: Option<RecoverySnapshot> = None;
    let mut written: Option<(PathBuf, u64)> = None;
    // 首个快照立即写盘，之后按间隔写，缩短崩溃时的数据丢失窗口。
    // Write the first snapshot immediately, then on an interval, shrinking the loss window.
    let mut next_write = Instant::now();
    loop {
        let wait = next_write.saturating_duration_since(Instant::now());
        match requests.recv_timeout(wait) {
            Ok(RecoveryRequest::Snapshot(snapshot)) => {
                pending = Some(*snapshot);
                if Instant::now() < next_write {
                    continue;
                }
            }
            Ok(RecoveryRequest::Discard(path)) => {
                // 删除请求到达后，之前的快照一律作废，避免文件刚删除又被写回。
                // Once a discard arrives, earlier snapshots are obsolete so the file never
                // reappears right after deletion.
                pending = None;
                written = None;
                let _ = std::fs::remove_file(path);
                continue;
            }
            Ok(RecoveryRequest::Shutdown) => return,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        }
        if let Some(snapshot) = pending.take() {
            let target = snapshot.target;
            let marker = (target.clone(), snapshot.edit_seq);
            if written.as_ref() != Some(&marker) {
                // 恢复文件可能位于尚未创建的临时目录，写盘前先确保父目录存在。
                // The recovery file may live in a missing temporary directory, so create it first.
                if let Some(parent) = target.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let result = crate::core::project_runtime::ProjectRuntime::save_project(
                    &target,
                    &snapshot.project,
                    &snapshot.peaks,
                );
                written = Some(marker);
                let _ = outcomes.send(RecoveryOutcome {
                    target: target.clone(),
                    result,
                });
            }
        }
        next_write = Instant::now() + RECOVERY_SAVE_INTERVAL;
    }
}

/// 所属模块：app / 自动保存与崩溃恢复 / 路径
/// 功能：计算崩溃恢复文件路径：已保存工程放在工程目录的 `.autosave.sfsd`，未保存工程放临时目录。
/// 输入：
/// - `project_path`：工程当前路径；`None` 表示从未保存。
/// - `title`：工程展示名，用于临时文件名。
/// - `window_id`：窗口编号，避免多个未保存工程互相覆盖。
/// 输出/副作用：返回恢复文件路径；不创建文件。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: app / autosave and crash recovery / paths
/// Purpose: Compute the recovery-file path: `.autosave.sfsd` next to a saved project, or the
/// temporary directory for a never-saved project.
/// Inputs:
/// - `project_path`: current project path; `None` means it was never saved.
/// - `title`: display name used for the temporary file name.
/// - `window_id`: window id so several unsaved projects never overwrite each other.
/// Outputs / side effects: the recovery path; no file is created.
/// Complexity: O(path length) time and space.
fn recovery_target_path(project_path: Option<&Path>, title: &str, window_id: u64) -> PathBuf {
    if let Some(path) = project_path {
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .filter(|stem| !stem.is_empty())
            .unwrap_or_else(|| "project".to_string());
        return path.with_file_name(format!("{stem}.{RECOVERY_SUFFIX}.sfsd"));
    }
    std::env::temp_dir()
        .join(RECOVERY_DIR_NAME)
        .join(format!("{}-{window_id}.sfsd", sanitize_file_name(title)))
}

/// 所属模块：app / 自动保存与崩溃恢复 / 路径
/// 功能：把工程名转换为合法的文件名片段；非字母数字字符丢弃，空白转下划线。
/// 输入：`name` 工程名。
/// 输出/副作用：返回文件名片段；全部被过滤时回退为 `project`。
/// 复杂度：时间 O(字符数)，空间 O(字符数)。
///
/// Module: app / autosave and crash recovery / paths
/// Purpose: Turn a project name into a legal file-name fragment; non-alphanumeric characters are
/// dropped and whitespace becomes an underscore.
/// Inputs: `name` — project name.
/// Outputs / side effects: the fragment; falls back to `project` when everything is filtered.
/// Complexity: O(character count) time and space.
fn sanitize_file_name(name: &str) -> String {
    let mut fragment = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_alphanumeric() || character == '-' || character == '_' {
            fragment.push(character);
        } else if character.is_whitespace() {
            fragment.push('_');
        }
    }
    if fragment.is_empty() {
        "project".to_string()
    } else {
        fragment
    }
}

impl SurfingApp {
    /// 所属模块：app / 自动保存与崩溃恢复
    /// 功能：每帧统一调度：轮询恢复结果 → 推送最新快照 → 按间隔触发自动保存。
    /// 输入：`self`。
    /// 输出/副作用：可能启动一次自动保存、更新状态栏、推送恢复快照；不绘制界面。
    /// 复杂度：时间 O(1)（快照克隆按间隔发生），空间 O(1)。
    ///
    /// Module: app / autosave and crash recovery
    /// Purpose: Per-frame scheduling: poll recovery results → push the newest snapshot → trigger
    /// the interval autosave.
    /// Inputs: `self`.
    /// Outputs / side effects: may start one autosave, update the status bar and push a recovery
    /// snapshot; paints nothing.
    /// Complexity: O(1) time (the snapshot clone happens on an interval) and O(1) space.
    pub(super) fn poll_autosave(&mut self) {
        self.poll_recovery();
        self.push_recovery_snapshot();
        self.tick_autosave();
    }

    /// 所属模块：app / 自动保存与崩溃恢复
    /// 功能：到达设置中的间隔且工程已有路径时启动自动保存；间隔为 0 表示关闭。
    /// 输入：`self`。
    /// 输出/副作用：可能设置 `save_rx`、`last_autosave` 与状态栏。
    /// 复杂度：时间 O(1)，空间 O(1)（写盘在后台线程）。
    ///
    /// Module: app / autosave and crash recovery
    /// Purpose: Start an autosave when the configured interval elapsed and the project has a path;
    /// an interval of 0 disables autosave.
    /// Inputs: `self`.
    /// Outputs / side effects: may set `save_rx`, `last_autosave` and the status bar.
    /// Complexity: O(1) time and O(1) space (the write runs on a worker thread).
    fn tick_autosave(&mut self) {
        if self.settings.autosave_seconds < AUTOSAVE_MIN_SECONDS {
            return;
        }
        if !self.dirty || self.project_path.is_none() || self.close_prompt {
            return;
        }
        if self.save_rx.is_some()
            || self.engine_sync_rx.is_some()
            || self.is_loading_project()
            || self.instrument_load_pending
        {
            return;
        }
        if self.last_autosave.elapsed() < Duration::from_secs(self.settings.autosave_seconds) {
            return;
        }
        self.last_autosave = Instant::now();
        self.save_project_in_background(false, true);
    }

    /// 所属模块：app / 自动保存与崩溃恢复
    /// 功能：把当前工程快照交给崩溃恢复线程；内容或工程代次变化时才推送。
    /// 输入：`self`。
    /// 输出/副作用：可能替换 `recovery_path`（并删除旧恢复文件）、写入 `last_recovery_marker` 与
    /// `last_recovery_push`；不修改工程。
    /// 复杂度：时间 O(工程大小)（仅按间隔克隆），空间 O(工程大小)。
    ///
    /// Module: app / autosave and crash recovery
    /// Purpose: Hand the current project snapshot to the crash-recovery thread; only when the
    /// content or the project epoch changed.
    /// Inputs: `self`.
    /// Outputs / side effects: may replace `recovery_path` (removing the old file) and update
    /// `last_recovery_marker` / `last_recovery_push`; the project is untouched.
    /// Complexity: O(project size) time (the clone happens on an interval) and O(project size) space.
    fn push_recovery_snapshot(&mut self) {
        if self.recovery.is_none() || !self.dirty {
            return;
        }
        let marker = (self.project_epoch, self.edit_seq);
        if self.last_recovery_marker == Some(marker) {
            return;
        }
        if self.last_recovery_push.elapsed() < RECOVERY_SNAPSHOT_INTERVAL {
            return;
        }
        let title = self.project_title();
        let target = recovery_target_path(self.project_path.as_deref(), &title, self.window_id);
        if self.recovery_path.as_ref() != Some(&target) {
            // 目标路径变化（工程另存为 / 首次保存）时先让线程删除旧文件，再记录新路径。
            // When the target changes (save-as or first save), let the thread remove the old file
            // before recording the new path.
            if let Some(previous) = self.recovery_path.take() {
                if let Some(worker) = &self.recovery {
                    worker.discard(previous);
                }
            }
            self.recovery_path = Some(target.clone());
        }
        let snapshot = RecoverySnapshot {
            project: self.project.clone(),
            peaks: self.collect_peak_entries(),
            target,
            edit_seq: self.edit_seq,
        };
        let Some(worker) = &self.recovery else {
            return;
        };
        if worker.send(snapshot).is_ok() {
            self.last_recovery_marker = Some(marker);
            self.last_recovery_push = Instant::now();
        }
    }

    /// 所属模块：app / 自动保存与崩溃恢复
    /// 功能：轮询恢复线程结果；失败写入状态栏，未保存工程首次生成恢复文件时提示一次。
    /// 输入：`self`。
    /// 输出/副作用：可能更新 `status` 与 `recovery_notice_shown`。
    /// 复杂度：时间 O(结果数)，空间 O(1)。
    ///
    /// Module: app / autosave and crash recovery
    /// Purpose: Poll recovery results; report failures and, for never-saved projects, announce the
    /// first recovery file once.
    /// Inputs: `self`.
    /// Outputs / side effects: may update `status` and `recovery_notice_shown`.
    /// Complexity: O(result count) time and O(1) space.
    fn poll_recovery(&mut self) {
        let Some(worker) = &self.recovery else {
            return;
        };
        let mut latest = None;
        while let Some(outcome) = worker.try_recv() {
            latest = Some(outcome);
        }
        let Some(outcome) = latest else {
            return;
        };
        match outcome.result {
            Ok(()) => {
                if self.project_path.is_none() && !self.recovery_notice_shown {
                    self.recovery_notice_shown = true;
                    self.status = format!(
                        "工程尚未保存，已在临时目录创建恢复文件 {}",
                        outcome.target.display()
                    );
                }
            }
            Err(error) => {
                self.status = format!("崩溃恢复文件写入失败: {error}");
            }
        }
    }

    /// 所属模块：app / 自动保存与崩溃恢复
    /// 功能：删除当前恢复文件（工程已保存或用户放弃修改后不再需要）。
    /// 输入：`self`。
    /// 输出/副作用：清空 `recovery_path` 并尽力删除文件；失败静默忽略。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / autosave and crash recovery
    /// Purpose: Remove the current recovery file (no longer needed once saved or explicitly
    /// discarded).
    /// Inputs: `self`.
    /// Outputs / side effects: clears `recovery_path` and best-effort deletes the file.
    /// Complexity: O(1) time and space.
    pub(super) fn remove_recovery_file(&mut self) {
        if let Some(path) = self.recovery_path.take() {
            // 交给恢复线程删除：它可能正在写同一个文件，由它按消息顺序收尾最安全。
            // Let the recovery thread delete it: it may be writing that file right now, and
            // message order guarantees the delete happens last.
            match &self.recovery {
                Some(worker) => worker.discard(path),
                None => {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }

    /// 所属模块：app / 自动保存与崩溃恢复
    /// 功能：工程被替换（新建 / 打开 / 导入）后重置自动保存、恢复快照与关闭确认状态。
    /// 输入：`self`。
    /// 输出/副作用：删除旧恢复文件；重置计时器、快照标记与未保存确认。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / autosave and crash recovery
    /// Purpose: Reset autosave, the recovery snapshot and the close confirmation after the project
    /// is replaced (new / open / import).
    /// Inputs: `self`.
    /// Outputs / side effects: removes the old recovery file, resets timers/snapshot markers and
    /// dismisses the close confirmation.
    /// Complexity: O(1) time and space.
    pub(super) fn reset_autosave_state(&mut self) {
        self.remove_recovery_file();
        self.last_autosave = Instant::now();
        self.last_recovery_push = Instant::now();
        // 以当前状态作为基线：新工程在第一次真实编辑前不产生恢复文件。
        // Use the current state as the baseline: a fresh project produces no recovery file until
        // the first real edit.
        self.last_recovery_marker = Some((self.project_epoch, self.edit_seq));
        self.recovery_notice_shown = false;
        self.close_prompt = false;
        self.pending_close = false;
    }

    /// 所属模块：app / 关闭确认
    /// 功能：判断窗口关闭时是否需要确认：存在未保存修改或确认对话框已打开。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / close confirmation
    /// Purpose: Whether closing the window needs confirmation: unsaved changes exist or the dialog
    /// is already open.
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1) time and space.
    pub(crate) fn has_unsaved_changes(&self) -> bool {
        self.dirty || self.close_prompt
    }

    /// 所属模块：app / 关闭确认
    /// 功能：绘制未保存确认对话框，并把用户选择转换为保存、放弃或取消。
    /// 输入：`ctx`：当前窗口的 egui 上下文。
    /// 输出/副作用：可能启动保存、清除 `dirty`、删除恢复文件或请求关闭窗口。
    /// 复杂度：每帧 O(1)。
    ///
    /// Module: app / close confirmation
    /// Purpose: Draw the unsaved-changes dialog and turn the user's choice into save, discard or
    /// cancel.
    /// Inputs: `ctx`: egui context of this window.
    /// Outputs / side effects: may start a save, clear `dirty`, remove the recovery file or request
    /// the window to close.
    /// Complexity: O(1) per frame.
    pub(super) fn ui_close_prompt(&mut self, ctx: &egui::Context) {
        let title = self.project_title();
        let saving = self.save_rx.is_some();
        let mut action: Option<ClosePromptAction> = None;
        let response = egui::Modal::new(egui::Id::new("unsaved_close_prompt")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(
                egui::RichText::new("保存更改")
                    .size(14.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(format!("工程「{title}」有未保存的修改，关闭后会丢失。"))
                    .size(11.5)
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if saving {
                    ui.add(egui::Spinner::new().size(14.0));
                    ui.label(
                        egui::RichText::new("正在保存…")
                            .size(11.5)
                            .color(theme::TEXT_DIM),
                    );
                    return;
                }
                if ui
                    .button("保存并关闭")
                    .on_hover_text("保存工程后关闭窗口")
                    .clicked()
                {
                    action = Some(ClosePromptAction::Save);
                }
                if ui
                    .button("不保存")
                    .on_hover_text("丢弃未保存的修改并关闭窗口")
                    .clicked()
                {
                    action = Some(ClosePromptAction::Discard);
                }
                if ui
                    .button("取消")
                    .on_hover_text("返回编辑，不关闭窗口")
                    .clicked()
                {
                    action = Some(ClosePromptAction::Cancel);
                }
            });
        });
        // 点击遮罩或按下 Esc 等同于取消，避免误触导致修改丢失。
        // Clicking the backdrop or pressing Esc counts as cancel so changes are never lost by a
        // stray click.
        if response.should_close() && action.is_none() {
            action = Some(ClosePromptAction::Cancel);
        }
        match action {
            Some(ClosePromptAction::Save) => {
                self.save_project();
                // 保存已启动或已有保存在进行时保持对话框；状态栏已由保存流程给出原因
                // （例如插件仍在加载、保存被取消），对话框保持可见供用户再次选择。
                // Keep the dialog while the save runs or another save is in flight; the save flow
                // already explains why nothing started (plugins loading, dialog cancelled).
            }
            Some(ClosePromptAction::Discard) => {
                self.dirty = false;
                self.close_prompt = false;
                self.remove_recovery_file();
                self.pending_close = true;
            }
            Some(ClosePromptAction::Cancel) => {
                self.close_prompt = false;
                self.status = "已取消关闭".into();
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：从未保存的工程把恢复文件写入临时目录，并用窗口编号区分。
    /// Covers: never-saved projects put the recovery file in the temp directory, keyed by window id.
    #[test]
    fn unsaved_projects_use_the_temp_directory() {
        let target = recovery_target_path(None, "我的 工程", 7);
        assert!(target.starts_with(std::env::temp_dir().join(RECOVERY_DIR_NAME)));
        assert!(target
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with("-7.sfsd")));
    }

    /// 覆盖：已保存工程的恢复文件紧邻工程文件，且不覆盖原文件。
    /// Covers: a saved project keeps its recovery file next to the project without overwriting it.
    #[test]
    fn saved_projects_use_a_sibling_autosave_file() {
        let project = Path::new("C:/Music/Demo.sfsd");
        let target = recovery_target_path(Some(project), "Demo", 1);
        assert_eq!(target, Path::new("C:/Music/Demo.autosave.sfsd"));
    }

    /// 覆盖：非法文件名片段被清理，空名回退为 `project`。
    /// Covers: illegal file-name fragments are stripped and an empty name falls back to `project`.
    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(sanitize_file_name("a/b:c*d"), "abcd");
        assert_eq!(sanitize_file_name("   "), "___");
        assert_eq!(sanitize_file_name("///"), "project");
    }
}
