//! 所属模块：ai / 提示词
//! 文件用途：
//! - 拼接编曲智能体的系统提示词：输出协议、工程快照、附件、乐器清单、可编辑资料库与技能。
//! - 把当前工程压缩成模型可读的文本（轨道、片段、音域、已有音符与可调参数）。
//!
//! 函数清单：
//! - `AiAttachment`：用户附加的文本/MIDI 内容。
//! - `AiPromptSelection`：输入框选择的技能与可用/必须使用的乐器。
//! - `AiPluginParamInfo` / `AiPluginParamEntry`：插件参数快照。
//! - `system_prompt`：生成完整系统提示词。
//! - `project_context`：生成当前工程摘要、已有音符与内置参数。
//! - `instrument_constraints`：可用/必须使用的乐器约束。
//! - `plugin_param_context`：插件参数清单。
//! - `attachment_context`：生成附件摘要。
//! - `note_line` / `format_note`：音符文本行。
//!
//! Module: ai / prompts
//! Purpose:
//! - Build the composition agent's system prompt: output contract, project snapshot, attachments,
//!   instrument catalogue, editable library and skills.
//! - Compress the current project into model-readable text (tracks, clips, ranges, existing notes and
//!   adjustable parameters).
//!
//! Functions:
//! - `AiAttachment`: user-attached text/MIDI content.
//! - `AiPromptSelection`: composer-selected skills and usable/required instruments.
//! - `AiPluginParamInfo` / `AiPluginParamEntry`: plugin parameter snapshot.
//! - `system_prompt`: build the full system prompt.
//! - `project_context`: build the project summary, existing notes and native parameters.
//! - `instrument_constraints`: usable/required instrument rules.
//! - `plugin_param_context`: plugin parameter catalogue.
//! - `attachment_context`: build the attachment summary.
//! - `note_line` / `format_note`: note text lines.

use super::library::AiLibrary;
use super::plan::{AiPluginInfo, AiPluginKind, pitch_name};
use super::skills::instrument_catalog;
use crate::core::model::{
    ControllerKind, EffectRef, InstrumentRef, NativeEffectParam, Project, Track,
};

/// 每个片段最多写入提示词的音符数，避免长工程占满上下文。
/// Maximum notes per clip included in the prompt so long projects do not fill the context.
const NOTES_PER_CLIP: usize = 96;
/// 全部轨道最多写入提示词的音符数。
/// Maximum notes across all tracks included in the prompt.
const NOTES_TOTAL: usize = 720;
/// 插件参数清单最多列出的槽位数。
/// Maximum plugin slots listed in the parameter catalogue.
const MAX_PLUGIN_SLOTS: usize = 12;
/// 每个插件槽位最多列出的参数数。
/// Maximum parameters listed per plugin slot.
const MAX_PLUGIN_PARAMS: usize = 32;
/// 工程参数清单最多列出的轨道数。
/// Maximum tracks listed in the native parameter catalogue.
const MAX_PARAM_TRACKS: usize = 24;

/// 用户附加的文本内容（MIDI 附件、轨道或片段复制已转换为文本）。
/// User-attached text content (MIDI, track and clip copies are converted to text).
#[derive(Clone, Debug)]
pub struct AiAttachment {
    pub name: String,
    pub text: String,
}

/// 输入框中显式选择的技能与乐器约束。
/// Skills and instrument constraints explicitly selected in the composer.
#[derive(Clone, Debug, Default)]
pub struct AiPromptSelection {
    /// 额外加载的基础技能 id。
    /// Additional theory-skill ids.
    pub skill_ids: Vec<String>,
    /// 额外加载的编曲技巧 id。
    /// Additional technique ids.
    pub technique_ids: Vec<String>,
    /// 用户是否手动改过技能选择；为假时由智能体默认技能兜底。
    /// Whether the user edited skill picks; when false the agent defaults apply.
    pub skills_customized: bool,
    /// 只允许使用的乐器（id / 名称 / VST 名称）；空表示不限制。
    /// Instruments the model may use (ids/names/VST names); empty means unrestricted.
    pub usable_instruments: Vec<String>,
    /// 必须至少使用一次的乐器。
    /// Instruments that must be used at least once.
    pub required_instruments: Vec<String>,
}

/// 一个插件的可调参数快照。
/// Adjustable-parameter snapshot of one plugin slot.
#[derive(Clone, Debug, Default)]
pub struct AiPluginParamInfo {
    /// 轨道显示名。
    /// Track display name.
    pub track: String,
    /// `instrument` 或 `effect`。
    /// `instrument` or `effect`.
    pub slot: String,
    /// 效果器槽位下标（`slot = effect` 时有效）。
    /// Effect-slot index (valid when `slot = effect`).
    pub effect_index: Option<usize>,
    /// 插件显示名。
    /// Plugin display name.
    pub plugin: String,
    pub params: Vec<AiPluginParamEntry>,
}

/// 一个插件参数的快照（归一化 0~1）。
/// Snapshot of one plugin parameter (normalized 0..=1).
#[derive(Clone, Debug)]
pub struct AiPluginParamEntry {
    pub index: usize,
    pub name: String,
    pub value: f32,
    pub display: String,
}

/// 所属模块：ai / 提示词
/// 功能：生成完整的系统提示词：角色、输出 JSON 协议、工程快照、附件、乐器/插件清单与技能。
/// 输入：
/// - `library`：主页 Agent 栏维护的可编辑资料库。
/// - `agent_id`：当前智能体 id；未知 id 回退到第一个智能体。
/// - `selection`：输入框选择的技能与可用/必须使用的乐器。
/// - `project`：当前工程（只读）。
/// - `attachments`：用户附加的文本/MIDI/轨道或片段副本。
/// - `plugins`：已安装 VST 插件清单（不含本机路径）。
/// - `plugin_params`：插件参数快照。
/// 输出/副作用：系统提示词；不修改工程。
/// 复杂度：时间 O(工程大小 + 技能长度 + 附件长度 + 插件数)，空间 O(同)。
///
/// Module: ai / prompts
/// Purpose: Build the full system prompt: role, JSON output contract, project snapshot, attachments,
/// instrument/plugin catalogues and skills.
/// Inputs:
/// - `library`: editable library maintained by the home Agent column.
/// - `agent_id`: current agent id; unknown ids fall back to the first agent.
/// - `selection`: composer-selected skills and usable/required instruments.
/// - `project`: current project (read-only).
/// - `attachments`: user text/MIDI/track/clip copies.
/// - `plugins`: installed VST catalogue (no local paths).
/// - `plugin_params`: plugin parameter snapshots.
/// Outputs / side effects: the system prompt; no project change.
/// Complexity: O(project + skills + attachments + plugins) time and space.
pub fn system_prompt(
    library: &AiLibrary,
    agent_id: &str,
    selection: &AiPromptSelection,
    project: &Project,
    attachments: &[AiAttachment],
    plugins: &[AiPluginInfo],
    plugin_params: &[AiPluginParamInfo],
) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "你是 SurfingStudio 内置的 AI 编曲助手。你可以通过结构化操作把 MIDI 直接写入当前工程，\
         可以调用已安装的 VST 乐器/效果器、调整乐器与效果器参数、写入控制器自动化，\
         也可以在需要时先联网检索插件资料。你的回复要简洁、专业、可执行。\n\n",
    );
    prompt.push_str("## 输出协议（必须严格遵守）\n");
    prompt.push_str(
        "只返回一个 JSON 对象，不要使用 Markdown 代码块，不要输出任何 JSON 以外的文字。结构如下：\n\
         {\n\
           \"reply\": \"给用户的 1~3 句中文说明，不要复述 JSON\",\n\
           \"research\": [],\n\
           \"skills\": [],\n\
           \"operations\": [ ... ]\n\
         }\n\
         字段顺序必须是 reply → research → skills → operations，宿主会边接收 `operations` 边写入工程。\n\
         如果没有需要修改工程的内容，`operations` 返回空数组。\n\
         如果你不确定某个已安装 VST 插件的音色、用途或适合的参数，把 1~3 条英文检索词放进 `research`，\
         并让 `operations` 为空；宿主会联网搜索后再次调用你，届时必须返回最终计划且 `research` 为空。\n\
         如果需要某个编曲技巧的具体写法，把技巧 id 放进 `skills`（最多 4 个），并让 `operations` 为空；\
         宿主会加载技巧正文后再次调用你，届时 `skills` 必须为空。\n\
         不需要检索或加载技巧时，`research` 与 `skills` 都必须是空数组。\n\n",
    );
    prompt.push_str("### 可用操作\n");
    prompt.push_str(
        "- `{\"op\":\"set_tempo\",\"bpm\":84}`：设置速度（40~240）。\n\
         - `{\"op\":\"set_time_signature\",\"numerator\":4,\"denominator\":4}`：设置拍号。\n\
         - `{\"op\":\"clear_project\"}`：删除全部轨道。**只有用户明确要求“清空/重新开始”时才使用。**\n\
         - `{\"op\":\"add_track\",\"name\":\"钢琴\",\"instrument\":\"GrandPiano\",\"volume\":0.85,\"pan\":-0.1}`：新建内置乐器轨道。\n\
         - `{\"op\":\"set_track\",\"track\":\"钢琴\",\"volume\":0.8,\"pan\":-0.15,\"mute\":false}`：修改已有轨道。\n\
         - `{\"op\":\"add_clip\",\"track\":\"钢琴\",\"start_beat\":0,\"length_beats\":32,\"notes\":[[60,0,2,72],[\"E4\",2,2,78]]}`：写入 MIDI 片段。\n\
         - `{\"op\":\"add_effect\",\"track\":\"钢琴\",\"effect\":\"reverb\"}`：添加内置效果器（reverb/delay/chorus/compressor/overdrive/distortion/fuzz/phaser）。\n\
         - `{\"op\":\"add_plugin_track\",\"name\":\"Pad\",\"plugin\":\"Serum\",\"program\":0,\"volume\":0.8}`：用已安装 VST 乐器新建轨道。\n\
         - `{\"op\":\"set_plugin_instrument\",\"track\":\"Pad\",\"plugin\":\"Serum\",\"program\":0}`：把已有轨道的乐器换成 VST。\n\
         - `{\"op\":\"add_plugin_effect\",\"track\":\"Pad\",\"plugin\":\"Pro-Q 4\"}`：给轨道添加已安装 VST 效果器。\n\
         - `{\"op\":\"set_instrument_params\",\"track\":\"Pad\",\"params\":{\"cutoff\":4200,\"attack\":0.02,\"level\":0.8}}`：修改内置乐器参数（键名见“乐器与效果器参数”）。\n\
         - `{\"op\":\"set_effect_params\",\"track\":\"Pad\",\"index\":0,\"enabled\":true,\"params\":{\"mix\":0.35,\"time\":0.5}}`：修改内置效果器参数或旁路；也可用 `effect` 名称代替 `index`。\n\
         - `{\"op\":\"set_plugin_params\",\"track\":\"Pad\",\"slot\":\"instrument\",\"params\":[{\"index\":3,\"value\":0.42}]}`：按参数序号修改 VST 乐器参数（0~1）。\n\
         - `{\"op\":\"set_plugin_params\",\"track\":\"Pad\",\"slot\":\"effect\",\"effect\":0,\"params\":[{\"index\":12,\"value\":0.6}]}`：修改 VST 效果器参数。\n\
         - `{\"op\":\"set_controller\",\"track\":\"Pad\",\"controller\":\"volume\",\"points\":[{\"beat\":0,\"value\":0.8},{\"beat\":8,\"value\":0.5}],\"replace\":true}`：写入控制器子轨；控制器可为 volume / pan / sustain / pitch_bend，值 0~1。\n\n",
    );
    prompt.push_str("### 音符数组\n");
    prompt.push_str(
        "`[音高, 起始拍, 时值, 力度]`，`起始拍` 相对片段开头，`时值` 单位为拍，力度 1~127（可省略，默认 88）。\n\
         音高可写 MIDI 数字或音名：`60`、`\"C4\"`、`\"F#3\"`、`\"Bb5\"`；`C4` = 60。\n\n",
    );
    prompt.push_str("### 硬性规则\n");
    prompt.push_str(
        "1. 内置乐器只能使用下方“乐器清单”中的 id 或中文名；不要虚构音色。\n\
         2. VST 插件只能使用下方“已安装 VST 插件”中的名称；可以写 `名称` 或 `厂商 名称`，不要写本机路径。\n\
         3. **音色优先 VST**：只要“已安装 VST 插件”不为空，用户想要的音色/乐器/效果（合成器、Pad、Lead、钢琴、\
            鼓、贝斯、吉他、人声、混响、延迟、EQ、压缩、失真等）必须优先用清单中的 VST 乐器/效果器，\
            用 `add_plugin_track` / `set_plugin_instrument` / `add_plugin_effect` 落地；只有清单里没有对应角色、\
            或用户明确要求内置音色时，才用内置乐器。`reply` 要说明用了哪个插件。\n\
         4. 一条轨道只写一种乐器；不同乐器分轨。\n\
         5. 音符与片段必须落在乐器音域内；旋律优先舒适音区。\n\
         6. 默认 4/4、8~32 小节；用户指定速度/调性/风格时以用户为准。\n\
         7. 修改已有工程时优先在现有轨道上新增片段，或按名称引用轨道；不要随意删除用户的轨道。\n\
         8. 所有操作按数组顺序执行；先 `set_tempo` / 建轨 / 换插件，再 `add_clip`，最后加效果器。\n\
         9. 遵循所选智能体的风格技能：轻音乐要通透、不要过重；J-Pop 可以更满、更强，但不要所有轨道同时抢主旋律。\n\
         10. 如果用户只是提问或讨论，`operations` 保持空数组，直接回答。\n\
         11. 只有确实需要插件资料时才使用 `research`；只有需要某个技巧的详细写法时才使用 `skills`。\n\
         12. `operations` 会被边生成边执行：请把独立的轨道/片段操作写成多个对象，不要把整首歌塞进一个巨大对象。\n\
         13. 用户指定“可以使用的乐器”时，只能使用该列表中的乐器；指定“必须使用的乐器”时，至少为每一件乐器建立\
            一条轨道并写入内容。\n\
         14. VST 参数只能使用“已连接插件参数”里出现的 index；新加的插件可能下一帧才可调参数。控制器自动化写入\
            `set_controller`，不要用 `set_track` 冒充渐强/渐弱。\n\n",
    );
    prompt.push_str("## 当前工程\n");
    prompt.push_str(&project_context(project));
    prompt.push('\n');
    if !attachments.is_empty() {
        prompt.push_str("## 用户附件\n");
        prompt.push_str(&attachment_context(attachments));
        prompt.push('\n');
    }
    prompt.push_str("## 乐器清单\n");
    prompt.push_str(&instrument_catalog());
    prompt.push_str("\n## 已安装 VST 插件\n");
    prompt.push_str(&plugin_catalog(plugins));
    prompt.push_str("\n## 乐器使用约束\n");
    prompt.push_str(&instrument_constraints(selection));
    prompt.push_str("\n## 乐器与效果器参数\n");
    prompt.push_str(&parameter_context(project));
    if !plugin_params.is_empty() {
        prompt.push_str("\n## 已连接插件参数\n");
        prompt.push_str(&plugin_param_context(plugin_params));
    }
    prompt.push_str("\n## 参数与控制器操作要求\n");
    prompt.push_str(
        "用户要求“调音色/改参数/更亮/更暗/更多混响/更宽”时，必须实际输出参数操作，不能只回答建议。\n\
         - VST 乐器：`set_plugin_params`，`slot` 用 instrument，`params` 使用“已连接插件参数”中的 index 与 0~1 值。示例：\
           `{\"op\":\"set_plugin_params\",\"track\":\"Pad\",\"slot\":\"instrument\",\"params\":[{\"index\":0,\"value\":0.65}]}`。\n\
         - VST 效果器：`set_plugin_params`，`slot` 用 effect，`effect` 填效果器槽位下标。示例：\
           `{\"op\":\"set_plugin_params\",\"track\":\"Pad\",\"slot\":\"effect\",\"effect\":1,\"params\":[{\"index\":4,\"value\":0.3}]}`。\n\
         - 内置乐器/效果器：分别用 `set_instrument_params` / `set_effect_params`，键名使用“乐器与效果器参数”中列出的键。\n\
         - 用户要求渐强/渐弱/声像移动/延音踏板/弯音时，必须用 `set_controller`，不要用 `set_track` 代替。\
           音量与声像 0.5 为中心/100%，延音 0/1 为 OFF/ON，弯音 0.5 为中心；`replace: true` 会替换整条子轨。\n\
         - 参数序号只能来自清单；清单中没有的插件参数不要猜测。新挂载的插件若暂未列出参数，先完成挂载，下一轮再调参数。\n",
    );
    prompt.push_str("\n## 可调用编曲技巧\n");
    prompt.push_str(&library.technique_index());
    if let Some(agent) = library.agent(agent_id).or_else(|| library.agents.first()) {
        prompt.push_str("\n## 本次智能体：");
        prompt.push_str(&agent.name);
        prompt.push('（');
        prompt.push_str(&agent.description);
        prompt.push_str("）\n");
        if !agent.style.trim().is_empty() {
            prompt.push_str(&agent.style);
            prompt.push('\n');
        }
        let mut skill_ids = if selection.skills_customized {
            Vec::new()
        } else {
            agent.default_skill_ids.clone()
        };
        skill_ids.extend(selection.skill_ids.iter().cloned());
        let skill_text = library.load_basics(&skill_ids);
        if !skill_text.trim().is_empty() {
            prompt.push_str("## 本次加载的基础技能\n");
            prompt.push_str(&skill_text);
            prompt.push('\n');
        }
        let mut instrument_ids = agent.default_instrument_ids.clone();
        instrument_ids.extend(instrument_ids_for(
            library,
            selection
                .usable_instruments
                .iter()
                .chain(selection.required_instruments.iter())
                .map(String::as_str),
        ));
        let instrument_text = library.instrument_notes(&instrument_ids);
        if !instrument_text.trim().is_empty() {
            prompt.push_str("\n## 乐器资料\n");
            prompt.push_str(&instrument_text);
            prompt.push('\n');
        }
        let mut technique_ids = if selection.skills_customized {
            Vec::new()
        } else {
            agent.default_technique_ids.clone()
        };
        technique_ids.extend(selection.technique_ids.iter().cloned());
        let (technique_text, loaded) = library.load_techniques(&technique_ids);
        if !loaded.is_empty() {
            prompt.push_str("\n## 本次已加载的技巧正文\n");
            prompt.push_str(&technique_text);
            prompt.push('\n');
        }
    }
    prompt.push_str(
        "\n\n再次强调：只输出 JSON 对象；`reply` 使用简体中文；不要在 JSON 里写注释。\
         已安装 VST 插件时，音色与效果优先使用清单中的 VST（`add_plugin_track` / `set_plugin_instrument` / \
         `add_plugin_effect`），并在 `reply` 中说明用了哪个插件。",
    );
    prompt
}

/// 所属模块：ai / 提示词
/// 功能：把“可用/必须使用的乐器”写成硬约束。
/// 输入：`selection`：输入框选择。
/// 输出/副作用：约束文本；不修改选择。
/// 复杂度：时间 O(选择数)，空间 O(选择数 × 名称长度)。
///
/// Module: ai / prompts
/// Purpose: Render usable/required instrument selections as hard constraints.
/// Inputs: `selection`: composer selection.
/// Outputs / side effects: constraint text; the selection is unchanged.
/// Complexity: O(selection) time and O(selection × name length) space.
fn instrument_constraints(selection: &AiPromptSelection) -> String {
    if selection.usable_instruments.is_empty() && selection.required_instruments.is_empty() {
        return "用户没有限制乐器；仍须遵守“音色优先 VST”与乐器清单。\n".into();
    }
    let mut text = String::new();
    if !selection.usable_instruments.is_empty() {
        text.push_str("只允许使用以下乐器（id / 名称 / VST）：\n");
        for instrument in &selection.usable_instruments {
            text.push_str(&format!("- {instrument}\n"));
        }
    }
    if !selection.required_instruments.is_empty() {
        text.push_str("必须使用的乐器（至少各建立一条轨道并写入内容）：\n");
        for instrument in &selection.required_instruments {
            text.push_str(&format!("- {instrument}\n"));
        }
    }
    text
}

/// 所属模块：ai / 提示词
/// 功能：把用户选择映射到资料库中的乐器资料 id。
/// 输入：`library` 资料库；`names` 选择项（id、名称或别称）。
/// 输出/副作用：去重后的资料 id；不修改状态。
/// 复杂度：时间 O(选择数 × 乐器数)，空间 O(资料数)。
///
/// Module: ai / prompts
/// Purpose: Map composer selections to instrument-note ids in the library.
/// Inputs: `library`; `names` (ids, names or aliases).
/// Outputs / side effects: de-duplicated note ids; no state change.
/// Complexity: O(selection × instruments) time and O(notes) space.
fn instrument_ids_for<'a>(
    library: &AiLibrary,
    names: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    let mut ids = Vec::new();
    for name in names {
        let lower = name.trim().to_lowercase();
        if lower.is_empty() {
            continue;
        }
        let found = library.instruments.iter().find(|instrument| {
            instrument.id.to_lowercase() == lower
                || instrument.name.to_lowercase() == lower
                || instrument
                    .aliases
                    .split(',')
                    .any(|alias| alias.trim().to_lowercase() == lower)
        });
        if let Some(instrument) = found {
            if !ids.contains(&instrument.id) {
                ids.push(instrument.id.clone());
            }
        }
    }
    ids
}

/// 所属模块：ai / 提示词
/// 功能：列出各轨道内置乐器与内置效果器的可调参数及当前值。
/// 输入：`project`：当前工程。
/// 输出/副作用：参数清单文本；不修改工程。
/// 复杂度：时间 O(轨道数 × 参数数)，空间 O(文本长度)。
///
/// Module: ai / prompts
/// Purpose: List adjustable native instrument/effect parameters and their current values.
/// Inputs: `project`.
/// Outputs / side effects: parameter text; no project change.
/// Complexity: O(tracks × parameters) time and O(text length) space.
fn parameter_context(project: &Project) -> String {
    let mut text = String::from(
        "内置参数使用真实单位或归一化值；VST 参数使用下方“已连接插件参数”中的 index 与 0~1 值。\n",
    );
    if project.tracks.is_empty() {
        text.push_str("当前没有轨道。\n");
        return text;
    }
    for track in project.tracks.iter().take(MAX_PARAM_TRACKS) {
        text.push_str(&format!(
            "- 轨道「{}」乐器 {}：",
            track.name,
            track.instrument.label()
        ));
        text.push_str(&native_instrument_params(track));
        text.push('\n');
        for (index, effect) in track.effects.iter().enumerate() {
            match effect {
                EffectRef::Native(native) => {
                    let params = native
                        .kind
                        .params()
                        .iter()
                        .map(|param| {
                            format!(
                                "{}={}",
                                native_param_key(*param),
                                native.settings.get(*param)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    text.push_str(&format!(
                        "  效果器[{index}] {}（{}）：{params}\n",
                        native.kind.label(),
                        if native.enabled { "启用" } else { "旁路" }
                    ));
                }
                EffectRef::Plugin(plugin) => {
                    text.push_str(&format!(
                        "  效果器[{index}] {}（VST，参数见“已连接插件参数”）：{}\n",
                        plugin.name,
                        if plugin.enabled { "启用" } else { "旁路" }
                    ));
                }
            }
        }
    }
    text
}

/// 所属模块：ai / 提示词
/// 功能：输出一个轨道内置乐器的参数键值。
/// 输入：`track`：轨道。
/// 输出/副作用：一行参数文本；不修改轨道。
/// 复杂度：时间 O(参数数)，空间 O(文本长度)。
///
/// Module: ai / prompts
/// Purpose: Render one track's native instrument parameters.
/// Inputs: `track`.
/// Outputs / side effects: one parameter line; the track is unchanged.
/// Complexity: O(parameters) time and O(text length) space.
fn native_instrument_params(track: &Track) -> String {
    match &track.instrument {
        InstrumentRef::Synth(synth) => format!(
            "waveform={:?} attack={:.3}s decay={:.3}s sustain={:.2} release={:.3}s cutoff={:.0}Hz resonance={:.2} detune={:.3} octave={} level={:.2}",
            synth.waveform,
            synth.attack,
            synth.decay,
            synth.sustain,
            synth.release,
            synth.cutoff,
            synth.resonance,
            synth.detune,
            synth.octave,
            synth.level
        ),
        InstrumentRef::Drums(drums) => {
            format!("level={:.2} tone={:.2}", drums.level, drums.tone)
        }
        InstrumentRef::Native(native) => format!(
            "level={:.2} tone={:.2} attack={:.3}s release={:.3}s sustain={:.2} drive={:.2} vibrato={:.2}",
            native.settings.level,
            native.settings.tone,
            native.settings.attack,
            native.settings.release,
            native.settings.sustain,
            native.settings.drive,
            native.settings.vibrato
        ),
        InstrumentRef::Sample(sample) => format!(
            "level={:.2} tone={:.2} attack={:.3}s release={:.3}s velocity={:.2} neural={:.2}",
            sample.settings.level,
            sample.settings.tone,
            sample.settings.attack,
            sample.settings.release,
            sample.settings.velocity,
            sample.settings.neural
        ),
        InstrumentRef::Plugin(plugin) => {
            format!("{}（VST，参数见“已连接插件参数”）", plugin.name)
        }
    }
}

/// 所属模块：ai / 提示词
/// 功能：返回内置效果器参数在 JSON 中使用的键名。
/// 输入：`param`：效果器参数。
/// 输出/副作用：键名；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / prompts
/// Purpose: Return the JSON key of a native effect parameter.
/// Inputs: `param`.
/// Outputs / side effects: key text; no state change.
/// Complexity: O(1) time and space.
fn native_param_key(param: NativeEffectParam) -> &'static str {
    match param {
        NativeEffectParam::Drive => "drive",
        NativeEffectParam::Tone => "tone",
        NativeEffectParam::Mix => "mix",
        NativeEffectParam::Rate => "rate",
        NativeEffectParam::Depth => "depth",
        NativeEffectParam::Time => "time",
        NativeEffectParam::Feedback => "feedback",
        NativeEffectParam::Threshold => "threshold",
        NativeEffectParam::Ratio => "ratio",
        NativeEffectParam::Attack => "attack",
        NativeEffectParam::Release => "release",
        NativeEffectParam::Level => "level",
    }
}

/// 所属模块：ai / 提示词
/// 功能：把插件参数快照写成索引、名称、当前值与显示值。
/// 输入：`plugin_params`：插件参数快照。
/// 输出/副作用：清单文本；不修改快照。
/// 复杂度：时间 O(槽位数 × 参数数)，空间 O(文本长度)。
///
/// Module: ai / prompts
/// Purpose: Render plugin parameter snapshots with index, name, current and display values.
/// Inputs: `plugin_params`.
/// Outputs / side effects: catalogue text; the snapshots are unchanged.
/// Complexity: O(slots × parameters) time and O(text length) space.
fn plugin_param_context(plugin_params: &[AiPluginParamInfo]) -> String {
    let mut text = String::from(
        "修改时使用 `set_plugin_params`：`slot` 为 instrument 或 effect，`effect` 为效果器槽位下标，\
         `params` 为 `[{\"index\":参数序号,\"value\":0~1}]`。\n",
    );
    for slot in plugin_params.iter().take(MAX_PLUGIN_SLOTS) {
        let position = slot
            .effect_index
            .map(|index| format!("效果器[{index}]"))
            .unwrap_or_else(|| "乐器".into());
        text.push_str(&format!(
            "- 轨道「{}」{position} {}：\n",
            slot.track, slot.plugin
        ));
        for param in slot.params.iter().take(MAX_PLUGIN_PARAMS) {
            let suffix = if param.display.trim().is_empty() {
                String::new()
            } else {
                format!("（{}）", param.display)
            };
            text.push_str(&format!(
                "  [{}] {} = {:.3}{suffix}\n",
                param.index, param.name, param.value
            ));
        }
        if slot.params.len() > MAX_PLUGIN_PARAMS {
            text.push_str("  …（参数过多，已省略）\n");
        }
    }
    text
}

/// 所属模块：ai / 提示词
/// 功能：把工程压缩为文本：速度、拍号、轨道/片段摘要与有限数量的已有音符。
/// 输入：`project`：当前工程。
/// 输出/副作用：工程摘要文本；不修改工程。
/// 复杂度：时间 O(轨道数 + 片段数 + 写入音符数)，空间 O(同)。
///
/// Module: ai / prompts
/// Purpose: Compress a project into text: tempo, time signature, track/clip summaries and a limited
/// number of existing notes.
/// Inputs: `project`: current project.
/// Outputs / side effects: project summary text; no project change.
/// Complexity: O(tracks + clips + notes written) time and space.
pub fn project_context(project: &Project) -> String {
    let mut text = format!(
        "速度 {} BPM；拍号 {}/{}；每小节 {:.2} 拍；工程内容结束于第 {:.2} 拍。\n",
        project.bpm,
        project.time_sig_num,
        project.time_sig_den,
        project.beats_per_bar(),
        project.content_end_beat()
    );
    if project.tracks.is_empty() {
        text.push_str("当前工程没有轨道。\n");
        return text;
    }
    text.push_str("轨道列表：\n");
    let mut written = 0usize;
    for track in &project.tracks {
        let note_count: usize = track.clips.iter().map(|clip| clip.notes.len()).sum();
        let kind = if track.is_group() { "组合" } else { "普通" };
        text.push_str(&format!(
            "- id={} 名称「{}」（{}） 乐器={} 片段={} 音符={} 音量={:.2} 声像={:.2}{}\n",
            track.id,
            track.name,
            kind,
            track.instrument.label(),
            track.clips.len(),
            note_count,
            track.volume,
            track.pan,
            if track.muted { " 静音" } else { "" }
        ));
        if !track.effects.is_empty() {
            let effects = track
                .effects
                .iter()
                .map(|effect| effect.label())
                .collect::<Vec<_>>()
                .join("、");
            text.push_str(&format!("  效果器：{effects}\n"));
        }
        let controllers = ControllerKind::ALL
            .iter()
            .filter_map(|kind| {
                let count = track.controllers.get(*kind).len();
                (count > 0).then(|| format!("{}:{count} 点", kind.label()))
            })
            .collect::<Vec<_>>();
        if !controllers.is_empty() {
            text.push_str(&format!("  控制器子轨：{}\n", controllers.join("，")));
        }
        if track.is_group() {
            continue;
        }
        for (clip_index, clip) in track.clips.iter().enumerate() {
            if clip.is_audio() {
                text.push_str(&format!(
                    "  片段 {}「{}」：音频 {:.2}~{:.2} 拍\n",
                    clip_index + 1,
                    clip.name,
                    clip.start,
                    clip.end()
                ));
                continue;
            }
            text.push_str(&format!(
                "  片段 {}「{}」：{:.2}~{:.2} 拍，{} 个音符",
                clip_index + 1,
                clip.name,
                clip.start,
                clip.end(),
                clip.notes.len()
            ));
            if clip.notes.is_empty() {
                text.push('\n');
                continue;
            }
            text.push_str("；音高:相对拍:时值:力度 = ");
            let mut clip_written = 0usize;
            for note in &clip.notes {
                if written >= NOTES_TOTAL || clip_written >= NOTES_PER_CLIP {
                    break;
                }
                text.push_str(&format_note(
                    note.pitch,
                    note.start,
                    note.length,
                    note.velocity,
                ));
                text.push(' ');
                written += 1;
                clip_written += 1;
            }
            if clip_written < clip.notes.len() {
                text.push_str("…（其余省略）");
            }
            text.push('\n');
            if written >= NOTES_TOTAL {
                text.push_str("（已达到上下文中的音符上限，其余轨道音符省略）\n");
                return text;
            }
        }
    }
    text
}

/// 所属模块：ai / 提示词
/// 功能：把已安装 VST 插件清单压成提示词文本；不包含本机路径。
/// 输入：`plugins`：插件条目。
/// 输出/副作用：清单文本；不修改状态。
/// 复杂度：时间 O(插件数)，空间 O(同)。
///
/// Module: ai / prompts
/// Purpose: Render the installed VST catalogue into prompt text without local paths.
/// Inputs: `plugins`: plugin entries.
/// Outputs / side effects: catalogue text; no state change.
/// Complexity: O(plugins) time and space.
fn plugin_catalog(plugins: &[AiPluginInfo]) -> String {
    const MAX_PLUGINS: usize = 300;
    if plugins.is_empty() {
        return "当前没有已扫描的 VST 插件（可能尚未扫描，或当前平台不支持 VST）。\
                不要使用 add_plugin_track / set_plugin_instrument / add_plugin_effect。\n"
            .into();
    }
    let mut text = format!(
        "已扫描到 {} 个可用插件；只要用户需要音色或效果，**优先使用下列 VST**。\n\
         角色 | 名称 | 厂商 | 分类 | 格式 | 说明\n",
        plugins.len()
    );
    for plugin in plugins.iter().take(MAX_PLUGINS) {
        text.push_str(&plugin.prompt_line());
        text.push('\n');
    }
    if plugins.len() > MAX_PLUGINS {
        text.push_str(&format!(
            "（其余 {} 个插件省略；如需要请让用户缩小范围）\n",
            plugins.len() - MAX_PLUGINS
        ));
    }
    let instruments = plugins
        .iter()
        .filter(|plugin| plugin.kind != AiPluginKind::Effect)
        .take(4)
        .map(|plugin| plugin.name.as_str())
        .collect::<Vec<_>>();
    let effects = plugins
        .iter()
        .filter(|plugin| plugin.kind != AiPluginKind::Instrument)
        .take(4)
        .map(|plugin| plugin.name.as_str())
        .collect::<Vec<_>>();
    if !instruments.is_empty() {
        text.push_str(&format!("优先候选乐器：{}。\n", instruments.join("、")));
    }
    if !effects.is_empty() {
        text.push_str(&format!("优先候选效果器：{}。\n", effects.join("、")));
    }
    text.push_str(
        "选择插件时写名称；重名时写“厂商 名称”。不确定插件音色或用途时，先用 research 检索。\n",
    );
    text
}

/// 所属模块：ai / 提示词
/// 功能：把附件内容拼成文本，并限制单个附件的长度。
/// 输入：`attachments`：用户附件列表。
/// 输出/副作用：附件摘要文本；不修改附件。
/// 复杂度：时间 O(附件总长度)，空间 O(附件总长度)。
///
/// Module: ai / prompts
/// Purpose: Concatenate attachments and cap each one's length.
/// Inputs: `attachments`: user attachments.
/// Outputs / side effects: attachment summary text; the attachments are unchanged.
/// Complexity: O(total attachment length) time and space.
fn attachment_context(attachments: &[AiAttachment]) -> String {
    const MAX_CHARS: usize = 12_000;
    let mut text = String::new();
    for (index, attachment) in attachments.iter().enumerate() {
        text.push_str(&format!("### 附件 {}：{}\n", index + 1, attachment.name));
        let body: String = attachment.text.chars().take(MAX_CHARS).collect();
        text.push_str(&body);
        if attachment.text.chars().count() > MAX_CHARS {
            text.push_str("\n…（附件过长，已截断）");
        }
        text.push('\n');
    }
    text
}

/// 所属模块：ai / 提示词
/// 功能：生成一行紧凑音符文本。
/// 输入：音高、相对起始拍、时值、力度。
/// 输出/副作用：`C4:0.00:2.00:80` 形式的文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / prompts
/// Purpose: Build one compact note line.
/// Inputs: pitch, relative start, length and velocity.
/// Outputs / side effects: text in `C4:0.00:2.00:80` form; no state change.
/// Complexity: O(1) time and space.
fn format_note(pitch: u8, start: f64, length: f64, velocity: u8) -> String {
    format!("{}:{start:.2}:{length:.2}:{velocity}", pitch_name(pitch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{Clip, InstrumentRef, Note, Project, SynthSettings};

    /// 覆盖：系统提示词包含输出协议、工程轨道与乐器清单。
    /// Covers: the system prompt contains the output contract, project tracks and instrument catalogue.
    #[test]
    fn system_prompt_contains_contract_and_project() {
        let mut project = Project::default();
        project.bpm = 84.0;
        let index = project.add_track(InstrumentRef::Synth(SynthSettings::default()));
        let mut clip = Clip::new(1, "段落", 0.0, 4.0);
        clip.notes.push(Note {
            start: 0.0,
            length: 2.0,
            pitch: 60,
            velocity: 80,
        });
        project.tracks[index].clips.push(clip);
        let library = AiLibrary::seeded();
        let prompt = system_prompt(
            &library,
            "light-music",
            &AiPromptSelection::default(),
            &project,
            &[],
            &[],
            &[],
        );
        assert!(prompt.contains("\"operations\""));
        assert!(prompt.contains("速度 84 BPM"));
        assert!(prompt.contains("GrandPiano"));
        assert!(prompt.contains("基础乐理"));
        assert!(prompt.contains("C4:0.00:2.00:80"));
    }

    /// 覆盖：附件文本被写入提示词。
    /// Covers: attachment text is included in the prompt.
    #[test]
    fn attachments_appear_in_prompt() {
        let attachments = vec![AiAttachment {
            name: "melody.mid".into(),
            text: "0.00 C4 1.00 v90".into(),
        }];
        let library = AiLibrary::seeded();
        let prompt = system_prompt(
            &library,
            "light-music",
            &AiPromptSelection::default(),
            &Project::default(),
            &attachments,
            &[],
            &[],
        );
        assert!(prompt.contains("melody.mid"));
        assert!(prompt.contains("v90"));
    }

    /// 覆盖：已安装插件写入提示词，但不包含本机路径。
    /// Covers: installed plugins appear in the prompt without local paths.
    #[test]
    fn plugin_catalogue_appears_without_paths() {
        let plugins = vec![AiPluginInfo {
            name: "Serum".into(),
            vendor: "Xfer Records".into(),
            category: "Synth".into(),
            description: "波表合成器".into(),
            format: crate::core::model::PluginFormat::Vst3,
            path: std::path::PathBuf::from("C:/secret/Serum.vst3"),
            uid: "uid".into(),
            kind: crate::ai::AiPluginKind::Instrument,
            has_midi_input: true,
        }];
        let library = AiLibrary::seeded();
        let prompt = system_prompt(
            &library,
            "light-music",
            &AiPromptSelection::default(),
            &Project::default(),
            &[],
            &plugins,
            &[],
        );
        assert!(prompt.contains("Serum | Xfer Records"));
        assert!(prompt.contains("波表合成器"));
        assert!(prompt.contains("add_plugin_track"));
        assert!(!prompt.contains("C:/secret"));
    }
}
