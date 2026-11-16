//! 所属模块：ai / 内置技能素材
//! 文件用途：
//! - 以编译期内嵌的 Markdown 技能描述乐理、现代编曲、风格（轻音乐 / J-Pop）与各配器写法。
//! - 提供编曲技巧的初始内容（`techniques/*.md`）与内置采样音色清单；可编辑资料库在
//!   `ai/library.rs` 中初始化与增删改。
//!
//! 函数清单：
//! - `instrument_catalog`：内置采样音色库的 id / 中文名 / 音域清单。
//!
//! Module: ai / built-in skill sources
//! Purpose:
//! - Embed Markdown skills describing theory, modern arrangement, styles (light music / J-Pop) and
//!   instrument writing at compile time.
//! - Provide the initial technique bodies (`techniques/*.md`) and the built-in sample catalogue; the
//!   editable library is seeded and managed in `ai/library.rs`.
//!
//! Functions:
//! - `instrument_catalog`: ids, Chinese names and ranges of the built-in sample bank.

use crate::core::model::bank_instruments;

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

    /// 覆盖：内置技能正文非空。
    /// Covers: every embedded skill has a non-empty body.
    #[test]
    fn embedded_skill_bodies_are_present() {
        assert!(!MUSIC_THEORY.body.trim().is_empty());
        assert!(!LIGHT_MUSIC.body.trim().is_empty());
        assert!(!MODERN_ARRANGEMENT.body.trim().is_empty());
        assert!(!J_POP.body.trim().is_empty());
        assert!(!TECHNIQUE_SKILLS[0].body.trim().is_empty());
        assert_eq!(INSTRUMENT_SKILLS.len(), 8);
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
