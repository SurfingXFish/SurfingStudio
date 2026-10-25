//! 所属模块：app / 界面状态类型
//! 文件用途：
//! - 定义仅用于界面的临时状态：剪贴板、浮动卷帘、主页页面、工作区视图等。
//!
//! 导出项：
//! - `ScanMode` / `Clipboard` / `RollKind` / `RollWindow` / `RollDragState`：编辑与扫描状态。
//! - `PianoViewState` / `LanePastePrompt`：卷帘视图与子轨覆盖确认。
//! - `WorkspaceView` / `HomePage`：主页与工作室视图、主页左侧菜单页面（项目 / 模板 / 插件 / 提示）。
//! - `WindowAction`：窗口级动作（显示主页 / 打开工程窗口 / 新建工程窗口 / 回到工程窗口）。
//! - `InstrumentCategory` / `InstrumentPicker`：乐器窗口的种类与目标轨道。
//!
//! Module: app / UI-only state types
//! Purpose:
//! - Define transient UI state: clipboard, floating rolls, home pages and the workspace view.
//!
//! Exports:
//! - `ScanMode` / `Clipboard` / `RollKind` / `RollWindow` / `RollDragState`: editing and scan state.
//! - `PianoViewState` / `LanePastePrompt`: roll view state and child-lane overwrite prompt.
//! - `WorkspaceView` / `HomePage`: home/studio views and the home sidebar page (projects / templates / plugins / tips).
//! - `WindowAction`: window-level actions (show home / open a project window / create a project window / resume).
//! - `InstrumentCategory` / `InstrumentPicker`: instrument window categories and target track.

use crate::core::model::{
    AutomationPoint, Clip, ControllerKind, EffectRef, Note, TemplateId, TrackId,
};
use eframe::egui;
use std::path::PathBuf;

/// 当前工作区：主页或工作室（编曲界面）。
/// Current workspace: the home page or the studio (arrangement) view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceView {
    Home,
    Studio,
}

/// 窗口级动作：由 `SurfingApp` 产生，由外壳 `WindowHost` 执行。
/// Window-level action produced by `SurfingApp` and executed by `WindowHost`.
#[derive(Clone, Debug)]
pub enum WindowAction {
    /// 显示主页窗口（工程窗口保持打开）。
    /// Show the home window (project windows stay open).
    ShowHome,
    /// 打开工程文件窗口。
    /// Open a project file window.
    OpenProject { path: PathBuf },
    /// 新建工程窗口；`template` 为 `Some` 时用模板/范例预填充。
    /// Create a project window; `template` pre-fills it with a template/example.
    NewProject { template: Option<TemplateId> },
    /// 回到最近使用的工程窗口。
    /// Resume the most recently used project window.
    ResumeProject,
}

/// 主页右侧内容对应的左侧菜单项。
/// Home sidebar entry that selects the right-hand content page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HomePage {
    Projects,
    Templates,
    Plugins,
    Tips,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanMode {
    All,
    Vst3,
    Legacy,
    /// 启动时的增量扫描：复用缓存记录，只识别新增或变化的插件文件。
    /// Incremental startup scan: reuse cached records and only classify new or changed files.
    Incremental,
}

/// 乐器窗口左侧的种类筛选。
/// Category filter on the left side of the instrument window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InstrumentCategory {
    #[default]
    All,
    Favorites,
    Piano,
    AcousticGuitar,
    ElectricGuitar,
    Strings,
    /// 与外部音乐合成软件桥接的插件。
    /// Plugins that bridge to another music synthesis application.
    Bridge,
    Other,
}

impl InstrumentCategory {
    pub const ALL: [InstrumentCategory; 8] = [
        InstrumentCategory::All,
        InstrumentCategory::Favorites,
        InstrumentCategory::Piano,
        InstrumentCategory::AcousticGuitar,
        InstrumentCategory::ElectricGuitar,
        InstrumentCategory::Strings,
        InstrumentCategory::Bridge,
        InstrumentCategory::Other,
    ];

    /// 所属模块：app / 界面状态 / 乐器窗口
    /// 功能：返回种类的中文名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / UI state / instrument window
    /// Purpose: Return the Chinese label of a category.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            InstrumentCategory::All => "全部",
            InstrumentCategory::Favorites => "收藏",
            InstrumentCategory::Piano => "钢琴",
            InstrumentCategory::AcousticGuitar => "木吉他",
            InstrumentCategory::ElectricGuitar => "电吉他",
            InstrumentCategory::Strings => "弦乐",
            InstrumentCategory::Bridge => "桥接器",
            InstrumentCategory::Other => "其他",
        }
    }
}

/// 乐器窗口：目标轨道、种类筛选与搜索文本。
/// Instrument window: target track, category filter and search text.
#[derive(Clone, Debug)]
pub struct InstrumentPicker {
    pub track_id: TrackId,
    pub category: InstrumentCategory,
    pub search: String,
}

impl InstrumentPicker {
    /// 所属模块：app / 界面状态 / 乐器窗口
    /// 功能：为指定轨道创建乐器窗口状态（默认“全部”且无搜索）。
    /// 输入：`track_id` 目标轨道 id。
    /// 输出/副作用：返回窗口状态；不修改工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / UI state / instrument window
    /// Purpose: Create instrument-window state for one track (all categories, empty search).
    /// Inputs: `track_id` — target track id.
    /// Outputs / side effects: window state; no project change.
    /// Complexity: O(1) time and space.
    pub fn new(track_id: TrackId) -> Self {
        Self {
            track_id,
            category: InstrumentCategory::All,
            search: String::new(),
        }
    }
}

/// Copied selection (Ctrl+C), pasted with Ctrl+V.
#[derive(Clone, Debug, Default)]
pub enum Clipboard {
    #[default]
    Empty,
    /// (original track index, clip) so cross-track copies paste back to their
    /// own tracks.
    Clips(Vec<(usize, Clip)>),
    Notes(Vec<Note>),
    ControllerPoints(Vec<AutomationPoint>),
    /// A whole controller child lane, copied from the arrangement header.
    ControllerLane {
        kind: ControllerKind,
        points: Vec<AutomationPoint>,
    },
    /// A single insert effect.
    Effect(EffectRef),
    /// A whole insert chain, copied from the effects child lane.
    EffectChain(Vec<EffectRef>),
}

/// Which editor roll a floating window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RollKind {
    PianoRoll,
    Controller(ControllerKind),
    Effects,
}

/// One floating editor roll window. Each track can own at most one.
#[derive(Clone, Debug)]
pub struct RollWindow {
    pub track_id: TrackId,
    pub kind: RollKind,
    /// Render this window last so it appears above the others.
    pub bring_to_front: bool,
}

/// An in-progress "pull the roll out of the bottom panel" gesture. The new
/// window is created as soon as the pointer leaves a small threshold and then
/// follows the mouse until release.
#[derive(Clone, Copy, Debug)]
pub struct RollDragState {
    pub track_id: TrackId,
    pub kind: RollKind,
    /// Pointer position when the drag started, in the source viewport.
    pub press_pos: egui::Pos2,
    /// Pointer offset inside the roll panel at drag start.
    pub grab: egui::Vec2,
    /// Size of the roll panel that was grabbed.
    pub size: egui::Vec2,
    /// Monitor-space origin of the source viewport's content.
    pub source_origin: egui::Pos2,
    /// Difference between the source window's outer and inner top-left corner.
    pub decoration: egui::Vec2,
    pub started: bool,
    /// Monitor-space outer position the floating window should follow.
    pub desired: egui::Pos2,
}

/// Saved scroll/zoom of a piano roll, kept per track so switching tracks or
/// opening another track never moves an already-open roll.
#[derive(Clone, Copy, Debug)]
pub struct PianoViewState {
    pub zoom: f32,
    pub view_beat: f64,
    pub top_pitch: i32,
    pub note_h: f32,
}

impl Default for PianoViewState {
    fn default() -> Self {
        Self {
            zoom: 2.0,
            view_beat: 0.0,
            top_pitch: 88,
            note_h: 15.0,
        }
    }
}

/// Pending paste that would overwrite an existing child lane.
#[derive(Clone, Debug)]
pub struct LanePastePrompt {
    pub target_track: usize,
    pub payload: Clipboard,
}
