//! 所属模块：audio / 音频 I/O 运行时 / AIFF 后端
//! 文件用途：
//! - 实现只写后端 `AiffCodec`：把交错多声道采样写成 AIFF（16-bit / 24-bit PCM）。
//! - AIFF 与 WAV 并列作为导出格式；读取由 Symphonia 后端负责。
//!
//! 函数清单：
//! - `AiffCodec::info` / `encode` / `encode_with`：后端接口实现。
//! - `encode_aiff`：写入 FORM/AIFF 容器、COMM 与 SSND 块。
//! - `extended_80`：把采样率编码为 80 位 IEEE 扩展浮点。
//!
//! Module: audio / audio I/O runtime / AIFF backend
//! Purpose:
//! - Implement the write-only backend `AiffCodec`: write interleaved multichannel samples as AIFF
//!   (16-bit / 24-bit PCM).
//! - AIFF sits next to WAV as an export format; reading is handled by the Symphonia backend.
//!
//! Functions:
//! - `AiffCodec::info` / `encode` / `encode_with`: backend interface.
//! - `encode_aiff`: write the FORM/AIFF container, COMM and SSND chunks.
//! - `extended_80`: encode the sample rate as an 80-bit IEEE extended float.

use super::{AudioBitDepth, AudioCodec, AudioCodecInfo, AudioEncodeOptions};
use std::io::{BufWriter, Write};
use std::path::Path;

/// AIFF 只写后端。
/// Write-only AIFF backend.
pub struct AiffCodec;

impl AudioCodec for AiffCodec {
    /// 所属模块：audio / 音频 I/O 运行时 / AIFF 后端
    /// 功能：返回后端能力：可写、不可读（读取交给 Symphonia）。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回能力描述；不修改状态。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / AIFF backend
    /// Purpose: Return capabilities: writable, not readable (Symphonia handles reading).
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: the descriptor; no state change.
    ///
    /// Complexity: O(1) time and space.
    fn info(&self) -> AudioCodecInfo {
        AudioCodecInfo {
            id: "aiff",
            label: "AIFF",
            extensions: &["aiff", "aif", "aifc"],
            can_read: false,
            can_write: true,
        }
    }

    /// 所属模块：audio / 音频 I/O 运行时 / AIFF 后端
    /// 功能：解码接口对 AIFF 只写后端不可用。
    /// 输入：`path` 音频路径。
    ///
    /// 输出/副作用：始终返回错误；不读取文件。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / AIFF backend
    /// Purpose: Decoding is unavailable on the write-only AIFF backend.
    /// Inputs: `path` — audio path.
    ///
    /// Outputs / side effects: always an error; the file is not read.
    ///
    /// Complexity: O(1) time and space.
    fn decode(&self, _path: &Path) -> Result<super::AudioFile, String> {
        Err("AIFF 读取由 Symphonia 后端负责".into())
    }

    /// 所属模块：audio / 音频 I/O 运行时 / AIFF 后端
    /// 功能：AIFF 只写后端不提供时长查询。
    /// 输入：`path` 音频路径。
    ///
    /// 输出/副作用：始终返回错误；不读取文件。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / AIFF backend
    /// Purpose: The write-only AIFF backend does not provide duration lookup.
    /// Inputs: `path` — audio path.
    ///
    /// Outputs / side effects: always an error; the file is not read.
    ///
    /// Complexity: O(1) time and space.
    fn duration_seconds(&self, _path: &Path) -> Result<f64, String> {
        Err("AIFF 时长由 Symphonia 后端负责".into())
    }

    /// 所属模块：audio / 音频 I/O 运行时 / AIFF 后端
    /// 功能：按默认参数写出 16-bit 立体声 AIFF。
    /// 输入：
    /// - `path`：目标路径。
    /// - `samples`：交错立体声采样。
    /// - `sample_rate`：采样率。
    ///
    /// 输出/副作用：写入文件；采样 clamp 到 `[-1, 1]`。
    ///
    /// 复杂度：时间 O(采样数)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / AIFF backend
    /// Purpose: Write 16-bit stereo AIFF with the default options.
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
        encode_aiff(path, samples, &options)
    }

    /// 所属模块：audio / 音频 I/O 运行时 / AIFF 后端
    /// 功能：按导出参数写出 16-bit / 24-bit AIFF；32-bit 浮点回退为 24-bit。
    /// 输入：
    /// - `path`：目标路径。
    /// - `samples`：交错多声道采样。
    /// - `options`：采样率、声道数与位深。
    ///
    /// 输出/副作用：写入文件；采样 clamp 到 `[-1, 1]`。
    ///
    /// 复杂度：时间 O(采样数)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / AIFF backend
    /// Purpose: Write 16-bit / 24-bit AIFF with the requested options; 32-bit float falls back to
    /// 24-bit PCM.
    /// Inputs:
    /// - `path`: destination path.
    /// - `samples`: interleaved multichannel samples.
    /// - `options`: sample rate, channel count and bit depth.
    ///
    /// Outputs / side effects: writes the file; samples are clamped to `[-1, 1]`.
    ///
    /// Complexity: O(sample count) time and O(1) space.
    fn encode_with(
        &self,
        path: &Path,
        samples: &[f32],
        options: &AudioEncodeOptions,
    ) -> Result<(), String> {
        encode_aiff(path, samples, options)
    }
}

/// 所属模块：audio / 音频 I/O 运行时 / AIFF 后端
/// 功能：写出 AIFF 容器：FORM/AIFF + COMM + SSND。
/// 输入：
/// - `path`：目标路径。
/// - `samples`：交错多声道采样。
/// - `options`：采样率、声道数与位深。
///
/// 输出/副作用：创建并写入文件；失败返回中文原因。
///
/// 复杂度：时间 O(采样数)，空间 O(1)（使用 BufWriter）。
///
/// Module: audio / audio I/O runtime / AIFF backend
/// Purpose: Write the AIFF container: FORM/AIFF + COMM + SSND.
/// Inputs:
/// - `path`: destination path.
/// - `samples`: interleaved multichannel samples.
/// - `options`: sample rate, channel count and bit depth.
///
/// Outputs / side effects: creates and writes the file; failures return a Chinese reason.
///
/// Complexity: O(sample count) time and O(1) space (via BufWriter).
fn encode_aiff(path: &Path, samples: &[f32], options: &AudioEncodeOptions) -> Result<(), String> {
    let channels = options.channels.max(1);
    // AIFF 只支持 PCM；32-bit 浮点导出改写为 24-bit，避免声明与数据不一致。
    // AIFF only carries PCM; a 32-bit float request is written as 24-bit so the declaration always
    // matches the payload.
    let bits = match options.bit_depth {
        AudioBitDepth::Int24 | AudioBitDepth::Float32 => 24u16,
        AudioBitDepth::Int16 => 16u16,
    };
    let bytes_per_sample = (bits / 8) as usize;
    let frames = samples.len() / channels;
    let data_bytes = frames * channels * bytes_per_sample;
    let pad = data_bytes % 2;
    let form_size = 4 + (8 + 18) + (8 + 8 + data_bytes + pad);

    let file =
        std::fs::File::create(path).map_err(|error| format!("创建 AIFF 文件失败: {error}"))?;
    let mut writer = BufWriter::new(file);
    write_all(&mut writer, b"FORM")?;
    write_all(&mut writer, &(form_size as u32).to_be_bytes())?;
    write_all(&mut writer, b"AIFF")?;
    write_all(&mut writer, b"COMM")?;
    write_all(&mut writer, &18u32.to_be_bytes())?;
    write_all(&mut writer, &(channels as u16).to_be_bytes())?;
    write_all(&mut writer, &(frames as u32).to_be_bytes())?;
    write_all(&mut writer, &bits.to_be_bytes())?;
    write_all(&mut writer, &extended_80(options.sample_rate.max(1.0)))?;
    write_all(&mut writer, b"SSND")?;
    write_all(&mut writer, &((8 + data_bytes) as u32).to_be_bytes())?;
    write_all(&mut writer, &0u32.to_be_bytes())?;
    write_all(&mut writer, &0u32.to_be_bytes())?;

    for frame in 0..frames {
        for channel in 0..channels {
            let sample = samples[frame * channels + channel].clamp(-1.0, 1.0);
            if bits == 16 {
                let value = (sample * i16::MAX as f32) as i16;
                write_all(&mut writer, &value.to_be_bytes())?;
            } else {
                let value = (sample * 8_388_607.0) as i32;
                let bytes = value.to_be_bytes();
                write_all(&mut writer, &bytes[1..4])?;
            }
        }
    }
    if pad == 1 {
        write_all(&mut writer, &[0u8])?;
    }
    writer
        .flush()
        .map_err(|error| format!("完成 AIFF 写入失败: {error}"))?;
    Ok(())
}

/// 所属模块：audio / 音频 I/O 运行时 / AIFF 后端
/// 功能：把采样率编码为 AIFF 的 80 位 IEEE 扩展浮点。
/// 输入：`value`：采样率（Hz，正数）。
/// 输出/副作用：返回 10 字节大端表示；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: audio / audio I/O runtime / AIFF backend
/// Purpose: Encode the sample rate as AIFF's 80-bit IEEE extended float.
/// Inputs: `value`: sample rate in Hz (positive).
/// Outputs / side effects: a 10-byte big-endian representation; no state change.
/// Complexity: O(1) time and space.
fn extended_80(value: f64) -> [u8; 10] {
    let mut out = [0u8; 10];
    if !value.is_finite() || value <= 0.0 {
        return out;
    }
    // 指数为 floor(log2(value))，尾数归一化到 [1, 2)；偏置 16383。
    // The exponent is floor(log2(value)), the mantissa is normalised to [1, 2) and biased by 16383.
    let exponent = value.log2().floor() as i32;
    let mantissa = value / 2f64.powi(exponent);
    // 80 位扩展浮点的尾数为 64 位显式整数部分（不含隐藏位）。
    // The 80-bit extended float stores a 64-bit explicit mantissa (no hidden bit).
    let mantissa_bits = (mantissa * 2f64.powi(63)) as u64;
    let biased = (exponent + 16_383) as u16;
    out[0..2].copy_from_slice(&biased.to_be_bytes());
    out[2..10].copy_from_slice(&mantissa_bits.to_be_bytes());
    out
}

/// 所属模块：audio / 音频 I/O 运行时 / AIFF 后端
/// 功能：写入一段字节并把 IO 错误转换为中文提示。
/// 输入：
/// - `writer`：目标写入器。
/// - `bytes`：待写字节。
/// 输出/副作用：写入缓冲；失败返回中文错误。
/// 复杂度：时间 O(字节数)，空间 O(1)。
///
/// Module: audio / audio I/O runtime / AIFF backend
/// Purpose: Write a byte slice and convert IO errors into a Chinese message.
/// Inputs:
/// - `writer`: destination writer.
/// - `bytes`: bytes to write.
/// Outputs / side effects: buffered write; a Chinese error on failure.
/// Complexity: O(bytes) time and O(1) space.
fn write_all<W: Write>(writer: &mut W, bytes: &[u8]) -> Result<(), String> {
    writer
        .write_all(bytes)
        .map_err(|error| format!("写入 AIFF 失败: {error}"))
}
