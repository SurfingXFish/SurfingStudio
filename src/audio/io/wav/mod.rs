//! 所属模块：audio / 音频 I/O 运行时 / WAV 后端
//! 文件用途：
//! - 实现 WAV 格式后端 `WavCodec`：解码 PCM/Float WAV、读取时长与写出 WAV。
//! - 导出支持 16-bit / 24-bit PCM 与 32-bit 浮点，并严格按请求的声道数与采样率写头。
//! - 解码结果包含左右声道与固定 bin 数的峰值包络（多声道按 ITU 系数下混）。
//!
//! 函数清单：
//! - `WavCodec::info` / `decode` / `duration_seconds` / `encode` / `encode_with`：后端接口实现。
//! - `decode_wav` / `wav_duration_seconds` / `encode_wav`：WAV 编解码实现。
//!
//! Module: audio / audio I/O runtime / WAV backend
//! Purpose:
//! - Implement the WAV backend `WavCodec`: decode PCM/Float WAV, read duration and write WAV.
//! - Export supports 16-bit / 24-bit PCM and 32-bit float, writing the requested channel count and
//!   sample rate into the header.
//! - Decoded audio contains left/right channels and a fixed-bin peak envelope (multichannel uses
//!   ITU downmix).
//!
//! Functions:
//! - `WavCodec::info` / `decode` / `duration_seconds` / `encode` / `encode_with`: backend interface.
//! - `decode_wav` / `wav_duration_seconds` / `encode_wav`: WAV codec implementation.

use super::{
    build_peaks, split_stereo, AudioBitDepth, AudioCodec, AudioCodecInfo, AudioEncodeOptions,
    AudioFile,
};
use std::path::Path;

/// WAV 格式后端。
/// WAV format backend.
pub struct WavCodec;

impl AudioCodec for WavCodec {
    /// 所属模块：audio / 音频 I/O 运行时 / WAV 后端
    /// 功能：返回 WAV 后端能力：支持读写。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回能力描述；不修改状态。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / WAV backend
    /// Purpose: Return the WAV capabilities: readable and writable.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the descriptor; no state change.
    ///
    /// Complexity: O(1) time and space.
    fn info(&self) -> AudioCodecInfo {
        AudioCodecInfo {
            id: "wav",
            label: "WAV",
            extensions: &["wav"],
            can_read: true,
            can_write: true,
        }
    }

    /// 所属模块：audio / 音频 I/O 运行时 / WAV 后端
    /// 功能：解码 WAV 为内存音频。
    /// 输入：`path` WAV 路径。
    ///
    /// 输出/副作用：返回解码音频；只读文件。
    ///
    /// 复杂度：时间 O(采样数)，空间 O(采样数)。
    ///
    /// Module: audio / audio I/O runtime / WAV backend
    /// Purpose: Decode WAV into in-memory audio.
    /// Inputs: `path` — WAV path.
    ///
    /// Outputs / side effects: returns decoded audio; the file is read-only.
    ///
    /// Complexity: O(sample count) time and space.
    fn decode(&self, path: &Path) -> Result<AudioFile, String> {
        decode_wav(path)
    }

    /// 所属模块：audio / 音频 I/O 运行时 / WAV 后端
    /// 功能：只读 WAV 文件头计算时长。
    /// 输入：`path` WAV 路径。
    ///
    /// 输出/副作用：返回秒数；只读文件头。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / WAV backend
    /// Purpose: Compute duration from the WAV header only.
    /// Inputs: `path` — WAV path.
    ///
    /// Outputs / side effects: returns seconds; only the header is read.
    ///
    /// Complexity: O(1) time and space.
    fn duration_seconds(&self, path: &Path) -> Result<f64, String> {
        wav_duration_seconds(path)
    }

    /// 所属模块：audio / 音频 I/O 运行时 / WAV 后端
    /// 功能：把交错立体声写出为 16-bit WAV。
    /// 输入：
    /// - `path`：目标路径。
    /// - `samples`：交错立体声采样。
    /// - `sample_rate`：采样率。
    ///
    /// 输出/副作用：写入文件；采样 clamp 到 `[-1, 1]`。
    ///
    /// 复杂度：时间 O(采样数)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / WAV backend
    /// Purpose: Write interleaved stereo as 16-bit WAV.
    /// Inputs:
    /// - `path`: destination path.
    /// - `samples`: interleaved stereo samples.
    /// - `sample_rate`: sample rate.
    ///
    /// Outputs / side effects: writes the file; samples are clamped to `[-1, 1]`.
    ///
    /// Complexity: O(sample count) time and O(1) space.
    fn encode(&self, path: &Path, samples: &[f32], sample_rate: f64) -> Result<(), String> {
        let options = AudioEncodeOptions {
            sample_rate,
            channels: 2,
            bit_depth: AudioBitDepth::Int16,
        };
        encode_wav(path, samples, &options)
    }

    /// 所属模块：audio / 音频 I/O 运行时 / WAV 后端
    /// 功能：按导出参数写出 16-bit / 24-bit PCM 或 32-bit 浮点 WAV。
    /// 输入：
    /// - `path`：目标路径。
    /// - `samples`：交错多声道采样。
    /// - `options`：采样率、声道数与位深。
    ///
    /// 输出/副作用：写入文件；PCM 采样 clamp 到 `[-1, 1]`。
    ///
    /// 复杂度：时间 O(采样数)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / WAV backend
    /// Purpose: Write 16-bit / 24-bit PCM or 32-bit float WAV with the requested options.
    /// Inputs:
    /// - `path`: destination path.
    /// - `samples`: interleaved multichannel samples.
    /// - `options`: sample rate, channel count and bit depth.
    ///
    /// Outputs / side effects: writes the file; PCM samples are clamped to `[-1, 1]`.
    ///
    /// Complexity: O(sample count) time and O(1) space.
    fn encode_with(
        &self,
        path: &Path,
        samples: &[f32],
        options: &AudioEncodeOptions,
    ) -> Result<(), String> {
        encode_wav(path, samples, options)
    }
}

/// 所属模块：audio / 音频 I/O 运行时 / WAV 后端
/// 功能：读取 WAV 并拆分为左右声道与峰值包络。
/// 输入：`path` WAV 路径。
///
/// 输出/副作用：返回解码音频；只读文件。
///
/// 复杂度：时间 O(采样数)，空间 O(采样数)。
///
/// Module: audio / audio I/O runtime / WAV backend
/// Purpose: Read WAV and split it into channels plus a peak envelope.
/// Inputs: `path` — WAV path.
///
/// Outputs / side effects: returns decoded audio; the file is read-only.
///
/// Complexity: O(sample count) time and space.
fn decode_wav(path: &Path) -> Result<AudioFile, String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|error| format!("无法读取音频: {error}"))?;
    let spec = reader.spec();
    if spec.channels == 0 {
        return Err("音频没有声道".into());
    }
    let channels = spec.channels as usize;
    let sample_rate = spec.sample_rate.max(1) as f64;

    let mut interleaved: Vec<f32> = Vec::new();
    match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => {
            for sample in reader.samples::<f32>() {
                interleaved.push(sample.map_err(|error| format!("音频数据损坏: {error}"))?);
            }
        }
        (hound::SampleFormat::Int, 16) => {
            for sample in reader.samples::<i16>() {
                let value = sample.map_err(|error| format!("音频数据损坏: {error}"))?;
                interleaved.push(value as f32 / i16::MAX as f32);
            }
        }
        (hound::SampleFormat::Int, 32) => {
            for sample in reader.samples::<i32>() {
                let value = sample.map_err(|error| format!("音频数据损坏: {error}"))?;
                interleaved.push((value as f64 / i32::MAX as f64) as f32);
            }
        }
        (hound::SampleFormat::Int, bits) => {
            let scale = (1u64 << (bits.min(32) - 1)) as f32;
            for sample in reader.samples::<i32>() {
                let value = sample.map_err(|error| format!("音频数据损坏: {error}"))?;
                interleaved.push(value as f32 / scale);
            }
        }
        _ => return Err("不支持的 WAV 编码格式".into()),
    }

    let frames = interleaved.len() / channels;
    if frames == 0 {
        return Err("音频文件为空".into());
    }
    // 多声道按标准顺序下混，避免 5.1 素材只取前两声道而丢失中置与环绕。
    // Multichannel sources downmix in standard order so centre and surround are never dropped.
    let (left, right) = split_stereo(&interleaved, channels);
    let peaks = build_peaks(&left, &right);
    Ok(AudioFile {
        sample_rate,
        left,
        right,
        peaks,
    })
}

/// 所属模块：audio / 音频 I/O 运行时 / WAV 后端
/// 功能：只读 WAV 头部计算时长。
/// 输入：`path` WAV 路径。
///
/// 输出/副作用：返回秒数；只读文件头。
///
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / audio I/O runtime / WAV backend
/// Purpose: Compute duration from the WAV header only.
/// Inputs: `path` — WAV path.
///
/// Outputs / side effects: returns seconds; only the header is read.
///
/// Complexity: O(1) time and space.
fn wav_duration_seconds(path: &Path) -> Result<f64, String> {
    let reader = hound::WavReader::open(path).map_err(|error| format!("无法读取音频: {error}"))?;
    let spec = reader.spec();
    if spec.sample_rate == 0 {
        return Err("音频采样率无效".into());
    }
    Ok(reader.duration() as f64 / spec.sample_rate as f64)
}

/// 所属模块：audio / 音频 I/O 运行时 / WAV 后端
/// 功能：按导出参数把交错采样写成 WAV（16/24-bit PCM 或 32-bit 浮点）。
/// 输入：
/// - `path`：目标路径。
/// - `samples`：交错多声道采样。
/// - `options`：采样率、声道数与位深。
///
/// 输出/副作用：写入文件；PCM 采样 clamp 到 `[-1, 1]`。
///
/// 复杂度：时间 O(采样数)，空间 O(1)。
///
/// Module: audio / audio I/O runtime / WAV backend
/// Purpose: Write interleaved samples as WAV with the requested options (16/24-bit PCM or 32-bit
/// float).
/// Inputs:
/// - `path`: destination path.
/// - `samples`: interleaved multichannel samples.
/// - `options`: sample rate, channel count and bit depth.
///
/// Outputs / side effects: writes the file; PCM samples are clamped to `[-1, 1]`.
///
/// Complexity: O(sample count) time and O(1) space.
fn encode_wav(path: &Path, samples: &[f32], options: &AudioEncodeOptions) -> Result<(), String> {
    let channels = options.channels.max(1);
    let spec = hound::WavSpec {
        channels: channels as u16,
        sample_rate: options.sample_rate.max(1.0) as u32,
        bits_per_sample: options.bit_depth.bits(),
        sample_format: match options.bit_depth {
            AudioBitDepth::Float32 => hound::SampleFormat::Float,
            AudioBitDepth::Int16 | AudioBitDepth::Int24 => hound::SampleFormat::Int,
        },
    };
    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|e| format!("创建 WAV 文件失败: {e}"))?;
    // 交错样本按目标声道数截断，避免多出的尾样本写入损坏的帧。
    let usable = samples.len() / channels * channels;
    let usable = &samples[..usable];
    match options.bit_depth {
        AudioBitDepth::Float32 => {
            for sample in usable {
                writer
                    .write_sample(sample.clamp(-1.0, 1.0))
                    .map_err(|e| format!("写入 WAV 失败: {e}"))?;
            }
        }
        AudioBitDepth::Int16 => {
            for sample in usable {
                let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                writer
                    .write_sample(value)
                    .map_err(|e| format!("写入 WAV 失败: {e}"))?;
            }
        }
        AudioBitDepth::Int24 => {
            for sample in usable {
                let value = (sample.clamp(-1.0, 1.0) * 8_388_607.0) as i32;
                writer
                    .write_sample(value)
                    .map_err(|e| format!("写入 WAV 失败: {e}"))?;
            }
        }
    }
    writer
        .finalize()
        .map_err(|e| format!("完成 WAV 写入失败: {e}"))?;
    Ok(())
}
