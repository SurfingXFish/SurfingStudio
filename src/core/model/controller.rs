//! Automation controllers and interpolation.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ControllerKind {
    Volume,
    Pan,
    Velocity,
    Sustain,
    PitchBend,
}

impl ControllerKind {
    /// 目前可添加到轨道子轨的控制器；力度已迁移到钢琴卷帘的力度控制器，不再作为自动化子轨。
    /// Controllers that can be added as track sub-lanes; velocity moved to the piano-roll velocity
    /// lane and is no longer offered as an automation sub-lane.
    pub const ALL: [ControllerKind; 4] = [
        ControllerKind::Volume,
        ControllerKind::Pan,
        ControllerKind::Sustain,
        ControllerKind::PitchBend,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ControllerKind::Volume => "音量",
            ControllerKind::Pan => "声像",
            ControllerKind::Velocity => "力度",
            ControllerKind::Sustain => "延音",
            ControllerKind::PitchBend => "弯音",
        }
    }

    pub fn center(self) -> f32 {
        match self {
            // Volume and velocity are multipliers: 0.5 = 100 %, 1.0 = 200 %.
            ControllerKind::Volume | ControllerKind::Velocity => 0.5,
            ControllerKind::Pan | ControllerKind::PitchBend => 0.5,
            // Sustain defaults to OFF.
            ControllerKind::Sustain => 0.0,
        }
    }

    pub fn display_value(self, normalized: f32) -> String {
        match self {
            ControllerKind::Volume => format!("{:.0}%", normalized * 200.0),
            ControllerKind::Pan => {
                let pan = normalized * 2.0 - 1.0;
                if pan.abs() < 0.01 {
                    "C".into()
                } else if pan < 0.0 {
                    format!("L{:.0}", -pan * 100.0)
                } else {
                    format!("R{:.0}", pan * 100.0)
                }
            }
            ControllerKind::Velocity => format!("{:.0}%", normalized * 200.0),
            ControllerKind::Sustain => {
                if normalized >= 0.5 {
                    "ON".into()
                } else {
                    "OFF".into()
                }
            }
            ControllerKind::PitchBend => {
                format!("{:+.0}", (normalized * 2.0 - 1.0) * 100.0)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutomationPoint {
    pub beat: f64,
    /// Normalized 0..1.
    pub value: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ControllerLanes {
    #[serde(default)]
    pub volume: Vec<AutomationPoint>,
    #[serde(default)]
    pub pan: Vec<AutomationPoint>,
    #[serde(default)]
    pub velocity: Vec<AutomationPoint>,
    #[serde(default)]
    pub sustain: Vec<AutomationPoint>,
    #[serde(default)]
    pub pitch_bend: Vec<AutomationPoint>,
}

impl ControllerLanes {
    pub fn get(&self, kind: ControllerKind) -> &[AutomationPoint] {
        match kind {
            ControllerKind::Volume => &self.volume,
            ControllerKind::Pan => &self.pan,
            ControllerKind::Velocity => &self.velocity,
            ControllerKind::Sustain => &self.sustain,
            ControllerKind::PitchBend => &self.pitch_bend,
        }
    }

    pub fn get_mut(&mut self, kind: ControllerKind) -> &mut Vec<AutomationPoint> {
        match kind {
            ControllerKind::Volume => &mut self.volume,
            ControllerKind::Pan => &mut self.pan,
            ControllerKind::Velocity => &mut self.velocity,
            ControllerKind::Sustain => &mut self.sustain,
            ControllerKind::PitchBend => &mut self.pitch_bend,
        }
    }

    pub fn value_at(&self, kind: ControllerKind, beat: f64) -> Option<f32> {
        evaluate_automation(self.get(kind), beat)
    }
}

/// Linear interpolation between automation points. Returns `None` when empty.
pub fn evaluate_automation(points: &[AutomationPoint], beat: f64) -> Option<f32> {
    if points.is_empty() {
        return None;
    }
    if beat <= points[0].beat {
        return Some(points[0].value);
    }
    for pair in points.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if beat <= b.beat {
            let span = (b.beat - a.beat).max(1e-9);
            let t = ((beat - a.beat) / span).clamp(0.0, 1.0) as f32;
            return Some(a.value + (b.value - a.value) * t);
        }
    }
    points.last().map(|point| point.value)
}
