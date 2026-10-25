//! 所属模块：core / 数据模型 / 音乐模板
//! 文件用途：
//! - 维护主流音乐风格模板目录（风格、调性、BPM、和弦进行、鼓组/Bass/和声/主旋律写法）。
//! - 把模板展开为可直接播放的 `Project`，供主页“模板”页快速创建工程。
//! - 维护内置范例工程目录，并支持以更长的小节数生成完整范例。
//!
//! 函数清单：
//! - `TemplateFamily::ALL` / `label`：模板家族筛选与中文名称。
//! - `Scale::intervals` / `semitone`：音阶与级数换算。
//! - `ChordQuality::intervals`：和弦音程。
//! - `MusicTemplate::track_summary`：轨道摘要文本。
//! - `templates` / `find_template` / `display_name`：模板目录查询。
//! - `example_projects` / `find_example` / `build_example_project`：范例工程查询与生成。
//! - `build_project` / `build_project_with_bars` / `build_by_id` / `Project::from_template`：模板展开与按 id 生成。
//! - `add_drum_track` / `add_bass_track` / `add_harmony_track` / `add_lead_track` / `add_pad_track`：轨道生成。
//! - `drum_notes` / `bass_notes` / `harmony_notes` / `harmony_notes_with` / `lead_notes` / `pad_notes`：音符生成。
//! - `chord_at` / `chord_root` / `chord_pitches` / `tone_pitch` / `arp_pitch` / `chord_tone`：和弦与音高换算。
//! - `push_chord` / `push_note` / `push_track` / `step_beats`：生成辅助。
//! - `harmony_instrument` / `lead_instrument` / `bass_instrument` / `native_with`：乐器构造。
//!
//! Module: core / data model / music templates
//! Purpose:
//! - Maintain a catalog of mainstream music-style templates (style, key, BPM, chord progression,
//!   drum/bass/harmony/lead writing).
//! - Expand a template into a playable `Project` for the home page's template page.
//! - Maintain built-in example projects that can be generated at a longer bar count.
//!
//! Functions:
//! - `TemplateFamily::ALL` / `label`: template-family filters and Chinese labels.
//! - `Scale::intervals` / `semitone`: scale intervals and degree conversion.
//! - `ChordQuality::intervals`: chord intervals.
//! - `MusicTemplate::track_summary`: track summary text.
//! - `templates` / `find_template` / `display_name`: catalog queries.
//! - `example_projects` / `find_example` / `build_example_project`: example queries and generation.
//! - `build_project` / `build_project_with_bars` / `build_by_id` / `Project::from_template`: expansion and id lookup.
//! - `add_drum_track` / `add_bass_track` / `add_harmony_track` / `add_lead_track` / `add_pad_track`: track generation.
//! - `drum_notes` / `bass_notes` / `harmony_notes` / `harmony_notes_with` / `lead_notes` / `pad_notes`: note generation.
//! - `chord_at` / `chord_root` / `chord_pitches` / `tone_pitch` / `arp_pitch` / `chord_tone`: chord and pitch conversion.
//! - `push_chord` / `push_note` / `push_track` / `step_beats`: generation helpers.
//! - `harmony_instrument` / `lead_instrument` / `bass_instrument` / `native_with`: instrument builders.

use super::clip::{Clip, Note};
use super::instrument::{
    DrumSettings, InstrumentRef, NativeInstrument, NativeInstrumentKind, NativeInstrumentSettings,
    SynthSettings, Waveform,
};
use super::project::Project;
use super::track::Track;

/// 模板/范例的稳定标识；与应用层 `WindowAction` 共享。
/// Stable template/example identifier shared with the app-layer `WindowAction`.
pub type TemplateId = &'static str;

/// 模板家族：用于主页模板页的快速筛选。
/// Template family used by the quick filters on the home template page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateFamily {
    Pop,
    Rock,
    Electronic,
    Urban,
    Jazz,
    Classical,
    Roots,
    Other,
}

impl TemplateFamily {
    pub const ALL: [TemplateFamily; 8] = [
        TemplateFamily::Pop,
        TemplateFamily::Rock,
        TemplateFamily::Electronic,
        TemplateFamily::Urban,
        TemplateFamily::Jazz,
        TemplateFamily::Classical,
        TemplateFamily::Roots,
        TemplateFamily::Other,
    ];

    /// 所属模块：core / 数据模型 / 音乐模板
    /// 功能：返回模板家族的中文名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / music templates
    /// Purpose: Return the Chinese label of a template family.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            TemplateFamily::Pop => "流行",
            TemplateFamily::Rock => "摇滚",
            TemplateFamily::Electronic => "电子",
            TemplateFamily::Urban => "嘻哈 / R&B",
            TemplateFamily::Jazz => "爵士",
            TemplateFamily::Classical => "古典 / 管弦",
            TemplateFamily::Roots => "民谣 / 根源",
            TemplateFamily::Other => "其他",
        }
    }
}

/// 模板使用的音阶。
/// Scale used by a template.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Major,
    Minor,
    Dorian,
    Mixolydian,
    Blues,
    PentatonicMinor,
}

impl Scale {
    /// 所属模块：core / 数据模型 / 音乐模板
    /// 功能：返回音阶的半音音程表。
    /// 输入：`self`。
    /// 输出/副作用：静态切片；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / music templates
    /// Purpose: Return the semitone interval table of the scale.
    /// Inputs: `self`.
    /// Outputs / side effects: static slice; no state change.
    /// Complexity: O(1) time and space.
    fn intervals(self) -> &'static [i32] {
        match self {
            Scale::Major => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::Blues => &[0, 3, 5, 6, 7, 10],
            Scale::PentatonicMinor => &[0, 3, 5, 7, 10],
        }
    }

    /// 所属模块：core / 数据模型 / 音乐模板
    /// 功能：把音阶级数换算为相对主音的半音数（支持跨八度与负数）。
    /// 输入：`degree` 音阶级数（0 为主音，7 为高八度主音）。
    /// 输出/副作用：半音偏移；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / music templates
    /// Purpose: Convert a scale degree into semitones above the tonic (supports octaves and
    /// negative degrees).
    /// Inputs: `degree` scale degree (0 is the tonic, 7 is the octave).
    /// Outputs / side effects: semitone offset; no state change.
    /// Complexity: O(1) time and space.
    fn semitone(self, degree: i32) -> i32 {
        let intervals = self.intervals();
        let length = intervals.len() as i32;
        let octave = degree.div_euclid(length);
        let index = degree.rem_euclid(length) as usize;
        intervals[index] + 12 * octave
    }
}

/// 和弦性质。
/// Chord quality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChordQuality {
    Major,
    Minor,
    Dominant7,
    Major7,
    Minor7,
    Sus2,
    Diminished,
}

impl ChordQuality {
    /// 所属模块：core / 数据模型 / 音乐模板
    /// 功能：返回和弦的相对音程。
    /// 输入：`self`。
    /// 输出/副作用：静态切片；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / music templates
    /// Purpose: Return the chord intervals relative to its root.
    /// Inputs: `self`.
    /// Outputs / side effects: static slice; no state change.
    /// Complexity: O(1) time and space.
    fn intervals(self) -> &'static [i32] {
        match self {
            ChordQuality::Major => &[0, 4, 7],
            ChordQuality::Minor => &[0, 3, 7],
            ChordQuality::Dominant7 => &[0, 4, 7, 10],
            ChordQuality::Major7 => &[0, 4, 7, 11],
            ChordQuality::Minor7 => &[0, 3, 7, 10],
            ChordQuality::Sus2 => &[0, 2, 7],
            ChordQuality::Diminished => &[0, 3, 6],
        }
    }
}

/// 一个和弦：音阶级数 + 和弦性质。
/// One chord: scale degree plus quality.
#[derive(Clone, Copy, Debug)]
pub struct ChordSpec {
    pub degree: i32,
    pub quality: ChordQuality,
}

/// 鼓组格子（16 分音符，0..15 为一个小节）。
/// Drum grid (16th notes; 0..15 is one bar).
#[derive(Clone, Copy, Debug)]
pub struct DrumGroove {
    pub kick: &'static [u8],
    pub snare: &'static [u8],
    pub ghost: &'static [u8],
    pub hat: &'static [u8],
    pub open_hat: &'static [u8],
    pub clap: &'static [u8],
    pub tom: &'static [u8],
    /// 摇摆量：0 = 直拍，1 = 最大摇摆。
    /// Swing amount: 0 = straight, 1 = maximum swing.
    pub swing: f64,
}

/// Bass 写法中的一步。
/// One bass pattern step.
#[derive(Clone, Copy, Debug)]
pub struct BassStep {
    pub step: u8,
    pub interval: i32,
    pub velocity: u8,
    pub length_steps: u8,
}

/// 和声/键盘写法。
/// Harmony/keyboard writing pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarmonyPattern {
    Sustain,
    Half,
    Offbeat,
    Stabs,
    Arp8,
    Arp16,
    Broken,
    Comp,
    Tremolo,
    Fingerpick,
    Montuno,
}

/// 和声轨使用的音色。
/// Voice used by the harmony track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarmonyVoice {
    Piano,
    SynthPad,
    SynthPluck,
    AcousticGuitar,
    ElectricGuitar,
    Strings,
    Organ,
}

/// 主旋律写法：2 小节动机。
/// Lead motif spanning two bars.
#[derive(Clone, Copy, Debug)]
pub struct Motif {
    /// 2 小节内的 16 分音符位置（0..31）。
    /// 16th-note positions within two bars (0..31).
    pub steps: &'static [u8],
    /// 和弦音索引（0 = 根音，1 = 三音，2 = 五音，3 = 高八度根音……）。
    /// Chord-tone indices (0 = root, 1 = third, 2 = fifth, 3 = octave root, ...).
    pub tones: &'static [u8],
    pub length_steps: u8,
    pub velocity: u8,
    /// 相对和弦根音的八度偏移。
    /// Octave offset relative to the chord root.
    pub octave: i32,
}

/// 主旋律音色。
/// Lead voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeadVoice {
    SynthLead,
    SynthPluck,
    ElectricGuitar,
    DistortedGuitar,
    AcousticGuitar,
    Strings,
    Piano,
}

/// 一条主流音乐风格模板。
/// One mainstream-style template.
#[derive(Clone, Copy, Debug)]
pub struct MusicTemplate {
    pub id: TemplateId,
    pub name: &'static str,
    pub style: &'static str,
    pub family: TemplateFamily,
    pub bpm: f64,
    pub key_root: u8,
    pub key_label: &'static str,
    pub scale: Scale,
    pub description: &'static str,
    pub tags: &'static [&'static str],
    pub chords: &'static [ChordSpec],
    pub drums: DrumGroove,
    pub bass: &'static [BassStep],
    pub harmony: HarmonyPattern,
    pub harmony_voice: HarmonyVoice,
    pub lead: Option<Motif>,
    pub lead_voice: LeadVoice,
    pub pad: bool,
    pub bars: u32,
}

impl MusicTemplate {
    /// 所属模块：core / 数据模型 / 音乐模板
    /// 功能：生成轨道摘要（用于模板卡片）。
    /// 输入：`self`。
    /// 输出/副作用：返回“鼓组 · 贝斯 · …”文本；不修改状态。
    /// 复杂度：时间 O(轨道数)，空间 O(文本长度)。
    ///
    /// Module: core / data model / music templates
    /// Purpose: Build the track summary shown on a template card.
    /// Inputs: `self`.
    /// Outputs / side effects: "drums · bass · …" text; no state change.
    /// Complexity: O(track count) time and O(text length) space.
    pub fn track_summary(&self) -> String {
        let mut parts = vec!["鼓组".to_string(), "贝斯".to_string()];
        parts.push(
            match self.harmony_voice {
                HarmonyVoice::Piano => "钢琴",
                HarmonyVoice::SynthPad => "合成器铺底",
                HarmonyVoice::SynthPluck => "合成器琶音",
                HarmonyVoice::AcousticGuitar => "木吉他",
                HarmonyVoice::ElectricGuitar => "电吉他",
                HarmonyVoice::Strings => "弦乐",
                HarmonyVoice::Organ => "风琴",
            }
            .to_string(),
        );
        if self.lead.is_some() {
            parts.push(
                match self.lead_voice {
                    LeadVoice::SynthLead => "合成器主音",
                    LeadVoice::SynthPluck => "合成器拨弦",
                    LeadVoice::ElectricGuitar => "电吉他主音",
                    LeadVoice::DistortedGuitar => "失真吉他",
                    LeadVoice::AcousticGuitar => "木吉他主音",
                    LeadVoice::Strings => "弦乐主旋律",
                    LeadVoice::Piano => "钢琴主旋律",
                }
                .to_string(),
            );
        }
        if self.pad {
            parts.push("铺底".to_string());
        }
        parts.join(" · ")
    }
}

/// 内置范例工程：由某条模板扩展为更长的小节数。
/// Built-in example project: a template expanded to a longer bar count.
#[derive(Clone, Copy, Debug)]
pub struct ExampleProject {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub template_id: TemplateId,
    pub bars: u32,
    /// 写入 `examples/` 时使用的文件名（不含扩展名）。
    /// File name (without extension) used when writing under `examples/`.
    pub file_name: &'static str,
}

const COLORS: [[u8; 3]; 6] = [
    [45, 212, 191],
    [251, 191, 36],
    [129, 140, 248],
    [244, 114, 182],
    [52, 211, 153],
    [251, 146, 60],
];

const K_POP: &[u8] = &[0, 8];
const K_ROCK: &[u8] = &[0, 6, 8, 14];
const K_HIPHOP: &[u8] = &[0, 7, 10];
const K_TRAP: &[u8] = &[0, 7, 10];
const K_FOUR: &[u8] = &[0, 4, 8, 12];
const K_LOFI: &[u8] = &[0, 8];
const K_RNB: &[u8] = &[0, 7, 10];
const K_JAZZ: &[u8] = &[0];
const K_COUNTRY: &[u8] = &[0, 8];
const K_REGGAE: &[u8] = &[8];
const K_METAL: &[u8] = &[0, 2, 4, 6, 8, 10, 12, 14];
const K_FOLK: &[u8] = &[0, 8];
const K_BLUES: &[u8] = &[0, 8];
const K_LATIN: &[u8] = &[0, 8];
const K_SYNTHWAVE: &[u8] = &[0, 8];
const K_AMBIENT: &[u8] = &[0];
const K_FUNK: &[u8] = &[0, 6, 8, 14];

const S_BACKBEAT: &[u8] = &[4, 12];
const S_HALF: &[u8] = &[8];
const S_ONE_DROP: &[u8] = &[12];
const S_JAZZ: &[u8] = &[4, 12];
const S_CLAVE: &[u8] = &[0, 3, 6, 10, 12];
const S_EMPTY: &[u8] = &[];

const H_EIGHTH: &[u8] = &[0, 2, 4, 6, 8, 10, 12, 14];
const H_SIXTEENTH: &[u8] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
const H_OFFBEAT: &[u8] = &[2, 6, 10, 14];
const H_JAZZ: &[u8] = &[0, 3, 6, 8, 11, 14];
const H_SPARSE: &[u8] = &[0, 2, 5, 8, 10, 13];
const H_EMPTY: &[u8] = &[];

const OH_OFFBEAT: &[u8] = &[2, 6, 10, 14];
const OH_LAST: &[u8] = &[14];
const OH_EMPTY: &[u8] = &[];

const CLAP_BACKBEAT: &[u8] = &[4, 12];
const CLAP_TRAP: &[u8] = &[8];
const CLAP_EMPTY: &[u8] = &[];

const GHOST_EMPTY: &[u8] = &[];
const GHOST_FUNK: &[u8] = &[3, 7, 11, 15];
const GHOST_SOUL: &[u8] = &[7, 15];

const TOM_EMPTY: &[u8] = &[];
const TOM_FILL: &[u8] = &[13, 15];

const GROOVE_POP: DrumGroove = DrumGroove {
    kick: K_POP,
    snare: S_BACKBEAT,
    ghost: GHOST_EMPTY,
    hat: H_EIGHTH,
    open_hat: OH_LAST,
    clap: CLAP_EMPTY,
    tom: TOM_EMPTY,
    swing: 0.0,
};
const GROOVE_ROCK: DrumGroove = DrumGroove {
    kick: K_ROCK,
    snare: S_BACKBEAT,
    ghost: GHOST_EMPTY,
    hat: H_EIGHTH,
    open_hat: OH_LAST,
    clap: CLAP_EMPTY,
    tom: TOM_FILL,
    swing: 0.0,
};
const GROOVE_HIPHOP: DrumGroove = DrumGroove {
    kick: K_HIPHOP,
    snare: S_BACKBEAT,
    ghost: GHOST_SOUL,
    hat: H_EIGHTH,
    open_hat: OH_EMPTY,
    clap: CLAP_EMPTY,
    tom: TOM_EMPTY,
    swing: 0.25,
};
const GROOVE_TRAP: DrumGroove = DrumGroove {
    kick: K_TRAP,
    snare: S_HALF,
    ghost: GHOST_SOUL,
    hat: H_SIXTEENTH,
    open_hat: OH_LAST,
    clap: CLAP_TRAP,
    tom: TOM_EMPTY,
    swing: 0.0,
};
const GROOVE_HOUSE: DrumGroove = DrumGroove {
    kick: K_FOUR,
    snare: S_EMPTY,
    ghost: GHOST_EMPTY,
    hat: H_EIGHTH,
    open_hat: OH_OFFBEAT,
    clap: CLAP_BACKBEAT,
    tom: TOM_EMPTY,
    swing: 0.0,
};
const GROOVE_TECHNO: DrumGroove = DrumGroove {
    kick: K_FOUR,
    snare: S_EMPTY,
    ghost: GHOST_EMPTY,
    hat: H_EIGHTH,
    open_hat: OH_OFFBEAT,
    clap: CLAP_BACKBEAT,
    tom: TOM_EMPTY,
    swing: 0.0,
};
const GROOVE_LOFI: DrumGroove = DrumGroove {
    kick: K_LOFI,
    snare: S_BACKBEAT,
    ghost: GHOST_SOUL,
    hat: H_EIGHTH,
    open_hat: OH_EMPTY,
    clap: CLAP_EMPTY,
    tom: TOM_EMPTY,
    swing: 0.45,
};
const GROOVE_RNB: DrumGroove = DrumGroove {
    kick: K_RNB,
    snare: S_BACKBEAT,
    ghost: GHOST_SOUL,
    hat: H_SPARSE,
    open_hat: OH_EMPTY,
    clap: CLAP_EMPTY,
    tom: TOM_EMPTY,
    swing: 0.2,
};
const GROOVE_JAZZ: DrumGroove = DrumGroove {
    kick: K_JAZZ,
    snare: S_JAZZ,
    ghost: GHOST_SOUL,
    hat: H_JAZZ,
    open_hat: OH_EMPTY,
    clap: CLAP_EMPTY,
    tom: TOM_EMPTY,
    swing: 0.66,
};
const GROOVE_ORCHESTRAL: DrumGroove = DrumGroove {
    kick: &[0, 8],
    snare: S_EMPTY,
    ghost: GHOST_EMPTY,
    hat: H_EMPTY,
    open_hat: OH_EMPTY,
    clap: CLAP_EMPTY,
    tom: &[4, 12],
    swing: 0.0,
};
const GROOVE_COUNTRY: DrumGroove = DrumGroove {
    kick: K_COUNTRY,
    snare: S_BACKBEAT,
    ghost: GHOST_FUNK,
    hat: H_EIGHTH,
    open_hat: OH_EMPTY,
    clap: CLAP_EMPTY,
    tom: TOM_EMPTY,
    swing: 0.15,
};
const GROOVE_REGGAE: DrumGroove = DrumGroove {
    kick: K_REGGAE,
    snare: S_ONE_DROP,
    ghost: GHOST_EMPTY,
    hat: H_OFFBEAT,
    open_hat: OH_OFFBEAT,
    clap: CLAP_EMPTY,
    tom: TOM_EMPTY,
    swing: 0.0,
};
const GROOVE_METAL: DrumGroove = DrumGroove {
    kick: K_METAL,
    snare: S_BACKBEAT,
    ghost: GHOST_EMPTY,
    hat: H_EIGHTH,
    open_hat: OH_LAST,
    clap: CLAP_EMPTY,
    tom: TOM_EMPTY,
    swing: 0.0,
};
const GROOVE_FOLK: DrumGroove = DrumGroove {
    kick: K_FOLK,
    snare: S_BACKBEAT,
    ghost: GHOST_FUNK,
    hat: H_EIGHTH,
    open_hat: OH_EMPTY,
    clap: CLAP_EMPTY,
    tom: TOM_EMPTY,
    swing: 0.0,
};
const GROOVE_BLUES: DrumGroove = DrumGroove {
    kick: K_BLUES,
    snare: S_BACKBEAT,
    ghost: GHOST_SOUL,
    hat: H_JAZZ,
    open_hat: OH_EMPTY,
    clap: CLAP_EMPTY,
    tom: TOM_EMPTY,
    swing: 0.66,
};
const GROOVE_LATIN: DrumGroove = DrumGroove {
    kick: K_LATIN,
    snare: S_CLAVE,
    ghost: GHOST_EMPTY,
    hat: H_EIGHTH,
    open_hat: OH_EMPTY,
    clap: CLAP_EMPTY,
    tom: &[7, 15],
    swing: 0.0,
};
const GROOVE_SYNTHWAVE: DrumGroove = DrumGroove {
    kick: K_SYNTHWAVE,
    snare: S_BACKBEAT,
    ghost: GHOST_EMPTY,
    hat: H_EIGHTH,
    open_hat: OH_LAST,
    clap: CLAP_EMPTY,
    tom: TOM_FILL,
    swing: 0.0,
};
const GROOVE_AMBIENT: DrumGroove = DrumGroove {
    kick: K_AMBIENT,
    snare: S_EMPTY,
    ghost: GHOST_EMPTY,
    hat: H_EMPTY,
    open_hat: OH_EMPTY,
    clap: CLAP_EMPTY,
    tom: &[8],
    swing: 0.0,
};
const GROOVE_FUNK: DrumGroove = DrumGroove {
    kick: K_FUNK,
    snare: S_BACKBEAT,
    ghost: GHOST_FUNK,
    hat: H_SIXTEENTH,
    open_hat: OH_LAST,
    clap: CLAP_EMPTY,
    tom: TOM_EMPTY,
    swing: 0.12,
};

const BASS_POP: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 110,
        length_steps: 3,
    },
    BassStep {
        step: 8,
        interval: 0,
        velocity: 104,
        length_steps: 3,
    },
    BassStep {
        step: 11,
        interval: 7,
        velocity: 82,
        length_steps: 1,
    },
];
const BASS_ROCK: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 112,
        length_steps: 1,
    },
    BassStep {
        step: 2,
        interval: 0,
        velocity: 96,
        length_steps: 1,
    },
    BassStep {
        step: 4,
        interval: 7,
        velocity: 104,
        length_steps: 1,
    },
    BassStep {
        step: 6,
        interval: 0,
        velocity: 96,
        length_steps: 1,
    },
    BassStep {
        step: 8,
        interval: 0,
        velocity: 112,
        length_steps: 1,
    },
    BassStep {
        step: 10,
        interval: 0,
        velocity: 96,
        length_steps: 1,
    },
    BassStep {
        step: 12,
        interval: 7,
        velocity: 104,
        length_steps: 1,
    },
    BassStep {
        step: 14,
        interval: 0,
        velocity: 96,
        length_steps: 1,
    },
];
const BASS_HIPHOP: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 115,
        length_steps: 6,
    },
    BassStep {
        step: 7,
        interval: 0,
        velocity: 94,
        length_steps: 2,
    },
    BassStep {
        step: 10,
        interval: 0,
        velocity: 100,
        length_steps: 4,
    },
];
const BASS_TRAP: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 120,
        length_steps: 8,
    },
    BassStep {
        step: 10,
        interval: 0,
        velocity: 104,
        length_steps: 6,
    },
];
const BASS_HOUSE: &[BassStep] = &[
    BassStep {
        step: 2,
        interval: 0,
        velocity: 106,
        length_steps: 1,
    },
    BassStep {
        step: 6,
        interval: 0,
        velocity: 106,
        length_steps: 1,
    },
    BassStep {
        step: 10,
        interval: 0,
        velocity: 106,
        length_steps: 1,
    },
    BassStep {
        step: 14,
        interval: 0,
        velocity: 106,
        length_steps: 1,
    },
];
const BASS_TECHNO: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 112,
        length_steps: 1,
    },
    BassStep {
        step: 2,
        interval: 0,
        velocity: 92,
        length_steps: 1,
    },
    BassStep {
        step: 4,
        interval: 0,
        velocity: 106,
        length_steps: 1,
    },
    BassStep {
        step: 6,
        interval: 0,
        velocity: 92,
        length_steps: 1,
    },
    BassStep {
        step: 8,
        interval: 12,
        velocity: 112,
        length_steps: 1,
    },
    BassStep {
        step: 10,
        interval: 0,
        velocity: 92,
        length_steps: 1,
    },
    BassStep {
        step: 12,
        interval: 7,
        velocity: 106,
        length_steps: 1,
    },
    BassStep {
        step: 14,
        interval: 0,
        velocity: 92,
        length_steps: 1,
    },
];
const BASS_LOFI: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 100,
        length_steps: 4,
    },
    BassStep {
        step: 6,
        interval: 0,
        velocity: 88,
        length_steps: 2,
    },
    BassStep {
        step: 8,
        interval: 0,
        velocity: 100,
        length_steps: 4,
    },
    BassStep {
        step: 14,
        interval: 7,
        velocity: 84,
        length_steps: 2,
    },
];
const BASS_RNB: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 110,
        length_steps: 6,
    },
    BassStep {
        step: 7,
        interval: 0,
        velocity: 90,
        length_steps: 2,
    },
    BassStep {
        step: 10,
        interval: 0,
        velocity: 100,
        length_steps: 4,
    },
];
const BASS_JAZZ: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 104,
        length_steps: 3,
    },
    BassStep {
        step: 4,
        interval: 4,
        velocity: 96,
        length_steps: 3,
    },
    BassStep {
        step: 8,
        interval: 7,
        velocity: 100,
        length_steps: 3,
    },
    BassStep {
        step: 12,
        interval: 10,
        velocity: 94,
        length_steps: 3,
    },
];
const BASS_ORCHESTRAL: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 90,
        length_steps: 15,
    },
    BassStep {
        step: 8,
        interval: 7,
        velocity: 76,
        length_steps: 7,
    },
];
const BASS_COUNTRY: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 106,
        length_steps: 3,
    },
    BassStep {
        step: 4,
        interval: 7,
        velocity: 96,
        length_steps: 3,
    },
    BassStep {
        step: 8,
        interval: 0,
        velocity: 106,
        length_steps: 3,
    },
    BassStep {
        step: 12,
        interval: 7,
        velocity: 96,
        length_steps: 3,
    },
];
const BASS_REGGAE: &[BassStep] = &[
    BassStep {
        step: 2,
        interval: 0,
        velocity: 110,
        length_steps: 1,
    },
    BassStep {
        step: 6,
        interval: 0,
        velocity: 104,
        length_steps: 1,
    },
    BassStep {
        step: 10,
        interval: 0,
        velocity: 110,
        length_steps: 1,
    },
    BassStep {
        step: 14,
        interval: 0,
        velocity: 104,
        length_steps: 1,
    },
];
const BASS_METAL: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 116,
        length_steps: 1,
    },
    BassStep {
        step: 2,
        interval: 0,
        velocity: 104,
        length_steps: 1,
    },
    BassStep {
        step: 4,
        interval: 0,
        velocity: 112,
        length_steps: 1,
    },
    BassStep {
        step: 6,
        interval: 0,
        velocity: 104,
        length_steps: 1,
    },
    BassStep {
        step: 8,
        interval: 0,
        velocity: 116,
        length_steps: 1,
    },
    BassStep {
        step: 10,
        interval: 0,
        velocity: 104,
        length_steps: 1,
    },
    BassStep {
        step: 12,
        interval: 7,
        velocity: 112,
        length_steps: 1,
    },
    BassStep {
        step: 14,
        interval: 0,
        velocity: 104,
        length_steps: 1,
    },
];
const BASS_FOLK: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 100,
        length_steps: 3,
    },
    BassStep {
        step: 4,
        interval: 7,
        velocity: 90,
        length_steps: 3,
    },
    BassStep {
        step: 8,
        interval: 0,
        velocity: 100,
        length_steps: 3,
    },
    BassStep {
        step: 12,
        interval: 7,
        velocity: 90,
        length_steps: 3,
    },
];
const BASS_BLUES: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 106,
        length_steps: 2,
    },
    BassStep {
        step: 3,
        interval: 7,
        velocity: 90,
        length_steps: 2,
    },
    BassStep {
        step: 6,
        interval: 0,
        velocity: 100,
        length_steps: 2,
    },
    BassStep {
        step: 8,
        interval: 0,
        velocity: 106,
        length_steps: 2,
    },
    BassStep {
        step: 11,
        interval: 7,
        velocity: 90,
        length_steps: 2,
    },
    BassStep {
        step: 14,
        interval: 10,
        velocity: 100,
        length_steps: 2,
    },
];
const BASS_LATIN: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 106,
        length_steps: 3,
    },
    BassStep {
        step: 6,
        interval: 7,
        velocity: 94,
        length_steps: 2,
    },
    BassStep {
        step: 8,
        interval: 0,
        velocity: 106,
        length_steps: 3,
    },
    BassStep {
        step: 14,
        interval: 7,
        velocity: 94,
        length_steps: 2,
    },
];
const BASS_SYNTHWAVE: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 106,
        length_steps: 1,
    },
    BassStep {
        step: 2,
        interval: 0,
        velocity: 92,
        length_steps: 1,
    },
    BassStep {
        step: 4,
        interval: 0,
        velocity: 106,
        length_steps: 1,
    },
    BassStep {
        step: 6,
        interval: 0,
        velocity: 92,
        length_steps: 1,
    },
    BassStep {
        step: 8,
        interval: 12,
        velocity: 106,
        length_steps: 1,
    },
    BassStep {
        step: 10,
        interval: 0,
        velocity: 92,
        length_steps: 1,
    },
    BassStep {
        step: 12,
        interval: 7,
        velocity: 106,
        length_steps: 1,
    },
    BassStep {
        step: 14,
        interval: 0,
        velocity: 92,
        length_steps: 1,
    },
];
const BASS_AMBIENT: &[BassStep] = &[BassStep {
    step: 0,
    interval: 0,
    velocity: 88,
    length_steps: 15,
}];
const BASS_FUNK: &[BassStep] = &[
    BassStep {
        step: 0,
        interval: 0,
        velocity: 116,
        length_steps: 2,
    },
    BassStep {
        step: 3,
        interval: 0,
        velocity: 88,
        length_steps: 1,
    },
    BassStep {
        step: 6,
        interval: 7,
        velocity: 102,
        length_steps: 2,
    },
    BassStep {
        step: 8,
        interval: 0,
        velocity: 112,
        length_steps: 2,
    },
    BassStep {
        step: 11,
        interval: 0,
        velocity: 88,
        length_steps: 1,
    },
    BassStep {
        step: 14,
        interval: 10,
        velocity: 102,
        length_steps: 2,
    },
];

const M_POP: Motif = Motif {
    steps: &[0, 2, 4, 6, 8, 10, 12, 14],
    tones: &[0, 1, 2, 1, 3, 2, 1, 0],
    length_steps: 1,
    velocity: 96,
    octave: 1,
};
const M_SPARSE: Motif = Motif {
    steps: &[0, 6, 10, 14, 20, 26],
    tones: &[0, 2, 1, 2, 3, 2],
    length_steps: 2,
    velocity: 88,
    octave: 1,
};
const M_ARP: Motif = Motif {
    steps: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    tones: &[0, 1, 2, 3, 4, 5, 4, 3, 2, 1, 0, 1, 2, 3, 2, 1],
    length_steps: 1,
    velocity: 82,
    octave: 1,
};
const M_RIFF: Motif = Motif {
    steps: &[0, 2, 3, 4, 6, 8, 10, 12, 14],
    tones: &[0, 0, 1, 0, 2, 0, 1, 0, 2],
    length_steps: 1,
    velocity: 112,
    octave: 0,
};
const M_LONG: Motif = Motif {
    steps: &[0, 16],
    tones: &[0, 2],
    length_steps: 8,
    velocity: 78,
    octave: 1,
};
const M_STABS: Motif = Motif {
    steps: &[0, 3, 6, 8, 11, 14],
    tones: &[0, 1, 2, 1, 2, 0],
    length_steps: 1,
    velocity: 92,
    octave: 1,
};

/// 全部内置模板（按主流风格覆盖排序）。
/// All built-in templates, ordered by mainstream-style coverage.
static TEMPLATES: [MusicTemplate; 19] = [
    MusicTemplate {
        id: "pop",
        name: "流行 Pop",
        style: "Pop",
        family: TemplateFamily::Pop,
        bpm: 112.0,
        key_root: 60,
        key_label: "C 大调",
        scale: Scale::Major,
        description: "I–V–vi–IV 进行的通用流行骨架，钢琴和声加合成器主旋律，适合快速写副歌。",
        tags: &["Piano", "Hook", "Radio"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 4,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Minor,
            },
            ChordSpec {
                degree: 3,
                quality: ChordQuality::Major,
            },
        ],
        drums: GROOVE_POP,
        bass: BASS_POP,
        harmony: HarmonyPattern::Half,
        harmony_voice: HarmonyVoice::Piano,
        lead: Some(M_POP),
        lead_voice: LeadVoice::SynthLead,
        pad: false,
        bars: 8,
    },
    MusicTemplate {
        id: "rock",
        name: "摇滚 Rock",
        style: "Rock",
        family: TemplateFamily::Rock,
        bpm: 128.0,
        key_root: 52,
        key_label: "E 小调",
        scale: Scale::Minor,
        description: "强拍底鼓与 2/4 拍军鼓，电吉他分解和弦加五声音阶主音，适合摇滚副歌。",
        tags: &["Guitar", "Backbeat", "Drive"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Minor,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 2,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 6,
                quality: ChordQuality::Major,
            },
        ],
        drums: GROOVE_ROCK,
        bass: BASS_ROCK,
        harmony: HarmonyPattern::Broken,
        harmony_voice: HarmonyVoice::ElectricGuitar,
        lead: Some(M_POP),
        lead_voice: LeadVoice::ElectricGuitar,
        pad: false,
        bars: 8,
    },
    MusicTemplate {
        id: "hiphop",
        name: "嘻哈 Hip-Hop",
        style: "Boom Bap",
        family: TemplateFamily::Urban,
        bpm: 90.0,
        key_root: 57,
        key_label: "A 小调",
        scale: Scale::Minor,
        description: "Boom Bap 律动：偏后的军鼓、摇摆八分音符与采样感钢琴和弦，留出说唱空间。",
        tags: &["Swing", "Sample", "Boom Bap"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Minor,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 2,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 6,
                quality: ChordQuality::Major,
            },
        ],
        drums: GROOVE_HIPHOP,
        bass: BASS_HIPHOP,
        harmony: HarmonyPattern::Sustain,
        harmony_voice: HarmonyVoice::Piano,
        lead: None,
        lead_voice: LeadVoice::SynthPluck,
        pad: false,
        bars: 8,
    },
    MusicTemplate {
        id: "trap",
        name: "陷阱 Trap",
        style: "Trap",
        family: TemplateFamily::Urban,
        bpm: 140.0,
        key_root: 54,
        key_label: "F# 小调",
        scale: Scale::Minor,
        description: "半速军鼓 + 16 分踩镲 + 长音 808，暗色合成器铺底，适合 Trap / Drill 写作。",
        tags: &["808", "Half-time", "Dark"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Minor,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 2,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 6,
                quality: ChordQuality::Major,
            },
        ],
        drums: GROOVE_TRAP,
        bass: BASS_TRAP,
        harmony: HarmonyPattern::Sustain,
        harmony_voice: HarmonyVoice::SynthPad,
        lead: Some(M_SPARSE),
        lead_voice: LeadVoice::SynthPluck,
        pad: true,
        bars: 8,
    },
    MusicTemplate {
        id: "house",
        name: "浩室 House",
        style: "House",
        family: TemplateFamily::Electronic,
        bpm: 124.0,
        key_root: 57,
        key_label: "A 小调",
        scale: Scale::Minor,
        description: "四踩底鼓、反拍开镲与反拍贝斯，配合切分和弦，是 120–128 BPM 舞曲的通用骨架。",
        tags: &["Four-on-floor", "Offbeat", "Club"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Minor,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 2,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 6,
                quality: ChordQuality::Major,
            },
        ],
        drums: GROOVE_HOUSE,
        bass: BASS_HOUSE,
        harmony: HarmonyPattern::Offbeat,
        harmony_voice: HarmonyVoice::SynthPluck,
        lead: None,
        lead_voice: LeadVoice::SynthLead,
        pad: true,
        bars: 8,
    },
    MusicTemplate {
        id: "techno",
        name: "科技舞曲 Techno",
        style: "Techno",
        family: TemplateFamily::Electronic,
        bpm: 132.0,
        key_root: 57,
        key_label: "A 小调",
        scale: Scale::Minor,
        description: "四踩底鼓、16 分脉冲贝斯与短促和弦，强调机械律动与留白。",
        tags: &["Club", "Pulse", "Minimal"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Minor,
            },
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Minor,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 6,
                quality: ChordQuality::Major,
            },
        ],
        drums: GROOVE_TECHNO,
        bass: BASS_TECHNO,
        harmony: HarmonyPattern::Stabs,
        harmony_voice: HarmonyVoice::SynthPluck,
        lead: Some(M_ARP),
        lead_voice: LeadVoice::SynthPluck,
        pad: false,
        bars: 8,
    },
    MusicTemplate {
        id: "lofi",
        name: "低保真 Lo-fi",
        style: "Lo-fi Hip-Hop",
        family: TemplateFamily::Urban,
        bpm: 82.0,
        key_root: 50,
        key_label: "D 小调",
        scale: Scale::Minor,
        description: "慢速摇摆鼓组、柔和钢琴分解与温暖贝斯，适合 Lo-fi / Chillhop 学习歌单。",
        tags: &["Chill", "Swing", "Study"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Minor7,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Major7,
            },
            ChordSpec {
                degree: 2,
                quality: ChordQuality::Major7,
            },
            ChordSpec {
                degree: 6,
                quality: ChordQuality::Dominant7,
            },
        ],
        drums: GROOVE_LOFI,
        bass: BASS_LOFI,
        harmony: HarmonyPattern::Broken,
        harmony_voice: HarmonyVoice::Piano,
        lead: Some(M_SPARSE),
        lead_voice: LeadVoice::Piano,
        pad: true,
        bars: 8,
    },
    MusicTemplate {
        id: "rnb",
        name: "节奏布鲁斯 R&B",
        style: "Contemporary R&B",
        family: TemplateFamily::Urban,
        bpm: 92.0,
        key_root: 58,
        key_label: "Bb 大调",
        scale: Scale::Major,
        description: "ii–V–I–vi 爵士化和声、切分鼓组与柔和电钢琴，适合现代 R&B 写作。",
        tags: &["Soul", "Groove", "Keys"],
        chords: &[
            ChordSpec {
                degree: 1,
                quality: ChordQuality::Minor7,
            },
            ChordSpec {
                degree: 4,
                quality: ChordQuality::Dominant7,
            },
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Major7,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Minor7,
            },
        ],
        drums: GROOVE_RNB,
        bass: BASS_RNB,
        harmony: HarmonyPattern::Comp,
        harmony_voice: HarmonyVoice::SynthPluck,
        lead: Some(M_POP),
        lead_voice: LeadVoice::SynthPluck,
        pad: true,
        bars: 8,
    },
    MusicTemplate {
        id: "jazz",
        name: "爵士 Jazz",
        style: "Jazz",
        family: TemplateFamily::Jazz,
        bpm: 132.0,
        key_root: 60,
        key_label: "C 大调",
        scale: Scale::Major,
        description: "ii–V–I–vi 七和弦、Walking Bass 与摇摆 ride，适合标准曲与爵士练习。",
        tags: &["Swing", "ii-V-I", "Standards"],
        chords: &[
            ChordSpec {
                degree: 1,
                quality: ChordQuality::Minor7,
            },
            ChordSpec {
                degree: 4,
                quality: ChordQuality::Dominant7,
            },
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Major7,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Minor7,
            },
        ],
        drums: GROOVE_JAZZ,
        bass: BASS_JAZZ,
        harmony: HarmonyPattern::Comp,
        harmony_voice: HarmonyVoice::Piano,
        lead: Some(M_POP),
        lead_voice: LeadVoice::Piano,
        pad: false,
        bars: 8,
    },
    MusicTemplate {
        id: "orchestral",
        name: "管弦 Orchestral",
        style: "Cinematic",
        family: TemplateFamily::Classical,
        bpm: 96.0,
        key_root: 60,
        key_label: "C 大调",
        scale: Scale::Major,
        description: "I–vi–IV–V 弦乐长音、低音弦与定音鼓，适合影视配乐与古典风格入门。",
        tags: &["Strings", "Cinematic", "Score"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Minor,
            },
            ChordSpec {
                degree: 3,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 4,
                quality: ChordQuality::Major,
            },
        ],
        drums: GROOVE_ORCHESTRAL,
        bass: BASS_ORCHESTRAL,
        harmony: HarmonyPattern::Sustain,
        harmony_voice: HarmonyVoice::Strings,
        lead: Some(M_LONG),
        lead_voice: LeadVoice::Strings,
        pad: true,
        bars: 8,
    },
    MusicTemplate {
        id: "country",
        name: "乡村 Country",
        style: "Country",
        family: TemplateFamily::Roots,
        bpm: 108.0,
        key_root: 55,
        key_label: "G 大调",
        scale: Scale::Major,
        description: "I–IV–V 进行、根五贝斯与木吉他指弹，适合乡村 / 美式民谣。",
        tags: &["Acoustic", "Roots", "Story"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 3,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 4,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Major,
            },
        ],
        drums: GROOVE_COUNTRY,
        bass: BASS_COUNTRY,
        harmony: HarmonyPattern::Fingerpick,
        harmony_voice: HarmonyVoice::AcousticGuitar,
        lead: Some(M_POP),
        lead_voice: LeadVoice::AcousticGuitar,
        pad: false,
        bars: 8,
    },
    MusicTemplate {
        id: "reggae",
        name: "雷鬼 Reggae",
        style: "Reggae",
        family: TemplateFamily::Roots,
        bpm: 76.0,
        key_root: 57,
        key_label: "A 小调",
        scale: Scale::Minor,
        description: "One Drop 鼓组、反拍吉他切音与反拍贝斯，是雷鬼 / Dub 的核心律动。",
        tags: &["One Drop", "Skank", "Dub"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Minor,
            },
            ChordSpec {
                degree: 6,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 6,
                quality: ChordQuality::Major,
            },
        ],
        drums: GROOVE_REGGAE,
        bass: BASS_REGGAE,
        harmony: HarmonyPattern::Offbeat,
        harmony_voice: HarmonyVoice::ElectricGuitar,
        lead: None,
        lead_voice: LeadVoice::SynthLead,
        pad: false,
        bars: 8,
    },
    MusicTemplate {
        id: "metal",
        name: "金属 Metal",
        style: "Metal",
        family: TemplateFamily::Rock,
        bpm: 160.0,
        key_root: 52,
        key_label: "E 小调",
        scale: Scale::Minor,
        description: "16 分双踩底鼓、失真吉他强力和弦与暗色主音，适合现代金属。",
        tags: &["Distortion", "Double Kick", "Riff"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Minor,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 2,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 6,
                quality: ChordQuality::Major,
            },
        ],
        drums: GROOVE_METAL,
        bass: BASS_METAL,
        harmony: HarmonyPattern::Stabs,
        harmony_voice: HarmonyVoice::ElectricGuitar,
        lead: Some(M_RIFF),
        lead_voice: LeadVoice::DistortedGuitar,
        pad: false,
        bars: 8,
    },
    MusicTemplate {
        id: "folk",
        name: "民谣 Folk",
        style: "Folk",
        family: TemplateFamily::Roots,
        bpm: 92.0,
        key_root: 55,
        key_label: "G 大调",
        scale: Scale::Major,
        description: "I–V–vi–IV 木吉他指弹与轻鼓组，适合 Singer-Songwriter 民谣。",
        tags: &["Acoustic", "Songwriter", "Warm"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 4,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Minor,
            },
            ChordSpec {
                degree: 3,
                quality: ChordQuality::Major,
            },
        ],
        drums: GROOVE_FOLK,
        bass: BASS_FOLK,
        harmony: HarmonyPattern::Fingerpick,
        harmony_voice: HarmonyVoice::AcousticGuitar,
        lead: Some(M_POP),
        lead_voice: LeadVoice::AcousticGuitar,
        pad: false,
        bars: 8,
    },
    MusicTemplate {
        id: "blues",
        name: "蓝调 Blues",
        style: "Blues",
        family: TemplateFamily::Roots,
        bpm: 96.0,
        key_root: 57,
        key_label: "A 蓝调",
        scale: Scale::Blues,
        description: "12 小节蓝调骨架的 4 小节循环：I7–IV7–I7–V7，摇摆鼓组与电吉他，适合即兴。",
        tags: &["12 Bar", "Shuffle", "Guitar"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Dominant7,
            },
            ChordSpec {
                degree: 3,
                quality: ChordQuality::Dominant7,
            },
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Dominant7,
            },
            ChordSpec {
                degree: 4,
                quality: ChordQuality::Dominant7,
            },
        ],
        drums: GROOVE_BLUES,
        bass: BASS_BLUES,
        harmony: HarmonyPattern::Comp,
        harmony_voice: HarmonyVoice::ElectricGuitar,
        lead: Some(M_STABS),
        lead_voice: LeadVoice::ElectricGuitar,
        pad: false,
        bars: 8,
    },
    MusicTemplate {
        id: "latin",
        name: "拉丁 / 巴萨诺瓦 Latin",
        style: "Bossa Nova",
        family: TemplateFamily::Other,
        bpm: 128.0,
        key_root: 53,
        key_label: "F 大调",
        scale: Scale::Major,
        description: "Bossa Nova 的 I–vi–ii–V 进行、钢琴 montuno 与拉丁打击乐，适合轻快拉丁风。",
        tags: &["Bossa", "Clave", "Montuno"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Major7,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Minor7,
            },
            ChordSpec {
                degree: 1,
                quality: ChordQuality::Minor7,
            },
            ChordSpec {
                degree: 4,
                quality: ChordQuality::Dominant7,
            },
        ],
        drums: GROOVE_LATIN,
        bass: BASS_LATIN,
        harmony: HarmonyPattern::Montuno,
        harmony_voice: HarmonyVoice::Piano,
        lead: Some(M_POP),
        lead_voice: LeadVoice::Piano,
        pad: false,
        bars: 8,
    },
    MusicTemplate {
        id: "synthwave",
        name: "合成器浪潮 Synthwave",
        style: "Synthwave",
        family: TemplateFamily::Electronic,
        bpm: 100.0,
        key_root: 57,
        key_label: "A 小调",
        scale: Scale::Minor,
        description: "复古合成器琶音、门限鼓组与 warm pad，适合 80 年代 Retrowave。",
        tags: &["Retro", "Arp", "80s"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Minor,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 2,
                quality: ChordQuality::Major,
            },
            ChordSpec {
                degree: 6,
                quality: ChordQuality::Major,
            },
        ],
        drums: GROOVE_SYNTHWAVE,
        bass: BASS_SYNTHWAVE,
        harmony: HarmonyPattern::Arp8,
        harmony_voice: HarmonyVoice::SynthPluck,
        lead: Some(M_ARP),
        lead_voice: LeadVoice::SynthLead,
        pad: true,
        bars: 8,
    },
    MusicTemplate {
        id: "ambient",
        name: "氛围 Ambient",
        style: "Ambient",
        family: TemplateFamily::Electronic,
        bpm: 70.0,
        key_root: 50,
        key_label: "D 大调",
        scale: Scale::Major,
        description: "长音 pad、稀疏低音与极简主旋律，留出空间感，适合氛围 / 配乐。",
        tags: &["Pad", "Space", "Cinematic"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Major7,
            },
            ChordSpec {
                degree: 4,
                quality: ChordQuality::Major7,
            },
            ChordSpec {
                degree: 5,
                quality: ChordQuality::Minor7,
            },
            ChordSpec {
                degree: 3,
                quality: ChordQuality::Major7,
            },
        ],
        drums: GROOVE_AMBIENT,
        bass: BASS_AMBIENT,
        harmony: HarmonyPattern::Sustain,
        harmony_voice: HarmonyVoice::SynthPad,
        lead: Some(M_LONG),
        lead_voice: LeadVoice::Strings,
        pad: true,
        bars: 8,
    },
    MusicTemplate {
        id: "funk",
        name: "放克 Funk",
        style: "Funk",
        family: TemplateFamily::Urban,
        bpm: 108.0,
        key_root: 52,
        key_label: "E 小调",
        scale: Scale::Minor,
        description: "16 分踩镲、切分贝斯与电吉他短和弦，强调反拍与律动空隙。",
        tags: &["Groove", "Syncopation", "Guitar"],
        chords: &[
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Minor7,
            },
            ChordSpec {
                degree: 3,
                quality: ChordQuality::Dominant7,
            },
            ChordSpec {
                degree: 0,
                quality: ChordQuality::Minor7,
            },
            ChordSpec {
                degree: 6,
                quality: ChordQuality::Dominant7,
            },
        ],
        drums: GROOVE_FUNK,
        bass: BASS_FUNK,
        harmony: HarmonyPattern::Stabs,
        harmony_voice: HarmonyVoice::ElectricGuitar,
        lead: Some(M_STABS),
        lead_voice: LeadVoice::ElectricGuitar,
        pad: false,
        bars: 8,
    },
];

/// 内置范例工程目录（由模板扩展为 16 小节）。
/// Built-in example catalog (templates expanded to 16 bars).
static EXAMPLES: [ExampleProject; 6] = [
    ExampleProject {
        id: "example-synthwave",
        name: "城市霓虹",
        description: "16 小节 Synthwave 范例：琶音、门限鼓组与复古主音。",
        template_id: "synthwave",
        bars: 16,
        file_name: "城市霓虹_合成器浪潮",
    },
    ExampleProject {
        id: "example-lofi",
        name: "午后咖啡",
        description: "16 小节 Lo-fi 范例：摇摆鼓组、柔和钢琴与温暖贝斯。",
        template_id: "lofi",
        bars: 16,
        file_name: "午后咖啡_低保真",
    },
    ExampleProject {
        id: "example-orchestral",
        name: "晨光序曲",
        description: "16 小节管弦范例：弦乐长音、定音鼓与电影感主旋律。",
        template_id: "orchestral",
        bars: 16,
        file_name: "晨光序曲_管弦",
    },
    ExampleProject {
        id: "example-jazz",
        name: "午夜爵士",
        description: "16 小节爵士范例：ii–V–I、Walking Bass 与摇摆鼓组。",
        template_id: "jazz",
        bars: 16,
        file_name: "午夜爵士_爵士",
    },
    ExampleProject {
        id: "example-metal",
        name: "赤色风暴",
        description: "16 小节金属范例：双踩底鼓、失真节奏吉他与强力和弦。",
        template_id: "metal",
        bars: 16,
        file_name: "赤色风暴_金属",
    },
    ExampleProject {
        id: "example-latin",
        name: "热带午后",
        description: "16 小节 Bossa Nova 范例：钢琴 montuno 与拉丁打击乐。",
        template_id: "latin",
        bars: 16,
        file_name: "热带午后_巴萨诺瓦",
    },
];

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：返回全部模板。
/// 输入：无。
/// 输出/副作用：静态切片；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / music templates
/// Purpose: Return all templates.
/// Inputs: none.
/// Outputs / side effects: static slice; no state change.
/// Complexity: O(1) time and space.
pub fn templates() -> &'static [MusicTemplate] {
    &TEMPLATES
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：按 id 查找模板。
/// 输入：`id` 模板标识。
/// 输出/副作用：模板引用或 `None`；不修改状态。
/// 复杂度：时间 O(模板数)，空间 O(1)。
///
/// Module: core / data model / music templates
/// Purpose: Find a template by id.
/// Inputs: `id` template identifier.
/// Outputs / side effects: template reference or `None`; no state change.
/// Complexity: O(template count) time and O(1) space.
pub fn find_template(id: &str) -> Option<&'static MusicTemplate> {
    TEMPLATES.iter().find(|template| template.id == id)
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：返回全部范例工程。
/// 输入：无。
/// 输出/副作用：静态切片；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / music templates
/// Purpose: Return all example projects.
/// Inputs: none.
/// Outputs / side effects: static slice; no state change.
/// Complexity: O(1) time and space.
pub fn example_projects() -> &'static [ExampleProject] {
    &EXAMPLES
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：按 id 查找范例。
/// 输入：`id` 范例标识。
/// 输出/副作用：范例引用或 `None`；不修改状态。
/// 复杂度：时间 O(范例数)，空间 O(1)。
///
/// Module: core / data model / music templates
/// Purpose: Find an example by id.
/// Inputs: `id` example identifier.
/// Outputs / side effects: example reference or `None`; no state change.
/// Complexity: O(example count) time and O(1) space.
pub fn find_example(id: &str) -> Option<&'static ExampleProject> {
    EXAMPLES.iter().find(|example| example.id == id)
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：返回模板或范例的显示名称（用于新工程窗口标题）。
/// 输入：`id` 模板/范例标识。
/// 输出/副作用：静态名称或 `None`；不修改状态。
/// 复杂度：时间 O(模板数 + 范例数)，空间 O(1)。
///
/// Module: core / data model / music templates
/// Purpose: Return the display name of a template or example (used by the new window title).
/// Inputs: `id` template/example identifier.
/// Outputs / side effects: static name or `None`; no state change.
/// Complexity: O(template count + example count) time and O(1) space.
pub fn display_name(id: &str) -> Option<&'static str> {
    find_template(id)
        .map(|template| template.name)
        .or_else(|| find_example(id).map(|example| example.name))
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：把模板展开为工程。
/// 输入：`template` 模板。
/// 输出/副作用：返回新工程；不修改模板。
/// 复杂度：时间 O(小节数 × 音符数)，空间 O(音符总数)。
///
/// Module: core / data model / music templates
/// Purpose: Expand a template into a project.
/// Inputs: `template` template.
/// Outputs / side effects: new project; the template is unchanged.
/// Complexity: O(bars × notes) time and O(total notes) space.
pub fn build_project(template: &MusicTemplate) -> Project {
    build_project_with_bars(template, template.bars, template.name)
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：按指定小节数与工程名展开模板（范例工程复用同一生成器）。
/// 输入：
/// - `template`：模板。
/// - `bars`：小节数，至少 4。
/// - `name`：工程名。
/// 输出/副作用：返回新工程；不修改模板。
/// 复杂度：时间 O(小节数 × 音符数)，空间 O(音符总数)。
///
/// Module: core / data model / music templates
/// Purpose: Expand a template with a given bar count and project name (examples reuse the builder).
/// Inputs:
/// - `template`: template.
/// - `bars`: bar count, at least 4.
/// - `name`: project name.
/// Outputs / side effects: new project; the template is unchanged.
/// Complexity: O(bars × notes) time and O(total notes) space.
pub fn build_project_with_bars(template: &MusicTemplate, bars: u32, name: &str) -> Project {
    let bars = bars.max(4);
    let length_beats = bars as f64 * 4.0;
    let mut project = Project {
        name: name.to_string(),
        bpm: template.bpm,
        time_sig_num: 4,
        time_sig_den: 4,
        length_beats,
        loop_enabled: true,
        loop_start: 0.0,
        loop_end: length_beats,
        ..Default::default()
    };
    add_drum_track(&mut project, template, bars);
    add_bass_track(&mut project, template, bars);
    add_harmony_track(&mut project, template, bars);
    add_lead_track(&mut project, template, bars);
    add_pad_track(&mut project, template, bars);
    project
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：把范例展开为工程。
/// 输入：`example` 范例。
/// 输出/副作用：返回新工程；不修改范例。
/// 复杂度：时间 O(小节数 × 音符数)，空间 O(音符总数)。
///
/// Module: core / data model / music templates
/// Purpose: Expand an example into a project.
/// Inputs: `example` example.
/// Outputs / side effects: new project; the example is unchanged.
/// Complexity: O(bars × notes) time and O(total notes) space.
pub fn build_example_project(example: &ExampleProject) -> Option<Project> {
    let template = find_template(example.template_id)?;
    Some(build_project_with_bars(
        template,
        example.bars,
        example.name,
    ))
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：按模板/范例 id 生成工程。
/// 输入：`id` 模板或范例标识。
/// 输出/副作用：工程或 `None`；不修改状态。
/// 复杂度：时间 O(小节数 × 音符数)，空间 O(音符总数)。
///
/// Module: core / data model / music templates
/// Purpose: Build a project from a template or example id.
/// Inputs: `id` template or example identifier.
/// Outputs / side effects: project or `None`; no state change.
/// Complexity: O(bars × notes) time and O(total notes) space.
pub fn build_by_id(id: &str) -> Option<Project> {
    if let Some(template) = find_template(id) {
        return Some(build_project(template));
    }
    let example = find_example(id)?;
    build_example_project(example)
}

impl Project {
    /// 所属模块：core / 数据模型 / 音乐模板
    /// 功能：按模板或范例 id 构造工程；未知 id 返回 `None`。
    /// 输入：`id` 模板/范例标识。
    /// 输出/副作用：返回新工程；不修改其他状态。
    /// 复杂度：时间 O(小节数 × 音符数)，空间 O(音符总数)。
    ///
    /// Module: core / data model / music templates
    /// Purpose: Build a project from a template or example id; unknown ids return `None`.
    /// Inputs: `id` template/example identifier.
    /// Outputs / side effects: new project; no other state change.
    /// Complexity: O(bars × notes) time and O(total notes) space.
    pub fn from_template(id: &str) -> Option<Project> {
        build_by_id(id)
    }
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：生成鼓组轨道（16 分格子，含力度变化与摇摆）。
/// 输入：`project` 目标工程；`template` 模板；`bars` 小节数。
/// 输出/副作用：向工程追加一条鼓组轨道与片段。
/// 复杂度：时间 O(小节数 × 鼓点数)，空间 O(鼓点总数)。
///
/// Module: core / data model / music templates
/// Purpose: Generate the drum track (16th grid with velocity variation and swing).
/// Inputs: `project` target project; `template` template; `bars` bar count.
/// Outputs / side effects: appends one drum track and clip.
/// Complexity: O(bars × hits) time and O(total hits) space.
fn add_drum_track(project: &mut Project, template: &MusicTemplate, bars: u32) {
    let notes = drum_notes(template, bars);
    let instrument = InstrumentRef::Drums(DrumSettings {
        level: 0.9,
        tone: 0.55,
    });
    push_track(project, "鼓组", instrument, 0.85, 0.0, notes);
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：生成贝斯轨道（和弦根音 + 模板节奏型）。
/// 输入：`project` 目标工程；`template` 模板；`bars` 小节数。
/// 输出/副作用：向工程追加一条贝斯轨道与片段。
/// 复杂度：时间 O(小节数 × Bass 步数)，空间 O(音符总数)。
///
/// Module: core / data model / music templates
/// Purpose: Generate the bass track (chord root plus the template rhythm).
/// Inputs: `project` target project; `template` template; `bars` bar count.
/// Outputs / side effects: appends one bass track and clip.
/// Complexity: O(bars × bass steps) time and O(total notes) space.
fn add_bass_track(project: &mut Project, template: &MusicTemplate, bars: u32) {
    let notes = bass_notes(template, bars);
    let instrument = bass_instrument(template);
    push_track(project, "贝斯", instrument, 0.8, 0.0, notes);
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：生成和声/键盘轨道（钢琴、吉他、弦乐、合成器琶音等）。
/// 输入：`project` 目标工程；`template` 模板；`bars` 小节数。
/// 输出/副作用：向工程追加一条和声轨道与片段。
/// 复杂度：时间 O(小节数 × 和弦音数)，空间 O(音符总数)。
///
/// Module: core / data model / music templates
/// Purpose: Generate the harmony/keyboard track (piano, guitar, strings, synth arp, ...).
/// Inputs: `project` target project; `template` template; `bars` bar count.
/// Outputs / side effects: appends one harmony track and clip.
/// Complexity: O(bars × chord tones) time and O(total notes) space.
fn add_harmony_track(project: &mut Project, template: &MusicTemplate, bars: u32) {
    let notes = harmony_notes(template, bars);
    let Some(instrument) = harmony_instrument(template.harmony_voice) else {
        return;
    };
    let (volume, pan) = match template.harmony_voice {
        HarmonyVoice::Piano => (0.64, -0.08),
        HarmonyVoice::AcousticGuitar => (0.7, -0.12),
        HarmonyVoice::ElectricGuitar => (0.62, -0.18),
        HarmonyVoice::Strings => (0.56, 0.0),
        HarmonyVoice::SynthPad => (0.5, 0.0),
        HarmonyVoice::SynthPluck => (0.56, 0.1),
        HarmonyVoice::Organ => (0.52, 0.0),
    };
    push_track(project, "和弦", instrument, volume, pan, notes);
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：生成主旋律轨道；模板没有动机时跳过。
/// 输入：`project` 目标工程；`template` 模板；`bars` 小节数。
/// 输出/副作用：向工程追加一条主旋律轨道与片段。
/// 复杂度：时间 O(小节数 × 动机步数)，空间 O(音符总数)。
///
/// Module: core / data model / music templates
/// Purpose: Generate the lead track; skipped when the template has no motif.
/// Inputs: `project` target project; `template` template; `bars` bar count.
/// Outputs / side effects: appends one lead track and clip.
/// Complexity: O(bars × motif steps) time and O(total notes) space.
fn add_lead_track(project: &mut Project, template: &MusicTemplate, bars: u32) {
    if template.lead.is_none() {
        return;
    }
    let notes = lead_notes(template, bars);
    let instrument = lead_instrument(template.lead_voice);
    push_track(project, "主旋律", instrument, 0.58, 0.16, notes);
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：生成铺底轨道；模板未启用 pad 时跳过。
/// 输入：`project` 目标工程；`template` 模板；`bars` 小节数。
/// 输出/副作用：向工程追加一条合成器铺底轨道与片段。
/// 复杂度：时间 O(小节数 × 和弦音数)，空间 O(音符总数)。
///
/// Module: core / data model / music templates
/// Purpose: Generate the pad track; skipped when the template disables pads.
/// Inputs: `project` target project; `template` template; `bars` bar count.
/// Outputs / side effects: appends one synth-pad track and clip.
/// Complexity: O(bars × chord tones) time and O(total notes) space.
fn add_pad_track(project: &mut Project, template: &MusicTemplate, bars: u32) {
    if !template.pad {
        return;
    }
    let notes = pad_notes(template, bars);
    let instrument = InstrumentRef::Synth(SynthSettings {
        waveform: Waveform::Saw,
        attack: 0.55,
        decay: 1.0,
        sustain: 0.9,
        release: 1.8,
        cutoff: 2200.0,
        resonance: 0.1,
        detune: 0.22,
        octave: 0,
        level: 0.42,
    });
    push_track(project, "铺底", instrument, 0.42, -0.1, notes);
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：把模板鼓组格子展开为 MIDI 音符（含摇摆与强弱拍力度）。
/// 输入：`template` 模板；`bars` 小节数。
/// 输出/副作用：返回鼓组音符；不修改模板。
/// 复杂度：时间 O(小节数 × 每小节鼓点数)，空间 O(鼓点总数)。
///
/// Module: core / data model / music templates
/// Purpose: Expand the template drum grid into MIDI notes (with swing and accent velocities).
/// Inputs: `template` template; `bars` bar count.
/// Outputs / side effects: drum notes; the template is unchanged.
/// Complexity: O(bars × hits per bar) time and O(total hits) space.
fn drum_notes(template: &MusicTemplate, bars: u32) -> Vec<Note> {
    let groove = template.drums;
    let mut notes = Vec::new();
    for bar in 0..bars {
        let base = bar as f64 * 4.0;
        for &step in groove.kick {
            push_note(
                &mut notes,
                base + step_beats(step, groove.swing),
                0.12,
                36,
                118,
            );
        }
        for &step in groove.snare {
            push_note(
                &mut notes,
                base + step_beats(step, groove.swing),
                0.12,
                38,
                108,
            );
        }
        for &step in groove.ghost {
            push_note(
                &mut notes,
                base + step_beats(step, groove.swing),
                0.1,
                38,
                46,
            );
        }
        for &step in groove.hat {
            let velocity = if step % 4 == 0 { 72 } else { 54 };
            push_note(
                &mut notes,
                base + step_beats(step, groove.swing),
                0.08,
                42,
                velocity,
            );
        }
        for &step in groove.open_hat {
            push_note(
                &mut notes,
                base + step_beats(step, groove.swing),
                0.2,
                46,
                82,
            );
        }
        for &step in groove.clap {
            push_note(
                &mut notes,
                base + step_beats(step, groove.swing),
                0.12,
                39,
                100,
            );
        }
        for &step in groove.tom {
            push_note(
                &mut notes,
                base + step_beats(step, groove.swing),
                0.16,
                45,
                96,
            );
        }
    }
    notes
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：把 Bass 节奏型展开为音符；音高以和弦根音下移两个八度为基准。
/// 输入：`template` 模板；`bars` 小节数。
/// 输出/副作用：返回贝斯音符；不修改模板。
/// 复杂度：时间 O(小节数 × Bass 步数)，空间 O(音符总数)。
///
/// Module: core / data model / music templates
/// Purpose: Expand the bass pattern into notes, based two octaves below the chord root.
/// Inputs: `template` template; `bars` bar count.
/// Outputs / side effects: bass notes; the template is unchanged.
/// Complexity: O(bars × bass steps) time and O(total notes) space.
fn bass_notes(template: &MusicTemplate, bars: u32) -> Vec<Note> {
    let swing = template.drums.swing;
    let mut notes = Vec::new();
    for bar in 0..bars {
        let chord = chord_at(template, bar as usize);
        let root = chord_root(template, chord) - 24;
        for step in template.bass {
            let start = bar as f64 * 4.0 + step_beats(step.step, swing);
            let length = step.length_steps.max(1) as f64 * 0.25;
            push_note(
                &mut notes,
                start,
                length,
                root + step.interval,
                step.velocity,
            );
        }
    }
    notes
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：按模板默认和声写法生成和声音符。
/// 输入：`template` 模板；`bars` 小节数。
/// 输出/副作用：返回和声音符；不修改模板。
/// 复杂度：时间 O(小节数 × 和弦音数)，空间 O(音符总数)。
///
/// Module: core / data model / music templates
/// Purpose: Generate harmony notes using the template's default harmony pattern.
/// Inputs: `template` template; `bars` bar count.
/// Outputs / side effects: harmony notes; the template is unchanged.
/// Complexity: O(bars × chord tones) time and O(total notes) space.
fn harmony_notes(template: &MusicTemplate, bars: u32) -> Vec<Note> {
    harmony_notes_with(template, bars, template.harmony)
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：按指定和声写法生成音符（模板与 pad 复用）。
/// 输入：`template` 模板；`bars` 小节数；`pattern` 和声写法。
/// 输出/副作用：返回和声音符；不修改模板。
/// 复杂度：时间 O(小节数 × 和弦音数)，空间 O(音符总数)。
///
/// Module: core / data model / music templates
/// Purpose: Generate notes with a given harmony pattern (shared by templates and pads).
/// Inputs: `template` template; `bars` bar count; `pattern` harmony pattern.
/// Outputs / side effects: harmony notes; the template is unchanged.
/// Complexity: O(bars × chord tones) time and O(total notes) space.
fn harmony_notes_with(template: &MusicTemplate, bars: u32, pattern: HarmonyPattern) -> Vec<Note> {
    let mut notes = Vec::new();
    for bar in 0..bars {
        let chord = chord_at(template, bar as usize);
        let tones = chord_pitches(template, chord);
        let base = bar as f64 * 4.0;
        match pattern {
            HarmonyPattern::Sustain => push_chord(&mut notes, &tones, base, 3.85, 74),
            HarmonyPattern::Half => {
                push_chord(&mut notes, &tones, base, 1.85, 76);
                push_chord(&mut notes, &tones, base + 2.0, 1.85, 70);
            }
            HarmonyPattern::Offbeat => {
                for offset in [0.5, 1.5, 2.5, 3.5] {
                    push_chord(&mut notes, &tones, base + offset, 0.22, 84);
                }
            }
            HarmonyPattern::Stabs => {
                for step in [0_u8, 3, 6, 8, 11, 14] {
                    push_chord(
                        &mut notes,
                        &tones,
                        base + step_beats(step, template.drums.swing),
                        0.22,
                        88,
                    );
                }
            }
            HarmonyPattern::Arp8 => {
                for index in 0..8 {
                    let pitch = arp_pitch(&tones, index);
                    push_note(&mut notes, base + index as f64 * 0.5, 0.42, pitch, 82);
                }
            }
            HarmonyPattern::Arp16 => {
                for index in 0..16 {
                    let pitch = arp_pitch(&tones, index);
                    push_note(&mut notes, base + index as f64 * 0.25, 0.2, pitch, 76);
                }
            }
            HarmonyPattern::Broken => {
                for (index, tone) in [0_u8, 2, 1, 3].iter().enumerate() {
                    let pitch = tone_pitch(&tones, *tone);
                    push_note(&mut notes, base + index as f64, 0.9, pitch, 78);
                }
            }
            HarmonyPattern::Comp => {
                for step in [0_u8, 3, 6, 10, 12] {
                    push_chord(
                        &mut notes,
                        &tones,
                        base + step_beats(step, template.drums.swing),
                        0.28,
                        80,
                    );
                }
            }
            HarmonyPattern::Tremolo => {
                for index in 0..16 {
                    push_chord(&mut notes, &tones, base + index as f64 * 0.25, 0.15, 62);
                }
            }
            HarmonyPattern::Fingerpick => {
                for (index, tone) in [0_u8, 2, 3, 1, 2, 3, 1, 2].iter().enumerate() {
                    let pitch = tone_pitch(&tones, *tone);
                    push_note(&mut notes, base + index as f64 * 0.5, 0.48, pitch, 82);
                }
            }
            HarmonyPattern::Montuno => {
                for (index, step) in [0_u8, 3, 6, 8, 11, 14].iter().enumerate() {
                    let tone = [0_u8, 2, 1, 3, 2, 1][index];
                    let pitch = tone_pitch(&tones, tone);
                    push_note(
                        &mut notes,
                        base + step_beats(*step, template.drums.swing),
                        0.22,
                        pitch,
                        84,
                    );
                }
            }
        }
    }
    notes
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：把 2 小节主旋律动机展开到整段；音高取当前小节和弦音。
/// 输入：`template` 模板；`bars` 小节数。
/// 输出/副作用：返回主旋律音符；不修改模板。
/// 复杂度：时间 O(小节数 × 动机步数)，空间 O(音符总数)。
///
/// Module: core / data model / music templates
/// Purpose: Expand the two-bar lead motif across the arrangement, taking chord tones per bar.
/// Inputs: `template` template; `bars` bar count.
/// Outputs / side effects: lead notes; the template is unchanged.
/// Complexity: O(bars × motif steps) time and O(total notes) space.
fn lead_notes(template: &MusicTemplate, bars: u32) -> Vec<Note> {
    let Some(motif) = template.lead else {
        return Vec::new();
    };
    let mut notes = Vec::new();
    let groups = (bars + 1) / 2;
    for group in 0..groups {
        for (index, &step) in motif.steps.iter().enumerate() {
            if step >= 32 {
                continue;
            }
            let Some(&tone) = motif.tones.get(index) else {
                continue;
            };
            let bar = group * 2 + (step / 16) as u32;
            if bar >= bars {
                continue;
            }
            let chord = chord_at(template, bar as usize);
            let pitch = chord_tone(template, chord, tone, motif.octave);
            let start = bar as f64 * 4.0 + step_beats(step % 16, template.drums.swing);
            let length = motif.length_steps.max(1) as f64 * 0.25;
            push_note(&mut notes, start, length, pitch, motif.velocity);
        }
    }
    notes
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：生成铺底长音（复用 Sustain 和声写法）。
/// 输入：`template` 模板；`bars` 小节数。
/// 输出/副作用：返回铺底音符；不修改模板。
/// 复杂度：时间 O(小节数 × 和弦音数)，空间 O(音符总数)。
///
/// Module: core / data model / music templates
/// Purpose: Generate sustained pad notes (reusing the Sustain harmony pattern).
/// Inputs: `template` template; `bars` bar count.
/// Outputs / side effects: pad notes; the template is unchanged.
/// Complexity: O(bars × chord tones) time and O(total notes) space.
fn pad_notes(template: &MusicTemplate, bars: u32) -> Vec<Note> {
    harmony_notes_with(template, bars, HarmonyPattern::Sustain)
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：取指定小节的和弦；和弦表为空时回退到主和弦。
/// 输入：`template` 模板；`bar` 小节索引。
/// 输出/副作用：返回和弦配置；不修改模板。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / music templates
/// Purpose: Return the chord for a bar, falling back to the tonic when the table is empty.
/// Inputs: `template` template; `bar` bar index.
/// Outputs / side effects: chord spec; the template is unchanged.
/// Complexity: O(1) time and space.
fn chord_at(template: &MusicTemplate, bar: usize) -> ChordSpec {
    if template.chords.is_empty() {
        ChordSpec {
            degree: 0,
            quality: ChordQuality::Major,
        }
    } else {
        template.chords[bar % template.chords.len()]
    }
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：计算和弦根音的 MIDI 音高（主音 + 音阶级数）。
/// 输入：`template` 模板；`chord` 和弦配置。
/// 输出/副作用：返回 MIDI 音高；不修改模板。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / music templates
/// Purpose: Compute the chord-root MIDI pitch (tonic plus scale degree).
/// Inputs: `template` template; `chord` chord spec.
/// Outputs / side effects: MIDI pitch; the template is unchanged.
/// Complexity: O(1) time and space.
fn chord_root(template: &MusicTemplate, chord: ChordSpec) -> i32 {
    template.key_root as i32 + template.scale.semitone(chord.degree)
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：返回和弦的 MIDI 音高列表。
/// 输入：`template` 模板；`chord` 和弦配置。
/// 输出/副作用：返回音高列表；不修改模板。
/// 复杂度：时间 O(和弦音数)，空间 O(和弦音数)。
///
/// Module: core / data model / music templates
/// Purpose: Return the MIDI pitches of a chord.
/// Inputs: `template` template; `chord` chord spec.
/// Outputs / side effects: pitch list; the template is unchanged.
/// Complexity: O(chord tones) time and space.
fn chord_pitches(template: &MusicTemplate, chord: ChordSpec) -> Vec<i32> {
    let root = chord_root(template, chord);
    chord
        .quality
        .intervals()
        .iter()
        .map(|interval| root + interval)
        .collect()
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：按和弦音索引取音高，索引超过和弦音数时向上叠八度。
/// 输入：`tones` 和弦音高；`index` 和弦音索引。
/// 输出/副作用：返回 MIDI 音高；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / music templates
/// Purpose: Get a chord-tone pitch by index, adding octaves when the index exceeds the chord size.
/// Inputs: `tones` chord pitches; `index` chord-tone index.
/// Outputs / side effects: MIDI pitch; no state change.
/// Complexity: O(1) time and space.
fn tone_pitch(tones: &[i32], index: u8) -> i32 {
    if tones.is_empty() {
        return 60;
    }
    let length = tones.len() as i32;
    let index = index as i32;
    tones[index.rem_euclid(length) as usize] + 12 * index.div_euclid(length)
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：按琶音序号取和弦音高，序号超过和弦音数时向上叠八度。
/// 输入：`tones` 和弦音高；`index` 琶音序号。
/// 输出/副作用：返回 MIDI 音高；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / music templates
/// Purpose: Get an arpeggio pitch by step, adding octaves when the step exceeds the chord size.
/// Inputs: `tones` chord pitches; `index` arpeggio step.
/// Outputs / side effects: MIDI pitch; no state change.
/// Complexity: O(1) time and space.
fn arp_pitch(tones: &[i32], index: usize) -> i32 {
    if tones.is_empty() {
        return 60;
    }
    let length = tones.len();
    tones[index % length] + 12 * (index / length) as i32
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：取和弦音并叠加八度，用于主旋律动机。
/// 输入：`template` 模板；`chord` 和弦；`tone` 和弦音索引；`octave` 八度偏移。
/// 输出/副作用：返回 MIDI 音高；不修改模板。
/// 复杂度：时间 O(和弦音数)，空间 O(和弦音数)。
///
/// Module: core / data model / music templates
/// Purpose: Get a chord tone plus an octave offset for the lead motif.
/// Inputs: `template` template; `chord` chord; `tone` chord-tone index; `octave` octave offset.
/// Outputs / side effects: MIDI pitch; the template is unchanged.
/// Complexity: O(chord tones) time and space.
fn chord_tone(template: &MusicTemplate, chord: ChordSpec, tone: u8, octave: i32) -> i32 {
    let mut tones = chord_pitches(template, chord);
    for pitch in &mut tones {
        *pitch += 12 * octave;
    }
    tone_pitch(&tones, tone)
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：把一组和弦音写成同时发声的音符。
/// 输入：`notes` 输出列表；`tones` 和弦音高；`start` 起始 beat；`length` 长度 beat；`velocity` 力度。
/// 输出/副作用：向 `notes` 追加音符；不修改其他状态。
/// 复杂度：时间 O(和弦音数)，空间 O(和弦音数)。
///
/// Module: core / data model / music templates
/// Purpose: Write a chord as simultaneous notes.
/// Inputs: `notes` output list; `tones` chord pitches; `start` start beat; `length` length in beats;
/// `velocity` MIDI velocity.
/// Outputs / side effects: appends notes; no other state change.
/// Complexity: O(chord tones) time and space.
fn push_chord(notes: &mut Vec<Note>, tones: &[i32], start: f64, length: f64, velocity: u8) {
    for &pitch in tones {
        push_note(notes, start, length, pitch, velocity);
    }
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：写入一个音符，并把音高/力度限制到 MIDI 合法范围。
/// 输入：`notes` 输出列表；`start` 起始 beat；`length` 长度 beat；`pitch` MIDI 音高；`velocity` 力度。
/// 输出/副作用：向 `notes` 追加音符；不修改其他状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / music templates
/// Purpose: Push one note, clamping pitch and velocity to the MIDI range.
/// Inputs: `notes` output list; `start` start beat; `length` length in beats; `pitch` MIDI pitch;
/// `velocity` MIDI velocity.
/// Outputs / side effects: appends one note; no other state change.
/// Complexity: O(1) time and space.
fn push_note(notes: &mut Vec<Note>, start: f64, length: f64, pitch: i32, velocity: u8) {
    notes.push(Note {
        start,
        length,
        pitch: pitch.clamp(0, 127) as u8,
        velocity: velocity.clamp(1, 127),
    });
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：创建一条轨道与一个覆盖整段的片段，并加入工程。
/// 输入：`project` 目标工程；`name` 轨道名；`instrument` 乐器；`volume` 音量；`pan` 声像；`notes` 音符。
/// 输出/副作用：分配轨道/片段 id、选择配色并追加到工程。
/// 复杂度：时间 O(1)，空间 O(音符数)（所有权转移）。
///
/// Module: core / data model / music templates
/// Purpose: Create a track and a whole-length clip, then add them to the project.
/// Inputs: `project` target project; `name` track name; `instrument` instrument; `volume` level;
/// `pan` pan; `notes` notes.
/// Outputs / side effects: allocates track/clip ids, picks a colour and appends the track.
/// Complexity: O(1) time and O(note count) space (ownership is moved).
fn push_track(
    project: &mut Project,
    name: &str,
    instrument: InstrumentRef,
    volume: f32,
    pan: f32,
    notes: Vec<Note>,
) {
    let id = project.next_track_id();
    let clip_id = project.next_clip_id();
    let mut track = Track::new(
        id,
        name,
        COLORS[project.tracks.len() % COLORS.len()],
        instrument,
    );
    track.volume = volume;
    track.pan = pan;
    track.expanded = true;
    let mut clip = Clip::new(clip_id, name, 0.0, project.length_beats);
    clip.notes = notes;
    track.clips.push(clip);
    project.tracks.push(track);
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：把 16 分格子序号换算为 beat，并按摇摆量推迟弱拍。
/// 输入：`step` 16 分格子序号（0..15）；`swing` 摇摆量（0..1）。
/// 输出/副作用：返回相对小节起点的 beat；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / music templates
/// Purpose: Convert a 16th-grid step into beats, delaying weak steps by the swing amount.
/// Inputs: `step` 16th-grid index (0..15); `swing` swing amount (0..1).
/// Outputs / side effects: beat offset from the bar start; no state change.
/// Complexity: O(1) time and space.
fn step_beats(step: u8, swing: f64) -> f64 {
    let mut position = step as f64 * 0.25;
    if swing > 0.0 {
        if step % 4 == 2 {
            position += swing * 0.16;
        } else if step % 2 == 1 {
            position += swing * 0.08;
        }
    }
    position
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：创建原生乐器并按模板覆盖音量/音色。
/// 输入：`kind` 乐器类型；`level` 音量；`tone` 音色。
/// 输出/副作用：返回乐器引用；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / music templates
/// Purpose: Create a native instrument with template-level volume/tone overrides.
/// Inputs: `kind` instrument kind; `level` volume; `tone` tone.
/// Outputs / side effects: instrument reference; no state change.
/// Complexity: O(1) time and space.
fn native_with(kind: NativeInstrumentKind, level: f32, tone: f32) -> InstrumentRef {
    let mut settings = NativeInstrumentSettings::for_kind(kind);
    settings.level = level;
    settings.tone = tone;
    InstrumentRef::Native(NativeInstrument { kind, settings })
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：按和声音色构造乐器（钢琴、吉他、弦乐、合成器 pad/pluck/风琴）。
/// 输入：`voice` 和声音色。
/// 输出/副作用：返回乐器引用；未知音色不可能出现。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / music templates
/// Purpose: Build the harmony instrument (piano, guitar, strings, synth pad/pluck/organ).
/// Inputs: `voice` harmony voice.
/// Outputs / side effects: instrument reference; unknown voices cannot occur.
/// Complexity: O(1) time and space.
fn harmony_instrument(voice: HarmonyVoice) -> Option<InstrumentRef> {
    Some(match voice {
        HarmonyVoice::Piano => native_with(NativeInstrumentKind::Piano, 0.8, 0.62),
        HarmonyVoice::AcousticGuitar => {
            native_with(NativeInstrumentKind::AcousticGuitar, 0.78, 0.62)
        }
        HarmonyVoice::ElectricGuitar => {
            native_with(NativeInstrumentKind::ElectricGuitar, 0.7, 0.62)
        }
        HarmonyVoice::Strings => native_with(NativeInstrumentKind::StringLegato, 0.62, 0.5),
        HarmonyVoice::SynthPad => InstrumentRef::Synth(SynthSettings {
            waveform: Waveform::Saw,
            attack: 0.4,
            decay: 0.8,
            sustain: 0.85,
            release: 1.4,
            cutoff: 2600.0,
            resonance: 0.1,
            detune: 0.2,
            octave: 0,
            level: 0.5,
        }),
        HarmonyVoice::SynthPluck => InstrumentRef::Synth(SynthSettings {
            waveform: Waveform::Triangle,
            attack: 0.003,
            decay: 0.24,
            sustain: 0.0,
            release: 0.24,
            cutoff: 5200.0,
            resonance: 0.16,
            detune: 0.05,
            octave: 0,
            level: 0.52,
        }),
        HarmonyVoice::Organ => InstrumentRef::Synth(SynthSettings {
            waveform: Waveform::Square,
            attack: 0.01,
            decay: 0.15,
            sustain: 0.8,
            release: 0.12,
            cutoff: 4600.0,
            resonance: 0.08,
            detune: 0.02,
            octave: 0,
            level: 0.42,
        }),
    })
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：按主旋律音色构造乐器。
/// 输入：`voice` 主旋律音色。
/// 输出/副作用：返回乐器引用；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / music templates
/// Purpose: Build the lead instrument.
/// Inputs: `voice` lead voice.
/// Outputs / side effects: instrument reference; no state change.
/// Complexity: O(1) time and space.
fn lead_instrument(voice: LeadVoice) -> InstrumentRef {
    match voice {
        LeadVoice::SynthLead => InstrumentRef::Synth(SynthSettings {
            waveform: Waveform::Saw,
            attack: 0.01,
            decay: 0.2,
            sustain: 0.55,
            release: 0.25,
            cutoff: 6800.0,
            resonance: 0.2,
            detune: 0.08,
            octave: 0,
            level: 0.52,
        }),
        LeadVoice::SynthPluck => InstrumentRef::Synth(SynthSettings {
            waveform: Waveform::Triangle,
            attack: 0.002,
            decay: 0.2,
            sustain: 0.1,
            release: 0.22,
            cutoff: 6200.0,
            resonance: 0.12,
            detune: 0.04,
            octave: 0,
            level: 0.55,
        }),
        LeadVoice::ElectricGuitar => native_with(NativeInstrumentKind::ElectricGuitar, 0.7, 0.64),
        LeadVoice::DistortedGuitar => {
            native_with(NativeInstrumentKind::DistortedGuitar, 0.62, 0.55)
        }
        LeadVoice::AcousticGuitar => native_with(NativeInstrumentKind::AcousticGuitar, 0.76, 0.64),
        LeadVoice::Strings => native_with(NativeInstrumentKind::StringLegato, 0.64, 0.52),
        LeadVoice::Piano => native_with(NativeInstrumentKind::Piano, 0.78, 0.64),
    }
}

/// 所属模块：core / 数据模型 / 音乐模板
/// 功能：按模板家族选择贝斯音色（电子类用合成器，其余用原生贝斯）。
/// 输入：`template` 模板。
/// 输出/副作用：返回乐器引用；不修改模板。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / music templates
/// Purpose: Pick the bass voice by family (synth for electronic, native bass otherwise).
/// Inputs: `template` template.
/// Outputs / side effects: instrument reference; the template is unchanged.
/// Complexity: O(1) time and space.
fn bass_instrument(template: &MusicTemplate) -> InstrumentRef {
    match template.family {
        TemplateFamily::Electronic => InstrumentRef::Synth(SynthSettings {
            waveform: Waveform::Square,
            attack: 0.003,
            decay: 0.18,
            sustain: 0.55,
            release: 0.14,
            cutoff: 900.0,
            resonance: 0.12,
            detune: 0.02,
            octave: -1,
            level: 0.6,
        }),
        _ => native_with(NativeInstrumentKind::BassGuitar, 0.84, 0.42),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：全部模板都能生成至少三条轨道且音符在 MIDI 合法范围内。
    /// Covers: every template builds at least three tracks with notes inside the MIDI range.
    #[test]
    fn every_template_builds_playable_notes() {
        for template in templates() {
            let project = build_project(template);
            assert!(
                project.tracks.len() >= 3,
                "{} has too few tracks",
                template.name
            );
            let mut note_count = 0;
            for track in &project.tracks {
                for clip in &track.clips {
                    for note in &clip.notes {
                        note_count += 1;
                        assert!(note.length > 0.0, "{} zero-length note", template.name);
                        assert!(note.pitch <= 127, "{} pitch overflow", template.name);
                        assert!(note.velocity > 0, "{} zero velocity", template.name);
                    }
                }
            }
            assert!(note_count > 0, "{} produced no notes", template.name);
        }
    }

    /// 覆盖：模板目录覆盖全部模板家族，且条数足以代表主流风格。
    /// Covers: the catalog covers every template family with enough entries for mainstream styles.
    #[test]
    fn templates_cover_mainstream_families() {
        assert!(templates().len() >= 17, "template catalog too small");
        for family in TemplateFamily::ALL {
            assert!(
                templates().iter().any(|template| template.family == family),
                "missing family {}",
                family.label()
            );
        }
    }

    /// 覆盖：全部范例都能展开为比模板更长、带音符的工程。
    /// Covers: every example expands into a longer project with notes.
    #[test]
    fn examples_expand_into_longer_projects() {
        for example in example_projects() {
            let project = build_example_project(example).expect("example template");
            assert_eq!(project.name, example.name);
            assert!(project.length_beats >= example.bars as f64 * 4.0);
            assert!(
                project
                    .tracks
                    .iter()
                    .flat_map(|track| track.clips.iter())
                    .any(|clip| !clip.notes.is_empty()),
                "{} produced no notes",
                example.name
            );
        }
    }
}
