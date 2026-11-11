//! 所属模块：ai / 内置技能
//! 文件用途：
//! - 以编译期内嵌的 Markdown 技能描述乐理、现代编曲、风格（轻音乐 / J-Pop）与各配器写法。
//! - 以“索引 + 按需加载”的方式提供编曲技巧 skill（`techniques/*.md`），供模型自主调用。
//! - 定义 AI 智能体（轻音乐创作、J-Pop）及其必选技能集合。
//! - 生成给模型的乐器清单，确保计划只使用软件真实存在的音色。
//!
//! 函数清单：
//! - `AiAgentKind::id` / `label` / `description` / `ALL`：智能体信息。
//! - `AiSkill` / `skill_catalog` / `agent_skills`：技能条目与智能体技能集合。
//! - `skill_prompt`：把一个智能体的全部技能拼成系统提示词片段。
//! - `technique_catalog` / `load_techniques`：编曲技巧索引与按需加载。
//! - `instrument_catalog`：内置采样音色库的 id / 中文名 / 音域清单。
//!
//! Module: ai / built-in skills
//! Purpose:
//! - Embed Markdown skills describing theory, modern arrangement, styles (light music / J-Pop) and
//!   instrument writing at compile time.
//! - Offer arrangement techniques as an index plus on-demand bodies (`techniques/*.md`) the model
//!   can request itself.
//! - Define AI agents (light music, J-Pop) and their mandatory skill sets.
//! - Build the instrument catalogue so plans only reference sounds the app really has.
//!
//! Functions:
//! - `AiAgentKind::id` / `label` / `description` / `ALL`: agent data.
//! - `AiSkill` / `skill_catalog` / `agent_skills`: skill entries and an agent's skill set.
//! - `skill_prompt`: concatenate an agent's skills into a system-prompt section.
//! - `technique_catalog` / `load_techniques`: technique index and on-demand loading.
//! - `instrument_catalog`: ids, Chinese names and ranges of the built-in sample bank.

use crate::core::model::bank_instruments;

/// AI 智能体类型。
/// AI agent kinds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AiAgentKind {
    /// 轻音乐创作：旋律优先、和声柔和、配器透明的器乐写作。
    /// Light-music composition: melody-first instrumental writing with soft harmony and transparent
    /// orchestration.
    #[default]
    LightMusic,
    /// J-Pop：强 Hook、王道进行/丸サ进行、副歌优先、转调与高密度配器。
    /// J-Pop: strong hooks, royal-road / Marusa progressions, chorus-first form, modulations and dense
    /// production.
    JPop,
}

impl AiAgentKind {
    pub const ALL: [AiAgentKind; 2] = [AiAgentKind::LightMusic, AiAgentKind::JPop];

    /// 所属模块：ai / 智能体
    /// 功能：返回持久化标识。
    /// 输入：`self`。
    /// 输出/副作用：静态字符串；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / agent
    /// Purpose: Return the persisted id.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn id(self) -> &'static str {
        match self {
            AiAgentKind::LightMusic => "light-music",
            AiAgentKind::JPop => "j-pop",
        }
    }

    /// 所属模块：ai / 智能体
    /// 功能：返回下拉展示名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / agent
    /// Purpose: Return the selector label.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            AiAgentKind::LightMusic => "轻音乐创作",
            AiAgentKind::JPop => "J-Pop",
        }
    }

    /// 所属模块：ai / 智能体
    /// 功能：返回能力说明（用于下拉提示）。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / agent
    /// Purpose: Return the capability description used by the selector tooltip.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn description(self) -> &'static str {
        match self {
            AiAgentKind::LightMusic => "旋律优先、和声柔和、配器透明的轻音乐编曲",
            AiAgentKind::JPop => "强 Hook、王道/丸サ进行、副歌优先、转调与高密度配器",
        }
    }
}

/// 一段内置技能（Markdown）。
/// One embedded skill (Markdown).
#[derive(Clone, Copy, Debug)]
pub struct AiSkill {
    pub id: &'static str,
    pub title: &'static str,
    pub body: &'static str,
}

pub const MUSIC_THEORY: AiSkill = AiSkill {
    id: "music-theory",
    title: "基础乐理",
    body: include_str!("skills/music-theory.md"),
};

pub const LIGHT_MUSIC: AiSkill = AiSkill {
    id: "light-music",
    title: "轻音乐风格",
    body: include_str!("skills/light-music.md"),
};

pub const MODERN_ARRANGEMENT: AiSkill = AiSkill {
    id: "modern-arrangement",
    title: "现代编曲与曲式",
    body: include_str!("skills/modern-arrangement.md"),
};

pub const J_POP: AiSkill = AiSkill {
    id: "j-pop",
    title: "J-Pop 风格",
    body: include_str!("skills/j-pop.md"),
};

pub const PIANO: AiSkill = AiSkill {
    id: "instrument-piano",
    title: "配器：键盘与钢琴",
    body: include_str!("skills/instruments/piano.md"),
};

pub const GUITAR_BASS: AiSkill = AiSkill {
    id: "instrument-guitar-bass",
    title: "配器：吉他、贝斯与拨弦",
    body: include_str!("skills/instruments/guitar-bass.md"),
};

pub const STRINGS: AiSkill = AiSkill {
    id: "instrument-strings",
    title: "配器：弦乐组",
    body: include_str!("skills/instruments/strings.md"),
};

pub const WOODWINDS: AiSkill = AiSkill {
    id: "instrument-woodwinds",
    title: "配器：木管组",
    body: include_str!("skills/instruments/woodwinds.md"),
};

pub const BRASS: AiSkill = AiSkill {
    id: "instrument-brass",
    title: "配器：铜管组",
    body: include_str!("skills/instruments/brass.md"),
};

pub const PERCUSSION: AiSkill = AiSkill {
    id: "instrument-percussion",
    title: "配器：鼓组与打击乐",
    body: include_str!("skills/instruments/percussion.md"),
};

pub const SYNTH_ELECTRONIC: AiSkill = AiSkill {
    id: "instrument-synth",
    title: "配器：合成器与电子音色",
    body: include_str!("skills/instruments/synth-electronic.md"),
};

pub const FOLK_WORLD: AiSkill = AiSkill {
    id: "instrument-folk-world",
    title: "配器：民乐与世界乐器",
    body: include_str!("skills/instruments/folk-world.md"),
};

/// 全部乐器技能。
/// Every instrument skill.
pub const INSTRUMENT_SKILLS: [AiSkill; 8] = [
    PIANO,
    GUITAR_BASS,
    STRINGS,
    WOODWINDS,
    BRASS,
    PERCUSSION,
    SYNTH_ELECTRONIC,
    FOLK_WORLD,
];

/// 一个可被模型按需加载的编曲技巧。
/// An arranging technique the model may load on demand.
#[derive(Clone, Copy, Debug)]
pub struct AiTechnique {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub body: &'static str,
}

/// 全部可调用技巧：索引常驻提示词，正文按需加载。
/// Every on-demand technique: the index stays in the prompt, the body loads on request.
pub const TECHNIQUE_SKILLS: [AiTechnique; 14] = [
    AiTechnique {
        id: "groove-pocket",
        title: "律动与 Pocket",
        summary: "音符相对网格的固定偏移、三层鼓与力度层次；用于去机械化、增加推进力",
        body: include_str!("skills/techniques/groove-pocket.md"),
    },
    AiTechnique {
        id: "syncopation",
        title: "切分",
        summary: "重音移到弱拍/弱位；用于推动力、现代感与 pre-chorus",
        body: include_str!("skills/techniques/syncopation.md"),
    },
    AiTechnique {
        id: "call-response",
        title: "呼应",
        summary: "主句与回应轮流出现；用于配器对话、填补句尾空当",
        body: include_str!("skills/techniques/call-response.md"),
    },
    AiTechnique {
        id: "ostinato",
        title: "固定音型",
        summary: "短音型持续重复；用于稳定地基、氛围与 build",
        body: include_str!("skills/techniques/ostinato.md"),
    },
    AiTechnique {
        id: "countermelody",
        title: "副旋律",
        summary: "与主旋律同时进行的独立次级旋律；用于增加纵向层次",
        body: include_str!("skills/techniques/countermelody.md"),
    },
    AiTechnique {
        id: "pedal-point",
        title: "持续音",
        summary: "低音/高音长音保持，上方和声变化；用于悬空与张力",
        body: include_str!("skills/techniques/pedal-point.md"),
    },
    AiTechnique {
        id: "tension-release",
        title: "张力与释放",
        summary: "延迟解决、段落切换的期待管理；用于副歌/落点",
        body: include_str!("skills/techniques/tension-release.md"),
    },
    AiTechnique {
        id: "layering",
        title: "分层配器",
        summary: "按频段/角色分层，增删层控制能量；用于整理“空/糊”",
        body: include_str!("skills/techniques/layering.md"),
    },
    AiTechnique {
        id: "stabs",
        title: "和弦短切",
        summary: "短促重音和弦 + 留白；用于 Funk/J-Pop 节奏点",
        body: include_str!("skills/techniques/stabs.md"),
    },
    AiTechnique {
        id: "walking-bass",
        title: "行走贝斯",
        summary: "四分音符级进连接根音；用于爵士化段落与和声引导",
        body: include_str!("skills/techniques/walking-bass.md"),
    },
    AiTechnique {
        id: "half-time-double-time",
        title: "半速与双倍",
        summary: "不改 BPM，改听感速度；用于副歌/桥段的换挡",
        body: include_str!("skills/techniques/half-time-double-time.md"),
    },
    AiTechnique {
        id: "riser-impact",
        title: "上升与落点",
        summary: "riser 积累期待 + impact 释放；用于段落切换的电影感",
        body: include_str!("skills/techniques/riser-impact.md"),
    },
    AiTechnique {
        id: "silence-space",
        title: "留白",
        summary: "主动删音符/声部；用于呼吸、突出后文与结尾",
        body: include_str!("skills/techniques/silence-space.md"),
    },
    AiTechnique {
        id: "modulation",
        title: "转调",
        summary: "升调/关系调/中音转调；用于最后副歌与桥段",
        body: include_str!("skills/techniques/modulation.md"),
    },
];

/// 所属模块：ai / 技巧索引
/// 功能：生成常驻提示词的技巧索引（id、名称、一句话特点/用途）。
/// 输入：无。
/// 输出/副作用：索引文本；不修改状态。
/// 复杂度：时间 O(技巧数)，空间 O(同)。
///
/// Module: ai / technique index
/// Purpose: Build the always-present technique index (id, title, one-line use).
/// Inputs: none.
/// Outputs / side effects: index text; no state change.
/// Complexity: O(techniques) time and space.
pub fn technique_catalog() -> String {
    let mut text = String::from(
        "技巧 id | 名称 | 特点 / 适用情况\n\
         需要具体写法时把 id 放进输出 JSON 的 `skills` 数组（最多 4 个），宿主加载后再次调用你。\n",
    );
    for technique in TECHNIQUE_SKILLS.iter() {
        text.push_str(&format!(
            "{} | {} | {}\n",
            technique.id, technique.title, technique.summary
        ));
    }
    text
}

/// 所属模块：ai / 技巧加载
/// 功能：按 id 加载技巧正文，最多 4 个；未知 id 会被忽略并在文本中说明。
/// 输入：`ids`：模型请求的技巧 id。
/// 输出/副作用：拼接的正文与已加载 id；不修改状态。
/// 复杂度：时间 O(请求数 × 技巧数)，空间 O(正文长度)。
///
/// Module: ai / technique loading
/// Purpose: Load technique bodies by id, at most four; unknown ids are reported.
/// Inputs: `ids`: technique ids requested by the model.
/// Outputs / side effects: concatenated bodies and loaded ids; no state change.
/// Complexity: O(requested × techniques) time and O(body length) space.
pub fn load_techniques(ids: &[String]) -> (String, Vec<String>) {
    let mut text = String::new();
    let mut loaded = Vec::new();
    let mut unknown = Vec::new();
    for id in ids {
        if loaded.len() >= 4 {
            break;
        }
        let key = id.trim().to_lowercase();
        if key.is_empty() || loaded.iter().any(|entry| entry == &key) {
            continue;
        }
        if let Some(technique) = TECHNIQUE_SKILLS
            .iter()
            .find(|technique| technique.id == key)
        {
            text.push_str(&format!(
                "\n\n# 技巧：{}（{}）\n{}",
                technique.title, technique.id, technique.body
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

/// 所属模块：ai / 技能
/// 功能：返回全部内置技能（乐理 + 风格 + 配器）。
/// 输入：无。
/// 输出/副作用：技能引用列表；不修改状态。
/// 复杂度：时间 O(技能数)，空间 O(技能数)。
///
/// Module: ai / skills
/// Purpose: Return every built-in skill (theory + style + instruments).
/// Inputs: none.
/// Outputs / side effects: skill references; no state change.
/// Complexity: O(skills) time and space.
pub fn skill_catalog() -> Vec<&'static AiSkill> {
    let mut skills = vec![&MUSIC_THEORY, &MODERN_ARRANGEMENT, &LIGHT_MUSIC, &J_POP];
    skills.extend(INSTRUMENT_SKILLS.iter());
    skills
}

/// 所属模块：ai / 技能
/// 功能：返回指定智能体必须加载的技能；乐理与配器是所有智能体的基础。
/// 输入：`agent`：智能体类型。
/// 输出/副作用：技能引用列表；不修改状态。
/// 复杂度：时间 O(技能数)，空间 O(技能数)。
///
/// Module: ai / skills
/// Purpose: Return the skills an agent must load; theory and instruments are mandatory for every agent.
/// Inputs: `agent`: agent kind.
/// Outputs / side effects: skill references; no state change.
/// Complexity: O(skills) time and space.
pub fn agent_skills(agent: AiAgentKind) -> Vec<&'static AiSkill> {
    let style = match agent {
        AiAgentKind::LightMusic => &LIGHT_MUSIC,
        AiAgentKind::JPop => &J_POP,
    };
    let mut skills = vec![&MUSIC_THEORY, &MODERN_ARRANGEMENT, style];
    skills.extend(INSTRUMENT_SKILLS.iter());
    skills
}

/// 所属模块：ai / 技能
/// 功能：把一个智能体的全部技能拼成系统提示词片段，附带来源说明。
/// 输入：`agent`：智能体类型。
/// 输出/副作用：Markdown 文本；不修改状态。
/// 复杂度：时间 O(技能总长度)，空间 O(技能总长度)。
///
/// Module: ai / skills
/// Purpose: Concatenate an agent's skills into a system-prompt section with source notes.
/// Inputs: `agent`: agent kind.
/// Outputs / side effects: Markdown text; no state change.
/// Complexity: O(total skill length) time and space.
pub fn skill_prompt(agent: AiAgentKind) -> String {
    let mut text = String::from("以下是本次编曲必须遵循的内置技能，请完整阅读并在写作时执行。\n");
    // 说明资料来源：技能由公开乐理与配器资料整理，模型不应再引用无法验证的具体曲目。
    // Source note: the skills are compiled from public theory/orchestration references; the model must
    // not cite unverifiable specific pieces.
    text.push_str("技能内容整理自公开乐理、配器法与轻音乐风格资料，仅用于写作约束。\n");
    for skill in agent_skills(agent) {
        text.push_str("\n\n# 技能：");
        text.push_str(skill.title);
        text.push('（');
        text.push_str(skill.id);
        text.push_str("）\n");
        text.push_str(skill.body);
    }
    text
}

/// 所属模块：ai / 音色清单
/// 功能：生成内置采样音色库的 id / 中文名 / 音域清单，供模型选择乐器。
/// 输入：无。
/// 输出/副作用：文本清单；不修改状态。
/// 复杂度：时间 O(乐器数)，空间 O(乐器数 × 名称长度)。
///
/// Module: ai / instrument catalogue
/// Purpose: Build the id/name/range catalogue of the built-in sample bank for the model.
/// Inputs: none.
/// Outputs / side effects: text catalogue; no state change.
/// Complexity: O(instruments) time and O(instruments × name length) space.
pub fn instrument_catalog() -> String {
    let mut text = String::from(
        "乐器 id | 中文名 | MIDI 音域（含端点）\n\
         drums | 内置鼓机 | 36-50（GM 鼓键位，见打击乐技能）\n\
         synth | 内置合成器 | 0-127\n",
    );
    for entry in bank_instruments() {
        text.push_str(&format!(
            "{} | {} | {}-{}\n",
            entry.id, entry.name, entry.range[0], entry.range[1]
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：轻音乐智能体包含乐理、风格与全部配器技能。
    /// Covers: the light-music agent loads theory, style and every instrument skill.
    #[test]
    fn agent_skills_include_theory_and_instruments() {
        let skills = agent_skills(AiAgentKind::LightMusic);
        assert!(skills.iter().any(|skill| skill.id == "music-theory"));
        assert!(skills.iter().any(|skill| skill.id == "light-music"));
        assert!(skills.iter().any(|skill| skill.id == "modern-arrangement"));
        assert_eq!(skills.len(), 3 + INSTRUMENT_SKILLS.len());
        assert!(!MUSIC_THEORY.body.trim().is_empty());
        assert!(!LIGHT_MUSIC.body.trim().is_empty());
        assert!(!MODERN_ARRANGEMENT.body.trim().is_empty());
    }

    /// 覆盖：J-Pop 智能体加载乐理、现代编曲、J-Pop 风格与全部配器。
    /// Covers: the J-Pop agent loads theory, modern arrangement, J-Pop style and every instrument.
    #[test]
    fn jpop_agent_skills_include_style_and_instruments() {
        let skills = agent_skills(AiAgentKind::JPop);
        assert!(skills.iter().any(|skill| skill.id == "j-pop"));
        assert!(skills.iter().any(|skill| skill.id == "modern-arrangement"));
        assert_eq!(skills.len(), 3 + INSTRUMENT_SKILLS.len());
        assert!(!J_POP.body.trim().is_empty());
    }

    /// 覆盖：技巧索引包含律动，按 id 加载正文时只加载请求项。
    /// Covers: the technique index lists groove and loading only returns requested bodies.
    #[test]
    fn technique_index_and_loading() {
        let index = technique_catalog();
        assert!(index.contains("groove-pocket"));
        assert!(index.contains("律动"));
        let (text, loaded) = load_techniques(&[
            "groove-pocket".to_string(),
            "syncopation".to_string(),
            "missing".to_string(),
        ]);
        assert!(text.contains("# 技巧：律动与 Pocket"));
        assert!(text.contains("未找到技巧"));
        assert_eq!(loaded, vec!["groove-pocket", "syncopation"]);
        assert!(!TECHNIQUE_SKILLS[0].body.trim().is_empty());
    }

    /// 覆盖：音色清单包含大钢琴与民族乐器。
    /// Covers: the catalogue contains the grand piano and Chinese instruments.
    #[test]
    fn instrument_catalog_lists_bank_entries() {
        let catalog = instrument_catalog();
        assert!(catalog.contains("GrandPiano | 大钢琴"));
        assert!(catalog.contains("Guzheng"));
        assert!(catalog.contains("drums"));
    }
}
