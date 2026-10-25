//! Tracks and their child lanes.

use super::clip::Clip;
use super::controller::{ControllerKind, ControllerLanes};
use super::default_revision;
use super::effect::EffectRef;
use super::instrument::InstrumentRef;
use super::TrackId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    #[default]
    Midi,
    Audio,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    pub color: [u8; 3],
    /// Default role of the main lane. Tracks are not restricted: a MIDI track
    /// can still receive audio clips and vice versa.
    #[serde(default)]
    pub kind: TrackKind,
    pub instrument: InstrumentRef,
    /// Insert effect chain, processed from left to right.
    #[serde(default)]
    pub effects: Vec<EffectRef>,
    #[serde(default)]
    pub controllers: ControllerLanes,
    /// Controller sub-lanes shown under the track (empty = only the FX chain).
    #[serde(default)]
    pub visible_controllers: Vec<ControllerKind>,
    pub clips: Vec<Clip>,
    pub volume: f32,
    pub pan: f32,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub solo: bool,
    /// Whether the controller/effect child lanes are expanded in the arrangement.
    #[serde(default)]
    pub expanded: bool,
    /// Incremented whenever the instrument, effects or controller content changes.
    #[serde(skip, default = "default_revision")]
    pub revision: u64,
}

impl Track {
    pub fn new(id: TrackId, name: &str, color: [u8; 3], instrument: InstrumentRef) -> Self {
        Self {
            id,
            name: name.to_string(),
            color,
            kind: TrackKind::Midi,
            instrument,
            effects: Vec::new(),
            controllers: ControllerLanes::default(),
            visible_controllers: Vec::new(),
            clips: Vec::new(),
            volume: 0.8,
            pan: 0.0,
            muted: false,
            solo: false,
            expanded: false,
            revision: 1,
        }
    }

    pub fn instrument_name(&self) -> String {
        self.instrument.label()
    }

    /// Whether the lane is driven by audio samples only. The stored `kind` is
    /// only the track's default role, so the actual clips decide: a MIDI track
    /// that only holds an audio clip behaves like an audio lane.
    pub fn is_audio_lane(&self) -> bool {
        if !self.clips.is_empty() {
            return self.clips.iter().all(Clip::is_audio);
        }
        self.kind == TrackKind::Audio
    }

    pub fn has_midi_clips(&self) -> bool {
        self.clips.iter().any(|clip| !clip.is_audio())
    }

    pub fn has_audio_clips(&self) -> bool {
        self.clips.iter().any(Clip::is_audio)
    }

    /// Controllers that can actually affect this main lane. Velocity and
    /// sustain only shape MIDI notes, so an audio-only lane cannot use them.
    pub fn supports_controller(&self, kind: ControllerKind) -> bool {
        if matches!(kind, ControllerKind::Velocity | ControllerKind::Sustain) {
            if self.clips.is_empty() {
                // Empty lanes derive their role from the track kind.
                return self.kind == TrackKind::Midi;
            }
            return self.has_midi_clips();
        }
        true
    }

    /// A lane may not mix MIDI and audio clips: controller effectiveness would
    /// become ambiguous.
    pub fn accepts_clip(&self, clip: &Clip) -> bool {
        if clip.is_audio() {
            !self.has_midi_clips()
        } else {
            !self.has_audio_clips()
        }
    }

    pub fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
    }
}
