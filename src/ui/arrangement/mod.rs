//! 所属模块：ui::arrangement / 编曲区交互
//! 文件用途：
//! - 绘制可展开的 MIDI / 音频轨道、控制器与效果器子轨。
//! - 处理滚动、缩放、片段拖动、选择、拖放 WAV 与主输出交互。
//!
//! 函数清单：
//! - `SurfingApp::ui_arrangement`：编曲区绘制与全部交互入口。
//!
//! Module: ui::arrangement / arrangement interaction
//! Purpose:
//! - Paint expandable MIDI/audio tracks with controller and effect sub-lanes.
//! - Handle scrolling, zooming, clip dragging, selection, WAV drops and master output.
//!
//! Functions:
//! - `SurfingApp::ui_arrangement`: arrangement painting and interaction entry point.

use crate::app::SurfingApp;
use crate::core::model::{evaluate_automation, AudioSource, Clip, ControllerKind};

use crate::ui::{theme, ArrangementDrag, BottomView, MouseMode};
use eframe::egui::{self, Align2, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Vec2};

mod layout;
use layout::*;

impl SurfingApp {
    /// 所属模块：ui::arrangement / 编曲区交互
    /// 功能：绘制编曲区并处理滚动、缩放、片段编辑、拖放与主输出交互。
    /// 输入：
    /// - `ui`：当前 egui 画布。
    ///
    /// 输出/副作用：修改选择、工程片段/控制器、视图状态与状态栏；可能打开卷帘窗口。
    ///
    /// 复杂度：每帧 O(可见轨道数 + 可见片段数 + 可见控制器点)。
    ///
    /// Module: ui::arrangement / arrangement interaction
    /// Purpose: Paint the arrangement and handle scrolling, zooming, clip editing,
    /// drops and master output interaction.
    /// Inputs:
    /// - `ui`: current egui canvas.
    ///
    /// Outputs / side effects: mutates selection, project clips/controllers, view state
    /// and the status bar; may open roll windows.
    ///
    /// Complexity: O(visible tracks + visible clips + visible controller points) per frame.
    pub fn ui_arrangement(&mut self, ui: &mut egui::Ui) {
        let playhead = self.playhead();
        // 不自动跟随播放头：播放时用户可以自由平移。
        // No auto-follow: the user can pan freely while the transport runs.

        let available = ui.available_size();
        if available.x < 120.0 || available.y < 80.0 {
            return;
        }
        let (response, painter) = ui.allocate_painter(available, Sense::click_and_drag());
        let rect = response.rect;
        let header_w = track_header_width(rect);
        let lane_left = rect.left() + header_w;
        let lane_right = rect.right();
        let lane_width = (lane_right - lane_left).max(1.0);
        if lane_width < 20.0 {
            return;
        }
        let ruler_bottom = rect.top() + RULER_H;
        // The master bus has its own height (dragged by its bottom edge) and is
        // deliberately not affected by the track row zoom.
        let master_top = ruler_bottom;
        let master_total_h = if self.show_master {
            let max_total =
                (rect.bottom() - master_top - MASTER_GAP - 24.0).clamp(MASTER_MIN_H, MASTER_MAX_H);
            self.master_height.clamp(MASTER_MIN_H, max_total)
        } else {
            0.0
        };
        let master_main_h = (master_total_h - MASTER_SUB_H).max(0.0);
        let master_sub_h = if self.show_master { MASTER_SUB_H } else { 0.0 };
        let master_effect_top = master_top + master_main_h;
        let master_bottom = master_effect_top + master_sub_h;
        let lane_top = if self.show_master {
            master_bottom + MASTER_GAP
        } else {
            master_bottom
        };

        // Wheel: Ctrl zooms rows vertically, Ctrl+Shift zooms the timeline
        // horizontally, Shift pans, plain wheel scrolls rows.
        if response.hovered() {
            let (event_scroll, event_ctrl, event_shift) = ui.input(|i| {
                let mut scroll = 0.0_f32;
                let mut ctrl = false;
                let mut shift = false;
                for event in &i.events {
                    if let egui::Event::MouseWheel {
                        delta, modifiers, ..
                    } = event
                    {
                        // Some platforms report Shift+wheel as a horizontal
                        // delta, so accept both axes while Shift is held.
                        scroll += if modifiers.shift {
                            delta.y + delta.x
                        } else {
                            delta.y
                        };
                        ctrl |= modifiers.command;
                        shift |= modifiers.shift;
                    }
                }
                (scroll, ctrl, shift)
            });
            let (smooth_y, smooth_x, ctrl_now, shift_now) = ui.input(|i| {
                (
                    i.smooth_scroll_delta.y,
                    i.smooth_scroll_delta.x,
                    i.modifiers.command,
                    i.modifiers.shift,
                )
            });
            let smooth = if shift_now {
                smooth_y + smooth_x
            } else {
                smooth_y
            };
            // Prefer the per-event modifiers so a wheel notch keeps its Ctrl/Shift
            // meaning even after the key is released during smoothing.
            let (scroll, ctrl, shift) = if event_scroll.abs() > 0.1 {
                // Wheel events report whole lines; normalize to egui points.
                (event_scroll * 50.0, event_ctrl, event_shift)
            } else {
                (smooth, ctrl_now, shift_now)
            };
            if scroll.abs() > 0.1 {
                if ctrl && shift {
                    // Ctrl+Shift zooms the timeline horizontally.
                    let pointer = response.hover_pos().unwrap_or(rect.center());
                    let old_ppb = lane_width as f64 / self.visible_beats.max(1.0);
                    let anchor_beat = self.view_start + (pointer.x - lane_left) as f64 / old_ppb;
                    self.visible_beats =
                        (self.visible_beats * (1.0 - scroll as f64 * 0.002)).clamp(8.0, 64.0);
                    let new_ppb = lane_width as f64 / self.visible_beats;
                    self.view_start =
                        (anchor_beat - (pointer.x - lane_left) as f64 / new_ppb).max(0.0);
                } else if ctrl {
                    // Ctrl zooms the rows vertically.
                    let pointer = response.hover_pos().unwrap_or(rect.center());
                    let old_scale = self.arr_row_scale;
                    let content_y = (pointer.y - lane_top + self.arr_scroll).max(0.0) / old_scale;
                    self.arr_row_scale = (old_scale * (1.0 + scroll * 0.0015)).clamp(0.55, 2.4);
                    self.arr_scroll =
                        (content_y * self.arr_row_scale - (pointer.y - lane_top)).max(0.0);
                } else if shift {
                    let ppb = lane_width as f64 / self.visible_beats.max(1.0);
                    self.view_start = (self.view_start - scroll as f64 / ppb).max(0.0);
                } else {
                    self.arr_scroll -= scroll;
                }
            }
        }
        // 触摸屏双指捏合：以捏合中心为锚点缩放时间轴；桌面没有多点触控，不受影响。
        // Pinch on touch screens: zoom the timeline anchored at the pinch centre; desktop has no
        // multi-touch, so it is unaffected.
        let pinch = ui.input(|i| {
            i.multi_touch()
                .map(|multi| (multi.zoom_delta, multi.center_pos))
        });
        // 捏合期间必须停掉单指拖动，否则第二根手指落下前已经开始的拖动会继续移动片段/选区。
        // Stop single-pointer dragging while pinching: a drag started by the first finger would
        // otherwise keep moving clips or the marquee.
        let pinching = pinch.is_some();
        if pinching {
            self.arr_drag = ArrangementDrag::None;
        }
        if let Some((zoom, center)) = pinch {
            if (zoom - 1.0).abs() > 1e-3 && lane_width > 1.0 && rect.contains(center) {
                let old_ppb = lane_width as f64 / self.visible_beats.max(1.0);
                let anchor_beat = self.view_start + (center.x - lane_left) as f64 / old_ppb;
                self.visible_beats = (self.visible_beats / zoom as f64).clamp(8.0, 64.0);
                let new_ppb = lane_width as f64 / self.visible_beats;
                self.view_start = (anchor_beat - (center.x - lane_left) as f64 / new_ppb).max(0.0);
            }
        }
        let row_scale = self.arr_row_scale;
        let visible_height = (rect.bottom() - lane_top).max(1.0);
        // Effective pointer tool: holding Shift temporarily switches to Draw.
        let mode = if ui.input(|i| i.modifiers.shift) {
            MouseMode::Draw
        } else {
            self.mouse_mode
        };
        let content_height = total_height(&self.project, row_scale);
        let max_scroll = (content_height - visible_height).max(0.0);
        self.arr_scroll = self.arr_scroll.clamp(0.0, max_scroll);

        let ppb = lane_width as f64 / self.visible_beats.max(1.0);
        let view_start = self.view_start;
        let scroll = self.arr_scroll;
        let beat_to_x = |beat: f64| lane_left + ((beat - view_start) * ppb) as f32;
        let x_to_beat = |x: f32| view_start + (x - lane_left) as f64 / ppb;
        let beats_per_bar = self.project.beats_per_bar();
        let ruler_top = rect.top();
        let main_h = main_row_h(row_scale);
        let sub_h = sub_row_h(row_scale);
        let add_h = add_row_h(row_scale);
        // The app refreshes these once per frame in `refresh_engine_snapshot`,
        // so drawing never touches the engine lock.
        let levels_l = self.ui_levels_l.clone();
        let levels_r = self.ui_levels_r.clone();
        let master_levels = self.ui_master_levels;
        let waveforms = self.ui_waveforms.clone();
        let mut master_plus_rect: Option<Rect> = None;
        let mut master_close_rect: Option<Rect> = None;

        painter.rect_filled(rect, CornerRadius::from(0), theme::BG_DEEP);
        painter.rect_filled(
            Rect::from_min_max(egui::pos2(lane_left, master_top), rect.max),
            CornerRadius::from(0),
            theme::BG,
        );

        // Track row backgrounds.
        let mut offset = 0.0_f32;
        for (index, track) in self.project.tracks.iter().enumerate() {
            let block = block_height(track, row_scale);
            let y = lane_top + offset - scroll;
            offset += block;
            if y + block < lane_top || y > rect.bottom() {
                continue;
            }
            // Clip row backgrounds to the scrolling lane area so partially
            // visible child rows never cover the pinned master bus.
            let painter = painter.with_clip_rect(Rect::from_min_max(
                egui::pos2(lane_left, lane_top),
                egui::pos2(lane_right, rect.bottom()),
            ));
            let main_rect =
                Rect::from_min_max(egui::pos2(lane_left, y), egui::pos2(lane_right, y + main_h));
            if main_rect.bottom() >= lane_top {
                let fill = if index == self.selected_track {
                    theme::with_alpha(theme::ACCENT, 10)
                } else if index % 2 == 0 {
                    theme::BG
                } else {
                    theme::PANEL
                };
                painter.rect_filled(main_rect, CornerRadius::from(0), fill);
                if index == self.selected_track {
                    painter.rect_stroke(
                        main_rect,
                        CornerRadius::from(0),
                        Stroke::new(1.0_f32, theme::with_alpha(theme::ACCENT, 90)),
                        StrokeKind::Inside,
                    );
                }
            }
            if track.expanded {
                let open_controller = if self.bottom_open {
                    match self.bottom_view {
                        BottomView::Controller { track_id, kind } if track_id == track.id => {
                            Some(kind)
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                let mut sy = y + main_h;
                // Thin row that hosts the "+" button for adding controllers.
                let add_rect = Rect::from_min_max(
                    egui::pos2(lane_left, sy),
                    egui::pos2(lane_right, sy + add_h),
                );
                if add_rect.bottom() >= lane_top && add_rect.top() <= rect.bottom() {
                    painter.rect_filled(
                        add_rect,
                        CornerRadius::from(0),
                        theme::with_alpha(theme::ACCENT, 8),
                    );
                    painter.rect_filled(
                        Rect::from_min_max(
                            egui::pos2(lane_left, add_rect.bottom() - 2.0),
                            egui::pos2(lane_right, add_rect.bottom()),
                        ),
                        CornerRadius::from(0),
                        theme::BG_DEEP,
                    );
                }
                sy += add_h;
                for (row_index, kind) in track.visible_controllers.iter().enumerate() {
                    let highlighted = open_controller == Some(*kind)
                        || self.selected_controller == Some((track.id, *kind));
                    let sub_rect = Rect::from_min_max(
                        egui::pos2(lane_left, sy),
                        egui::pos2(lane_right, sy + sub_h),
                    );
                    if sub_rect.bottom() >= lane_top && sub_rect.top() <= rect.bottom() {
                        painter.rect_filled(
                            sub_rect,
                            CornerRadius::from(0),
                            if highlighted {
                                theme::with_alpha(theme::ACCENT, 22)
                            } else if row_index % 2 == 0 {
                                theme::PANEL_2
                            } else {
                                theme::PANEL
                            },
                        );
                        // A solid dark gap makes adjacent sub-lanes easy to separate.
                        painter.rect_filled(
                            Rect::from_min_max(
                                egui::pos2(lane_left, sub_rect.bottom() - 2.0),
                                egui::pos2(lane_right, sub_rect.bottom()),
                            ),
                            CornerRadius::from(0),
                            theme::BG_DEEP,
                        );
                    }
                    sy += sub_h;
                }
                let effects_rect = Rect::from_min_max(
                    egui::pos2(lane_left, sy),
                    egui::pos2(lane_right, sy + sub_h),
                );
                let effects_selected = self.selected_effects_lane == Some(track.id);
                if effects_rect.bottom() >= lane_top && effects_rect.top() <= rect.bottom() {
                    painter.rect_filled(
                        effects_rect,
                        CornerRadius::from(0),
                        theme::with_alpha(theme::ACCENT, if effects_selected { 34 } else { 14 }),
                    );
                    if effects_selected {
                        painter.rect_stroke(
                            effects_rect,
                            CornerRadius::from(0),
                            Stroke::new(1.0_f32, theme::with_alpha(theme::ACCENT, 160)),
                            StrokeKind::Inside,
                        );
                    }
                    painter.rect_filled(
                        Rect::from_min_max(
                            egui::pos2(lane_left, effects_rect.bottom() - 2.0),
                            egui::pos2(lane_right, effects_rect.bottom()),
                        ),
                        CornerRadius::from(0),
                        theme::BG_DEEP,
                    );
                }
            }
            painter.rect_filled(
                Rect::from_min_max(
                    egui::pos2(lane_left, y + block - 2.0),
                    egui::pos2(lane_right, y + block),
                ),
                CornerRadius::from(0),
                theme::BG_DEEP,
            );
        }

        // Loop shading.
        if self.project.loop_enabled {
            let x0 = beat_to_x(self.project.loop_start).max(lane_left);
            let x1 = beat_to_x(self.project.loop_end).min(lane_right);
            if x1 > x0 {
                painter.rect_filled(
                    Rect::from_min_max(egui::pos2(x0, ruler_top), egui::pos2(x1, rect.bottom())),
                    CornerRadius::from(0),
                    theme::with_alpha(theme::ACCENT, 9),
                );
            }
        }

        // Beat/bar grid.
        let first_beat = view_start.floor() as i64;
        let last_beat = (view_start + self.visible_beats).ceil() as i64 + 1;
        for beat in first_beat..last_beat {
            let x = beat_to_x(beat as f64);
            if x < lane_left - 1.0 || x > lane_right + 1.0 {
                continue;
            }
            let is_bar = (beat as f64 % beats_per_bar).abs() < 1e-6;
            painter.vline(
                x,
                ruler_top..=rect.bottom(),
                Stroke::new(
                    1.0_f32,
                    if is_bar {
                        theme::with_alpha(theme::LINE, 210)
                    } else {
                        theme::with_alpha(theme::LINE, 70)
                    },
                ),
            );
        }

        // Ruler.
        let ruler_rect =
            Rect::from_min_max(rect.min, egui::pos2(rect.right(), ruler_top + RULER_H));
        painter.rect_filled(ruler_rect, CornerRadius::from(0), theme::PANEL_2);
        painter.hline(
            rect.left()..=rect.right(),
            ruler_top + RULER_H,
            Stroke::new(1.0_f32, theme::LINE),
        );
        painter.rect_filled(
            Rect::from_min_max(rect.min, egui::pos2(lane_left, ruler_top + RULER_H)),
            CornerRadius::from(0),
            theme::PANEL,
        );
        // Master-bus visibility toggle.
        let toggle_rect = Rect::from_center_size(
            egui::pos2(rect.left() + 13.0, ruler_top + RULER_H * 0.5),
            Vec2::splat(16.0),
        );
        let tri = if self.show_master {
            vec![
                egui::pos2(toggle_rect.left() + 3.0, toggle_rect.top() + 6.0),
                egui::pos2(toggle_rect.right() - 3.0, toggle_rect.top() + 6.0),
                egui::pos2(toggle_rect.center().x, toggle_rect.bottom() - 5.0),
            ]
        } else {
            vec![
                egui::pos2(toggle_rect.left() + 5.0, toggle_rect.top() + 3.0),
                egui::pos2(toggle_rect.left() + 5.0, toggle_rect.bottom() - 3.0),
                egui::pos2(toggle_rect.right() - 4.0, toggle_rect.center().y),
            ]
        };
        painter.add(egui::Shape::convex_polygon(
            tri,
            theme::TEXT_DIM,
            Stroke::NONE,
        ));
        painter.text(
            egui::pos2(rect.left() + 26.0, ruler_top + RULER_H * 0.5),
            Align2::LEFT_CENTER,
            "轨道 / TRACKS",
            FontId::proportional(10.0),
            theme::TEXT_DIM,
        );
        if ui
            .interact(toggle_rect, ui.id().with("toggle_master"), Sense::click())
            .on_hover_text("显示 / 隐藏主控轨道")
            .clicked()
        {
            self.show_master = !self.show_master;
        }
        let first_bar = (view_start / beats_per_bar).floor() as i64;
        let last_bar = ((view_start + self.visible_beats) / beats_per_bar).ceil() as i64 + 1;
        for bar in first_bar..last_bar {
            let x = beat_to_x(bar as f64 * beats_per_bar);
            if x < lane_left - 20.0 || x > lane_right {
                continue;
            }
            painter.text(
                egui::pos2(x + 4.0, ruler_top + 8.0),
                Align2::LEFT_TOP,
                format!("{}", bar),
                FontId::monospace(11.0),
                theme::TEXT_DIM,
            );
            let y = ruler_top + RULER_H - 9.0;
            painter.line_segment(
                [egui::pos2(x, y), egui::pos2(x, y + 6.0)],
                Stroke::new(1.0_f32, theme::TEXT_FAINT),
            );
        }

        // Loop locator flags (drag to change the loop range).
        let loop_start_x = beat_to_x(self.project.loop_start);
        let loop_end_x = beat_to_x(self.project.loop_end);
        let start_flag_rect = Rect::from_min_max(
            egui::pos2(loop_start_x - 9.0, ruler_top + 2.0),
            egui::pos2(loop_start_x + 9.0, ruler_top + RULER_H),
        );
        let end_flag_rect = Rect::from_min_max(
            egui::pos2(loop_end_x - 9.0, ruler_top + 2.0),
            egui::pos2(loop_end_x + 9.0, ruler_top + RULER_H),
        );
        for (x, is_start) in [(loop_start_x, true), (loop_end_x, false)] {
            if x < lane_left - 14.0 || x > lane_right + 14.0 {
                continue;
            }
            let color = if self.project.loop_enabled {
                theme::ACCENT
            } else {
                theme::AMBER
            };
            painter.line_segment(
                [
                    egui::pos2(x, ruler_top + RULER_H),
                    egui::pos2(x, rect.bottom()),
                ],
                Stroke::new(
                    1.0_f32,
                    theme::with_alpha(color, if self.project.loop_enabled { 110 } else { 50 }),
                ),
            );
            let top = ruler_top + 3.0;
            let points = if is_start {
                vec![
                    egui::pos2(x, top),
                    egui::pos2(x + 13.0, top),
                    egui::pos2(x, top + 11.0),
                ]
            } else {
                vec![
                    egui::pos2(x, top),
                    egui::pos2(x - 13.0, top),
                    egui::pos2(x, top + 11.0),
                ]
            };
            painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
            painter.line_segment(
                [egui::pos2(x, top), egui::pos2(x, top + 15.0)],
                Stroke::new(2.0_f32, color),
            );
            painter.text(
                egui::pos2(x + if is_start { 5.0 } else { -5.0 }, top + 16.0),
                if is_start {
                    Align2::LEFT_TOP
                } else {
                    Align2::RIGHT_TOP
                },
                if is_start { "起" } else { "止" },
                FontId::monospace(8.0),
                color,
            );
        }

        // Pinned master block: volume controller on top, effect chain below.
        let master_volume_rect = if !self.show_master {
            None
        } else {
            let main_rect = Rect::from_min_max(
                egui::pos2(lane_left, master_top),
                egui::pos2(lane_right, master_effect_top),
            );
            painter.rect_filled(main_rect, CornerRadius::from(0), theme::PANEL_2);
            // Master volume automation (the main lane is a volume controller).
            let value_y = |value: f32| {
                main_rect.bottom()
                    - 4.0
                    - value.clamp(0.0, 1.0) * (main_rect.height() - 8.0).max(4.0)
            };
            let master_points = self.project.master_controllers.get(ControllerKind::Volume);
            if master_points.is_empty() {
                let ratio = (self.project.master_volume / 1.5).clamp(0.0, 1.0);
                painter.hline(
                    main_rect.x_range(),
                    value_y(ratio),
                    Stroke::new(2.0_f32, theme::ACCENT),
                );
                painter.circle_filled(
                    egui::pos2(main_rect.left() + 10.0, value_y(ratio)),
                    4.0,
                    theme::ACCENT,
                );
            } else {
                let steps = (main_rect.width() / 4.0).max(4.0) as usize;
                let mut previous: Option<egui::Pos2> = None;
                for step in 0..=steps {
                    let x = main_rect.left() + step as f32 / steps as f32 * main_rect.width();
                    let value = evaluate_automation(master_points, x_to_beat(x)).unwrap_or(0.5);
                    let point = egui::pos2(x, value_y(value));
                    if let Some(previous) = previous {
                        painter
                            .line_segment([previous, point], Stroke::new(2.0_f32, theme::ACCENT));
                    }
                    previous = Some(point);
                }
                for point in master_points {
                    if point.beat < view_start || point.beat > view_start + self.visible_beats {
                        continue;
                    }
                    painter.circle_filled(
                        egui::pos2(beat_to_x(point.beat), value_y(point.value)),
                        3.5,
                        theme::ACCENT,
                    );
                }
            }

            let sub_rect = Rect::from_min_max(
                egui::pos2(lane_left, master_effect_top),
                egui::pos2(lane_right, master_bottom),
            );
            painter.rect_filled(sub_rect, CornerRadius::from(0), theme::PANEL);
            painter.rect_filled(
                Rect::from_min_max(
                    egui::pos2(lane_left, sub_rect.bottom() - 2.0),
                    egui::pos2(lane_right, sub_rect.bottom()),
                ),
                CornerRadius::from(0),
                theme::BG_DEEP,
            );

            // Header: main row.
            let master_header = Rect::from_min_max(
                egui::pos2(rect.left(), master_top),
                egui::pos2(lane_left, master_effect_top),
            );
            painter.rect_filled(master_header, CornerRadius::from(0), theme::PANEL_3);
            painter.rect_filled(
                Rect::from_min_max(
                    egui::pos2(rect.left(), master_top),
                    egui::pos2(rect.left() + 4.0, master_effect_top),
                ),
                CornerRadius::from(0),
                theme::ACCENT,
            );
            painter.text(
                egui::pos2(rect.left() + 28.0, master_top + 6.0),
                Align2::LEFT_TOP,
                "主控 / MASTER",
                FontId::proportional(13.0),
                theme::TEXT,
            );
            painter.text(
                egui::pos2(rect.left() + 28.0, master_top + 24.0),
                Align2::LEFT_TOP,
                format!(
                    "总音量 {:.0}%",
                    self.project.master_volume_at(playhead) * 100.0
                ),
                FontId::proportional(9.0),
                theme::TEXT_DIM,
            );
            let bar_area = Rect::from_min_max(
                egui::pos2(rect.left() + 94.0, master_effect_top - 18.0),
                egui::pos2(lane_left - 6.0, master_effect_top - 5.0),
            );
            if bar_area.width() > 16.0 {
                // Master volume is edited in the controller roll, so the bars
                // are meter-only: no drag handle.
                draw_volume_bars(
                    &painter,
                    bar_area,
                    self.project.master_volume_at(playhead),
                    master_levels.0,
                    master_levels.1,
                    false,
                );
            }
            // Header: effect sub-lane (indented like a child lane).
            let sub_header = Rect::from_min_max(
                egui::pos2(rect.left(), master_effect_top),
                egui::pos2(lane_left, master_bottom),
            );
            painter.rect_filled(sub_header, CornerRadius::from(0), theme::PANEL_2);
            painter.rect_filled(
                Rect::from_min_max(
                    egui::pos2(rect.left() + 8.0, master_effect_top + 2.0),
                    egui::pos2(rect.left() + 11.0, master_bottom - 3.0),
                ),
                CornerRadius::from(0),
                theme::with_alpha(theme::ACCENT, 90),
            );
            painter.text(
                egui::pos2(rect.left() + 20.0, sub_header.center().y),
                Align2::LEFT_CENTER,
                format!(
                    "效果器链 · {}",
                    usize::from(self.project.master_effect.is_some())
                ),
                FontId::proportional(10.0),
                theme::ACCENT,
            );

            if let Some(effect) = &self.project.master_effect {
                painter.text(
                    egui::pos2(lane_left + 10.0, sub_rect.center().y),
                    Align2::LEFT_CENTER,
                    format!("{}  ·  {}", effect.label(), effect.badge()),
                    FontId::proportional(12.0),
                    theme::ACCENT,
                );
                let close = Rect::from_center_size(
                    egui::pos2(lane_right - 16.0, sub_rect.center().y),
                    Vec2::splat(18.0),
                );
                painter.text(
                    close.center(),
                    Align2::CENTER_CENTER,
                    "×",
                    FontId::proportional(13.0),
                    theme::TEXT_DIM,
                );
                master_close_rect = Some(close);
            } else {
                let chip = Rect::from_center_size(
                    egui::pos2(lane_left + 24.0, sub_rect.center().y),
                    Vec2::new(28.0, 20.0),
                );
                painter.rect_filled(
                    chip,
                    CornerRadius::from(5),
                    theme::with_alpha(theme::ACCENT, 28),
                );
                painter.text(
                    chip.center(),
                    Align2::CENTER_CENTER,
                    "+",
                    FontId::proportional(16.0),
                    theme::ACCENT,
                );
                master_plus_rect = Some(chip);
            }
            // Divider between the pinned master bus and the instrument tracks.
            let gap = Rect::from_min_max(
                egui::pos2(rect.left(), master_bottom),
                egui::pos2(rect.right(), lane_top),
            );
            painter.rect_filled(gap, CornerRadius::from(0), theme::BG_DEEP);
            painter.hline(
                gap.x_range(),
                gap.top() + 0.5,
                Stroke::new(1.0_f32, theme::with_alpha(theme::LINE, 220)),
            );
            painter.hline(
                gap.x_range(),
                gap.center().y,
                Stroke::new(2.0_f32, theme::with_alpha(theme::ACCENT, 70)),
            );
            painter.hline(
                gap.x_range(),
                gap.bottom() - 0.5,
                Stroke::new(1.0_f32, theme::with_alpha(theme::LINE, 240)),
            );
            // Grip indicating that the bottom edge can be dragged vertically.
            let grip_x = lane_right - 18.0;
            let grip_y = gap.center().y;
            for offset in [-2.0_f32, 0.0, 2.0] {
                painter.hline(
                    (grip_x - 7.0)..=(grip_x + 7.0),
                    grip_y + offset,
                    Stroke::new(1.0_f32, theme::with_alpha(theme::TEXT_FAINT, 150)),
                );
            }
            Some(main_rect)
        };

        // Clips on main lanes. Draw with an unclipped clip rect inside a painter
        // clipped to the lane area so content keeps moving while the view pans.
        let clip_painter = painter.with_clip_rect(Rect::from_min_max(
            egui::pos2(lane_left, lane_top),
            egui::pos2(lane_right, rect.bottom()),
        ));
        let mut row_offset = 0.0_f32;
        for (index, track) in self.project.tracks.iter().enumerate() {
            let y = lane_top + row_offset - scroll;
            row_offset += block_height(track, row_scale);
            if y + main_h < lane_top || y > rect.bottom() {
                continue;
            }
            let track_color = theme::track_color(track.color);
            for (clip_index, clip) in track.clips.iter().enumerate() {
                let color = clip.color.map(theme::palette_color).unwrap_or(track_color);
                let x0 = beat_to_x(clip.start);
                let x1 = beat_to_x(clip.end());
                if x1 < lane_left || x0 > lane_right {
                    continue;
                }
                let clip_rect =
                    Rect::from_min_max(egui::pos2(x0, y + 3.0), egui::pos2(x1, y + main_h - 3.0));
                if clip_rect.width() < 4.0 {
                    continue;
                }
                let selected = self.selected_clips.contains(&(index, clip_index))
                    || (self.selected_track == index && self.selected_clip == Some(clip_index));
                clip_painter.rect_filled(
                    clip_rect,
                    CornerRadius::from(0),
                    theme::with_alpha(color, 46),
                );
                clip_painter.rect_filled(
                    Rect::from_min_size(clip_rect.min, Vec2::new(clip_rect.width(), 3.0)),
                    CornerRadius::from(0),
                    color,
                );
                clip_painter.rect_stroke(
                    clip_rect,
                    CornerRadius::from(0),
                    Stroke::new(
                        if selected { 1.8_f32 } else { 1.0_f32 },
                        if selected {
                            egui::Color32::WHITE
                        } else {
                            theme::with_alpha(color, 150)
                        },
                    ),
                    StrokeKind::Inside,
                );
                // Beat guides inside the clip. They use the same beat-to-x
                // mapping as the arrangement grid, so both clip edges and the
                // notes stay visually locked to the beat lines.
                if clip_rect.width() > 8.0 {
                    let grid_painter = clip_painter.with_clip_rect(clip_rect);
                    let first = clip.start.floor() as i64;
                    let last = clip.end().ceil() as i64;
                    for beat in first..=last {
                        let beat = beat as f64;
                        if beat < clip.start - 1e-9 || beat > clip.end() + 1e-9 {
                            continue;
                        }
                        let x = beat_to_x(beat);
                        if x <= clip_rect.left() + 0.5 || x >= clip_rect.right() - 0.5 {
                            continue;
                        }
                        let is_bar = (beat % beats_per_bar).abs() < 1e-6;
                        grid_painter.vline(
                            x,
                            clip_rect.top()..=clip_rect.bottom(),
                            Stroke::new(
                                1.0_f32,
                                if is_bar {
                                    theme::with_alpha(egui::Color32::WHITE, 52)
                                } else {
                                    theme::with_alpha(egui::Color32::WHITE, 20)
                                },
                            ),
                        );
                    }
                }
                if let Some(source) = &clip.audio {
                    let clip_seconds = clip.length * 60.0 / self.project.bpm.max(1.0);
                    let total_seconds = source.length_seconds.max(clip_seconds).max(1e-6);
                    let start_fraction = (source.offset_seconds / total_seconds) as f32;
                    let span_fraction = (clip_seconds / total_seconds) as f32;
                    draw_audio_waveform(
                        &clip_painter,
                        clip_rect,
                        waveforms.get(&source.path).map(Vec::as_slice),
                        color,
                        start_fraction,
                        span_fraction,
                    );
                } else if !clip.notes.is_empty() && clip_rect.height() > 20.0 {
                    let mut min_pitch = 127u8;
                    let mut max_pitch = 0u8;
                    for note in &clip.notes {
                        min_pitch = min_pitch.min(note.pitch);
                        max_pitch = max_pitch.max(note.pitch);
                    }
                    let span = (max_pitch.saturating_sub(min_pitch)).max(10) as f32;
                    let inner = Rect::from_min_max(
                        egui::pos2(clip_rect.left() + 3.0, clip_rect.top() + 18.0),
                        egui::pos2(clip_rect.right() - 3.0, clip_rect.bottom() - 5.0),
                    );
                    // Clip the preview to the clip body: notes must never draw
                    // past the MIDI clip edges.
                    let note_painter = clip_painter.with_clip_rect(clip_rect.shrink(1.0));
                    for note in clip.notes.iter().take(400) {
                        // Notes trimmed away by a shorter clip stay hidden.
                        if note.start < -1e-6 || note.end() > clip.length + 1e-6 {
                            continue;
                        }
                        // Absolute beat mapping keeps the preview glued to the
                        // clip while the timeline pans.
                        let nx0 = beat_to_x(clip.start + note.start).max(clip_rect.left() + 1.0);
                        let nx1 = beat_to_x(clip.start + note.end()).min(clip_rect.right() - 1.0);
                        if nx1 <= nx0 {
                            continue;
                        }
                        let ny = inner.bottom()
                            - ((note.pitch.saturating_sub(min_pitch)) as f32 / span)
                                * inner.height();
                        note_painter.line_segment(
                            [egui::pos2(nx0, ny), egui::pos2(nx1, ny)],
                            Stroke::new(1.6_f32, theme::with_alpha(color, 190)),
                        );
                    }
                }
                if clip_rect.width() > 34.0 {
                    clip_painter.text(
                        egui::pos2(clip_rect.left() + 7.0, clip_rect.top() + 7.0),
                        Align2::LEFT_TOP,
                        &clip.name,
                        FontId::proportional(10.0),
                        theme::TEXT,
                    );
                }
                // 音频片段实时读取源文件，MIDI 片段实时渲染，均无需等待缓存。
                // Audio clips stream from source and MIDI clips render live; no cache wait.
                if clip.audio.is_some() {
                    if clip_rect.width() > 70.0 {
                        clip_painter.text(
                            egui::pos2(clip_rect.right() - 9.0, clip_rect.top() + 9.0),
                            Align2::RIGHT_CENTER,
                            "音频",
                            FontId::proportional(9.0),
                            theme::with_alpha(color, 220),
                        );
                    }
                } else if clip_rect.width() > 70.0 {
                    clip_painter.circle_filled(
                        egui::pos2(clip_rect.right() - 9.0, clip_rect.top() + 9.0),
                        3.0,
                        theme::GREEN,
                    );
                }
            }
        }

        // Controller / effect sub-lanes.
        row_offset = 0.0;
        let mut add_effect_rects: Vec<(usize, Rect)> = Vec::new();
        for (index, track) in self.project.tracks.iter().enumerate() {
            let block_y = lane_top + row_offset - scroll;
            row_offset += block_height(track, row_scale);
            if !track.expanded {
                continue;
            }
            // Keep automation lines inside the lane area so scrolled-up child
            // lanes never bleed over the pinned master bus.
            let painter = painter.with_clip_rect(Rect::from_min_max(
                egui::pos2(lane_left, lane_top),
                egui::pos2(lane_right, rect.bottom()),
            ));
            let color = theme::track_color(track.color);
            let open_controller = if self.bottom_open {
                match self.bottom_view {
                    BottomView::Controller { track_id, kind } if track_id == track.id => Some(kind),
                    _ => None,
                }
            } else {
                None
            };
            let mut sy = block_y + main_h + add_h;
            for kind in track.visible_controllers.iter().copied() {
                let lane_color = if track.supports_controller(kind) {
                    color
                } else {
                    theme::TEXT_FAINT
                };
                let selected = self.selected_controller == Some((track.id, kind));
                let lane_rect = Rect::from_min_max(
                    egui::pos2(lane_left, sy),
                    egui::pos2(lane_right, sy + sub_h),
                );
                if lane_rect.bottom() >= lane_top && lane_rect.top() <= rect.bottom() {
                    if open_controller == Some(kind) || selected {
                        painter.rect_stroke(
                            lane_rect,
                            CornerRadius::from(0),
                            Stroke::new(
                                if selected { 1.6_f32 } else { 1.0_f32 },
                                theme::with_alpha(theme::ACCENT, 150),
                            ),
                            StrokeKind::Inside,
                        );
                    }
                    let inner = Rect::from_min_max(
                        egui::pos2(lane_rect.left() + 2.0, lane_rect.top() + 4.0),
                        egui::pos2(lane_rect.right() - 2.0, lane_rect.bottom() - 4.0),
                    );
                    let points = track.controllers.get(kind);
                    if !points.is_empty()
                        && matches!(kind, ControllerKind::Pan | ControllerKind::PitchBend)
                    {
                        painter.hline(
                            inner.x_range(),
                            inner.center().y,
                            Stroke::new(1.0_f32, theme::with_alpha(theme::LINE, 110)),
                        );
                    }
                    if points.is_empty() {
                        // Empty lanes still show the parameter's neutral line.
                        let value = kind.center();
                        let y =
                            inner.bottom() - (value.clamp(0.0, 1.0) * 0.92 + 0.04) * inner.height();
                        painter.line_segment(
                            [egui::pos2(inner.left(), y), egui::pos2(inner.right(), y)],
                            Stroke::new(1.4_f32, theme::with_alpha(lane_color, 140)),
                        );
                    } else {
                        let mut previous: Option<egui::Pos2> = None;
                        let steps = (inner.width() / 4.0).max(4.0) as usize;
                        for step in 0..=steps {
                            let beat = view_start + step as f64 / steps as f64 * self.visible_beats;
                            let value = evaluate_automation(points, beat).unwrap_or(kind.center());
                            let point = egui::pos2(
                                inner.left() + step as f32 / steps as f32 * inner.width(),
                                inner.bottom() - value.clamp(0.0, 1.0) * inner.height(),
                            );
                            if let Some(previous) = previous {
                                painter.line_segment(
                                    [previous, point],
                                    Stroke::new(1.4_f32, theme::with_alpha(lane_color, 210)),
                                );
                            }
                            previous = Some(point);
                        }
                    }
                }
                sy += sub_h;
            }
            let effects_rect = Rect::from_min_max(
                egui::pos2(lane_left, sy),
                egui::pos2(lane_right, sy + sub_h),
            );
            if effects_rect.bottom() >= lane_top && effects_rect.top() <= rect.bottom() {
                let names: Vec<String> =
                    track.effects.iter().map(|effect| effect.label()).collect();
                let text = names.join("  →  ");
                if !text.is_empty() {
                    painter.text(
                        egui::pos2(effects_rect.left() + 10.0, effects_rect.center().y),
                        Align2::LEFT_CENTER,
                        text,
                        FontId::proportional(10.0),
                        color,
                    );
                } else {
                    // Empty chain: offer a "+" like the master effect row.
                    let chip = Rect::from_center_size(
                        egui::pos2(effects_rect.left() + 24.0, effects_rect.center().y),
                        Vec2::new(28.0, 20.0),
                    );
                    painter.rect_filled(
                        chip,
                        CornerRadius::from(5),
                        theme::with_alpha(theme::ACCENT, 28),
                    );
                    painter.text(
                        chip.center(),
                        Align2::CENTER_CENTER,
                        "+",
                        FontId::proportional(16.0),
                        theme::ACCENT,
                    );
                    add_effect_rects.push((index, chip));
                }
            }
        }

        // Track headers.
        let mut add_controller_rects: Vec<(usize, Rect)> = Vec::new();
        let mut remove_controller_rects: Vec<(usize, ControllerKind, Rect)> = Vec::new();
        let mut volume_bar_rects: Vec<(usize, [Rect; 2])> = Vec::new();
        row_offset = 0.0;
        for (index, track) in self.project.tracks.iter().enumerate() {
            let block_y = lane_top + row_offset - scroll;
            let block = block_height(track, row_scale);
            row_offset += block;
            if block_y + block < lane_top || block_y > rect.bottom() {
                continue;
            }
            // Keep header content below the pinned master bus when scrolling.
            let painter = painter.with_clip_rect(Rect::from_min_max(
                egui::pos2(rect.left(), lane_top.max(block_y)),
                egui::pos2(lane_left, (block_y + block).min(rect.bottom())),
            ));
            let color = theme::track_color(track.color);
            // Main header block, drawn separately so child lanes can live in a
            // nested block underneath (same structure as the master bus).
            let main_rect = Rect::from_min_max(
                egui::pos2(rect.left(), block_y),
                egui::pos2(lane_left, block_y + main_h),
            );
            let header_fill = if index == self.selected_track {
                theme::PANEL_3
            } else {
                theme::PANEL_2
            };
            painter.rect_filled(main_rect, CornerRadius::from(0), header_fill);
            painter.rect_filled(
                Rect::from_min_max(
                    egui::pos2(rect.left(), block_y),
                    egui::pos2(rect.left() + 4.0, block_y + main_h),
                ),
                CornerRadius::from(0),
                color,
            );
            if index == self.selected_track {
                painter.rect_stroke(
                    main_rect,
                    CornerRadius::from(0),
                    Stroke::new(1.0_f32, theme::with_alpha(color, 180)),
                    StrokeKind::Inside,
                );
            }
            // Expand chevron.
            let chevron = Rect::from_min_size(
                egui::pos2(rect.left() + 8.0, block_y + (main_h - 16.0) * 0.5),
                Vec2::new(16.0, 16.0),
            );
            let triangle = if track.expanded {
                vec![
                    egui::pos2(chevron.left() + 3.0, chevron.top() + 5.0),
                    egui::pos2(chevron.right() - 3.0, chevron.top() + 5.0),
                    egui::pos2(chevron.center().x, chevron.bottom() - 4.0),
                ]
            } else {
                vec![
                    egui::pos2(chevron.left() + 5.0, chevron.top() + 3.0),
                    egui::pos2(chevron.left() + 5.0, chevron.bottom() - 3.0),
                    egui::pos2(chevron.right() - 4.0, chevron.center().y),
                ]
            };
            painter.add(egui::Shape::convex_polygon(
                triangle,
                if index == self.selected_track {
                    color
                } else {
                    theme::TEXT_DIM
                },
                Stroke::NONE,
            ));

            painter.text(
                egui::pos2(rect.left() + 28.0, block_y + 9.0),
                Align2::LEFT_TOP,
                &track.name,
                FontId::proportional(13.0),
                theme::TEXT,
            );
            if main_h >= 64.0 {
                let subtitle = if track.is_audio_lane() {
                    "音频轨道".to_string()
                } else {
                    track.instrument_name()
                };
                painter.text(
                    egui::pos2(rect.left() + 28.0, block_y + 27.0),
                    Align2::LEFT_TOP,
                    subtitle,
                    FontId::proportional(10.0),
                    theme::TEXT_DIM,
                );
            }

            let controls_y = block_y + main_h - 25.0 * row_scale.clamp(0.55, 2.4);
            let mute_rect = Rect::from_min_size(
                egui::pos2(rect.left() + 28.0, controls_y),
                Vec2::new(26.0, 18.0),
            );
            let solo_rect = Rect::from_min_size(
                egui::pos2(rect.left() + 60.0, controls_y),
                Vec2::new(26.0, 18.0),
            );
            painter.rect_filled(
                mute_rect,
                CornerRadius::from(4),
                if track.muted {
                    theme::RED
                } else {
                    theme::PANEL_3
                },
            );
            painter.text(
                mute_rect.center(),
                Align2::CENTER_CENTER,
                "M",
                FontId::monospace(11.0),
                if track.muted {
                    theme::BG_DEEP
                } else {
                    theme::TEXT_DIM
                },
            );
            painter.rect_filled(
                solo_rect,
                CornerRadius::from(4),
                if track.solo {
                    theme::AMBER
                } else {
                    theme::PANEL_3
                },
            );
            painter.text(
                solo_rect.center(),
                Align2::CENTER_CENTER,
                "S",
                FontId::monospace(11.0),
                if track.solo {
                    theme::BG_DEEP
                } else {
                    theme::TEXT_DIM
                },
            );
            // Two horizontal L/R bars double as volume sliders.
            let bar_bottom = block_y + main_h - 4.0 * row_scale.clamp(0.55, 1.6);
            let bar_area = Rect::from_min_max(
                egui::pos2(rect.left() + 94.0, bar_bottom - 14.0),
                egui::pos2(lane_left - 6.0, bar_bottom),
            );
            if bar_area.width() > 16.0 {
                let bars = draw_volume_bars(
                    &painter,
                    bar_area,
                    track.volume,
                    levels_l.get(index).copied().unwrap_or(0.0),
                    levels_r.get(index).copied().unwrap_or(0.0),
                    true,
                );
                volume_bar_rects.push((index, bars));
            }

            // Sub-lane headers.
            if track.expanded {
                let open_controller = if self.bottom_open {
                    match self.bottom_view {
                        BottomView::Controller { track_id, kind } if track_id == track.id => {
                            Some(kind)
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                // Child lanes live in an indented list of independent cells.
                // Keep the indentation small so the child list takes a little
                // more of the header width.
                let sub_left = rect.left() + 4.0;
                let sub_right = lane_left - 1.0;
                let sub_top = (block_y + main_h).max(lane_top);
                let sub_bottom = (block_y + block).min(rect.bottom() - 1.0);
                if sub_bottom > sub_top + 2.0 {
                    painter.hline(
                        rect.left()..=lane_left,
                        block_y + main_h,
                        Stroke::new(1.0_f32, theme::with_alpha(theme::LINE, 200)),
                    );
                    painter.rect_filled(
                        Rect::from_min_max(
                            egui::pos2(sub_left, sub_top),
                            egui::pos2(sub_right, sub_bottom),
                        ),
                        CornerRadius::from(4),
                        theme::PANEL,
                    );
                }
                // Inset colour tab shared by every child cell: it does not fill
                // the full cell height, which keeps the cells visually separate.
                let draw_tab = |top: f32, bottom: f32| {
                    if bottom - top > 6.0 {
                        painter.rect_filled(
                            Rect::from_min_max(
                                egui::pos2(sub_left + 3.0, top + 4.0),
                                egui::pos2(sub_left + 6.0, bottom - 4.0),
                            ),
                            CornerRadius::from(2),
                            theme::with_alpha(color, 120),
                        );
                    }
                };
                let mut sy = block_y + main_h;
                // "+" row.
                let add_cell =
                    Rect::from_min_max(egui::pos2(sub_left, sy), egui::pos2(sub_right, sy + add_h));
                if add_cell.bottom() >= lane_top && add_cell.top() <= rect.bottom() {
                    // The add row spans the full child-cell width, but its
                    // highlight only covers a short centred strip around "+".
                    let fill = Rect::from_center_size(
                        add_cell.center(),
                        Vec2::new(add_cell.width() * 0.55, (add_cell.height() - 8.0).max(12.0)),
                    );
                    painter.rect_filled(
                        fill,
                        CornerRadius::from(4),
                        theme::with_alpha(theme::ACCENT, 10),
                    );
                    painter.text(
                        add_cell.center(),
                        Align2::CENTER_CENTER,
                        "+",
                        FontId::proportional(18.0),
                        theme::ACCENT,
                    );
                    add_controller_rects.push((index, add_cell));
                    painter.hline(
                        sub_left..=sub_right,
                        add_cell.bottom(),
                        Stroke::new(1.0_f32, theme::with_alpha(theme::LINE, 150)),
                    );
                }
                sy += add_h;
                for kind in track.visible_controllers.iter().copied() {
                    let cell = Rect::from_min_max(
                        egui::pos2(sub_left, sy),
                        egui::pos2(sub_right, sy + sub_h),
                    );
                    let supported = track.supports_controller(kind);
                    let selected = self.selected_controller == Some((track.id, kind));
                    if cell.bottom() >= lane_top && cell.top() <= rect.bottom() {
                        if open_controller == Some(kind) || selected {
                            painter.rect_filled(
                                cell,
                                CornerRadius::from(0),
                                theme::with_alpha(theme::ACCENT, if selected { 34 } else { 22 }),
                            );
                        }
                        if selected {
                            painter.rect_stroke(
                                cell,
                                CornerRadius::from(0),
                                Stroke::new(1.2_f32, theme::with_alpha(theme::ACCENT, 180)),
                                StrokeKind::Inside,
                            );
                        }
                        draw_tab(cell.top(), cell.bottom());
                        let label = kind.label().to_string();
                        let text_color = if !supported {
                            theme::TEXT_FAINT
                        } else if open_controller == Some(kind) || selected {
                            theme::ACCENT
                        } else {
                            // Child-lane labels default to the main track color.
                            color
                        };
                        let mut job = egui::text::LayoutJob::simple(
                            label,
                            FontId::proportional(10.0),
                            text_color,
                            f32::INFINITY,
                        );
                        if !supported {
                            job.sections[0].format.strikethrough = Stroke::new(1.0_f32, text_color);
                        }
                        let galley = painter.layout_job(job);
                        painter.galley(
                            egui::pos2(sub_left + 14.0, cell.center().y - galley.size().y * 0.5),
                            galley,
                            text_color,
                        );
                        let close_rect = Rect::from_center_size(
                            egui::pos2(sub_right - 14.0, cell.center().y),
                            Vec2::new(16.0, 16.0),
                        );
                        painter.text(
                            close_rect.center(),
                            Align2::CENTER_CENTER,
                            "×",
                            FontId::proportional(11.0),
                            theme::TEXT_DIM,
                        );
                        remove_controller_rects.push((index, kind, close_rect));
                        painter.hline(
                            sub_left..=sub_right,
                            cell.bottom(),
                            Stroke::new(1.0_f32, theme::with_alpha(theme::LINE, 150)),
                        );
                    }
                    sy += sub_h;
                }
                let effects_cell =
                    Rect::from_min_max(egui::pos2(sub_left, sy), egui::pos2(sub_right, sy + sub_h));
                if effects_cell.bottom() >= lane_top && effects_cell.top() <= rect.bottom() {
                    let selected = self.selected_effects_lane == Some(track.id);
                    if selected {
                        painter.rect_filled(
                            effects_cell,
                            CornerRadius::from(0),
                            theme::with_alpha(theme::ACCENT, 26),
                        );
                        painter.rect_stroke(
                            effects_cell,
                            CornerRadius::from(0),
                            Stroke::new(1.2_f32, theme::with_alpha(theme::ACCENT, 170)),
                            StrokeKind::Inside,
                        );
                    }
                    draw_tab(effects_cell.top(), effects_cell.bottom());
                    painter.text(
                        egui::pos2(sub_left + 14.0, effects_cell.center().y),
                        Align2::LEFT_CENTER,
                        format!("效果器链 · {}", track.effects.len()),
                        FontId::proportional(10.0),
                        if selected { theme::ACCENT } else { color },
                    );
                }
            }
        }

        // 轨道列表末尾的「+ 添加轨道」行：工具栏不再提供加轨按钮。
        // Trailing "+ add track" row in the track list; the toolbar no longer offers one.
        let add_track_h = add_track_row_h(row_scale);
        let add_track_y = lane_top + content_height - add_track_h - scroll;
        if add_track_y + add_track_h >= lane_top && add_track_y <= rect.bottom() {
            let row_rect = Rect::from_min_max(
                egui::pos2(rect.left(), add_track_y),
                egui::pos2(lane_right, add_track_y + add_track_h),
            );
            let add_response = ui.interact(row_rect, ui.id().with("add_track_row"), Sense::click());
            let hovered = add_response.hovered();
            if hovered {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            painter.rect_filled(row_rect, CornerRadius::from(0), theme::BG);
            painter.hline(
                rect.left()..=lane_right,
                add_track_y,
                Stroke::new(1.0_f32, theme::with_alpha(theme::LINE, 180)),
            );
            let chip = Rect::from_min_max(
                egui::pos2(rect.left() + 8.0, add_track_y + 4.0),
                egui::pos2(lane_left - 8.0, add_track_y + add_track_h - 4.0),
            );
            painter.rect_filled(
                chip,
                CornerRadius::from(5),
                if hovered {
                    theme::PANEL_3
                } else {
                    theme::with_alpha(theme::ACCENT, 10)
                },
            );
            painter.text(
                chip.center(),
                Align2::CENTER_CENTER,
                "+ 添加 MIDI 轨道",
                FontId::proportional(11.5),
                theme::ACCENT,
            );
            // 加轨不再询问类型，直接创建默认 MIDI 合成器轨道。
            // Adding a track no longer asks for a type; it creates a default MIDI synth track.
            if add_response.clicked() {
                self.project
                    .add_track(crate::core::model::InstrumentRef::Synth(
                        crate::core::model::SynthSettings::default(),
                    ));
                self.selected_track = self.project.tracks.len() - 1;
                self.selected_clip = None;
                self.mark_dirty();
            }
        }

        // "+" controller picker and per-controller delete buttons.
        let mut pending_add: Option<(usize, ControllerKind)> = None;
        for (track_index, chip_rect) in add_controller_rects {
            let response = ui.interact(
                chip_rect,
                ui.id().with(("add_controller", track_index)),
                Sense::click(),
            );
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let existing = self
                .project
                .tracks
                .get(track_index)
                .map(|track| track.visible_controllers.clone())
                .unwrap_or_default();
            let supported: Vec<(ControllerKind, bool)> = self
                .project
                .tracks
                .get(track_index)
                .map(|track| {
                    ControllerKind::ALL
                        .iter()
                        .map(|kind| (*kind, track.supports_controller(*kind)))
                        .collect()
                })
                .unwrap_or_else(|| {
                    ControllerKind::ALL
                        .iter()
                        .map(|kind| (*kind, true))
                        .collect()
                });
            egui::Popup::menu(&response).show(|ui| {
                ui.label(
                    egui::RichText::new("添加控制器")
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                );
                for (kind, is_supported) in &supported {
                    if existing.contains(kind) {
                        continue;
                    }
                    if *is_supported {
                        if ui.button(kind.label()).clicked() {
                            pending_add = Some((track_index, *kind));
                        }
                    } else {
                        ui.add_enabled(
                            false,
                            egui::Button::new(
                                egui::RichText::new(kind.label())
                                    .color(theme::TEXT_FAINT)
                                    .strikethrough(),
                            ),
                        )
                        .on_disabled_hover_text("音频片段不支持该控制器");
                    }
                }
            });
        }
        if let Some((track_index, kind)) = pending_add {
            if let Some(track) = self.project.tracks.get_mut(track_index) {
                track.visible_controllers.push(kind);
                track.expanded = true;
                track.bump_revision();
            }
            self.mark_dirty();
        }

        let mut pending_remove: Option<(usize, ControllerKind)> = None;
        for (track_index, kind, close_rect) in remove_controller_rects {
            let response = ui.interact(
                close_rect.expand(2.0),
                ui.id().with(("remove_controller", track_index, kind)),
                Sense::click(),
            );
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                pending_remove = Some((track_index, kind));
            }
        }
        if let Some((track_index, kind)) = pending_remove {
            if let Some(track_id) = self.project.tracks.get(track_index).map(|track| track.id) {
                self.remove_controller(track_id, kind);
            }
        }

        // Volume handles on the L/R bars.
        for (track_index, bars) in volume_bar_rects {
            for (channel, bar) in bars.iter().enumerate() {
                let response = ui.interact(
                    *bar,
                    ui.id().with(("track_volume", track_index, channel)),
                    Sense::click_and_drag(),
                );
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
                if !pinching && (response.dragged() || response.clicked()) {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let fraction =
                            ((pos.x - bar.left()) / bar.width().max(1.0)).clamp(0.0, 1.0);
                        if let Some(track) = self.project.tracks.get_mut(track_index) {
                            track.volume = fraction * 1.5;
                        }
                        self.mark_dirty();
                    }
                }
            }
        }

        // Master volume lane: display-only, double-click opens the controller
        // roll where the automation can be edited.
        if let Some(rect) = master_volume_rect {
            let response = ui.interact(rect, ui.id().with("master_volume_lane"), Sense::click());
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.double_clicked() {
                self.bottom_view = BottomView::Controller {
                    track_id: crate::core::model::MASTER_TRACK_ID,
                    kind: ControllerKind::Volume,
                };
                self.bottom_open = true;
                self.controller_selection.clear();
                self.controller_marquee_start = None;
            }
        }
        if let Some(rect) = master_plus_rect {
            let response = ui.interact(rect, ui.id().with("master_add_effect"), Sense::click());
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                self.open_browser(crate::ui::PluginTarget::MasterEffect);
            }
        }
        for (track_index, rect) in add_effect_rects {
            let response = ui.interact(
                rect,
                ui.id().with(("track_add_effect", track_index)),
                Sense::click(),
            );
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                self.selected_track = track_index;
                self.open_browser(crate::ui::PluginTarget::Effect);
            }
        }
        if let Some(rect) = master_close_rect {
            let response = ui.interact(rect, ui.id().with("master_remove_effect"), Sense::click());
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                self.project.master_effect = None;
                self.close_editor(0, crate::audio::SlotKind::Master);
                self.mark_dirty();
            }
        }

        // Cut preview: a dashed line follows the pointer over a MIDI clip.
        if mode == MouseMode::Cut {
            if let Some(pos) = response.hover_pos() {
                if pos.x >= lane_left && pos.y >= lane_top {
                    if let Some((track_index, RowKind::Main, row_rect)) = row_rect_at(
                        &self.project,
                        pos,
                        lane_top,
                        scroll,
                        lane_left,
                        lane_width,
                        row_scale,
                    ) {
                        let beat = snap(x_to_beat(pos.x), self.snap_beats);
                        let inside = self
                            .project
                            .tracks
                            .get(track_index)
                            .map(|track| {
                                track
                                    .clips
                                    .iter()
                                    .any(|clip| beat > clip.start && beat < clip.end())
                            })
                            .unwrap_or(false);
                        if inside {
                            let x = beat_to_x(beat);
                            let mut y = row_rect.top() + 3.0;
                            let bottom = row_rect.bottom() - 3.0;
                            while y < bottom {
                                let end = (y + 5.0).min(bottom);
                                painter.line_segment(
                                    [egui::pos2(x, y), egui::pos2(x, end)],
                                    Stroke::new(1.6_f32, theme::AMBER),
                                );
                                y += 9.0;
                            }
                        }
                    }
                }
            }
        }

        // Separator below the pinned master block.
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(rect.left(), lane_top),
                egui::pos2(rect.right(), lane_top + 3.0),
            ),
            CornerRadius::from(0),
            theme::BG_DEEP,
        );

        // Playhead.
        let playhead_x = beat_to_x(playhead);
        if playhead_x >= lane_left && playhead_x <= lane_right {
            painter.line_segment(
                [
                    egui::pos2(playhead_x, ruler_top),
                    egui::pos2(playhead_x, rect.bottom()),
                ],
                Stroke::new(1.5_f32, theme::ACCENT),
            );
            painter.circle_filled(egui::pos2(playhead_x, ruler_top + 3.0), 4.0, theme::ACCENT);
        }

        // ---- interactions ----------------------------------------------------
        if self.show_master {
            if let Some(pos) = response.hover_pos() {
                if (pos.y - master_bottom).abs() <= 6.0 {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                }
            }
        }
        if response.hovered()
            && mode != MouseMode::Select
            && response
                .hover_pos()
                .map(|pos| pos.x >= lane_left && pos.y >= lane_top)
                .unwrap_or(false)
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }

        // Resize cursor when hovering a clip edge in select mode.
        if mode == MouseMode::Select {
            if let Some(pos) = response.hover_pos() {
                if pos.x >= lane_left && pos.y >= lane_top {
                    if let Some((track_index, RowKind::Main, _)) = row_rect_at(
                        &self.project,
                        pos,
                        lane_top,
                        scroll,
                        lane_left,
                        lane_width,
                        row_scale,
                    ) {
                        let beat = x_to_beat(pos.x);
                        let on_edge = self
                            .project
                            .tracks
                            .get(track_index)
                            .map(|track| {
                                track.clips.iter().any(|clip| {
                                    beat >= clip.start
                                        && beat <= clip.end()
                                        && ((pos.x - beat_to_x(clip.start)).abs() <= 7.0
                                            || (pos.x - beat_to_x(clip.end())).abs() <= 7.0)
                                })
                            })
                            .unwrap_or(false);
                        if on_edge {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                        }
                    }
                }
            }
        }

        if response.drag_started() && !pinching {
            let origin = ui
                .input(|i| i.pointer.press_origin())
                .or(response.interact_pointer_pos());
            if let Some(pos) = origin {
                if pos.y < ruler_bottom {
                    if start_flag_rect.expand(3.0).contains(pos) {
                        self.arr_drag = ArrangementDrag::LoopStart;
                    } else if end_flag_rect.expand(3.0).contains(pos) {
                        self.arr_drag = ArrangementDrag::LoopEnd;
                    } else {
                        self.arr_drag = ArrangementDrag::Ruler;
                        self.seek(snap(x_to_beat(pos.x), self.snap_beats).max(0.0));
                    }
                } else if pos.y < lane_top {
                    // Master bus bottom edge drags the bus height; anything else
                    // on the pinned master block is not draggable.
                    if self.show_master && (pos.y - master_bottom).abs() <= 6.0 {
                        self.arr_drag = ArrangementDrag::MasterResize;
                    } else {
                        self.arr_drag = ArrangementDrag::None;
                    }
                } else if pos.x >= lane_left {
                    if let Some((track_index, kind, _)) = row_rect_at(
                        &self.project,
                        pos,
                        lane_top,
                        scroll,
                        lane_left,
                        lane_width,
                        row_scale,
                    ) {
                        if kind == RowKind::Main {
                            self.selected_track = track_index;
                            self.selected_note = None;
                            self.selected_notes.clear();
                            self.selected_controller = None;
                            self.selected_effect = None;
                            self.selected_effects_lane = None;
                            let beat = x_to_beat(pos.x);
                            let hit = self.project.tracks.get(track_index).and_then(|track| {
                                track
                                    .clips
                                    .iter()
                                    .enumerate()
                                    .find(|(_, clip)| beat >= clip.start && beat <= clip.end())
                                    .map(|(index, clip)| (index, clip.clone()))
                            });
                            match mode {
                                MouseMode::Cut => {
                                    if let Some((clip_index, _)) = hit {
                                        let cut = snap(beat, self.snap_beats);
                                        if split_clip(
                                            &mut self.project,
                                            track_index,
                                            clip_index,
                                            cut,
                                        ) {
                                            self.selected_clip = Some(clip_index);
                                            self.mark_dirty();
                                            self.status = "已切割 MIDI 片段".into();
                                        }
                                    }
                                    self.arr_drag = ArrangementDrag::None;
                                }
                                MouseMode::Draw => {
                                    if let Some((clip_index, clip)) = hit {
                                        self.selected_clip = Some(clip_index);
                                        self.selected_clips = vec![(track_index, clip_index)];
                                        self.focus_piano_on_selection();
                                        self.arr_drag = ArrangementDrag::ClipMove {
                                            grab_beat: beat,
                                            group: vec![(track_index, clip_index, clip.start)],
                                        };
                                    } else if self
                                        .project
                                        .tracks
                                        .get(track_index)
                                        .map(|track| track.has_audio_clips())
                                        .unwrap_or(false)
                                    {
                                        self.status =
                                            "该轨道已有音频片段，无法创建 MIDI 片段".into();
                                        self.arr_drag = ArrangementDrag::None;
                                    } else {
                                        let start = snap(beat, self.snap_beats).max(0.0);
                                        let length = self.project.beats_per_bar().max(1.0);
                                        let id = self.project.next_clip_id();
                                        let mut new_index = None;
                                        if let Some(track) =
                                            self.project.tracks.get_mut(track_index)
                                        {
                                            track.clips.push(crate::core::model::Clip::new(
                                                id,
                                                "新片段",
                                                start,
                                                length,
                                            ));
                                            new_index = Some(track.clips.len() - 1);
                                        }
                                        if let Some(clip_index) = new_index {
                                            self.selected_clip = Some(clip_index);
                                            self.selected_clips = vec![(track_index, clip_index)];
                                            self.arr_drag = ArrangementDrag::CreateClip {
                                                track: track_index,
                                                clip: clip_index,
                                                start_beat: start,
                                            };
                                            self.mark_dirty();
                                            self.status = "已创建 MIDI 片段".into();
                                        } else {
                                            self.arr_drag = ArrangementDrag::None;
                                        }
                                    }
                                }
                                MouseMode::Select => {
                                    if let Some((clip_index, clip)) = hit {
                                        if !self.selected_clips.contains(&(track_index, clip_index))
                                        {
                                            self.selected_clips = vec![(track_index, clip_index)];
                                        }
                                        self.selected_clip = Some(clip_index);
                                        self.selected_track = track_index;
                                        self.focus_piano_on_selection();
                                        // Grabbed edge detection: left/right edges
                                        // resize, body drags.
                                        let x0 = beat_to_x(clip.start);
                                        let x1 = beat_to_x(clip.end());
                                        let left_edge = (pos.x - x0).abs() <= 7.0;
                                        let right_edge = (pos.x - x1).abs() <= 7.0;
                                        let group: Vec<(usize, usize, f64, f64)> = self
                                            .selected_clips
                                            .iter()
                                            .filter_map(|(t, c)| {
                                                let clip =
                                                    self.project.tracks.get(*t)?.clips.get(*c)?;
                                                Some((*t, *c, clip.start, clip.length))
                                            })
                                            .collect();
                                        if left_edge || right_edge {
                                            self.arr_drag = ArrangementDrag::ClipResize {
                                                left_edge,
                                                anchor: (track_index, clip_index),
                                                group,
                                            };
                                        } else {
                                            self.arr_drag = ArrangementDrag::ClipMove {
                                                grab_beat: beat,
                                                group: group
                                                    .into_iter()
                                                    .map(|(t, c, start, _)| (t, c, start))
                                                    .collect(),
                                            };
                                        }
                                    } else {
                                        self.selected_clip = None;
                                        self.selected_clips.clear();
                                        self.arr_drag = ArrangementDrag::Marquee { start: pos };
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else if response.dragged() && !pinching {
            let pointer = response.interact_pointer_pos();
            match self.arr_drag.clone() {
                ArrangementDrag::Ruler => {
                    if let Some(pos) = pointer {
                        self.seek(snap(x_to_beat(pos.x), self.snap_beats).max(0.0));
                    }
                }
                ArrangementDrag::LoopStart => {
                    if let Some(pos) = pointer {
                        let beat = snap(x_to_beat(pos.x), self.snap_beats).max(0.0);
                        self.project.loop_start = beat.min(self.project.loop_end - 0.25);
                        self.mark_dirty();
                    }
                }
                ArrangementDrag::LoopEnd => {
                    if let Some(pos) = pointer {
                        let beat = snap(x_to_beat(pos.x), self.snap_beats).max(0.25);
                        self.project.loop_end = beat.max(self.project.loop_start + 0.25);
                        self.mark_dirty();
                    }
                }
                ArrangementDrag::MasterResize => {
                    if let Some(pos) = pointer {
                        let max_total = (rect.bottom() - master_top - MASTER_GAP - 24.0)
                            .clamp(MASTER_MIN_H, MASTER_MAX_H);
                        self.master_height = (pos.y - master_top).clamp(MASTER_MIN_H, max_total);
                    }
                }
                ArrangementDrag::ClipMove { grab_beat, group } => {
                    if let Some(pos) = pointer {
                        let mut group = group;
                        // Single clips may hop to another track by dragging
                        // vertically; multi-track selections only slide in time.
                        if group.len() == 1 {
                            let (track, clip_index, orig_start) = group[0];
                            let target =
                                row_at(&self.project, pos.y - lane_top + scroll, row_scale)
                                    .filter(|(_, kind, _, _)| *kind == RowKind::Main)
                                    .map(|(index, _, _, _)| index)
                                    .unwrap_or(track);
                            if target != track {
                                // MIDI and audio clips may never share a lane:
                                // child-lane effectiveness would be ambiguous.
                                let moving_is_audio = self
                                    .project
                                    .tracks
                                    .get(track)
                                    .and_then(|track| track.clips.get(clip_index))
                                    .map(Clip::is_audio)
                                    .unwrap_or(false);
                                let allowed = self
                                    .project
                                    .tracks
                                    .get(track)
                                    .and_then(|track| track.clips.get(clip_index))
                                    .zip(self.project.tracks.get(target))
                                    .map(|(clip, dest)| dest.accepts_clip(clip))
                                    .unwrap_or(false);
                                if !allowed {
                                    self.status = if moving_is_audio {
                                        "目标轨道已有 MIDI 片段，无法拖入音频片段".into()
                                    } else {
                                        "目标轨道已有音频片段，无法拖入 MIDI 片段".into()
                                    };
                                } else {
                                    let moved =
                                        self.project.tracks.get_mut(track).and_then(|track| {
                                            if clip_index < track.clips.len() {
                                                Some(track.clips.remove(clip_index))
                                            } else {
                                                None
                                            }
                                        });
                                    if let Some(clip) = moved {
                                        let mut new_index = None;
                                        if let Some(dest) = self.project.tracks.get_mut(target) {
                                            dest.clips.push(clip);
                                            new_index = Some(dest.clips.len() - 1);
                                        }
                                        if let Some(new_index) = new_index {
                                            group = vec![(target, new_index, orig_start)];
                                            self.selected_track = target;
                                            self.selected_clip = Some(new_index);
                                            self.selected_clips = vec![(target, new_index)];
                                            self.arr_drag = ArrangementDrag::ClipMove {
                                                grab_beat,
                                                group: group.clone(),
                                            };
                                            self.mark_dirty();
                                        }
                                    }
                                }
                            }
                        }
                        let grid = self.snap_beats;
                        let delta = snap(x_to_beat(pos.x) - grab_beat, grid);
                        let mut changed = false;
                        for (track_index, clip_index, orig_start) in &group {
                            if let Some(clip) = self
                                .project
                                .tracks
                                .get_mut(*track_index)
                                .and_then(|track| track.clips.get_mut(*clip_index))
                            {
                                clip.start = (orig_start + delta).max(0.0);
                                clip.bump_revision();
                                changed = true;
                            }
                        }
                        if changed {
                            self.mark_dirty();
                        }
                    }
                }
                ArrangementDrag::ClipResize {
                    left_edge,
                    anchor,
                    group,
                } => {
                    if let Some(pos) = pointer {
                        let beat = x_to_beat(pos.x);
                        let grid = if self.snap_beats > 0.0 {
                            self.snap_beats
                        } else {
                            0.25
                        };
                        let anchor_orig = group
                            .iter()
                            .find(|(t, c, _, _)| (*t, *c) == anchor)
                            .copied();
                        if let Some((_, _, anchor_start, anchor_len)) = anchor_orig {
                            let mut changed = false;
                            for (track_index, clip_index, start, length) in &group {
                                if let Some(clip) = self
                                    .project
                                    .tracks
                                    .get_mut(*track_index)
                                    .and_then(|track| track.clips.get_mut(*clip_index))
                                {
                                    if left_edge {
                                        // Right edge stays put; the left edge
                                        // drags forward or backward.
                                        let delta = snap(beat, grid)
                                            .min(anchor_start + anchor_len - grid)
                                            .max(0.0)
                                            - anchor_start;
                                        let new_start = (start + delta).max(0.0);
                                        let new_len = (length - (new_start - start)).max(grid);
                                        // Trimming the front edge must not drag the
                                        // musical content along with it.
                                        let shift = new_start - clip.start;
                                        clip.start = new_start;
                                        clip.length = new_len.min(512.0);
                                        if shift.abs() > 1e-9 {
                                            for note in &mut clip.notes {
                                                note.start -= shift;
                                            }
                                        }
                                    } else {
                                        let new_anchor_len =
                                            snap(beat - anchor_start, grid).max(grid);
                                        let delta = new_anchor_len - anchor_len;
                                        clip.length = (length + delta).max(grid).min(512.0);
                                    }
                                    clip.bump_revision();
                                    changed = true;
                                }
                            }
                            if changed {
                                self.mark_dirty();
                            }
                        }
                    }
                }
                ArrangementDrag::CreateClip {
                    track,
                    clip,
                    start_beat,
                } => {
                    if let Some(pos) = pointer {
                        let grid = if self.snap_beats > 0.0 {
                            self.snap_beats
                        } else {
                            0.25
                        };
                        let end = snap(x_to_beat(pos.x), grid);
                        let length = (end - start_beat).max(grid).min(512.0);
                        let mut changed = false;
                        if let Some(clip) = self
                            .project
                            .tracks
                            .get_mut(track)
                            .and_then(|t| t.clips.get_mut(clip))
                        {
                            clip.length = length;
                            clip.bump_revision();
                            changed = true;
                        }
                        if changed {
                            self.mark_dirty();
                        }
                    }
                }
                ArrangementDrag::Marquee { start } => {
                    if let Some(pos) = pointer {
                        let band = Rect::from_two_pos(start, pos);
                        // Cross-track selection: any clip intersecting the band.
                        let mut selected: Vec<(usize, usize)> = Vec::new();
                        let mut offset = 0.0_f32;
                        for (track_index, track) in self.project.tracks.iter().enumerate() {
                            let block = block_height(track, row_scale);
                            let row_top = lane_top + offset - scroll;
                            offset += block;
                            let row_rect = Rect::from_min_max(
                                egui::pos2(lane_left, row_top),
                                egui::pos2(lane_right, row_top + main_h),
                            );
                            if !band.intersects(row_rect) {
                                continue;
                            }
                            for (clip_index, clip) in track.clips.iter().enumerate() {
                                let x0 = beat_to_x(clip.start);
                                let x1 = beat_to_x(clip.end());
                                let clip_rect = Rect::from_min_max(
                                    egui::pos2(x0, row_top),
                                    egui::pos2(x1, row_top + main_h),
                                );
                                if band.intersects(clip_rect) {
                                    selected.push((track_index, clip_index));
                                }
                            }
                        }
                        self.selected_clip = selected.last().map(|(_, clip)| *clip);
                        self.selected_clips = selected;
                        // Scroll the timeline while dragging near the edges.
                        let step = self.visible_beats * 0.02;
                        if pos.x < lane_left + 24.0 {
                            self.view_start = (self.view_start - step).max(0.0);
                            ui.ctx().request_repaint();
                        } else if pos.x > lane_right - 24.0 {
                            self.view_start += step;
                            ui.ctx().request_repaint();
                        }
                    }
                }
                ArrangementDrag::None => {}
            }
        }
        if response.drag_stopped() {
            self.arr_drag = ArrangementDrag::None;
        }

        let double_clicked = response.double_clicked();
        let single_clicked = response.clicked();
        if let Some(pos) = response.interact_pointer_pos() {
            if double_clicked || single_clicked {
                if pos.y < ruler_bottom {
                    if single_clicked && !toggle_rect.contains(pos) {
                        self.seek(snap(x_to_beat(pos.x), self.snap_beats).max(0.0));
                    }
                } else if pos.y < lane_top {
                    // Master main lane: volume controller. Master effect sub-lane:
                    // open its editor or the effect browser.
                    if double_clicked && pos.y < master_bottom {
                        if pos.y >= master_effect_top {
                            // Open the master effect chain roll.
                            self.bottom_view = BottomView::Effects {
                                track_id: crate::core::model::MASTER_TRACK_ID,
                            };
                            self.bottom_open = true;
                        } else {
                            self.bottom_view = BottomView::Controller {
                                track_id: crate::core::model::MASTER_TRACK_ID,
                                kind: ControllerKind::Volume,
                            };
                            self.bottom_open = true;
                            self.controller_selection.clear();
                            self.controller_marquee_start = None;
                        }
                    }
                } else if pos.x < lane_left {
                    if let Some((track_index, kind, row_rect)) = row_rect_at(
                        &self.project,
                        pos,
                        lane_top,
                        scroll,
                        lane_left,
                        lane_width,
                        row_scale,
                    ) {
                        let previous_track = self.selected_track;
                        self.selected_track = track_index;
                        if previous_track != track_index && kind != RowKind::Main {
                            // Selecting a child lane of another track makes the
                            // old clip/note selection invalid. The piano roll
                            // itself stays on its editing track.
                            self.selected_clip = None;
                            self.selected_clips.clear();
                            self.selected_note = None;
                            self.selected_notes.clear();
                        }
                        self.selected_note = None;
                        let track_id = self.project.tracks[track_index].id;
                        let row_y = row_rect.top();
                        let row_main_h = main_row_h(row_scale);
                        let chevron = Rect::from_min_size(
                            egui::pos2(rect.left() + 8.0, row_y + (row_main_h - 16.0) * 0.5),
                            Vec2::new(16.0, 16.0),
                        );
                        let controls_y = row_y + row_main_h - 25.0 * row_scale.clamp(0.55, 2.4);
                        let mute_rect = Rect::from_min_size(
                            egui::pos2(rect.left() + 28.0, controls_y),
                            Vec2::new(26.0, 18.0),
                        );
                        let solo_rect = Rect::from_min_size(
                            egui::pos2(rect.left() + 60.0, controls_y),
                            Vec2::new(26.0, 18.0),
                        );
                        // 乐器名称（副标题）区域：单击打开乐器窗口。
                        // Instrument name (subtitle) area: a single click opens the instrument window.
                        let subtitle_rect = Rect::from_min_size(
                            egui::pos2(rect.left() + 24.0, row_y + 23.0),
                            Vec2::new((lane_left - rect.left() - 28.0).max(40.0), 18.0),
                        );
                        if single_clicked
                            && kind == RowKind::Main
                            && row_main_h >= 64.0
                            && subtitle_rect.contains(pos)
                        {
                            self.open_instrument_picker(track_index);
                        } else if single_clicked && kind == RowKind::Main && chevron.contains(pos) {
                            if let Some(track) = self.project.tracks.get_mut(track_index) {
                                track.expanded = !track.expanded;
                            }
                            self.mark_dirty();
                        } else if single_clicked && kind == RowKind::Main && mute_rect.contains(pos)
                        {
                            if let Some(track) = self.project.tracks.get_mut(track_index) {
                                track.muted = !track.muted;
                            }
                            self.mark_dirty();
                        } else if single_clicked && kind == RowKind::Main && solo_rect.contains(pos)
                        {
                            if let Some(track) = self.project.tracks.get_mut(track_index) {
                                track.solo = !track.solo;
                            }
                            self.mark_dirty();
                        } else if double_clicked
                            && kind == RowKind::Main
                            && !chevron.contains(pos)
                            && !mute_rect.contains(pos)
                            && !solo_rect.contains(pos)
                        {
                            self.open_track_editor(track_index, None);
                        } else if double_clicked
                            && !matches!(kind, RowKind::Main | RowKind::AddController)
                        {
                            match kind {
                                RowKind::Controller(controller) => {
                                    self.selected_controller = Some((track_id, controller));
                                    self.selected_effects_lane = None;
                                    self.request_roll(
                                        track_id,
                                        crate::app::RollKind::Controller(controller),
                                    );
                                }
                                RowKind::Effects => {
                                    self.selected_controller = None;
                                    self.selected_effects_lane = Some(track_id);
                                    self.request_roll(track_id, crate::app::RollKind::Effects);
                                }
                                RowKind::Main | RowKind::AddController => {}
                            }
                            self.controller_selection.clear();
                            self.controller_marquee_start = None;
                        } else if single_clicked && !double_clicked {
                            // Single click only selects the child lane. It must
                            // not clear the clip/note selection, otherwise the
                            // piano roll loses its content on a plain click.
                            match kind {
                                RowKind::Controller(controller) => {
                                    self.selected_controller = Some((track_id, controller));
                                    self.selected_effect = None;
                                    self.selected_effects_lane = None;
                                }
                                RowKind::Effects => {
                                    self.selected_controller = None;
                                    self.selected_effect = None;
                                    self.selected_effects_lane = Some(track_id);
                                }
                                _ => {}
                            }
                        }
                    }
                } else if let Some((track_index, kind, _row_rect)) = row_rect_at(
                    &self.project,
                    pos,
                    lane_top,
                    scroll,
                    lane_left,
                    lane_width,
                    row_scale,
                ) {
                    let previous_track = self.selected_track;
                    self.selected_track = track_index;
                    if previous_track != track_index && kind != RowKind::Main {
                        // Keep the selection consistent when another track's
                        // child lane is selected.
                        self.selected_clip = None;
                        self.selected_clips.clear();
                        self.selected_note = None;
                        self.selected_notes.clear();
                    }
                    if single_clicked && !double_clicked && mode == MouseMode::Select {
                        // Select mode: clicking the lane moves the playhead.
                        self.seek(snap(x_to_beat(pos.x), self.snap_beats).max(0.0));
                    }
                    let track_id = self.project.tracks[track_index].id;
                    if double_clicked {
                        self.selected_note = None;
                        match kind {
                            RowKind::Main => {
                                let beat = snap(x_to_beat(pos.x), self.snap_beats).max(0.0);
                                let existing =
                                    self.project.tracks.get(track_index).and_then(|track| {
                                        track.clips.iter().position(|clip| {
                                            beat >= clip.start && beat < clip.end()
                                        })
                                    });
                                if let Some(clip_index) = existing {
                                    self.selected_clip = Some(clip_index);
                                    let is_audio = self
                                        .project
                                        .tracks
                                        .get(track_index)
                                        .and_then(|track| track.clips.get(clip_index))
                                        .map(Clip::is_audio)
                                        .unwrap_or(false);
                                    if is_audio {
                                        self.selected_track = track_index;
                                        self.status =
                                            "音频片段没有钢琴卷帘；可拖入其他音频或双击空白处创建 MIDI 片段"
                                                .into();
                                    } else {
                                        self.open_track_editor(track_index, Some(clip_index));
                                    }
                                } else {
                                    let track_id =
                                        self.project.tracks.get(track_index).map(|track| track.id);
                                    let detached = track_id
                                        .map(|id| self.has_roll_window(id))
                                        .unwrap_or(false);
                                    if detached {
                                        // Re-focus the detached roll instead of
                                        // creating a clip underneath it.
                                        self.open_track_editor(track_index, None);
                                    } else if self
                                        .project
                                        .tracks
                                        .get(track_index)
                                        .map(|track| track.has_audio_clips())
                                        .unwrap_or(false)
                                    {
                                        self.status =
                                            "该轨道已有音频片段，无法创建 MIDI 片段".into();
                                    } else {
                                        let id = self.project.next_clip_id();
                                        let length = self.project.beats_per_bar() * 2.0;
                                        let mut new_index = None;
                                        if let Some(track) =
                                            self.project.tracks.get_mut(track_index)
                                        {
                                            let mut clip = crate::core::model::Clip::new(
                                                id,
                                                "新片段",
                                                beat,
                                                length,
                                            );
                                            clip.revision = 1;
                                            track.clips.push(clip);
                                            new_index = Some(track.clips.len() - 1);
                                        }
                                        if let Some(new_index) = new_index {
                                            self.selected_clip = Some(new_index);
                                            self.mark_dirty();
                                            self.open_track_editor(track_index, Some(new_index));
                                            self.status =
                                                "已创建空白 MIDI 片段，双击钢琴卷帘添加音符".into();
                                        }
                                    }
                                }
                            }
                            RowKind::Controller(controller) => {
                                self.selected_clip = None;
                                self.selected_controller = Some((track_id, controller));
                                self.selected_effect = None;
                                self.selected_effects_lane = None;
                                self.request_roll(
                                    track_id,
                                    crate::app::RollKind::Controller(controller),
                                );
                            }
                            RowKind::Effects => {
                                self.selected_clip = None;
                                self.selected_controller = None;
                                self.selected_effects_lane = Some(track_id);
                                self.request_roll(track_id, crate::app::RollKind::Effects);
                            }
                            RowKind::AddController => {}
                        }
                        self.controller_selection.clear();
                        self.controller_marquee_start = None;
                    } else if kind == RowKind::Main {
                        self.selected_note = None;
                        self.selected_notes.clear();
                        self.selected_controller = None;
                        self.selected_effect = None;
                        self.selected_effects_lane = None;
                        let beat = x_to_beat(pos.x);
                        match mode {
                            MouseMode::Cut => {
                                let clip_index =
                                    self.project.tracks.get(track_index).and_then(|track| {
                                        track.clips.iter().position(|clip| {
                                            beat >= clip.start && beat <= clip.end()
                                        })
                                    });
                                if let Some(clip_index) = clip_index {
                                    let cut = snap(beat, self.snap_beats);
                                    if split_clip(&mut self.project, track_index, clip_index, cut) {
                                        self.selected_clip = Some(clip_index);
                                        self.mark_dirty();
                                        self.status = "已切割 MIDI 片段".into();
                                    }
                                }
                            }
                            MouseMode::Draw => {
                                let existing =
                                    self.project.tracks.get(track_index).and_then(|track| {
                                        track.clips.iter().position(|clip| {
                                            beat >= clip.start && beat <= clip.end()
                                        })
                                    });
                                if let Some(clip_index) = existing {
                                    self.selected_clip = Some(clip_index);
                                    self.selected_clips = vec![(track_index, clip_index)];
                                    self.focus_piano_on_selection();
                                } else if self
                                    .project
                                    .tracks
                                    .get(track_index)
                                    .map(|track| track.has_audio_clips())
                                    .unwrap_or(false)
                                {
                                    self.status = "该轨道已有音频片段，无法创建 MIDI 片段".into();
                                } else {
                                    let start = snap(beat, self.snap_beats).max(0.0);
                                    let length = self.project.beats_per_bar().max(1.0);
                                    let id = self.project.next_clip_id();
                                    let mut new_index = None;
                                    if let Some(track) = self.project.tracks.get_mut(track_index) {
                                        track.clips.push(crate::core::model::Clip::new(
                                            id,
                                            "新片段",
                                            start,
                                            length,
                                        ));
                                        new_index = Some(track.clips.len() - 1);
                                    }
                                    if let Some(clip_index) = new_index {
                                        self.selected_clip = Some(clip_index);
                                        self.selected_clips = vec![(track_index, clip_index)];
                                        self.mark_dirty();
                                        self.focus_piano_on_selection();
                                        self.status = "已创建 MIDI 片段".into();
                                    }
                                }
                            }
                            MouseMode::Select => {
                                let clip_index =
                                    self.project.tracks.get(track_index).and_then(|track| {
                                        track.clips.iter().position(|clip| {
                                            beat >= clip.start && beat <= clip.end()
                                        })
                                    });
                                self.selected_clip = clip_index;
                                self.selected_clips = clip_index
                                    .map(|clip| (track_index, clip))
                                    .into_iter()
                                    .collect();
                                if clip_index.is_some() {
                                    self.focus_piano_on_selection();
                                }
                            }
                        }
                    } else {
                        // Single click on a child lane only selects it; the
                        // clip/note selection (and thus the piano roll) stays.
                        match kind {
                            RowKind::Controller(controller) => {
                                self.selected_controller = Some((track_id, controller));
                                self.selected_effect = None;
                                self.selected_effects_lane = None;
                            }
                            RowKind::Effects => {
                                self.selected_controller = None;
                                self.selected_effect = None;
                                self.selected_effects_lane = Some(track_id);
                            }
                            RowKind::Main | RowKind::AddController => {}
                        }
                    }
                }
            }
        }

        // Dropping a WAV file on a main lane adds an audio clip. Dropping on
        // empty space creates a fresh audio track at the playhead.
        let dropped = ui.ctx().input(|i| i.raw.dropped_files.clone());
        if !dropped.is_empty() {
            let pointer = ui.ctx().input(|i| i.pointer.hover_pos());
            for file in dropped {
                let Some(path) = file.path else {
                    continue;
                };
                let extension = path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if extension != "wav" {
                    self.status = "仅支持拖入 WAV 音频文件".into();
                    continue;
                }
                let duration = match crate::audio::io::AudioRuntime::audio_duration_seconds(&path) {
                    Ok(duration) => duration,
                    Err(error) => {
                        self.status = error;
                        continue;
                    }
                };
                let target = pointer
                    .and_then(|pos| {
                        row_rect_at(
                            &self.project,
                            pos,
                            lane_top,
                            scroll,
                            lane_left,
                            lane_width,
                            row_scale,
                        )
                    })
                    .filter(|(_, kind, _)| *kind == RowKind::Main)
                    .map(|(index, _, _)| index);
                let start_beat = pointer
                    .map(|pos| snap(x_to_beat(pos.x), self.snap_beats))
                    .unwrap_or_else(|| snap(self.playhead(), self.snap_beats))
                    .max(0.0);
                let track_index = match target {
                    Some(index) => {
                        // A lane that already holds MIDI clips cannot also hold
                        // audio clips, otherwise child-lane effectiveness is
                        // ambiguous.
                        if self
                            .project
                            .tracks
                            .get(index)
                            .map(|track| track.has_midi_clips())
                            .unwrap_or(false)
                        {
                            self.status = "该轨道已有 MIDI 片段，无法拖入音频片段".into();
                            continue;
                        }
                        index
                    }
                    None => self.project.add_audio_track(),
                };
                let name = path
                    .file_stem()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_else(|| "音频片段".into());
                let length = (duration * self.project.bpm.max(1.0) / 60.0).max(0.25);
                let id = self.project.next_clip_id();
                let mut clip = Clip::new(id, &name, start_beat, length);
                clip.audio = Some(AudioSource {
                    path,
                    name,
                    length_seconds: duration,
                    offset_seconds: 0.0,
                });
                if let Some(track) = self.project.tracks.get_mut(track_index) {
                    track.clips.push(clip);
                    track.bump_revision();
                }
                self.selected_track = track_index;
                self.selected_clip = self
                    .project
                    .tracks
                    .get(track_index)
                    .and_then(|track| track.clips.len().checked_sub(1));
                self.selected_clips.clear();
                self.mark_dirty();
                self.status = format!("已添加音频片段 · {:.1} 秒", duration);
            }
        }

        // Rubber-band selection overlay.
        if let ArrangementDrag::Marquee { start } = &self.arr_drag {
            if let Some(pos) = response.interact_pointer_pos() {
                let band = Rect::from_two_pos(*start, pos).intersect(Rect::from_min_max(
                    egui::pos2(lane_left, lane_top),
                    rect.max,
                ));
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
    }
}
