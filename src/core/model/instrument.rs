//! 所属模块：core / 数据模型 / 乐器引用
//! 文件用途：
//! - 定义内置合成器、鼓机与原生乐器的音色参数。
//! - 定义 `InstrumentRef`（合成器 / 鼓机 / 原生乐器 / 插件）与稳定身份键。
//!
//! 函数清单：
//! - `Waveform::label` / `ALL`：波形显示名与全部波形。
//! - `SynthSettings` / `DrumSettings` / `NativeInstrumentSettings` 的 `Default`：默认音色。
//! - `NativeInstrumentKind::label` / `key` / `ALL`：原生乐器名称、身份键与列表。
//! - `NativeInstrumentSettings::for_kind`：按乐器类型生成默认参数。
//! - `NativeInstrument::new` / `InstrumentRef::label` / `key`：乐器引用构造与展示。
//!
//! Module: core / data model / instrument references
//! Purpose:
//! - Define tone settings for the built-in synth, drum kit and native instruments.
//! - Define `InstrumentRef` (synth / drums / native / plugin) and its stable identity keys.
//!
//! Functions:
//! - `Waveform::label` / `ALL`: waveform labels and the full waveform list.
//! - `Default` for `SynthSettings` / `DrumSettings` / `NativeInstrumentSettings`: default tones.
//! - `NativeInstrumentKind::label` / `key` / `ALL`: native names, identity keys and list.
//! - `NativeInstrumentSettings::for_kind`: kind-specific default parameters.
//! - `NativeInstrument::new` / `InstrumentRef::label` / `key`: reference construction and labels.

use super::plugin::PluginRef;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Waveform {
    Sine,
    Saw,
    Square,
    Triangle,
}

impl Waveform {
    pub fn label(&self) -> &'static str {
        match self {
            Waveform::Sine => "正弦",
            Waveform::Saw => "锯齿",
            Waveform::Square => "方波",
            Waveform::Triangle => "三角",
        }
    }

    pub const ALL: [Waveform; 4] = [
        Waveform::Sine,
        Waveform::Saw,
        Waveform::Square,
        Waveform::Triangle,
    ];
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SynthSettings {
    pub waveform: Waveform,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub cutoff: f32,
    pub resonance: f32,
    pub detune: f32,
    pub octave: i32,
    pub level: f32,
}

impl Default for SynthSettings {
    fn default() -> Self {
        Self {
            waveform: Waveform::Saw,
            attack: 0.008,
            decay: 0.18,
            sustain: 0.62,
            release: 0.22,
            cutoff: 5200.0,
            resonance: 0.18,
            detune: 0.06,
            octave: 0,
            level: 0.75,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrumSettings {
    pub level: f32,
    pub tone: f32,
}

impl Default for DrumSettings {
    fn default() -> Self {
        Self {
            level: 0.9,
            tone: 0.5,
        }
    }
}

/// 内置建模/采样式原生乐器类型。
/// Built-in modeled native instrument kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeInstrumentKind {
    Piano,
    AcousticGuitar,
    ElectricGuitar,
    DistortedGuitar,
    BassGuitar,
    Violin,
    StringStaccato,
    StringPizzicato,
    StringLegato,
}

impl NativeInstrumentKind {
    /// 所属模块：core / 数据模型 / 乐器引用
    /// 功能：返回原生乐器的中文显示名。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / instrument reference
    /// Purpose: Return the Chinese display name of a native instrument.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(&self) -> &'static str {
        match self {
            NativeInstrumentKind::Piano => "钢琴",
            NativeInstrumentKind::AcousticGuitar => "木吉他",
            NativeInstrumentKind::ElectricGuitar => "电吉他",
            NativeInstrumentKind::DistortedGuitar => "失真电吉他",
            NativeInstrumentKind::BassGuitar => "贝斯",
            NativeInstrumentKind::Violin => "小提琴",
            NativeInstrumentKind::StringStaccato => "弦乐断奏",
            NativeInstrumentKind::StringPizzicato => "弦乐拨奏",
            NativeInstrumentKind::StringLegato => "弦乐联奏",
        }
    }

    /// 所属模块：core / 数据模型 / 乐器引用
    /// 功能：返回原生乐器的稳定身份键，用于运行时复用判断。
    /// 输入：`self`。
    /// 输出/副作用：静态字符串；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / instrument reference
    /// Purpose: Return the stable identity key used to reuse a runtime instance.
    /// Inputs: `self`.
    /// Outputs / side effects: static string; no state change.
    /// Complexity: O(1) time and space.
    pub fn key(&self) -> &'static str {
        match self {
            NativeInstrumentKind::Piano => "piano",
            NativeInstrumentKind::AcousticGuitar => "acoustic-guitar",
            NativeInstrumentKind::ElectricGuitar => "electric-guitar",
            NativeInstrumentKind::DistortedGuitar => "distorted-guitar",
            NativeInstrumentKind::BassGuitar => "bass-guitar",
            NativeInstrumentKind::Violin => "violin",
            NativeInstrumentKind::StringStaccato => "strings-staccato",
            NativeInstrumentKind::StringPizzicato => "strings-pizzicato",
            NativeInstrumentKind::StringLegato => "strings-legato",
        }
    }

    pub const ALL: [NativeInstrumentKind; 9] = [
        NativeInstrumentKind::Piano,
        NativeInstrumentKind::AcousticGuitar,
        NativeInstrumentKind::ElectricGuitar,
        NativeInstrumentKind::DistortedGuitar,
        NativeInstrumentKind::BassGuitar,
        NativeInstrumentKind::Violin,
        NativeInstrumentKind::StringStaccato,
        NativeInstrumentKind::StringPizzicato,
        NativeInstrumentKind::StringLegato,
    ];
}

/// 原生乐器的公共音色参数（归一化或秒）。
/// Shared tone parameters for native instruments (normalized values or seconds).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NativeInstrumentSettings {
    pub level: f32,
    pub tone: f32,
    pub attack: f32,
    pub release: f32,
    pub sustain: f32,
    pub drive: f32,
    pub vibrato: f32,
}

impl Default for NativeInstrumentSettings {
    /// 所属模块：core / 数据模型 / 乐器引用
    /// 功能：提供通用的默认音色参数。
    /// 输入：无。
    /// 输出/副作用：返回默认参数；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / instrument reference
    /// Purpose: Provide generic default tone parameters.
    /// Inputs: none.
    /// Outputs / side effects: default settings; no state change.
    /// Complexity: O(1) time and space.
    fn default() -> Self {
        Self {
            level: 0.75,
            tone: 0.5,
            attack: 0.005,
            release: 0.5,
            sustain: 0.65,
            drive: 0.0,
            vibrato: 0.0,
        }
    }
}

impl NativeInstrumentSettings {
    /// 所属模块：core / 数据模型 / 乐器引用
    /// 功能：按乐器类型生成合适的默认音色参数。
    /// 输入：`kind` 原生乐器类型。
    /// 输出/副作用：返回参数集合；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / instrument reference
    /// Purpose: Build kind-specific default tone parameters.
    /// Inputs: `kind` native instrument kind.
    /// Outputs / side effects: settings; no state change.
    /// Complexity: O(1) time and space.
    pub fn for_kind(kind: NativeInstrumentKind) -> Self {
        let mut settings = Self::default();
        match kind {
            NativeInstrumentKind::Piano => {
                settings.level = 0.82;
                settings.tone = 0.62;
                settings.attack = 0.0015;
                settings.release = 0.7;
                settings.sustain = 0.72;
            }
            NativeInstrumentKind::AcousticGuitar => {
                settings.level = 0.8;
                settings.tone = 0.62;
                settings.attack = 0.003;
                settings.release = 1.2;
                settings.sustain = 0.6;
                settings.drive = 0.05;
            }
            NativeInstrumentKind::ElectricGuitar => {
                settings.level = 0.72;
                settings.tone = 0.62;
                settings.attack = 0.003;
                settings.release = 1.0;
                settings.sustain = 0.65;
                settings.drive = 0.18;
                settings.vibrato = 0.04;
            }
            NativeInstrumentKind::DistortedGuitar => {
                settings.level = 0.62;
                settings.tone = 0.55;
                settings.attack = 0.002;
                settings.release = 0.9;
                settings.sustain = 0.72;
                settings.drive = 0.8;
                settings.vibrato = 0.03;
            }
            NativeInstrumentKind::BassGuitar => {
                settings.level = 0.85;
                settings.tone = 0.38;
                settings.attack = 0.003;
                settings.release = 0.9;
                settings.sustain = 0.7;
                settings.drive = 0.1;
            }
            NativeInstrumentKind::Violin => {
                settings.level = 0.7;
                settings.tone = 0.58;
                settings.attack = 0.12;
                settings.release = 0.45;
                settings.sustain = 0.88;
                settings.vibrato = 0.32;
            }
            NativeInstrumentKind::StringStaccato => {
                settings.level = 0.68;
                settings.tone = 0.62;
                settings.attack = 0.02;
                settings.release = 0.16;
                settings.sustain = 0.45;
                settings.vibrato = 0.18;
            }
            NativeInstrumentKind::StringPizzicato => {
                settings.level = 0.75;
                settings.tone = 0.55;
                settings.attack = 0.002;
                settings.release = 0.35;
                settings.sustain = 0.4;
            }
            NativeInstrumentKind::StringLegato => {
                settings.level = 0.68;
                settings.tone = 0.52;
                settings.attack = 0.28;
                settings.release = 0.9;
                settings.sustain = 0.95;
                settings.vibrato = 0.45;
            }
        }
        settings
    }
}

/// 原生乐器引用：类型 + 可调音色。
/// Native instrument reference: kind plus adjustable tone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NativeInstrument {
    pub kind: NativeInstrumentKind,
    #[serde(default)]
    pub settings: NativeInstrumentSettings,
}

impl NativeInstrument {
    /// 所属模块：core / 数据模型 / 乐器引用
    /// 功能：按类型创建带默认音色的原生乐器。
    /// 输入：`kind` 原生乐器类型。
    /// 输出/副作用：返回新的乐器引用；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / instrument reference
    /// Purpose: Create a native instrument with kind-specific defaults.
    /// Inputs: `kind` native instrument kind.
    /// Outputs / side effects: the new reference; no state change.
    /// Complexity: O(1) time and space.
    pub fn new(kind: NativeInstrumentKind) -> Self {
        Self {
            kind,
            settings: NativeInstrumentSettings::for_kind(kind),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum InstrumentRef {
    Synth(SynthSettings),
    Drums(DrumSettings),
    Native(NativeInstrument),
    Plugin(PluginRef),
}

impl InstrumentRef {
    pub fn label(&self) -> String {
        match self {
            InstrumentRef::Synth(_) => "内置合成器".to_string(),
            InstrumentRef::Drums(_) => "内置鼓机".to_string(),
            InstrumentRef::Native(native) => native.kind.label().to_string(),
            InstrumentRef::Plugin(p) => {
                if p.name.is_empty() {
                    p.path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "插件".to_string())
                } else {
                    p.name.clone()
                }
            }
        }
    }

    pub fn key(&self) -> String {
        match self {
            InstrumentRef::Synth(_) => "builtin:synth".into(),
            InstrumentRef::Drums(_) => "builtin:drums".into(),
            InstrumentRef::Native(native) => format!("builtin:{}", native.kind.key()),
            InstrumentRef::Plugin(p) => p.key(),
        }
    }
}
