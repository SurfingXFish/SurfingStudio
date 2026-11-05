//! 所属模块：core::model / 轨道
//! 文件用途：
//! - 定义轨道、轨道类型（MIDI / 音频 / 组合）、子轨可见性与组合父子关系。
//! - 提供片段接受规则、控制器有效性与听感版本递增等纯规则。
//!
//! 函数清单：
//! - `Track::new` / `Track::new_group`：创建普通轨道或组合轨道。
//! - `Track::is_group` / `instrument_name`：组合判断与乐器显示名。
//! - `Track::child_lanes_visible`：子轨是否显示（折叠组合只保留主轨）。
//! - `Track::is_audio_lane` / `has_midi_clips` / `has_audio_clips`：轨道内容类型判断。
//! - `Track::supports_controller` / `accepts_clip`：控制器有效性与片段接受规则。
//! - `Track::bump_revision`：递增听感版本。
//!
//! Module: core::model / track
//! Purpose:
//! - Define tracks, track kinds (MIDI / audio / group), child-lane visibility and group parents.
//! - Provide pure rules for clip acceptance, controller support and audible revision bumps.
//!
//! Functions:
//! - `Track::new` / `Track::new_group`: create an ordinary or group track.
//! - `Track::is_group` / `instrument_name`: group check and instrument label.
//! - `Track::is_audio_lane` / `has_midi_clips` / `has_audio_clips`: track content type checks.
//! - `Track::supports_controller` / `accepts_clip`: controller support and clip acceptance rules.
//! - `Track::bump_revision`: bump the audible revision.

use super::TrackId;
use super::clip::Clip;
use super::controller::{ControllerKind, ControllerLanes};
use super::default_revision;
use super::effect::EffectRef;
use super::instrument::InstrumentRef;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    #[default]
    Midi,
    Audio,
    /// 组合轨道：不承载片段与乐器，只把成员轨道的混音汇总后经过自身效果器链。
    /// Group track: holds no clips or instrument; it sums member tracks and runs its own effects.
    Group,
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
    /// 输出通道对下标：0 = 前左/前右，1 = 中置/低频，2 = 环绕左/环绕右，3 = 后左/后右。
    /// Output channel-pair index: 0 = front L/R, 1 = centre/LFE, 2 = surround L/R, 3 = back L/R.
    /// 工程为立体声布局时只有 0 有效；越界值由引擎夹取。放在结构结尾以便旧工程按默认值解码。
    /// Only 0 is valid for a stereo project; the engine clamps out-of-range values. Kept at the end
    /// so older projects decode through the default.
    #[serde(default)]
    pub output_pair: u8,
    /// 绑定的 MIDI 输入端口名；`None` 表示跟随设置中的默认设备。
    /// Bound MIDI input port name; `None` follows the default device in the settings.
    /// 绑定后该端口的音符优先送到本轨道，默认设备只控制未绑定的选中轨道。
    /// A bound port routes its notes to this track first; the default device only drives the
    /// selected track when that track has no explicit binding.
    #[serde(default)]
    pub input_device: Option<String>,
    /// 上级组合轨道 id；`None` 表示直接挂在主输出下。
    /// Parent group id; `None` means the track belongs directly to the master bus.
    /// 组合轨道自身也可以有上级，用于嵌套组合。
    /// A group track may have a parent too, which allows nested groups.
    #[serde(default)]
    pub group: Option<TrackId>,
    /// 组合轨道是否折叠成员；只在 `kind = Group` 时有意义，折叠后成员行在编曲区隐藏。
    /// Whether a group folds its members; only meaningful for `kind = Group`, where folded members
    /// are hidden in the arrangement.
    #[serde(default)]
    pub collapsed: bool,
    /// 渲染音频轨道记录的来源轨道 id；`None` 表示普通轨道。
    /// Source track id for a rendered audio track; `None` for ordinary tracks.
    /// 右键该轨道可重新渲染来源轨道并覆盖其音频。
    /// The context menu can re-render the source track and replace this clip's audio.
    #[serde(default)]
    pub rendered_from: Option<TrackId>,
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
            output_pair: 0,
            input_device: None,
            group: None,
            collapsed: false,
            rendered_from: None,
            expanded: false,
            revision: 1,
        }
    }

    /// 所属模块：core::model / 轨道
    /// 功能：创建一条组合轨道：主轨作为音量控制器，默认展开以显示效果器子轨。
    /// 输入：
    /// - `id`：新轨道 id。
    /// - `name`：显示名称。
    /// - `color`：轨道颜色。
    /// - `group`：上级组合 id；`None` 表示直接挂在主输出下。
    /// 输出/副作用：返回组合轨道；不修改工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core::model / track
    /// Purpose: Create a group track whose main lane acts as a volume controller and which is
    /// expanded by default so its effect sub-lane is visible.
    /// Inputs:
    /// - `id`: new track id.
    /// - `name`: display name.
    /// - `color`: track colour.
    /// - `group`: parent group id; `None` means the master bus.
    /// Outputs / side effects: the group track; no project change.
    /// Complexity: O(1) time and space.
    pub fn new_group(id: TrackId, name: &str, color: [u8; 3], group: Option<TrackId>) -> Self {
        let mut track = Self::new(id, name, color, InstrumentRef::Synth(Default::default()));
        track.kind = TrackKind::Group;
        track.expanded = true;
        track.group = group;
        track
    }

    /// 所属模块：core::model / 轨道
    /// 功能：是否为组合轨道。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core::model / track
    /// Purpose: Whether this is a group track.
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1) time and space.
    pub fn is_group(&self) -> bool {
        self.kind == TrackKind::Group
    }

    /// 所属模块：core::model / 轨道
    /// 功能：组合轨道是否显示子轨（控制器/效果器）。折叠的组合轨道只保留主轨。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core::model / track
    /// Purpose: Whether child lanes (controllers/effects) are visible. A folded group shows only its
    /// main lane.
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1) time and space.
    pub fn child_lanes_visible(&self) -> bool {
        self.expanded && !(self.is_group() && self.collapsed)
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
        if self.is_group() {
            // 组合轨道只汇总成员的音频，力度/延音对混音没有意义。
            // A group only sums member audio, so velocity and sustain have no effect on the mix.
            return !matches!(kind, ControllerKind::Velocity | ControllerKind::Sustain);
        }
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
        if self.is_group() {
            // 组合轨道只承载成员轨道的混音，本身不接受片段。
            // A group only carries the summed member audio and accepts no clips.
            return false;
        }
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
