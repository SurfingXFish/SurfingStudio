//! 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
//! 文件用途：
//! - 把 SoundBug 轨道、片段、音符与插件引用转换为 `Project` 数据。
//! - 处理缺失插件回退、单位换算与 id 分配。
//!
//! 函数清单：
//! - `import_track` / `import_midi_clip` / `import_audio_clip`：轨道与片段转换。
//! - `parse_plugin` / `instrument_for` / `detect_format` / `plugin_ref`：插件转换。
//! - `decode_soundbug_state`：解码 SoundBug 的插件状态文本。
//! - `read_bpm` / `read_time_signature` / `read_master_volume`：速度、拍号与主音量。
//! - `seconds_to_beats` / `parse_color` / `allocate_id` / `push_warning`：辅助函数。
//!
//! Module: core / project runtime / SoundBug backend / conversion
//! Purpose:
//! - Convert SoundBug tracks, clips, notes and plugin references into `Project` data.
//! - Handle missing-plugin fallback, unit conversion and id allocation.
//!
//! Functions:
//! - `import_track` / `import_midi_clip` / `import_audio_clip`: track and clip conversion.
//! - `parse_plugin` / `instrument_for` / `detect_format` / `plugin_ref`: plugin conversion.
//! - `decode_soundbug_state`: decode SoundBug's text-encoded plugin state.
//! - `read_bpm` / `read_time_signature` / `read_master_volume`: tempo, time signature and master volume.
//! - `seconds_to_beats` / `parse_color` / `allocate_id` / `push_warning`: helpers.

use super::edit::Node;
use super::manifest::SoundBugManifest;
use crate::core::model::{
    AudioSource, Clip, InstrumentRef, Note, PluginFormat, PluginRef, SynthSettings, Track,
    TrackKind,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// 新建轨道时的默认音量。
/// Default volume for imported tracks without a SoundBug volume plugin.
const DEFAULT_TRACK_VOLUME: f32 = 0.8;

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
/// 功能：把一条 SoundBug 轨道节点转换为 `Track`。
/// 输入：
/// - `node`：`TRACK` 或 `WAVETRACK` 节点。
/// - `manifest`：媒体表，用于解析音频片段来源。
/// - `base_dir`：工程目录。
/// - `bpm`：工程速度，用于秒与 beat 的换算。
/// - `used_track_ids` / `next_track_id`：轨道 id 去重与分配。
/// - `used_clip_ids` / `next_clip_id`：片段 id 去重与分配。
/// - `warnings`：收集缺失插件/媒体的提示。
///
/// 输出/副作用：返回轨道；若轨道既无名称也无内容则返回 `None`。
///
/// 复杂度：时间 O(插件数 + 片段数 + 音符数)，空间 O(同)。
///
/// Module: core / project runtime / SoundBug backend / conversion
/// Purpose: Convert one SoundBug track node into a `Track`.
/// Inputs:
/// - `node`: a `TRACK` or `WAVETRACK` node.
/// - `manifest`: media table used to resolve audio clip sources.
/// - `base_dir`: project directory.
/// - `bpm`: project tempo used for seconds-to-beats conversion.
/// - `used_track_ids` / `next_track_id`: track id dedup and allocation.
/// - `used_clip_ids` / `next_clip_id`: clip id dedup and allocation.
/// - `warnings`: collects missing plugin/media messages.
///
/// Outputs / side effects: returns the track, or `None` when it has no name and no content.
///
/// Complexity: O(plugin count + clip count + note count) time and space.
// 导入一条轨道需要同时传递 id 分配器与警告收集器，参数较多但职责单一。
// Importing one track needs the id allocators and warning collector together.
#[allow(clippy::too_many_arguments)]
pub(super) fn import_track(
    node: &Node,
    manifest: &SoundBugManifest,
    base_dir: &Path,
    bpm: f64,
    used_track_ids: &mut HashSet<u64>,
    next_track_id: &mut u64,
    used_clip_ids: &mut HashSet<u64>,
    next_clip_id: &mut u64,
    warnings: &mut Vec<String>,
) -> Result<Option<Track>, String> {
    let is_audio_track = node.name == "WAVETRACK";
    let track_id = allocate_id(node.prop_u64("id"), used_track_ids, next_track_id);
    let color = node
        .prop_str("colour")
        .and_then(|value| parse_color(&value))
        .unwrap_or([45, 212, 191]);
    let muted = node.prop_bool("mute").unwrap_or(false);
    let solo = node.prop_bool("solo").unwrap_or(false);

    let plugins: Vec<SoundBugPlugin> = node
        .children
        .iter()
        .filter(|child| child.name == "PLUGIN")
        .map(parse_plugin)
        .collect();

    let mut volume = DEFAULT_TRACK_VOLUME;
    let mut pan = 0.0_f32;
    for plugin in &plugins {
        if plugin.kind == "volume" {
            if let Some(value) = plugin.volume {
                volume = value.clamp(0.0, 1.5);
            }
            if let Some(value) = plugin.pan {
                pan = value.clamp(-1.0, 1.0);
            }
        }
    }

    let mut track_name = node.prop_str("name").unwrap_or_default();
    let mut track = if is_audio_track {
        let mut track = Track::new(
            track_id,
            &track_name,
            color,
            InstrumentRef::Synth(SynthSettings::default()),
        );
        track.kind = TrackKind::Audio;
        track
    } else {
        let instrument = instrument_for(node, &plugins, warnings);
        Track::new(track_id, &track_name, color, instrument)
    };
    track.muted = muted;
    track.solo = solo;
    track.volume = volume;
    track.pan = pan;

    let instrument_plugin_index = if is_audio_track {
        None
    } else {
        plugins.iter().position(|plugin| plugin.format.is_some())
    };
    for (index, plugin) in plugins.iter().enumerate() {
        if Some(index) == instrument_plugin_index || !plugin.enabled {
            continue;
        }
        let Some(format) = plugin.format else {
            continue;
        };
        let Some(path) = plugin.path.clone() else {
            continue;
        };
        if !path.exists() {
            push_warning(
                warnings,
                format!(
                    "效果器未安装，已跳过：{} ({})",
                    plugin.display_name,
                    path.display()
                ),
            );
            continue;
        }
        track
            .effects
            .push(crate::core::model::EffectRef::Plugin(plugin_ref(
                format,
                path,
                &plugin.display_name,
                plugin.state.clone(),
                plugin.uid.clone(),
                plugin.state_is_binary,
            )));
    }

    for child in &node.children {
        match child.name.as_str() {
            "MIDICLIP" if !is_audio_track => {
                if let Some(clip) = import_midi_clip(child, bpm, used_clip_ids, next_clip_id)? {
                    track.clips.push(clip);
                }
            }
            "AUDIOCLIP" => {
                if let Some(clip) = import_audio_clip(
                    child,
                    manifest,
                    base_dir,
                    bpm,
                    used_clip_ids,
                    next_clip_id,
                    warnings,
                ) {
                    track.clips.push(clip);
                }
            }
            _ => {}
        }
    }

    if track_name.trim().is_empty() {
        track_name = track
            .clips
            .first()
            .map(|clip| clip.name.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| {
                if is_audio_track {
                    format!("音频轨道 {track_id}")
                } else {
                    format!("MIDI 轨道 {track_id}")
                }
            });
    }
    track.name = track_name;
    track.bump_revision();
    Ok(Some(track))
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
/// 功能：把 SoundBug 的 `MIDICLIP` 转换为 MIDI 片段。
/// 输入：
/// - `node`：`MIDICLIP` 节点。
/// - `bpm`：工程速度。
/// - `used_clip_ids` / `next_clip_id`：片段 id 去重与分配。
///
/// 输出/副作用：返回片段；空片段返回 `None`。
///
/// 复杂度：时间 O(音符数)，空间 O(音符数)。
///
/// Module: core / project runtime / SoundBug backend / conversion
/// Purpose: Convert a SoundBug `MIDICLIP` into a MIDI clip.
/// Inputs:
/// - `node`: a `MIDICLIP` node.
/// - `bpm`: project tempo.
/// - `used_clip_ids` / `next_clip_id`: clip id dedup and allocation.
///
/// Outputs / side effects: returns the clip, or `None` for an empty clip.
///
/// Complexity: O(note count) time and space.
fn import_midi_clip(
    node: &Node,
    bpm: f64,
    used_clip_ids: &mut HashSet<u64>,
    next_clip_id: &mut u64,
) -> Result<Option<Clip>, String> {
    let start_beat = seconds_to_beats(node.prop_f64("start").unwrap_or(0.0), bpm).max(0.0);
    let length_beats = seconds_to_beats(node.prop_f64("length").unwrap_or(0.0), bpm).max(0.0);
    let mut notes = Vec::new();
    if let Some(sequence) = node.child("SEQUENCE") {
        for note in sequence
            .children
            .iter()
            .filter(|child| child.name == "NOTE")
        {
            let pitch = note.prop_f64("p").unwrap_or(60.0).round().clamp(0.0, 127.0) as u8;
            let start = note.prop_f64("b").unwrap_or(0.0).max(0.0);
            let length = note.prop_f64("l").unwrap_or(0.25).max(0.01);
            let velocity = note
                .prop_f64("v")
                .unwrap_or(100.0)
                .round()
                .clamp(1.0, 127.0) as u8;
            notes.push(Note {
                start,
                length,
                pitch,
                velocity,
            });
        }
    }
    if notes.is_empty() {
        return Ok(None);
    }
    let note_end_beats = notes.iter().map(Note::end).fold(0.0_f64, f64::max);
    // SoundBug 允许音符超出片段窗口；保留全部音符，避免导入时丢音。
    // SoundBug can store notes beyond the clip window; keep them all so no note is lost.
    let length_beats = length_beats.max(note_end_beats).max(0.25);
    let clip_id = allocate_id(node.prop_u64("id"), used_clip_ids, next_clip_id);
    let name = node
        .prop_str("name")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "MIDI 片段".to_string());
    let mut clip = Clip::new(clip_id, name, start_beat, length_beats);
    clip.notes = notes;
    clip.bump_revision();
    Ok(Some(clip))
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
/// 功能：把 SoundBug 的 `AUDIOCLIP` 转换为音频片段，并解析媒体文件与源偏移。
/// 输入：
/// - `node`：`AUDIOCLIP` 节点。
/// - `manifest`：媒体表。
/// - `base_dir`：工程目录。
/// - `bpm`：工程速度。
/// - `used_clip_ids` / `next_clip_id`：片段 id 去重与分配。
/// - `warnings`：收集缺失媒体提示。
///
/// 输出/副作用：返回音频片段；无法确定来源时仍保留片段并记录警告。
///
/// 复杂度：时间 O(媒体数量)，空间 O(1)（不含片段本身）。
///
/// Module: core / project runtime / SoundBug backend / conversion
/// Purpose: Convert a SoundBug `AUDIOCLIP` into an audio clip and resolve its media
/// file and source offset.
/// Inputs:
/// - `node`: an `AUDIOCLIP` node.
/// - `manifest`: media table.
/// - `base_dir`: project directory.
/// - `bpm`: project tempo.
/// - `used_clip_ids` / `next_clip_id`: clip id dedup and allocation.
/// - `warnings`: collects missing-media messages.
///
/// Outputs / side effects: returns the audio clip; when the source cannot be resolved
/// the clip is kept with a warning.
///
/// Complexity: O(media count) time and O(1) extra space (excluding the clip).
fn import_audio_clip(
    node: &Node,
    manifest: &SoundBugManifest,
    base_dir: &Path,
    bpm: f64,
    used_clip_ids: &mut HashSet<u64>,
    next_clip_id: &mut u64,
    warnings: &mut Vec<String>,
) -> Option<Clip> {
    let name = node
        .prop_str("name")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "音频片段".to_string());
    let start_beat = seconds_to_beats(node.prop_f64("start").unwrap_or(0.0), bpm).max(0.0);
    let length_beats = seconds_to_beats(node.prop_f64("length").unwrap_or(0.0), bpm).max(0.25);
    let offset_seconds = node.prop_f64("offset").unwrap_or(0.0).max(0.0);

    let media = manifest
        .media
        .iter()
        .find(|media| media.kind.eq_ignore_ascii_case("wave") && media.name == name)
        .or_else(|| {
            manifest.media.iter().find(|media| {
                media.kind.eq_ignore_ascii_case("wave")
                    && media
                        .path
                        .file_stem()
                        .map(|stem| stem.to_string_lossy() == name)
                        .unwrap_or(false)
            })
        });
    let (path, source_length_seconds) = match media {
        Some(media) => (media.path.clone(), media.duration_seconds),
        None => {
            let guessed = base_dir.join("Data").join(format!("{name}.wav"));
            push_warning(
                warnings,
                format!("未找到音频媒体，已按约定路径引用：{}", guessed.display()),
            );
            (guessed, length_beats * 60.0 / bpm.max(1.0))
        }
    };
    if !path.exists() {
        push_warning(warnings, format!("音频文件不存在：{}", path.display()));
    }

    let clip_id = allocate_id(node.prop_u64("id"), used_clip_ids, next_clip_id);
    let mut clip = Clip::new(clip_id, name.clone(), start_beat, length_beats);
    clip.audio = Some(AudioSource {
        path,
        name,
        length_seconds: source_length_seconds,
        offset_seconds,
    });
    clip.bump_revision();
    Some(clip)
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
/// 功能：解析一个 SoundBug `PLUGIN` 节点的基础信息。
/// 输入：
/// - `node`：`PLUGIN` 节点。
///
/// 输出/副作用：返回插件描述；不创建插件实例。
///
/// 复杂度：时间 O(属性数)，空间 O(1)。
///
/// Module: core / project runtime / SoundBug backend / conversion
/// Purpose: Parse the basic information of a SoundBug `PLUGIN` node.
/// Inputs:
/// - `node`: a `PLUGIN` node.
///
/// Outputs / side effects: returns the plugin description; no plugin instance is created.
///
/// Complexity: O(property count) time and O(1) space.
fn parse_plugin(node: &Node) -> SoundBugPlugin {
    let kind = node.prop_str("type").unwrap_or_default();
    let display_name = node
        .prop_str("name")
        .filter(|value| !value.trim().is_empty())
        .or_else(|| node.prop_str("uniqueId"))
        .unwrap_or_else(|| kind.clone());
    let path = node.prop_str("filename").map(PathBuf::from);
    let format = path.as_ref().and_then(|path| detect_format(path));
    // 状态可能是二进制块（tag 3）或带 `.` 分隔的 Base64 文本。
    // 文本形式是 SoundBug 自己的编码，不保证是标准 VST3 状态；只保留数据，
    // 不自动恢复，避免让个别插件在 load_state 中崩溃。
    // State is either a binary blob (tag 3) or dot-separated Base64 text.
    // The text form is SoundBug's own encoding and may not be a standard VST3
    // state; preserve it but do not auto-restore it.
    let binary_state = node.prop_bytes("state");
    let state_is_binary = binary_state.is_some();
    let state = binary_state.or_else(|| {
        node.prop_str("state")
            .and_then(|text| decode_soundbug_state(&text))
    });
    SoundBugPlugin {
        kind,
        display_name,
        path,
        format,
        state,
        state_is_binary,
        uid: node.prop_str("uid").unwrap_or_default(),
        enabled: node.prop_bool("enabled").unwrap_or(true),
        volume: node.prop_f64("volume").map(|value| value as f32),
        pan: node.prop_f64("pan").map(|value| value as f32),
    }
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
/// 功能：选择 MIDI 轨道的乐器；缺失的插件降级为内置乐器并记录警告。
/// 输入：
/// - `node`：轨道节点，用于判断鼓轨。
/// - `plugins`：轨道插件列表。
/// - `warnings`：警告收集。
///
/// 输出/副作用：返回乐器引用；可能向 `warnings` 追加提示。
///
/// 复杂度：时间 O(插件数)，空间 O(1)。
///
/// Module: core / project runtime / SoundBug backend / conversion
/// Purpose: Choose the instrument of a MIDI track; missing plugins fall back to a
/// built-in instrument with a warning.
/// Inputs:
/// - `node`: track node, used to detect drum tracks.
/// - `plugins`: track plugin list.
/// - `warnings`: warning collector.
///
/// Outputs / side effects: returns the instrument reference; may append a warning.
///
/// Complexity: O(plugin count) time and O(1) space.
fn instrument_for(
    node: &Node,
    plugins: &[SoundBugPlugin],
    warnings: &mut Vec<String>,
) -> InstrumentRef {
    let track_name = node.prop_str("name").unwrap_or_default();
    if let Some(plugin) = plugins.iter().find(|plugin| plugin.format.is_some()) {
        let plugin_exists = plugin
            .path
            .as_ref()
            .map(|path| path.exists())
            .unwrap_or(false);
        if plugin_exists {
            if let (Some(format), Some(path)) = (plugin.format, plugin.path.clone()) {
                return InstrumentRef::Plugin(plugin_ref(
                    format,
                    path,
                    &plugin.display_name,
                    plugin.state.clone(),
                    plugin.uid.clone(),
                    plugin.state_is_binary,
                ));
            }
        } else {
            push_warning(
                warnings,
                format!(
                    "乐器插件未安装，已用内置乐器替代：{} ({})",
                    plugin.display_name,
                    plugin
                        .path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default()
                ),
            );
        }
    }
    if track_name.contains('鼓')
        || plugins
            .iter()
            .any(|plugin| plugin.display_name.to_ascii_lowercase().contains("drum"))
    {
        InstrumentRef::Drums(Default::default())
    } else {
        InstrumentRef::Synth(SynthSettings::default())
    }
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
/// 功能：根据文件扩展名判断插件格式。
/// 输入：`path` 插件路径。
///
/// 输出/副作用：返回 `Vst3` / `Vst2`；无法识别时返回 `None`。
///
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / project runtime / SoundBug backend / conversion
/// Purpose: Detect the plugin format from its file extension.
/// Inputs: `path` — plugin path.
///
/// Outputs / side effects: returns `Vst3` / `Vst2`, or `None` when unknown.
///
/// Complexity: O(1) time and space.
fn detect_format(path: &Path) -> Option<PluginFormat> {
    match path.extension().and_then(|value| value.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("vst3") => Some(PluginFormat::Vst3),
        Some(extension) if extension.eq_ignore_ascii_case("dll") => Some(PluginFormat::Vst2),
        _ => None,
    }
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
/// 功能：构造带插件状态与可选 VST3 class id 的插件引用。
/// 输入：
/// - `format`：插件格式。
/// - `path`：插件路径。
/// - `name`：显示名。
/// - `state`：SoundBug 保存的插件状态（已解码）。
/// - `uid`：SoundBug 的 uid；仅 32 位十六进制时作为 VST3 class id。
///
/// 输出/副作用：返回 `PluginRef`；无效 uid 留空以使用插件默认类。
///
/// 复杂度：时间 O(状态长度)，空间 O(状态长度)。
///
/// Module: core / project runtime / SoundBug backend / conversion
/// Purpose: Build a plugin reference carrying the imported state and an optional
/// VST3 class id.
/// Inputs:
/// - `format`: plugin format.
/// - `path`: plugin path.
/// - `name`: display name.
/// - `state`: decoded SoundBug plugin state.
/// - `uid`: SoundBug uid; used as a VST3 class id only when it is 32 hex digits.
///
/// Outputs / side effects: returns a `PluginRef`; invalid uids stay empty so the
/// plugin's default class is loaded.
///
/// Complexity: O(state length) time and space.
fn plugin_ref(
    format: PluginFormat,
    path: PathBuf,
    name: &str,
    state: Option<Vec<u8>>,
    uid: String,
    restore_state: bool,
) -> PluginRef {
    let mut plugin = PluginRef::new(format, path, name.to_string());
    let valid_uid = uid.len() == 32 && uid.chars().all(|character| character.is_ascii_hexdigit());
    if valid_uid {
        plugin.uid = uid;
    }
    plugin.state = state.unwrap_or_default().into();
    plugin.restore_state = restore_state;
    plugin
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
/// 功能：解码 SoundBug 的插件状态文本（Base64，`.` 为分段符）。
/// 输入：`text`：状态文本。
/// 输出/副作用：返回解码字节；遇到非法字符时返回 `None`。
/// 复杂度：时间 O(文本长度)，空间 O(解码长度)。
///
/// Module: core / project runtime / SoundBug backend / conversion
/// Purpose: Decode SoundBug's plugin state text (Base64 with `.` separators).
/// Inputs: `text`: state text.
/// Outputs / side effects: decoded bytes, or `None` on invalid characters.
/// Complexity: O(text length) time and O(decoded length) space.
fn decode_soundbug_state(text: &str) -> Option<Vec<u8>> {
    fn digit(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut output = Vec::with_capacity(text.len() * 3 / 4);
    let mut accumulator = 0u32;
    let mut bits = 0u32;
    for byte in text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace() && *byte != b'.')
    {
        if byte == b'=' {
            break;
        }
        let value = digit(byte)?;
        accumulator = (accumulator << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
        }
    }
    (!output.is_empty()).then_some(output)
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
/// 功能：读取第一个 TEMPO 的速度。
/// 输入：`root` EDIT 根节点。
///
/// 输出/副作用：返回 BPM；缺失时 `None`。
///
/// 复杂度：时间 O(1)（节点数量固定），空间 O(1)。
///
/// Module: core / project runtime / SoundBug backend / conversion
/// Purpose: Read the tempo of the first TEMPO node.
/// Inputs: `root` — EDIT root node.
///
/// Outputs / side effects: returns BPM, or `None` when absent.
///
/// Complexity: O(1) time and space.
pub(super) fn read_bpm(root: &Node) -> Option<f64> {
    let sequence = root.child("TEMPOSEQUENCE")?;
    let tempo = sequence.child("TEMPO")?;
    tempo
        .prop_f64("bpm")
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(20.0, 400.0))
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
/// 功能：读取拍号，缺失时使用 4/4。
/// 输入：`root` EDIT 根节点。
///
/// 输出/副作用：返回 `(分子, 分母)`；不修改状态。
///
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / project runtime / SoundBug backend / conversion
/// Purpose: Read the time signature, defaulting to 4/4.
/// Inputs: `root` — EDIT root node.
///
/// Outputs / side effects: returns `(numerator, denominator)`.
///
/// Complexity: O(1) time and space.
pub(super) fn read_time_signature(root: &Node) -> (u8, u8) {
    let Some(sequence) = root.child("TEMPOSEQUENCE") else {
        return (4, 4);
    };
    let Some(signature) = sequence.child("TIMESIG") else {
        return (4, 4);
    };
    let numerator = signature
        .prop_f64("numerator")
        .unwrap_or(4.0)
        .round()
        .clamp(1.0, 32.0) as u8;
    let denominator = signature
        .prop_f64("denominator")
        .unwrap_or(4.0)
        .round()
        .clamp(1.0, 32.0) as u8;
    (numerator, denominator)
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
/// 功能：读取主输出音量插件中的线性增益。
/// 输入：`root` EDIT 根节点。
///
/// 输出/副作用：返回主音量；缺失时 `None`。
///
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / project runtime / SoundBug backend / conversion
/// Purpose: Read the linear gain from the master volume plugin.
/// Inputs: `root` — EDIT root node.
///
/// Outputs / side effects: returns the master volume, or `None` when absent.
///
/// Complexity: O(1) time and space.
pub(super) fn read_master_volume(root: &Node) -> Option<f32> {
    let master = root.child("MASTERVOLUME")?;
    let plugin = master.children.iter().find(|child| {
        child.name == "PLUGIN" && child.prop_str("type").as_deref() == Some("volume")
    })?;
    plugin
        .prop_f64("volume")
        .map(|value| (value as f32).clamp(0.0, 1.5))
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
/// 功能：把秒转换为 beat。
/// 输入：
/// - `seconds`：秒。
/// - `bpm`：四分音符每分钟。
///
/// 输出/副作用：返回 beat 数；BPM 非法时按 1 处理。
///
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / project runtime / SoundBug backend / conversion
/// Purpose: Convert seconds to beats.
/// Inputs:
/// - `seconds`: seconds.
/// - `bpm`: quarter notes per minute.
///
/// Outputs / side effects: returns beats; invalid BPM is treated as 1.
///
/// Complexity: O(1) time and space.
pub(super) fn seconds_to_beats(seconds: f64, bpm: f64) -> f64 {
    seconds * bpm.max(1.0) / 60.0
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
/// 功能：解析 SoundBug 的 8 位十六进制颜色（AARRGGBB）。
/// 输入：`value` 颜色文本。
///
/// 输出/副作用：返回 RGB；格式无效时 `None`。
///
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / project runtime / SoundBug backend / conversion
/// Purpose: Parse SoundBug's 8-digit hex color (AARRGGBB).
/// Inputs: `value` — color text.
///
/// Outputs / side effects: returns RGB, or `None` when invalid.
///
/// Complexity: O(1) time and space.
fn parse_color(value: &str) -> Option<[u8; 3]> {
    let value = value.trim().trim_start_matches('#');
    let parsed = u32::from_str_radix(value, 16).ok()?;
    Some([
        ((parsed >> 16) & 0xFF) as u8,
        ((parsed >> 8) & 0xFF) as u8,
        (parsed & 0xFF) as u8,
    ])
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
/// 功能：分配一个未被占用的 id。
/// 输入：
/// - `preferred`：SoundBug 原始 id。
/// - `used`：已占用集合。
/// - `next`：递增分配游标。
///
/// 输出/副作用：返回 id；会修改 `used` 与 `next`。
///
/// 复杂度：时间 O(冲突数)，空间 O(1)。
///
/// Module: core / project runtime / SoundBug backend / conversion
/// Purpose: Allocate an unused id.
/// Inputs:
/// - `preferred`: original SoundBug id.
/// - `used`: set of used ids.
/// - `next`: allocation cursor.
///
/// Outputs / side effects: returns an id; mutates `used` and `next`.
///
/// Complexity: O(collisions) time and O(1) space.
fn allocate_id(preferred: Option<u64>, used: &mut HashSet<u64>, next: &mut u64) -> u64 {
    if let Some(id) = preferred {
        if id > 0 && used.insert(id) {
            *next = (*next).max(id + 1);
            return id;
        }
    }
    while !used.insert(*next) {
        *next += 1;
    }
    let id = *next;
    *next += 1;
    id
}

/// 所属模块：core / 工程运行时 / SoundBug 后端 / 转换
/// 功能：向警告列表追加一条去重后的提示。
/// 输入：
/// - `warnings`：警告列表。
/// - `message`：提示文本。
///
/// 输出/副作用：可能修改 `warnings`。
///
/// 复杂度：时间 O(警告数)，空间 O(1)。
///
/// Module: core / project runtime / SoundBug backend / conversion
/// Purpose: Append a de-duplicated warning message.
/// Inputs:
/// - `warnings`: warning list.
/// - `message`: message text.
///
/// Outputs / side effects: may mutate `warnings`.
///
/// Complexity: O(warning count) time and O(1) space.
fn push_warning(warnings: &mut Vec<String>, message: String) {
    if !warnings.contains(&message) {
        warnings.push(message);
    }
}

/// SoundBug 插件节点解析结果。
/// Parsed SoundBug plugin node.
struct SoundBugPlugin {
    kind: String,
    display_name: String,
    path: Option<PathBuf>,
    format: Option<PluginFormat>,
    state: Option<Vec<u8>>,
    /// 状态来自二进制 tag 3（可作为 VST3 状态尝试恢复）。
    /// State came from the binary tag 3 block (safe to try as a VST3 state).
    state_is_binary: bool,
    uid: String,
    enabled: bool,
    volume: Option<f32>,
    pan: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：秒到 beat 的换算与非法 BPM 的处理。
    /// Covers: seconds-to-beats conversion and invalid BPM handling.
    #[test]
    fn seconds_to_beats_uses_tempo() {
        assert!((seconds_to_beats(60.0, 120.0) - 120.0).abs() < 1e-9);
        assert!((seconds_to_beats(1.0, 0.0) - (1.0 / 60.0)).abs() < 1e-9);
    }

    /// 覆盖：SoundBug 状态文本的 `.` 分段 Base64 解码。
    /// Covers: decoding SoundBug's dot-separated Base64 state text.
    #[test]
    fn decodes_soundbug_state_text() {
        assert_eq!(decode_soundbug_state("TWFu"), Some(b"Man".to_vec()));
        assert_eq!(decode_soundbug_state("TW.Fu"), Some(b"Man".to_vec()));
        assert_eq!(decode_soundbug_state("TW!Fu"), None);
    }

    /// 覆盖：AARRGGBB 颜色解析。
    /// Covers: AARRGGBB color parsing.
    #[test]
    fn parses_soundbug_colors() {
        assert_eq!(parse_color("9c2360ff"), Some([0x23, 0x60, 0xFF]));
        assert_eq!(parse_color("#FF102030"), Some([0x10, 0x20, 0x30]));
        assert_eq!(parse_color("not-a-color"), None);
    }
}
