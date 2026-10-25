//! 所属模块：core / 工程运行时 / SoundBug 后端 / 入口
//! 文件用途：
//! - 提供 SoundBug `.sndp` 导入入口与结果类型。
//! - 编排清单解析、EDIT 树解析与工程转换，并保持只读导入。
//!
//! 函数清单：
//! - `import_project`：读取 `.sndp` + `comp.dat` 并返回转换后的工程与警告。
//!
//! Module: core / project runtime / SoundBug backend / entry
//! Purpose:
//! - Provide the SoundBug `.sndp` import entry point and result type.
//! - Orchestrate manifest parsing, EDIT tree parsing and project conversion; the
//!   original project is only read.
//!
//! Functions:
//! - `import_project`: read `.sndp` + `comp.dat` and return the converted project
//!   plus warnings.

use super::convert::{
    import_track, read_bpm, read_master_volume, read_time_signature, seconds_to_beats,
};
use super::edit::parse_edit;
use super::manifest::{parse_sndp, read_comp};
use crate::core::model::{Clip, Project};
use std::collections::HashSet;
use std::path::Path;

/// 导入结果：工程本体与可展示给用户的警告。
/// Import result: the converted project and user-facing warnings.
pub struct ImportedProject {
    pub project: Project,
    pub warnings: Vec<String>,
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 入口
/// 功能：读取 `.sndp` 与其引用的 `comp.dat`，转换为 SurfingStudio 工程。
/// 输入：
/// - `path`：`.sndp` 文件路径；`comp.dat` 路径从清单媒体表解析。
///
/// 输出/副作用：
/// - 返回转换后的 `Project` 与警告列表；不修改原始文件。
///
/// 复杂度：时间 O(清单大小 + comp.dat 大小 + 音符数)，空间 O(解压后大小)。
///
/// Module: core / project runtime / SoundBug backend / entry
/// Purpose: Read an `.sndp` manifest and its `comp.dat`, then convert them into a
/// SurfingStudio project.
/// Inputs:
/// - `path`: path to the `.sndp` file; `comp.dat` is resolved from the media table.
///
/// Outputs / side effects:
/// - Returns the converted `Project` and a warning list; original files are untouched.
///
/// Complexity: O(manifest size + comp.dat size + note count) time and
/// O(decompressed size) space.
pub fn import_project(path: &Path) -> Result<ImportedProject, String> {
    let manifest = parse_sndp(path)?;
    let comp_path = manifest
        .comp_path
        .clone()
        .ok_or_else(|| "SoundBug 工程缺少 comp.dat，无法导入".to_string())?;
    let raw = read_comp(&comp_path)?;
    let root = parse_edit(&raw)?;
    let bpm = read_bpm(&root).unwrap_or(120.0);
    let (time_sig_num, time_sig_den) = read_time_signature(&root);
    let master_volume = read_master_volume(&root).unwrap_or(0.85);

    let mut warnings = Vec::new();
    let mut used_track_ids = HashSet::new();
    let mut next_track_id = 1u64;
    let mut used_clip_ids = HashSet::new();
    let mut next_clip_id = 1u64;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));

    let mut tracks = Vec::new();
    for child in &root.children {
        if child.name != "TRACK" && child.name != "WAVETRACK" {
            continue;
        }
        if let Some(track) = import_track(
            child,
            &manifest,
            base_dir,
            bpm,
            &mut used_track_ids,
            &mut next_track_id,
            &mut used_clip_ids,
            &mut next_clip_id,
            &mut warnings,
        )? {
            tracks.push(track);
        }
    }

    let content_end_beats = tracks
        .iter()
        .flat_map(|track| track.clips.iter())
        .map(Clip::end)
        .fold(0.0_f64, f64::max);
    let timeline_end_beats = seconds_to_beats(manifest.duration_seconds, bpm).max(0.0);
    let length_beats = content_end_beats.max(timeline_end_beats).max(4.0);

    let project = Project {
        version: crate::core::model::CURRENT_PROJECT_VERSION,
        name: if manifest.name.trim().is_empty() {
            "SoundBug 工程".to_string()
        } else {
            manifest.name.trim().to_string()
        },
        bpm,
        time_sig_num,
        time_sig_den,
        tracks,
        master_volume,
        master_controllers: Default::default(),
        master_effect: None,
        loop_enabled: false,
        loop_start: 0.0,
        loop_end: length_beats,
        length_beats,
    };

    Ok(ImportedProject { project, warnings })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::InstrumentRef;
    use std::path::PathBuf;

    /// 覆盖：仓库示例工程可被完整导入（示例缺失时跳过）。
    /// Covers: the bundled example project imports completely (skipped when absent).
    #[test]
    fn imports_bundled_soundbug_example() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join("光年外的流星_编曲工程")
            .join("光年外的流星.sndp");
        if !path.exists() {
            eprintln!("示例工程不存在，跳过: {}", path.display());
            return;
        }
        let imported = import_project(&path).expect("import SoundBug example");
        let project = imported.project;
        assert!((project.bpm - 124.0).abs() < 1e-6, "bpm={}", project.bpm);
        assert_eq!((project.time_sig_num, project.time_sig_den), (4, 4));
        assert!(
            project.tracks.len() >= 10,
            "tracks={}",
            project.tracks.len()
        );
        let audio_clips = project
            .tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .filter(|clip| clip.audio.is_some())
            .count();
        assert!(audio_clips >= 5, "audio clips={audio_clips}");
        let missing_audio = project
            .tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .filter_map(|clip| clip.audio.as_ref())
            .filter(|source| !source.path.exists())
            .count();
        assert_eq!(missing_audio, 0, "media paths must resolve next to .sndp");
        let notes = project
            .tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .map(|clip| clip.notes.len())
            .sum::<usize>();
        assert!(notes > 2000, "notes={notes}");
        let offset = project
            .tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .filter_map(|clip| clip.audio.as_ref())
            .map(|source| source.offset_seconds)
            .fold(0.0_f64, f64::max);
        assert!(
            offset > 1.0,
            "expected a non-zero source offset, got {offset}"
        );
        assert!(
            project.length_beats > 400.0,
            "length={}",
            project.length_beats
        );
        // 插件状态必须随导入保留，而不是只留下路径。
        // Plugin state must survive the import, not just the path.
        let plugins: Vec<_> = project
            .tracks
            .iter()
            .flat_map(|track| {
                let instrument = match &track.instrument {
                    InstrumentRef::Plugin(plugin) => Some(plugin.clone()),
                    _ => None,
                };
                instrument.into_iter().chain(
                    track
                        .effects
                        .iter()
                        .filter_map(|effect| effect.plugin().cloned()),
                )
            })
            .collect();
        assert!(
            plugins.iter().any(|plugin| !plugin.state.is_empty()),
            "imported plugins must keep their state"
        );
        // 文本编码的状态可能不是标准 VST3 状态，只保留不自动恢复。
        // Text-encoded states may not be standard VST3 states; preserve but do
        // not auto-restore them.
        assert!(
            plugins.iter().any(|plugin| !plugin.restore_state),
            "unverified imported state must be marked as not auto-restored"
        );
    }

    /// 覆盖：SoundBug 导入结果可保存为 `.sfsd` 并重新读取。
    /// Covers: an imported SoundBug project can be saved as `.sfsd` and read back.
    #[test]
    fn imported_example_round_trips_as_sfsd() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join("光年外的流星_编曲工程")
            .join("光年外的流星.sndp");
        if !path.exists() {
            eprintln!("示例工程不存在，跳过: {}", path.display());
            return;
        }
        let imported = import_project(&path).expect("import SoundBug example");
        let temp = std::env::temp_dir().join(format!(
            "surfingstudio_soundbug_roundtrip_{}.sfsd",
            std::process::id()
        ));
        let runtime = crate::core::project_runtime::ProjectRuntime::standard();
        runtime
            .save(&temp, &imported.project, &[])
            .expect("write imported project");
        let loaded = runtime.open(&temp).expect("read imported project");
        let _ = std::fs::remove_file(&temp);
        assert_eq!(loaded.project.tracks.len(), imported.project.tracks.len());
        assert!((loaded.project.bpm - imported.project.bpm).abs() < 1e-9);
        let loaded_offset = loaded
            .project
            .tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .filter_map(|clip| clip.audio.as_ref())
            .map(|source| source.offset_seconds)
            .fold(0.0_f64, f64::max);
        assert!(loaded_offset > 1.0, "offset lost after round trip");
    }
}
