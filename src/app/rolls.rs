//! Floating editor roll windows and their drag-out gesture.

use super::*;

/// 浮动卷帘窗口内容的内边距（逻辑点）：独立窗口没有底部面板的外边距。
/// Inner margin of a detached roll window in points; standalone windows have no panel padding.
const DETACHED_ROLL_MARGIN: i8 = 8;

impl SurfingApp {
    // ----- floating editor rolls -----------------------------------------

    pub fn has_roll_window(&self, track_id: TrackId) -> bool {
        self.roll_windows
            .iter()
            .any(|window| window.track_id == track_id)
    }

    /// Detach an editor roll of `track_id` into a floating window. Reuses the
    /// existing window instead of opening a duplicate and switches its content
    /// when another roll of the same track is requested.
    pub fn open_roll_window(&mut self, track_id: TrackId, kind: RollKind) {
        if !crate::platform::caps().multi_window {
            // 单窗口平台（安卓）没有独立窗口：卷帘直接显示在底部编辑区。
            // Single-window platforms (Android) have no standalone windows: show the roll in the
            // bottom editor instead.
            self.open_roll_in_bottom(track_id, kind);
            self.status = "当前系统在底部编辑区打开卷帘".into();
            return;
        }
        if let Some(window) = self
            .roll_windows
            .iter_mut()
            .find(|window| window.track_id == track_id)
        {
            window.kind = kind;
            window.bring_to_front = true;
        } else {
            self.roll_windows.push(RollWindow {
                track_id,
                kind,
                bring_to_front: true,
            });
        }
        self.status = "已打开独立编辑卷帘窗口".into();
    }

    fn open_roll_in_bottom(&mut self, track_id: TrackId, kind: RollKind) {
        match kind {
            RollKind::PianoRoll => {
                self.piano_track = Some(track_id);
                self.bottom_view = BottomView::PianoRoll;
            }
            RollKind::Controller(controller) => {
                self.selected_controller = Some((track_id, controller));
                self.selected_effects_lane = None;
                self.bottom_view = BottomView::Controller {
                    track_id,
                    kind: controller,
                };
            }
            RollKind::Effects => {
                self.selected_controller = None;
                self.selected_effects_lane = Some(track_id);
                self.bottom_view = BottomView::Effects { track_id };
            }
        }
        self.bottom_open = true;
    }

    /// Open a track roll. An already detached window is focused instead of a
    /// second editor being opened for the same track.
    pub fn request_roll(&mut self, track_id: TrackId, kind: RollKind) {
        if self.has_roll_window(track_id) {
            self.open_roll_window(track_id, kind);
            self.bottom_open = false;
            self.status = "已切换到独立编辑卷帘窗口".into();
        } else {
            self.open_roll_in_bottom(track_id, kind);
        }
    }

    /// Double-click entry point. Brings an existing floating roll to the front;
    /// otherwise opens the bottom piano roll.
    pub fn open_track_editor(&mut self, track_index: usize, clip_index: Option<usize>) {
        let Some(track_id) = self.project.tracks.get(track_index).map(|track| track.id) else {
            return;
        };
        self.selected_track = track_index;
        self.piano_track = Some(track_id);
        if clip_index.is_some() {
            self.selected_clip = clip_index;
            self.focus_piano_on_selection();
        }
        if self.has_roll_window(track_id) {
            if matches!(self.bottom_view, BottomView::PianoRoll) {
                self.bottom_open = false;
            }
            // Requirement: double-clicking the track raises its existing window
            // instead of opening another one.
            self.open_roll_window(track_id, RollKind::PianoRoll);
            self.bottom_open = false;
            return;
        }
        self.open_roll_in_bottom(track_id, RollKind::PianoRoll);
    }

    /// Begin pulling a roll out of the bottom panel by its toolbar. The actual
    /// window is created after a small movement threshold and then follows the
    /// pointer (see [`Self::update_roll_drag`]).
    pub fn begin_roll_drag(
        &mut self,
        ui: &egui::Ui,
        track_id: TrackId,
        kind: RollKind,
        panel_rect: egui::Rect,
    ) {
        let Some(pointer) = ui.input(|i| {
            i.pointer
                .interact_pos()
                .or_else(|| i.pointer.press_origin())
        }) else {
            return;
        };
        let (origin, decoration) = ui.input(|i| {
            let viewport = i.viewport();
            let origin = viewport
                .inner_rect
                .map(|rect| rect.min)
                .unwrap_or(egui::Pos2::ZERO);
            let decoration = match (viewport.outer_rect, viewport.inner_rect) {
                (Some(outer), Some(inner)) => outer.min - inner.min,
                _ => egui::Vec2::ZERO,
            };
            (origin, decoration)
        });
        let grab = pointer - panel_rect.min;
        let desired = origin + (pointer - grab).to_vec2() - decoration;
        self.roll_drag = Some(RollDragState {
            track_id,
            kind,
            press_pos: pointer,
            grab,
            size: egui::vec2(
                panel_rect.width().max(420.0),
                panel_rect.height().max(280.0),
            ),
            source_origin: origin,
            decoration,
            started: false,
            desired,
        });
    }

    /// Advance an active toolbar drag: create the floating window once the
    /// pointer moved far enough and make the window follow the mouse.
    pub fn update_roll_drag(&mut self, ctx: &egui::Context) {
        let Some(mut drag) = self.roll_drag.take() else {
            return;
        };
        let (down, pointer) = ctx.input(|i| (i.pointer.primary_down(), i.pointer.interact_pos()));
        let Some(pointer) = pointer else {
            if down {
                self.roll_drag = Some(drag);
            }
            return;
        };
        drag.desired = drag.source_origin + (pointer - drag.grab).to_vec2() - drag.decoration;
        if !drag.started && (pointer - drag.press_pos).length() >= 8.0 {
            self.open_roll_window(drag.track_id, drag.kind);
            if let Some(window) = self
                .roll_windows
                .iter_mut()
                .find(|window| window.track_id == drag.track_id)
            {
                // The drag owns the pointer; focusing the new window would
                // interrupt it.
                window.bring_to_front = false;
            }
            self.bottom_open = false;
            drag.started = true;
        }
        if down {
            self.roll_drag = Some(drag);
        }
    }

    /// Make the whole roll toolbar strip draggable, except the actual controls.
    ///
    /// `bar` is the entire toolbar row. `widgets` are the rectangles of the
    /// interactive controls on it (buttons, sliders, combo boxes, the title
    /// drag handle). Any other point on the strip — the empty space before,
    /// between and after the controls — starts pulling the roll out into its
    /// own window. This is checked against the pointer directly instead of
    /// registering an overlapping widget, so the controls keep their own
    /// clicks and drags.
    pub fn roll_toolbar_drag(
        &mut self,
        ui: &egui::Ui,
        bar: egui::Rect,
        widgets: &[egui::Rect],
        track_id: Option<TrackId>,
        kind: RollKind,
        panel_rect: egui::Rect,
        enabled: bool,
    ) {
        let Some(track_id) = track_id else {
            return;
        };
        if !enabled || self.roll_drag.is_some() {
            return;
        }
        let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) else {
            return;
        };
        let over_control = widgets.iter().any(|rect| rect.contains(pointer));
        if !bar.contains(pointer) || over_control {
            return;
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        if ui.input(|i| i.pointer.primary_pressed()) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            self.begin_roll_drag(ui, track_id, kind, panel_rect);
        }
    }

    /// Detach the roll currently shown by an editor canvas into its own OS
    /// window. The gesture is a long press (>= 0.45 s) followed by a drag, so
    /// normal note/point editing still works.
    pub fn try_detach_roll(
        &mut self,
        ui: &egui::Ui,
        rect: egui::Rect,
        track_id: TrackId,
        kind: RollKind,
        press_on_item: bool,
    ) -> bool {
        if press_on_item || !crate::platform::caps().multi_window {
            // 单窗口平台没有可拖出的独立窗口，保留底部编辑区。
            // Single-window platforms cannot drag a roll out, so the bottom editor stays.
            return false;
        }
        let (down, origin, start_time, now, pointer) = ui.input(|i| {
            (
                i.pointer.primary_down(),
                i.pointer.press_origin(),
                i.pointer.press_start_time(),
                i.time,
                i.pointer.interact_pos(),
            )
        });
        let (Some(origin), Some(pointer), Some(start_time)) = (origin, pointer, start_time) else {
            return false;
        };
        if !down || !rect.contains(origin) {
            return false;
        }
        let held = now - start_time >= 0.45;
        let moved = (pointer - origin).length() >= 36.0;
        if !(held && moved) {
            return false;
        }
        self.open_roll_window(track_id, kind);
        self.bottom_open = false;
        self.status = "已拖出独立编辑卷帘窗口".into();
        true
    }

    pub(super) fn ui_roll_windows(&mut self, ctx: &egui::Context) {
        // 每帧先清空焦点归属：没有任何浮动卷帘时，空格仍交回工程窗口/消息钩子处理。
        // Reset the ownership flag every frame: with no floating roll, Space belongs to the
        // project window / message hook again.
        self.roll_window_focused = false;
        if self.roll_windows.is_empty() {
            return;
        }
        self.roll_windows.retain(|window| {
            self.project
                .tracks
                .iter()
                .any(|track| track.id == window.track_id)
        });
        if self.roll_windows.is_empty() {
            return;
        }
        let mut windows = std::mem::take(&mut self.roll_windows);
        // Render the focused window last so it is raised above the others.
        windows.sort_by_key(|window| window.bring_to_front);
        let mut closed: Vec<TrackId> = Vec::new();
        for window in &mut windows {
            let Some(track_index) = self
                .project
                .tracks
                .iter()
                .position(|track| track.id == window.track_id)
            else {
                continue;
            };
            let track_id = window.track_id;
            let track_name = self.project.tracks[track_index].name.clone();
            let kind = window.kind;
            // 视口 id 必须包含窗口编号：不同工程窗口可以同时打开同一 track id 的卷帘。
            // The viewport id must include the window id: two project windows may open a roll for
            // the same track id at the same time.
            let viewport_id =
                egui::ViewportId::from_hash_of(("roll_window", self.window_id, track_id));
            if window.bring_to_front {
                window.bring_to_front = false;
                // A minimized window must be restored before it can be raised.
                ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Focus);
            }
            let title = match kind {
                RollKind::PianoRoll => format!("钢琴卷帘 · {track_name}"),
                RollKind::Controller(controller) => {
                    format!("控制器 {} · {track_name}", controller.label())
                }
                RollKind::Effects => format!("效果器链 · {track_name}"),
            };
            let saved_track = self.selected_track;
            self.selected_track = track_index;
            let mut activating = false;
            let mut close_requested = false;
            let drag = self
                .roll_drag
                .filter(|drag| drag.started && drag.track_id == track_id);
            let mut builder = egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size(match drag {
                    Some(drag) => [drag.size.x, drag.size.y],
                    None => [880.0, 520.0],
                })
                .with_min_inner_size([420.0, 280.0])
                .with_resizable(true)
                .with_decorations(true)
                .with_close_button(true)
                .with_minimize_button(true)
                .with_maximize_button(true);
            if let Some(drag) = drag {
                builder = builder.with_position(drag.desired);
            }
            ctx.show_viewport_immediate(viewport_id, builder, |ui, _class| {
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    close_requested = true;
                }
                if ui.ctx().input(|i| i.pointer.any_pressed()) {
                    activating = true;
                }
                // 浮动卷帘获得焦点时把本工程记为“最近焦点”：消息钩子捕获的空格与 MIDI
                // 输入都归属这个工程窗口，而不是主页或其它工程。
                // When the floating roll is focused, mark this project as the most recently focused
                // one so hook-captured Space and MIDI input belong to this project.
                if ui.ctx().input(|i| i.viewport().focused.unwrap_or(false)) {
                    self.roll_window_focused = true;
                    self.shared.set_last_focused_window(self.window_id);
                }
                // The child is a real OS window, so it receives its own
                // keyboard events; run the same shortcut handling here.
                self.handle_keyboard(ui.ctx());
                ui.painter()
                    .rect_filled(ui.max_rect(), egui::CornerRadius::from(0), theme::BG);
                // 独立窗口没有底部面板的内边距；补一圈外边距，避免工具条标题与圆角贴住
                // 窗口边缘而被裁切。
                // The detached window has no bottom-panel padding; add an outer margin so the
                // toolbar title and rounded corners never touch the window edge.
                egui::Frame::NONE
                    .inner_margin(egui::Margin::same(DETACHED_ROLL_MARGIN))
                    .show(ui, |ui| match kind {
                        RollKind::PianoRoll => self.ui_piano_roll_for(ui, track_id, true),
                        RollKind::Controller(controller) => {
                            self.ui_controller_lane(ui, track_id, controller, true)
                        }
                        RollKind::Effects => self.ui_effects(ui, track_id, true),
                    });
            });
            if let Some(drag) = drag {
                // Keep the detached roll under the pointer while the toolbar is
                // being dragged out.
                ctx.send_viewport_cmd_to(
                    viewport_id,
                    egui::ViewportCommand::OuterPosition(drag.desired),
                );
            }
            self.selected_track = if activating { track_index } else { saved_track };
            if activating {
                // Clicking a window raises it above the other rolls.
                window.bring_to_front = true;
            }
            if close_requested {
                closed.push(track_id);
            }
        }
        windows.retain(|window| !closed.contains(&window.track_id));
        self.roll_windows = windows;
    }
}
