//! Controller-lane editor (volume / pan / sustain / pitch bend).

use crate::app::{RollKind, SurfingApp};
use crate::core::model::{evaluate_automation, AutomationPoint, ControllerKind};
use crate::ui::theme;
use crate::ui::MouseMode;
use eframe::egui::{self, Align2, CornerRadius, FontId, Sense, Stroke, StrokeKind, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Brush,
    Line,
    Smooth,
    Erase,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Brush => "画笔",
            Tool::Line => "直线",
            Tool::Smooth => "平滑",
            Tool::Erase => "擦除",
        }
    }
}

impl SurfingApp {
    pub fn ui_controller_lane(
        &mut self,
        ui: &mut egui::Ui,
        track_id: u64,
        kind: ControllerKind,
        detached: bool,
    ) {
        let is_master = track_id == crate::core::model::MASTER_TRACK_ID;
        let track_index = if is_master {
            None
        } else {
            self.project.tracks.iter().position(|t| t.id == track_id)
        };
        if !is_master && track_index.is_none() {
            return;
        }
        let track_index = track_index.unwrap_or(0);
        let tool = self.controller_tool;
        let line_start = self.controller_line_start.clone();
        let mode = if ui.input(|i| i.modifiers.shift) {
            MouseMode::Draw
        } else {
            self.mouse_mode
        };
        let default_value = if is_master {
            (self.project.master_volume / 1.5).clamp(0.0, 1.0)
        } else {
            kind.center()
        };
        let playhead = self.playhead();
        let current_value = if is_master {
            evaluate_automation(self.project.master_controllers.get(kind), playhead)
                .unwrap_or(default_value)
        } else {
            self.project.tracks[track_index]
                .controllers
                .value_at(kind, playhead)
                .unwrap_or(default_value)
        };
        let value_text = if is_master {
            format!("{:.0}%", current_value * 150.0)
        } else {
            kind.display_value(current_value)
        };
        let supported = is_master || self.project.tracks[track_index].supports_controller(kind);
        let title_color = if !supported {
            theme::TEXT_FAINT
        } else if is_master {
            theme::ACCENT
        } else {
            theme::track_color(self.project.tracks[track_index].color)
        };
        let panel_rect = ui.available_rect_before_wrap();
        let mut controls: Vec<egui::Rect> = Vec::new();
        let row = ui
            .horizontal(|ui| {
                // Effective lanes use the main-track color by default; lanes that
                // cannot affect the current content are grayed out and struck
                // through.
                let mut title = egui::RichText::new(if is_master {
                    format!("主控 · {}", kind.label())
                } else {
                    format!("⠿ 控制器 · {}", kind.label())
                })
                .color(title_color)
                .strong()
                .size(13.0);
                if !supported {
                    title = title.strikethrough();
                }
                let title_response =
                    ui.add(egui::Label::new(title).sense(egui::Sense::click_and_drag()));
                controls.push(title_response.rect);
                if !detached && !is_master && title_response.drag_started() {
                    self.begin_roll_drag(ui, track_id, RollKind::Controller(kind), panel_rect);
                }
                title_response.on_hover_text("拖动工具条可把卷帘拖出为独立窗口");
                ui.separator();
                for value in [Tool::Brush, Tool::Line, Tool::Smooth, Tool::Erase] {
                    let tool = ui.selectable_label(self.controller_tool == value, value.label());
                    controls.push(tool.rect);
                    if tool.clicked() {
                        self.controller_tool = value;
                    }
                }
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !detached {
                        let close = ui.small_button("×").on_hover_text("关闭卷帘");
                        controls.push(close.rect);
                        if close.clicked() {
                            self.bottom_open = false;
                        }
                    }
                    let clear = ui.button("清空");
                    controls.push(clear.rect);
                    if clear.clicked() {
                        if let Some(lane) = self.controller_lane_mut(track_id, kind) {
                            lane.clear();
                        }
                        self.mark_dirty();
                    }
                    ui.label(
                        egui::RichText::new(value_text)
                            .size(11.0)
                            .color(theme::TEXT_DIM),
                    );
                });
            })
            .response;
        self.roll_toolbar_drag(
            ui,
            row.rect,
            &controls,
            Some(track_id),
            RollKind::Controller(kind),
            panel_rect,
            !detached && !is_master,
        );
        ui.add_space(4.0);

        let available = ui.available_size();
        if available.y < 40.0 || available.x < 80.0 {
            return;
        }
        let (response, painter) = ui.allocate_painter(available, Sense::click_and_drag());
        let rect = response.rect;
        let inner = rect.shrink2(Vec2::new(8.0, 8.0));
        painter.rect_filled(rect, CornerRadius::from(6), theme::BG_DEEP);
        painter.rect_stroke(
            rect,
            CornerRadius::from(6),
            Stroke::new(1.0_f32, theme::LINE),
            StrokeKind::Inside,
        );

        let visible_beats = self.visible_beats.max(4.0);
        let view_start = self.view_start;
        let px_per_beat = inner.width() as f64 / visible_beats;
        let beat_to_x = |beat: f64| inner.left() + ((beat - view_start) * px_per_beat) as f32;
        let x_to_beat = |x: f32| view_start + (x - inner.left()) as f64 / px_per_beat;
        let value_to_y = |value: f32| inner.bottom() - value.clamp(0.0, 1.0) * inner.height();

        // Grid.
        let beats_per_bar = self.project.beats_per_bar();
        let first = view_start.floor() as i64;
        let last = (view_start + visible_beats).ceil() as i64 + 1;
        for beat in first..last {
            let x = beat_to_x(beat as f64);
            let is_bar = (beat as f64 % beats_per_bar).abs() < 1e-6;
            painter.vline(
                x,
                inner.top()..=inner.bottom(),
                Stroke::new(
                    1.0_f32,
                    if is_bar {
                        theme::with_alpha(theme::LINE, 220)
                    } else {
                        theme::with_alpha(theme::LINE, 100)
                    },
                ),
            );
            if is_bar {
                painter.text(
                    egui::pos2(x + 3.0, inner.top() + 3.0),
                    Align2::LEFT_TOP,
                    format!("{}", beat / beats_per_bar.max(1.0) as i64),
                    FontId::monospace(9.0),
                    theme::TEXT_DIM,
                );
            }
        }
        // Centre guide for every controller lane.
        {
            let y = value_to_y(0.5);
            let mut x = inner.left();
            while x < inner.right() {
                let end = (x + 6.0).min(inner.right());
                painter.hline(
                    x..=end,
                    y,
                    Stroke::new(1.0_f32, theme::with_alpha(theme::ACCENT, 80)),
                );
                x += 12.0;
            }
        }

        let (points, color) = if is_master {
            (
                self.project.master_controllers.get(kind).to_vec(),
                theme::ACCENT,
            )
        } else {
            (
                self.project.tracks[track_index]
                    .controllers
                    .get(kind)
                    .to_vec(),
                theme::track_color(self.project.tracks[track_index].color),
            )
        };

        // Long-press and drag blank canvas space to detach the roll into its
        // own window. Pressing an existing point keeps the normal edit gesture.
        if !detached && !is_master {
            let press_near_point = ui
                .input(|i| i.pointer.press_origin())
                .map(|origin| {
                    points.iter().any(|point| {
                        (beat_to_x(point.beat) - origin.x).abs() <= 10.0
                            && (value_to_y(point.value) - origin.y).abs() <= 10.0
                    })
                })
                .unwrap_or(false);
            if self.try_detach_roll(
                ui,
                inner,
                track_id,
                RollKind::Controller(kind),
                press_near_point,
            ) {
                return;
            }
        }

        if points.is_empty() {
            // No automation points: render the controller's default value.
            let y = value_to_y(default_value);
            painter.line_segment(
                [egui::pos2(inner.left(), y), egui::pos2(inner.right(), y)],
                Stroke::new(2.0_f32, theme::with_alpha(color, 190)),
            );
        }
        if points.len() >= 2 {
            for pair in points.windows(2) {
                if kind == ControllerKind::Sustain {
                    let y0 = value_to_y(pair[0].value);
                    let y1 = value_to_y(pair[1].value);
                    painter.hline(
                        beat_to_x(pair[0].beat)..=beat_to_x(pair[1].beat),
                        y0,
                        Stroke::new(2.0_f32, color),
                    );
                    if (y0 - y1).abs() > 0.5 {
                        painter.vline(
                            beat_to_x(pair[1].beat),
                            y0..=y1,
                            Stroke::new(2.0_f32, color),
                        );
                    }
                } else {
                    painter.line_segment(
                        [
                            egui::pos2(beat_to_x(pair[0].beat), value_to_y(pair[0].value)),
                            egui::pos2(beat_to_x(pair[1].beat), value_to_y(pair[1].value)),
                        ],
                        Stroke::new(2.0_f32, color),
                    );
                }
            }
        }
        // Regions before the first and after the last point hold the edge value.
        if let (Some(first), Some(last)) = (points.first(), points.last()) {
            let first_x = beat_to_x(first.beat);
            if first_x > inner.left() + 0.5 {
                painter.hline(
                    inner.left()..=first_x,
                    value_to_y(first.value),
                    Stroke::new(2.0_f32, color),
                );
            }
            let last_x = beat_to_x(last.beat);
            if last_x < inner.right() - 0.5 {
                painter.hline(
                    last_x..=inner.right(),
                    value_to_y(last.value),
                    Stroke::new(2.0_f32, color),
                );
            }
        }
        for (index, point) in points.iter().enumerate() {
            let center = egui::pos2(beat_to_x(point.beat), value_to_y(point.value));
            if self.controller_selection.contains(&index) {
                painter.circle_stroke(center, 6.0, Stroke::new(1.6_f32, egui::Color32::WHITE));
            }
            painter.circle_filled(
                center,
                if self.controller_selection.contains(&index) {
                    4.0
                } else {
                    3.0
                },
                color,
            );
        }

        let playhead = self.playhead();
        let playhead_x = beat_to_x(playhead);
        if playhead_x >= inner.left() && playhead_x <= inner.right() {
            painter.line_segment(
                [
                    egui::pos2(playhead_x, inner.top()),
                    egui::pos2(playhead_x, inner.bottom()),
                ],
                Stroke::new(1.5_f32, theme::ACCENT),
            );
        }

        let grid = self.snap_beats.max(0.0625);
        let pointer_to_point = |pos: egui::Pos2| {
            let raw = ((inner.bottom() - pos.y) / inner.height()).clamp(0.0, 1.0);
            AutomationPoint {
                beat: (x_to_beat(pos.x) / grid).round() * grid,
                value: if kind == ControllerKind::Sustain {
                    if raw >= 0.5 {
                        1.0
                    } else {
                        0.0
                    }
                } else {
                    raw
                },
            }
        };

        if mode != MouseMode::Cut {
            if response.drag_started() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let near_point = points.iter().any(|point| {
                        (beat_to_x(point.beat) - pos.x).abs() <= 8.0
                            && (value_to_y(point.value) - pos.y).abs() <= 8.0
                    });
                    let marquee = mode == MouseMode::Select
                        && tool == Tool::Brush
                        && !near_point
                        && inner.contains(pos);
                    if marquee {
                        self.controller_marquee_start = Some(pos);
                        self.controller_selection.clear();
                    } else {
                        match tool {
                            Tool::Line => {
                                self.controller_line_start = Some(pointer_to_point(pos));
                            }
                            Tool::Smooth => {
                                if let Some(lane) = self.controller_lane_mut(track_id, kind) {
                                    smooth_lane(lane);
                                }
                                self.mark_dirty();
                            }
                            Tool::Erase => {
                                let x = x_to_beat(pos.x);
                                if let Some(lane) = self.controller_lane_mut(track_id, kind) {
                                    erase_near(lane, x, 0.35);
                                }
                                self.mark_dirty();
                            }
                            Tool::Brush => {
                                if let Some(lane) = self.controller_lane_mut(track_id, kind) {
                                    draw_point(lane, pointer_to_point(pos));
                                }
                                self.mark_dirty();
                            }
                        }
                    }
                }
            } else if response.dragged() {
                if let Some(start) = self.controller_marquee_start {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let band = egui::Rect::from_two_pos(start, pos).expand(3.0);
                        self.controller_selection = points
                            .iter()
                            .enumerate()
                            .filter(|(_, point)| {
                                band.contains(egui::pos2(
                                    beat_to_x(point.beat),
                                    value_to_y(point.value),
                                ))
                            })
                            .map(|(index, _)| index)
                            .collect();
                        let step = self.visible_beats * 0.02;
                        if pos.x < inner.left() + 20.0 {
                            self.view_start = (self.view_start - step).max(0.0);
                            ui.ctx().request_repaint();
                        } else if pos.x > inner.right() - 20.0 {
                            self.view_start += step;
                            ui.ctx().request_repaint();
                        }
                    }
                } else if let Some(pos) = response.interact_pointer_pos() {
                    match tool {
                        Tool::Line => {
                            if let Some(start) = line_start.clone() {
                                let end = pointer_to_point(pos);
                                if let Some(lane) = self.controller_lane_mut(track_id, kind) {
                                    lane.clear();
                                    let steps = 24;
                                    for step in 0..=steps {
                                        let t = step as f64 / steps as f64;
                                        lane.push(AutomationPoint {
                                            beat: start.beat + (end.beat - start.beat) * t,
                                            value: start.value
                                                + (end.value - start.value) * t as f32,
                                        });
                                    }
                                }
                                self.mark_dirty();
                            }
                        }
                        Tool::Brush => {
                            if let Some(lane) = self.controller_lane_mut(track_id, kind) {
                                draw_point(lane, pointer_to_point(pos));
                            }
                            self.mark_dirty();
                        }
                        Tool::Erase => {
                            let x = x_to_beat(pos.x);
                            if let Some(lane) = self.controller_lane_mut(track_id, kind) {
                                erase_near(lane, x, 0.35);
                            }
                            self.mark_dirty();
                        }
                        Tool::Smooth => {}
                    }
                }
            }
            if response.drag_stopped() {
                self.controller_line_start = None;
                self.controller_marquee_start = None;
            }

            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if inner.contains(pos) {
                        if mode == MouseMode::Select {
                            // Select mode: a click seeks the playhead.
                            self.seek(x_to_beat(pos.x).max(0.0));
                            self.controller_selection.clear();
                        } else if tool == Tool::Brush {
                            if let Some(lane) = self.controller_lane_mut(track_id, kind) {
                                draw_point(lane, pointer_to_point(pos));
                            }
                            self.mark_dirty();
                        }
                    }
                }
            }
        }

        // Rubber-band selection overlay.
        if let Some(start) = self.controller_marquee_start {
            if let Some(pos) = response.interact_pointer_pos() {
                let band = egui::Rect::from_two_pos(start, pos).intersect(inner);
                if band.width() > 1.0 && band.height() > 1.0 {
                    painter.rect_filled(
                        band,
                        CornerRadius::from(2),
                        theme::with_alpha(theme::ACCENT, 26),
                    );
                    painter.rect_stroke(
                        band,
                        CornerRadius::from(2),
                        Stroke::new(1.0_f32, theme::ACCENT),
                        StrokeKind::Inside,
                    );
                }
            }
        }

        let current = evaluate_automation(&points, playhead).unwrap_or(default_value);
        painter.text(
            egui::pos2(inner.right() - 4.0, inner.bottom() - 14.0),
            Align2::RIGHT_BOTTOM,
            if is_master {
                format!("{:.0}%", current * 150.0)
            } else {
                kind.display_value(current)
            },
            FontId::monospace(11.0),
            theme::TEXT,
        );
    }
}

fn draw_point(points: &mut Vec<AutomationPoint>, point: AutomationPoint) {
    erase_near(points, point.beat, 0.001);
    points.push(point);
    points.sort_by(|a, b| a.beat.total_cmp(&b.beat));
}

fn erase_near(points: &mut Vec<AutomationPoint>, beat: f64, radius: f64) {
    points.retain(|point| (point.beat - beat).abs() > radius);
}

fn smooth_lane(points: &mut Vec<AutomationPoint>) {
    if points.len() < 3 {
        return;
    }
    let original = points.clone();
    for index in 1..original.len() - 1 {
        let average =
            (original[index - 1].value + original[index].value + original[index + 1].value) / 3.0;
        points[index].value = average;
    }
}
