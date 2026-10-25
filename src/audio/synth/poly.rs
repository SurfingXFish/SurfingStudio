//! Polyphonic subtractive synthesizer with PolyBLEP oscillators.

use super::{GateKind, SynthEvent};
use crate::core::model::{SynthSettings, Waveform};
use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EnvStage {
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    note: u8,
    velocity: f32,
    phase_a: f32,
    phase_b: f32,
    inc_a: f32,
    inc_b: f32,
    env: f32,
    stage: EnvStage,
    ic1: f32,
    ic2: f32,
    age: u64,
    pending_release: bool,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            active: false,
            note: 60,
            velocity: 1.0,
            phase_a: 0.0,
            phase_b: 0.0,
            inc_a: 0.0,
            inc_b: 0.0,
            env: 0.0,
            stage: EnvStage::Off,
            ic1: 0.0,
            ic2: 0.0,
            age: 0,
            pending_release: false,
        }
    }
}

pub struct PolySynth {
    sample_rate: f32,
    settings: SynthSettings,
    voices: Vec<Voice>,
    age_counter: u64,
    sustain_pedal: bool,
    pitch_bend_semitones: f32,
}

impl PolySynth {
    pub fn new(sample_rate: f32, settings: SynthSettings) -> Self {
        Self {
            sample_rate,
            settings,
            voices: (0..24).map(|_| Voice::default()).collect(),
            age_counter: 0,
            sustain_pedal: false,
            pitch_bend_semitones: 0.0,
        }
    }

    pub fn set_sustain(&mut self, on: bool) {
        if self.sustain_pedal && !on {
            for voice in &mut self.voices {
                if voice.active && voice.pending_release {
                    voice.pending_release = false;
                    voice.stage = EnvStage::Release;
                }
            }
        }
        self.sustain_pedal = on;
    }

    pub fn set_pitch_bend(&mut self, semitones: f32) {
        self.pitch_bend_semitones = semitones.clamp(-24.0, 24.0);
    }

    pub fn set_settings(&mut self, settings: SynthSettings) {
        self.settings = settings;
    }

    pub fn all_notes_off(&mut self) {
        for voice in &mut self.voices {
            if voice.active {
                voice.stage = EnvStage::Release;
            }
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        // Re-use a voice already playing this note, or steal the oldest.
        let slot = self
            .voices
            .iter()
            .position(|v| v.active && v.note == note)
            .or_else(|| self.voices.iter().position(|v| !v.active))
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| v.age)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });

        self.age_counter = self.age_counter.wrapping_add(1);
        let voice = &mut self.voices[slot];
        let semitones = (note as f32 - 69.0) + self.settings.octave as f32 * 12.0;
        let freq = 440.0 * 2.0_f32.powf(semitones / 12.0);
        let detune_semitones = self.settings.detune.clamp(0.0, 1.0) * 0.18;
        voice.active = true;
        voice.note = note;
        voice.velocity = (velocity.max(1) as f32 / 127.0).powf(0.7);
        voice.phase_a = 0.0;
        voice.phase_b = 0.37;
        voice.inc_a = freq / self.sample_rate;
        voice.inc_b = freq * 2.0_f32.powf(detune_semitones / 12.0) / self.sample_rate;
        voice.env = 0.0;
        voice.stage = EnvStage::Attack;
        voice.ic1 = 0.0;
        voice.ic2 = 0.0;
        voice.age = self.age_counter;
        voice.pending_release = false;
    }

    fn note_off(&mut self, note: u8) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note {
                if self.sustain_pedal {
                    voice.pending_release = true;
                } else {
                    voice.stage = EnvStage::Release;
                }
            }
        }
    }

    /// Render mono audio for one block, applying events at their frame offsets.
    pub fn render(&mut self, out: &mut [f32], events: &[SynthEvent]) {
        out.fill(0.0);
        let mut event_index = 0;
        let sr = self.sample_rate.max(1.0);
        let settings = self.settings.clone();
        let attack_step = 1.0 / (settings.attack.max(0.001) * sr);
        let decay_step =
            (1.0 - settings.sustain.clamp(0.0, 1.0)) / (settings.decay.max(0.005) * sr);
        let release_step = 1.0 / (settings.release.max(0.005) * sr);
        let sustain = settings.sustain.clamp(0.0, 1.0);

        let cutoff = settings.cutoff.clamp(30.0, sr * 0.45);
        let g = (PI * cutoff / sr).tan();
        let k = 2.0 - 2.0 * settings.resonance.clamp(0.0, 0.95);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let bend_ratio = 2.0_f32.powf(self.pitch_bend_semitones / 12.0);

        for (frame, sample_out) in out.iter_mut().enumerate() {
            while event_index < events.len() && events[event_index].frame <= frame {
                let event = events[event_index];
                match event.kind {
                    GateKind::On => self.note_on(event.pitch, event.velocity),
                    GateKind::Off => self.note_off(event.pitch),
                }
                event_index += 1;
            }

            let mut mix = 0.0;
            for voice in &mut self.voices {
                if !voice.active {
                    continue;
                }
                match voice.stage {
                    EnvStage::Attack => {
                        voice.env += attack_step;
                        if voice.env >= 1.0 {
                            voice.env = 1.0;
                            voice.stage = EnvStage::Decay;
                        }
                    }
                    EnvStage::Decay => {
                        voice.env -= decay_step;
                        if voice.env <= sustain {
                            voice.env = sustain;
                            voice.stage = EnvStage::Sustain;
                        }
                    }
                    EnvStage::Sustain => {
                        voice.env = sustain;
                    }
                    EnvStage::Release => {
                        voice.env -= release_step;
                        if voice.env <= 0.0 {
                            voice.env = 0.0;
                            voice.stage = EnvStage::Off;
                            voice.active = false;
                            continue;
                        }
                    }
                    EnvStage::Off => continue,
                }

                let inc_a = voice.inc_a * bend_ratio;
                let inc_b = voice.inc_b * bend_ratio;
                let a = oscillator(settings.waveform, voice.phase_a, inc_a);
                let b = oscillator(settings.waveform, voice.phase_b, inc_b);
                voice.phase_a += inc_a;
                if voice.phase_a >= 1.0 {
                    voice.phase_a -= 1.0;
                }
                voice.phase_b += inc_b;
                if voice.phase_b >= 1.0 {
                    voice.phase_b -= 1.0;
                }

                let raw = (a * 0.62 + b * 0.38) * voice.env * voice.velocity;

                // Cytomic style state-variable low-pass filter.
                let v3 = raw - voice.ic2;
                let v1 = a1 * voice.ic1 + a2 * v3;
                let v2 = voice.ic2 + a2 * voice.ic1 + a3 * v3;
                voice.ic1 = 2.0 * v1 - voice.ic1;
                voice.ic2 = 2.0 * v2 - voice.ic2;
                mix += v2;
            }

            mix *= settings.level;
            *sample_out += (mix * 1.2).tanh() * 0.8;
        }
    }
}

pub(super) fn poly_blep(t: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        return 0.0;
    }
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

fn oscillator(wave: Waveform, phase: f32, inc: f32) -> f32 {
    match wave {
        Waveform::Sine => (phase * 2.0 * PI).sin(),
        Waveform::Saw => (2.0 * phase - 1.0) - poly_blep(phase, inc),
        Waveform::Square => {
            let mut value = if phase < 0.5 { 1.0 } else { -1.0 };
            value += poly_blep(phase, inc);
            let shifted = (phase + 0.5) % 1.0;
            value -= poly_blep(shifted, inc);
            value * 0.7
        }
        Waveform::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
    }
}
