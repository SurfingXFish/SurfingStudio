//! 所属模块：ai / 可编辑资料库
//! 文件用途：
//! - 定义可在主页 Agent 栏中增删改的编曲智能体、基础乐理、编曲技巧与乐器资料。
//! - 内置资料由编译期 Markdown 技能初始化；用户修改后随全局设置保存到本机。
//! - 为提示词提供按 id 解析、正文加载与技巧索引。
//!
//! 函数清单：
//! - `AiLibrary::default` / `seeded` / `migrate`：内置资料库、旧版迁移（风格入智能体、乐器拆分）。
//! - `AiLibrary::normalise`：清理空 id、去重并裁剪非法字段。
//! - `AiLibrary::agent` / `basic` / `technique` / `instrument`：按 id 查询条目。
//! - `AiLibrary::agent_skill_ids` / `agent_technique_ids` / `agent_instrument_ids`：智能体默认集合。
//! - `AiLibrary::load_basics` / `load_techniques`：按 id 加载正文（带数量与长度上限）。
//! - `AiLibrary::technique_index` / `instrument_notes`：提示词索引与乐器写作要点。
//! - `AiAgentProfile::new` / `AiSkillDoc::new` / `AiInstrumentDoc::new`：空白条目。
//! - `default_instrument_docs` / `instrument_tips`：每件内置乐器一条资料与分类技巧。
//! - `make_id` / `unique_id`：由名称生成稳定 id。
//!
//! Module: ai / editable library
//! Purpose:
//! - Define arrangement agents, theory skills, arranging techniques and instrument notes that can be
//!   created, edited and deleted in the home Agent column.
//! - Seed built-in entries from the compile-time Markdown skills; user edits persist in settings.
//! - Resolve ids, load bodies and build the technique index for prompts.
//!
//! Functions:
//! - `AiLibrary::default` / `seeded` / `migrate`: built-in library and legacy migration.
//! - `AiLibrary::normalise`: drop empty ids, de-duplicate and trim invalid fields.
//! - `AiLibrary::agent` / `basic` / `technique` / `instrument`: look up an entry by id.
//! - `AiLibrary::agent_skill_ids` / `agent_technique_ids` / `agent_instrument_ids`: agent defaults.
//! - `AiLibrary::load_basics` / `load_techniques`: load bodies by id with count/length caps.
//! - `AiLibrary::technique_index` / `instrument_notes`: prompt index and instrument notes.
//! - `AiAgentProfile::new` / `AiSkillDoc::new` / `AiInstrumentDoc::new`: blank entries.
//! - `make_id` / `unique_id`: build a stable id from a display name.

use serde::{Deserialize, Serialize};

use super::skills::{J_POP, LIGHT_MUSIC, MODERN_ARRANGEMENT, MUSIC_THEORY, TECHNIQUE_SKILLS};
use crate::core::model::bank_instruments;

/// 当前资料库结构版本；旧版本在加载时自动迁移。
/// Current library schema version; older versions migrate on load.
pub const CURRENT_LIBRARY_VERSION: u32 = 3;

/// 智能体加载基础技能的默认上限，避免一次塞入过多正文。
/// Default cap on theory skills loaded for one agent.
const MAX_AGENT_SKILLS: usize = 12;
/// 单次加载编曲技巧的数量上限。
/// Maximum number of technique bodies loaded at once.
const MAX_LOADED_TECHNIQUES: usize = 6;
/// 单个技巧正文进入提示词的最大字符数。
/// Maximum characters of one technique body included in a prompt.
const MAX_TECHNIQUE_CHARS: usize = 24_000;
/// 单个基础技能正文进入提示词的最大字符数。
/// Maximum characters of one theory skill body included in a prompt.
const MAX_SKILL_CHARS: usize = 24_000;
/// 单个乐器资料正文进入提示词的最大字符数。
/// Maximum characters of one instrument note included in a prompt.
const MAX_INSTRUMENT_CHARS: usize = 16_000;

/// 一个可管理的编曲智能体。
/// One editable arrangement agent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AiAgentProfile {
    /// 稳定 id，用于下拉选择与默认技能绑定。
    /// Stable id used by the selector and default-skill bindings.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// 额外的角色说明（可为空；风格正文通常放在基础技能中）。
    /// Extra persona text (may be empty; the style body usually lives in a theory skill).
    #[serde(default)]
    pub style: String,
    /// 默认加载的基础技能 id。
    /// Default theory-skill ids.
    #[serde(default)]
    pub default_skill_ids: Vec<String>,
    /// 默认加载的编曲技巧 id（正文会直接进入提示词，建议保持为空）。
    /// Default technique ids (bodies enter the prompt directly; keep this empty unless needed).
    #[serde(default)]
    pub default_technique_ids: Vec<String>,
    /// 默认加载的乐器资料 id。
    /// Default instrument-note ids.
    #[serde(default)]
    pub default_instrument_ids: Vec<String>,
}

impl AiAgentProfile {
    /// 所属模块：ai / 可编辑资料库 / 智能体
    /// 功能：创建一个空白智能体。
    /// 输入：`id` 稳定标识。
    /// 输出/副作用：空白条目；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / editable library / agent
    /// Purpose: Create a blank agent.
    /// Inputs: `id`: stable identity.
    /// Outputs / side effects: the blank entry; no state change.
    /// Complexity: O(1) time and space.
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            name: "新智能体".into(),
            description: "描述该智能体的写作取向".into(),
            style: String::new(),
            default_skill_ids: Vec::new(),
            default_technique_ids: Vec::new(),
            default_instrument_ids: Vec::new(),
        }
    }
}

/// 一条基础乐理技能或编曲技巧。
/// One theory skill or arranging technique.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AiSkillDoc {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    /// 一句话特点，用于索引与列表。
    /// One-line summary used by the index and lists.
    #[serde(default)]
    pub summary: String,
    /// Markdown 正文。
    /// Markdown body.
    #[serde(default)]
    pub body: String,
}

impl AiSkillDoc {
    /// 所属模块：ai / 可编辑资料库 / 技能
    /// 功能：创建一个空白技能条目。
    /// 输入：`id` 稳定标识。
    /// 输出/副作用：空白条目；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / editable library / skill
    /// Purpose: Create a blank skill entry.
    /// Inputs: `id`: stable identity.
    /// Outputs / side effects: the blank entry; no state change.
    /// Complexity: O(1) time and space.
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            title: "新技能".into(),
            summary: String::new(),
            body: String::new(),
        }
    }
}

/// 一条乐器资料（说明、使用场景、写作技巧；可对应内置采样乐器或 VST 插件）。
/// One instrument note (description, usage, writing tips; may describe a bank instrument or VST).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AiInstrumentDoc {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// 别称或插件名，用逗号分隔，便于模型把清单名称与资料对应。
    /// Aliases or plugin names separated by commas so the model can match catalogue names.
    #[serde(default)]
    pub aliases: String,
    /// 分类，如“键盘”“弦乐”“合成器”。
    /// Category such as keyboard, strings or synth.
    #[serde(default)]
    pub category: String,
    /// 音域或演奏范围说明。
    /// Playable range or register notes.
    #[serde(default)]
    pub range: String,
    /// 音色与角色说明。
    /// Tone and role description.
    #[serde(default)]
    pub description: String,
    /// 典型使用场景。
    /// Typical usage scenarios.
    #[serde(default)]
    pub usage: String,
    /// 写作与演奏技巧。
    /// Writing and performance tips.
    #[serde(default)]
    pub techniques: String,
    /// 更完整的 Markdown 正文（可为空）。
    /// Fuller Markdown body (may be empty).
    #[serde(default)]
    pub body: String,
}

impl AiInstrumentDoc {
    /// 所属模块：ai / 可编辑资料库 / 乐器
    /// 功能：创建一个空白乐器资料。
    /// 输入：`id` 稳定标识。
    /// 输出/副作用：空白条目；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / editable library / instrument
    /// Purpose: Create a blank instrument note.
    /// Inputs: `id`: stable identity.
    /// Outputs / side effects: the blank entry; no state change.
    /// Complexity: O(1) time and space.
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            name: "新乐器".into(),
            aliases: String::new(),
            category: String::new(),
            range: String::new(),
            description: String::new(),
            usage: String::new(),
            techniques: String::new(),
            body: String::new(),
        }
    }
}

/// 主页 Agent 栏与 AI 提示词共用的可编辑资料库。
/// Editable library shared by the home Agent column and AI prompts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AiLibrary {
    /// 资料库结构版本，用于加载时迁移旧数据。
    /// Library schema version used to migrate older data on load.
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub agents: Vec<AiAgentProfile>,
    /// 基础乐理技能。
    /// Theory skills.
    #[serde(default)]
    pub basics: Vec<AiSkillDoc>,
    /// 编曲技巧。
    /// Arranging techniques.
    #[serde(default)]
    pub techniques: Vec<AiSkillDoc>,
    /// 乐器资料。
    /// Instrument notes.
    #[serde(default)]
    pub instruments: Vec<AiInstrumentDoc>,
}

impl Default for AiLibrary {
    /// 所属模块：ai / 可编辑资料库
    /// 功能：返回内置资料库。
    /// 输入：无。
    /// 输出/副作用：内置条目；不读盘。
    /// 复杂度：时间 O(条目数)，空间 O(内嵌正文)。
    ///
    /// Module: ai / editable library
    /// Purpose: Return the built-in library.
    /// Inputs: none.
    /// Outputs / side effects: built-in entries; no disk access.
    /// Complexity: O(entries) time and O(embedded text) space.
    fn default() -> Self {
        Self::seeded()
    }
}

impl AiLibrary {
    /// 所属模块：ai / 可编辑资料库
    /// 功能：用编译期内嵌技能初始化资料库。
    /// 输入：无。
    /// 输出/副作用：内置资料库；不修改状态。
    /// 复杂度：时间 O(技能数)，空间 O(内嵌正文)。
    ///
    /// Module: ai / editable library
    /// Purpose: Seed the library from the compile-time skills.
    /// Inputs: none.
    /// Outputs / side effects: the built-in library; no state change.
    /// Complexity: O(skills) time and O(embedded text) space.
    pub fn seeded() -> Self {
        let basics = vec![
            skill_doc(&MUSIC_THEORY, "基础乐理、和声与节奏"),
            skill_doc(&MODERN_ARRANGEMENT, "段落表、能量曲线与现代和声"),
        ];
        let techniques = TECHNIQUE_SKILLS
            .iter()
            .map(|technique| AiSkillDoc {
                id: technique.id.to_string(),
                title: technique.title.to_string(),
                summary: technique.summary.to_string(),
                body: technique.body.to_string(),
            })
            .collect();
        let instruments = default_instrument_docs();
        let agents = vec![
            AiAgentProfile {
                id: "light-music".into(),
                name: "轻音乐创作".into(),
                description: "旋律优先、和声柔和、配器透明".into(),
                // 风格正文属于智能体的“创作准则”，不再混在基础技能列表里。
                // The style body is the agent's writing guideline, not a theory skill.
                style: LIGHT_MUSIC.body.to_string(),
                default_skill_ids: vec![MUSIC_THEORY.id.into(), MODERN_ARRANGEMENT.id.into()],
                default_technique_ids: Vec::new(),
                default_instrument_ids: Vec::new(),
            },
            AiAgentProfile {
                id: "j-pop".into(),
                name: "J-Pop".into(),
                description: "强 Hook、王道/丸サ进行、副歌优先、转调与高密度配器".into(),
                style: J_POP.body.to_string(),
                default_skill_ids: vec![MUSIC_THEORY.id.into(), MODERN_ARRANGEMENT.id.into()],
                default_technique_ids: Vec::new(),
                default_instrument_ids: Vec::new(),
            },
        ];
        let mut library = Self {
            version: CURRENT_LIBRARY_VERSION,
            agents,
            basics,
            techniques,
            instruments,
        };
        library.normalise();
        library
    }

    /// 所属模块：ai / 可编辑资料库
    /// 功能：把旧版资料库迁移到当前结构：风格移入智能体创作准则、乐器拆分为单件资料。
    /// 输入：`self`。
    /// 输出/副作用：原地修改资料库并写入当前版本号；不访问文件。
    /// 复杂度：时间 O(条目数 + 乐器数)，空间 O(乐器数)。
    ///
    /// Module: ai / editable library
    /// Purpose: Migrate an older library: move styles into agent guidelines and split grouped
    /// instruments into one entry per instrument.
    /// Inputs: `self`.
    /// Outputs / side effects: mutates the library and writes the current version; no file access.
    /// Complexity: O(entries + instruments) time and O(instruments) space.
    pub fn migrate(&mut self) {
        if self.version >= CURRENT_LIBRARY_VERSION {
            self.normalise();
            return;
        }
        if self.version < 2 {
            // 旧版把轻音乐 / J-Pop 风格放在基础技能中；迁移到智能体的创作准则。
            // Older builds kept the light-music / J-Pop styles in basics; move them to guidelines.
            for (agent_id, style) in [("light-music", &LIGHT_MUSIC), ("j-pop", &J_POP)] {
                if let Some(agent) = self.agents.iter_mut().find(|agent| agent.id == agent_id) {
                    if agent.style.trim().chars().count() < 200 {
                        agent.style = style.body.to_string();
                    }
                    agent.default_skill_ids.retain(|id| id != style.id);
                }
            }
            self.basics
                .retain(|skill| skill.id != LIGHT_MUSIC.id && skill.id != J_POP.id);
        }
        if self.version < 3 {
            // 旧版八条配器技能把多件乐器混在一条资料里；改为每件乐器一条资料。
            // Older eight orchestration skills grouped several instruments; split them per instrument.
            const OLD_IDS: [&str; 8] = [
                "instrument-piano",
                "instrument-guitar-bass",
                "instrument-strings",
                "instrument-woodwinds",
                "instrument-brass",
                "instrument-percussion",
                "instrument-synth",
                "instrument-folk-world",
            ];
            let had_grouped = self
                .instruments
                .iter()
                .any(|instrument| OLD_IDS.contains(&instrument.id.as_str()));
            if had_grouped {
                self.instruments
                    .retain(|instrument| !OLD_IDS.contains(&instrument.id.as_str()));
                for instrument in default_instrument_docs() {
                    if !self
                        .instruments
                        .iter()
                        .any(|existing| existing.id == instrument.id)
                    {
                        self.instruments.push(instrument);
                    }
                }
            }
            for agent in &mut self.agents {
                agent
                    .default_instrument_ids
                    .retain(|id| !OLD_IDS.contains(&id.as_str()));
            }
        }
        self.version = CURRENT_LIBRARY_VERSION;
        self.normalise();
    }

    /// 所属模块：ai / 可编辑资料库
    /// 功能：清理资料：去除空 id、同 id 重复项，裁剪空白名称并限制默认技能数量。
    /// 输入：`self`。
    /// 输出/副作用：原地修改资料库；不访问文件。
    /// 复杂度：时间 O(条目数 × 默认技能数)，空间 O(条目数)。
    ///
    /// Module: ai / editable library
    /// Purpose: Clean the library: drop empty ids and duplicates, trim blank names and cap default
    /// skill counts.
    /// Inputs: `self`.
    /// Outputs / side effects: mutates the library in place; no file access.
    /// Complexity: O(entries × default skills) time and O(entries) space.
    pub fn normalise(&mut self) {
        dedupe_by_id(&mut self.agents, |agent| &agent.id);
        dedupe_by_id(&mut self.basics, |skill| &skill.id);
        dedupe_by_id(&mut self.techniques, |skill| &skill.id);
        dedupe_by_id(&mut self.instruments, |instrument| &instrument.id);
        for agent in &mut self.agents {
            agent.name = agent.name.trim().to_string();
            if agent.name.is_empty() {
                agent.name = "未命名智能体".into();
            }
            dedupe_strings(&mut agent.default_skill_ids);
            dedupe_strings(&mut agent.default_technique_ids);
            dedupe_strings(&mut agent.default_instrument_ids);
            agent.default_skill_ids.truncate(MAX_AGENT_SKILLS);
        }
        for skill in self.basics.iter_mut().chain(self.techniques.iter_mut()) {
            skill.title = skill.title.trim().to_string();
            if skill.title.is_empty() {
                skill.title = "未命名技能".into();
            }
        }
        for instrument in &mut self.instruments {
            instrument.name = instrument.name.trim().to_string();
            if instrument.name.is_empty() {
                instrument.name = "未命名乐器".into();
            }
        }
    }

    /// 所属模块：ai / 可编辑资料库
    /// 功能：按 id 查找智能体。
    /// 输入：`id`：智能体 id。
    /// 输出/副作用：条目引用或 `None`；不修改状态。
    /// 复杂度：时间 O(智能体数)，空间 O(1)。
    ///
    /// Module: ai / editable library
    /// Purpose: Find an agent by id.
    /// Inputs: `id`: agent id.
    /// Outputs / side effects: the entry or `None`; no state change.
    /// Complexity: O(agents) time and O(1) space.
    pub fn agent(&self, id: &str) -> Option<&AiAgentProfile> {
        self.agents.iter().find(|agent| agent.id == id)
    }

    /// 所属模块：ai / 可编辑资料库
    /// 功能：按 id 查找基础技能。
    /// 输入：`id`：技能 id。
    /// 输出/副作用：条目引用或 `None`；不修改状态。
    /// 复杂度：时间 O(技能数)，空间 O(1)。
    ///
    /// Module: ai / editable library
    /// Purpose: Find a theory skill by id.
    /// Inputs: `id`: skill id.
    /// Outputs / side effects: the entry or `None`; no state change.
    /// Complexity: O(skills) time and O(1) space.
    pub fn basic(&self, id: &str) -> Option<&AiSkillDoc> {
        self.basics.iter().find(|skill| skill.id == id)
    }

    /// 所属模块：ai / 可编辑资料库
    /// 功能：按 id 查找编曲技巧。
    /// 输入：`id`：技巧 id。
    /// 输出/副作用：条目引用或 `None`；不修改状态。
    /// 复杂度：时间 O(技巧数)，空间 O(1)。
    ///
    /// Module: ai / editable library
    /// Purpose: Find an arranging technique by id.
    /// Inputs: `id`: technique id.
    /// Outputs / side effects: the entry or `None`; no state change.
    /// Complexity: O(techniques) time and O(1) space.
    pub fn technique(&self, id: &str) -> Option<&AiSkillDoc> {
        self.techniques.iter().find(|skill| skill.id == id)
    }

    /// 所属模块：ai / 可编辑资料库
    /// 功能：按 id 查找乐器资料。
    /// 输入：`id`：乐器资料 id。
    /// 输出/副作用：条目引用或 `None`；不修改状态。
    /// 复杂度：时间 O(乐器数)，空间 O(1)。
    ///
    /// Module: ai / editable library
    /// Purpose: Find an instrument note by id.
    /// Inputs: `id`: instrument note id.
    /// Outputs / side effects: the entry or `None`; no state change.
    /// Complexity: O(instruments) time and O(1) space.
    pub fn instrument(&self, id: &str) -> Option<&AiInstrumentDoc> {
        self.instruments
            .iter()
            .find(|instrument| instrument.id == id)
    }

    /// 所属模块：ai / 可编辑资料库
    /// 功能：返回智能体默认基础技能 id；未知智能体回退到第一个智能体。
    /// 输入：`agent_id`：智能体 id。
    /// 输出/副作用：id 列表；不修改状态。
    /// 复杂度：时间 O(默认技能数)，空间 O(默认技能数)。
    ///
    /// Module: ai / editable library
    /// Purpose: Return the agent's default theory-skill ids; unknown ids fall back to the first agent.
    /// Inputs: `agent_id`: agent id.
    /// Outputs / side effects: id list; no state change.
    /// Complexity: O(default skills) time and space.
    pub fn agent_skill_ids(&self, agent_id: &str) -> Vec<String> {
        self.resolve_agent(agent_id)
            .map(|agent| agent.default_skill_ids.clone())
            .unwrap_or_default()
    }

    /// 所属模块：ai / 可编辑资料库
    /// 功能：返回智能体默认编曲技巧 id。
    /// 输入：`agent_id`：智能体 id。
    /// 输出/副作用：id 列表；不修改状态。
    /// 复杂度：时间 O(默认技巧数)，空间 O(默认技巧数)。
    ///
    /// Module: ai / editable library
    /// Purpose: Return the agent's default technique ids.
    /// Inputs: `agent_id`: agent id.
    /// Outputs / side effects: id list; no state change.
    /// Complexity: O(default techniques) time and space.
    pub fn agent_technique_ids(&self, agent_id: &str) -> Vec<String> {
        self.resolve_agent(agent_id)
            .map(|agent| agent.default_technique_ids.clone())
            .unwrap_or_default()
    }

    /// 所属模块：ai / 可编辑资料库
    /// 功能：返回智能体默认乐器资料 id。
    /// 输入：`agent_id`：智能体 id。
    /// 输出/副作用：id 列表；不修改状态。
    /// 复杂度：时间 O(默认乐器数)，空间 O(默认乐器数)。
    ///
    /// Module: ai / editable library
    /// Purpose: Return the agent's default instrument-note ids.
    /// Inputs: `agent_id`: agent id.
    /// Outputs / side effects: id list; no state change.
    /// Complexity: O(default instruments) time and space.
    pub fn agent_instrument_ids(&self, agent_id: &str) -> Vec<String> {
        self.resolve_agent(agent_id)
            .map(|agent| agent.default_instrument_ids.clone())
            .unwrap_or_default()
    }

    /// 所属模块：ai / 可编辑资料库
    /// 功能：按 id 加载基础技能正文，跳过未知 id 并限制总长度。
    /// 输入：`ids`：技能 id。
    /// 输出/副作用：拼接正文；不修改状态。
    /// 复杂度：时间 O(请求数 × 技能数 + 正文长度)，空间 O(正文长度)。
    ///
    /// Module: ai / editable library
    /// Purpose: Load theory-skill bodies by id, skip unknown ids and cap the total length.
    /// Inputs: `ids`: skill ids.
    /// Outputs / side effects: concatenated bodies; no state change.
    /// Complexity: O(requested × skills + body length) time and O(body length) space.
    pub fn load_basics(&self, ids: &[String]) -> String {
        let mut text = String::new();
        for id in ids {
            let Some(skill) = self.basic(id) else {
                continue;
            };
            text.push_str(&format!(
                "\n\n# 技能：{}（{}）\n{}",
                skill.title,
                skill.id,
                truncate_body(&skill.body, MAX_SKILL_CHARS)
            ));
        }
        text
    }

    /// 所属模块：ai / 可编辑资料库
    /// 功能：按 id 加载编曲技巧正文，最多 6 个；未知 id 汇总说明。
    /// 输入：`ids`：技巧 id。
    /// 输出/副作用：拼接正文与已加载 id；不修改状态。
    /// 复杂度：时间 O(请求数 × 技巧数 + 正文长度)，空间 O(正文长度)。
    ///
    /// Module: ai / editable library
    /// Purpose: Load technique bodies by id, at most six; unknown ids are reported together.
    /// Inputs: `ids`: technique ids.
    /// Outputs / side effects: concatenated bodies and loaded ids; no state change.
    /// Complexity: O(requested × techniques + body length) time and O(body length) space.
    pub fn load_techniques(&self, ids: &[String]) -> (String, Vec<String>) {
        let mut text = String::new();
        let mut loaded = Vec::new();
        let mut unknown = Vec::new();
        for id in ids {
            if loaded.len() >= MAX_LOADED_TECHNIQUES {
                break;
            }
            let key = id.trim().to_lowercase();
            if key.is_empty() || loaded.iter().any(|entry| entry == &key) {
                continue;
            }
            if let Some(technique) = self.technique(&key) {
                text.push_str(&format!(
                    "\n\n# 技巧：{}（{}）\n{}",
                    technique.title,
                    technique.id,
                    truncate_body(&technique.body, MAX_TECHNIQUE_CHARS)
                ));
                loaded.push(key);
            } else {
                unknown.push(id.clone());
            }
        }
        if !unknown.is_empty() {
            text.push_str(&format!(
                "\n\n（未找到技巧：{}；可用 id 见技巧索引。）",
                unknown.join("、")
            ));
        }
        (text, loaded)
    }

    /// 所属模块：ai / 可编辑资料库
    /// 功能：生成常驻提示词的技巧索引（id、名称、一句话特点）。
    /// 输入：无。
    /// 输出/副作用：索引文本；不修改状态。
    /// 复杂度：时间 O(技巧数)，空间 O(技巧数)。
    ///
    /// Module: ai / editable library
    /// Purpose: Build the always-present technique index (id, title, one-line use).
    /// Inputs: none.
    /// Outputs / side effects: index text; no state change.
    /// Complexity: O(techniques) time and space.
    pub fn technique_index(&self) -> String {
        let mut text = String::from(
            "技巧 id | 名称 | 特点 / 适用情况\n\
             需要具体写法时把 id 放进输出 JSON 的 `skills` 数组（最多 4 个），宿主加载后再次调用你。\n",
        );
        for technique in &self.techniques {
            text.push_str(&format!(
                "{} | {} | {}\n",
                technique.id, technique.title, technique.summary
            ));
        }
        text
    }

    /// 所属模块：ai / 可编辑资料库
    /// 功能：把指定乐器资料拼成提示词段落。
    /// 输入：`ids`：乐器资料 id。
    /// 输出/副作用：Markdown 文本；不修改状态。
    /// 复杂度：时间 O(请求数 × 乐器数 + 正文长度)，空间 O(正文长度)。
    ///
    /// Module: ai / editable library
    /// Purpose: Concatenate the requested instrument notes into a prompt section.
    /// Inputs: `ids`: instrument-note ids.
    /// Outputs / side effects: Markdown text; no state change.
    /// Complexity: O(requested × instruments + text length) time and O(text length) space.
    pub fn instrument_notes(&self, ids: &[String]) -> String {
        let mut text = String::new();
        let mut seen = Vec::new();
        for id in ids {
            if seen.iter().any(|entry| entry == id) {
                continue;
            }
            let Some(instrument) = self.instrument(id) else {
                continue;
            };
            seen.push(id.clone());
            text.push_str(&format!(
                "\n\n## 乐器资料：{}（{}）\n分类：{}；音域：{}；别称：{}\n说明：{}\n使用场景：{}\n技巧：{}\n{}",
                instrument.name,
                instrument.id,
                instrument.category,
                instrument.range,
                instrument.aliases,
                instrument.description,
                instrument.usage,
                instrument.techniques,
                truncate_body(&instrument.body, MAX_INSTRUMENT_CHARS)
            ));
        }
        text
    }

    fn resolve_agent(&self, agent_id: &str) -> Option<&AiAgentProfile> {
        self.agent(agent_id).or_else(|| self.agents.first())
    }
}

/// 所属模块：ai / 可编辑资料库
/// 功能：把编译期技能转换成基础技能条目。
/// 输入：`skill`：内嵌技能；`summary`：列表摘要。
/// 输出/副作用：条目；不修改状态。
/// 复杂度：时间 O(正文长度)，空间 O(正文长度)。
///
/// Module: ai / editable library
/// Purpose: Convert a compile-time skill into a theory-skill entry.
/// Inputs: `skill`: embedded skill; `summary`: list summary.
/// Outputs / side effects: the entry; no state change.
/// Complexity: O(body length) time and space.
fn skill_doc(skill: &super::skills::AiSkill, summary: &str) -> AiSkillDoc {
    AiSkillDoc {
        id: skill.id.to_string(),
        title: skill.title.to_string(),
        summary: summary.to_string(),
        body: skill.body.to_string(),
    }
}

/// 所属模块：ai / 可编辑资料库
/// 功能：为每件内置乐器生成一条独立资料（内置合成器、鼓机与 128 件采样乐器）。
/// 输入：无。
/// 输出/副作用：乐器资料列表；不修改状态。
/// 复杂度：时间 O(乐器数)，空间 O(乐器数)。
///
/// Module: ai / editable library
/// Purpose: Build one entry per built-in instrument (synth, drum kit and the 128 sampled instruments).
/// Inputs: none.
/// Outputs / side effects: instrument notes; no state change.
/// Complexity: O(instruments) time and space.
fn default_instrument_docs() -> Vec<AiInstrumentDoc> {
    let mut docs = vec![
        AiInstrumentDoc {
            id: "instrument-synth".into(),
            name: "内置合成器".into(),
            aliases: "synth".into(),
            category: "合成器".into(),
            range: "0-127".into(),
            description: "内置复音合成器，可调波形、滤波与包络。".into(),
            usage: "适合 Pad、Lead、Bass 与电子音色；也可作为临时试听音色。".into(),
            techniques: "用滤波与包络塑造明暗；长音留出起音与释放，避免与钢琴/吉他同频段堆叠。"
                .into(),
            body: String::new(),
        },
        AiInstrumentDoc {
            id: "instrument-drums".into(),
            name: "内置鼓机".into(),
            aliases: "drums".into(),
            category: "打击乐".into(),
            range: "36-50".into(),
            description: "内置 808/909 风格鼓机，使用 GM 鼓键位。".into(),
            usage: "用于节奏骨架；底鼓、军鼓、踩镲分层编写。".into(),
            techniques:
                "保持底鼓与贝斯的低频分工；踩镲用力度与固定偏移制造律动，避免每拍完全一致。".into(),
            body: String::new(),
        },
    ];
    for entry in bank_instruments() {
        let range = format!("{}-{}", entry.range[0], entry.range[1]);
        docs.push(AiInstrumentDoc {
            id: format!("instrument-{}", entry.id.to_lowercase()),
            name: entry.name.clone(),
            aliases: entry.id.clone(),
            category: entry.category.clone(),
            range: range.clone(),
            description: format!("{}采样乐器，音域 {range}。", entry.name),
            usage: format!(
                "作为{}声部使用；按音域分配旋律、和声或低音，必要时与相近音色叠加。",
                entry.category
            ),
            techniques: instrument_tips(&entry.category).into(),
            body: String::new(),
        });
    }
    docs
}

/// 所属模块：ai / 可编辑资料库
/// 功能：按分类返回单件乐器的通用写作技巧。
/// 输入：`category`：乐器分类。
/// 输出/副作用：技巧文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / editable library
/// Purpose: Return generic writing tips for one instrument category.
/// Inputs: `category`.
/// Outputs / side effects: tip text; no state change.
/// Complexity: O(1) time and space.
fn instrument_tips(category: &str) -> &'static str {
    let category = category.to_lowercase();
    if category.contains("钢琴") || category.contains("键") {
        "旋律优先舒适音区；左手避免与贝斯抢根音，和弦排列保持中低频通透。"
    } else if category.contains("吉他") {
        "分解和弦注意换把与空弦共鸣；失真声部减少三音堆叠，留出中频给主旋律。"
    } else if category.contains("贝斯") {
        "根音与底鼓对齐，适当用经过音连接；避免与底鼓同时占满最低频段。"
    } else if category.contains("提琴") || category.contains("弦") {
        "长音注意弓法与换弦，连奏留出呼吸；分组写法比整组齐奏更有层次。"
    } else if category.contains("管") || category.contains("笛") || category.contains("萨克斯")
    {
        "按乐句换气，避免超长连续音；高音区降低力度，和声保持三度/六度间距。"
    } else if category.contains("铜") || category.contains("号") {
        "重音与短促音型更出彩；避免长时间高音齐奏，和声低音交给低铜管。"
    } else if category.contains("鼓") || category.contains("打击") {
        "用力度层次与固定偏移做律动；主击与填充分开写，避免每小节同样密度。"
    } else if category.contains("合成") || category.contains("电子") {
        "用滤波与包络控制能量；Pad 留中频空位，Lead 与主旋律错开音区。"
    } else if category.contains("民乐") || category.contains("世界") {
        "保留乐器原有演奏法（滑音、揉弦、轮指）；不要用 Western 平均律和声硬套。"
    } else {
        "按真实音域写作；长音留出呼吸，避免与同频段乐器同时堆叠。"
    }
}

/// 所属模块：ai / 可编辑资料库
/// 功能：按 id 去重，保留第一次出现的条目并丢弃空 id。
/// 输入：`items`：条目列表；`id_of`：取 id 的函数。
/// 输出/副作用：原地修改列表；不修改其他状态。
/// 复杂度：时间 O(条目数²)，空间 O(1)。
///
/// Module: ai / editable library
/// Purpose: De-duplicate by id, keep the first entry and drop empty ids.
/// Inputs: `items`: entries; `id_of`: id accessor.
/// Outputs / side effects: mutates the list in place.
/// Complexity: O(entries²) time and O(1) space.
fn dedupe_by_id<T>(items: &mut Vec<T>, id_of: impl Fn(&T) -> &String) {
    let mut index = 0;
    while index < items.len() {
        let id = id_of(&items[index]).trim().to_string();
        if id.is_empty() {
            items.remove(index);
            continue;
        }
        let duplicate = items[..index].iter().any(|entry| id_of(entry).trim() == id);
        if duplicate {
            items.remove(index);
            continue;
        }
        index += 1;
    }
}

/// 所属模块：ai / 可编辑资料库
/// 功能：去重字符串列表并移除空项。
/// 输入：`items`：字符串列表。
/// 输出/副作用：原地修改；不修改其他状态。
/// 复杂度：时间 O(条目数²)，空间 O(1)。
///
/// Module: ai / editable library
/// Purpose: De-duplicate a string list and drop empty entries.
/// Inputs: `items`: string list.
/// Outputs / side effects: mutates the list in place.
/// Complexity: O(entries²) time and O(1) space.
fn dedupe_strings(items: &mut Vec<String>) {
    let mut seen: Vec<String> = Vec::new();
    let mut index = 0;
    while index < items.len() {
        let trimmed = items[index].trim().to_string();
        if trimmed.is_empty() || seen.contains(&trimmed) {
            items.remove(index);
            continue;
        }
        seen.push(trimmed.clone());
        items[index] = trimmed;
        index += 1;
    }
}

/// 所属模块：ai / 可编辑资料库
/// 功能：按字符数裁剪正文并附加省略说明。
/// 输入：`text` 正文；`limit` 最大字符数。
/// 输出/副作用：裁剪后的文本；不修改输入。
/// 复杂度：时间 O(limit)，空间 O(limit)。
///
/// Module: ai / editable library
/// Purpose: Truncate a body by characters and append an ellipsis note.
/// Inputs: `text`; `limit`.
/// Outputs / side effects: truncated text; the input is unchanged.
/// Complexity: O(limit) time and space.
fn truncate_body(text: &str, limit: usize) -> String {
    let mut result: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        result.push_str("\n…（正文过长，已截断）");
    }
    result
}

/// 所属模块：ai / 可编辑资料库
/// 功能：由显示名称生成 id（小写、kebab-case、只保留字母数字）。
/// 输入：`name` 显示名称。
/// 输出/副作用：id 文本；名称为空时返回 `entry`。
/// 复杂度：时间 O(名称长度)，空间 O(名称长度)。
///
/// Module: ai / editable library
/// Purpose: Build an id from a display name (lower-case kebab-case, alphanumeric only).
/// Inputs: `name`.
/// Outputs / side effects: id text; empty names return `entry`.
/// Complexity: O(name length) time and space.
pub fn make_id(name: &str, fallback: &str) -> String {
    let mut id = String::new();
    let mut pending_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !id.is_empty() {
                id.push('-');
            }
            pending_dash = false;
            id.push(ch.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    if id.is_empty() {
        fallback.to_string()
    } else {
        id
    }
}

/// 所属模块：ai / 可编辑资料库
/// 功能：生成在现有 id 集合中唯一的 id。
/// 输入：`base` 基础 id；`taken` 已占用的 id。
/// 输出/副作用：唯一 id；不修改状态。
/// 复杂度：时间 O(占用数)，空间 O(1)。
///
/// Module: ai / editable library
/// Purpose: Build an id unique among the existing ids.
/// Inputs: `base`; `taken`.
/// Outputs / side effects: a unique id; no state change.
/// Complexity: O(taken) time and O(1) space.
pub fn unique_id(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_string();
    }
    for suffix in 2..=10_000 {
        let candidate = format!("{base}-{suffix}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    format!("{base}-10000")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：内置资料库包含两个智能体、两条基础技能、全部技巧与每件乐器一条资料。
    /// Covers: two agents, two theory skills, every technique and one entry per instrument.
    #[test]
    fn seeded_library_has_builtin_entries() {
        let library = AiLibrary::seeded();
        assert_eq!(library.agents.len(), 2);
        assert_eq!(library.basics.len(), 2);
        assert_eq!(library.techniques.len(), TECHNIQUE_SKILLS.len());
        assert_eq!(library.instruments.len(), bank_instruments().len() + 2);
        assert!(library.agent("light-music").is_some());
        assert!(library.basic("music-theory").is_some());
        assert!(library.technique("groove-pocket").is_some());
        assert!(library.instrument("instrument-grandpiano").is_some());
        assert!(library.instrument("instrument-synth").is_some());
    }

    /// 覆盖：旧版资料库迁移后风格进入智能体准则、分组乐器被拆成单件。
    /// Covers: migration moves styles into agent guidelines and splits grouped instruments.
    #[test]
    fn migration_splits_styles_and_instruments() {
        let mut library = AiLibrary::seeded();
        library.version = 0;
        library.basics.push(skill_doc(&LIGHT_MUSIC, "旧风格"));
        library.agents[0].style = "短说明".into();
        library.agents[0]
            .default_skill_ids
            .push("light-music".into());
        library.instruments.push(AiInstrumentDoc {
            id: "instrument-piano".into(),
            name: "键盘与钢琴".into(),
            ..AiInstrumentDoc::new("instrument-piano")
        });
        library.migrate();
        assert_eq!(library.version, CURRENT_LIBRARY_VERSION);
        assert!(library.basics.iter().all(|skill| skill.id != "light-music"));
        assert!(library.agents[0].style.contains("轻音乐"));
        assert!(
            !library.agents[0]
                .default_skill_ids
                .contains(&"light-music".to_string())
        );
        assert!(library.instrument("instrument-piano").is_none());
        assert!(library.instrument("instrument-grandpiano").is_some());
    }

    /// 覆盖：技巧按 id 加载正文，未知 id 会被说明。
    /// Covers: techniques load by id and unknown ids are reported.
    #[test]
    fn technique_loading_skips_unknown_ids() {
        let library = AiLibrary::seeded();
        let (text, loaded) = library.load_techniques(&[
            "groove-pocket".into(),
            "syncopation".into(),
            "missing".into(),
        ]);
        assert!(text.contains("技巧：律动与 Pocket"));
        assert!(text.contains("未找到技巧"));
        assert_eq!(loaded, vec!["groove-pocket", "syncopation"]);
    }

    /// 覆盖：id 归一化会去重、丢弃空项并生成 kebab-case。
    /// Covers: normalisation de-duplicates, drops empty ids and builds kebab-case ids.
    #[test]
    fn normalise_and_make_id() {
        assert_eq!(make_id("My New Agent!", "agent"), "my-new-agent");
        assert_eq!(make_id("智能体", "agent"), "agent");
        assert_eq!(unique_id("agent", |id| id == "agent"), "agent-2");
        let mut library = AiLibrary::seeded();
        library.agents.push(library.agents[0].clone());
        library.basics.push(AiSkillDoc::new(""));
        library.normalise();
        assert_eq!(library.agents.len(), 2);
        assert!(library.basics.iter().all(|skill| !skill.id.is_empty()));
    }
}
