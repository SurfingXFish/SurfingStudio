//! 所属模块：app / 工程历史版本
//! 文件用途：
//! - 在保存前把现有工程复制到同目录的 `Backups` 文件夹，形成可回看的历史版本。
//! - 按设置中的保留数量清理最旧版本，并为主页项目下拉提供版本列表。
//!
//! 函数清单：
//! - `BackupVersion`：一个历史版本的路径、保存时间与大小。
//! - `backup_dir` / `backup_prefix` / `backup_versions`：版本目录、文件名前缀与列表。
//! - `create_backup` / `prune_backups` / `is_history_path`：创建、清理历史版本与路径判定。
//!
//! Module: app / project history versions
//! Purpose:
//! - Copy the existing project into a sibling `Backups` folder before saving, producing reviewable
//!   historical versions.
//! - Prune the oldest versions according to the configured retention and expose the list to the
//!   home page project dropdown.
//!
//! Functions:
//! - `BackupVersion`: path, save time and size of one history version.
//! - `backup_dir` / `backup_prefix` / `backup_versions`: version directory, name prefix and list.
//! - `create_backup` / `prune_backups`: create and prune history versions.

use super::time_fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// 历史版本目录名。
/// History-version directory name.
const BACKUP_DIR_NAME: &str = "Backups";
/// 历史版本文件扩展名。
/// History-version file extension.
const BACKUP_EXTENSION: &str = "sfsd";
/// 两次自动备份之间的最小间隔：自动保存每分钟一次时避免堆满版本。
/// Minimum interval between automatic backups, so minute-level autosaves do not flood versions.
const BACKUP_MIN_INTERVAL: Duration = Duration::from_secs(300);
/// 版本文件名中的时间戳长度（`YYYYMMDD-HHmmss`）。
/// Length of the timestamp inside a version file name (`YYYYMMDD-HHmmss`).
const BACKUP_STAMP_LEN: usize = 15;

/// 一个历史版本。
/// One history version.
#[derive(Clone, Debug)]
pub(super) struct BackupVersion {
    pub path: PathBuf,
    pub saved_at: SystemTime,
    pub bytes: u64,
}

/// 所属模块：app / 工程历史版本
/// 功能：判断路径是否位于历史版本目录，打开这些文件时不应写入项目列表。
/// 输入：`path` 工程文件路径。
/// 输出/副作用：布尔值；不修改状态。
/// 复杂度：时间 O(路径长度)，空间 O(1)。
///
/// Module: app / project history versions
/// Purpose: Whether a path lives in the history directory; opening such files must not join the
/// project list.
/// Inputs: `path` — project file path.
/// Outputs / side effects: boolean; no state change.
/// Complexity: O(path length) time and O(1) space.
pub(super) fn is_history_path(path: &Path) -> bool {
    path.parent()
        .and_then(|parent| parent.file_name())
        .is_some_and(|name| name.eq_ignore_ascii_case(BACKUP_DIR_NAME))
}

/// 所属模块：app / 工程历史版本
/// 功能：返回工程对应的历史版本目录 `<工程目录>/Backups`。
/// 输入：`project` 工程文件路径。
/// 输出/副作用：返回目录路径；不创建目录。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: app / project history versions
/// Purpose: Return the history directory `<project dir>/Backups`.
/// Inputs: `project` — project file path.
/// Outputs / side effects: the directory path; no directory is created.
/// Complexity: O(path length) time and space.
pub(super) fn backup_dir(project: &Path) -> PathBuf {
    project
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(BACKUP_DIR_NAME)
}

/// 所属模块：app / 工程历史版本
/// 功能：返回历史版本文件名前缀 `<工程名>-`。
/// 输入：`project` 工程文件路径。
/// 输出/副作用：返回前缀文本；不修改状态。
/// 复杂度：时间 O(名称长度)，空间 O(名称长度)。
///
/// Module: app / project history versions
/// Purpose: Return the version file-name prefix `<stem>-`.
/// Inputs: `project` — project file path.
/// Outputs / side effects: the prefix text; no state change.
/// Complexity: O(name length) time and space.
fn backup_prefix(project: &Path) -> String {
    let stem = project
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "project".into());
    format!("{stem}-")
}

/// 所属模块：app / 工程历史版本
/// 功能：列出某个工程的历史版本，按保存时间从新到旧排序。
/// 输入：`project` 工程文件路径。
/// 输出/副作用：返回版本列表；只读目录。
/// 复杂度：时间 O(目录项数)，空间 O(版本数)。
///
/// Module: app / project history versions
/// Purpose: List the history versions of a project, newest first.
/// Inputs: `project` — project file path.
/// Outputs / side effects: the version list; reads the directory only.
/// Complexity: O(directory entries) time and O(versions) space.
pub(super) fn backup_versions(project: &Path) -> Vec<BackupVersion> {
    let directory = backup_dir(project);
    let prefix = backup_prefix(project);
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut versions: Vec<BackupVersion> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some(BACKUP_EXTENSION) {
                return None;
            }
            let name = path.file_name()?.to_string_lossy().to_string();
            if !name.starts_with(&prefix) {
                return None;
            }
            // 只接受 `<前缀><时间戳>.sfsd`，避免把名称相近的其它文件当成版本。
            // Accept only `<prefix><stamp>.sfsd` so unrelated look-alike files are ignored.
            let stamp = &name[prefix.len()..name.len().saturating_sub(5)];
            if stamp.len() < BACKUP_STAMP_LEN || !stamp.contains('-') {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            Some(BackupVersion {
                saved_at: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                bytes: metadata.len(),
                path,
            })
        })
        .collect();
    versions.sort_by_key(|version| std::cmp::Reverse(version.saved_at));
    versions
}

/// 所属模块：app / 工程历史版本
/// 功能：保存前创建一份历史版本：工程存在、距离上次备份超过最小间隔时才复制。
/// 输入：
/// - `project`：现有工程文件路径。
/// - `retention`：保留的版本数量（至少 1）。
/// 输出/副作用：创建 `Backups` 目录并复制文件，随后清理超出保留数的旧版本；
/// 返回新建版本的路径（未创建时为 `None`）。
/// 复杂度：时间 O(工程大小 + 版本数)，空间 O(工程大小)。
///
/// Module: app / project history versions
/// Purpose: Create a history version before saving: copy the existing project only when it exists
/// and the newest backup is older than the minimum interval.
/// Inputs:
/// - `project`: existing project file path.
/// - `retention`: number of kept versions (at least 1).
/// Outputs / side effects: creates the `Backups` directory, copies the file and prunes old versions;
/// returns the new version path (`None` when nothing was created).
/// Complexity: O(project size + versions) time and O(project size) space.
pub(super) fn create_backup(project: &Path, retention: usize) -> Result<Option<PathBuf>, String> {
    if !project.is_file() {
        return Ok(None);
    }
    let versions = backup_versions(project);
    if let Some(newest) = versions.first() {
        if SystemTime::now()
            .duration_since(newest.saved_at)
            .unwrap_or(Duration::ZERO)
            < BACKUP_MIN_INTERVAL
        {
            return Ok(None);
        }
    }
    let directory = backup_dir(project);
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("创建历史版本目录失败: {error}"))?;
    let prefix = backup_prefix(project);
    let stamp = time_fmt::format_compact(SystemTime::now());
    let mut target = directory.join(format!("{prefix}{stamp}.{BACKUP_EXTENSION}"));
    let mut suffix = 2;
    while target.exists() {
        target = directory.join(format!("{prefix}{stamp}-{suffix}.{BACKUP_EXTENSION}"));
        suffix += 1;
    }
    std::fs::copy(project, &target).map_err(|error| format!("写入历史版本失败: {error}"))?;
    prune_backups(project, retention);
    Ok(Some(target))
}

/// 所属模块：app / 工程历史版本
/// 功能：按保留数量删除最旧版本，保留最新的 `retention` 个。
/// 输入：
/// - `project`：工程文件路径。
/// - `retention`：保留数量（至少 1）。
/// 输出/副作用：删除多余文件；失败静默忽略（历史版本不是关键数据）。
/// 复杂度：时间 O(版本数)，空间 O(1)。
///
/// Module: app / project history versions
/// Purpose: Delete the oldest versions and keep the newest `retention` files.
/// Inputs:
/// - `project`: project file path.
/// - `retention`: number of kept versions (at least 1).
/// Outputs / side effects: removes extra files; failures are ignored (versions are not critical).
/// Complexity: O(versions) time and O(1) space.
pub(super) fn prune_backups(project: &Path, retention: usize) {
    let keep = retention.max(1);
    for version in backup_versions(project).into_iter().skip(keep) {
        let _ = std::fs::remove_file(version.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：历史版本目录与文件名前缀跟随工程名。
    /// Covers: the history directory and prefix follow the project name.
    #[test]
    fn backup_location_follows_the_project_name() {
        let project = Path::new("C:/Music/Demo.sfsd");
        assert_eq!(backup_dir(project), Path::new("C:/Music/Backups"));
        assert_eq!(backup_prefix(project), "Demo-");
    }

    /// 覆盖：创建历史版本后可按时间列出，并按保留数量清理。
    /// Covers: a created version is listed by time and pruned to the retention count.
    #[test]
    fn backups_are_created_listed_and_pruned() {
        let directory = std::env::temp_dir().join(format!(
            "surfing_backup_test_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|duration| duration.subsec_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&directory).expect("create temp dir");
        let project = directory.join("Demo.sfsd");
        std::fs::write(&project, b"first").expect("write project");
        let created = create_backup(&project, 1)
            .expect("backup")
            .expect("created");
        assert!(created.exists());
        let versions = backup_versions(&project);
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].bytes, 5);
        // 间隔限制内不会重复创建。
        // No duplicate backup inside the minimum interval.
        assert!(create_backup(&project, 1).expect("second").is_none());
        let _ = std::fs::remove_dir_all(&directory);
    }
}
