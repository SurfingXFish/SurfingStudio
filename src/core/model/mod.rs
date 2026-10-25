//! 所属模块：core / 数据模型
//! 文件用途：
//! - 声明纯数据模型子模块并导出工程、轨道、片段、乐器与模板类型。
//! - 定义工程清单版本、主输出伪轨道 id 与 revision 默认值。
//!
//! 导出项：
//! - `clip`：MIDI / 音频片段与音符。
//! - `controller`：自动化控制器与插值规则。
//! - `effect`：内置原生效果器与效果链引用。
//! - `instrument`：内置合成器 / 鼓机 / 原生乐器设置与乐器引用。
//! - `plugin`：插件引用与格式。
//! - `track`：轨道、子轨与类型约束。
//! - `project`：工程容器、默认值与 Demo 编曲。
//! - `template`：主流音乐风格模板、范例工程与展开器。
//!
//! Module: core / data model
//! Purpose:
//! - Declare the pure data-model submodules and export project, track, clip, instrument and
//!   template types.
//! - Define the project manifest version, master pseudo-track id and revision default.
//!
//! Exports:
//! - `clip`: MIDI/audio clips and notes.
//! - `controller`: automation controllers and interpolation rules.
//! - `effect`: native effects and effect-chain references.
//! - `instrument`: synth/drum/native instrument settings and references.
//! - `plugin`: plugin references and formats.
//! - `track`: tracks, child lanes and type constraints.
//! - `project`: project container, defaults and demo arrangement.
//! - `template`: mainstream music templates, example projects and the expander.

mod clip;
mod controller;
mod effect;
mod instrument;
mod plugin;
mod project;
pub mod template;
mod track;

pub use clip::{AudioSource, Clip, Note};
pub use controller::{evaluate_automation, AutomationPoint, ControllerKind, ControllerLanes};
pub use effect::{
    EffectRef, NativeEffect, NativeEffectKind, NativeEffectParam, NativeEffectSettings,
};
pub use instrument::{
    DrumSettings, InstrumentRef, NativeInstrument, NativeInstrumentKind, NativeInstrumentSettings,
    SynthSettings, Waveform,
};
pub use plugin::{PluginFormat, PluginRef};
pub use project::Project;
pub use template::{
    ChordQuality, ChordSpec, ExampleProject, MusicTemplate, TemplateFamily, TemplateId,
};
pub use track::{Track, TrackKind};

pub const CURRENT_PROJECT_VERSION: u32 = 2;

pub type TrackId = u64;

/// Sentinel id for the pinned master bus. Real tracks start at 1, so 0 can
/// never collide with a track id.
pub const MASTER_TRACK_ID: TrackId = 0;

/// `serde` default for the non-persisted revision counters.
pub(super) fn default_revision() -> u64 {
    1
}
