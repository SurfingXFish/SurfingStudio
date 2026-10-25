//! Project container, defaults and the built-in demo arrangement.

use super::clip::{Clip, Note};
use super::controller::{evaluate_automation, ControllerKind, ControllerLanes};
use super::effect::EffectRef;
use super::instrument::{DrumSettings, InstrumentRef, SynthSettings, Waveform};
use super::track::{Track, TrackKind};
use super::{TrackId, CURRENT_PROJECT_VERSION};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    #[serde(default = "default_project_name")]
    pub name: String,
    pub bpm: f64,
    #[serde(default = "default_time_sig_num")]
    pub time_sig_num: u8,
    #[serde(default = "default_time_sig_den")]
    pub time_sig_den: u8,
    #[serde(default)]
    pub tracks: Vec<Track>,
    #[serde(default = "default_master_volume")]
    pub master_volume: f32,
    /// Automation for the master bus. The main master lane is a volume
    /// controller, so its points are edited in the controller roll.
    #[serde(default)]
    pub master_controllers: ControllerLanes,
    #[serde(default)]
    pub master_effect: Option<EffectRef>,
    #[serde(default)]
    pub loop_enabled: bool,
    #[serde(default)]
    pub loop_start: f64,
    #[serde(default = "default_loop_end")]
    pub loop_end: f64,
    /// Minimum arrangement length in beats (can grow automatically).
    #[serde(default = "default_length")]
    pub length_beats: f64,
}

fn default_project_name() -> String {
    "未命名工程".into()
}
fn default_time_sig_num() -> u8 {
    4
}
fn default_time_sig_den() -> u8 {
    4
}
fn default_master_volume() -> f32 {
    0.85
}
fn default_loop_end() -> f64 {
    16.0
}
fn default_length() -> f64 {
    32.0
}

impl Default for Project {
    fn default() -> Self {
        Self {
            version: CURRENT_PROJECT_VERSION,
            name: "未命名工程".into(),
            bpm: 120.0,
            time_sig_num: 4,
            time_sig_den: 4,
            tracks: Vec::new(),
            master_volume: 0.85,
            master_controllers: ControllerLanes::default(),
            master_effect: None,
            loop_enabled: false,
            loop_start: 0.0,
            loop_end: 16.0,
            length_beats: 32.0,
        }
    }
}

impl Project {
    pub fn beats_per_bar(&self) -> f64 {
        self.time_sig_num as f64 * 4.0 / self.time_sig_den as f64
    }

    pub fn next_track_id(&self) -> TrackId {
        self.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1
    }

    pub fn next_clip_id(&self) -> u64 {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(|c| c.id))
            .max()
            .unwrap_or(0)
            + 1
    }

    pub fn add_track(&mut self, instrument: InstrumentRef) -> usize {
        let id = self.next_track_id();
        let palette: [[u8; 3]; 6] = [
            [45, 212, 191],
            [251, 191, 36],
            [129, 140, 248],
            [244, 114, 182],
            [52, 211, 153],
            [251, 146, 60],
        ];
        let color = palette[(self.tracks.len()) % palette.len()];
        let name = match &instrument {
            InstrumentRef::Drums(_) => format!("鼓组 {}", id),
            InstrumentRef::Native(native) => format!("{} {}", native.kind.label(), id),
            InstrumentRef::Plugin(_) => format!("插件轨道 {}", id),
            InstrumentRef::Synth(_) => format!("MIDI 轨道 {}", id),
        };
        self.tracks.push(Track::new(id, &name, color, instrument));
        self.tracks.len() - 1
    }

    /// Create a main lane intended for audio clips. It still hosts the same
    /// instrument/controller model, so MIDI clips can be added later.
    pub fn add_audio_track(&mut self) -> usize {
        let index = self.add_track(InstrumentRef::Synth(SynthSettings::default()));
        let id = self.tracks[index].id;
        let track = &mut self.tracks[index];
        track.kind = TrackKind::Audio;
        track.name = format!("音频轨道 {id}");
        index
    }

    /// Master gain at a given beat: volume automation if present, otherwise
    /// the static master volume.
    pub fn master_volume_at(&self, beat: f64) -> f32 {
        evaluate_automation(self.master_controllers.get(ControllerKind::Volume), beat)
            .map(|value| value * 1.5)
            .unwrap_or(self.master_volume)
            .clamp(0.0, 1.5)
    }

    pub fn end_beat(&self) -> f64 {
        let content = self
            .tracks
            .iter()
            .flat_map(|t| t.clips.iter())
            .map(|c| c.end())
            .fold(0.0_f64, f64::max);
        content.max(self.length_beats).max(4.0)
    }

    pub fn content_end_beat(&self) -> f64 {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter())
            .map(|c| c.end())
            .fold(0.0_f64, f64::max)
    }

    /// Playback stops automatically two bars after the final MIDI clip.
    pub fn auto_stop_beat(&self) -> f64 {
        let end = self.content_end_beat();
        if end <= 0.0 {
            0.0
        } else {
            end + self.beats_per_bar() * 2.0
        }
    }

    pub fn any_solo(&self) -> bool {
        self.tracks.iter().any(|t| t.solo)
    }

    /// A short demo arrangement so the first launch makes sound immediately.
    pub fn demo() -> Self {
        let mut project = Project {
            name: "Surfing Demo".into(),
            bpm: 110.0,
            loop_enabled: false,
            loop_start: 0.0,
            loop_end: 16.0,
            length_beats: 32.0,
            ..Default::default()
        };

        // Lead
        let mut lead = Track::new(
            1,
            "Surf Lead",
            [45, 212, 191],
            InstrumentRef::Synth(SynthSettings {
                waveform: Waveform::Saw,
                cutoff: 6200.0,
                resonance: 0.22,
                level: 0.55,
                ..Default::default()
            }),
        );
        let melody: [(f64, f64, u8); 12] = [
            (0.0, 0.5, 69),
            (0.5, 0.5, 72),
            (1.0, 1.0, 76),
            (2.0, 0.5, 74),
            (2.5, 0.5, 72),
            (3.0, 1.0, 69),
            (4.0, 0.5, 67),
            (4.5, 0.5, 69),
            (5.0, 1.0, 72),
            (6.0, 0.5, 74),
            (6.5, 0.5, 76),
            (7.0, 1.0, 79),
        ];
        let mut notes = Vec::new();
        for bar in 0..4 {
            for (start, len, pitch) in melody {
                notes.push(Note {
                    start: start + bar as f64 * 8.0,
                    length: len,
                    pitch,
                    velocity: 98,
                });
            }
        }
        let mut lead_clip = Clip::new(1, "Lead Pattern", 0.0, 32.0);
        lead_clip.notes = notes;
        lead.clips.push(lead_clip);
        lead.volume = 0.7;
        lead.expanded = true;
        project.tracks.push(lead);

        // Chords
        let mut chords = Track::new(
            2,
            "Warm Chords",
            [251, 191, 36],
            InstrumentRef::Synth(SynthSettings {
                waveform: Waveform::Triangle,
                attack: 0.03,
                release: 0.45,
                cutoff: 3400.0,
                level: 0.36,
                ..Default::default()
            }),
        );
        let progression: [[u8; 3]; 4] = [[57, 60, 64], [53, 57, 60], [48, 52, 55], [55, 59, 62]];
        let mut chord_notes = Vec::new();
        for bar in 0..4 {
            let chord = progression[bar];
            for pitch in chord {
                chord_notes.push(Note {
                    start: bar as f64 * 8.0,
                    length: 3.75,
                    pitch,
                    velocity: 72,
                });
            }
        }
        let mut chord_clip = Clip::new(2, "Am - F - C - G", 0.0, 32.0);
        chord_clip.notes = chord_notes;
        chords.clips.push(chord_clip);
        chords.volume = 0.62;
        project.tracks.push(chords);

        // Bass
        let mut bass = Track::new(
            3,
            "Sub Bass",
            [129, 140, 248],
            InstrumentRef::Synth(SynthSettings {
                waveform: Waveform::Square,
                attack: 0.004,
                decay: 0.12,
                sustain: 0.5,
                release: 0.12,
                cutoff: 900.0,
                level: 0.5,
                octave: -1,
                ..Default::default()
            }),
        );
        let roots = [45u8, 41, 36, 43];
        let mut bass_notes = Vec::new();
        for bar in 0..4 {
            for step in 0..8 {
                bass_notes.push(Note {
                    start: bar as f64 * 8.0 + step as f64 * 0.5,
                    length: 0.42,
                    pitch: roots[bar],
                    velocity: if step % 2 == 0 { 108 } else { 82 },
                });
            }
        }
        let mut bass_clip = Clip::new(3, "Bass Pulse", 0.0, 32.0);
        bass_clip.notes = bass_notes;
        bass.clips.push(bass_clip);
        bass.volume = 0.68;
        project.tracks.push(bass);

        // Drums
        let mut drums = Track::new(
            4,
            "Chip Drums",
            [244, 114, 182],
            InstrumentRef::Drums(DrumSettings::default()),
        );
        let mut drum_notes = Vec::new();
        for bar in 0..4 {
            let base = bar as f64 * 8.0;
            for beat in 0..8 {
                // Kick on every beat, extra on the "and" of 3.
                drum_notes.push(Note {
                    start: base + beat as f64,
                    length: 0.12,
                    pitch: 36,
                    velocity: if beat % 2 == 0 { 120 } else { 95 },
                });
                // Hats on eighths.
                drum_notes.push(Note {
                    start: base + beat as f64 + 0.5,
                    length: 0.08,
                    pitch: 42,
                    velocity: if beat % 2 == 0 { 75 } else { 58 },
                });
            }
            // Snare on beats 2 and 4.
            for beat in [1.0, 3.0, 5.0, 7.0] {
                drum_notes.push(Note {
                    start: base + beat,
                    length: 0.12,
                    pitch: 38,
                    velocity: 105,
                });
            }
        }
        let mut drum_clip = Clip::new(4, "Four On The Floor", 0.0, 32.0);
        drum_clip.notes = drum_notes;
        drums.clips.push(drum_clip);
        drums.volume = 0.8;
        project.tracks.push(drums);

        project
    }

    pub fn empty() -> Self {
        let mut project = Project {
            name: "新工程".into(),
            bpm: 120.0,
            ..Default::default()
        };
        project.add_track(InstrumentRef::Synth(SynthSettings::default()));
        project
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_round_trips_through_json() {
        let project = Project::demo();
        let json = serde_json::to_string(&project).expect("serialize");
        let loaded: Project = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(loaded.tracks.len(), project.tracks.len());
        assert_eq!(loaded.bpm, project.bpm);
        assert_eq!(
            loaded.tracks[0].clips[0].notes.len(),
            project.tracks[0].clips[0].notes.len()
        );
    }
}
