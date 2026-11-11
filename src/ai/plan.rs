//! 所属模块：ai / 编曲计划
//! 文件用途：
//! - 定义 AI 返回的 JSON 编曲计划（回复文本 + 操作数组）与音符数组格式。
//! - 把操作原子地写入 `Project`：任一操作失败都不保留部分结果（调用方先克隆工程）。
//! - 提供音高名与 MIDI 编号互转、乐器/效果器名称解析。
//!
//! 函数清单：
//! - `AiPlan::parse` / `AiPlan::apply`：解析模型输出并写入工程。
//! - `AiOperation`：设置速度/拍号、清空、加轨、改轨、写片段、加内置/插件效果器。
//! - `AiPluginInfo` / `resolve_plugin`：已安装 VST 插件的清单与名称解析。
//! - `AiNote`：`[音高, 起始拍, 时值, 力度]` 的自定义反序列化。
//! - `AiApplySummary` / `summary_text`：写入结果摘要。
//! - `parse_pitch` / `pitch_name`：音高名与 MIDI 编号互转。
//! - `resolve_instrument` / `resolve_effect`：名称到内置乐器/效果器的映射。
//! - `find_track` / `track_key`：按名称或 id 查找轨道。
//!
//! Module: ai / composition plan
//! Purpose:
//! - Define the JSON composition plan returned by the model (reply text + operation array) and the
//!   note array format.
//! - Apply operations atomically to `Project`: a failure must not keep partial results (the caller
//!   clones the project first).
//! - Provide pitch-name/MIDI conversion and instrument/effect-name resolution.
//!
//! Functions:
//! - `AiPlan::parse` / `AiPlan::apply`: parse the model output and write it into a project.
//! - `AiOperation`: tempo/time signature, clear, add/set track, write clip, add native/plugin effect.
//! - `AiPluginInfo` / `resolve_plugin`: installed VST catalogue and name resolution.
//! - `AiNote`: custom deserialization of `[pitch, start, length, velocity]`.
//! - `AiApplySummary` / `summary_text`: write summary.
//! - `parse_pitch` / `pitch_name`: pitch-name and MIDI conversion.
//! - `resolve_instrument` / `resolve_effect`: name to built-in instrument/effect mapping.
//! - `find_track` / `track_key`: track lookup by name or id.

use crate::core::model::{
    Clip, EffectRef, InstrumentRef, NativeEffect, NativeEffectKind, NativeInstrument,
    NativeInstrumentKind, Note, PluginFormat, PluginRef, Project, SampleInstrument, SynthSettings,
    TrackKind, bank_instruments,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

/// 单个片段/音符允许的最大拍位置；防止模型输出夸张数值把工程撑到不可用。
/// Maximum beat position for one clip/note; keeps wild model output from making a project unusable.
const MAX_POSITION_BEATS: f64 = 100_000.0;

/// 插件在 AI 清单中的角色。
/// Plugin role inside the AI catalogue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiPluginKind {
    Instrument,
    Effect,
    /// 扫描器未能判断类型；乐器与效果器操作都允许尝试。
    /// The scanner could not classify it; both instrument and effect operations may try it.
    Unknown,
}

impl AiPluginKind {
    /// 所属模块：ai / 编曲计划 / 插件
    /// 功能：返回清单中的中文角色名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / composition plan / plugins
    /// Purpose: Return the Chinese role label used in the catalogue.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            AiPluginKind::Instrument => "乐器",
            AiPluginKind::Effect => "效果器",
            AiPluginKind::Unknown => "未知",
        }
    }
}

/// 已安装插件的一行清单：AI 只用它选择插件，不暴露本机路径。
/// One installed-plugin row used by the AI; local paths stay out of the prompt.
#[derive(Clone, Debug)]
pub struct AiPluginInfo {
    pub name: String,
    pub vendor: String,
    pub category: String,
    /// 离线资料库中的中文说明（如“波表合成器”），帮助模型判断音色用途。
    /// Chinese description from the offline catalog (e.g. “wavetable synth”) so the model knows the
    /// plugin's role.
    pub description: String,
    pub format: PluginFormat,
    pub path: PathBuf,
    pub uid: String,
    pub kind: AiPluginKind,
    pub has_midi_input: bool,
}

impl AiPluginInfo {
    /// 所属模块：ai / 编曲计划 / 插件
    /// 功能：生成提示词中的一行插件描述（角色、名称、厂商、分类、格式、说明与 MIDI）。
    /// 输入：`self`。
    /// 输出/副作用：文本；不修改状态。
    /// 复杂度：时间 O(名称长度)，空间 O(名称长度)。
    ///
    /// Module: ai / composition plan / plugins
    /// Purpose: Build one prompt line describing the plugin (role, name, vendor, category, format,
    /// description and MIDI).
    /// Inputs: `self`.
    /// Outputs / side effects: text; no state change.
    /// Complexity: O(name length) time and space.
    pub fn prompt_line(&self) -> String {
        let mut line = format!(
            "{} | {} | {} | {} | {} | {}",
            self.kind.label(),
            self.name,
            if self.vendor.is_empty() {
                "未知厂商"
            } else {
                &self.vendor
            },
            if self.category.is_empty() {
                "未分类"
            } else {
                &self.category
            },
            self.format.label(),
            if self.description.is_empty() {
                "—"
            } else {
                &self.description
            }
        );
        if self.has_midi_input {
            line.push_str(" | MIDI");
        }
        line
    }
}

/// 模型输出的完整编曲计划。
/// The full composition plan returned by the model.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct AiPlan {
    /// 面向用户的简短说明，直接显示在对话气泡中。
    /// Short user-facing explanation shown directly in the chat bubble.
    #[serde(default)]
    pub reply: String,
    /// 需要联网检索的查询；非空时宿主会先搜索，再让模型给出最终计划。
    /// Web-search queries; when non-empty the host searches first and asks the model again.
    #[serde(default)]
    pub research: Vec<String>,
    /// 需要加载的技巧 id；非空时宿主会加载技巧正文后再次调用模型。
    /// Technique ids to load; when non-empty the host loads their bodies and calls the model again.
    #[serde(default)]
    pub skills: Vec<String>,
    /// 要写入工程的操作，按顺序执行。
    /// Operations written into the project, executed in order.
    #[serde(default)]
    pub operations: Vec<AiOperation>,
}

impl AiPlan {
    /// 所属模块：ai / 编曲计划
    /// 功能：把模型返回的 JSON 文本解析为计划；允许外层包裹代码块标记。
    /// 输入：`text`：模型返回的 JSON（应满足 `response_format = json_object`）。
    /// 输出/副作用：计划或中文错误；不修改工程。
    /// 复杂度：时间 O(文本长度)，空间 O(计划大小)。
    ///
    /// Module: ai / composition plan
    /// Purpose: Parse the JSON text returned by the model; tolerate surrounding code fences.
    /// Inputs: `text`: JSON returned by the model (`response_format = json_object`).
    /// Outputs / side effects: the plan or a Chinese error; no project change.
    /// Complexity: O(text length) time and O(plan size) space.
    pub fn parse(text: &str) -> Result<Self, String> {
        let trimmed = text.trim();
        let json = strip_code_fence(trimmed);
        serde_json::from_str(json)
            .map_err(|error| format!("AI 返回的编曲计划不是有效 JSON：{error}"))
    }

    /// 所属模块：ai / 编曲计划
    /// 功能：按顺序执行全部操作并累计摘要；任一操作失败立即返回错误。
    /// 输入：`project`：目标工程（调用方应先克隆，失败时丢弃副本）；`plugins`：已安装插件清单。
    /// 输出/副作用：修改 `project`；返回新增轨道/片段/音符/插件数量。
    /// 复杂度：时间 O(操作数 × (轨道数 + 音符数 + 插件数))，空间 O(1)（摘要）。
    ///
    /// Module: ai / composition plan
    /// Purpose: Execute every operation in order and accumulate a summary; the first failure aborts.
    /// Inputs: `project`: target project (callers clone first and discard the copy on failure);
    /// `plugins`: installed-plugin catalogue.
    /// Outputs / side effects: mutates `project`; returns the number of added tracks/clips/notes/plugins.
    /// Complexity: O(operations × (tracks + notes + plugins)) time and O(1) summary space.
    pub fn apply(
        &self,
        project: &mut Project,
        plugins: &[AiPluginInfo],
    ) -> Result<AiApplySummary, String> {
        let mut summary = AiApplySummary::default();
        for (index, operation) in self.operations.iter().enumerate() {
            operation
                .apply(project, plugins, &mut summary)
                .map_err(|error| format!("第 {} 个操作失败：{error}", index + 1))?;
        }
        Ok(summary)
    }
}

/// 一条编曲操作。
/// One composition operation.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AiOperation {
    /// 设置速度（BPM）。
    /// Set the tempo (BPM).
    SetTempo { bpm: f64 },
    /// 设置拍号。
    /// Set the time signature.
    SetTimeSignature { numerator: u8, denominator: u8 },
    /// 清空全部轨道；仅在用户明确要求时使用。
    /// Remove every track; only when the user explicitly asks.
    ClearProject,
    /// 新建轨道。
    /// Add a track.
    AddTrack {
        #[serde(default)]
        name: Option<String>,
        instrument: String,
        #[serde(default)]
        volume: Option<f32>,
        #[serde(default)]
        pan: Option<f32>,
    },
    /// 修改已有轨道的音量/声像/静音。
    /// Change volume/pan/mute of an existing track.
    SetTrack {
        track: String,
        #[serde(default)]
        volume: Option<f32>,
        #[serde(default)]
        pan: Option<f32>,
        #[serde(default)]
        mute: Option<bool>,
    },
    /// 在轨道上写入一个 MIDI 片段。
    /// Write one MIDI clip on a track.
    AddClip {
        track: String,
        start_beat: f64,
        length_beats: f64,
        #[serde(default)]
        notes: Vec<AiNote>,
    },
    /// 给轨道追加一个内置效果器。
    /// Append a built-in effect to a track.
    AddEffect { track: String, effect: String },
    /// 新建一条使用已安装 VST 乐器的轨道。
    /// Add a track using an installed VST instrument.
    AddPluginTrack {
        #[serde(default)]
        name: Option<String>,
        plugin: String,
        #[serde(default)]
        program: Option<i32>,
        #[serde(default)]
        volume: Option<f32>,
        #[serde(default)]
        pan: Option<f32>,
    },
    /// 把已有轨道的乐器替换为已安装 VST。
    /// Replace an existing track's instrument with an installed VST.
    SetPluginInstrument {
        track: String,
        plugin: String,
        #[serde(default)]
        program: Option<i32>,
    },
    /// 给轨道追加一个已安装的 VST 效果器。
    /// Append an installed VST effect to a track.
    AddPluginEffect {
        track: String,
        plugin: String,
        #[serde(default)]
        program: Option<i32>,
    },
}

impl AiOperation {
    /// 所属模块：ai / 编曲计划 / 操作
    /// 功能：执行单条操作并更新摘要。
    /// 输入：`project` 目标工程；`plugins` 已安装插件清单；`summary` 累计摘要。
    /// 输出/副作用：修改工程与摘要；参数非法时返回中文错误。
    /// 复杂度：时间 O(轨道数 + 本操作音符数 + 插件数)，空间 O(1)。
    ///
    /// Module: ai / composition plan / operation
    /// Purpose: Execute one operation and update the summary.
    /// Inputs: `project` target project; `plugins` installed-plugin catalogue; `summary` accumulated
    /// summary.
    /// Outputs / side effects: mutates the project and summary; invalid parameters return a Chinese error.
    /// Complexity: O(tracks + notes in this operation + plugins) time and O(1) space.
    pub fn apply(
        &self,
        project: &mut Project,
        plugins: &[AiPluginInfo],
        summary: &mut AiApplySummary,
    ) -> Result<(), String> {
        match self {
            AiOperation::SetTempo { bpm } => {
                if !bpm.is_finite() || *bpm <= 0.0 {
                    return Err("BPM 必须是正数".into());
                }
                project.bpm = bpm.clamp(40.0, 240.0);
                summary.tempo = Some(project.bpm);
            }
            AiOperation::SetTimeSignature {
                numerator,
                denominator,
            } => {
                if !(1..=12).contains(numerator) || ![1u8, 2, 4, 8, 16].contains(denominator) {
                    return Err("拍号超出范围（分子 1~12，分母 1/2/4/8/16）".into());
                }
                project.time_sig_num = *numerator;
                project.time_sig_den = *denominator;
                summary.time_signature = Some((*numerator, *denominator));
            }
            AiOperation::ClearProject => {
                project.tracks.clear();
                project.master_controllers = Default::default();
                project.loop_enabled = false;
                project.length_beats = 32.0;
                summary.cleared = true;
            }
            AiOperation::AddTrack {
                name,
                instrument,
                volume,
                pan,
            } => {
                let instrument_ref = resolve_instrument(instrument)?;
                let default_name = instrument_ref.label();
                let index = project.add_track(instrument_ref);
                let track = project
                    .tracks
                    .get_mut(index)
                    .ok_or_else(|| "新建轨道失败".to_string())?;
                let requested = name.as_deref().map(str::trim).unwrap_or_default();
                if !requested.is_empty() {
                    track.name = requested.to_string();
                } else if !default_name.is_empty() {
                    track.name = default_name;
                }
                if let Some(volume) = volume {
                    track.volume = volume.clamp(0.0, 1.5);
                }
                if let Some(pan) = pan {
                    track.pan = pan.clamp(-1.0, 1.0);
                }
                summary.tracks_added += 1;
            }
            AiOperation::SetTrack {
                track,
                volume,
                pan,
                mute,
            } => {
                let index = find_track(project, track)?;
                let target = &mut project.tracks[index];
                if let Some(volume) = volume {
                    target.volume = volume.clamp(0.0, 1.5);
                }
                if let Some(pan) = pan {
                    target.pan = pan.clamp(-1.0, 1.0);
                }
                if let Some(mute) = mute {
                    target.muted = *mute;
                }
                target.bump_revision();
                summary.tracks_updated += 1;
            }
            AiOperation::AddClip {
                track,
                start_beat,
                length_beats,
                notes,
            } => {
                if !start_beat.is_finite() || *start_beat < 0.0 || *start_beat > MAX_POSITION_BEATS
                {
                    return Err(format!("片段起点必须在 0~{MAX_POSITION_BEATS:.0} 拍之间"));
                }
                if !length_beats.is_finite()
                    || *length_beats <= 0.0
                    || *length_beats > MAX_POSITION_BEATS
                {
                    return Err(format!("片段时值必须在 0~{MAX_POSITION_BEATS:.0} 拍之间"));
                }
                let index = find_track(project, track)?;
                let clip_id = project.next_clip_id();
                let target = &mut project.tracks[index];
                if target.is_group() {
                    return Err(format!("「{}」是组合轨道，不能写入 MIDI 片段", target.name));
                }
                if target.kind == TrackKind::Audio && target.clips.is_empty() {
                    return Err(format!("「{}」是音频轨道，不能写入 MIDI 片段", target.name));
                }
                if target.has_audio_clips() {
                    return Err(format!(
                        "「{}」已经包含音频片段，不能再写入 MIDI",
                        target.name
                    ));
                }
                let mut clip = Clip::new(
                    clip_id,
                    format!("AI 片段 {}", target.name),
                    *start_beat,
                    *length_beats,
                );
                let mut clip_end = *length_beats;
                for note in notes {
                    let start = note.start.clamp(0.0, MAX_POSITION_BEATS);
                    let length = note.length.clamp(0.0625, MAX_POSITION_BEATS);
                    clip.notes.push(Note {
                        start,
                        length,
                        pitch: note.pitch,
                        velocity: note.velocity.clamp(1, 127),
                    });
                    clip_end = clip_end.max(start + length);
                }
                clip.length = clip_end.max(0.25);
                summary.clips_added += 1;
                summary.notes_added += clip.notes.len();
                target.clips.push(clip);
                target.bump_revision();
            }
            AiOperation::AddEffect { track, effect } => {
                let kind = resolve_effect(effect)
                    .ok_or_else(|| format!("不认识的内置效果器「{effect}」"))?;
                let index = find_track(project, track)?;
                let target = &mut project.tracks[index];
                target
                    .effects
                    .push(EffectRef::Native(NativeEffect::new(kind)));
                target.bump_revision();
                summary.effects_added += 1;
            }
            AiOperation::AddPluginTrack {
                name,
                plugin,
                program,
                volume,
                pan,
            } => {
                let info = resolve_plugin(plugin, plugins, true)?;
                let index = project.add_track(InstrumentRef::Plugin(plugin_ref(info, *program)));
                let track = project
                    .tracks
                    .get_mut(index)
                    .ok_or_else(|| "新建插件轨道失败".to_string())?;
                let requested = name.as_deref().map(str::trim).unwrap_or_default();
                track.name = if requested.is_empty() {
                    info.name.clone()
                } else {
                    requested.to_string()
                };
                if let Some(volume) = volume {
                    track.volume = volume.clamp(0.0, 1.5);
                }
                if let Some(pan) = pan {
                    track.pan = pan.clamp(-1.0, 1.0);
                }
                summary.tracks_added += 1;
                summary.plugins_added += 1;
            }
            AiOperation::SetPluginInstrument {
                track,
                plugin,
                program,
            } => {
                let info = resolve_plugin(plugin, plugins, true)?;
                let index = find_track(project, track)?;
                let target = &mut project.tracks[index];
                if target.is_group() {
                    return Err(format!("「{}」是组合轨道，不能设置乐器", target.name));
                }
                if target.has_audio_clips() {
                    return Err(format!(
                        "「{}」包含音频片段，不能替换为 VST 乐器",
                        target.name
                    ));
                }
                target.instrument = InstrumentRef::Plugin(plugin_ref(info, *program));
                target.bump_revision();
                summary.tracks_updated += 1;
                summary.plugins_added += 1;
            }
            AiOperation::AddPluginEffect {
                track,
                plugin,
                program,
            } => {
                let info = resolve_plugin(plugin, plugins, false)?;
                let index = find_track(project, track)?;
                let target = &mut project.tracks[index];
                target
                    .effects
                    .push(EffectRef::Plugin(plugin_ref(info, *program)));
                target.bump_revision();
                summary.effects_added += 1;
                summary.plugins_added += 1;
            }
        }
        Ok(())
    }

    /// 所属模块：ai / 编曲计划 / 操作
    /// 功能：返回给用户看的一行操作说明，用于流式写入时逐步展示。
    /// 输入：`self`。
    /// 输出/副作用：中文文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(文本长度)。
    ///
    /// Module: ai / composition plan / operation
    /// Purpose: Return a one-line Chinese description shown while operations stream into the project.
    /// Inputs: `self`.
    /// Outputs / side effects: Chinese text; no state change.
    /// Complexity: O(1) time and O(text length) space.
    pub fn describe(&self) -> String {
        match self {
            AiOperation::SetTempo { bpm } => format!("设置速度 {bpm:.0} BPM"),
            AiOperation::SetTimeSignature {
                numerator,
                denominator,
            } => format!("设置拍号 {numerator}/{denominator}"),
            AiOperation::ClearProject => "清空原有轨道".to_string(),
            AiOperation::AddTrack {
                name, instrument, ..
            } => format!(
                "新建轨道「{}」",
                name.as_deref()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .unwrap_or(instrument)
            ),
            AiOperation::SetTrack { track, .. } => format!("调整轨道「{track}」"),
            AiOperation::AddClip {
                track,
                length_beats,
                notes,
                ..
            } => format!(
                "写入「{track}」片段（{} 拍 / {} 音符）",
                length_beats.round(),
                notes.len()
            ),
            AiOperation::AddEffect { track, effect } => {
                format!("给「{track}」添加效果器 {effect}")
            }
            AiOperation::AddPluginTrack { plugin, .. } => {
                format!("加载 VST 乐器「{plugin}」")
            }
            AiOperation::SetPluginInstrument { track, plugin, .. } => {
                format!("把「{track}」换成 VST「{plugin}」")
            }
            AiOperation::AddPluginEffect { track, plugin, .. } => {
                format!("给「{track}」挂载 VST 效果器「{plugin}」")
            }
        }
    }
}

/// 所属模块：ai / 编曲计划 / 插件
/// 功能：按名称在已安装插件中查找：优先精确名称，其次“厂商 + 名称”，最后部分匹配；
/// 只接受与操作角色兼容的插件（未知类型两边都允许）。
/// 输入：
/// - `name`：模型给出的插件名，可带厂商；
/// - `plugins`：已安装插件清单；
/// - `want_instrument`：`true` 表示找乐器，`false` 表示找效果器。
/// 输出/副作用：插件条目或中文错误；不修改状态。
/// 复杂度：时间 O(插件数)，空间 O(候选名称长度)。
///
/// Module: ai / composition plan / plugins
/// Purpose: Find an installed plugin by name: exact name first, then "vendor name", then a partial
/// match; only role-compatible plugins are accepted (unknown roles are allowed on both sides).
/// Inputs:
/// - `name`: plugin name from the model, optionally with the vendor;
/// - `plugins`: installed-plugin catalogue;
/// - `want_instrument`: `true` for an instrument, `false` for an effect.
/// Outputs / side effects: the plugin entry or a Chinese error; no state change.
/// Complexity: O(plugins) time and O(candidate name length) space.
fn resolve_plugin<'a>(
    name: &str,
    plugins: &'a [AiPluginInfo],
    want_instrument: bool,
) -> Result<&'a AiPluginInfo, String> {
    if plugins.is_empty() {
        return Err("当前没有已扫描的 VST 插件；请先在「VST 插件」菜单扫描，或改用内置乐器".into());
    }
    let raw = name.trim();
    if raw.is_empty() {
        return Err("插件名称为空".into());
    }
    // 模型常写出 “Serum (64 bit)”“Pro-Q 4 VST3” 之类；去掉空白与标点后更容易命中。
    // Models often write names like “Serum (64 bit)” or “Pro-Q 4 VST3”; dropping whitespace and
    // punctuation makes matching tolerant.
    let text = normalise_plugin_name(raw);
    if text.is_empty() {
        return Err("插件名称为空".into());
    }
    let compatible = |info: &&AiPluginInfo| match info.kind {
        AiPluginKind::Unknown => true,
        AiPluginKind::Instrument => want_instrument,
        AiPluginKind::Effect => !want_instrument,
    };
    if let Some(info) = plugins
        .iter()
        .filter(compatible)
        .find(|info| normalise_plugin_name(&info.name) == text)
    {
        return Ok(info);
    }
    if let Some(info) = plugins.iter().filter(compatible).find(|info| {
        let vendor = normalise_plugin_name(&info.vendor);
        if vendor.is_empty() {
            return false;
        }
        let plugin_name = normalise_plugin_name(&info.name);
        text == format!("{vendor}{plugin_name}") || text == format!("{plugin_name}{vendor}")
    }) {
        return Ok(info);
    }
    let matches: Vec<&AiPluginInfo> = plugins
        .iter()
        .filter(compatible)
        .filter(|info| {
            let candidate = normalise_plugin_name(&info.name);
            !candidate.is_empty()
                && (candidate.contains(&text)
                    || (text.chars().count() >= 3 && text.contains(&candidate)))
        })
        .collect();
    match matches.as_slice() {
        [single] => Ok(*single),
        [] => {
            let names = plugins
                .iter()
                .filter(compatible)
                .take(8)
                .map(|info| info.name.as_str())
                .collect::<Vec<_>>()
                .join("、");
            Err(format!(
                "找不到{}「{name}」；可用插件：{names}",
                if want_instrument {
                    "乐器"
                } else {
                    "效果器"
                }
            ))
        }
        many => {
            let names = many
                .iter()
                .take(8)
                .map(|info| {
                    if info.vendor.is_empty() {
                        info.name.clone()
                    } else {
                        format!("{}（{}）", info.name, info.vendor)
                    }
                })
                .collect::<Vec<_>>()
                .join("、");
            Err(format!("插件「{name}」有多个匹配：{names}；请写上厂商"))
        }
    }
}

/// 所属模块：ai / 编曲计划 / 插件
/// 功能：把插件名规范化为小写字母数字串，忽略空格、括号、连字符与版本后缀。
/// 输入：`text`：模型或清单中的名称。
/// 输出/副作用：规范化文本；不修改输入。
/// 复杂度：时间 O(名称长度)，空间 O(名称长度)。
///
/// Module: ai / composition plan / plugins
/// Purpose: Normalise a plugin name into lower-case alphanumerics, ignoring spaces, brackets,
/// hyphens and version suffixes.
/// Inputs: `text`: name from the model or the catalogue.
/// Outputs / side effects: normalised text; the input is unchanged.
/// Complexity: O(name length) time and space.
fn normalise_plugin_name(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// 所属模块：ai / 编曲计划 / 插件
/// 功能：由清单条目构造 `PluginRef`；指定程序号时关闭状态恢复，避免空状态覆盖预设。
/// 输入：`info` 插件条目；`program` 工厂预设号。
/// 输出/副作用：插件引用；不修改状态。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: ai / composition plan / plugins
/// Purpose: Build a `PluginRef` from a catalogue entry; a program index disables state restore so an
/// empty state cannot override the preset.
/// Inputs: `info` plugin entry; `program` factory preset index.
/// Outputs / side effects: plugin reference; no state change.
/// Complexity: O(path length) time and space.
fn plugin_ref(info: &AiPluginInfo, program: Option<i32>) -> PluginRef {
    let mut plugin = PluginRef::new(info.format, info.path.clone(), info.name.clone());
    plugin.uid = info.uid.clone();
    plugin.program = program;
    if program.is_some() {
        plugin.restore_state = false;
    }
    plugin
}

/// 一个 MIDI 音符：`[音高, 起始拍, 时值, 力度]`（力度可省略，默认 88）。
/// One MIDI note: `[pitch, start, length, velocity]` (velocity may be omitted, default 88).
#[derive(Clone, Copy, Debug)]
pub struct AiNote {
    pub pitch: u8,
    pub start: f64,
    pub length: f64,
    pub velocity: u8,
}

impl<'de> Deserialize<'de> for AiNote {
    /// 所属模块：ai / 编曲计划 / 音符
    /// 功能：接受 `[音高, 起始拍, 时值]` 或 `[音高, 起始拍, 时值, 力度]`，音高可为 MIDI 数字或音名。
    /// 输入：反序列化器。
    /// 输出/副作用：音符或错误；不修改状态。
    /// 复杂度：时间 O(字段数)，空间 O(1)。
    ///
    /// Module: ai / composition plan / note
    /// Purpose: Accept `[pitch, start, length]` or `[pitch, start, length, velocity]`; pitch may be a
    /// MIDI number or a note name.
    /// Inputs: deserializer.
    /// Outputs / side effects: the note or an error; no state change.
    /// Complexity: O(fields) time and O(1) space.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = Vec::<Value>::deserialize(deserializer)?;
        if !(3..=4).contains(&raw.len()) {
            return Err(serde::de::Error::custom(
                "音符必须是 [音高, 起始拍, 时值] 或 [音高, 起始拍, 时值, 力度]",
            ));
        }
        let pitch = parse_pitch(&raw[0]).map_err(serde::de::Error::custom)?;
        let start = number(&raw[1], "起始拍").map_err(serde::de::Error::custom)?;
        let length = number(&raw[2], "时值").map_err(serde::de::Error::custom)?;
        let velocity = raw
            .get(3)
            .map(|value| number(value, "力度"))
            .transpose()
            .map_err(serde::de::Error::custom)?
            .unwrap_or(88.0)
            .clamp(1.0, 127.0) as u8;
        Ok(Self {
            pitch,
            start,
            length,
            velocity,
        })
    }
}

/// 计划写入结果摘要。
/// Summary of what a plan wrote.
#[derive(Clone, Copy, Debug, Default)]
pub struct AiApplySummary {
    pub tracks_added: usize,
    pub tracks_updated: usize,
    pub clips_added: usize,
    pub notes_added: usize,
    pub effects_added: usize,
    pub plugins_added: usize,
    pub cleared: bool,
    pub tempo: Option<f64>,
    pub time_signature: Option<(u8, u8)>,
}

impl AiApplySummary {
    /// 所属模块：ai / 编曲计划 / 摘要
    /// 功能：生成状态栏与对话气泡使用的中文摘要。
    /// 输入：`self`。
    /// 输出/副作用：摘要文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / composition plan / summary
    /// Purpose: Build the Chinese summary used by the status bar and chat bubble.
    /// Inputs: `self`.
    /// Outputs / side effects: summary text; no state change.
    /// Complexity: O(1) time and space.
    pub fn summary_text(&self) -> String {
        let mut parts = Vec::new();
        if self.cleared {
            parts.push("清空原有轨道".to_string());
        }
        if let Some(bpm) = self.tempo {
            parts.push(format!("速度 {bpm:.0} BPM"));
        }
        if let Some((numerator, denominator)) = self.time_signature {
            parts.push(format!("拍号 {numerator}/{denominator}"));
        }
        if self.tracks_added > 0 {
            parts.push(format!("新建 {} 条轨道", self.tracks_added));
        }
        if self.tracks_updated > 0 {
            parts.push(format!("调整 {} 条轨道", self.tracks_updated));
        }
        if self.clips_added > 0 {
            parts.push(format!(
                "写入 {} 个片段 / {} 个音符",
                self.clips_added, self.notes_added
            ));
        }
        if self.effects_added > 0 {
            parts.push(format!("添加 {} 个效果器", self.effects_added));
        }
        if self.plugins_added > 0 {
            parts.push(format!("加载 {} 个 VST 插件", self.plugins_added));
        }
        if parts.is_empty() {
            "没有修改工程".into()
        } else {
            parts.join("，")
        }
    }
}

/// 所属模块：ai / 编曲计划 / 解析
/// 功能：去掉模型偶尔附带的 ```json 代码块标记。
/// 输入：`text`：可能带代码块标记的 JSON 文本。
/// 输出/副作用：内部 JSON 文本；不修改输入。
/// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
///
/// Module: ai / composition plan / parsing
/// Purpose: Remove the ```json code fence the model occasionally adds.
/// Inputs: `text`: JSON that may carry a code fence.
/// Outputs / side effects: inner JSON text; the input is unchanged.
/// Complexity: O(text length) time and space.
fn strip_code_fence(text: &str) -> &str {
    let trimmed = text.trim();
    let without_open = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```JSON"))
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .trim_start();
    without_open
        .strip_suffix("```")
        .unwrap_or(without_open)
        .trim()
}

/// 所属模块：ai / 编曲计划 / 音高
/// 功能：把 JSON 值解析为 MIDI 音高：数字直接取整，字符串按音名解析。
/// 输入：`value`：数字或音名（如 `"C4"`、`"F#3"`、`"Bb5"`）。
/// 输出/副作用：0~127 的音高或中文错误；不修改输入。
/// 复杂度：时间 O(音名长度)，空间 O(1)。
///
/// Module: ai / composition plan / pitch
/// Purpose: Parse a JSON value into a MIDI pitch: numbers are rounded, strings are note names.
/// Inputs: `value`: number or note name (`"C4"`, `"F#3"`, `"Bb5"`).
/// Outputs / side effects: pitch 0..=127 or a Chinese error; the input is unchanged.
/// Complexity: O(note-name length) time and O(1) space.
pub fn parse_pitch(value: &Value) -> Result<u8, String> {
    if let Some(number) = value.as_f64() {
        if !number.is_finite() {
            return Err("音高不是有效数字".into());
        }
        return Ok(number.round().clamp(0.0, 127.0) as u8);
    }
    let Some(name) = value.as_str() else {
        return Err("音高必须是 MIDI 数字或音名".into());
    };
    parse_pitch_name(name)
}

/// 所属模块：ai / 编曲计划 / 音高
/// 功能：解析音名到 MIDI 音高；八度省略时按第 4 八度处理，`C4` = 60。
/// 输入：`name`：音名文本。
/// 输出/副作用：0~127 的音高或中文错误；不修改输入。
/// 复杂度：时间 O(音名长度)，空间 O(1)。
///
/// Module: ai / composition plan / pitch
/// Purpose: Parse a note name into a MIDI pitch; a missing octave means octave 4 and `C4` = 60.
/// Inputs: `name`: note-name text.
/// Outputs / side effects: pitch 0..=127 or a Chinese error; the input is unchanged.
/// Complexity: O(note-name length) time and O(1) space.
fn parse_pitch_name(name: &str) -> Result<u8, String> {
    let text = name.trim();
    let mut chars = text.chars();
    let letter = chars
        .next()
        .ok_or_else(|| "音名为空".to_string())?
        .to_ascii_uppercase();
    let base = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return Err(format!("无法识别的音名「{name}」")),
    };
    let rest: String = chars.collect();
    let mut accidental = 0i32;
    let mut octave_text = rest.as_str();
    if octave_text.starts_with('#') {
        accidental = 1;
        octave_text = &octave_text[1..];
    } else if octave_text.starts_with('b') || octave_text.starts_with('♭') {
        accidental = -1;
        octave_text = octave_text.trim_start_matches('b').trim_start_matches('♭');
    }
    let octave = if octave_text.is_empty() {
        4
    } else {
        octave_text
            .parse::<i32>()
            .map_err(|_| format!("无法识别的音名「{name}」"))?
    };
    let midi = (octave + 1) * 12 + base + accidental;
    Ok(midi.clamp(0, 127) as u8)
}

/// 所属模块：ai / 编曲计划 / 音高
/// 功能：把 MIDI 音高转为音名（`C4` = 60），用于工程提示词。
/// 输入：`pitch`：0~127 的 MIDI 音高。
/// 输出/副作用：音名文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / composition plan / pitch
/// Purpose: Convert a MIDI pitch into a note name (`C4` = 60) for the project prompt.
/// Inputs: `pitch`: MIDI pitch 0..=127.
/// Outputs / side effects: note-name text; no state change.
/// Complexity: O(1) time and space.
pub fn pitch_name(pitch: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = pitch as i32 / 12 - 1;
    format!("{}{}", NAMES[(pitch % 12) as usize], octave)
}

/// 所属模块：ai / 编曲计划 / 名称解析
/// 功能：把模型给出的乐器名解析为内置乐器：优先采样音色库 id / 中文名，
/// 其次原生乐器键，最后回退内置合成器。
/// 输入：`name`：音色库 id、中文名或 `drums` / `synth`。
/// 输出/副作用：乐器引用或中文错误；不修改状态。
/// 复杂度：时间 O(清单大小)，空间 O(1)。
///
/// Module: ai / composition plan / name resolution
/// Purpose: Resolve a model-provided instrument name: sample-bank id or Chinese name first, then
/// native instrument keys, and finally the built-in synth.
/// Inputs: `name`: bank id, Chinese name, or `drums` / `synth`.
/// Outputs / side effects: instrument reference or a Chinese error; no state change.
/// Complexity: O(manifest size) time and O(1) space.
pub fn resolve_instrument(name: &str) -> Result<InstrumentRef, String> {
    let text = name.trim();
    if text.is_empty() {
        return Err("乐器名称为空".into());
    }
    let lower = text.to_lowercase();
    if matches!(lower.as_str(), "drums" | "drum" | "drumkit" | "鼓" | "鼓组") {
        return Ok(InstrumentRef::Drums(Default::default()));
    }
    if matches!(lower.as_str(), "synth" | "合成器") {
        return Ok(InstrumentRef::Synth(SynthSettings::default()));
    }
    for entry in bank_instruments() {
        if entry.id.eq_ignore_ascii_case(text) || entry.name == text {
            return Ok(InstrumentRef::Sample(SampleInstrument::new(&entry.id)));
        }
    }
    for kind in NativeInstrumentKind::ALL {
        if kind.key() == lower || kind.label() == text {
            return Ok(InstrumentRef::Native(NativeInstrument::new(kind)));
        }
    }
    Err(format!(
        "未知乐器「{name}」，请使用乐器清单中的 id 或中文名"
    ))
}

/// 所属模块：ai / 编曲计划 / 名称解析
/// 功能：把中英文效果器名解析为内置原生效果器。
/// 输入：`name`：如 `reverb` / `混响` / `delay` / `压缩`。
/// 输出/副作用：效果器类型或 `None`；不修改状态。
/// 复杂度：时间 O(名称长度)，空间 O(1)。
///
/// Module: ai / composition plan / name resolution
/// Purpose: Resolve a Chinese or English effect name into a built-in native effect.
/// Inputs: `name`: e.g. `reverb` / `混响` / `delay` / `压缩`.
/// Outputs / side effects: effect kind or `None`; no state change.
/// Complexity: O(name length) time and O(1) space.
pub fn resolve_effect(name: &str) -> Option<NativeEffectKind> {
    let lower = name.trim().to_lowercase();
    let kind = match lower.as_str() {
        "overdrive" | "过载" => NativeEffectKind::Overdrive,
        "distortion" | "失真" => NativeEffectKind::Distortion,
        "fuzz" | "法兹" => NativeEffectKind::Fuzz,
        "chorus" | "合唱" => NativeEffectKind::Chorus,
        "phaser" | "相位" => NativeEffectKind::Phaser,
        "delay" | "延时" | "echo" => NativeEffectKind::Delay,
        "reverb" | "混响" | "hall" | "plate" => NativeEffectKind::Reverb,
        "compressor" | "compress" | "压缩" => NativeEffectKind::Compressor,
        _ => return None,
    };
    Some(kind)
}

/// 所属模块：ai / 编曲计划 / 轨道查找
/// 功能：按轨道 id、名称（忽略大小写与首尾空白）查找轨道；名称可部分匹配。
/// 输入：`project` 工程；`key` 模型给出的轨道名或 id。
/// 输出/副作用：轨道下标或中文错误；不修改工程。
/// 复杂度：时间 O(轨道数)，空间 O(1)。
///
/// Module: ai / composition plan / track lookup
/// Purpose: Find a track by id or name (case-insensitive, trimmed); names may match partially.
/// Inputs: `project` project; `key` track name or id from the model.
/// Outputs / side effects: track index or a Chinese error; no project change.
/// Complexity: O(tracks) time and O(1) space.
pub fn find_track(project: &Project, key: &str) -> Result<usize, String> {
    let text = key.trim();
    if text.is_empty() {
        return Err("没有指定轨道".into());
    }
    if let Ok(id) = text.parse::<u64>() {
        if let Some(index) = project.track_index_of(id) {
            return Ok(index);
        }
    }
    let lower = text.to_lowercase();
    if let Some(index) = project
        .tracks
        .iter()
        .position(|track| track.name.to_lowercase() == lower)
    {
        return Ok(index);
    }
    if let Some(index) = project
        .tracks
        .iter()
        .position(|track| track.name.to_lowercase().contains(&lower))
    {
        return Ok(index);
    }
    let names = project
        .tracks
        .iter()
        .map(|track| track.name.as_str())
        .collect::<Vec<_>>()
        .join("、");
    Err(format!("找不到轨道「{key}」；当前轨道：{names}"))
}

/// 所属模块：ai / 编曲计划 / 数值
/// 功能：把 JSON 值转为有限浮点数，非数字时报错。
/// 输入：`value` JSON 值；`what` 字段中文名（用于错误文本）。
/// 输出/副作用：数值或中文错误；不修改输入。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / composition plan / number
/// Purpose: Convert a JSON value into a finite float; non-numbers error.
/// Inputs: `value` JSON value; `what` Chinese field name for the error text.
/// Outputs / side effects: the number or a Chinese error; the input is unchanged.
/// Complexity: O(1) time and space.
fn number(value: &Value, what: &str) -> Result<f64, String> {
    let number = value.as_f64().ok_or_else(|| format!("{what}必须是数字"))?;
    if !number.is_finite() {
        return Err(format!("{what}必须是有限数字"));
    }
    Ok(number)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：音名与 MIDI 编号互转，八度省略时按第 4 八度处理。
    /// Covers: note-name/MIDI conversion with octave 4 as the default.
    #[test]
    fn pitch_names_round_trip() {
        assert_eq!(parse_pitch(&Value::from(60)).expect("number"), 60);
        assert_eq!(parse_pitch(&Value::from("C4")).expect("name"), 60);
        assert_eq!(parse_pitch(&Value::from("F#3")).expect("sharp"), 54);
        assert_eq!(parse_pitch(&Value::from("Bb5")).expect("flat"), 82);
        assert_eq!(parse_pitch(&Value::from("C")).expect("default octave"), 60);
        assert_eq!(pitch_name(60), "C4");
        assert_eq!(pitch_name(61), "C#4");
        assert!(parse_pitch(&Value::from("H2")).is_err());
        assert!(parse_pitch(&Value::from(true)).is_err());
    }

    /// 覆盖：计划按顺序执行，新建轨道后可用名称写片段。
    /// Covers: operations run in order and a new track can be addressed by name.
    #[test]
    fn plan_applies_tracks_and_clips() {
        let mut project = Project::default();
        project.tracks.clear();
        let plan = AiPlan::parse(
            r#"{
              "reply":"ok",
              "operations":[
                {"op":"set_tempo","bpm":72},
                {"op":"add_track","name":"钢琴","instrument":"GrandPiano"},
                {"op":"add_clip","track":"钢琴","start_beat":0,"length_beats":8,
                 "notes":[[60,0,2,80],["E4",2,2,70],["G4",4,2,90],["C5",6,2,100]]},
                {"op":"add_effect","track":"钢琴","effect":"reverb"}
              ]
            }"#,
        )
        .expect("plan");
        let summary = plan.apply(&mut project, &[]).expect("apply");
        assert_eq!(project.bpm, 72.0);
        assert_eq!(project.tracks.len(), 1);
        assert_eq!(project.tracks[0].clips.len(), 1);
        assert_eq!(project.tracks[0].clips[0].notes.len(), 4);
        assert_eq!(project.tracks[0].clips[0].notes[3].pitch, 72);
        assert_eq!(project.tracks[0].effects.len(), 1);
        assert_eq!(summary.notes_added, 4);
        assert_eq!(summary.effects_added, 1);
    }

    /// 覆盖：未知轨道与非法音高会拒绝整条计划。
    /// Covers: an unknown track or invalid pitch rejects the plan.
    #[test]
    fn plan_rejects_invalid_operations() {
        let mut project = Project::empty();
        let plan = AiPlan::parse(
            r#"{"operations":[{"op":"add_clip","track":"不存在","start_beat":0,"length_beats":4,"notes":[[60,0,1,80]]}]}"#,
        )
        .expect("plan");
        assert!(plan.apply(&mut project, &[]).is_err());
        let bad_note = AiPlan::parse(
            r#"{"operations":[{"op":"add_clip","track":"MIDI 轨道 1","start_beat":0,"length_beats":4,"notes":[["H9",0,1,80]]}]}"#,
        );
        assert!(bad_note.is_err());
    }

    /// 覆盖：代码块包裹的 JSON 也能解析。
    /// Covers: JSON wrapped in a code fence still parses.
    #[test]
    fn plan_parses_fenced_json() {
        let plan =
            AiPlan::parse("```json\n{\"reply\":\"hi\",\"operations\":[]}\n```").expect("plan");
        assert_eq!(plan.reply, "hi");
        assert!(plan.operations.is_empty());
    }

    /// 覆盖：乐器与效果器名称解析。
    /// Covers: instrument and effect name resolution.
    #[test]
    fn instrument_and_effect_names_resolve() {
        assert!(matches!(
            resolve_instrument("GrandPiano").expect("piano"),
            InstrumentRef::Sample(_)
        ));
        assert!(matches!(
            resolve_instrument("鼓组").expect("drums"),
            InstrumentRef::Drums(_)
        ));
        assert!(resolve_instrument("不存在的乐器").is_err());
        assert_eq!(resolve_effect("混响"), Some(NativeEffectKind::Reverb));
        assert_eq!(resolve_effect("unknown"), None);
    }

    /// 覆盖：已安装 VST 乐器与效果器可以按名称写入工程。
    /// Covers: installed VST instruments and effects are written by name.
    #[test]
    fn plugin_operations_resolve_installed_plugins() {
        let plugins = vec![
            AiPluginInfo {
                name: "Serum".into(),
                vendor: "Xfer Records".into(),
                category: "Synth".into(),
                description: "波表合成器".into(),
                format: PluginFormat::Vst3,
                path: std::path::PathBuf::from("C:/plugins/Serum.vst3"),
                uid: "serum-uid".into(),
                kind: AiPluginKind::Instrument,
                has_midi_input: true,
            },
            AiPluginInfo {
                name: "Pro-Q 4".into(),
                vendor: "FabFilter".into(),
                category: "EQ".into(),
                description: "动态均衡器".into(),
                format: PluginFormat::Vst3,
                path: std::path::PathBuf::from("C:/plugins/Pro-Q 4.vst3"),
                uid: "proq-uid".into(),
                kind: AiPluginKind::Effect,
                has_midi_input: false,
            },
        ];
        let plan = AiPlan::parse(
            r#"{"operations":[
                {"op":"add_plugin_track","name":"Pad","plugin":"Serum"},
                {"op":"add_clip","track":"Pad","start_beat":0,"length_beats":4,"notes":[[60,0,1,80]]},
                {"op":"add_plugin_effect","track":"Pad","plugin":"FabFilter Pro-Q 4"}
            ]}"#,
        )
        .expect("plan");
        let mut project = Project::default();
        project.tracks.clear();
        let summary = plan.apply(&mut project, &plugins).expect("apply");
        assert!(matches!(
            project.tracks[0].instrument,
            InstrumentRef::Plugin(_)
        ));
        assert!(matches!(
            project.tracks[0].effects.first(),
            Some(EffectRef::Plugin(_))
        ));
        assert_eq!(summary.plugins_added, 2);
        assert_eq!(summary.notes_added, 1);
        // 模型常带版本后缀或厂商名：规范化后仍应命中。
        // Models often add version suffixes or vendors: normalised matching must still resolve.
        assert!(resolve_plugin("Serum (64 bit)", &plugins, true).is_ok());
        assert!(resolve_plugin("Xfer Records Serum", &plugins, true).is_ok());
        assert!(resolve_plugin("Pro-Q 4 VST3", &plugins, false).is_ok());
    }

    /// 覆盖：没有插件清单时插件操作会被拒绝。
    /// Covers: plugin operations are rejected without an installed-plugin catalogue.
    #[test]
    fn plugin_operations_need_a_catalogue() {
        let plan = AiPlan::parse(r#"{"operations":[{"op":"add_plugin_track","plugin":"Serum"}]}"#)
            .expect("plan");
        let mut project = Project::default();
        assert!(plan.apply(&mut project, &[]).is_err());
    }
}
