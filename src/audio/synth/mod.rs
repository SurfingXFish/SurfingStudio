//! 所属模块：audio / 内置乐器
//! 文件用途：
//! - 聚合内置复音合成器、鼓机、原生建模乐器、采样音色库播放器与神经音色模型，
//!   并提供本块音符事件类型。
//!
//! Module: audio / built-in instruments
//! Purpose:
//! - Aggregate the built-in polyphonic synth, drum kit, modeled native instruments, sampled
//!   bank player and neural timbre model, and expose the per-block note event type.

mod drums;
mod modeled;
mod neural;
mod poly;
mod sampled;

pub use drums::DrumKit;
pub use modeled::ModeledInstrument;
pub use neural::HIDDEN_WIDTHS;
pub use poly::PolySynth;
pub use sampled::SampledInstrument;

/// A note gate scheduled at a frame offset inside the current block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateKind {
    On,
    Off,
}

#[derive(Clone, Copy, Debug)]
pub struct SynthEvent {
    pub frame: usize,
    pub kind: GateKind,
    pub pitch: u8,
    pub velocity: u8,
}
