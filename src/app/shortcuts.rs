//! 所属模块：app / 可配置快捷键
//! 文件用途：
//! - 定义可绑定快捷键的动作、默认绑定、解析/格式化与冲突检测。
//! - 把键盘事件匹配到动作，并提供视图滑动与控制器画笔切换的默认实现。
//!
//! 函数清单：
//! - `ShortcutAction::ALL` / `id` / `label` / `group` / `default_bindings`：动作元数据。
//! - `Binding::new` / `label` / `text` / `parse` / `matches`：单个按键绑定。
//! - `AppSettings::shortcut_bindings` / `shortcut_label` / `shortcut_pressed` / `set_shortcut` /
//!   `reset_shortcut` / `reset_all_shortcuts` / `shortcut_conflict`：设置的读写与匹配。
//! - `SurfingApp::scroll_view` / `ScrollDirection`：按方向滑动编曲区或钢琴卷帘视图。
//! - `key_label` / `key_name` / `key_from_name`：按键的中文显示名、持久化名与解析。
//!
//! Module: app / configurable shortcuts
//! Purpose:
//! - Define bindable actions, their defaults, parsing/formatting and conflict detection.
//! - Match keyboard events to actions and implement view scrolling and controller tool switching.
//!
//! Functions:
//! - `ShortcutAction::ALL` / `id` / `label` / `group` / `default_bindings`: action metadata.
//! - `Binding::new` / `label` / `text` / `parse` / `matches`: one key binding.
//! - `AppSettings::shortcut_bindings` / `shortcut_label` / `shortcut_pressed` / `set_shortcut` /
//!   `reset_shortcut` / `reset_all_shortcuts` / `shortcut_conflict`: settings access and matching.
//! - `SurfingApp::scroll_view` / `ScrollDirection`: scroll the arrangement or piano roll in one
//!   direction.
//! - `key_label` / `key_name` / `key_from_name`: display names, persistence names and parsing.

use super::settings::AppSettings;
use super::*;

/// 快捷键动作分组。
/// Shortcut action groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutGroup {
    Transport,
    FileEdit,
    View,
    Tools,
    Scroll,
}

impl ShortcutGroup {
    pub const ALL: [ShortcutGroup; 5] = [
        ShortcutGroup::Transport,
        ShortcutGroup::FileEdit,
        ShortcutGroup::View,
        ShortcutGroup::Tools,
        ShortcutGroup::Scroll,
    ];

    /// 所属模块：app / 可配置快捷键
    /// 功能：返回分组名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Return the group label.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            ShortcutGroup::Transport => "传输",
            ShortcutGroup::FileEdit => "文件与编辑",
            ShortcutGroup::View => "视图",
            ShortcutGroup::Tools => "工具与画笔",
            ShortcutGroup::Scroll => "滑动",
        }
    }
}

/// 可配置的快捷键动作。
/// Actions whose keys can be configured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShortcutAction {
    PlayPause,
    GoToStart,
    GoToEnd,
    Save,
    Open,
    New,
    Export,
    Undo,
    Redo,
    Copy,
    Paste,
    Delete,
    Cancel,
    Help,
    PluginBrowser,
    EffectsRoll,
    ControllerRoll,
    ToolSelect,
    ToolDraw,
    ToolCut,
    ControllerBrush,
    ControllerLine,
    ControllerSmooth,
    ControllerErase,
    ControllerToolNext,
    ScrollLeft,
    ScrollRight,
    ScrollUp,
    ScrollDown,
}

impl ShortcutAction {
    pub const ALL: [ShortcutAction; 29] = [
        ShortcutAction::PlayPause,
        ShortcutAction::GoToStart,
        ShortcutAction::GoToEnd,
        ShortcutAction::Save,
        ShortcutAction::Open,
        ShortcutAction::New,
        ShortcutAction::Export,
        ShortcutAction::Undo,
        ShortcutAction::Redo,
        ShortcutAction::Copy,
        ShortcutAction::Paste,
        ShortcutAction::Delete,
        ShortcutAction::Cancel,
        ShortcutAction::Help,
        ShortcutAction::PluginBrowser,
        ShortcutAction::EffectsRoll,
        ShortcutAction::ControllerRoll,
        ShortcutAction::ToolSelect,
        ShortcutAction::ToolDraw,
        ShortcutAction::ToolCut,
        ShortcutAction::ControllerBrush,
        ShortcutAction::ControllerLine,
        ShortcutAction::ControllerSmooth,
        ShortcutAction::ControllerErase,
        ShortcutAction::ControllerToolNext,
        ShortcutAction::ScrollLeft,
        ShortcutAction::ScrollRight,
        ShortcutAction::ScrollUp,
        ShortcutAction::ScrollDown,
    ];

    /// 所属模块：app / 可配置快捷键
    /// 功能：返回持久化用的稳定标识。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Return the stable id used for persistence.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn id(self) -> &'static str {
        match self {
            ShortcutAction::PlayPause => "play_pause",
            ShortcutAction::GoToStart => "go_to_start",
            ShortcutAction::GoToEnd => "go_to_end",
            ShortcutAction::Save => "save",
            ShortcutAction::Open => "open",
            ShortcutAction::New => "new",
            ShortcutAction::Export => "export",
            ShortcutAction::Undo => "undo",
            ShortcutAction::Redo => "redo",
            ShortcutAction::Copy => "copy",
            ShortcutAction::Paste => "paste",
            ShortcutAction::Delete => "delete",
            ShortcutAction::Cancel => "cancel",
            ShortcutAction::Help => "help",
            ShortcutAction::PluginBrowser => "plugin_browser",
            ShortcutAction::EffectsRoll => "effects_roll",
            ShortcutAction::ControllerRoll => "controller_roll",
            ShortcutAction::ToolSelect => "tool_select",
            ShortcutAction::ToolDraw => "tool_draw",
            ShortcutAction::ToolCut => "tool_cut",
            ShortcutAction::ControllerBrush => "controller_brush",
            ShortcutAction::ControllerLine => "controller_line",
            ShortcutAction::ControllerSmooth => "controller_smooth",
            ShortcutAction::ControllerErase => "controller_erase",
            ShortcutAction::ControllerToolNext => "controller_tool_next",
            ShortcutAction::ScrollLeft => "scroll_left",
            ShortcutAction::ScrollRight => "scroll_right",
            ShortcutAction::ScrollUp => "scroll_up",
            ShortcutAction::ScrollDown => "scroll_down",
        }
    }

    /// 所属模块：app / 可配置快捷键
    /// 功能：返回设置页显示的动作名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Return the action label shown in the settings page.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            ShortcutAction::PlayPause => "播放 / 暂停",
            ShortcutAction::GoToStart => "回到开头",
            ShortcutAction::GoToEnd => "跳到结尾",
            ShortcutAction::Save => "保存工程",
            ShortcutAction::Open => "打开工程",
            ShortcutAction::New => "新建工程",
            ShortcutAction::Export => "导出 WAV",
            ShortcutAction::Undo => "撤销",
            ShortcutAction::Redo => "重做",
            ShortcutAction::Copy => "复制",
            ShortcutAction::Paste => "粘贴",
            ShortcutAction::Delete => "删除选中",
            ShortcutAction::Cancel => "取消 / 关闭",
            ShortcutAction::Help => "帮助窗口",
            ShortcutAction::PluginBrowser => "插件浏览器",
            ShortcutAction::EffectsRoll => "效果器链",
            ShortcutAction::ControllerRoll => "控制器卷帘",
            ShortcutAction::ToolSelect => "选择工具",
            ShortcutAction::ToolDraw => "绘制工具",
            ShortcutAction::ToolCut => "切割工具",
            ShortcutAction::ControllerBrush => "控制器画笔",
            ShortcutAction::ControllerLine => "控制器直线",
            ShortcutAction::ControllerSmooth => "控制器平滑",
            ShortcutAction::ControllerErase => "控制器擦除",
            ShortcutAction::ControllerToolNext => "切换下一个控制器画笔",
            ShortcutAction::ScrollLeft => "视图向左滑动",
            ShortcutAction::ScrollRight => "视图向右滑动",
            ShortcutAction::ScrollUp => "视图向上滑动",
            ShortcutAction::ScrollDown => "视图向下滑动",
        }
    }

    /// 所属模块：app / 可配置快捷键
    /// 功能：返回动作所属分组。
    /// 输入：`self`。
    /// 输出/副作用：分组枚举；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Return the action's group.
    /// Inputs: `self`.
    /// Outputs / side effects: the group; no state change.
    /// Complexity: O(1) time and space.
    pub fn group(self) -> ShortcutGroup {
        match self {
            ShortcutAction::PlayPause | ShortcutAction::GoToStart | ShortcutAction::GoToEnd => {
                ShortcutGroup::Transport
            }
            ShortcutAction::Save
            | ShortcutAction::Open
            | ShortcutAction::New
            | ShortcutAction::Export
            | ShortcutAction::Undo
            | ShortcutAction::Redo
            | ShortcutAction::Copy
            | ShortcutAction::Paste
            | ShortcutAction::Delete
            | ShortcutAction::Cancel => ShortcutGroup::FileEdit,
            ShortcutAction::Help
            | ShortcutAction::PluginBrowser
            | ShortcutAction::EffectsRoll
            | ShortcutAction::ControllerRoll => ShortcutGroup::View,
            ShortcutAction::ToolSelect
            | ShortcutAction::ToolDraw
            | ShortcutAction::ToolCut
            | ShortcutAction::ControllerBrush
            | ShortcutAction::ControllerLine
            | ShortcutAction::ControllerSmooth
            | ShortcutAction::ControllerErase
            | ShortcutAction::ControllerToolNext => ShortcutGroup::Tools,
            ShortcutAction::ScrollLeft
            | ShortcutAction::ScrollRight
            | ShortcutAction::ScrollUp
            | ShortcutAction::ScrollDown => ShortcutGroup::Scroll,
        }
    }

    /// 所属模块：app / 可配置快捷键
    /// 功能：返回该动作的默认绑定（可有多组）。
    /// 输入：`self`。
    /// 输出/副作用：新的绑定列表；不修改状态。
    /// 复杂度：时间 O(绑定数)，空间 O(绑定数)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Return the default bindings of the action (possibly several).
    /// Inputs: `self`.
    /// Outputs / side effects: a new binding list; no state change.
    /// Complexity: O(bindings) time and space.
    pub fn default_bindings(self) -> Vec<Binding> {
        use egui::Key;
        const NONE: bool = false;
        const CTRL: bool = true;
        match self {
            ShortcutAction::PlayPause => vec![Binding::new(Key::Space, NONE, NONE, NONE)],
            ShortcutAction::GoToStart => vec![Binding::new(Key::Home, NONE, NONE, NONE)],
            ShortcutAction::GoToEnd => vec![Binding::new(Key::End, NONE, NONE, NONE)],
            ShortcutAction::Save => vec![Binding::new(Key::S, CTRL, NONE, NONE)],
            ShortcutAction::Open => vec![Binding::new(Key::O, CTRL, NONE, NONE)],
            ShortcutAction::New => vec![Binding::new(Key::N, CTRL, NONE, NONE)],
            ShortcutAction::Export => vec![Binding::new(Key::E, CTRL, NONE, NONE)],
            ShortcutAction::Undo => vec![Binding::new(Key::Z, CTRL, NONE, NONE)],
            ShortcutAction::Redo => vec![
                Binding::new(Key::Z, CTRL, true, NONE),
                Binding::new(Key::Y, CTRL, NONE, NONE),
                // 保留旧版帮助中的 Ctrl+Shift+V 重做别名。
                // Keep Ctrl+Shift+V as the redo alias documented by older builds.
                Binding::new(Key::V, CTRL, true, NONE),
            ],
            ShortcutAction::Copy => vec![Binding::new(Key::C, CTRL, NONE, NONE)],
            ShortcutAction::Paste => vec![Binding::new(Key::V, CTRL, NONE, NONE)],
            ShortcutAction::Delete => vec![
                Binding::new(Key::Delete, NONE, NONE, NONE),
                Binding::new(Key::Backspace, NONE, NONE, NONE),
            ],
            ShortcutAction::Cancel => vec![Binding::new(Key::Escape, NONE, NONE, NONE)],
            ShortcutAction::Help => vec![Binding::new(Key::F1, NONE, NONE, NONE)],
            ShortcutAction::PluginBrowser => vec![Binding::new(Key::F9, NONE, NONE, NONE)],
            ShortcutAction::EffectsRoll => vec![Binding::new(Key::F8, NONE, NONE, NONE)],
            ShortcutAction::ControllerRoll => vec![Binding::new(Key::F7, NONE, NONE, NONE)],
            ShortcutAction::ToolSelect => vec![Binding::new(Key::Num1, NONE, NONE, NONE)],
            ShortcutAction::ToolDraw => vec![Binding::new(Key::Num2, NONE, NONE, NONE)],
            ShortcutAction::ToolCut => vec![Binding::new(Key::Num3, NONE, NONE, NONE)],
            ShortcutAction::ControllerBrush => vec![Binding::new(Key::Num1, CTRL, NONE, NONE)],
            ShortcutAction::ControllerLine => vec![Binding::new(Key::Num2, CTRL, NONE, NONE)],
            ShortcutAction::ControllerSmooth => vec![Binding::new(Key::Num3, CTRL, NONE, NONE)],
            ShortcutAction::ControllerErase => vec![Binding::new(Key::Num4, CTRL, NONE, NONE)],
            ShortcutAction::ControllerToolNext => vec![Binding::new(Key::Num4, NONE, NONE, NONE)],
            ShortcutAction::ScrollLeft => vec![Binding::new(Key::ArrowLeft, NONE, NONE, NONE)],
            ShortcutAction::ScrollRight => vec![Binding::new(Key::ArrowRight, NONE, NONE, NONE)],
            ShortcutAction::ScrollUp => vec![Binding::new(Key::ArrowUp, NONE, NONE, NONE)],
            ShortcutAction::ScrollDown => vec![Binding::new(Key::ArrowDown, NONE, NONE, NONE)],
        }
    }
}

/// 单个按键绑定（按键 + 修饰键）。
/// One key binding (key plus modifiers).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Binding {
    pub key: egui::Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Binding {
    /// 所属模块：app / 可配置快捷键
    /// 功能：构造一个绑定。
    /// 输入：`key` 按键；`ctrl` / `shift` / `alt` 修饰键。
    /// 输出/副作用：绑定值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Build one binding.
    /// Inputs: `key`; `ctrl` / `shift` / `alt` modifiers.
    /// Outputs / side effects: the binding; no state change.
    /// Complexity: O(1) time and space.
    pub const fn new(key: egui::Key, ctrl: bool, shift: bool, alt: bool) -> Self {
        Self {
            key,
            ctrl,
            shift,
            alt,
        }
    }

    /// 所属模块：app / 可配置快捷键
    /// 功能：返回可持久化、可展示的绑定文本（如 `Ctrl+Shift+S`）。
    /// 输入：`self`。
    /// 输出/副作用：绑定文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Return the persistable/display text of the binding (e.g. `Ctrl+Shift+S`).
    /// Inputs: `self`.
    /// Outputs / side effects: binding text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(4);
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        parts.push(key_label(self.key));
        parts.join("+")
    }

    /// 所属模块：app / 可配置快捷键
    /// 功能：返回可被 `parse` 还原的持久化文本（按键名统一使用英文）。
    /// 输入：`self`。
    /// 输出/副作用：绑定文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Return persistence text that `parse` can round-trip (key names in English).
    /// Inputs: `self`.
    /// Outputs / side effects: binding text; no state change.
    /// Complexity: O(1) time and space.
    pub fn text(self) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(4);
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        parts.push(key_name(self.key));
        parts.join("+")
    }

    /// 所属模块：app / 可配置快捷键
    /// 功能：解析绑定文本；无法识别时返回 `None`。
    /// 输入：`text`：形如 `Ctrl+Shift+S` 的文本（大小写不敏感）。
    /// 输出/副作用：绑定值；不修改状态。
    /// 复杂度：时间 O(文本长度)，空间 O(1)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Parse binding text; return `None` when unrecognised.
    /// Inputs: `text`: text such as `Ctrl+Shift+S` (case-insensitive).
    /// Outputs / side effects: the binding; no state change.
    /// Complexity: O(text length) time and O(1) space.
    pub fn parse(text: &str) -> Option<Self> {
        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut key = None;
        for part in text.split('+').map(str::trim) {
            if part.is_empty() {
                continue;
            }
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "cmd" | "command" => ctrl = true,
                "shift" => shift = true,
                "alt" | "option" => alt = true,
                other => key = key_from_name(other),
            }
        }
        key.map(|key| Self {
            key,
            ctrl,
            shift,
            alt,
        })
    }

    /// 所属模块：app / 可配置快捷键
    /// 功能：判断该绑定是否在本帧被按下（修饰键必须完全一致）。
    /// 输入：
    /// - `ctx`：egui 上下文。
    /// 输出/副作用：返回布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Whether this binding was pressed this frame (modifiers must match exactly).
    /// Inputs:
    /// - `ctx`: egui context.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1) time and space.
    pub fn matches(self, ctx: &egui::Context) -> bool {
        ctx.input(|i| {
            i.key_pressed(self.key)
                && i.modifiers.command == self.ctrl
                && i.modifiers.shift == self.shift
                && i.modifiers.alt == self.alt
        })
    }
}

fn key_label(key: egui::Key) -> &'static str {
    use egui::Key;
    match key {
        Key::Space => "空格",
        Key::Escape => "Esc",
        Key::Enter => "Enter",
        Key::Tab => "Tab",
        Key::Backspace => "Backspace",
        Key::Delete => "Delete",
        Key::ArrowLeft => "←",
        Key::ArrowRight => "→",
        Key::ArrowUp => "↑",
        Key::ArrowDown => "↓",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
        Key::Insert => "Insert",
        other => key_name(other),
    }
}

fn key_name(key: egui::Key) -> &'static str {
    use egui::Key;
    match key {
        Key::A => "A",
        Key::B => "B",
        Key::C => "C",
        Key::D => "D",
        Key::E => "E",
        Key::F => "F",
        Key::G => "G",
        Key::H => "H",
        Key::I => "I",
        Key::J => "J",
        Key::K => "K",
        Key::L => "L",
        Key::M => "M",
        Key::N => "N",
        Key::O => "O",
        Key::P => "P",
        Key::Q => "Q",
        Key::R => "R",
        Key::S => "S",
        Key::T => "T",
        Key::U => "U",
        Key::V => "V",
        Key::W => "W",
        Key::X => "X",
        Key::Y => "Y",
        Key::Z => "Z",
        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num4 => "4",
        Key::Num5 => "5",
        Key::Num6 => "6",
        Key::Num7 => "7",
        Key::Num8 => "8",
        Key::Num9 => "9",
        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",
        Key::Space => "Space",
        Key::Escape => "Escape",
        Key::Enter => "Enter",
        Key::Tab => "Tab",
        Key::Backspace => "Backspace",
        Key::Delete => "Delete",
        Key::Insert => "Insert",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",
        Key::ArrowUp => "Up",
        Key::ArrowDown => "Down",
        Key::Minus => "Minus",
        Key::Plus => "Plus",
        Key::Equals => "Equals",
        Key::Comma => "Comma",
        Key::Period => "Period",
        Key::Slash => "Slash",
        Key::Semicolon => "Semicolon",
        Key::Quote => "Quote",
        Key::Backtick => "Backtick",
        Key::OpenBracket => "OpenBracket",
        Key::CloseBracket => "CloseBracket",
        Key::Backslash => "Backslash",
        _ => "Unknown",
    }
}

fn key_from_name(name: &str) -> Option<egui::Key> {
    use egui::Key;
    let key = match name {
        "a" => Key::A,
        "b" => Key::B,
        "c" => Key::C,
        "d" => Key::D,
        "e" => Key::E,
        "f" => Key::F,
        "g" => Key::G,
        "h" => Key::H,
        "i" => Key::I,
        "j" => Key::J,
        "k" => Key::K,
        "l" => Key::L,
        "m" => Key::M,
        "n" => Key::N,
        "o" => Key::O,
        "p" => Key::P,
        "q" => Key::Q,
        "r" => Key::R,
        "s" => Key::S,
        "t" => Key::T,
        "u" => Key::U,
        "v" => Key::V,
        "w" => Key::W,
        "x" => Key::X,
        "y" => Key::Y,
        "z" => Key::Z,
        "0" => Key::Num0,
        "1" => Key::Num1,
        "2" => Key::Num2,
        "3" => Key::Num3,
        "4" => Key::Num4,
        "5" => Key::Num5,
        "6" => Key::Num6,
        "7" => Key::Num7,
        "8" => Key::Num8,
        "9" => Key::Num9,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        "space" => Key::Space,
        "escape" | "esc" => Key::Escape,
        "enter" | "return" => Key::Enter,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "insert" | "ins" => Key::Insert,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "pgup" => Key::PageUp,
        "pagedown" | "pgdn" => Key::PageDown,
        "left" | "arrowleft" => Key::ArrowLeft,
        "right" | "arrowright" => Key::ArrowRight,
        "up" | "arrowup" => Key::ArrowUp,
        "down" | "arrowdown" => Key::ArrowDown,
        "minus" => Key::Minus,
        "plus" => Key::Plus,
        "equals" => Key::Equals,
        "comma" => Key::Comma,
        "period" => Key::Period,
        "slash" => Key::Slash,
        "semicolon" => Key::Semicolon,
        "quote" => Key::Quote,
        "backtick" => Key::Backtick,
        "openbracket" => Key::OpenBracket,
        "closebracket" => Key::CloseBracket,
        "backslash" => Key::Backslash,
        _ => return None,
    };
    Some(key)
}

impl AppSettings {
    /// 所属模块：app / 可配置快捷键
    /// 功能：返回动作当前生效的绑定；用户未自定义时使用默认值。
    /// 输入：`action` 动作。
    /// 输出/副作用：绑定列表；不修改状态。
    /// 复杂度：时间 O(绑定数 × 文本长度)，空间 O(绑定数)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Return the effective bindings of an action; defaults apply when the user has not
    /// customised it.
    /// Inputs: `action`: the action.
    /// Outputs / side effects: binding list; no state change.
    /// Complexity: O(bindings × text length) time and O(bindings) space.
    pub fn shortcut_bindings(&self, action: ShortcutAction) -> Vec<Binding> {
        let Some(text) = self.shortcuts.get(action.id()) else {
            return action.default_bindings();
        };
        text.split(',')
            .filter_map(|entry| Binding::parse(entry))
            .collect()
    }

    /// 所属模块：app / 可配置快捷键
    /// 功能：返回动作的展示文本；未绑定时返回“未绑定”。
    /// 输入：`action` 动作。
    /// 输出/副作用：展示文本；不修改状态。
    /// 复杂度：时间 O(绑定数)，空间 O(绑定数)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Return the display text of an action's bindings, or "未绑定" when unbound.
    /// Inputs: `action`: the action.
    /// Outputs / side effects: display text; no state change.
    /// Complexity: O(bindings) time and space.
    pub fn shortcut_label(&self, action: ShortcutAction) -> String {
        let labels: Vec<String> = self
            .shortcut_bindings(action)
            .iter()
            .map(|binding| binding.label())
            .collect();
        if labels.is_empty() {
            "未绑定".into()
        } else {
            labels.join(" / ")
        }
    }

    /// 所属模块：app / 可配置快捷键
    /// 功能：判断动作是否在本帧被按下。
    /// 输入：
    /// - `ctx`：egui 上下文。
    /// - `action`：动作。
    /// 输出/副作用：返回布尔值；不修改状态。
    /// 复杂度：时间 O(绑定数)，空间 O(绑定数)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Whether the action was pressed this frame.
    /// Inputs:
    /// - `ctx`: egui context.
    /// - `action`: the action.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(bindings) time and space.
    pub fn shortcut_pressed(&self, ctx: &egui::Context, action: ShortcutAction) -> bool {
        self.shortcut_bindings(action)
            .iter()
            .any(|binding| binding.matches(ctx))
    }

    /// 所属模块：app / 可配置快捷键
    /// 功能：写入动作的绑定；与默认值相同时删除自定义记录。
    /// 输入：
    /// - `action`：动作。
    /// - `bindings`：新的绑定列表（空表示未绑定）。
    /// 输出/副作用：修改 `shortcuts`；不写盘。
    /// 复杂度：时间 O(绑定数)，空间 O(绑定数)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Store an action's bindings; an exact default match drops the custom entry.
    /// Inputs:
    /// - `action`: the action.
    /// - `bindings`: new bindings (empty means unbound).
    /// Outputs / side effects: mutates `shortcuts`; never writes to disk.
    /// Complexity: O(bindings) time and space.
    pub fn set_shortcut(&mut self, action: ShortcutAction, bindings: &[Binding]) {
        if bindings == action.default_bindings().as_slice() {
            self.shortcuts.remove(action.id());
            return;
        }
        let text = bindings
            .iter()
            .map(|binding| binding.text())
            .collect::<Vec<_>>()
            .join(", ");
        self.shortcuts.insert(action.id().to_string(), text);
    }

    /// 所属模块：app / 可配置快捷键
    /// 功能：恢复动作的默认绑定。
    /// 输入：`action` 动作。
    /// 输出/副作用：删除该动作的自定义记录；不写盘。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Restore the default bindings of one action.
    /// Inputs: `action`: the action.
    /// Outputs / side effects: removes the custom entry; never writes to disk.
    /// Complexity: O(1) time and space.
    pub fn reset_shortcut(&mut self, action: ShortcutAction) {
        self.shortcuts.remove(action.id());
    }

    /// 所属模块：app / 可配置快捷键
    /// 功能：恢复全部快捷键默认值。
    /// 输入：`self`。
    /// 输出/副作用：清空 `shortcuts`；不写盘。
    /// 复杂度：时间 O(自定义条数)，空间 O(1)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Restore every default shortcut.
    /// Inputs: `self`.
    /// Outputs / side effects: clears `shortcuts`; never writes to disk.
    /// Complexity: O(custom entries) time and O(1) space.
    pub fn reset_all_shortcuts(&mut self) {
        self.shortcuts.clear();
    }

    /// 所属模块：app / 可配置快捷键
    /// 功能：查找与给定绑定冲突的其它动作。
    /// 输入：
    /// - `action`：正在设置的动作。
    /// - `binding`：新绑定。
    /// 输出/副作用：冲突动作或 `None`；不修改状态。
    /// 复杂度：时间 O(动作数 × 绑定数)，空间 O(1)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Find another action already using the given binding.
    /// Inputs:
    /// - `action`: the action being edited.
    /// - `binding`: the new binding.
    /// Outputs / side effects: the conflicting action or `None`; no state change.
    /// Complexity: O(actions × bindings) time and O(1) space.
    pub fn shortcut_conflict(
        &self,
        action: ShortcutAction,
        binding: Binding,
    ) -> Option<ShortcutAction> {
        ShortcutAction::ALL
            .iter()
            .copied()
            .filter(|other| *other != action)
            .find(|other| {
                self.shortcut_bindings(*other)
                    .iter()
                    .any(|existing| *existing == binding)
            })
    }
}

/// 每次垂直滑动编曲区的距离（点）。
/// Distance scrolled vertically in the arrangement per shortcut press (points).
const SCROLL_ROW_STEP: f32 = 60.0;
/// 每次垂直滑动钢琴卷帘的音高数（半音）。
/// Number of semitones scrolled in the piano roll per shortcut press.
const SCROLL_PITCH_STEP: i32 = 2;

/// 视图滑动方向。
/// View scroll direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollDirection {
    Left,
    Right,
    Up,
    Down,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：绑定文本（持久化格式）可无损往返解析。
    /// Covers: binding text (the persistence format) round-trips through the parser.
    #[test]
    fn binding_text_round_trips() {
        let cases = [
            Binding::new(egui::Key::Space, false, false, false),
            Binding::new(egui::Key::S, true, false, false),
            Binding::new(egui::Key::Z, true, true, false),
            Binding::new(egui::Key::ArrowLeft, false, false, false),
            Binding::new(egui::Key::F7, false, false, false),
        ];
        for binding in cases {
            assert_eq!(
                Binding::parse(&binding.text()),
                Some(binding),
                "text={}",
                binding.text()
            );
        }
    }

    /// 覆盖：默认快捷键之间不存在重复绑定。
    /// Covers: the default shortcuts contain no duplicate bindings.
    #[test]
    fn default_shortcuts_have_no_conflicts() {
        let settings = AppSettings::default();
        for action in ShortcutAction::ALL {
            for binding in settings.shortcut_bindings(action) {
                assert!(
                    settings.shortcut_conflict(action, binding).is_none(),
                    "{} conflicts on {}",
                    action.label(),
                    binding.label()
                );
            }
        }
    }

    /// 覆盖：自定义快捷键随设置 JSON 往返后仍然生效。
    /// Covers: a custom shortcut survives a settings JSON round-trip.
    #[test]
    fn settings_shortcuts_round_trip_through_json() {
        let mut settings = AppSettings::default();
        let custom = Binding::new(egui::Key::K, true, false, true);
        settings.set_shortcut(ShortcutAction::Save, &[custom]);
        let text = serde_json::to_string(&settings).expect("serialize settings");
        let restored: AppSettings = serde_json::from_str(&text).expect("deserialize settings");
        assert_eq!(
            restored.shortcut_bindings(ShortcutAction::Save),
            vec![custom]
        );
    }

    /// 覆盖：自定义绑定覆盖默认值，清除后恢复默认。
    /// Covers: a custom binding overrides the default and clearing restores it.
    #[test]
    fn custom_shortcut_overrides_and_clears() {
        let mut settings = AppSettings::default();
        let custom = Binding::new(egui::Key::K, true, false, true);
        settings.set_shortcut(ShortcutAction::Save, &[custom]);
        assert_eq!(
            settings.shortcut_bindings(ShortcutAction::Save),
            vec![custom]
        );
        settings.set_shortcut(ShortcutAction::Save, &[]);
        assert!(settings.shortcut_bindings(ShortcutAction::Save).is_empty());
        settings.reset_shortcut(ShortcutAction::Save);
        assert_eq!(
            settings.shortcut_bindings(ShortcutAction::Save),
            ShortcutAction::Save.default_bindings()
        );
    }
}

impl SurfingApp {
    /// 所属模块：app / 可配置快捷键
    /// 功能：按方向滑动当前编辑视图：编曲区/控制器的时间轴，或钢琴卷帘的音高与时间。
    /// 输入：`direction` 滑动方向。
    /// 输出/副作用：修改 `view_start` / `piano_view_beat` / `arr_scroll` / `piano_top_pitch`；
    /// 不修改工程内容。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / configurable shortcuts
    /// Purpose: Scroll the current editor in one direction: the arrangement/controller timeline or
    /// the piano roll's time and pitch axes.
    /// Inputs: `direction`: scroll direction.
    /// Outputs / side effects: updates `view_start` / `piano_view_beat` / `arr_scroll` /
    /// `piano_top_pitch`; project content is untouched.
    /// Complexity: O(1) time and space.
    pub(super) fn scroll_view(&mut self, direction: ScrollDirection) {
        let piano_active = self.bottom_open
            && self.piano_track.is_some()
            && matches!(self.bottom_view, BottomView::PianoRoll);
        match direction {
            ScrollDirection::Left | ScrollDirection::Right => {
                let bar = self.project.beats_per_bar().max(0.25);
                let delta = if direction == ScrollDirection::Right {
                    bar
                } else {
                    -bar
                };
                if piano_active {
                    self.piano_view_beat = (self.piano_view_beat + delta).max(0.0);
                } else {
                    self.view_start = (self.view_start + delta).max(0.0);
                }
            }
            ScrollDirection::Up | ScrollDirection::Down => {
                let sign = if direction == ScrollDirection::Down {
                    1.0
                } else {
                    -1.0
                };
                if piano_active {
                    self.piano_top_pitch = self.piano_top_pitch + sign as i32 * SCROLL_PITCH_STEP;
                    self.piano_top_pitch = self.piano_top_pitch.clamp(24, 120);
                } else if !self.bottom_open
                    || !matches!(self.bottom_view, BottomView::Controller { .. })
                {
                    self.arr_scroll = (self.arr_scroll + sign * SCROLL_ROW_STEP).max(0.0);
                }
            }
        }
    }
}
