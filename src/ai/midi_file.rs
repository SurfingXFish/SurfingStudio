//! 所属模块：ai / MIDI 附件
//! 文件用途：
//! - 读取标准 MIDI 文件（SMF 格式 0/1/2），把音符、速度与拍号转成文本供 AI 阅读。
//! - 只做只读解析，不支持 SMPTE 时间码、不支持导入到工程。
//!
//! 函数清单：
//! - `attachment_text`：解析字节并生成附件文本。
//! - `parse_track`：解析单条 MTrk，返回音符、轨道名、速度与拍号。
//! - `read_vlq` / `be_u16` / `be_u32`：MIDI 变长数与整数读取。
//! - `finish_note`：把 Note On/Off 配对结果写入音符列表。
//!
//! Module: ai / MIDI attachments
//! Purpose:
//! - Read Standard MIDI Files (SMF format 0/1/2) and turn notes, tempo and time signature into text
//!   the model can read.
//! - Read-only parsing; SMPTE timecode is unsupported and nothing is imported into the project.
//!
//! Functions:
//! - `attachment_text`: parse bytes into attachment text.
//! - `parse_track`: parse one MTrk into notes, track name, tempo and time signature.
//! - `read_vlq` / `be_u16` / `be_u32`: MIDI variable-length and integer readers.
//! - `finish_note`: record a matched Note On/Off pair.

use std::collections::HashMap;

/// 解析出的单个音符（绝对 tick）。
/// One parsed note (absolute ticks).
#[derive(Clone, Copy, Debug)]
struct ParsedNote {
    start_tick: u64,
    end_tick: u64,
    pitch: u8,
    velocity: u8,
}

/// 单条 MIDI 轨道的解析结果。
/// Parse result of one MIDI track.
#[derive(Clone, Debug, Default)]
struct ParsedTrack {
    name: String,
    channel: u8,
    notes: Vec<ParsedNote>,
}

/// 单条轨道的解析结果：轨道、初始速度与拍号。
/// Parse result of one track: the track, its initial tempo and time signature.
type ParsedTrackResult = (ParsedTrack, Option<f64>, Option<(u8, u8)>);

/// 所属模块：ai / MIDI 附件
/// 功能：解析 MIDI 字节并生成附件文本：格式、PPQ、初始速度、拍号与逐轨音符。
/// 输入：
/// - `name`：附件文件名（仅用于标题）。
/// - `bytes`：文件字节。
/// 输出/副作用：附件文本或中文错误；不修改输入。
/// 复杂度：时间 O(事件数 + 音符数)，空间 O(音符数)。
///
/// Module: ai / MIDI attachments
/// Purpose: Parse MIDI bytes into attachment text: format, PPQ, initial tempo, time signature and
/// per-track notes.
/// Inputs:
/// - `name`: attachment file name (title only).
/// - `bytes`: file bytes.
/// Outputs / side effects: attachment text or a Chinese error; the input is unchanged.
/// Complexity: O(events + notes) time and O(notes) space.
pub fn attachment_text(name: &str, bytes: &[u8]) -> Result<String, String> {
    if bytes.len() < 14 || &bytes[0..4] != b"MThd" {
        return Err("不是有效的 MIDI 文件（缺少 MThd 头）".into());
    }
    let header_len = be_u32(&bytes[4..8]) as usize;
    if header_len < 6 || 8 + header_len > bytes.len() {
        return Err("MIDI 头长度非法".into());
    }
    let format = be_u16(&bytes[8..10]);
    let declared_tracks = be_u16(&bytes[10..12]);
    let division = be_u16(&bytes[12..14]);
    if division & 0x8000 != 0 {
        return Err("暂不支持 SMPTE 时间码的 MIDI 文件".into());
    }
    let ppq = f64::from(division.max(1));
    let mut tracks = Vec::new();
    let mut bpm: Option<f64> = None;
    let mut time_sig: Option<(u8, u8)> = None;
    let mut position = 8 + header_len;
    let mut parsed_count = 0u16;
    while parsed_count < declared_tracks {
        if position + 8 > bytes.len() || &bytes[position..position + 4] != b"MTrk" {
            break;
        }
        let length = be_u32(&bytes[position + 4..position + 8]) as usize;
        let start = position + 8;
        let end = start.saturating_add(length).min(bytes.len());
        if start > end {
            break;
        }
        let (track, tempo, signature) = parse_track(&bytes[start..end], ppq)?;
        if bpm.is_none() {
            bpm = tempo;
        }
        if time_sig.is_none() {
            time_sig = signature;
        }
        if !track.notes.is_empty() {
            tracks.push(track);
        }
        parsed_count += 1;
        position = start.saturating_add(length);
        if position >= bytes.len() {
            break;
        }
    }
    if tracks.is_empty() {
        return Err("MIDI 文件里没有可读取的音符".into());
    }
    let total: usize = tracks.iter().map(|track| track.notes.len()).sum();
    let (num, den) = time_sig.unwrap_or((4, 4));
    let mut text = format!(
        "MIDI 附件：{name}\n格式 {format}；PPQ {ppq:.0}；初始速度 {:.1} BPM；拍号 {num}/{den}；共 {total} 个音符\n",
        bpm.unwrap_or(120.0)
    );
    // 只导出前 600 个音符，避免附件占满上下文。
    // Export at most 600 notes so an attachment cannot fill the context.
    let mut written = 0usize;
    for (index, track) in tracks.iter().enumerate() {
        let label = if track.name.is_empty() {
            format!("轨道 {}", index + 1)
        } else {
            track.name.clone()
        };
        text.push_str(&format!(
            "### {label}（通道 {}）：{} 个音符\n",
            track.channel + 1,
            track.notes.len()
        ));
        let mut notes = track.notes.clone();
        notes.sort_by(|a, b| {
            a.start_tick
                .cmp(&b.start_tick)
                .then_with(|| a.pitch.cmp(&b.pitch))
        });
        for note in notes {
            if written >= 600 {
                text.push_str("…（其余音符省略）\n");
                return Ok(text);
            }
            let start = note.start_tick as f64 / ppq;
            let length = (note.end_tick.saturating_sub(note.start_tick)) as f64 / ppq;
            text.push_str(&format!(
                "{start:.3}拍 {} {length:.3}拍 v{}\n",
                super::plan::pitch_name(note.pitch),
                note.velocity
            ));
            written += 1;
        }
    }
    Ok(text)
}

/// 所属模块：ai / MIDI 附件 / 轨道
/// 功能：解析一条 MTrk：读取变长时值、运行状态、Meta 与通道事件，配对 Note On/Off。
/// 输入：`data` 轨道块字节；`ppq` 每四分音符 tick 数（用于速度换算前的合法性检查）。
/// 输出/副作用：返回轨道、初始速度与拍号；数据损坏时返回中文错误。
/// 复杂度：时间 O(事件数 + 音符数)，空间 O(音符数)。
///
/// Module: ai / MIDI attachments / track
/// Purpose: Parse one MTrk: variable-length deltas, running status, meta and channel events, pairing
/// Note On/Off.
/// Inputs: `data` track-chunk bytes; `ppq` ticks per quarter note.
/// Outputs / side effects: track, initial tempo and time signature; corrupt data returns a Chinese error.
/// Complexity: O(events + notes) time and O(notes) space.
fn parse_track(data: &[u8], _ppq: f64) -> Result<ParsedTrackResult, String> {
    let mut track = ParsedTrack::default();
    let mut open: HashMap<(u8, u8), Vec<(u64, u8)>> = HashMap::new();
    let mut tempo = None;
    let mut signature = None;
    let mut tick = 0u64;
    let mut index = 0usize;
    let mut running = 0u8;
    while index < data.len() {
        let (delta, used) = read_vlq(data, index)?;
        index += used;
        tick = tick.saturating_add(delta);
        if index >= data.len() {
            break;
        }
        let mut status = data[index];
        if status & 0x80 == 0 {
            if running == 0 {
                return Err("MIDI 轨道出现无效的运行状态".into());
            }
            status = running;
        } else {
            index += 1;
            if status < 0xF0 {
                running = status;
            }
        }
        if status == 0xFF {
            if index >= data.len() {
                break;
            }
            let kind = data[index];
            index += 1;
            let (length, used) = read_vlq(data, index)?;
            index += used;
            let length = length as usize;
            let end = index.saturating_add(length);
            if end > data.len() {
                break;
            }
            let payload = &data[index..end];
            match kind {
                0x51 if payload.len() >= 3 => {
                    let micros = (u32::from(payload[0]) << 16)
                        | (u32::from(payload[1]) << 8)
                        | u32::from(payload[2]);
                    if micros > 0 && tempo.is_none() {
                        tempo = Some(60_000_000.0 / f64::from(micros));
                    }
                }
                0x58 if payload.len() >= 2 => {
                    if signature.is_none() {
                        let num = payload[0].max(1);
                        let den = 1u8 << payload[1].min(6);
                        signature = Some((num, den));
                    }
                }
                0x03 => {
                    if track.name.is_empty() {
                        track.name = String::from_utf8_lossy(payload).trim().to_string();
                    }
                }
                0x2F => break,
                _ => {}
            }
            index = end;
            continue;
        }
        if status == 0xF0 || status == 0xF7 {
            let (length, used) = read_vlq(data, index)?;
            let length = length as usize;
            index = index.saturating_add(used).saturating_add(length);
            continue;
        }
        let kind = status & 0xF0;
        let channel = status & 0x0F;
        if status < 0x80 {
            return Err("MIDI 轨道缺少通道状态".into());
        }
        if kind == 0xC0 || kind == 0xD0 {
            index = index.saturating_add(1);
            continue;
        }
        if index + 2 > data.len() {
            break;
        }
        let data1 = data[index];
        let data2 = data[index + 1];
        index += 2;
        match kind {
            0x90 if data2 > 0 => {
                open.entry((channel, data1))
                    .or_default()
                    .push((tick, data2));
                track.channel = channel;
            }
            0x80 | 0x90 => {
                if let Some(stack) = open.get_mut(&(channel, data1)) {
                    if let Some((start_tick, velocity)) = stack.pop() {
                        finish_note(&mut track.notes, start_tick, tick, data1, velocity);
                    }
                }
                track.channel = channel;
            }
            _ => {}
        }
    }
    for ((_, pitch), stack) in open {
        for (start_tick, velocity) in stack {
            finish_note(&mut track.notes, start_tick, tick, pitch, velocity);
        }
    }
    // 超过 20000 个音符视为异常文件，避免附件文本与内存失控。
    // More than 20k notes is treated as an abnormal file to keep memory and text bounded.
    if track.notes.len() > 20_000 {
        return Err("MIDI 附件音符过多（超过 20000 个）".into());
    }
    Ok((track, tempo, signature))
}

/// 所属模块：ai / MIDI 附件 / 音符
/// 功能：把配对后的 Note On/Off 追加到列表；零长度音符补足为 1 tick。
/// 输入：`notes` 输出列表；起止 tick；音高；力度。
/// 输出/副作用：修改 `notes`；不返回错误。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / MIDI attachments / note
/// Purpose: Append a matched Note On/Off; zero-length notes get one tick.
/// Inputs: `notes` output list; start/end ticks; pitch; velocity.
/// Outputs / side effects: mutates `notes`; returns no error.
/// Complexity: O(1) time and space.
fn finish_note(
    notes: &mut Vec<ParsedNote>,
    start_tick: u64,
    end_tick: u64,
    pitch: u8,
    velocity: u8,
) {
    notes.push(ParsedNote {
        start_tick,
        end_tick: end_tick.max(start_tick + 1),
        pitch,
        velocity: velocity.clamp(1, 127),
    });
}

/// 所属模块：ai / MIDI 附件 / 读取
/// 功能：读取 MIDI 变长数（每字节 7 位，最高位为继续标志）。
/// 输入：`data` 字节；`index` 起始位置。
/// 输出/副作用：数值与消耗字节数；越界返回错误。
/// 复杂度：时间 O(变长数长度)，空间 O(1)。
///
/// Module: ai / MIDI attachments / reader
/// Purpose: Read a MIDI variable-length number (7 bits per byte, high bit continues).
/// Inputs: `data` bytes; `index` start position.
/// Outputs / side effects: value and consumed bytes; out-of-range returns an error.
/// Complexity: O(vlq length) time and O(1) space.
fn read_vlq(data: &[u8], index: usize) -> Result<(u64, usize), String> {
    let mut value = 0u64;
    let mut used = 0usize;
    loop {
        let byte = *data
            .get(index + used)
            .ok_or_else(|| "MIDI 变长数越界".to_string())?;
        value = (value << 7) | u64::from(byte & 0x7F);
        used += 1;
        if byte & 0x80 == 0 {
            return Ok((value, used));
        }
        if used > 4 {
            return Err("MIDI 变长数过长".into());
        }
    }
}

/// 所属模块：ai / MIDI 附件 / 读取
/// 功能：读取大端 16 位整数。
/// 输入：`data` 至少 2 字节。
/// 输出/副作用：数值；越界返回 0。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / MIDI attachments / reader
/// Purpose: Read a big-endian 16-bit integer.
/// Inputs: `data` with at least 2 bytes.
/// Outputs / side effects: the value; out-of-range returns 0.
/// Complexity: O(1) time and space.
fn be_u16(data: &[u8]) -> u16 {
    if data.len() < 2 {
        return 0;
    }
    (u16::from(data[0]) << 8) | u16::from(data[1])
}

/// 所属模块：ai / MIDI 附件 / 读取
/// 功能：读取大端 32 位整数。
/// 输入：`data` 至少 4 字节。
/// 输出/副作用：数值；越界返回 0。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / MIDI attachments / reader
/// Purpose: Read a big-endian 32-bit integer.
/// Inputs: `data` with at least 4 bytes.
/// Outputs / side effects: the value; out-of-range returns 0.
/// Complexity: O(1) time and space.
fn be_u32(data: &[u8]) -> u32 {
    if data.len() < 4 {
        return 0;
    }
    (u32::from(data[0]) << 24)
        | (u32::from(data[1]) << 16)
        | (u32::from(data[2]) << 8)
        | u32::from(data[3])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个最小的格式 0 MIDI：MThd + 一条 MTrk，包含速度、拍号与两个音符。
    /// Build a minimal format-0 MIDI: MThd + one MTrk with tempo, time signature and two notes.
    fn minimal_midi() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MThd");
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&1u16.to_be_bytes());
        bytes.extend_from_slice(&480u16.to_be_bytes());

        let mut track = Vec::new();
        // 速度 120 BPM：500000 µs/四分音符。
        track.extend_from_slice(&[0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]);
        // 4/4。
        track.extend_from_slice(&[0x00, 0xFF, 0x58, 0x04, 0x04, 0x02, 0x18, 0x08]);
        // C4 Note On，力度 100。
        track.extend_from_slice(&[0x00, 0x90, 60, 100]);
        // 480 tick 后 Note Off。
        track.extend_from_slice(&[0x83, 0x60, 0x80, 60, 0]);
        // E4，240 tick 后关闭。
        track.extend_from_slice(&[0x00, 0x90, 64, 90]);
        track.extend_from_slice(&[0x81, 0x70, 0x80, 64, 0]);
        // 结束。
        track.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);

        bytes.extend_from_slice(b"MTrk");
        bytes.extend_from_slice(&(track.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&track);
        bytes
    }

    /// 覆盖：最小 MIDI 能解析出速度、拍号、音名与时长。
    /// Covers: the minimal MIDI yields tempo, time signature, note names and lengths.
    #[test]
    fn minimal_midi_is_readable() {
        let text = attachment_text("test.mid", &minimal_midi()).expect("midi");
        assert!(text.contains("120.0 BPM"));
        assert!(text.contains("拍号 4/4"));
        assert!(text.contains("C4"));
        assert!(text.contains("E4"));
        assert!(text.contains("1.000拍"));
        assert!(text.contains("0.500拍"));
    }

    /// 覆盖：非 MIDI 数据被拒绝。
    /// Covers: non-MIDI data is rejected.
    #[test]
    fn invalid_midi_is_rejected() {
        assert!(attachment_text("bad.mid", b"not a midi").is_err());
        assert!(attachment_text("short.mid", &[]).is_err());
    }

    /// 覆盖：变长数读取。
    /// Covers: variable-length number reading.
    #[test]
    fn vlq_reads_multi_byte_values() {
        assert_eq!(read_vlq(&[0x00], 0).expect("one"), (0, 1));
        assert_eq!(read_vlq(&[0x7F], 0).expect("single"), (127, 1));
        assert_eq!(read_vlq(&[0x81, 0x00], 0).expect("two"), (128, 2));
        assert!(read_vlq(&[], 0).is_err());
    }
}
