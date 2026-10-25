//! 所属模块：core / 工程运行时框架
//! 文件用途：
//! - 提供类似 JVM 的工程运行时：应用层只调用框架方法，不直接依赖任何工程格式。
//! - 框架维护格式后端（`ProjectCodec`）注册表，负责打开、保存与格式间转译。
//! - `sfsd` / `soundbug` 作为后端仅实现“外部字节 ↔ 规范工程表示”的转译。
//!
//! 架构：
//! ```text
//! app（应用层）
//!   └─ ProjectRuntime（框架：加载 / 保存 / 转译 / 会话）
//!        ├─ SfsdCodec     → 读写 `.sfsd`（原生格式）
//!        └─ SoundBugCodec → 只读转译 `.sndp` / `comp.dat`
//! ```
//!
//! 函数清单：
//! - `ProjectRuntime::new` / `standard` / `Default::default`：运行时构造与默认后端注册。
//! - `ProjectRuntime::formats` / `codec_for` / `writer_for`：后端查询。
//! - `ProjectRuntime::open` / `save` / `translate`：框架核心方法。
//! - `ProjectRuntime::open_project` / `save_project` / `translate_project`：进程级默认运行时入口。
//!
//! Module: core / project runtime framework
//! Purpose:
//! - Provide a JVM-like project runtime: the app only calls framework methods and
//!   never depends on a concrete project format.
//! - Maintain a registry of format backends (`ProjectCodec`) and handle open, save
//!   and cross-format translation.
//! - `sfsd` / `soundbug` are backends that only translate external bytes to/from
//!   the canonical project representation.
//!
//! Architecture:
//! ```text
//! app (application layer)
//!   └─ ProjectRuntime (framework: open / save / translate / session)
//!        ├─ SfsdCodec     → read and write `.sfsd` (native format)
//!        └─ SoundBugCodec → read-only translation of `.sndp` / `comp.dat`
//! ```
//!
//! Functions:
//! - `ProjectRuntime::new` / `standard` / `Default::default`: runtime construction and
//!   default backend registration.
//! - `ProjectRuntime::formats` / `codec_for` / `writer_for`: backend queries.
//! - `ProjectRuntime::open` / `save` / `translate`: core framework methods.
//! - `ProjectRuntime::open_project` / `save_project` / `translate_project`: process-wide
//!   default runtime entry points.

mod codec;
mod session;
mod sfsd;
mod soundbug;

pub use codec::{CodecInfo, ProjectCodec};
pub use session::{ProjectSession, ProjectSource, WaveformPeak};
pub use sfsd::SfsdCodec;
pub use soundbug::SoundBugCodec;

use crate::core::model::Project;
use std::path::Path;
use std::sync::OnceLock;

/// 工程运行时框架：持有格式后端注册表并暴露统一操作。
/// Project runtime framework: owns the format backend registry and exposes unified
/// operations.
pub struct ProjectRuntime {
    codecs: Vec<Box<dyn ProjectCodec>>,
}

impl ProjectRuntime {
    /// 所属模块：core / 工程运行时 / 构造
    /// 功能：使用给定的后端创建运行时。
    /// 输入：`codecs` 格式后端列表，顺序决定未知扩展名的探测顺序。
    ///
    /// 输出/副作用：返回运行时；不修改外部状态。
    ///
    /// 复杂度：时间 O(后端数量)，空间 O(后端数量)。
    ///
    /// Module: core / project runtime / construction
    /// Purpose: Create a runtime from the given backends.
    /// Inputs: `codecs` — format backends; the order is the probe order for unknown
    /// extensions.
    ///
    /// Outputs / side effects: returns the runtime; no external state change.
    ///
    /// Complexity: O(backend count) time and space.
    pub fn new(codecs: Vec<Box<dyn ProjectCodec>>) -> Self {
        Self { codecs }
    }

    /// 所属模块：core / 工程运行时 / 构造
    /// 功能：创建标准运行时：注册 `SfsdCodec` 与 `SoundBugCodec`。
    /// 输入：无。
    ///
    /// 输出/副作用：返回标准运行时。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / construction
    /// Purpose: Create the standard runtime with `SfsdCodec` and `SoundBugCodec`.
    /// Inputs: none.
    ///
    /// Outputs / side effects: returns the standard runtime.
    ///
    /// Complexity: O(1) time and space.
    pub fn standard() -> Self {
        Self::new(vec![Box::new(SfsdCodec), Box::new(SoundBugCodec)])
    }

    /// 所属模块：core / 工程运行时 / 查询
    /// 功能：返回所有已注册后端的能力描述。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回能力列表；不修改状态。
    ///
    /// 复杂度：时间 O(后端数量)，空间 O(后端数量)。
    ///
    /// Module: core / project runtime / query
    /// Purpose: Return capability descriptors of all registered backends.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the capability list; no state change.
    ///
    /// Complexity: O(backend count) time and space.
    pub fn formats(&self) -> Vec<CodecInfo> {
        self.codecs.iter().map(|codec| codec.info()).collect()
    }

    /// 所属模块：core / 工程运行时 / 查询
    /// 功能：按扩展名查找可读后端。
    /// 输入：`path` 工程文件路径。
    ///
    /// 输出/副作用：返回后端引用；无匹配时为 `None`。
    ///
    /// 复杂度：时间 O(后端数量)，空间 O(1)。
    ///
    /// Module: core / project runtime / query
    /// Purpose: Find a readable backend by extension.
    /// Inputs: `path` — project file path.
    ///
    /// Outputs / side effects: returns a backend reference, or `None` when unmatched.
    ///
    /// Complexity: O(backend count) time and O(1) space.
    fn codec_for(&self, path: &Path) -> Option<&dyn ProjectCodec> {
        self.codecs
            .iter()
            .find(|codec| codec.info().can_read && codec.matches(path))
            .map(|codec| codec.as_ref())
    }

    /// 所属模块：core / 工程运行时 / 查询
    /// 功能：按扩展名查找可写后端。
    /// 输入：`path` 目标文件路径。
    ///
    /// 输出/副作用：返回后端引用；无匹配时为 `None`。
    ///
    /// 复杂度：时间 O(后端数量)，空间 O(1)。
    ///
    /// Module: core / project runtime / query
    /// Purpose: Find a writable backend by extension.
    /// Inputs: `path` — destination path.
    ///
    /// Outputs / side effects: returns a backend reference, or `None` when unmatched.
    ///
    /// Complexity: O(backend count) time and O(1) space.
    fn writer_for(&self, path: &Path) -> Option<&dyn ProjectCodec> {
        self.codecs
            .iter()
            .find(|codec| codec.info().can_write && codec.matches(path))
            .map(|codec| codec.as_ref())
    }

    /// 所属模块：core / 工程运行时 / 加载
    /// 功能：打开工程：优先按扩展名分派，未知扩展名时依次尝试全部可读后端。
    /// 输入：`path` 工程文件路径。
    ///
    /// 输出/副作用：返回工程会话；全部后端失败时返回汇总错误。
    ///
    /// 复杂度：时间 O(文件大小)（由后端决定），空间 O(工程大小)。
    ///
    /// Module: core / project runtime / loading
    /// Purpose: Open a project: dispatch by extension, or probe every readable backend
    /// when the extension is unknown.
    /// Inputs: `path` — project file path.
    ///
    /// Outputs / side effects: returns the project session; when every backend fails,
    /// returns an aggregated error.
    ///
    /// Complexity: O(file size) time (backend-specific) and O(project size) space.
    pub fn open(&self, path: &Path) -> Result<ProjectSession, String> {
        if let Some(codec) = self.codec_for(path) {
            return codec
                .decode(path)
                .map_err(|error| format!("{} 加载失败: {error}", codec.info().label));
        }

        // 扩展名未知时按注册顺序探测，兼容被改名的工程文件。
        // Probe in registration order when the extension is unknown, so renamed
        // project files can still be opened.
        let mut errors = Vec::new();
        for codec in self.codecs.iter().filter(|codec| codec.info().can_read) {
            match codec.decode(path) {
                Ok(session) => return Ok(session),
                Err(error) => errors.push(format!("{}: {error}", codec.info().label)),
            }
        }
        let supported = self
            .codecs
            .iter()
            .flat_map(|codec| codec.info().extensions.iter())
            .map(|extension| format!(".{extension}"))
            .collect::<Vec<_>>()
            .join(" / ");
        Err(format!(
            "无法识别工程格式 {}（支持 {}）: {}",
            path.display(),
            supported,
            errors.join("；")
        ))
    }

    /// 所属模块：core / 工程运行时 / 保存
    /// 功能：把规范工程保存为目标扩展名对应的格式。
    /// 输入：
    /// - `path`：目标文件路径。
    /// - `project`：规范工程表示。
    /// - `peaks`：波形峰值。
    ///
    /// 输出/副作用：写入目标文件；无可用后端时返回错误。
    ///
    /// 复杂度：时间 O(工程大小)（由后端决定），空间 O(压缩后大小)。
    ///
    /// Module: core / project runtime / saving
    /// Purpose: Save the canonical project to the format matching the target extension.
    /// Inputs:
    /// - `path`: destination path.
    /// - `project`: canonical project representation.
    /// - `peaks`: waveform peaks.
    ///
    /// Outputs / side effects: writes the destination; errors when no backend can write.
    ///
    /// Complexity: O(project size) time (backend-specific) and O(compressed size) space.
    pub fn save(
        &self,
        path: &Path,
        project: &Project,
        peaks: &[WaveformPeak],
    ) -> Result<(), String> {
        let Some(codec) = self.writer_for(path) else {
            let writable = self
                .formats()
                .into_iter()
                .filter(|info| info.can_write)
                .flat_map(|info| {
                    info.extensions
                        .iter()
                        .map(|extension| format!(".{extension}"))
                })
                .collect::<Vec<_>>()
                .join(" / ");
            return Err(format!(
                "不支持保存为 {}（可写格式：{}）",
                path.display(),
                writable
            ));
        };
        codec
            .encode(path, project, peaks)
            .map_err(|error| format!("{} 保存失败: {error}", codec.info().label))
    }

    /// 所属模块：core / 工程运行时 / 转译
    /// 功能：把任意受支持格式转译为另一种可写格式（解码 → 规范工程 → 编码）。
    /// 输入：
    /// - `source`：源工程路径。
    /// - `target`：目标工程路径，扩展名决定目标后端。
    ///
    /// 输出/副作用：写入目标文件；不修改源文件。
    ///
    /// 复杂度：时间 O(源文件大小 + 目标工程大小)，空间 O(工程大小)。
    ///
    /// Module: core / project runtime / translation
    /// Purpose: Translate any supported format into another writable format
    /// (decode → canonical project → encode).
    /// Inputs:
    /// - `source`: source project path.
    /// - `target`: destination project path; its extension selects the backend.
    ///
    /// Outputs / side effects: writes the destination; the source is untouched.
    ///
    /// Complexity: O(source file size + target project size) time and O(project size) space.
    pub fn translate(&self, source: &Path, target: &Path) -> Result<(), String> {
        let session = self.open(source)?;
        self.save(target, &session.project, &session.peaks)
    }

    /// 所属模块：core / 工程运行时 / 进程级入口
    /// 功能：使用进程级默认运行时打开工程。
    /// 输入：`path` 工程文件路径。
    ///
    /// 输出/副作用：返回工程会话；不修改文件。
    ///
    /// 复杂度：时间 O(文件大小)，空间 O(工程大小)。
    ///
    /// Module: core / project runtime / process-wide entry
    /// Purpose: Open a project through the process-wide default runtime.
    /// Inputs: `path` — project file path.
    ///
    /// Outputs / side effects: returns the project session; files are untouched.
    ///
    /// Complexity: O(file size) time and O(project size) space.
    pub fn open_project(path: &Path) -> Result<ProjectSession, String> {
        standard_runtime().open(path)
    }

    /// 所属模块：core / 工程运行时 / 进程级入口
    /// 功能：使用进程级默认运行时保存工程。
    /// 输入：
    /// - `path`：目标文件路径。
    /// - `project`：规范工程表示。
    /// - `peaks`：波形峰值。
    ///
    /// 输出/副作用：写入目标文件。
    ///
    /// 复杂度：时间 O(工程大小)，空间 O(压缩后大小)。
    ///
    /// Module: core / project runtime / process-wide entry
    /// Purpose: Save a project through the process-wide default runtime.
    /// Inputs:
    /// - `path`: destination path.
    /// - `project`: canonical project representation.
    /// - `peaks`: waveform peaks.
    ///
    /// Outputs / side effects: writes the destination.
    ///
    /// Complexity: O(project size) time and O(compressed size) space.
    pub fn save_project(
        path: &Path,
        project: &Project,
        peaks: &[WaveformPeak],
    ) -> Result<(), String> {
        standard_runtime().save(path, project, peaks)
    }

    /// 所属模块：core / 工程运行时 / 进程级入口
    /// 功能：使用进程级默认运行时执行工程格式转译。
    /// 输入：
    /// - `source`：源工程路径。
    /// - `target`：目标工程路径。
    ///
    /// 输出/副作用：写入目标文件；不修改源文件。
    ///
    /// 复杂度：时间 O(源文件大小 + 目标工程大小)，空间 O(工程大小)。
    ///
    /// Module: core / project runtime / process-wide entry
    /// Purpose: Translate a project format through the process-wide default runtime.
    /// Inputs:
    /// - `source`: source project path.
    /// - `target`: destination project path.
    ///
    /// Outputs / side effects: writes the destination; the source is untouched.
    ///
    /// Complexity: O(source file size + target project size) time and O(project size) space.
    pub fn translate_project(source: &Path, target: &Path) -> Result<(), String> {
        standard_runtime().translate(source, target)
    }
}

impl Default for ProjectRuntime {
    /// 所属模块：core / 工程运行时 / 构造
    /// 功能：创建标准运行时。
    /// 输入：无。
    ///
    /// 输出/副作用：返回标准运行时。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / project runtime / construction
    /// Purpose: Create the standard runtime.
    /// Inputs: none.
    ///
    /// Outputs / side effects: returns the standard runtime.
    ///
    /// Complexity: O(1) time and space.
    fn default() -> Self {
        Self::standard()
    }
}

/// 所属模块：core / 工程运行时 / 进程级入口
/// 功能：返回进程级默认运行时，首次调用时注册标准后端。
/// 输入：无。
///
/// 输出/副作用：返回静态运行时引用；只初始化一次。
///
/// 复杂度：时间 O(1)（首次 O(1)），空间 O(1)。
///
/// Module: core / project runtime / process-wide entry
/// Purpose: Return the process-wide default runtime, registering standard backends on
/// first use.
/// Inputs: none.
///
/// Outputs / side effects: returns a static runtime reference; initialized once.
///
/// Complexity: O(1) time (O(1) on first use) and O(1) space.
fn standard_runtime() -> &'static ProjectRuntime {
    static RUNTIME: OnceLock<ProjectRuntime> = OnceLock::new();
    RUNTIME.get_or_init(ProjectRuntime::standard)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：标准运行时能按扩展名把 `.sfsd` / `.sndp` 分派给对应后端。
    /// Covers: the standard runtime dispatches `.sfsd` / `.sndp` to matching backends.
    #[test]
    fn standard_runtime_dispatches_known_extensions() {
        let runtime = ProjectRuntime::standard();
        assert_eq!(
            runtime
                .codec_for(Path::new("a.SFSD"))
                .map(|codec| codec.info().id),
            Some("sfsd")
        );
        assert_eq!(
            runtime
                .codec_for(Path::new("b.SnDp"))
                .map(|codec| codec.info().id),
            Some("soundbug")
        );
        assert!(runtime.codec_for(Path::new("c.wav")).is_none());
    }

    /// 覆盖：未知扩展名且文件不存在时返回汇总错误，而不是 panic。
    /// Covers: an unknown extension with a missing file returns an aggregated error.
    #[test]
    fn unknown_extension_reports_supported_formats() {
        let runtime = ProjectRuntime::standard();
        let path = std::env::temp_dir().join("surfing_unknown_format.bin");
        let error = runtime.open(&path).err().expect("must fail");
        assert!(error.contains(".sfsd"), "error={error}");
        assert!(error.contains(".sndp"), "error={error}");
    }

    /// 覆盖：框架保存后可重新打开原生工程。
    /// Covers: a native project saved through the framework can be opened again.
    #[test]
    fn runtime_saves_and_opens_native_projects() {
        let project = Project::demo();
        let path =
            std::env::temp_dir().join(format!("surfing_runtime_{}.sfsd", std::process::id()));
        ProjectRuntime::save_project(&path, &project, &[]).expect("save");
        let session = ProjectRuntime::open_project(&path).expect("open");
        let _ = std::fs::remove_file(&path);
        assert_eq!(session.source, ProjectSource::Native);
        assert!((session.project.bpm - project.bpm).abs() < 1e-9);
    }

    /// 覆盖：生成的范例工程文件可被框架重新打开并包含音符。
    /// Covers: generated example-project files reopen through the framework and contain notes.
    #[test]
    fn generated_example_projects_open_with_notes() {
        use crate::core::model::template::example_projects;
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
        for example in example_projects() {
            let path = directory.join(format!("{}.sfsd", example.file_name));
            if !path.exists() {
                continue;
            }
            let session = ProjectRuntime::open_project(&path).expect("open example");
            let has_notes = session
                .project
                .tracks
                .iter()
                .flat_map(|track| track.clips.iter())
                .any(|clip| !clip.notes.is_empty());
            assert!(has_notes, "{} has no notes", example.name);
        }
    }

    /// 覆盖：按需把内置范例工程写入 `examples/`（生成器，普通测试跳过）。
    /// Covers: optionally write built-in example projects into `examples/` (generator, skipped by default).
    #[test]
    #[ignore = "生成 examples/ 下的范例工程文件；需要手动运行"]
    fn generate_example_projects() {
        use crate::core::model::template::{build_example_project, example_projects};
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
        std::fs::create_dir_all(&directory).expect("create examples directory");
        for example in example_projects() {
            let project = build_example_project(example).expect("example template");
            let path = directory.join(format!("{}.sfsd", example.file_name));
            ProjectRuntime::save_project(&path, &project, &[]).expect("save example project");
            println!("wrote {}", path.display());
        }
    }
}
