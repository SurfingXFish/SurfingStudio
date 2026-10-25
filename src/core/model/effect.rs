//! 所属模块：core / 数据模型 / 效果器引用
//! 文件用途：
//! - 定义内置原生效果器的类型、参数与启用开关。
//! - 定义效果链引用 `EffectRef`：VST 插件效果或内置原生效果共用同一链位置。
//! - 保持旧工程兼容：旧工程的效果链是 `PluginRef` 数组，反序列化后自动落为 `EffectRef::Plugin`。
//!
//! 函数清单：
//! - `NativeEffectKind::label` / `key` / `params`：效果器显示名、身份键与可调参数列表。
//! - `NativeEffectParam::label` / `display`：参数显示名与数值文本。
//! - `NativeEffectSettings::get` / `get_mut`：按参数枚举读写归一化值。
//! - `NativeEffectSettings::for_kind`：按效果器类型生成默认参数。
//! - `NativeEffect::new`：创建默认启用的内置效果器。
//! - `EffectRef::enabled` / `set_enabled` / `key` / `label` / `badge`：链引用通用查询。
//! - `EffectRef::plugin` / `plugin_mut` / `native` / `native_mut`：按类型访问内部引用。
//! - `default_enabled`：`enabled` 的 serde 默认值。
//!
//! Module: core / data model / effect reference
//! Purpose:
//! - Define built-in native effect kinds, their parameters and the enable switch.
//! - Define `EffectRef`, so a chain slot can hold either a VST plugin or a native effect.
//! - Keep legacy projects readable: a legacy `PluginRef` array deserializes into
//!   `EffectRef::Plugin`.
//!
//! Functions:
//! - `NativeEffectKind::label` / `key` / `params`: display name, identity key and parameter list.
//! - `NativeEffectParam::label` / `display`: parameter label and value text.
//! - `NativeEffectSettings::get` / `get_mut`: read/write a normalized parameter by enum.
//! - `NativeEffectSettings::for_kind`: default parameters for one effect kind.
//! - `NativeEffect::new`: create an enabled native effect.
//! - `EffectRef::enabled` / `set_enabled` / `key` / `label` / `badge`: shared chain queries.
//! - `EffectRef::plugin` / `plugin_mut` / `native` / `native_mut`: typed accessors.
//! - `default_enabled`: serde default for `enabled`.

use super::plugin::PluginRef;
use serde::{Deserialize, Serialize};

/// 内置原生效果器类型。
/// Built-in native effect kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeEffectKind {
    Overdrive,
    Distortion,
    Fuzz,
    Chorus,
    Phaser,
    Delay,
    Reverb,
    Compressor,
}

impl NativeEffectKind {
    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：返回内置效果器的中文显示名。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Return the Chinese display name of a native effect.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(&self) -> &'static str {
        match self {
            NativeEffectKind::Overdrive => "过载",
            NativeEffectKind::Distortion => "失真",
            NativeEffectKind::Fuzz => "法兹",
            NativeEffectKind::Chorus => "合唱",
            NativeEffectKind::Phaser => "相位",
            NativeEffectKind::Delay => "延时",
            NativeEffectKind::Reverb => "混响",
            NativeEffectKind::Compressor => "压缩",
        }
    }

    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：返回内置效果器的稳定身份键，用于运行时实例复用。
    /// 输入：`self`。
    /// 输出/副作用：静态字符串；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Return the stable identity key used to reuse a runtime instance.
    /// Inputs: `self`.
    /// Outputs / side effects: static string; no state change.
    /// Complexity: O(1) time and space.
    pub fn key(&self) -> &'static str {
        match self {
            NativeEffectKind::Overdrive => "native:overdrive",
            NativeEffectKind::Distortion => "native:distortion",
            NativeEffectKind::Fuzz => "native:fuzz",
            NativeEffectKind::Chorus => "native:chorus",
            NativeEffectKind::Phaser => "native:phaser",
            NativeEffectKind::Delay => "native:delay",
            NativeEffectKind::Reverb => "native:reverb",
            NativeEffectKind::Compressor => "native:compressor",
        }
    }

    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：返回该效果器实际可调参数，供检查器与效果器卷帘绘制。
    /// 输入：`self`。
    /// 输出/副作用：静态参数表；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Return the parameters exposed by this effect for the inspector and effect roll.
    /// Inputs: `self`.
    /// Outputs / side effects: static parameter table; no state change.
    /// Complexity: O(1) time and space.
    pub fn params(&self) -> &'static [NativeEffectParam] {
        match self {
            NativeEffectKind::Overdrive => &[
                NativeEffectParam::Drive,
                NativeEffectParam::Tone,
                NativeEffectParam::Level,
                NativeEffectParam::Mix,
            ],
            NativeEffectKind::Distortion => &[
                NativeEffectParam::Drive,
                NativeEffectParam::Tone,
                NativeEffectParam::Level,
                NativeEffectParam::Mix,
            ],
            NativeEffectKind::Fuzz => &[
                NativeEffectParam::Drive,
                NativeEffectParam::Tone,
                NativeEffectParam::Level,
                NativeEffectParam::Mix,
            ],
            NativeEffectKind::Chorus => &[
                NativeEffectParam::Rate,
                NativeEffectParam::Depth,
                NativeEffectParam::Mix,
                NativeEffectParam::Level,
            ],
            NativeEffectKind::Phaser => &[
                NativeEffectParam::Rate,
                NativeEffectParam::Depth,
                NativeEffectParam::Feedback,
                NativeEffectParam::Mix,
            ],
            NativeEffectKind::Delay => &[
                NativeEffectParam::Time,
                NativeEffectParam::Feedback,
                NativeEffectParam::Tone,
                NativeEffectParam::Mix,
            ],
            NativeEffectKind::Reverb => &[
                NativeEffectParam::Time,
                NativeEffectParam::Tone,
                NativeEffectParam::Mix,
                NativeEffectParam::Level,
            ],
            NativeEffectKind::Compressor => &[
                NativeEffectParam::Threshold,
                NativeEffectParam::Ratio,
                NativeEffectParam::Attack,
                NativeEffectParam::Release,
                NativeEffectParam::Level,
            ],
        }
    }

    pub const ALL: [NativeEffectKind; 8] = [
        NativeEffectKind::Overdrive,
        NativeEffectKind::Distortion,
        NativeEffectKind::Fuzz,
        NativeEffectKind::Chorus,
        NativeEffectKind::Phaser,
        NativeEffectKind::Delay,
        NativeEffectKind::Reverb,
        NativeEffectKind::Compressor,
    ];
}

/// 内置效果器可调参数（归一化 0..1 存储）。
/// Adjustable native-effect parameters (stored normalized in 0..1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeEffectParam {
    Drive,
    Tone,
    Mix,
    Rate,
    Depth,
    Time,
    Feedback,
    Threshold,
    Ratio,
    Attack,
    Release,
    Level,
}

impl NativeEffectParam {
    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：返回参数中文名。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Return the Chinese parameter label.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(&self) -> &'static str {
        match self {
            NativeEffectParam::Drive => "驱动",
            NativeEffectParam::Tone => "音色",
            NativeEffectParam::Mix => "混合",
            NativeEffectParam::Rate => "速率",
            NativeEffectParam::Depth => "深度",
            NativeEffectParam::Time => "时间",
            NativeEffectParam::Feedback => "反馈",
            NativeEffectParam::Threshold => "阈值",
            NativeEffectParam::Ratio => "比率",
            NativeEffectParam::Attack => "起音",
            NativeEffectParam::Release => "释放",
            NativeEffectParam::Level => "输出",
        }
    }

    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：把归一化值转换为用户可读文本（时间/频率/电平使用物理单位）。
    /// 输入：`value`：0..1 归一化值。
    /// 输出/副作用：显示文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Convert a normalized value into display text (time/rate/level use real units).
    /// Inputs: `value`: normalized 0..1 value.
    /// Outputs / side effects: display text; no state change.
    /// Complexity: O(1) time and space.
    pub fn display(&self, value: f32) -> String {
        let value = value.clamp(0.0, 1.0);
        match self {
            NativeEffectParam::Time => format!("{:.0} ms", 40.0 + value * 860.0),
            NativeEffectParam::Rate => format!("{:.2} Hz", 0.05 + value * 7.95),
            NativeEffectParam::Threshold => format!("{:.1} dB", -48.0 + value * 48.0),
            NativeEffectParam::Ratio => format!("{:.1}:1", 1.0 + value * 19.0),
            NativeEffectParam::Attack => format!("{:.0} ms", 1.0 + value * 99.0),
            NativeEffectParam::Release => format!("{:.0} ms", 20.0 + value * 1980.0),
            _ => format!("{:.0}%", value * 100.0),
        }
    }
}

/// 内置效果器的归一化参数集合。
/// Normalized parameter set shared by all native effects.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NativeEffectSettings {
    pub drive: f32,
    pub tone: f32,
    pub mix: f32,
    pub rate: f32,
    pub depth: f32,
    pub time: f32,
    pub feedback: f32,
    pub threshold: f32,
    pub ratio: f32,
    pub attack: f32,
    pub release: f32,
    pub level: f32,
}

impl Default for NativeEffectSettings {
    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：提供中性的默认参数（各效果器创建时会按类型覆盖）。
    /// 输入：无。
    /// 输出/副作用：返回默认参数；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Neutral defaults (each effect overrides them when constructed).
    /// Inputs: none.
    /// Outputs / side effects: default settings; no state change.
    /// Complexity: O(1) time and space.
    fn default() -> Self {
        Self {
            drive: 0.5,
            tone: 0.5,
            mix: 1.0,
            rate: 0.25,
            depth: 0.5,
            time: 0.4,
            feedback: 0.35,
            threshold: 0.55,
            ratio: 0.4,
            attack: 0.2,
            release: 0.4,
            level: 0.7,
        }
    }
}

impl NativeEffectSettings {
    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：按类型生成更合适的默认参数。
    /// 输入：`kind` 效果器类型。
    /// 输出/副作用：返回参数集合；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Build kind-specific defaults.
    /// Inputs: `kind` effect kind.
    /// Outputs / side effects: settings; no state change.
    /// Complexity: O(1) time and space.
    pub fn for_kind(kind: NativeEffectKind) -> Self {
        let mut settings = Self::default();
        match kind {
            NativeEffectKind::Overdrive => {
                settings.drive = 0.45;
                settings.tone = 0.55;
                settings.level = 0.7;
            }
            NativeEffectKind::Distortion => {
                settings.drive = 0.62;
                settings.tone = 0.45;
                settings.level = 0.6;
            }
            NativeEffectKind::Fuzz => {
                settings.drive = 0.72;
                settings.tone = 0.5;
                settings.level = 0.55;
            }
            NativeEffectKind::Chorus => {
                settings.rate = 0.25;
                settings.depth = 0.45;
                settings.mix = 0.45;
                settings.level = 0.8;
            }
            NativeEffectKind::Phaser => {
                settings.rate = 0.2;
                settings.depth = 0.6;
                settings.feedback = 0.4;
                settings.mix = 0.5;
                settings.level = 0.8;
            }
            NativeEffectKind::Delay => {
                settings.time = 0.4;
                settings.feedback = 0.35;
                settings.tone = 0.5;
                settings.mix = 0.3;
                settings.level = 0.8;
            }
            NativeEffectKind::Reverb => {
                settings.time = 0.5;
                settings.tone = 0.5;
                settings.mix = 0.3;
                settings.level = 0.9;
            }
            NativeEffectKind::Compressor => {
                settings.threshold = 0.55;
                settings.ratio = 0.4;
                settings.attack = 0.2;
                settings.release = 0.4;
                settings.level = 0.55;
            }
        }
        settings
    }

    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：读取一个归一化参数。
    /// 输入：`param` 参数枚举。
    /// 输出/副作用：参数值 0..1；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Read one normalized parameter.
    /// Inputs: `param` parameter enum.
    /// Outputs / side effects: value in 0..1; no state change.
    /// Complexity: O(1) time and space.
    pub fn get(&self, param: NativeEffectParam) -> f32 {
        match param {
            NativeEffectParam::Drive => self.drive,
            NativeEffectParam::Tone => self.tone,
            NativeEffectParam::Mix => self.mix,
            NativeEffectParam::Rate => self.rate,
            NativeEffectParam::Depth => self.depth,
            NativeEffectParam::Time => self.time,
            NativeEffectParam::Feedback => self.feedback,
            NativeEffectParam::Threshold => self.threshold,
            NativeEffectParam::Ratio => self.ratio,
            NativeEffectParam::Attack => self.attack,
            NativeEffectParam::Release => self.release,
            NativeEffectParam::Level => self.level,
        }
    }

    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：写入一个归一化参数（调用方负责 clamp）。
    /// 输入：`param` 参数枚举；`value` 新值。
    /// 输出/副作用：修改参数集合；不修改其他状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Write one normalized parameter (the caller clamps it).
    /// Inputs: `param` parameter enum; `value` new value.
    /// Outputs / side effects: mutates the settings; nothing else changes.
    /// Complexity: O(1) time and space.
    pub fn get_mut(&mut self, param: NativeEffectParam) -> &mut f32 {
        match param {
            NativeEffectParam::Drive => &mut self.drive,
            NativeEffectParam::Tone => &mut self.tone,
            NativeEffectParam::Mix => &mut self.mix,
            NativeEffectParam::Rate => &mut self.rate,
            NativeEffectParam::Depth => &mut self.depth,
            NativeEffectParam::Time => &mut self.time,
            NativeEffectParam::Feedback => &mut self.feedback,
            NativeEffectParam::Threshold => &mut self.threshold,
            NativeEffectParam::Ratio => &mut self.ratio,
            NativeEffectParam::Attack => &mut self.attack,
            NativeEffectParam::Release => &mut self.release,
            NativeEffectParam::Level => &mut self.level,
        }
    }
}

/// 一个内置原生效果器引用。
/// A built-in native effect reference.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NativeEffect {
    pub kind: NativeEffectKind,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub settings: NativeEffectSettings,
}

impl NativeEffect {
    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：按类型创建默认启用的内置效果器。
    /// 输入：`kind` 效果器类型。
    /// 输出/副作用：返回新的效果器引用；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Create an enabled native effect of the given kind.
    /// Inputs: `kind` effect kind.
    /// Outputs / side effects: the new reference; no state change.
    /// Complexity: O(1) time and space.
    pub fn new(kind: NativeEffectKind) -> Self {
        Self {
            kind,
            enabled: true,
            settings: NativeEffectSettings::for_kind(kind),
        }
    }
}

/// 效果链中的一个位置：VST 插件效果或内置原生效果。
/// One effect-chain slot: either a VST plugin effect or a built-in native effect.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EffectRef {
    Native(NativeEffect),
    Plugin(PluginRef),
}

impl EffectRef {
    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：返回该效果器当前是否参与处理。
    /// 输入：`self`。
    /// 输出/副作用：启用状态；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Whether this effect currently processes audio.
    /// Inputs: `self`.
    /// Outputs / side effects: enable flag; no state change.
    /// Complexity: O(1) time and space.
    pub fn enabled(&self) -> bool {
        match self {
            EffectRef::Native(effect) => effect.enabled,
            EffectRef::Plugin(plugin) => plugin.enabled,
        }
    }

    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：设置启用/旁路状态。
    /// 输入：`enabled` 目标状态。
    /// 输出/副作用：修改引用；不修改其他状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Set the enabled/bypassed state.
    /// Inputs: `enabled` target state.
    /// Outputs / side effects: mutates the reference; nothing else changes.
    /// Complexity: O(1) time and space.
    pub fn set_enabled(&mut self, enabled: bool) {
        match self {
            EffectRef::Native(effect) => effect.enabled = enabled,
            EffectRef::Plugin(plugin) => plugin.enabled = enabled,
        }
    }

    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：返回运行时复用所需的身份键。
    /// 输入：`self`。
    /// 输出/副作用：身份字符串；不修改状态。
    /// 复杂度：时间 O(路径长度)，空间 O(身份字符串长度)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Return the identity key used for runtime reuse.
    /// Inputs: `self`.
    /// Outputs / side effects: identity string; no state change.
    /// Complexity: O(path length) time and O(key length) space.
    pub fn key(&self) -> String {
        match self {
            EffectRef::Native(effect) => effect.kind.key().to_string(),
            EffectRef::Plugin(plugin) => plugin.key(),
        }
    }

    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：返回效果器显示名。
    /// 输入：`self`。
    /// 输出/副作用：显示文本；不修改状态。
    /// 复杂度：时间 O(名称长度)，空间 O(名称长度)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Return the display name of the effect.
    /// Inputs: `self`.
    /// Outputs / side effects: display text; no state change.
    /// Complexity: O(name length) time and space.
    pub fn label(&self) -> String {
        match self {
            EffectRef::Native(effect) => effect.kind.label().to_string(),
            EffectRef::Plugin(plugin) => {
                if plugin.name.is_empty() {
                    plugin
                        .path
                        .file_stem()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| "插件".into())
                } else {
                    plugin.name.clone()
                }
            }
        }
    }

    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：返回卡片角标文本（VST3 / VST2 / 内置）。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Return the card badge text (VST3 / VST2 / native).
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn badge(&self) -> &'static str {
        match self {
            EffectRef::Native(_) => "内置",
            EffectRef::Plugin(plugin) => plugin.format.label(),
        }
    }

    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：以插件引用形式访问（内置效果返回 `None`）。
    /// 输入：`self`。
    /// 输出/副作用：不可变插件引用；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Access as a plugin reference (`None` for native effects).
    /// Inputs: `self`.
    /// Outputs / side effects: immutable plugin reference; no state change.
    /// Complexity: O(1) time and space.
    pub fn plugin(&self) -> Option<&PluginRef> {
        match self {
            EffectRef::Plugin(plugin) => Some(plugin),
            EffectRef::Native(_) => None,
        }
    }

    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：以可变插件引用形式访问（内置效果返回 `None`）。
    /// 输入：`self`。
    /// 输出/副作用：可变插件引用；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Access mutably as a plugin reference (`None` for native effects).
    /// Inputs: `self`.
    /// Outputs / side effects: mutable plugin reference; no state change.
    /// Complexity: O(1) time and space.
    pub fn plugin_mut(&mut self) -> Option<&mut PluginRef> {
        match self {
            EffectRef::Plugin(plugin) => Some(plugin),
            EffectRef::Native(_) => None,
        }
    }

    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：以不可变内置效果形式访问。
    /// 输入：`self`。
    /// 输出/副作用：内置效果引用；插件返回 `None`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Access as a native effect.
    /// Inputs: `self`.
    /// Outputs / side effects: native effect reference; `None` for plugins.
    /// Complexity: O(1) time and space.
    pub fn native(&self) -> Option<&NativeEffect> {
        match self {
            EffectRef::Native(effect) => Some(effect),
            EffectRef::Plugin(_) => None,
        }
    }

    /// 所属模块：core / 数据模型 / 效果器引用
    /// 功能：以可变内置效果形式访问。
    /// 输入：`self`。
    /// 输出/副作用：可变内置效果引用；插件返回 `None`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / data model / effect reference
    /// Purpose: Access mutably as a native effect.
    /// Inputs: `self`.
    /// Outputs / side effects: mutable native effect reference; `None` for plugins.
    /// Complexity: O(1) time and space.
    pub fn native_mut(&mut self) -> Option<&mut NativeEffect> {
        match self {
            EffectRef::Native(effect) => Some(effect),
            EffectRef::Plugin(_) => None,
        }
    }
}

/// 所属模块：core / 数据模型 / 效果器引用
/// 功能：`enabled` 的 serde 默认值：旧工程加载后效果器默认生效。
/// 输入：无。
/// 输出/副作用：返回 `true`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / data model / effect reference
/// Purpose: serde default for `enabled`: effects from older projects stay active.
/// Inputs: none.
/// Outputs / side effects: `true`; no state change.
/// Complexity: O(1) time and space.
fn default_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：旧工程的插件数组仍可反序列化为 `EffectRef::Plugin`。
    /// Covers: legacy plugin arrays still deserialize into `EffectRef::Plugin`.
    #[test]
    fn legacy_plugin_array_deserializes_as_plugin_effect() {
        let json = r#"[{"format":"Vst3","path":"C:/plugins/test.vst3","name":"Test"}]"#;
        let effects: Vec<EffectRef> = serde_json::from_str(json).expect("deserialize effects");
        assert_eq!(effects.len(), 1);
        assert!(effects[0].plugin().is_some());
        assert!(effects[0].enabled());
    }

    /// 覆盖：内置效果器可往返序列化并保持类型与参数。
    /// Covers: a native effect round-trips with its kind and settings.
    #[test]
    fn native_effect_round_trips() {
        let effect = EffectRef::Native(NativeEffect::new(NativeEffectKind::Delay));
        let json = serde_json::to_string(&effect).expect("serialize");
        let loaded: EffectRef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(loaded.key(), "native:delay");
        assert!(loaded.native().is_some());
    }
}
