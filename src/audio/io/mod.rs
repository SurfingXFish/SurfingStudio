//! 所属模块：audio / 音频 I/O 运行时框架
//! 文件用途：
//! - 提供类似 JVM 的音频 I/O 运行时：应用层只调用 `AudioRuntime`。
//! - 维护音频格式后端（`AudioCodec`）注册表，负责导入、时长查询与导出。
//! - WAV 作为后端实现具体编解码，其他格式可按同一接口扩展。
//!
//! 函数清单：
//! - `AudioRuntime::new` / `standard` / `Default::default`：运行时构造与后端注册。
//! - `AudioRuntime::formats` / `codec_for` / `writer_for`：后端查询。
//! - `AudioRuntime::import` / `duration_seconds` / `export`：框架核心方法。
//! - `AudioRuntime::import_audio` / `audio_duration_seconds` / `export_audio` / `export_project` /
//!   `export_project_with_gpu`：进程级入口。
//!
//! Module: audio / audio I/O runtime framework
//! Purpose:
//! - Provide a JVM-like audio I/O runtime: the app only calls `AudioRuntime`.
//! - Maintain a registry of audio format backends (`AudioCodec`) for import, duration
//!   queries and export.
//! - WAV is one backend; other formats can follow the same interface.
//!
//! Functions:
//! - `AudioRuntime::new` / `standard` / `Default::default`: runtime construction and
//!   backend registration.
//! - `AudioRuntime::formats` / `codec_for` / `writer_for`: backend queries.
//! - `AudioRuntime::import` / `duration_seconds` / `export`: core framework methods.
//! - `AudioRuntime::import_audio` / `audio_duration_seconds` / `export_audio` /
//!   `export_project` / `export_project_with_gpu`: process-wide entries.

pub mod wav;

pub use wav::WavCodec;

use crate::audio::gpu::GpuRenderConfig;
use crate::core::model::Project;
use std::path::Path;
use std::sync::OnceLock;

/// 解码后的音频：左右声道与波形峰值。
/// Decoded audio: left/right channels and waveform peaks.
#[derive(Debug)]
pub struct AudioFile {
    pub sample_rate: f64,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    /// 编曲区预览使用的粗略绝对峰值包络。
    /// Coarse absolute-peak envelope used by the arrangement preview.
    pub peaks: Vec<f32>,
}

/// 音频后端能力描述。
/// Capability descriptor of an audio backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioCodecInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub extensions: &'static [&'static str],
    pub can_read: bool,
    pub can_write: bool,
}

impl AudioCodecInfo {
    /// 所属模块：audio / 音频 I/O 运行时 / 查询
    /// 功能：按扩展名判断该后端是否负责此路径（大小写不敏感）。
    /// 输入：`path` 音频文件路径。
    ///
    /// 输出/副作用：返回布尔值；不修改状态。
    ///
    /// 复杂度：时间 O(扩展名数量)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / query
    /// Purpose: Check whether the backend handles the path by extension (case-insensitive).
    /// Inputs: `path` — audio file path.
    ///
    /// Outputs / side effects: returns a boolean; no state change.
    ///
    /// Complexity: O(extension count) time and O(1) space.
    pub fn supports(&self, path: &Path) -> bool {
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            return false;
        };
        self.extensions
            .iter()
            .any(|supported| extension.eq_ignore_ascii_case(supported))
    }
}

/// 音频格式后端：把外部音频字节转译为内存音频，或把内存音频编码为文件。
/// Audio format backend: translate external audio bytes into memory, or encode back.
pub trait AudioCodec: Send + Sync {
    /// 所属模块：audio / 音频 I/O 运行时 / 后端接口
    /// 功能：返回后端能力描述。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回能力描述；不修改状态。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / backend interface
    /// Purpose: Return the backend capability descriptor.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the descriptor; no state change.
    ///
    /// Complexity: O(1) time and space.
    fn info(&self) -> AudioCodecInfo;

    /// 所属模块：audio / 音频 I/O 运行时 / 后端接口
    /// 功能：按扩展名判断该后端是否负责此路径。
    /// 输入：`path` 音频文件路径。
    ///
    /// 输出/副作用：返回布尔值；不修改状态。
    ///
    /// 复杂度：时间 O(扩展名数量)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / backend interface
    /// Purpose: Check whether the backend handles the path by extension.
    /// Inputs: `path` — audio file path.
    ///
    /// Outputs / side effects: returns a boolean; no state change.
    ///
    /// Complexity: O(extension count) time and O(1) space.
    fn matches(&self, path: &Path) -> bool {
        self.info().supports(path)
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 后端接口
    /// 功能：解码音频文件为内存音频。
    /// 输入：`path` 音频文件路径。
    ///
    /// 输出/副作用：返回解码结果；只读文件。
    ///
    /// 复杂度：时间 O(文件采样数)，空间 O(采样数)。
    ///
    /// Module: audio / audio I/O runtime / backend interface
    /// Purpose: Decode an audio file into memory.
    /// Inputs: `path` — audio file path.
    ///
    /// Outputs / side effects: returns the decoded audio; the file is read-only.
    ///
    /// Complexity: O(sample count) time and space.
    fn decode(&self, path: &Path) -> Result<AudioFile, String>;

    /// 所属模块：audio / audio I/O runtime / backend interface
    /// 功能：只读文件头计算时长。
    /// 输入：`path` 音频文件路径。
    ///
    /// 输出/副作用：返回秒数；只读文件头。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / backend interface
    /// Purpose: Compute the duration from the file header only.
    /// Inputs: `path` — audio file path.
    ///
    /// Outputs / side effects: returns seconds; only the header is read.
    ///
    /// Complexity: O(1) time and space.
    fn duration_seconds(&self, path: &Path) -> Result<f64, String>;

    /// 所属模块：audio / audio I/O 运行时 / 后端接口
    /// 功能：把交错立体声编码为音频文件；只读后端默认报错。
    /// 输入：
    /// - `path`：目标文件路径。
    /// - `samples`：交错立体声采样。
    /// - `sample_rate`：采样率。
    ///
    /// 输出/副作用：写入文件；不支持写入时返回错误。
    ///
    /// 复杂度：默认 O(1)；具体后端为 O(采样数)。
    ///
    /// Module: audio / audio I/O runtime / backend interface
    /// Purpose: Encode interleaved stereo samples into an audio file; read-only backends
    /// return an error by default.
    /// Inputs:
    /// - `path`: destination path.
    /// - `samples`: interleaved stereo samples.
    /// - `sample_rate`: sample rate.
    ///
    /// Outputs / side effects: writes the file; returns an error when unsupported.
    ///
    /// Complexity: O(1) by default; concrete backends are O(sample count).
    fn encode(&self, _path: &Path, _samples: &[f32], _sample_rate: f64) -> Result<(), String> {
        Err(format!("{} 不支持写入", self.info().label))
    }
}

/// 音频 I/O 运行时框架：持有格式后端注册表并暴露统一操作。
/// Audio I/O runtime framework: owns the format backend registry and unified operations.
pub struct AudioRuntime {
    codecs: Vec<Box<dyn AudioCodec>>,
}

impl AudioRuntime {
    /// 所属模块：audio / 音频 I/O 运行时 / 构造
    /// 功能：使用给定后端创建音频 I/O 运行时。
    /// 输入：`codecs` 音频格式后端列表。
    ///
    /// 输出/副作用：返回运行时；不修改外部状态。
    ///
    /// 复杂度：时间 O(后端数量)，空间 O(后端数量)。
    ///
    /// Module: audio / audio I/O runtime / construction
    /// Purpose: Create an audio I/O runtime from the given backends.
    /// Inputs: `codecs` — audio format backend list.
    ///
    /// Outputs / side effects: returns the runtime; no external state change.
    ///
    /// Complexity: O(backend count) time and space.
    pub fn new(codecs: Vec<Box<dyn AudioCodec>>) -> Self {
        Self { codecs }
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 构造
    /// 功能：创建标准运行时：注册 `WavCodec`。
    /// 输入：无。
    ///
    /// 输出/副作用：返回标准运行时。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / construction
    /// Purpose: Create the standard runtime with `WavCodec`.
    /// Inputs: none.
    ///
    /// Outputs / side effects: returns the standard runtime.
    ///
    /// Complexity: O(1) time and space.
    pub fn standard() -> Self {
        Self::new(vec![Box::new(WavCodec)])
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 查询
    /// 功能：返回全部后端能力描述。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回能力列表；不修改状态。
    ///
    /// 复杂度：时间 O(后端数量)，空间 O(后端数量)。
    ///
    /// Module: audio / audio I/O runtime / query
    /// Purpose: Return capability descriptors of all backends.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the capability list; no state change.
    ///
    /// Complexity: O(backend count) time and space.
    pub fn formats(&self) -> Vec<AudioCodecInfo> {
        self.codecs.iter().map(|codec| codec.info()).collect()
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 查询
    /// 功能：按扩展名查找可读后端。
    /// 输入：`path` 音频文件路径。
    ///
    /// 输出/副作用：返回后端引用；无匹配时为 `None`。
    ///
    /// 复杂度：时间 O(后端数量)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / query
    /// Purpose: Find a readable backend by extension.
    /// Inputs: `path` — audio file path.
    ///
    /// Outputs / side effects: returns a backend reference, or `None` when unmatched.
    ///
    /// Complexity: O(backend count) time and O(1) space.
    fn codec_for(&self, path: &Path) -> Option<&dyn AudioCodec> {
        self.codecs
            .iter()
            .find(|codec| codec.info().can_read && codec.matches(path))
            .map(|codec| codec.as_ref())
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 查询
    /// 功能：按扩展名查找可写后端。
    /// 输入：`path` 目标文件路径。
    ///
    /// 输出/副作用：返回后端引用；无匹配时为 `None`。
    ///
    /// 复杂度：时间 O(后端数量)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / query
    /// Purpose: Find a writable backend by extension.
    /// Inputs: `path` — destination path.
    ///
    /// Outputs / side effects: returns a backend reference, or `None` when unmatched.
    ///
    /// Complexity: O(backend count) time and O(1) space.
    fn writer_for(&self, path: &Path) -> Option<&dyn AudioCodec> {
        self.codecs
            .iter()
            .find(|codec| codec.info().can_write && codec.matches(path))
            .map(|codec| codec.as_ref())
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 导入
    /// 功能：按扩展名分派解码音频文件。
    /// 输入：`path` 音频文件路径。
    ///
    /// 输出/副作用：返回解码音频；不支持的格式返回错误。
    ///
    /// 复杂度：时间 O(文件采样数)，空间 O(采样数)。
    ///
    /// Module: audio / audio I/O runtime / import
    /// Purpose: Dispatch audio decoding by extension.
    /// Inputs: `path` — audio file path.
    ///
    /// Outputs / side effects: returns decoded audio; unsupported formats error.
    ///
    /// Complexity: O(sample count) time and space.
    pub fn import(&self, path: &Path) -> Result<AudioFile, String> {
        let Some(codec) = self.codec_for(path) else {
            return Err(format!(
                "不支持的音频格式 {}（支持 {}）",
                path.display(),
                self.supported_extensions()
            ));
        };
        codec
            .decode(path)
            .map_err(|error| format!("{} 解码失败: {error}", codec.info().label))
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 查询
    /// 功能：按扩展名分派读取音频时长。
    /// 输入：`path` 音频文件路径。
    ///
    /// 输出/副作用：返回秒数；不支持的格式返回错误。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / query
    /// Purpose: Dispatch audio duration lookup by extension.
    /// Inputs: `path` — audio file path.
    ///
    /// Outputs / side effects: returns seconds; unsupported formats error.
    ///
    /// Complexity: O(1) time and space.
    pub fn duration_seconds(&self, path: &Path) -> Result<f64, String> {
        let Some(codec) = self.codec_for(path) else {
            return Err(format!(
                "不支持的音频格式 {}（支持 {}）",
                path.display(),
                self.supported_extensions()
            ));
        };
        codec
            .duration_seconds(path)
            .map_err(|error| format!("{} 读取时长失败: {error}", codec.info().label))
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 导出
    /// 功能：把交错立体声编码为目标扩展名对应的音频格式。
    /// 输入：
    /// - `path`：目标文件路径。
    /// - `samples`：交错立体声采样。
    /// - `sample_rate`：采样率。
    ///
    /// 输出/副作用：写入目标文件；无可用后端时返回错误。
    ///
    /// 复杂度：时间 O(采样数)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / export
    /// Purpose: Encode interleaved stereo samples into the format matching the extension.
    /// Inputs:
    /// - `path`: destination path.
    /// - `samples`: interleaved stereo samples.
    /// - `sample_rate`: sample rate.
    ///
    /// Outputs / side effects: writes the destination; errors when no backend can write.
    ///
    /// Complexity: O(sample count) time and O(1) space.
    pub fn export(&self, path: &Path, samples: &[f32], sample_rate: f64) -> Result<(), String> {
        let Some(codec) = self.writer_for(path) else {
            return Err(format!(
                "不支持导出为 {}（可写格式：{}）",
                path.display(),
                self.writable_extensions()
            ));
        };
        codec
            .encode(path, samples, sample_rate)
            .map_err(|error| format!("{} 编码失败: {error}", codec.info().label))
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 查询
    /// 功能：生成受支持扩展名的展示文本。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回 `.wav` 形式的文本。
    ///
    /// 复杂度：时间 O(后端数量)，空间 O(扩展名数量)。
    ///
    /// Module: audio / audio I/O runtime / query
    /// Purpose: Build a display string of supported extensions.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns text such as `.wav`.
    ///
    /// Complexity: O(backend count) time and O(extension count) space.
    fn supported_extensions(&self) -> String {
        self.formats()
            .into_iter()
            .flat_map(|info| {
                info.extensions
                    .iter()
                    .map(|extension| format!(".{extension}"))
            })
            .collect::<Vec<_>>()
            .join(" / ")
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 查询
    /// 功能：生成可写扩展名的展示文本。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回 `.wav` 形式的文本。
    ///
    /// 复杂度：时间 O(后端数量)，空间 O(扩展名数量)。
    ///
    /// Module: audio / audio I/O runtime / query
    /// Purpose: Build a display string of writable extensions.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns text such as `.wav`.
    ///
    /// Complexity: O(backend count) time and O(extension count) space.
    fn writable_extensions(&self) -> String {
        self.formats()
            .into_iter()
            .filter(|info| info.can_write)
            .flat_map(|info| {
                info.extensions
                    .iter()
                    .map(|extension| format!(".{extension}"))
            })
            .collect::<Vec<_>>()
            .join(" / ")
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 进程级入口
    /// 功能：使用进程级默认运行时解码音频。
    /// 输入：`path` 音频文件路径。
    ///
    /// 输出/副作用：返回解码音频。
    ///
    /// 复杂度：时间 O(文件采样数)，空间 O(采样数)。
    ///
    /// Module: audio / audio I/O runtime / process-wide entry
    /// Purpose: Decode audio through the process-wide default runtime.
    /// Inputs: `path` — audio file path.
    ///
    /// Outputs / side effects: returns decoded audio.
    ///
    /// Complexity: O(sample count) time and space.
    pub fn import_audio(path: &Path) -> Result<AudioFile, String> {
        standard_runtime().import(path)
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 进程级入口
    /// 功能：使用进程级默认运行时读取音频时长。
    /// 输入：`path` 音频文件路径。
    ///
    /// 输出/副作用：返回秒数。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / process-wide entry
    /// Purpose: Read audio duration through the process-wide default runtime.
    /// Inputs: `path` — audio file path.
    ///
    /// Outputs / side effects: returns seconds.
    ///
    /// Complexity: O(1) time and space.
    pub fn audio_duration_seconds(path: &Path) -> Result<f64, String> {
        standard_runtime().duration_seconds(path)
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 进程级入口
    /// 功能：使用进程级默认运行时导出交错立体声。
    /// 输入：同 `AudioRuntime::export`。
    ///
    /// 输出/副作用：写入目标文件。
    ///
    /// 复杂度：时间 O(采样数)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / process-wide entry
    /// Purpose: Export interleaved stereo through the process-wide default runtime.
    /// Inputs: same as `AudioRuntime::export`.
    ///
    /// Outputs / side effects: writes the destination.
    ///
    /// Complexity: O(sample count) time and O(1) space.
    pub fn export_audio(path: &Path, samples: &[f32], sample_rate: f64) -> Result<(), String> {
        standard_runtime().export(path, samples, sample_rate)
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 进程级入口
    /// 功能：离线渲染工程并导出为目标音频格式。
    /// 输入：
    /// - `project`：要渲染的工程。
    /// - `path`：目标文件路径。
    /// - `sample_rate`：渲染采样率。
    ///
    /// 输出/副作用：写入目标文件；不修改工程。
    ///
    /// 复杂度：时间 O(工程长度 × 轨道数 + 采样数)，空间 O(采样数)。
    ///
    /// Module: audio / audio I/O runtime / process-wide entry
    /// Purpose: Render a project offline and export it to the target audio format.
    /// Inputs:
    /// - `project`: project to render.
    /// - `path`: destination path.
    /// - `sample_rate`: render sample rate.
    ///
    /// Outputs / side effects: writes the destination; the project is unchanged.
    ///
    /// Complexity: O(project length × track count + sample count) time and O(sample count)
    /// space.
    pub fn export_project(project: &Project, path: &Path, sample_rate: f64) -> Result<(), String> {
        Self::export_project_with_gpu(project, path, sample_rate, None).map(|_| ())
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 进程级入口
    /// 功能：离线渲染工程并把主总线软限幅交给 GPU，然后导出为目标音频格式。
    /// 输入：
    /// - `project`：要渲染的工程。
    /// - `path`：目标文件路径。
    /// - `sample_rate`：渲染采样率。
    /// - `gpu`：GPU 渲染配置；`None` 表示完全使用 CPU。
    /// 输出/副作用：写入目标文件；返回可选的 GPU 回退提示。
    /// 复杂度：时间 O(工程长度 × 轨道数 + 采样数)，空间 O(采样数)。
    ///
    /// Module: audio / audio I/O runtime / process-wide entry
    /// Purpose: Render the project offline with the master soft limiter on the GPU, then export it
    /// to the target audio format.
    /// Inputs:
    /// - `project`: project to render.
    /// - `path`: destination path.
    /// - `sample_rate`: render sample rate.
    /// - `gpu`: GPU render config; `None` uses the CPU only.
    /// Outputs / side effects: writes the destination; returns an optional GPU fallback warning.
    /// Complexity: O(project length × track count + sample count) time and O(sample count) space.
    pub fn export_project_with_gpu(
        project: &Project,
        path: &Path,
        sample_rate: f64,
        gpu: Option<&GpuRenderConfig>,
    ) -> Result<Option<String>, String> {
        let render = crate::audio::engine::Engine::render_offline_with(project, sample_rate, gpu)?;
        Self::export_audio(path, &render.samples, sample_rate)?;
        Ok(render.warning)
    }
}

impl Default for AudioRuntime {
    /// 所属模块：audio / 音频 I/O 运行时 / 构造
    /// 功能：创建标准音频 I/O 运行时。
    /// 输入：无。
    ///
    /// 输出/副作用：返回标准运行时。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / construction
    /// Purpose: Create the standard audio I/O runtime.
    /// Inputs: none.
    ///
    /// Outputs / side effects: returns the standard runtime.
    ///
    /// Complexity: O(1) time and space.
    fn default() -> Self {
        Self::standard()
    }
}

/// 所属模块：audio / 音频 I/O 运行时 / 进程级入口
/// 功能：返回进程级默认运行时，首次调用时注册标准后端。
/// 输入：无。
///
/// 输出/副作用：返回静态运行时引用；只初始化一次。
///
/// 复杂度：时间 O(1)（首次 O(1)），空间 O(1)。
///
/// Module: audio / audio I/O runtime / process-wide entry
/// Purpose: Return the process-wide default runtime, registering standard backends on
/// first use.
/// Inputs: none.
///
/// Outputs / side effects: returns a static runtime reference; initialized once.
///
/// Complexity: O(1) time (O(1) on first use) and O(1) space.
fn standard_runtime() -> &'static AudioRuntime {
    static RUNTIME: OnceLock<AudioRuntime> = OnceLock::new();
    RUNTIME.get_or_init(AudioRuntime::standard)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：标准运行时识别 `.wav` 并支持读写。
    /// Covers: the standard runtime recognizes `.wav` and supports read/write.
    #[test]
    fn standard_runtime_recognizes_wav() {
        let runtime = AudioRuntime::standard();
        assert_eq!(
            runtime
                .codec_for(Path::new("a.WAV"))
                .map(|codec| codec.info().id),
            Some("wav")
        );
        assert!(runtime.writer_for(Path::new("a.wav")).is_some());
        assert!(runtime.codec_for(Path::new("a.mp3")).is_none());
    }

    /// 覆盖：工程可离线渲染并导出为有效 WAV。
    /// Covers: a project renders offline and exports to a valid WAV.
    #[test]
    fn exports_a_valid_wav_file() {
        let project = Project::demo();
        let path = std::env::temp_dir().join("surfing_studio_export_test.wav");
        AudioRuntime::export_project(&project, &path, 48_000.0).expect("export");
        let metadata = std::fs::metadata(&path).expect("wav exists");
        assert!(metadata.len() > 44, "wav should contain audio data");
        let reader = hound::WavReader::open(&path).expect("read wav");
        assert_eq!(reader.spec().channels, 2);
        assert_eq!(reader.spec().sample_rate, 48_000);
        assert!(reader.len() > 48_000, "at least one second of audio");
        let _ = std::fs::remove_file(path);
    }
}
