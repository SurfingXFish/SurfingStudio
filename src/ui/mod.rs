//! 所属模块：ui / 共享视图类型
//! 文件用途：
//! - 声明各视图子模块。
//! - 定义底部卷帘、插件过滤、鼠标工具与拖拽状态等跨视图共享类型。
//!
//! Module: ui / shared view types
//! Purpose:
//! - Declare the view submodules.
//! - Define shared types: bottom views, plugin filters, pointer tools and drag state.

pub mod arrangement;
pub mod browser;
pub mod controller;
pub mod effect_picker;
pub mod effects;
pub mod export;
pub mod help;
pub mod inspector;
pub mod instrument_picker;
pub mod piano_roll;
pub mod theme;

use crate::core::model::{ControllerKind, TrackId};
use eframe::egui;

/// Which editor is shown in the bottom panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BottomView {
    #[default]
    PianoRoll,
    Controller {
        track_id: TrackId,
        kind: ControllerKind,
    },
    Effects {
        track_id: TrackId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginFilter {
    All,
    Instruments,
    Effects,
    Unknown,
}

/// 效果器窗口左侧的种类筛选。
/// Category filter on the left side of the effect window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EffectCategory {
    #[default]
    All,
    Favorites,
    Eq,
    Dynamics,
    Reverb,
    Delay,
    Distortion,
    Modulation,
    Other,
}

impl EffectCategory {
    pub const ALL: [EffectCategory; 9] = [
        EffectCategory::All,
        EffectCategory::Favorites,
        EffectCategory::Eq,
        EffectCategory::Dynamics,
        EffectCategory::Reverb,
        EffectCategory::Delay,
        EffectCategory::Distortion,
        EffectCategory::Modulation,
        EffectCategory::Other,
    ];

    /// 所属模块：ui / 效果器窗口
    /// 功能：返回种类的中文名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ui / effect window
    /// Purpose: Return the Chinese label of a category.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            EffectCategory::All => "全部",
            EffectCategory::Favorites => "收藏",
            EffectCategory::Eq => "EQ",
            EffectCategory::Dynamics => "动态",
            EffectCategory::Reverb => "混响",
            EffectCategory::Delay => "延时",
            EffectCategory::Distortion => "失真",
            EffectCategory::Modulation => "调制",
            EffectCategory::Other => "其他",
        }
    }
}

impl PluginFilter {
    pub fn label(&self) -> &'static str {
        match self {
            PluginFilter::All => "全部",
            PluginFilter::Instruments => "乐器",
            PluginFilter::Effects => "效果器",
            PluginFilter::Unknown => "未知",
        }
    }
}

/// Pointer tool shared by the arrangement, piano roll and controller roll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseMode {
    Select,
    Draw,
    Cut,
}

impl MouseMode {
    pub const ALL: [MouseMode; 3] = [MouseMode::Select, MouseMode::Draw, MouseMode::Cut];

    pub fn label(self) -> &'static str {
        match self {
            MouseMode::Select => "选择",
            MouseMode::Draw => "绘制",
            MouseMode::Cut => "切割",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            MouseMode::Select => "↖",
            MouseMode::Draw => "✎",
            MouseMode::Cut => "✂",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArrangementDrag {
    None,
    Ruler,
    LoopStart,
    LoopEnd,
    /// Dragging the master bus bottom edge to change its height.
    MasterResize,
    /// Dragging clips horizontally. `group` holds (track, clip, original start).
    /// Clips are never moved across tracks this way.
    ClipMove {
        grab_beat: f64,
        group: Vec<(usize, usize, f64)>,
    },
    /// Dragging a clip edge. `group` holds (track, clip, orig start, orig length).
    /// `anchor` is the clip whose edge was grabbed.
    ClipResize {
        left_edge: bool,
        anchor: (usize, usize),
        group: Vec<(usize, usize, f64, f64)>,
    },
    /// Selecting clips with a rubber band, across all tracks.
    Marquee {
        start: egui::Pos2,
    },
    /// Dragging out a new clip with the draw tool.
    CreateClip {
        track: usize,
        clip: usize,
        start_beat: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum PianoDrag {
    None,
    Create {
        clip: usize,
        note: usize,
        start: f64,
    },
    /// Dragging notes. `group` holds (clip, note, orig absolute start, orig pitch).
    Move {
        grab_offset: f64,
        anchor_start: f64,
        anchor_pitch: u8,
        group: Vec<(usize, usize, f64, u8)>,
    },
    /// Dragging a note edge. `group` holds (clip, note, orig absolute start, orig length).
    /// `anchor` is the note whose edge was grabbed.
    Resize {
        left_edge: bool,
        anchor: (usize, usize),
        group: Vec<(usize, usize, f64, f64)>,
    },
    /// Rubber-band selection of notes.
    Marquee {
        start: egui::Pos2,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginTarget {
    Instrument,
    Effect,
    MasterEffect,
}

pub struct BrowserState {
    pub target: PluginTarget,
    pub filter: String,
    pub kind_filter: PluginFilter,
    /// 效果器窗口的种类筛选。
    /// Category filter of the effect window.
    pub category: EffectCategory,
}

impl BrowserState {
    pub fn new(target: PluginTarget) -> Self {
        let kind_filter = match target {
            PluginTarget::Instrument => PluginFilter::Instruments,
            PluginTarget::Effect | PluginTarget::MasterEffect => PluginFilter::Effects,
        };
        Self {
            target,
            filter: String::new(),
            kind_filter,
            category: EffectCategory::All,
        }
    }
}
