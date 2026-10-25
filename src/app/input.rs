//! 所属模块：app / 输入
//! 文件用途：
//! - 处理主窗口与独立窗口的键盘快捷键、虚拟键盘试听与 MIDI 输入事件。
//! - 主页视图下屏蔽编辑/播放快捷键，避免误改后台工程。
//!
//! 函数清单：
//! - `handle_keyboard`：可配置快捷键、撤销、复制粘贴、试听与空格播放/暂停。
//! - `handle_controller_tool_shortcuts`：控制器画笔的循环/直选快捷键。
//! - `cancel_or_close`：取消/关闭当前弹窗或卷帘。
//! - `poll_midi`：消费 MIDI 输入队列。
//! - `keyboard_map`：电脑键盘到 MIDI 音高映射。
//!
//! Module: app / input
//! Purpose:
//! - Handle keyboard shortcuts, virtual-keyboard audition and MIDI input events for the main and
//!   standalone windows.
//! - Block editing/transport shortcuts on the home page so the background project is not changed
//!   by accident.
//!
//! Functions:
//! - `handle_keyboard`: configurable shortcuts, undo, copy/paste, audition and Space play/pause.
//! - `handle_controller_tool_shortcuts`: cycle/select controller brush shortcuts.
//! - `cancel_or_close`: close the current popup or roll.
//! - `poll_midi`: consume the MIDI input queue.
//! - `keyboard_map`: map computer keys to MIDI pitches.

use super::*;

impl SurfingApp {
    // ----- input handling -------------------------------------------------

    /// 所属模块：app / 输入
    /// 功能：处理键盘快捷键与试听：撤销/重做、复制粘贴、空格播放、Delete、文件快捷键、
    /// 鼠标工具与电脑键盘试听；主页视图直接返回。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：可能修改工程、播放状态、鼠标模式、试听音符与状态栏。
    /// 复杂度：时间 O(按键数 + 电脑键盘映射数)，空间 O(1)。
    ///
    /// Module: app / input
    /// Purpose: Handle keyboard shortcuts and audition: undo/redo, copy/paste, Space playback,
    /// Delete, file shortcuts, pointer tools and computer-keyboard audition; returns early on the
    /// home page.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: may change the project, transport, pointer mode, auditioned notes and
    /// the status bar.
    /// Complexity: O(keys + keyboard map entries) time and O(1) space.
    pub(super) fn handle_keyboard(&mut self, ctx: &egui::Context) {
        // 主页不编辑工程：屏蔽播放、删除与编辑快捷键，避免误改后台工程。
        // The home page does not edit the project: block transport, delete and editing shortcuts so
        // the background project cannot be changed by accident.
        if self.view == WorkspaceView::Home {
            return;
        }
        let typing = ctx.egui_wants_keyboard_input();
        // 没有按键事件时跳过全部绑定匹配，避免每帧为每个动作分配默认绑定。
        // Skip binding matching when no key event arrived, so no per-frame default-binding
        // allocations are made for every action.
        let any_key = ctx.input(|i| {
            i.events
                .iter()
                .any(|event| matches!(event, egui::Event::Key { pressed: true, .. }))
        });

        // 全部快捷键都读取设置中的绑定；输入框获得焦点时让给文本编辑。
        // Every shortcut reads its binding from the settings; text fields keep their own keys.
        if !typing && any_key {
            if self.settings.shortcut_pressed(ctx, ShortcutAction::Undo) {
                self.undo();
            }
            if self.settings.shortcut_pressed(ctx, ShortcutAction::Redo) {
                self.redo();
            }
            if self.settings.shortcut_pressed(ctx, ShortcutAction::Copy) {
                self.copy_selection();
            }
            if self.settings.shortcut_pressed(ctx, ShortcutAction::Paste) {
                self.paste_clipboard();
            }
            if self.settings.shortcut_pressed(ctx, ShortcutAction::Delete) {
                self.delete_selection();
            }
            if self.settings.shortcut_pressed(ctx, ShortcutAction::Save) {
                self.save_project();
            }
            if self.settings.shortcut_pressed(ctx, ShortcutAction::Open) {
                self.open_project_dialog();
            }
            if self.settings.shortcut_pressed(ctx, ShortcutAction::New) {
                self.request_new_project();
            }
            if self.settings.shortcut_pressed(ctx, ShortcutAction::Export) {
                self.export_wav_dialog();
            }
            if self
                .settings
                .shortcut_pressed(ctx, ShortcutAction::GoToStart)
            {
                self.jump_to_start();
            }
            if self.settings.shortcut_pressed(ctx, ShortcutAction::GoToEnd) {
                self.jump_to_end();
            }
            if self.settings.shortcut_pressed(ctx, ShortcutAction::Help) {
                self.show_help = true;
            }
            if self
                .settings
                .shortcut_pressed(ctx, ShortcutAction::PluginBrowser)
            {
                self.open_browser(crate::ui::PluginTarget::Instrument);
            }
            if self
                .settings
                .shortcut_pressed(ctx, ShortcutAction::EffectsRoll)
            {
                if let Some(track_id) = self.current_track_id() {
                    self.request_roll(track_id, RollKind::Effects);
                }
            }
            if self
                .settings
                .shortcut_pressed(ctx, ShortcutAction::ControllerRoll)
            {
                if let Some(track_id) = self.current_track_id() {
                    self.request_roll(
                        track_id,
                        RollKind::Controller(crate::core::model::ControllerKind::Volume),
                    );
                }
            }
            let mode = if self
                .settings
                .shortcut_pressed(ctx, ShortcutAction::ToolSelect)
            {
                Some(MouseMode::Select)
            } else if self
                .settings
                .shortcut_pressed(ctx, ShortcutAction::ToolDraw)
            {
                Some(MouseMode::Draw)
            } else if self.settings.shortcut_pressed(ctx, ShortcutAction::ToolCut) {
                Some(MouseMode::Cut)
            } else {
                None
            };
            if let Some(mode) = mode {
                self.mouse_mode = mode;
                self.status = format!("鼠标模式：{}", mode.label());
            }
            self.handle_controller_tool_shortcuts(ctx);
            if self
                .settings
                .shortcut_pressed(ctx, ShortcutAction::ScrollLeft)
            {
                self.scroll_view(ScrollDirection::Left);
            }
            if self
                .settings
                .shortcut_pressed(ctx, ShortcutAction::ScrollRight)
            {
                self.scroll_view(ScrollDirection::Right);
            }
            if self
                .settings
                .shortcut_pressed(ctx, ShortcutAction::ScrollUp)
            {
                self.scroll_view(ScrollDirection::Up);
            }
            if self
                .settings
                .shortcut_pressed(ctx, ShortcutAction::ScrollDown)
            {
                self.scroll_view(ScrollDirection::Down);
            }
            if self.settings.shortcut_pressed(ctx, ShortcutAction::Cancel) {
                self.cancel_or_close();
            }
        }

        // 独立窗口的空格由线程消息钩子捕获，不受主视口输入焦点影响；只有最近获得焦点的
        // 工程窗口消费它，避免多个工程窗口同时响应同一个按键。
        // 浮动卷帘窗口有自己的 egui 视口输入：钩子捕获的空格被丢弃且不在父窗口处理，
        // 由卷帘视口里的 `handle_keyboard` 直接处理按键，避免同一次空格被切换两次。
        // Space in standalone windows comes from the thread message hook; only the project window
        // that was focused last consumes it, so multiple windows cannot react to one key press.
        // Floating rolls receive their own egui input: the hook flag is dropped without acting in
        // the parent frame, and the roll viewport handles the key directly, so one Space press
        // never toggles playback twice.
        let owns_standalone_space = self.shared.last_focused_window() == self.window_id
            || ctx.input(|i| i.viewport().focused.unwrap_or(false));
        let hook_space = if self.roll_window_focused {
            // 卷帘视口会直接处理按键；丢弃钩子标志，避免它在之后的帧再次触发。
            // The roll viewport handles the key itself; drop the hook flag so it cannot fire later.
            let _ = super::hotkeys::take_space_toggle();
            false
        } else if owns_standalone_space {
            super::hotkeys::take_space_toggle()
                && self
                    .settings
                    .shortcut_bindings(ShortcutAction::PlayPause)
                    .iter()
                    .any(|binding| {
                        binding.key == egui::Key::Space
                            && !binding.ctrl
                            && !binding.shift
                            && !binding.alt
                    })
        } else {
            false
        };
        let play = (!typing
            && self
                .settings
                .shortcut_pressed(ctx, ShortcutAction::PlayPause))
            || hook_space;
        if play {
            self.toggle_play();
        }

        if !typing {
            let command_held = ctx.input(|i| i.modifiers.command);
            if command_held {
                let held: Vec<u8> = self.keyboard_held.drain().map(|(_, pitch)| pitch).collect();
                for pitch in held {
                    self.audition_off(pitch);
                }
            } else {
                for (key, pitch) in keyboard_map() {
                    let down = ctx.input(|i| i.key_down(key));
                    if down {
                        if !self.keyboard_held.contains_key(&key) {
                            self.keyboard_held.insert(key, pitch);
                            self.audition_on(pitch, 100);
                        }
                    } else if let Some(pitch) = self.keyboard_held.remove(&key) {
                        self.audition_off(pitch);
                    }
                }
            }
        }
    }

    /// 所属模块：app / 输入 / 控制器工具
    /// 功能：处理控制器画笔的快捷键：直接选择画笔/直线/平滑/擦除，或按顺序切换下一个。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：可能修改 `controller_tool` 与状态栏；不修改工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / input / controller tools
    /// Purpose: Handle controller-brush shortcuts: select brush/line/smooth/erase directly or cycle
    /// to the next tool.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: may change `controller_tool` and the status bar; the project is
    /// untouched.
    /// Complexity: O(1) time and space.
    fn handle_controller_tool_shortcuts(&mut self, ctx: &egui::Context) {
        use crate::ui::controller::Tool;
        let direct = if self
            .settings
            .shortcut_pressed(ctx, ShortcutAction::ControllerBrush)
        {
            Some(Tool::Brush)
        } else if self
            .settings
            .shortcut_pressed(ctx, ShortcutAction::ControllerLine)
        {
            Some(Tool::Line)
        } else if self
            .settings
            .shortcut_pressed(ctx, ShortcutAction::ControllerSmooth)
        {
            Some(Tool::Smooth)
        } else if self
            .settings
            .shortcut_pressed(ctx, ShortcutAction::ControllerErase)
        {
            Some(Tool::Erase)
        } else {
            None
        };
        if let Some(tool) = direct {
            self.controller_tool = tool;
            self.status = format!("控制器工具：{}", tool.label());
            return;
        }
        if self
            .settings
            .shortcut_pressed(ctx, ShortcutAction::ControllerToolNext)
        {
            self.controller_tool = match self.controller_tool {
                Tool::Brush => Tool::Line,
                Tool::Line => Tool::Smooth,
                Tool::Smooth => Tool::Erase,
                Tool::Erase => Tool::Brush,
            };
            self.status = format!("控制器工具：{}", self.controller_tool.label());
        }
    }

    /// 所属模块：app / 输入 / 取消
    /// 功能：处理“取消 / 关闭”快捷键：依次关闭乐器窗口、独立插件窗口或底部卷帘。
    /// 输入：`self`。
    /// 输出/副作用：可能关闭窗口/卷帘并更新状态栏；不修改工程内容。
    /// 复杂度：时间 O(编辑器数量)，空间 O(编辑器数量)。
    ///
    /// Module: app / input / cancel
    /// Purpose: Handle the "cancel / close" shortcut: close the instrument window, standalone plugin
    /// windows, or the bottom roll, in that order.
    /// Inputs: `self`.
    /// Outputs / side effects: may close windows/rolls and update the status bar; project content is
    /// untouched.
    /// Complexity: O(editors) time and space.
    fn cancel_or_close(&mut self) {
        // 内嵌界面随卷帘存在，Esc 只关闭独立窗口；没有独立窗口时关闭底部卷帘。
        // Inline editors live with the roll, so Esc only closes standalone windows;
        // without one it closes the bottom roll.
        let standalone: Vec<_> = self
            .editors
            .iter()
            .filter(|(_, window)| window.is_standalone() && window.is_visible())
            .map(|(key, _)| *key)
            .collect();
        if self.instrument_picker.is_some() {
            // 乐器窗口是普通弹窗，Esc 先关闭它。
            // The instrument window is a regular popup, so Esc closes it first.
            self.instrument_picker = None;
            self.status = "已关闭乐器窗口".into();
        } else if !standalone.is_empty() {
            for (track_id, slot) in standalone {
                self.close_editor(track_id, slot);
            }
            self.status = "已关闭插件窗口".into();
        } else if self.bottom_open {
            self.bottom_open = false;
        }
    }

    pub(super) fn poll_midi(&mut self) {
        // 主页不发声；多个工程窗口只让最近获得焦点的窗口消费 MIDI，避免重复触发音符。
        // The home page makes no sound; among project windows only the one focused last consumes
        // MIDI so notes are not triggered twice.
        if self.view != WorkspaceView::Studio || self.shared.last_focused_window() != self.window_id
        {
            return;
        }
        let events = self.shared.midi_events();
        if events.is_empty() {
            return;
        }
        for event in events {
            if event.on {
                self.audition_on(event.pitch, event.velocity);
            } else {
                self.audition_off(event.pitch);
            }
        }
    }
}

fn keyboard_map() -> Vec<(egui::Key, u8)> {
    use egui::Key;
    vec![
        (Key::Z, 48),
        (Key::S, 49),
        (Key::X, 50),
        (Key::D, 51),
        (Key::C, 52),
        (Key::V, 53),
        (Key::G, 54),
        (Key::B, 55),
        (Key::H, 56),
        (Key::N, 57),
        (Key::J, 58),
        (Key::M, 59),
        (Key::Q, 60),
        (Key::W, 62),
        (Key::E, 64),
        (Key::R, 65),
        (Key::Num5, 66),
        (Key::T, 67),
        (Key::Num6, 68),
        (Key::Y, 69),
        (Key::Num7, 70),
        (Key::U, 71),
        (Key::I, 72),
    ]
}
