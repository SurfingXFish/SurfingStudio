//! 所属模块：audio / 内置乐器
//! 文件用途：
//! - 聚合内置复音合成器、鼓机与原生建模乐器，并提供本块音符事件类型。
//!
//! Module: audio / built-in instruments
//! Purpose:
//! - Aggregate the built-in polyphonic synth, drum kit and modeled native instruments,
//!   and expose the per-block note event type.

mod drums;
mod modeled;
mod poly;

pub use drums::DrumKit;
pub use modeled::ModeledInstrument;
pub use poly::PolySynth;

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
