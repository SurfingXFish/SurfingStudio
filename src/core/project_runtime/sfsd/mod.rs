//! 所属模块：core / 工程运行时 / SurfingStudio 后端
//! 文件用途：
//! - 实现 `.sfsd` 格式后端 `SfsdCodec`：读取时把 SFS2 容器转译为规范工程会话。
//! - 保存时把规范工程编码为 `.sfsd` 容器。
//! - 编排容器校验、清单反序列化、插件状态块与波形峰值恢复。
//!
//! 函数清单：
//! - `SfsdCodec::info` / `decode` / `encode`：后端转译接口实现。
//! - `read_sfsd`：读取并校验 SFS2 工程。
//! - `write_project`：原子写入 `.sfsd` 工程。
//! - `write_container`：写入全部数据块与清单。
//!
//! Module: core / project runtime / SurfingStudio backend
//! Purpose:
//! - Implement the `.sfsd` backend `SfsdCodec`: translate SFS2 containers into
//!   canonical project sessions when reading.
//! - Encode the canonical project into an `.sfsd` container when saving.
//! - Orchestrate container validation, manifest deserialization, plugin state chunks
//!   and waveform peak restoration.
//!
//! Functions:
//! - `SfsdCodec::info` / `decode` / `encode`: backend translation interface.
//! - `read_sfsd`: read and validate an SFS2 project.
//! - `write_project`: atomically write an `.sfsd` project.
//! - `write_container`: write every data chunk and the manifest.

mod container;
mod manifest;

use self::container::{
    ContainerReader, ContainerWriter, CODEC_RAW, CODEC_ZSTD, TAG_META, TAG_PEAK, TAG_PLUG,
};
use self::manifest::{plugin_refs, plugin_refs_mut, Manifest, ManifestRef, PeakTable};
use super::{CodecInfo, ProjectCodec, ProjectSession, ProjectSource, WaveformPeak};
use crate::core::model::{Project, CURRENT_PROJECT_VERSION};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// SurfingStudio `.sfsd` 格式后端。
/// SurfingStudio `.sfsd` format backend.
pub struct SfsdCodec;

impl ProjectCodec for SfsdCodec {
    /// 所属模块：core / 工程运行时 / SurfingStudio 后端
    /// 功能：返回 `.sfsd` 后端能力：支持读写。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回能力描述；不修改状态。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / SurfingStudio backend
    /// Purpose: Return the `.sfsd` capabilities: readable and writable.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the descriptor; no state change.
    ///
    /// Complexity: O(1) time and space.
    fn info(&self) -> CodecInfo {
        CodecInfo {
            id: "sfsd",
            label: "SurfingStudio 工程",
            extensions: &["sfsd"],
            can_read: true,
            can_write: true,
        }
    }

    /// 所属模块：core / 工程运行时 / SurfingStudio 后端
    /// 功能：读取 `.sfsd` 并转译为规范工程会话。
    /// 输入：`path` 工程文件路径。
    ///
    /// 输出/副作用：返回工程会话；不修改文件。
    ///
    /// 复杂度：时间 O(文件大小)，空间 O(工程大小 + 峰值大小)。
    ///
    /// Module: core / project runtime / SurfingStudio backend
    /// Purpose: Read `.sfsd` and translate it into a canonical project session.
    /// Inputs: `path` — project file path.
    ///
    /// Outputs / side effects: returns the session; files are untouched.
    ///
    /// Complexity: O(file size) time and O(project size + peak size) space.
    fn decode(&self, path: &Path) -> Result<ProjectSession, String> {
        let (project, peaks) = read_sfsd(path)?;
        Ok(ProjectSession::new(
            path.to_path_buf(),
            project,
            peaks,
            Vec::new(),
            ProjectSource::Native,
        ))
    }

    /// 所属模块：core / 工程运行时 / SurfingStudio 后端
    /// 功能：把规范工程编码为 `.sfsd` 容器。
    /// 输入：
    /// - `path`：目标文件路径。
    /// - `project`：规范工程表示。
    /// - `peaks`：波形峰值。
    ///
    /// 输出/副作用：写入目标文件；失败时清理临时文件。
    ///
    /// 复杂度：时间 O(工程大小 + 峰值大小)，空间 O(压缩后大小)。
    ///
    /// Module: core / project runtime / SurfingStudio backend
    /// Purpose: Encode the canonical project into an `.sfsd` container.
    /// Inputs:
    /// - `path`: destination path.
    /// - `project`: canonical project representation.
    /// - `peaks`: waveform peaks.
    ///
    /// Outputs / side effects: writes the destination; removes the temp file on failure.
    ///
    /// Complexity: O(project size + peak size) time and O(compressed size) space.
    fn encode(&self, path: &Path, project: &Project, peaks: &[WaveformPeak]) -> Result<(), String> {
        write_project(path, project, peaks)
    }
}

/// 所属模块：core / 工程运行时 / SurfingStudio 后端
/// 功能：读取并校验 SFS2 工程容器，恢复插件状态与波形峰值。
/// 输入：`path` 工程文件路径。
///
/// 输出/副作用：返回工程与峰值；不修改文件。
///
/// 复杂度：时间 O(文件大小)，空间 O(工程大小 + 峰值大小)。
///
/// Module: core / project runtime / SurfingStudio backend
/// Purpose: Read and validate an SFS2 container, restoring plugin state and peaks.
/// Inputs: `path` — project file path.
///
/// Outputs / side effects: returns the project and peaks; files are untouched.
///
/// Complexity: O(file size) time and O(project size + peak size) space.
fn read_sfsd(path: &Path) -> Result<(Project, Vec<WaveformPeak>), String> {
    let mut reader = ContainerReader::open(path)?;

    let meta_entry = reader
        .find(TAG_META)
        .ok_or_else(|| "工程缺少数据块".to_string())?;
    let meta_bytes = reader.read_chunk(&meta_entry)?;
    let manifest: Manifest =
        rmp_serde::from_slice(&meta_bytes).map_err(|error| format!("工程数据解析失败: {error}"))?;
    let mut project = manifest.project;
    if project.version != CURRENT_PROJECT_VERSION {
        return Err(format!(
            "工程版本 {} 与当前版本 {} 不一致",
            project.version, CURRENT_PROJECT_VERSION
        ));
    }

    let mut refs = plugin_refs_mut(&mut project);
    if refs.len() != manifest.plugin_state_chunks.len() {
        return Err("插件状态数量与工程不匹配".into());
    }
    for (pref, chunk_index) in refs.iter_mut().zip(manifest.plugin_state_chunks.iter()) {
        let entry = reader
            .entry(*chunk_index as usize)
            .ok_or_else(|| "插件状态块索引无效".to_string())?;
        if entry.tag != TAG_PLUG {
            return Err("插件状态块类型错误".into());
        }
        let bytes = reader.read_chunk(&entry)?;
        pref.state = Arc::from(bytes);
    }
    drop(refs);

    let peaks = match reader.find(TAG_PEAK) {
        Some(entry) => {
            let bytes = reader.read_chunk(&entry)?;
            let table: PeakTable = rmp_serde::from_slice(&bytes)
                .map_err(|error| format!("波形数据解析失败: {error}"))?;
            table.peaks
        }
        None => Vec::new(),
    };

    Ok((project, peaks))
}

/// 所属模块：core / 工程运行时 / SurfingStudio 后端 / 写入
/// 功能：原子写入 `.sfsd` 工程（先写 `.tmp` 再替换）。
/// 输入：
/// - `path`：目标文件路径。
/// - `project`：要保存的工程。
/// - `peaks`：当前 UI 已知的波形峰值；未被工程引用的条目会被忽略。
///
/// 输出/副作用：写入或替换目标文件；失败时清理临时文件。
///
/// 复杂度：时间 O(工程大小 + 峰值大小)，空间 O(压缩后大小)。
///
/// Module: core / project runtime / SurfingStudio backend / writer
/// Purpose: Atomically write an `.sfsd` project (temp file + rename).
/// Inputs:
/// - `path`: destination path.
/// - `project`: project to save.
/// - `peaks`: waveform peaks known by the UI; unreferenced entries are ignored.
///
/// Outputs / side effects: writes or replaces the destination; removes the temp file
/// on failure.
///
/// Complexity: O(project size + peak size) time and O(compressed size) space.
fn write_project(path: &Path, project: &Project, peaks: &[WaveformPeak]) -> Result<(), String> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);

    let result = write_container(&temp, project, peaks);
    match result {
        Ok(()) => std::fs::rename(&temp, path).map_err(|error| {
            let _ = std::fs::remove_file(&temp);
            format!("无法替换工程文件: {error}")
        }),
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            Err(error)
        }
    }
}

/// 所属模块：core / 工程运行时 / SurfingStudio 后端 / 写入
/// 功能：写入全部数据块、清单与波形峰值表。
/// 输入：
/// - `path`：临时文件路径。
/// - `project`：要保存的工程。
/// - `peaks`：波形峰值候选。
///
/// 输出/副作用：写入文件；返回失败原因。
///
/// 复杂度：时间 O(工程大小 + 峰值大小)，空间 O(压缩后大小)。
///
/// Module: core / project runtime / SurfingStudio backend / writer
/// Purpose: Write every data chunk, the manifest and the peak table.
/// Inputs:
/// - `path`: temp file path.
/// - `project`: project to save.
/// - `peaks`: peak candidates.
///
/// Outputs / side effects: writes the file; failures are returned.
///
/// Complexity: O(project size + peak size) time and O(compressed size) space.
fn write_container(path: &Path, project: &Project, peaks: &[WaveformPeak]) -> Result<(), String> {
    let mut writer = ContainerWriter::create(path)?;

    // 插件状态优先写入：体积最大，且不需要编码工作。
    // Plugin state first: its size dominates and it needs no encoding work.
    let mut plugin_state_chunks = Vec::new();
    for pref in plugin_refs(project) {
        let index = writer.write_chunk(TAG_PLUG, CODEC_RAW, &pref.state)?;
        plugin_state_chunks.push(index);
    }

    let manifest = ManifestRef {
        project,
        plugin_state_chunks,
    };
    let meta = rmp_serde::to_vec(&manifest).map_err(|e| format!("工程序列化失败: {e}"))?;
    writer.write_chunk(TAG_META, CODEC_ZSTD, &meta)?;

    let referenced: HashSet<&Path> = project
        .tracks
        .iter()
        .flat_map(|track| track.clips.iter())
        .filter_map(|clip| clip.audio.as_ref())
        .map(|source| source.path.as_path())
        .collect();
    let known: Vec<WaveformPeak> = peaks
        .iter()
        .filter(|entry| referenced.contains(entry.path.as_path()) && !entry.bins.is_empty())
        .cloned()
        .collect();
    if !known.is_empty() {
        let table = rmp_serde::to_vec(&PeakTable { peaks: known })
            .map_err(|e| format!("波形数据序列化失败: {e}"))?;
        writer.write_chunk(TAG_PEAK, CODEC_ZSTD, &table)?;
    }

    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{
        AudioSource, Clip, DrumSettings, EffectRef, InstrumentRef, PluginFormat, PluginRef,
        Project, Track,
    };

    /// 所属模块：core / 工程运行时 / SurfingStudio 后端 / 测试
    /// 功能：生成带进程号与名称的临时工程路径。
    /// 输入：`name` 测试名称。
    ///
    /// 输出/副作用：返回临时路径；不创建文件。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / SurfingStudio backend / tests
    /// Purpose: Build a temp project path from the process id and test name.
    /// Inputs: `name` — test name.
    ///
    /// Outputs / side effects: returns the temp path; no file is created.
    ///
    /// Complexity: O(1) time and space.
    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("surfing_{name}_{}.sfsd", std::process::id()))
    }

    /// 覆盖：工程清单、插件状态块与波形峰值可完整往返。
    /// Covers: the project manifest, plugin state blobs and waveform peaks round-trip.
    #[test]
    fn round_trips_project_plugin_state_and_peaks() {
        let mut project = Project::demo();
        project.version = CURRENT_PROJECT_VERSION;
        project.master_effect = Some(EffectRef::Plugin(PluginRef {
            format: PluginFormat::Vst3,
            path: PathBuf::from("C:/plugins/test.vst3"),
            name: "Test".into(),
            uid: "abc".into(),
            state: Arc::from(vec![1u8, 2, 3, 250, 251, 252]),
            restore_state: true,
            params: vec![0.25, 0.75],
            enabled: true,
        }));
        project.tracks.push(Track::new(
            99,
            "Plugin Track",
            [1, 2, 3],
            InstrumentRef::Plugin(PluginRef {
                format: PluginFormat::Vst2,
                path: PathBuf::from("C:/plugins/inst.dll"),
                name: "Inst".into(),
                uid: "def".into(),
                state: Arc::from(Vec::new()),
                restore_state: true,
                params: vec![0.5],
                enabled: true,
            }),
        ));
        project.tracks.push(Track::new(
            100,
            "Drum Track",
            [4, 5, 6],
            InstrumentRef::Drums(DrumSettings::default()),
        ));
        let mut audio_clip = Clip::new(7, "Audio", 0.0, 4.0);
        audio_clip.audio = Some(AudioSource {
            path: PathBuf::from("C:/audio/loop.wav"),
            name: "loop".into(),
            length_seconds: 2.0,
            offset_seconds: 0.0,
        });
        project
            .tracks
            .last_mut()
            .expect("drum track")
            .clips
            .push(audio_clip);

        let peaks = vec![
            WaveformPeak {
                path: PathBuf::from("C:/audio/loop.wav"),
                bins: vec![0.1, 0.4, 0.9],
            },
            WaveformPeak {
                path: PathBuf::from("C:/audio/unused.wav"),
                bins: vec![1.0],
            },
        ];
        let path = temp_path("roundtrip");
        write_project(&path, &project, &peaks).expect("write");
        let (loaded, loaded_peaks) = read_sfsd(&path).expect("read");
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.tracks.len(), project.tracks.len());
        assert_eq!(loaded.bpm, project.bpm);
        let master = loaded
            .master_effect
            .expect("master effect")
            .plugin()
            .cloned()
            .expect("plugin effect");
        assert_eq!(master.state.as_ref(), &[1u8, 2, 3, 250, 251, 252]);
        assert_eq!(master.params, vec![0.25, 0.75]);
        let instrument = loaded.tracks[project.tracks.len() - 2].instrument.clone();
        assert!(matches!(instrument, InstrumentRef::Plugin(_)));
        assert_eq!(loaded_peaks.len(), 1, "only referenced peaks are kept");
        assert_eq!(loaded_peaks[0].bins, vec![0.1, 0.4, 0.9]);
    }

    /// 覆盖：损坏的容器必须被拒绝。
    /// Covers: corrupted containers must be rejected.
    #[test]
    fn rejects_corrupted_containers() {
        let project = Project::demo();
        let path = temp_path("corrupt");
        write_project(&path, &project, &[]).expect("write");
        let mut bytes = std::fs::read(&path).expect("read bytes");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(&path, &bytes).expect("write corrupted");
        assert!(read_sfsd(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
