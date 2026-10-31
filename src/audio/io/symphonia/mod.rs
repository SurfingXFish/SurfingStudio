//! 所属模块：audio / 音频 I/O 运行时 / Symphonia 后端
//! 文件用途：
//! - 使用纯 Rust 的 Symphonia 解码主流压缩与容器音频：MP3、FLAC、AAC·M4A、
//!   OGG Vorbis、AIFF、CAF、ALAC 与 PCM WAV。
//! - 解码结果统一为左右声道（多声道按 ITU 系数下混）与固定 bin 数的峰值包络。
//! - 只读后端：导出仍由 WAV / AIFF 后端负责。
//!
//! 函数清单：
//! - `SymphoniaCodec::info` / `decode` / `duration_seconds`：后端接口实现。
//! - `decode_with_symphonia`：完整解码并下混为立体声。
//! - `probe_track`：探测容器并返回默认音频轨的元数据。
//!
//! Module: audio / audio I/O runtime / Symphonia backend
//! Purpose:
//! - Decode mainstream compressed/container audio with pure-Rust Symphonia: MP3, FLAC, AAC·M4A,
//!   OGG Vorbis, AIFF, CAF, ALAC and PCM WAV.
//! - Normalise the result to left/right (multichannel uses ITU downmix) plus a fixed-bin peak
//!   envelope.
//! - Read-only: export stays with the WAV / AIFF backends.
//!
//! Functions:
//! - `SymphoniaCodec::info` / `decode` / `duration_seconds`: backend interface.
//! - `decode_with_symphonia`: fully decode and downmix to stereo.
//! - `probe_track`: probe the container and return the default audio track metadata.

use super::{build_peaks, split_stereo, AudioCodec, AudioCodecInfo, AudioFile};
use std::fs::File;
use std::path::Path;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// Symphonia 支持的导入扩展名；覆盖桌面与移动端最常见的音频格式。
/// Import extensions handled by Symphonia; covers the most common desktop and mobile formats.
const EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "m4a", "mp4", "m4v", "aac", "aif", "aiff", "aifc", "caf", "alac",
    "mkv", "webm",
];

/// Symphonia 格式后端。
/// Symphonia format backend.
pub struct SymphoniaCodec;

impl AudioCodec for SymphoniaCodec {
    /// 所属模块：audio / 音频 I/O 运行时 / Symphonia 后端
    /// 功能：返回后端能力：可读、不可写。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回能力描述；不修改状态。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: audio / audio I/O runtime / Symphonia backend
    /// Purpose: Return capabilities: readable, not writable.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: the descriptor; no state change.
    ///
    /// Complexity: O(1) time and space.
    fn info(&self) -> AudioCodecInfo {
        AudioCodecInfo {
            id: "symphonia",
            label: "Symphonia 压缩音频",
            extensions: EXTENSIONS,
            can_read: true,
            can_write: false,
        }
    }

    /// 所属模块：audio / 音频 I/O 运行时 / Symphonia 后端
    /// 功能：解码压缩音频为内存立体声。
    /// 输入：`path` 音频文件路径。
    ///
    /// 输出/副作用：返回解码音频；只读文件。
    ///
    /// 复杂度：时间 O(采样数 × 声道数)，空间 O(采样数)。
    ///
    /// Module: audio / audio I/O runtime / Symphonia backend
    /// Purpose: Decode compressed audio into in-memory stereo.
    /// Inputs: `path` — audio file path.
    ///
    /// Outputs / side effects: decoded audio; the file is read-only.
    ///
    /// Complexity: O(samples × channels) time and O(samples) space.
    fn decode(&self, path: &Path) -> Result<AudioFile, String> {
        decode_with_symphonia(path)
    }

    /// 所属模块：audio / 音频 I/O 运行时 / Symphonia 后端
    /// 功能：只读容器元数据计算时长；缺少帧数信息时回退为完整解码统计。
    /// 输入：`path` 音频文件路径。
    ///
    /// 输出/副作用：返回秒数；只读文件头（缺少帧数时才完整解码）。
    ///
    /// 复杂度：通常时间 O(1)；缺少帧数时 O(采样数)。
    ///
    /// Module: audio / audio I/O runtime / Symphonia backend
    /// Purpose: Compute the duration from container metadata; fall back to a full decode when the
    /// frame count is missing.
    /// Inputs: `path` — audio file path.
    ///
    /// Outputs / side effects: seconds; reads the header only (full decode when frames are missing).
    ///
    /// Complexity: normally O(1); O(samples) when the frame count is unavailable.
    fn duration_seconds(&self, path: &Path) -> Result<f64, String> {
        let (track, sample_rate) = probe_track(path)?;
        if let (Some(frames), Some(rate)) = (track.num_frames, sample_rate) {
            if rate > 0 {
                return Ok(frames as f64 / rate as f64);
            }
        }
        if let (Some(duration), Some(time_base)) = (track.duration, track.time_base) {
            if let Some(time) = time_base.calc_duration(duration) {
                let seconds = time.as_secs_f64();
                if seconds > 0.0 {
                    return Ok(seconds);
                }
            }
        }
        // 部分 MP3 / AAC 流没有帧数或时长，只能完整解码后按采样数统计。
        // Some MP3 / AAC streams carry neither frame count nor duration, so decode and count.
        let file = decode_with_symphonia(path)?;
        Ok(file.left.len() as f64 / file.sample_rate.max(1.0))
    }
}

/// 所属模块：audio / 音频 I/O 运行时 / Symphonia 后端
/// 功能：使用 Symphonia 完整解码一个音频文件，并把任意声道布局下混为立体声。
/// 输入：`path` 音频文件路径。
///
/// 输出/副作用：返回解码音频；只读文件。单帧解码错误会被跳过，容器错误直接返回。
///
/// 复杂度：时间 O(采样数 × 声道数)，空间 O(采样数)。
///
/// Module: audio / audio I/O runtime / Symphonia backend
/// Purpose: Fully decode one audio file with Symphonia and downmix any channel layout to stereo.
/// Inputs: `path` — audio file path.
///
/// Outputs / side effects: decoded audio; the file is read-only. Single-packet decode errors are
/// skipped, container errors are returned.
///
/// Complexity: O(samples × channels) time and O(samples) space.
fn decode_with_symphonia(path: &Path) -> Result<AudioFile, String> {
    let source = File::open(path).map_err(|error| format!("无法打开音频: {error}"))?;
    let stream = MediaSourceStream::new(Box::new(source), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|error| format!("无法识别音频容器: {error}"))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| "音频文件没有可解码的音轨".to_string())?
        .clone();
    let audio_params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or_else(|| "音频编码参数缺失".to_string())?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
        .map_err(|error| format!("不支持的音频编码: {error}"))?;
    let track_id = track.id;

    let mut interleaved: Vec<f32> = Vec::new();
    let mut channels = 0usize;
    let mut sample_rate = 0.0_f64;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            // 链式 OGG 等高级特性按文件结束处理，避免整段导入失败。
            // Chained OGG and similar advanced features are treated as end-of-file rather than
            // failing the whole import.
            Err(Error::ResetRequired) => break,
            Err(error) => return Err(format!("读取音频数据失败: {error}")),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(buffer) => buffer,
            // 单个损坏包不影响其余内容；与 Symphonia 官方示例一致跳过。
            // One damaged packet must not drop the rest; skip it like the official example.
            Err(Error::DecodeError(_)) | Err(Error::IoError(_)) => continue,
            Err(error) => return Err(format!("解码音频失败: {error}")),
        };
        let spec = decoded.spec();
        if channels == 0 {
            channels = spec.channels().count();
            sample_rate = spec.rate() as f64;
        }
        let sample_count = decoded.samples_interleaved();
        if sample_count == 0 {
            continue;
        }
        let start = interleaved.len();
        interleaved.resize(start + sample_count, 0.0);
        decoded.copy_to_slice_interleaved(&mut interleaved[start..]);
    }

    if interleaved.is_empty() || channels == 0 {
        return Err("音频文件为空或没有可解码的采样".into());
    }
    let (left, right) = split_stereo(&interleaved, channels);
    let peaks = build_peaks(&left, &right);
    Ok(AudioFile {
        sample_rate: sample_rate.max(1.0),
        left,
        right,
        peaks,
    })
}

/// 所属模块：audio / 音频 I/O 运行时 / Symphonia 后端
/// 功能：探测容器并返回默认音频轨与声明的采样率。
/// 输入：`path` 音频文件路径。
///
/// 输出/副作用：返回（音轨元数据、采样率）；不完整解码。
///
/// 复杂度：时间 O(文件头大小)，空间 O(1)。
///
/// Module: audio / audio I/O runtime / Symphonia backend
/// Purpose: Probe the container and return the default audio track plus its declared sample rate.
/// Inputs: `path` — audio file path.
///
/// Outputs / side effects: (track metadata, sample rate); no full decode.
///
/// Complexity: O(header size) time and O(1) space.
fn probe_track(path: &Path) -> Result<(symphonia::core::formats::Track, Option<u32>), String> {
    let source = File::open(path).map_err(|error| format!("无法打开音频: {error}"))?;
    let stream = MediaSourceStream::new(Box::new(source), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }
    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|error| format!("无法识别音频容器: {error}"))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| "音频文件没有可解码的音轨".to_string())?
        .clone();
    let sample_rate = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .and_then(|audio| audio.sample_rate);
    Ok((track, sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::io::aiff::AiffCodec;
    use crate::audio::io::{AudioBitDepth, AudioCodec, AudioEncodeOptions};

    /// 覆盖：Symphonia 可解码 AIFF 后端写出的音轨（验证容器/编码器接入）。
    /// Covers: Symphonia decodes a track written by the AIFF backend (verifies container/codec
    /// wiring).
    #[test]
    fn symphonia_decodes_an_aiff_file() {
        let path = std::env::temp_dir().join("surfing_studio_symphonia_aiff.aiff");
        let samples: Vec<f32> = (0..4_800)
            .map(|index| (index as f32 * 0.01).sin() * 0.4)
            .collect();
        let options = AudioEncodeOptions {
            sample_rate: 48_000.0,
            channels: 1,
            bit_depth: AudioBitDepth::Int16,
        };
        AiffCodec
            .encode_with(&path, &samples, &options)
            .expect("write aiff");
        let decoded = decode_with_symphonia(&path).expect("decode aiff");
        assert_eq!(decoded.left.len(), samples.len());
        assert_eq!(decoded.sample_rate, 48_000.0);
        assert!(
            decoded.left.iter().any(|sample| sample.abs() > 1e-3),
            "decoded audio should not be silent"
        );
        let _ = std::fs::remove_file(path);
    }

    /// 覆盖：Symphonia 解码真实压缩素材（通过 `SURFING_TEST_AUDIO` 指定本地文件时执行）。
    /// Covers: Symphonia decodes a real compressed asset (runs when `SURFING_TEST_AUDIO` points at
    /// one).
    #[test]
    fn symphonia_decodes_real_compressed_audio_when_configured() {
        let Ok(path) = std::env::var("SURFING_TEST_AUDIO") else {
            // 仓库不内置压缩音频素材；未配置时跳过而不是让测试失败。
            // The repository ships no compressed audio assets; skip unless configured.
            return;
        };
        let path = std::path::PathBuf::from(path);
        let decoded = decode_with_symphonia(&path).expect("decode compressed audio");
        assert!(decoded.sample_rate > 8_000.0);
        assert!(!decoded.left.is_empty());
        assert_eq!(decoded.left.len(), decoded.right.len());
    }
}
