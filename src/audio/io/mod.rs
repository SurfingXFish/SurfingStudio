//! 所属模块：audio / 音频 I/O 运行时框架
//! 文件用途：
//! - 提供类似 JVM 的音频 I/O 运行时：应用层只调用 `AudioRuntime`。
//! - 维护音频格式后端（`AudioCodec`）注册表，负责导入、时长查询与导出。
//! - WAV / Symphonia（MP3、FLAC、AAC·M4A、OGG Vorbis、AIFF、CAF、ALAC）负责导入，
//!   WAV / AIFF 负责导出；新增格式只需实现 `AudioCodec` 并注册。
//! - 统一提供峰值包络生成与多声道下混，避免各后端重复实现。
//!
//! 函数清单：
//! - `AudioRuntime::new` / `standard` / `Default::default`：运行时构造与后端注册。
//! - `AudioRuntime::formats` / `codec_for` / `writer_for` / `supports_import`：后端查询。
//! - `AudioRuntime::import` / `duration_seconds` / `export` / `export_with_options`：框架核心方法。
//! - `AudioRuntime::export_project` / `export_project_with_gpu` / `export_project_with_options`：
//!   离线渲染并导出。
//! - `AudioRuntime::import_audio` / `audio_duration_seconds` / `export_audio`：进程级入口。
//! - `build_peaks` / `split_stereo`：波形峰值与多声道下混辅助。
//!
//! Module: audio / audio I/O runtime framework
//! Purpose:
//! - Provide a JVM-like audio I/O runtime: the app only calls `AudioRuntime`.
//! - Maintain a registry of audio format backends (`AudioCodec`) for import, duration
//!   queries and export.
//! - WAV / Symphonia (MP3, FLAC, AAC·M4A, OGG Vorbis, AIFF, CAF, ALAC) handle import;
//!   WAV / AIFF handle export. New formats only need an `AudioCodec` implementation.
//! - Share peak-envelope and multichannel downmix helpers so backends never duplicate them.
//!
//! Functions:
//! - `AudioRuntime::new` / `standard` / `Default::default`: runtime construction and
//!   backend registration.
//! - `AudioRuntime::formats` / `codec_for` / `writer_for` / `supports_import`: backend queries.
//! - `AudioRuntime::import` / `duration_seconds` / `export` / `export_with_options`: core methods.
//! - `AudioRuntime::export_project` / `export_project_with_gpu` / `export_project_with_options`:
//!   offline render and export.
//! - `AudioRuntime::import_audio` / `audio_duration_seconds` / `export_audio`: process-wide entries.
//! - `build_peaks` / `split_stereo`: waveform-envelope and multichannel downmix helpers.

pub mod aiff;
pub mod symphonia;
pub mod wav;

pub use aiff::AiffCodec;
pub use symphonia::SymphoniaCodec;
pub use wav::WavCodec;

use crate::audio::engine::OfflineRenderOptions;
use crate::audio::gpu::GpuRenderConfig;
use crate::core::model::Project;
use std::path::Path;
use std::sync::OnceLock;

/// 波形峰值 bin 数；后端与持久化峰值条共用。
/// Number of waveform peak bins; shared by backends and persisted peak entries.
pub const PEAK_BINS: usize = 512;

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

/// 导出位深：16-bit PCM / 24-bit PCM / 32-bit 浮点。
/// Export bit depth: 16-bit PCM / 24-bit PCM / 32-bit float.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AudioBitDepth {
    #[default]
    Int16,
    Int24,
    Float32,
}

impl AudioBitDepth {
    /// 全部可导出的位深；导出窗口按此顺序展示。
    /// Every exportable bit depth; the export window lists them in this order.
    pub const ALL: [AudioBitDepth; 3] = [
        AudioBitDepth::Int16,
        AudioBitDepth::Int24,
        AudioBitDepth::Float32,
    ];

    /// 所属模块：audio / 音频 I/O 运行时 / 导出参数
    /// 功能：返回持久化与展示用的稳定标识。
    /// 输入：`self`。
    /// 输出/副作用：静态字符串；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / encode options
    /// Purpose: Return the stable id used for persistence and display.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn id(self) -> &'static str {
        match self {
            AudioBitDepth::Int16 => "int16",
            AudioBitDepth::Int24 => "int24",
            AudioBitDepth::Float32 => "float32",
        }
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 导出参数
    /// 功能：返回面向用户的位深名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / encode options
    /// Purpose: Return the user-facing bit-depth label.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            AudioBitDepth::Int16 => "16-bit PCM",
            AudioBitDepth::Int24 => "24-bit PCM",
            AudioBitDepth::Float32 => "32-bit 浮点",
        }
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 导出参数
    /// 功能：把持久化标识还原为位深；未知标识回退 16-bit PCM。
    /// 输入：`id`：设置中的标识。
    /// 输出/副作用：位深；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / encode options
    /// Purpose: Restore a bit depth from its persisted id; unknown ids fall back to 16-bit PCM.
    /// Inputs: `id`: the stored id.
    /// Outputs / side effects: the bit depth; no state change.
    /// Complexity: O(1) time and space.
    pub fn from_id(id: &str) -> Self {
        match id {
            "int24" => AudioBitDepth::Int24,
            "float32" => AudioBitDepth::Float32,
            _ => AudioBitDepth::Int16,
        }
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 导出参数
    /// 功能：返回该位深在文件头中声明的位数。
    /// 输入：`self`。
    /// 输出/副作用：16 / 24 / 32；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / encode options
    /// Purpose: Return the header bit count of this depth.
    /// Inputs: `self`.
    /// Outputs / side effects: 16 / 24 / 32; no state change.
    /// Complexity: O(1) time and space.
    pub fn bits(self) -> u16 {
        match self {
            AudioBitDepth::Int16 => 16,
            AudioBitDepth::Int24 => 24,
            AudioBitDepth::Float32 => 32,
        }
    }
}

/// 导出参数：采样率、声道数与位深。
/// Export options: sample rate, channel count and bit depth.
#[derive(Clone, Copy, Debug)]
pub struct AudioEncodeOptions {
    /// 采样率（Hz）。
    /// Sample rate in Hz.
    pub sample_rate: f64,
    /// 声道数；样本按交错顺序提供。
    /// Channel count; samples are supplied interleaved.
    pub channels: usize,
    /// 位深。
    /// Bit depth.
    pub bit_depth: AudioBitDepth,
}

impl Default for AudioEncodeOptions {
    /// 所属模块：audio / 音频 I/O 运行时 / 导出参数
    /// 功能：返回默认导出参数：48 kHz、立体声、16-bit PCM。
    /// 输入：无。
    /// 输出/副作用：导出参数；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / encode options
    /// Purpose: Return the default options: 48 kHz, stereo, 16-bit PCM.
    /// Inputs: none.
    /// Outputs / side effects: the options; no state change.
    /// Complexity: O(1) time and space.
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            channels: 2,
            bit_depth: AudioBitDepth::Int16,
        }
    }
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

    /// 所属模块：audio / 音频 I/O 运行时 / 后端接口
    /// 功能：按导出参数写出音频；默认退化为固定 16-bit 立体声编码。
    /// 输入：
    /// - `path`：目标文件路径。
    /// - `samples`：交错多声道采样。
    /// - `options`：采样率、声道数与位深。
    ///
    /// 输出/副作用：写入文件；不支持写入时返回错误。
    ///
    /// 复杂度：默认 O(采样数)；具体后端为 O(采样数)。
    ///
    /// Module: audio / audio I/O runtime / backend interface
    /// Purpose: Encode audio with the requested options; the default falls back to fixed 16-bit
    /// stereo encoding.
    /// Inputs:
    /// - `path`: destination path.
    /// - `samples`: interleaved multichannel samples.
    /// - `options`: sample rate, channel count and bit depth.
    ///
    /// Outputs / side effects: writes the file; returns an error when unsupported.
    ///
    /// Complexity: O(sample count) by default and in concrete backends.
    fn encode_with(
        &self,
        path: &Path,
        samples: &[f32],
        options: &AudioEncodeOptions,
    ) -> Result<(), String> {
        self.encode(path, samples, options.sample_rate)
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
    /// 功能：创建标准运行时：注册 `WavCodec`、`SymphoniaCodec` 与 `AiffCodec`。
    /// 输入：无。
    ///
    /// 输出/副作用：返回标准运行时。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / construction
    /// Purpose: Create the standard runtime with `WavCodec`, `SymphoniaCodec` and `AiffCodec`.
    /// Inputs: none.
    ///
    /// Outputs / side effects: returns the standard runtime.
    ///
    /// Complexity: O(1) time and space.
    pub fn standard() -> Self {
        // 注册顺序即读取分派顺序：WAV 优先走专用后端；其余压缩/容器格式交给 Symphonia。
        // The registration order is the read dispatch order: WAV uses its dedicated backend and
        // every other compressed/container format goes to Symphonia.
        Self::new(vec![
            Box::new(WavCodec),
            Box::new(SymphoniaCodec),
            Box::new(AiffCodec),
        ])
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

    /// 所属模块：audio / 音频 I/O 运行时 / 查询
    /// 功能：判断路径是否可导入（存在负责读取的后端）。
    /// 输入：`path` 音频文件路径。
    ///
    /// 输出/副作用：返回布尔值；不修改状态。
    ///
    /// 复杂度：时间 O(后端数量)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / query
    /// Purpose: Check whether a path is importable (some backend can read it).
    /// Inputs: `path` — audio file path.
    ///
    /// Outputs / side effects: a boolean; no state change.
    ///
    /// Complexity: O(backend count) time and O(1) space.
    pub fn supports_import(&self, path: &Path) -> bool {
        self.codec_for(path).is_some()
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 导出
    /// 功能：把交错立体声编码为目标扩展名对应的音频格式（16-bit PCM 默认参数）。
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
    /// Purpose: Encode interleaved stereo samples into the format matching the extension with the
    /// default 16-bit PCM options.
    /// Inputs:
    /// - `path`: destination path.
    /// - `samples`: interleaved stereo samples.
    /// - `sample_rate`: sample rate.
    ///
    /// Outputs / side effects: writes the destination; errors when no backend can write.
    ///
    /// Complexity: O(sample count) time and O(1) space.
    pub fn export(&self, path: &Path, samples: &[f32], sample_rate: f64) -> Result<(), String> {
        let options = AudioEncodeOptions {
            sample_rate,
            channels: 2,
            bit_depth: AudioBitDepth::Int16,
        };
        self.export_with_options(path, samples, &options)
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 导出
    /// 功能：按指定采样率、声道数与位深编码为目标扩展名对应的音频格式。
    /// 输入：
    /// - `path`：目标文件路径。
    /// - `samples`：交错多声道采样。
    /// - `options`：导出参数。
    ///
    /// 输出/副作用：写入目标文件；无可用后端或参数非法时返回错误。
    ///
    /// 复杂度：时间 O(采样数)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / export
    /// Purpose: Encode interleaved samples with the requested rate, channel count and bit depth into
    /// the format matching the extension.
    /// Inputs:
    /// - `path`: destination path.
    /// - `samples`: interleaved multichannel samples.
    /// - `options`: export options.
    ///
    /// Outputs / side effects: writes the destination; errors on unsupported formats or invalid
    /// options.
    ///
    /// Complexity: O(sample count) time and O(1) space.
    pub fn export_with_options(
        &self,
        path: &Path,
        samples: &[f32],
        options: &AudioEncodeOptions,
    ) -> Result<(), String> {
        if options.channels == 0 {
            return Err("导出声道数必须大于 0".into());
        }
        let Some(codec) = self.writer_for(path) else {
            return Err(format!(
                "不支持导出为 {}（可写格式：{}）",
                path.display(),
                self.writable_extensions()
            ));
        };
        codec
            .encode_with(path, samples, options)
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
    /// 功能：使用进程级默认运行时判断路径是否可导入。
    /// 输入：`path` 音频文件路径。
    ///
    /// 输出/副作用：返回布尔值；不修改状态。
    ///
    /// 复杂度：时间 O(后端数量)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / process-wide entry
    /// Purpose: Check whether a path is importable through the process-wide default runtime.
    /// Inputs: `path` — audio file path.
    ///
    /// Outputs / side effects: a boolean; no state change.
    ///
    /// Complexity: O(backend count) time and O(1) space.
    pub fn accepts_audio_file(path: &Path) -> bool {
        standard_runtime().supports_import(path)
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
        // 兼容入口按工程布局渲染，编码声道数必须与渲染结果一致，避免把环绕样本写进立体声文件头。
        // The legacy entry renders in the project layout, so the encoder channel count must match the
        // render; otherwise surround samples would be written under a stereo header.
        let encode = AudioEncodeOptions {
            sample_rate,
            channels: render.channels,
            bit_depth: AudioBitDepth::Int16,
        };
        standard_runtime().export_with_options(path, &render.samples, &encode)?;
        Ok(render.warning)
    }

    /// 所属模块：audio / 音频 I/O 运行时 / 进程级入口
    /// 功能：按完整渲染/导出参数离线渲染工程并写出目标音频文件。
    /// 输入：
    /// - `project`：要渲染的工程。
    /// - `path`：目标文件路径。
    /// - `render`：渲染参数（采样率、声道布局、范围、尾长、GPU）。
    /// - `encode`：编码参数（采样率、声道数、位深）；采样率必须与渲染一致。
    ///
    /// 输出/副作用：写入目标文件；返回可选的 GPU 回退提示；不修改工程。
    ///
    /// 复杂度：时间 O(渲染帧数 × 轨道数 + 样本数)，空间 O(样本数)。
    ///
    /// Module: audio / audio I/O runtime / process-wide entry
    /// Purpose: Render the project offline with full render options and write the target audio file.
    /// Inputs:
    /// - `project`: project to render.
    /// - `path`: destination path.
    /// - `render`: render options (sample rate, channel layout, range, tail, GPU).
    /// - `encode`: encode options (rate, channels, bit depth); the rate must match the render.
    ///
    /// Outputs / side effects: writes the destination; returns an optional GPU fallback warning; the
    /// project is unchanged.
    ///
    /// Complexity: O(rendered frames × tracks + samples) time and O(samples) space.
    pub fn export_project_with_options(
        project: &Project,
        path: &Path,
        render: &OfflineRenderOptions,
        encode: &AudioEncodeOptions,
    ) -> Result<Option<String>, String> {
        let rendered = crate::audio::engine::Engine::render_offline_options(project, render)?;
        if encode.channels != rendered.channels {
            return Err(format!(
                "导出声道数（{}）与渲染声道数（{}）不一致",
                encode.channels, rendered.channels
            ));
        }
        if (encode.sample_rate - rendered.sample_rate).abs() > 0.5 {
            return Err(format!(
                "导出采样率（{} Hz）与渲染采样率（{} Hz）不一致",
                encode.sample_rate, rendered.sample_rate
            ));
        }
        standard_runtime().export_with_options(path, &rendered.samples, encode)?;
        Ok(rendered.warning)
    }
}

/// 所属模块：audio / 音频 I/O 运行时 / 波形
/// 功能：由左右声道生成固定 bin 数的绝对峰值包络。
/// 输入：
/// - `left` / `right`：等长声道采样。
/// 输出/副作用：返回峰值包络；不修改输入。
/// 复杂度：时间 O(采样数)，空间 O(PEAK_BINS)。
///
/// Module: audio / audio I/O runtime / waveform
/// Purpose: Build a fixed-bin absolute peak envelope from the left/right channels.
/// Inputs:
/// - `left` / `right`: equal-length channel samples.
/// Outputs / side effects: the peak envelope; inputs are unchanged.
/// Complexity: O(sample count) time and O(PEAK_BINS) space.
pub fn build_peaks(left: &[f32], right: &[f32]) -> Vec<f32> {
    let frames = left.len().min(right.len());
    if frames == 0 {
        return Vec::new();
    }
    let bins = PEAK_BINS.min(frames);
    let mut peaks = vec![0.0_f32; bins];
    for (frame, (l, r)) in left.iter().zip(right.iter()).enumerate() {
        let bin = (frame * bins / frames).min(bins - 1);
        peaks[bin] = peaks[bin].max(l.abs()).max(r.abs());
    }
    peaks
}

/// 所属模块：audio / 音频 I/O 运行时 / 声道
/// 功能：把交错多声道采样拆为左右声道；多声道按标准电影顺序下混，单声道复制到两侧。
/// 输入：
/// - `interleaved`：交错采样。
/// - `channels`：每帧声道数（至少 1）。
/// 输出/副作用：返回 `(left, right)`；不修改输入。
/// 复杂度：时间 O(帧数 × 声道数)，空间 O(帧数 × 2)。
///
/// Module: audio / audio I/O runtime / channels
/// Purpose: Split interleaved multichannel samples into left/right; multichannel sources are
/// downmixed in standard film order and mono is duplicated to both sides.
/// Inputs:
/// - `interleaved`: interleaved samples.
/// - `channels`: channels per frame (at least 1).
/// Outputs / side effects: `(left, right)`; inputs are unchanged.
/// Complexity: O(frames × channels) time and O(frames × 2) space.
pub fn split_stereo(interleaved: &[f32], channels: usize) -> (Vec<f32>, Vec<f32>) {
    let channels = channels.max(1);
    let frames = interleaved.len() / channels;
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    if channels == 1 {
        for sample in interleaved.iter().take(frames) {
            left.push(*sample);
            right.push(*sample);
        }
        return (left, right);
    }
    if channels == 2 {
        for frame in 0..frames {
            left.push(interleaved[frame * 2]);
            right.push(interleaved[frame * 2 + 1]);
        }
        return (left, right);
    }
    // 5.1 / 7.1 等布局使用 ITU 下混系数：中置与环绕 -3 dB，后环绕 -6 dB，LFE -6 dB。
    // 5.1 / 7.1 layouts use ITU downmix coefficients: centre and surround at -3 dB, back surround
    // and LFE at -6 dB.
    const CENTER: f32 = std::f32::consts::FRAC_1_SQRT_2;
    const SURROUND: f32 = std::f32::consts::FRAC_1_SQRT_2;
    const BACK: f32 = 0.5;
    const LFE: f32 = 0.5;
    for frame in 0..frames {
        let base = frame * channels;
        let mut l = interleaved[base];
        let mut r = interleaved[base + 1];
        if channels >= 3 {
            l += interleaved[base + 2] * CENTER;
            r += interleaved[base + 2] * CENTER;
        }
        if channels >= 4 {
            l += interleaved[base + 3] * LFE;
            r += interleaved[base + 3] * LFE;
        }
        if channels >= 5 {
            l += interleaved[base + 4] * SURROUND;
        }
        if channels >= 6 {
            r += interleaved[base + 5] * SURROUND;
        }
        if channels >= 7 {
            l += interleaved[base + 6] * BACK;
        }
        if channels >= 8 {
            r += interleaved[base + 7] * BACK;
        }
        left.push(l);
        right.push(r);
    }
    (left, right)
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

    /// 覆盖：标准运行时识别 WAV 与主流压缩格式，并只把 WAV/AIFF 视为可写。
    /// Covers: the standard runtime recognizes WAV plus mainstream compressed formats and only
    /// treats WAV/AIFF as writable.
    #[test]
    fn standard_runtime_recognizes_mainstream_formats() {
        let runtime = AudioRuntime::standard();
        assert_eq!(
            runtime
                .codec_for(Path::new("a.WAV"))
                .map(|codec| codec.info().id),
            Some("wav")
        );
        assert!(runtime.writer_for(Path::new("a.wav")).is_some());
        assert_eq!(
            runtime
                .codec_for(Path::new("a.mp3"))
                .map(|codec| codec.info().id),
            Some("symphonia")
        );
        assert_eq!(
            runtime
                .codec_for(Path::new("a.flac"))
                .map(|codec| codec.info().id),
            Some("symphonia")
        );
        assert!(runtime.writer_for(Path::new("a.aiff")).is_some());
        assert!(runtime.codec_for(Path::new("a.xyz")).is_none());
    }

    /// 覆盖：5.1 交错采样按 ITU 系数下混到左右声道。
    /// Covers: 5.1 interleaved samples downmix to left/right with ITU coefficients.
    #[test]
    fn split_stereo_downmixes_surround_channels() {
        let interleaved = [0.0_f32, 0.0, 1.0, 0.0, 0.0, 0.0];
        let (left, right) = split_stereo(&interleaved, 6);
        let expected = std::f32::consts::FRAC_1_SQRT_2;
        assert!((left[0] - expected).abs() < 1e-6, "left={}", left[0]);
        assert!((right[0] - expected).abs() < 1e-6, "right={}", right[0]);
    }

    /// 覆盖：24-bit 5.1 WAV 导出写入正确的声道数与位深。
    /// Covers: 24-bit 5.1 WAV export writes the requested channel count and bit depth.
    #[test]
    fn exports_multichannel_24bit_wav() {
        let path = std::env::temp_dir().join("surfing_studio_surround_24bit.wav");
        let samples: Vec<f32> = (0..600)
            .map(|index| (index as f32 * 0.02).sin() * 0.2)
            .collect();
        let options = AudioEncodeOptions {
            sample_rate: 48_000.0,
            channels: 6,
            bit_depth: AudioBitDepth::Int24,
        };
        AudioRuntime::standard()
            .export_with_options(&path, &samples, &options)
            .expect("export surround wav");
        let reader = hound::WavReader::open(&path).expect("read wav");
        assert_eq!(reader.spec().channels, 6);
        assert_eq!(reader.spec().bits_per_sample, 24);
        assert_eq!(reader.spec().sample_rate, 48_000);
        assert_eq!(reader.len() as usize, samples.len());
        let _ = std::fs::remove_file(path);
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
