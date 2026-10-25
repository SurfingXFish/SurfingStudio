//! 所属模块：audio / 音频 I/O 运行时 / WAV 后端
//! 文件用途：
//! - 实现 WAV 格式后端 `WavCodec`：解码 PCM/Float WAV、读取时长与写出 16-bit WAV。
//! - 解码结果包含左右声道与固定 bin 数的峰值包络。
//!
//! 函数清单：
//! - `WavCodec::info` / `decode` / `duration_seconds` / `encode`：后端接口实现。
//! - `decode_wav` / `wav_duration_seconds` / `encode_wav`：WAV 编解码实现。
//!
//! Module: audio / audio I/O runtime / WAV backend
//! Purpose:
//! - Implement the WAV backend `WavCodec`: decode PCM/Float WAV, read duration and
//!   write 16-bit WAV.
//! - Decoded audio contains left/right channels and a fixed-bin peak envelope.
//!
//! Functions:
//! - `WavCodec::info` / `decode` / `duration_seconds` / `encode`: backend interface.
//! - `decode_wav` / `wav_duration_seconds` / `encode_wav`: WAV codec implementation.

use super::{AudioCodec, AudioCodecInfo, AudioFile};
use std::path::Path;

/// 波形峰值 bin 数。
/// Number of waveform peak bins.
const PEAK_BINS: usize = 512;

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
        encode_wav(path, samples, sample_rate)
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
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for frame in 0..frames {
        let base = frame * channels;
        let l = interleaved[base];
        let r = if channels > 1 {
            interleaved[base + 1]
        } else {
            l
        };
        left.push(l);
        right.push(r);
    }

    let mut peaks = vec![0.0_f32; PEAK_BINS.min(frames)];
    let bins = peaks.len().max(1);
    for (frame, (l, r)) in left.iter().zip(right.iter()).enumerate() {
        let bin = (frame * bins / frames).min(bins - 1);
        peaks[bin] = peaks[bin].max(l.abs()).max(r.abs());
    }

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
/// 功能：把交错立体声 clamp 后写成 16-bit WAV。
/// 输入：
/// - `path`：目标路径。
/// - `samples`：交错立体声采样。
/// - `sample_rate`：采样率。
///
/// 输出/副作用：写入文件；返回失败原因。
///
/// 复杂度：时间 O(采样数)，空间 O(1)。
///
/// Module: audio / audio I/O runtime / WAV backend
/// Purpose: Clamp interleaved stereo and write 16-bit WAV.
/// Inputs:
/// - `path`: destination path.
/// - `samples`: interleaved stereo samples.
/// - `sample_rate`: sample rate.
///
/// Outputs / side effects: writes the file; failures are returned.
///
/// Complexity: O(sample count) time and O(1) space.
fn encode_wav(path: &Path, samples: &[f32], sample_rate: f64) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sample_rate.max(1.0) as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|e| format!("创建 WAV 文件失败: {e}"))?;
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer
            .write_sample(value)
            .map_err(|e| format!("写入 WAV 失败: {e}"))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("完成 WAV 写入失败: {e}"))?;
    Ok(())
}
