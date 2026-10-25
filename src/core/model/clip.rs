//! 所属模块：core::model / 片段
//! 文件用途：
//! - 定义 MIDI 音符、音频来源与片段容器。
//! - 提供片段结束位置与听感版本递增等纯规则。
//!
//! 函数清单：
//! - `Note::end`：音符结束 beat。
//! - `Clip::new` / `Clip::is_audio` / `Clip::end` / `Clip::bump_revision`：片段构造、类型判断、结束位置与版本递增。
//!
//! Module: core::model / clips
//! Purpose:
//! - Define MIDI notes, audio sources and the clip container.
//! - Provide pure rules for clip end positions and audible revision bumps.
//!
//! Functions:
//! - `Note::end`: note end in beats.
//! - `Clip::new` / `Clip::is_audio` / `Clip::end` / `Clip::bump_revision`: clip construction, type check, end position and revision bump.

use super::default_revision;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// Start in beats, relative to the clip.
    pub start: f64,
    /// Length in beats.
    pub length: f64,
    pub pitch: u8,
    pub velocity: u8,
}

impl Note {
    pub fn end(&self) -> f64 {
        self.start + self.length
    }
}

/// Reference to a decoded audio file used by an audio clip. The sample data is
/// loaded into memory by the engine; only the path is persisted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioSource {
    pub path: PathBuf,
    #[serde(default)]
    pub name: String,
    /// Source duration in seconds, used by the arrangement before the file is
    /// decoded.
    #[serde(default)]
    pub length_seconds: f64,
    /// 源文件内部的起始位置，单位秒；默认 0。导入工程（如 SoundBug）可用它引用同一 WAV 的后续区间。
    /// Offset into the source file in seconds; defaults to 0. Imported projects
    /// (for example SoundBug) use it to reference a later region of the same WAV.
    #[serde(default)]
    pub offset_seconds: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clip {
    pub id: u64,
    pub name: String,
    /// Start in project beats.
    pub start: f64,
    /// Length in project beats.
    pub length: f64,
    pub notes: Vec<Note>,
    /// When set, this clip plays a WAV file instead of MIDI notes.
    #[serde(default)]
    pub audio: Option<AudioSource>,
    /// Optional per-clip color override (256-color palette index).
    #[serde(default)]
    pub color: Option<u8>,
    /// Incremented whenever the audible content changes, used for render caching.
    #[serde(skip, default = "default_revision")]
    pub revision: u64,
}

impl Clip {
    pub fn new(id: u64, name: impl Into<String>, start: f64, length: f64) -> Self {
        Self {
            id,
            name: name.into(),
            start,
            length,
            notes: Vec::new(),
            audio: None,
            color: None,
            revision: 1,
        }
    }

    pub fn is_audio(&self) -> bool {
        self.audio.is_some()
    }

    pub fn end(&self) -> f64 {
        self.start + self.length
    }

    pub fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
    }
}
