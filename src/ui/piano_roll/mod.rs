//! 所属模块：ui / 钢琴卷帘
//! 文件用途：
//! - 在一个绝对时间轴上绘制并编辑选中轨道的全部 MIDI 片段。
//! - 底部力度控制器按音符显示与编辑力度，由工具条开关控制显示。
//!
//! 函数清单：
//! - `ui_piano_roll` / `ui_piano_roll_for`：卷帘入口与单轨绘制/交互。
//! - `add_note_at`：按片段范围规则创建或扩展音符。
//! - `piano_toolbar`：工具条（量化、删除、力度开关、调式与网格）。
//! - `piano_placeholder`：空状态。
//!
//! Module: ui / piano roll
//! Purpose:
//! - Draw and edit every MIDI clip of the selected track on one absolute timeline.
//! - The bottom velocity lane shows and edits per-note velocity, toggled from the toolbar.
//!
//! Functions:
//! - `ui_piano_roll` / `ui_piano_roll_for`: roll entry and per-track drawing/interaction.
//! - `add_note_at`: create or extend notes following the clip-range rules.
//! - `piano_toolbar`: toolbar (quantise, delete, velocity switch, scale and grid).
//! - `piano_placeholder`: empty state.

use crate::app::{PianoViewState, RollKind, SurfingApp};
use crate::core::model::Clip;
use crate::ui::{theme, MouseMode, PianoDrag};
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind};

mod helpers;
use helpers::*;

impl SurfingApp {
    pub fn ui_piano_roll(&mut self, ui: &mut egui::Ui) {
        // The roll stays on the track that was opened for editing. A single
        // click elsewhere only changes the selection and must not swap (and
        // visually empty) the roll; double-clicking a track sets `piano_track`.
        let track_id = self
            .piano_track
            .filter(|id| self.project.tracks.iter().any(|track| track.id == *id))
            .or_else(|| self.current_track_id());
        let Some(track_id) = track_id else {
            let panel_rect = ui.available_rect_before_wrap();
            self.piano_toolbar(ui, None, false, panel_rect);
            ui.add_space(4.0);
            self.piano_placeholder(ui);
            return;
        };
        self.piano_track = Some(track_id);
        self.ui_piano_roll_for(ui, track_id, false);
    }

    /// 所属模块：ui / 钢琴卷帘
    /// 功能：绘制单轨钢琴卷帘：键盘、标尺、网格、片段与音符，以及底部力度控制器。
    /// 输入：
    /// - `ui`：卷帘容器。
    /// - `track_id`：要显示的轨道 id。
    /// - `detached`：是否位于浮动窗口（隐藏拖出/关闭按钮）。
    /// 输出/副作用：写回该轨道的 `PianoViewState`；编辑音符、力度与选择，并标记脏状态。
    /// 复杂度：每帧 O(可见音符数 + 可见网格线数)。
    ///
    /// Module: ui / piano roll
    /// Purpose: Draw one track's piano roll: keyboard, ruler, grid, clips, notes and the bottom
    /// velocity lane.
    /// Inputs:
    /// - `ui`: roll container.
    /// - `track_id`: the track to show.
    /// - `detached`: whether this is a floating window (hides detach/close buttons).
    /// Outputs / side effects: writes the track's `PianoViewState`; edits notes, velocity and the
    /// selection, marking the project dirty.
    /// Complexity: O(visible notes + visible grid lines) per frame.
    pub fn ui_piano_roll_for(&mut self, ui: &mut egui::Ui, track_id: u64, detached: bool) {
        let panel_rect = ui.available_rect_before_wrap();
        self.piano_toolbar(ui, Some(track_id), detached, panel_rect);
        ui.add_space(4.0);

        let Some(track_index) = self.project.tracks.iter().position(|t| t.id == track_id) else {
            self.piano_placeholder(ui);
            return;
        };
        let Some(track) = self.project.tracks.get(track_index) else {
            self.piano_placeholder(ui);
            return;
        };
        // Restore this track's own scroll/zoom. Opening or editing another track
        // must never change the view of a roll that is already open.
        let view = self.piano_views.get(&track_id).copied().unwrap_or_default();
        self.piano_zoom = view.zoom;
        self.piano_view_beat = view.view_beat;
        self.piano_top_pitch = view.top_pitch;
        self.piano_note_h = view.note_h;
        let track_color = theme::track_color(track.color);
        // Snapshot of every clip on the track (absolute timeline).
        let clips: Vec<(usize, Clip)> = track
            .clips
            .iter()
            .enumerate()
            .map(|(index, clip)| (index, clip.clone()))
            .collect();
        let content_end = clips
            .iter()
            .map(|(_, clip)| clip.end())
            .fold(0.0_f64, f64::max)
            .max(16.0);

        let available = ui.available_size();
        if available.y < 40.0 || available.x < 120.0 {
            return;
        }
        let (response, painter) = ui.allocate_painter(available, Sense::click_and_drag());
        let rect = response.rect;
        // Effective pointer tool: holding Shift temporarily switches to Draw.
        let mode = if ui.input(|i| i.modifiers.shift) {
            MouseMode::Draw
        } else {
            self.mouse_mode
        };
        let grid_left = rect.left() + KEYBOARD_W;
        let grid_width = (rect.right() - grid_left).max(20.0);
        let grid_top = rect.top() + RULER_H;
        // 底部力度控制器只在开关打开且面板足够高时占用空间，避免压扁主网格。
        // The bottom velocity lane only takes space when enabled and the panel is tall enough, so
        // the main grid is never squeezed.
        let lane_h = if self.piano_velocity_open && rect.height() >= 200.0 {
            VELOCITY_LANE_H.min(rect.height() * 0.4)
        } else {
            0.0
        };
        let grid_bottom = rect.bottom() - lane_h;

        let ppp = ui.ctx().pixels_per_point();
        let snap_px = |value: f32| (value * ppp).round() / ppp;

        // Wheel: plain scroll moves pitches, Shift pans, Ctrl zooms horizontally,
        // Ctrl+Shift zooms rows vertically.
        if response.hovered() {
            // Wheel deltas: some platforms report Shift+wheel on the horizontal
            // axis, others on the vertical one, so accept both.
            let (event_scroll, event_ctrl, event_shift) = ui.input(|i| {
                let mut scroll = 0.0_f32;
                let mut ctrl = false;
                let mut shift = false;
                for event in &i.events {
                    if let egui::Event::MouseWheel {
                        delta, modifiers, ..
                    } = event
                    {
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
            let (scroll, ctrl, shift) = if event_scroll.abs() > 0.1 {
                // Whole-line wheel events are scaled to points.
                (event_scroll * 50.0, event_ctrl, event_shift)
            } else {
                let smooth = if shift_now {
                    smooth_y + smooth_x
                } else {
                    smooth_y
                };
                (smooth, ctrl_now, shift_now)
            };
            if scroll.abs() > 0.1 {
                let pointer = response.hover_pos().unwrap_or(rect.center());
                if ctrl && shift {
                    // Ctrl+Shift zooms horizontally.
                    let visible_old = (content_end / self.piano_zoom.max(1.0) as f64).max(0.25);
                    let ppb_old = grid_width as f64 / visible_old;
                    let anchor_beat =
                        self.piano_view_beat + ((pointer.x - grid_left) as f64 / ppb_old).max(0.0);
                    self.piano_zoom =
                        (self.piano_zoom * (1.0 + scroll as f32 * 0.002)).clamp(1.0, 16.0);
                    let visible_new = (content_end / self.piano_zoom as f64).max(0.25);
                    let fraction = ((pointer.x - grid_left) / grid_width).clamp(0.0, 1.0) as f64;
                    self.piano_view_beat =
                        (anchor_beat - fraction * visible_new).clamp(0.0, content_end.max(1.0));
                } else if ctrl {
                    // Ctrl zooms the rows vertically.
                    let old_h = self.piano_note_h.max(4.0);
                    let new_h = (old_h * (1.0 + scroll * 0.0015)).clamp(9.0, 32.0);
                    let anchor = (pointer.y - grid_top) / old_h;
                    let top = self.piano_top_pitch as f32 - anchor + (pointer.y - grid_top) / new_h;
                    self.piano_note_h = new_h;
                    self.piano_top_pitch = (top.round() as i32).clamp(36, 120);
                } else if shift {
                    let visible = (content_end / self.piano_zoom.max(1.0) as f64).max(0.25);
                    let ppb = grid_width as f64 / visible;
                    self.piano_view_beat = (self.piano_view_beat - scroll as f64 / ppb)
                        .clamp(0.0, content_end.max(1.0));
                } else {
                    let note_h = self.piano_note_h.max(4.0);
                    self.piano_top_pitch =
                        (self.piano_top_pitch + (scroll / note_h) as i32).clamp(24, 120);
                }
            }
        }

        // 触摸屏双指捏合：以捏合中心为锚点横向缩放卷帘；桌面没有多点触控，不受影响。
        // Pinch on touch screens: zoom the roll horizontally, anchored at the pinch centre; desktop
        // has no multi-touch, so it is unaffected.
        let pinch = ui.input(|i| {
            i.multi_touch()
                .map(|multi| (multi.zoom_delta, multi.center_pos))
        });
        // 捏合期间必须停掉单指拖动，否则第二根手指落下前已经开始的拖动会继续移动音符。
        // Stop single-pointer dragging while pinching: a drag started by the first finger would
        // otherwise keep moving notes.
        let pinching = pinch.is_some();
        if pinching {
            self.piano_drag = PianoDrag::None;
        }
        if let Some((zoom, center)) = pinch {
            if (zoom - 1.0).abs() > 1e-3 && grid_width > 1.0 && rect.contains(center) {
                let visible_old = (content_end / self.piano_zoom.max(1.0) as f64).max(0.25);
                let ppb_old = grid_width as f64 / visible_old;
                let anchor_beat =
                    self.piano_view_beat + ((center.x - grid_left) as f64 / ppb_old).max(0.0);
                self.piano_zoom = (self.piano_zoom * zoom).clamp(1.0, 16.0);
                let visible_new = (content_end / self.piano_zoom as f64).max(0.25);
                let fraction = ((center.x - grid_left) / grid_width).clamp(0.0, 1.0) as f64;
                self.piano_view_beat =
                    (anchor_beat - fraction * visible_new).clamp(0.0, content_end.max(1.0));
            }
        }

        let note_h = if self.piano_note_h > 4.0 {
            self.piano_note_h
        } else {
            DEFAULT_NOTE_H
        };
        let visible_beats = (content_end / self.piano_zoom.max(1.0) as f64).max(0.25);
        let view_beat = self.piano_view_beat.clamp(0.0, content_end.max(1.0));
        let px_per_beat = (grid_width / visible_beats as f32).max(6.0);
        let beat_to_x = |beat: f64| grid_left + (beat - view_beat) as f32 * px_per_beat;
        let x_to_beat = |x: f32| view_beat + ((x - grid_left) / px_per_beat) as f64;
        let top_pitch = self.piano_top_pitch;
        let scale_root = self.scale_root;
        let scale_mode = self.scale_mode;
        let pitch_to_y = |pitch: u8| grid_top + (top_pitch - pitch as i32) as f32 * note_h;
        let y_to_pitch = |y: f32| -> u8 {
            let row = ((y - grid_top) / note_h).floor() as i32;
            (top_pitch - row).clamp(0, 127) as u8
        };

        painter.rect_filled(rect, CornerRadius::from(6), theme::BG_DEEP);

        // ---- keyboard --------------------------------------------------------
        painter.rect_filled(
            Rect::from_min_max(rect.min, egui::pos2(grid_left, grid_bottom)),
            CornerRadius::from(0),
            theme::PANEL,
        );
        for pitch in 0..=127u8 {
            let y0 = snap_px(pitch_to_y(pitch));
            let y1 = snap_px(y0 + note_h);
            if y1 < grid_top || y0 > grid_bottom {
                continue;
            }
            let row = Rect::from_min_max(
                egui::pos2(rect.left(), y0),
                egui::pos2(rect.left() + KEYBOARD_W, y1),
            );
            let black = is_black_key(pitch);
            let lit = in_scale(pitch, scale_root, scale_mode);
            let fill = match (black, lit) {
                (false, true) => theme::TEXT,
                (false, false) => theme::TEXT_DIM,
                (true, true) => theme::PANEL_3,
                (true, false) => theme::PANEL_2,
            };
            painter.rect_filled(row, CornerRadius::from(0), fill);
            if black {
                let black_key = Rect::from_min_max(
                    row.min,
                    egui::pos2(row.left() + KEYBOARD_W * 0.62, row.bottom() - 0.6),
                );
                painter.rect_filled(black_key, CornerRadius::from(1), theme::BG_DEEP);
            }
            painter.hline(
                row.x_range(),
                row.bottom(),
                Stroke::new(if black { 1.0_f32 } else { 0.6_f32 }, theme::BG_DEEP),
            );
            if !black {
                let name = NOTE_NAMES[(pitch % 12) as usize];
                let label = if pitch % 12 == 0 {
                    format!("{name}{}", pitch as i32 / 12 - 1)
                } else {
                    name.to_string()
                };
                painter.text(
                    egui::pos2(rect.left() + KEYBOARD_W - 6.0, row.center().y),
                    Align2::RIGHT_CENTER,
                    label,
                    FontId::monospace(9.0),
                    theme::BG_DEEP,
                );
            }
        }

        // ---- ruler -----------------------------------------------------------
        painter.rect_filled(
            Rect::from_min_max(rect.min, egui::pos2(rect.right(), grid_top)),
            CornerRadius::from(0),
            theme::PANEL_2,
        );
        painter.hline(
            grid_left..=rect.right(),
            grid_top,
            Stroke::new(1.0_f32, theme::LINE),
        );

        // ---- scale-aware key rows -------------------------------------------
        let scale_degree = |pitch: u8| (pitch as i32 - scale_root).rem_euclid(12) as usize;
        for pitch in 0..=127u8 {
            let y0 = snap_px(pitch_to_y(pitch));
            let y1 = snap_px(y0 + note_h);
            if y1 < grid_top || y0 > grid_bottom {
                continue;
            }
            let lit = in_scale(pitch, scale_root, scale_mode);
            let is_root = scale_degree(pitch) == 0;
            let fill = if lit {
                if is_root {
                    theme::with_alpha(theme::ACCENT, 30)
                } else {
                    theme::with_alpha(theme::ACCENT, 14)
                }
            } else {
                theme::BG_DEEP
            };
            painter.rect_filled(
                Rect::from_min_max(egui::pos2(grid_left, y0), egui::pos2(rect.right(), y1)),
                CornerRadius::from(0),
                fill,
            );
            painter.hline(
                grid_left..=rect.right(),
                y1,
                Stroke::new(
                    0.7_f32,
                    theme::with_alpha(theme::LINE, if lit { 90 } else { 35 }),
                ),
            );
        }

        // ---- grid ------------------------------------------------------------
        let subdivision = if self.snap_beats > 0.0 {
            self.snap_beats.min(0.25)
        } else {
            0.25
        };
        let beats_per_bar = self.project.beats_per_bar();
        let first_beat = view_beat.floor();
        let mut beat = first_beat;
        while beat <= view_beat + visible_beats + 1e-9 {
            let x = beat_to_x(beat);
            let is_bar = (beat % beats_per_bar).abs() < 1e-6;
            let is_beat = (beat - beat.round()).abs() < 1e-6;
            let stroke = if is_bar {
                Stroke::new(1.0_f32, theme::with_alpha(theme::LINE, 220))
            } else if is_beat {
                Stroke::new(1.0_f32, theme::with_alpha(theme::LINE, 120))
            } else {
                Stroke::new(1.0_f32, theme::with_alpha(theme::LINE, 45))
            };
            painter.vline(x, grid_top..=grid_bottom, stroke);
            if is_bar {
                painter.text(
                    egui::pos2(x + 3.0, rect.top() + 4.0),
                    Align2::LEFT_TOP,
                    format!("{}", (beat / beats_per_bar).floor() as i64),
                    FontId::monospace(9.0),
                    theme::TEXT_DIM,
                );
            }
            beat += subdivision.max(0.0625);
        }

        // ---- clip regions ----------------------------------------------------
        for (clip_index, clip) in &clips {
            let x0 = beat_to_x(clip.start);
            let x1 = beat_to_x(clip.end());
            if x1 < grid_left || x0 > rect.right() {
                continue;
            }
            let clip_color = clip.color.map(theme::palette_color).unwrap_or(track_color);
            let band = Rect::from_min_max(
                egui::pos2(x0.max(grid_left), grid_top),
                egui::pos2(x1.min(rect.right()), grid_bottom),
            );
            painter.rect_filled(
                band,
                CornerRadius::from(0),
                theme::with_alpha(clip_color, 12),
            );
            for x in [x0, x1] {
                painter.vline(
                    x,
                    grid_top..=grid_bottom,
                    Stroke::new(1.0_f32, theme::with_alpha(clip_color, 150)),
                );
            }
            if band.width() > 44.0 {
                painter.text(
                    egui::pos2(band.left() + 4.0, grid_top + 4.0),
                    Align2::LEFT_TOP,
                    format!("{} · {}", clip_index + 1, clip.name),
                    FontId::proportional(9.0),
                    theme::with_alpha(clip_color, 220),
                );
            }
        }

        // ---- notes -----------------------------------------------------------
        let mut hit_note: Option<(usize, usize, Rect, NoteEdge)> = None;
        let hover_pos = response.hover_pos();
        for (clip_index, clip) in &clips {
            let clip_color = clip.color.map(theme::palette_color).unwrap_or(track_color);
            for (note_index, note) in clip.notes.iter().enumerate() {
                let abs_start = clip.start + note.start;
                let abs_end = clip.start + note.end();
                if abs_end < view_beat || abs_start > view_beat + visible_beats {
                    continue;
                }
                let y = pitch_to_y(note.pitch);
                if y + note_h < grid_top || y > grid_bottom {
                    continue;
                }
                let x0 = beat_to_x(abs_start).max(grid_left);
                let x1 = beat_to_x(abs_end).min(rect.right()).max(x0 + 3.0);
                let note_rect =
                    Rect::from_min_max(egui::pos2(x0, y + 1.0), egui::pos2(x1, y + note_h - 1.0));
                let selected = self.selected_note == Some((*clip_index, note_index))
                    || self.selected_notes.contains(&(*clip_index, note_index));
                painter.rect_filled(note_rect, CornerRadius::from(2), clip_color);
                painter.rect_stroke(
                    note_rect,
                    CornerRadius::from(2),
                    Stroke::new(
                        if selected { 1.9_f32 } else { 1.0_f32 },
                        if selected {
                            Color32::WHITE
                        } else {
                            theme::with_alpha(clip_color, 255)
                        },
                    ),
                    StrokeKind::Inside,
                );
                if note_rect.width() > 26.0 {
                    painter.text(
                        egui::pos2(note_rect.left() + 4.0, note_rect.center().y),
                        Align2::LEFT_CENTER,
                        format!("{:.0}", note.velocity),
                        FontId::monospace(8.0),
                        theme::BG_DEEP,
                    );
                }
                if let Some(pos) = hover_pos {
                    if note_rect.contains(pos) {
                        let edge = if (note_rect.left() - pos.x).abs() <= 6.0 {
                            NoteEdge::Left
                        } else if (note_rect.right() - pos.x).abs() <= 6.0 {
                            NoteEdge::Right
                        } else {
                            NoteEdge::Body
                        };
                        hit_note = Some((*clip_index, note_index, note_rect, edge));
                    }
                }
            }
        }

        // ---- velocity lane ---------------------------------------------------
        // 力度控制器固定在卷帘底部：每根竖线对应一个音符的起始位置，圆点高度即力度，
        // 圆点右侧再画一条与音符等长的横线，便于在力度视图里直接看出时值。
        // The velocity lane is pinned to the bottom: each stem marks a note start, the knob height
        // is its velocity, and a bar as long as the note trails the knob so durations stay visible
        // in the velocity view.
        let lane_top = grid_bottom + VELOCITY_LANE_PAD;
        let lane_bottom = rect.bottom() - VELOCITY_LANE_PAD;
        let lane_span = (lane_bottom - lane_top).max(1.0);
        let velocity_to_y =
            |velocity: u8| lane_bottom - (velocity.clamp(1, 127) as f32 / 127.0) * lane_span;
        let velocity_from_y = |y: f32| -> u8 {
            let ratio = ((lane_bottom - y) / lane_span).clamp(0.0, 1.0);
            (1.0 + ratio * 126.0).round().clamp(1.0, 127.0) as u8
        };
        let velocity_hit = |pos: egui::Pos2| -> Option<(usize, usize)> {
            let mut best: Option<(f32, usize, usize)> = None;
            for (clip_index, clip) in &clips {
                for (note_index, note) in clip.notes.iter().enumerate() {
                    let x = beat_to_x(clip.start + note.start);
                    let distance = (x - pos.x).abs();
                    if distance <= VELOCITY_HIT_W
                        && best
                            .map(|(closest, _, _)| distance < closest)
                            .unwrap_or(true)
                    {
                        best = Some((distance, *clip_index, note_index));
                    }
                }
            }
            best.map(|(_, clip_index, note_index)| (clip_index, note_index))
        };
        let mut lane_hover: Option<(u8, f32)> = None;
        if lane_h > 0.0 {
            let lane_rect = Rect::from_min_max(egui::pos2(rect.left(), grid_bottom), rect.max);
            painter.rect_filled(lane_rect, CornerRadius::from(0), theme::BG_DEEP);
            painter.rect_filled(
                Rect::from_min_max(lane_rect.min, egui::pos2(grid_left, lane_rect.bottom())),
                CornerRadius::from(0),
                theme::PANEL,
            );
            painter.hline(
                rect.x_range(),
                grid_bottom,
                Stroke::new(1.0_f32, theme::with_alpha(theme::LINE, 220)),
            );
            for fraction in [0.25_f32, 0.5, 0.75] {
                let y = lane_bottom - fraction * lane_span;
                painter.hline(
                    grid_left..=rect.right(),
                    y,
                    Stroke::new(1.0_f32, theme::with_alpha(theme::LINE, 90)),
                );
            }
            painter.text(
                egui::pos2(rect.left() + 8.0, lane_rect.top() + 7.0),
                Align2::LEFT_TOP,
                "力度",
                FontId::proportional(10.0),
                theme::TEXT_DIM,
            );
            for (clip_index, clip) in &clips {
                for (note_index, note) in clip.notes.iter().enumerate() {
                    let x = beat_to_x(clip.start + note.start);
                    if x < grid_left - 2.0 || x > rect.right() + 2.0 {
                        continue;
                    }
                    let y = velocity_to_y(note.velocity);
                    let selected = self.selected_note == Some((*clip_index, note_index))
                        || self.selected_notes.contains(&(*clip_index, note_index));
                    let color = if selected {
                        theme::ACCENT
                    } else {
                        theme::TEXT_DIM
                    };
                    painter.vline(
                        x,
                        y..=lane_bottom,
                        Stroke::new(
                            1.1_f32,
                            theme::with_alpha(color, if selected { 220 } else { 130 }),
                        ),
                    );
                    // 圆点后方的横线与音符等长；越过视口右缘时按可见宽度裁切。
                    // The bar behind the knob matches the note length, clipped at the visible edge.
                    let bar_start = x.max(grid_left);
                    let bar_end = beat_to_x(clip.start + note.end()).min(rect.right());
                    if bar_end > bar_start {
                        painter.hline(
                            bar_start..=bar_end,
                            y,
                            Stroke::new(
                                VELOCITY_BAR_H,
                                theme::with_alpha(color, if selected { 200 } else { 110 }),
                            ),
                        );
                    }
                    painter.circle_filled(egui::pos2(x, y), VELOCITY_KNOB_R, color);
                    if let Some(pos) = hover_pos {
                        if pos.y >= grid_bottom && (pos.x - x).abs() <= VELOCITY_HIT_W {
                            lane_hover = Some((note.velocity, y));
                        }
                    }
                }
            }
            // 左侧栏显示悬停/选中音符的力度，并在对应高度画一条参考线。
            // The left gutter shows the hovered/selected velocity with a matching guide line.
            let selected_velocity = self
                .selected_note
                .and_then(|(clip_index, note_index)| {
                    clips
                        .iter()
                        .find(|(index, _)| *index == clip_index)
                        .and_then(|(_, clip)| clip.notes.get(note_index))
                })
                .map(|note| note.velocity);
            let readout = lane_hover
                .or_else(|| selected_velocity.map(|velocity| (velocity, velocity_to_y(velocity))));
            if let Some((velocity, y)) = readout {
                painter.hline(
                    grid_left..=rect.right(),
                    y,
                    Stroke::new(1.0_f32, theme::with_alpha(theme::ACCENT, 70)),
                );
                painter.text(
                    egui::pos2(rect.left() + 8.0, y),
                    Align2::LEFT_CENTER,
                    format!("{velocity}"),
                    FontId::monospace(10.0),
                    theme::TEXT_DIM,
                );
            }
        }

        // ---- playhead --------------------------------------------------------
        let playing = self.playhead();
        if playing >= view_beat && playing <= view_beat + visible_beats {
            let x = beat_to_x(playing);
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                Stroke::new(1.5_f32, theme::ACCENT),
            );
        }

        // ---- interactions ----------------------------------------------------
        let pitch_under = |y: f32| -> u8 { y_to_pitch(y) };
        let snap_beats = self.snap_beats;

        // 标尺：点击或拖动小节条直接切换播放位置。
        // Ruler: clicking or dragging the bar strip moves the playhead.
        if let Some(pos) = response.interact_pointer_pos() {
            if pos.y < grid_top && pos.x >= grid_left {
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if !pinching && (response.clicked() || response.dragged()) {
                    self.seek(x_to_beat(pos.x).max(0.0));
                }
            }
        }

        if response.is_pointer_button_down_on() {
            if let Some(pos) = response.interact_pointer_pos() {
                if pos.x < grid_left && pos.y >= grid_top && pos.y < grid_bottom {
                    let pitch = pitch_under(pos.y);
                    if self.virtual_key_note != Some(pitch) {
                        if let Some(previous) = self.virtual_key_note.take() {
                            self.audition_off(previous);
                        }
                        self.virtual_key_note = Some(pitch);
                        self.audition_on(pitch, 100);
                    }
                }
            }
        } else if let Some(previous) = self.virtual_key_note.take() {
            self.audition_off(previous);
        }

        if response.secondary_clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if pos.x >= grid_left && pos.y >= grid_top && pos.y < grid_bottom {
                    if let Some((clip_index, note_index, _, _)) = hit_note {
                        self.push_undo_now();
                        if let Some(track) = self.project.tracks.get_mut(track_index) {
                            if let Some(clip) = track.clips.get_mut(clip_index) {
                                if note_index < clip.notes.len() {
                                    clip.notes.remove(note_index);
                                    clip.bump_revision();
                                    self.selected_note = None;
                                    self.selected_notes.clear();
                                    self.mark_dirty();
                                    self.status = "已删除音符".into();
                                }
                            }
                        }
                    }
                }
            }
        }

        // Cut preview: a full-height dashed line follows the pointer.
        if mode == MouseMode::Cut {
            if let Some(pos) = response.hover_pos() {
                if pos.x >= grid_left && pos.y >= grid_top && pos.y < grid_bottom {
                    let beat = snap(x_to_beat(pos.x), snap_beats);
                    let x = beat_to_x(beat);
                    let mut y = grid_top;
                    while y < grid_bottom {
                        let end = (y + 5.0).min(grid_bottom);
                        painter.line_segment(
                            [egui::pos2(x, y), egui::pos2(x, end)],
                            Stroke::new(1.6_f32, theme::AMBER),
                        );
                        y += 9.0;
                    }
                }
            }
        }

        if response.hovered() && mode != MouseMode::Select {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        // Note edges show a horizontal-resize cursor (select mode).
        if response.hovered()
            && mode == MouseMode::Select
            && matches!(
                hit_note.map(|(_, _, _, edge)| edge),
                Some(NoteEdge::Left | NoteEdge::Right)
            )
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }

        let note_rect_of = |clip: &Clip, note: &crate::core::model::Note| {
            let y = pitch_to_y(note.pitch);
            let x0 = beat_to_x(clip.start + note.start);
            let x1 = beat_to_x(clip.start + note.end()).max(x0 + 3.0);
            Rect::from_min_max(egui::pos2(x0, y + 1.0), egui::pos2(x1, y + note_h - 1.0))
        };
        let hit_at = |pos: egui::Pos2| -> Option<(usize, usize, Rect, NoteEdge)> {
            for (clip_index, clip) in clips.iter().rev() {
                for (note_index, note) in clip.notes.iter().enumerate().rev() {
                    let note_rect = note_rect_of(clip, note);
                    if note_rect.contains(pos) {
                        let edge = if (note_rect.left() - pos.x).abs() <= 6.0 {
                            NoteEdge::Left
                        } else if (note_rect.right() - pos.x).abs() <= 6.0 {
                            NoteEdge::Right
                        } else {
                            NoteEdge::Body
                        };
                        return Some((*clip_index, note_index, note_rect, edge));
                    }
                }
            }
            None
        };

        // Long-press and drag empty grid space to detach the roll into its own
        // window. Pressing a note keeps the normal edit gesture.
        if !detached {
            let press_on_note = ui
                .input(|i| i.pointer.press_origin())
                .map(|origin| hit_at(origin).is_some())
                .unwrap_or(false);
            let canvas = Rect::from_min_max(
                egui::pos2(grid_left, grid_top),
                egui::pos2(rect.right(), grid_bottom),
            );
            if self.try_detach_roll(ui, canvas, track_id, RollKind::PianoRoll, press_on_note) {
                return;
            }
        }

        let mut cut_note_hit: Option<(usize, usize, f64)> = None;
        if response.drag_started() && !pinching {
            if let Some(pos) = response.interact_pointer_pos() {
                if pos.x >= grid_left && pos.y >= grid_top && pos.y < grid_bottom {
                    let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(pos);
                    let pointer_beat = x_to_beat(origin.x);
                    let hit = hit_at(origin);
                    if mode == MouseMode::Cut {
                        cut_note_hit = hit
                            .map(|(clip, note, _, _)| (clip, note, snap(pointer_beat, snap_beats)));
                    } else if let Some((clip_index, note_index, _, edge)) = hit {
                        if !self.selected_notes.contains(&(clip_index, note_index)) {
                            self.selected_notes = vec![(clip_index, note_index)];
                        }
                        self.selected_note = Some((clip_index, note_index));
                        self.selected_clip = Some(clip_index);
                        // Multi-selection resize: anchor first, then the rest.
                        let mut group: Vec<(usize, usize, f64, f64)> = Vec::new();
                        for (c, n) in self.selected_notes.clone() {
                            if let Some(note) = clips
                                .iter()
                                .find(|(index, _)| *index == c)
                                .and_then(|(_, clip)| clip.notes.get(n))
                            {
                                let clip_start = clips
                                    .iter()
                                    .find(|(index, _)| *index == c)
                                    .map(|(_, clip)| clip.start)
                                    .unwrap_or(0.0);
                                group.push((c, n, clip_start + note.start, note.length));
                            }
                        }
                        match edge {
                            NoteEdge::Left | NoteEdge::Right => {
                                self.piano_drag = PianoDrag::Resize {
                                    left_edge: edge == NoteEdge::Left,
                                    anchor: (clip_index, note_index),
                                    group,
                                };
                            }
                            NoteEdge::Body => {
                                let anchor_abs = group
                                    .iter()
                                    .find(|(c, n, _, _)| (*c, *n) == (clip_index, note_index))
                                    .map(|(_, _, start, _)| *start)
                                    .unwrap_or(pointer_beat);
                                let anchor_pitch = clips
                                    .iter()
                                    .find(|(index, _)| *index == clip_index)
                                    .and_then(|(_, clip)| clip.notes.get(note_index))
                                    .map(|note| note.pitch)
                                    .unwrap_or(60);
                                self.piano_drag = PianoDrag::Move {
                                    grab_offset: pointer_beat - anchor_abs,
                                    anchor_start: anchor_abs,
                                    anchor_pitch,
                                    group: group
                                        .into_iter()
                                        .map(|(c, n, start, _)| (c, n, start, 0))
                                        .collect(),
                                };
                                // Fill pitches per note.
                                if let PianoDrag::Move { group, .. } = &mut self.piano_drag {
                                    for entry in group.iter_mut() {
                                        if let Some(note) = clips
                                            .iter()
                                            .find(|(index, _)| *index == entry.0)
                                            .and_then(|(_, clip)| clip.notes.get(entry.1))
                                        {
                                            entry.3 = note.pitch;
                                        }
                                    }
                                }
                            }
                        }
                    } else if mode == MouseMode::Draw {
                        let pitch = pitch_under(origin.y);
                        let abs = snap(pointer_beat, snap_beats).max(0.0);
                        if let Some((clip_index, note_index)) =
                            self.add_note_at(track_index, abs, pitch, snap_beats)
                        {
                            self.selected_note = Some((clip_index, note_index));
                            self.selected_clip = Some(clip_index);
                            self.selected_notes = vec![(clip_index, note_index)];
                            self.piano_drag = PianoDrag::Create {
                                clip: clip_index,
                                note: note_index,
                                start: abs,
                            };
                            self.mark_dirty();
                        }
                    } else {
                        self.selected_note = None;
                        self.selected_notes.clear();
                        self.piano_drag = PianoDrag::Marquee { start: pos };
                    }
                }
            }
        } else if response.dragged() && !pinching {
            if let Some(pos) = response.interact_pointer_pos() {
                let pointer_beat = x_to_beat(pos.x);
                let pitch = pitch_under(pos.y);
                match self.piano_drag.clone() {
                    PianoDrag::Create { clip, note, start } => {
                        let mut changed = false;
                        if let Some(target) = self
                            .project
                            .tracks
                            .get_mut(track_index)
                            .and_then(|t| t.clips.get_mut(clip))
                        {
                            if let Some(note) = target.notes.get_mut(note) {
                                let max_len = (target.length - note.start).max(0.25);
                                let length = snap(pointer_beat - start, snap_beats.max(0.0625))
                                    .max(0.25)
                                    .min(max_len);
                                note.length = length;
                                changed = true;
                            }
                        }
                        if changed {
                            self.bump_clip_revision(track_index, clip);
                        }
                    }
                    PianoDrag::Move {
                        grab_offset,
                        anchor_start,
                        anchor_pitch,
                        group,
                    } => {
                        let delta_beat =
                            snap(pointer_beat - grab_offset, snap_beats) - anchor_start;
                        let delta_pitch = pitch as i32 - anchor_pitch as i32;
                        let mut touched: Vec<usize> = Vec::new();
                        if let Some(track) = self.project.tracks.get_mut(track_index) {
                            for (clip_index, note_index, start, note_pitch) in &group {
                                let (clip_len, clip_start) = track
                                    .clips
                                    .get(*clip_index)
                                    .map(|clip| (clip.length, clip.start))
                                    .unwrap_or((0.0, 0.0));
                                if let Some(note) = track
                                    .clips
                                    .get_mut(*clip_index)
                                    .and_then(|clip| clip.notes.get_mut(*note_index))
                                {
                                    let rel = (*start + delta_beat - clip_start)
                                        .clamp(0.0, (clip_len - note.length).max(0.0));
                                    note.start = rel;
                                    note.pitch =
                                        ((*note_pitch as i32 + delta_pitch).clamp(0, 127)) as u8;
                                    if !touched.contains(clip_index) {
                                        touched.push(*clip_index);
                                    }
                                }
                            }
                        }
                        for clip_index in touched {
                            self.bump_clip_revision(track_index, clip_index);
                        }
                    }
                    PianoDrag::Resize {
                        left_edge,
                        anchor,
                        group,
                    } => {
                        let anchor_orig = group
                            .iter()
                            .find(|(c, n, _, _)| (*c, *n) == anchor)
                            .copied();
                        if let Some((_, _, anchor_start, anchor_len)) = anchor_orig {
                            let mut touched: Vec<usize> = Vec::new();
                            if let Some(track) = self.project.tracks.get_mut(track_index) {
                                for (clip_index, note_index, start, length) in &group {
                                    let clip_start = track
                                        .clips
                                        .get(*clip_index)
                                        .map(|clip| clip.start)
                                        .unwrap_or(0.0);
                                    if let Some(note) = track
                                        .clips
                                        .get_mut(*clip_index)
                                        .and_then(|clip| clip.notes.get_mut(*note_index))
                                    {
                                        if left_edge {
                                            // Right edge stays put while the left
                                            // edge drags.
                                            let delta = snap(pointer_beat, snap_beats)
                                                .min(anchor_start + anchor_len - 0.25)
                                                - anchor_start;
                                            let new_abs = (*start + delta).max(clip_start);
                                            let new_len = (*length - (new_abs - *start)).max(0.25);
                                            note.start = new_abs - clip_start;
                                            note.length = new_len;
                                        } else {
                                            let new_anchor_len = snap(
                                                pointer_beat - anchor_start,
                                                snap_beats.max(0.0625),
                                            )
                                            .max(0.25);
                                            let delta = new_anchor_len - anchor_len;
                                            note.length = (*length + delta).max(0.25);
                                        }
                                        if !touched.contains(clip_index) {
                                            touched.push(*clip_index);
                                        }
                                    }
                                }
                            }
                            for clip_index in touched {
                                self.bump_clip_revision(track_index, clip_index);
                            }
                        }
                    }
                    PianoDrag::Marquee { start } => {
                        let band = Rect::from_two_pos(start, pos).intersect(Rect::from_min_max(
                            egui::pos2(grid_left, grid_top),
                            egui::pos2(rect.right(), grid_bottom),
                        ));
                        let mut selected: Vec<(usize, usize)> = Vec::new();
                        for (clip_index, clip) in &clips {
                            for (note_index, note) in clip.notes.iter().enumerate() {
                                if band.intersects(note_rect_of(clip, note)) {
                                    selected.push((*clip_index, note_index));
                                }
                            }
                        }
                        self.selected_note = selected.last().copied();
                        if let Some((clip, _)) = selected.last() {
                            self.selected_clip = Some(*clip);
                        }
                        self.selected_notes = selected;
                        // Scroll the roll while dragging near the edges.
                        let step = visible_beats * 0.02;
                        if pos.x < grid_left + 20.0 {
                            self.piano_view_beat = (self.piano_view_beat - step).max(0.0);
                            ui.ctx().request_repaint();
                        } else if pos.x > rect.right() - 20.0 {
                            self.piano_view_beat =
                                (self.piano_view_beat + step).min(content_end.max(1.0));
                            ui.ctx().request_repaint();
                        }
                    }
                    PianoDrag::None => {}
                }
            }
        }
        if response.drag_stopped() {
            self.piano_drag = PianoDrag::None;
        }

        if let Some((clip_index, note_index, cut_abs)) = cut_note_hit {
            let mut changed = false;
            if let Some(track) = self.project.tracks.get_mut(track_index) {
                if let Some(clip) = track.clips.get_mut(clip_index) {
                    changed = split_note(clip, note_index, cut_abs - clip.start);
                }
            }
            if changed {
                self.selected_note = None;
                self.selected_notes.clear();
                self.bump_clip_revision(track_index, clip_index);
                self.mark_dirty();
                self.status = "已切割音符".into();
            }
        }

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if pos.x >= grid_left && pos.y >= grid_top && pos.y < grid_bottom {
                    if mode == MouseMode::Select {
                        // Select mode: clicking moves the playhead.
                        self.seek(x_to_beat(pos.x).max(0.0));
                    }
                    let hit = hit_at(pos);
                    match mode {
                        MouseMode::Cut => {
                            if let Some((clip_index, note_index, _, _)) = hit {
                                let cut = snap(x_to_beat(pos.x), snap_beats);
                                let mut changed = false;
                                if let Some(track) = self.project.tracks.get_mut(track_index) {
                                    if let Some(clip) = track.clips.get_mut(clip_index) {
                                        changed = split_note(clip, note_index, cut - clip.start);
                                    }
                                }
                                if changed {
                                    self.selected_note = None;
                                    self.selected_notes.clear();
                                    self.bump_clip_revision(track_index, clip_index);
                                    self.mark_dirty();
                                    self.status = "已切割音符".into();
                                }
                            }
                        }
                        MouseMode::Draw => {
                            if let Some((clip_index, note_index, _, _)) = hit {
                                self.selected_note = Some((clip_index, note_index));
                                self.selected_clip = Some(clip_index);
                                self.selected_notes = vec![(clip_index, note_index)];
                            } else {
                                let pitch = pitch_under(pos.y);
                                let abs = snap(x_to_beat(pos.x), snap_beats).max(0.0);
                                if let Some((clip_index, note_index)) =
                                    self.add_note_at(track_index, abs, pitch, snap_beats)
                                {
                                    self.selected_note = Some((clip_index, note_index));
                                    self.selected_clip = Some(clip_index);
                                    self.selected_notes = vec![(clip_index, note_index)];
                                    self.mark_dirty();
                                }
                            }
                        }
                        MouseMode::Select => match hit {
                            Some((clip_index, note_index, _, _)) => {
                                self.selected_note = Some((clip_index, note_index));
                                self.selected_clip = Some(clip_index);
                                self.selected_notes = vec![(clip_index, note_index)];
                            }
                            None => {
                                self.selected_note = None;
                                self.selected_notes.clear();
                            }
                        },
                    }
                }
            }
        }

        // ---- velocity lane interactions --------------------------------------
        // 在力度控制器内按下/拖动：命中最近的音符起始位置并按其纵坐标改写力度。
        // Press/drag inside the velocity lane: grab the nearest note start and set its velocity from
        // the pointer's vertical position.
        if lane_h > 0.0 {
            let lane_pos = response
                .interact_pointer_pos()
                .filter(|pos| pos.y >= grid_bottom);
            if let Some(pos) = lane_pos {
                if let Some((clip_index, note_index)) = velocity_hit(pos) {
                    let velocity = velocity_from_y(pos.y);
                    let mut changed = false;
                    if let Some(note) = self
                        .project
                        .tracks
                        .get_mut(track_index)
                        .and_then(|track| track.clips.get_mut(clip_index))
                        .and_then(|clip| clip.notes.get_mut(note_index))
                    {
                        if note.velocity != velocity {
                            note.velocity = velocity;
                            changed = true;
                        }
                    }
                    if changed {
                        self.bump_clip_revision(track_index, clip_index);
                    }
                    self.selected_note = Some((clip_index, note_index));
                    self.selected_notes = vec![(clip_index, note_index)];
                    self.selected_clip = Some(clip_index);
                }
            }
        }

        // Rubber-band selection overlay.
        if let PianoDrag::Marquee { start } = &self.piano_drag {
            if let Some(pos) = response.interact_pointer_pos() {
                let band = Rect::from_two_pos(*start, pos).intersect(Rect::from_min_max(
                    egui::pos2(grid_left, grid_top),
                    egui::pos2(rect.right(), grid_bottom),
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

        painter.rect_stroke(
            rect,
            CornerRadius::from(6),
            Stroke::new(1.0_f32, theme::LINE),
            StrokeKind::Inside,
        );
        self.piano_views.insert(
            track_id,
            PianoViewState {
                zoom: self.piano_zoom,
                view_beat: self.piano_view_beat,
                top_pitch: self.piano_top_pitch,
                note_h: self.piano_note_h,
            },
        );
    }

    /// Place a note at an absolute beat on a track. Notes dropped just outside a
    /// clip (within one bar) extend that clip instead of being rejected; far
    /// from any clip a fresh one-bar clip is created.
    fn add_note_at(
        &mut self,
        track_index: usize,
        abs_beat: f64,
        pitch: u8,
        snap_beats: f64,
    ) -> Option<(usize, usize)> {
        let bar = self.project.beats_per_bar().max(1.0);
        let note_len = if snap_beats > 0.0 {
            snap_beats.max(0.25).min(4.0)
        } else {
            0.5
        };
        let start = if self.snap_beats > 0.0 {
            (abs_beat / self.snap_beats).floor() * self.snap_beats
        } else {
            abs_beat.floor()
        };
        let id = self.project.next_clip_id();
        if self
            .project
            .tracks
            .get(track_index)
            .map(|track| track.has_audio_clips())
            .unwrap_or(false)
        {
            self.status = "该轨道已有音频片段，无法创建 MIDI 片段".into();
            return None;
        }
        self.push_undo_now();
        let track = self.project.tracks.get_mut(track_index)?;
        // 1. Inside an existing clip?
        for (clip_index, clip) in track.clips.iter_mut().enumerate() {
            if abs_beat >= clip.start && abs_beat < clip.end() {
                let index = clip.notes.len();
                clip.notes.push(crate::core::model::Note {
                    start: abs_beat - clip.start,
                    length: note_len.min((clip.length - (abs_beat - clip.start)).max(0.25)),
                    pitch,
                    velocity: 100,
                });
                clip.bump_revision();
                return Some((clip_index, index));
            }
        }
        // 2. Within one bar behind a clip → extend it forward.
        let mut best: Option<(usize, f64)> = None;
        for (clip_index, clip) in track.clips.iter().enumerate() {
            let gap = abs_beat - clip.end();
            if gap >= -1e-6 && gap <= bar {
                if best.map(|(_, d)| gap < d).unwrap_or(true) {
                    best = Some((clip_index, gap));
                }
            }
        }
        if let Some((clip_index, _)) = best {
            let clip = &mut track.clips[clip_index];
            clip.length = (abs_beat + note_len - clip.start).max(clip.length);
            clip.bump_revision();
            let index = clip.notes.len();
            clip.notes.push(crate::core::model::Note {
                start: abs_beat - clip.start,
                length: note_len,
                pitch,
                velocity: 100,
            });
            return Some((clip_index, index));
        }
        // 3. Within one bar ahead of a clip → extend it backward.
        let mut best: Option<(usize, f64)> = None;
        for (clip_index, clip) in track.clips.iter().enumerate() {
            let gap = clip.start - (abs_beat + note_len);
            if gap >= -1e-6 && gap <= bar {
                if best.map(|(_, d)| gap < d).unwrap_or(true) {
                    best = Some((clip_index, gap));
                }
            }
        }
        if let Some((clip_index, _)) = best {
            let clip = &mut track.clips[clip_index];
            let delta = clip.start - abs_beat;
            for note in &mut clip.notes {
                note.start += delta;
            }
            clip.start = abs_beat;
            clip.length += delta;
            clip.bump_revision();
            let index = clip.notes.len();
            clip.notes.push(crate::core::model::Note {
                start: 0.0,
                length: note_len,
                pitch,
                velocity: 100,
            });
            return Some((clip_index, index));
        }
        // 4. Nothing nearby: start a fresh one-bar clip.
        let mut clip = Clip::new(id, "新片段", start, bar.max(note_len));
        clip.revision = 1;
        clip.notes.push(crate::core::model::Note {
            start: abs_beat - start,
            length: note_len,
            pitch,
            velocity: 100,
        });
        track.clips.push(clip);
        Some((track.clips.len() - 1, 0))
    }

    /// 所属模块：ui / 钢琴卷帘 / 工具条
    /// 功能：绘制卷帘工具条：量化、删除音符、力度显示开关、调式与网格，并处理拖出窗口手势。
    /// 输入：
    /// - `ui`：卷帘容器。
    /// - `track_id`：当前轨道（无轨道时为空）。
    /// - `detached`：是否位于浮动窗口。
    /// - `panel_rect`：拖出窗口时用于计算初始位置的面板矩形。
    /// 输出/副作用：修改量化、力度显示、调式与吸附设置；可请求拖出浮动窗口。
    /// 复杂度：每帧 O(选中音符数)（量化时）。
    ///
    /// Module: ui / piano roll / toolbar
    /// Purpose: Draw the roll toolbar: quantise, delete, velocity-lane switch, scale and grid, and
    /// handle the detach gesture.
    /// Inputs:
    /// - `ui`: roll container.
    /// - `track_id`: current track (empty when there is none).
    /// - `detached`: whether this is a floating window.
    /// - `panel_rect`: panel rect used to place a detached window.
    /// Outputs / side effects: edits quantise, velocity visibility, scale and snap settings; may
    /// detach the roll into a floating window.
    /// Complexity: O(selected notes) per frame while quantising.
    fn piano_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        track_id: Option<u64>,
        detached: bool,
        panel_rect: egui::Rect,
    ) {
        let track_index =
            track_id.and_then(|id| self.project.tracks.iter().position(|track| track.id == id));
        let track_color = track_index
            .map(|index| theme::track_color(self.project.tracks[index].color))
            .unwrap_or(theme::ACCENT);
        let mut controls: Vec<egui::Rect> = Vec::new();
        let row = ui
            .horizontal(|ui| {
                if !detached && track_id.is_some() {
                    // Drag the title out of the bottom panel to detach it into its
                    // own OS window.
                    let response = ui.add(
                        egui::Label::new(
                            egui::RichText::new("⠿ 钢琴卷帘")
                                .color(track_color)
                                .strong()
                                .size(13.0),
                        )
                        .sense(egui::Sense::click_and_drag()),
                    );
                    controls.push(response.rect);
                    if response.drag_started() {
                        if let Some(track_id) = track_id {
                            self.begin_roll_drag(ui, track_id, RollKind::PianoRoll, panel_rect);
                        }
                    }
                    response.on_hover_text("拖动工具条可把卷帘拖出为独立窗口");
                } else {
                    ui.label(
                        egui::RichText::new("钢琴卷帘")
                            .color(track_color)
                            .strong()
                            .size(13.0),
                    );
                }
                if let Some(track) = track_index.and_then(|index| self.project.tracks.get(index)) {
                    let name = format!(
                        "{} · {} 个 MIDI 片段",
                        track.name,
                        track.clips.iter().filter(|clip| !clip.is_audio()).count()
                    );
                    ui.label(egui::RichText::new(name).color(theme::TEXT_DIM).size(11.0));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !detached {
                        let close = ui.small_button("×").on_hover_text("关闭卷帘");
                        controls.push(close.rect);
                        if close.clicked() {
                            self.bottom_open = false;
                        }
                    }
                    // 右上角开关：控制底部力度控制器是否显示。
                    // Top-right switch: show or hide the bottom velocity lane.
                    let was_open = self.piano_velocity_open;
                    let mut velocity_open = was_open;
                    let toggle = theme::toggle_switch(ui, &mut velocity_open);
                    controls.push(toggle.rect);
                    toggle.on_hover_text(if was_open {
                        "隐藏底部力度控制器"
                    } else {
                        "显示底部力度控制器"
                    });
                    if velocity_open != was_open {
                        self.piano_velocity_open = velocity_open;
                    }
                    ui.label(
                        egui::RichText::new("力度")
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );
                    let quantize = ui
                        .button("量化")
                        .on_hover_text("按当前网格量化音符起始位置");
                    controls.push(quantize.rect);
                    if quantize.clicked() {
                        let grid = self.snap_beats.max(0.0625);
                        let mut targets: Vec<(usize, usize)> = if self.selected_notes.is_empty() {
                            track_index
                                .and_then(|index| self.project.tracks.get(index))
                                .map(|track| {
                                    track
                                        .clips
                                        .iter()
                                        .enumerate()
                                        .flat_map(|(clip_index, clip)| {
                                            (0..clip.notes.len())
                                                .map(move |note_index| (clip_index, note_index))
                                        })
                                        .collect()
                                })
                                .unwrap_or_default()
                        } else {
                            self.selected_notes.clone()
                        };
                        targets.sort_unstable();
                        targets.dedup();
                        if !targets.is_empty() {
                            self.push_undo_now();
                            let mut touched: Vec<usize> = Vec::new();
                            if let Some(index) = track_index {
                                if let Some(track) = self.project.tracks.get_mut(index) {
                                    for (clip_index, note_index) in &targets {
                                        if let Some(clip) = track.clips.get_mut(*clip_index) {
                                            if let Some(note) = clip.notes.get_mut(*note_index) {
                                                note.start = (note.start / grid).round() * grid;
                                                let max_start =
                                                    (clip.length - note.length).max(0.0);
                                                note.start = note.start.clamp(0.0, max_start);
                                                if !touched.contains(clip_index) {
                                                    touched.push(*clip_index);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some(index) = track_index {
                                for clip_index in touched {
                                    self.bump_clip_revision(index, clip_index);
                                }
                            }
                            self.mark_dirty();
                            self.status = "已量化音符".into();
                        }
                    }

                    if let Some((clip_index, note_index)) = self.selected_note {
                        let note = track_index
                            .and_then(|index| self.project.tracks.get(index))
                            .and_then(|track| track.clips.get(clip_index))
                            .and_then(|clip| clip.notes.get(note_index))
                            .cloned();
                        if let Some(note) = note {
                            let mut velocity = note.velocity as i32;
                            let slider = ui.add(
                                egui::Slider::new(&mut velocity, 1..=127)
                                    .text("力度")
                                    .show_value(true),
                            );
                            controls.push(slider.rect);
                            if slider.changed() {
                                if let Some(index) = track_index {
                                    if let Some(track) = self.project.tracks.get_mut(index) {
                                        if let Some(clip) = track.clips.get_mut(clip_index) {
                                            if let Some(note) = clip.notes.get_mut(note_index) {
                                                note.velocity = velocity.clamp(1, 127) as u8;
                                            }
                                        }
                                    }
                                    self.bump_clip_revision(index, clip_index);
                                }
                            }
                        }
                    }

                    let scale_mode = egui::ComboBox::from_id_salt("scale_mode")
                        .width(96.0)
                        .selected_text(SCALES[self.scale_mode.min(SCALES.len() - 1)].0)
                        .show_ui(ui, |ui| {
                            for (index, (name, _)) in SCALES.iter().enumerate() {
                                if ui
                                    .selectable_label(self.scale_mode == index, *name)
                                    .clicked()
                                {
                                    self.scale_mode = index;
                                }
                            }
                        });
                    controls.push(scale_mode.response.rect);
                    let scale_root = egui::ComboBox::from_id_salt("scale_root")
                        .width(52.0)
                        .selected_text(NOTE_NAMES[self.scale_root.rem_euclid(12) as usize])
                        .show_ui(ui, |ui| {
                            for (index, name) in NOTE_NAMES.iter().enumerate() {
                                if ui
                                    .selectable_label(self.scale_root == index as i32, *name)
                                    .clicked()
                                {
                                    self.scale_root = index as i32;
                                }
                            }
                        });
                    controls.push(scale_root.response.rect);
                    ui.label(
                        egui::RichText::new("调式")
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );

                    let label = snap_label(self.snap_beats);
                    let snap = egui::ComboBox::from_id_salt("snap_grid")
                        .width(70.0)
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            for value in [0.25, 0.5, 1.0, 2.0, 0.0] {
                                if ui
                                    .selectable_label(
                                        (self.snap_beats - value).abs() < 1e-9,
                                        snap_label(value),
                                    )
                                    .clicked()
                                {
                                    self.snap_beats = value;
                                }
                            }
                        });
                    controls.push(snap.response.rect);
                    ui.label(
                        egui::RichText::new("网格")
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );
                });
            })
            .response;
        self.roll_toolbar_drag(
            ui,
            row.rect,
            &controls,
            track_id,
            RollKind::PianoRoll,
            panel_rect,
            !detached,
        );
    }

    fn piano_placeholder(&mut self, ui: &mut egui::Ui) {
        let size = ui.available_size();
        let (response, painter) = ui.allocate_painter(size, Sense::hover());
        painter.rect_filled(response.rect, CornerRadius::from(6), theme::BG_DEEP);
    }
}
