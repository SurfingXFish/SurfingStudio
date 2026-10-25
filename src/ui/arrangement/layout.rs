//! 所属模块：ui::arrangement / 布局与绘制
//! 文件用途：
//! - 提供编曲区的行高、命中测试、吸附与绘制辅助。
//! - 绘制音频波形、音量条并实现片段切分。
//!
//! 函数清单：
//! - `main_row_h` / `sub_row_h` / `add_row_h`：行高计算。
//! - `sub_rows` / `block_height` / `total_height`：轨道块高度。
//! - `row_at` / `row_rect_at` / `track_header_width`：命中测试与头部宽度。
//! - `snap`：网格吸附。
//! - `draw_audio_waveform` / `draw_volume_bars`：波形与音量绘制。
//! - `split_clip`：在指定 beat 切分片段。
//!
//! Module: ui::arrangement / layout and painting
//! Purpose:
//! - Provide row heights, hit testing, snapping and painting helpers.
//! - Draw audio waveforms, volume bars and split clips.
//!
//! Functions:
//! - `main_row_h` / `sub_row_h` / `add_row_h`: row heights.
//! - `sub_rows` / `block_height` / `total_height`: track block heights.
//! - `row_at` / `row_rect_at` / `track_header_width`: hit testing and header width.
//! - `snap`: grid snapping.
//! - `draw_audio_waveform` / `draw_volume_bars`: waveform and volume painting.
//! - `split_clip`: split a clip at a beat position.

use crate::core::model::{Clip, ControllerKind, Project};
use crate::ui::theme;
use eframe::egui::{self, Align2, FontId, Rect, Stroke, Vec2};

pub(super) const HEADER_W: f32 = 196.0;
pub(super) const RULER_H: f32 = 30.0;
pub(super) const MAIN_ROW_H: f32 = 72.0;
pub(super) const SUB_ROW_H: f32 = 33.0;
pub(super) const ADD_ROW_H: f32 = 30.0;
/// 轨道列表末尾「+ 添加轨道」行的高度（逻辑点）。
/// Height of the trailing "+ add track" row in the track list (logical points).
pub(super) const ADD_TRACK_ROW_H: f32 = 34.0;
/// Visible divider between the pinned master bus and the instrument tracks.
pub(super) const MASTER_GAP: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowKind {
    Main,
    AddController,
    Controller(ControllerKind),
    Effects,
}

pub(super) fn main_row_h(scale: f32) -> f32 {
    (MAIN_ROW_H * scale).max(52.0)
}

pub(super) fn sub_row_h(scale: f32) -> f32 {
    (SUB_ROW_H * scale).max(23.0)
}

pub(super) fn add_row_h(scale: f32) -> f32 {
    (ADD_ROW_H * scale).max(16.0)
}

/// 所属模块：ui::arrangement / 布局
/// 功能：返回轨道列表末尾「+ 添加轨道」行的高度。
/// 输入：`scale` 行高缩放系数。
/// 输出/副作用：高度（逻辑点）；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui::arrangement / layout
/// Purpose: Return the height of the trailing "+ add track" row.
/// Inputs: `scale` — row scale factor.
/// Outputs / side effects: height in logical points; no state change.
/// Complexity: O(1) time and space.
pub(super) fn add_track_row_h(scale: f32) -> f32 {
    (ADD_TRACK_ROW_H * scale).max(26.0)
}

/// Height of the master effect-chain sub-lane. Fixed so the master bus does
/// not follow the track row zoom.
pub(super) const MASTER_SUB_H: f32 = 30.0;
pub(super) const MASTER_MIN_H: f32 = 75.0;
pub(super) const MASTER_MAX_H: f32 = 320.0;

/// Number of sub-rows: the "+ add controller" row, visible controllers and the
/// effect chain row.
pub(super) fn sub_rows(track: &crate::core::model::Track) -> usize {
    track.visible_controllers.len() + 2
}

pub(super) fn block_height(track: &crate::core::model::Track, scale: f32) -> f32 {
    if track.expanded {
        main_row_h(scale) + add_row_h(scale) + sub_row_h(scale) * (sub_rows(track) - 1) as f32
    } else {
        main_row_h(scale)
    }
}

pub(super) fn total_height(project: &Project, scale: f32) -> f32 {
    project
        .tracks
        .iter()
        .map(|track| block_height(track, scale))
        .sum::<f32>()
        + add_track_row_h(scale)
}

pub(super) fn row_at(project: &Project, y: f32, scale: f32) -> Option<(usize, RowKind, f32, f32)> {
    if y < 0.0 {
        return None;
    }
    let main_h = main_row_h(scale);
    let sub_h = sub_row_h(scale);
    let add_h = add_row_h(scale);
    let mut offset = 0.0_f32;
    for (index, track) in project.tracks.iter().enumerate() {
        if y < offset + main_h {
            return Some((index, RowKind::Main, offset, main_h));
        }
        offset += main_h;
        if track.expanded {
            if y < offset + add_h {
                return Some((index, RowKind::AddController, offset, add_h));
            }
            offset += add_h;
            for kind in &track.visible_controllers {
                if y < offset + sub_h {
                    return Some((index, RowKind::Controller(*kind), offset, sub_h));
                }
                offset += sub_h;
            }
            if y < offset + sub_h {
                return Some((index, RowKind::Effects, offset, sub_h));
            }
            offset += sub_h;
        }
    }
    None
}

pub(super) fn track_header_width(rect: Rect) -> f32 {
    HEADER_W.min(rect.width() * 0.4)
}

pub(super) fn row_rect_at(
    project: &Project,
    pos: egui::Pos2,
    lane_top: f32,
    scroll: f32,
    lane_left: f32,
    lane_width: f32,
    scale: f32,
) -> Option<(usize, RowKind, Rect)> {
    if pos.y < lane_top {
        return None;
    }
    let relative_y = pos.y - lane_top + scroll;
    let (index, kind, row_offset, height) = row_at(project, relative_y, scale)?;
    let rect = Rect::from_min_size(
        egui::pos2(lane_left, lane_top + row_offset - scroll),
        Vec2::new(lane_width, height),
    );
    Some((index, kind, rect))
}

pub(super) fn snap(value: f64, grid: f64) -> f64 {
    if grid <= 0.0 {
        value
    } else {
        (value / grid).round() * grid
    }
}

/// 所属模块：ui::arrangement / 布局与绘制
/// 功能：在片段矩形内绘制音频波形，并按源偏移取对应的峰值区段。
/// 输入：
/// - `painter`：绘制器。
/// - `clip_rect`：片段矩形。
/// - `peaks`：完整波形的峰值包络；`None` 时显示未加载提示。
/// - `color`：波形颜色。
/// - `start_fraction`：源文件中起始位置的比例（0~1）。
/// - `span_fraction`：片段覆盖源文件的比例（0~1）。
///
/// 输出/副作用：向 `painter` 写入线段；不修改其他状态。
///
/// 复杂度：时间 O(可见像素段数)，空间 O(1)。
///
/// Module: ui::arrangement / layout and painting
/// Purpose: Paint an audio waveform inside a clip rectangle, selecting the peak
/// region that matches the source offset.
/// Inputs:
/// - `painter`: painter.
/// - `clip_rect`: clip rectangle.
/// - `peaks`: full waveform peak envelope; `None` shows the not-loaded hint.
/// - `color`: waveform color.
/// - `start_fraction`: start position in the source (0..1).
/// - `span_fraction`: fraction of the source covered by the clip (0..1).
///
/// Outputs / side effects: writes line segments to `painter`; no other state changes.
///
/// Complexity: O(visible pixel steps) time and O(1) space.
pub(super) fn draw_audio_waveform(
    painter: &egui::Painter,
    clip_rect: Rect,
    peaks: Option<&[f32]>,
    color: egui::Color32,
    start_fraction: f32,
    span_fraction: f32,
) {
    let inner = Rect::from_min_max(
        egui::pos2(clip_rect.left() + 3.0, clip_rect.top() + 16.0),
        egui::pos2(clip_rect.right() - 3.0, clip_rect.bottom() - 4.0),
    );
    if inner.width() < 4.0 || inner.height() < 6.0 {
        return;
    }
    let painter = painter.with_clip_rect(inner);
    let center = inner.center().y;
    painter.hline(
        inner.x_range(),
        center,
        Stroke::new(1.0_f32, theme::with_alpha(color, 70)),
    );
    let Some(peaks) = peaks else {
        painter.text(
            inner.center(),
            Align2::CENTER_CENTER,
            "音频（未加载）",
            FontId::proportional(9.0),
            theme::TEXT_FAINT,
        );
        return;
    };
    if peaks.is_empty() {
        return;
    }
    let steps = (inner.width().round() as usize).clamp(2, 600);
    let half = inner.height() * 0.5 - 1.0;
    let start = start_fraction.clamp(0.0, 1.0);
    let span = span_fraction.clamp(0.0, 1.0 - start);
    for step in 0..steps {
        let t = (step as f32 + 0.5) / steps as f32;
        let source_t = (start + t * span).clamp(0.0, 1.0);
        let index = ((source_t * (peaks.len() - 1) as f32) as usize).min(peaks.len() - 1);
        let amplitude = peaks[index].clamp(0.0, 1.0);
        let x = inner.left() + t * inner.width();
        let height = (amplitude * half).max(0.5);
        painter.line_segment(
            [
                egui::pos2(x, center - height),
                egui::pos2(x, center + height),
            ],
            Stroke::new(1.0_f32, theme::with_alpha(color, 190)),
        );
    }
}

/// Draw two horizontal L/R level bars with a volume handle on each.
pub(super) fn draw_volume_bars(
    painter: &egui::Painter,
    area: Rect,
    volume: f32,
    level_l: f32,
    level_r: f32,
    handle: bool,
) -> [Rect; 2] {
    let gap = 2.0;
    let bar_h = ((area.height() - gap) * 0.5).clamp(4.0, 8.0);
    let left = Rect::from_min_size(area.min, Vec2::new(area.width(), bar_h));
    let right = Rect::from_min_size(
        egui::pos2(area.left(), area.top() + bar_h + gap),
        Vec2::new(area.width(), bar_h),
    );
    for (rect, level) in [(left, level_l), (right, level_r)] {
        theme::level_meter(painter, rect, level, false);
        if handle {
            let handle_x = rect.left() + rect.width() * (volume / 1.5).clamp(0.0, 1.0);
            painter.line_segment(
                [
                    egui::pos2(handle_x, rect.top() - 1.0),
                    egui::pos2(handle_x, rect.bottom() + 1.0),
                ],
                Stroke::new(2.0_f32, egui::Color32::WHITE),
            );
        }
    }
    [left, right]
}

/// Split a clip at an absolute beat. Notes that cross the cut point are split
/// as well, so both halves keep their musical content.
pub(super) fn split_clip(
    project: &mut Project,
    track_index: usize,
    clip_index: usize,
    cut: f64,
) -> bool {
    let right_id = project.next_clip_id();
    let Some(track) = project.tracks.get_mut(track_index) else {
        return false;
    };
    let Some(clip) = track.clips.get_mut(clip_index) else {
        return false;
    };
    if cut <= clip.start + 1e-6 || cut >= clip.end() - 1e-6 {
        return false;
    }
    let cut_rel = cut - clip.start;
    let right_len = clip.length - cut_rel;
    let mut left_notes = Vec::new();
    let mut right_notes = Vec::new();
    for note in clip.notes.drain(..) {
        let note_end = note.start + note.length;
        if note_end <= cut_rel + 1e-6 {
            left_notes.push(note);
        } else if note.start >= cut_rel - 1e-6 {
            let mut moved = note;
            moved.start -= cut_rel;
            right_notes.push(moved);
        } else {
            let mut left = note.clone();
            left.length = cut_rel - note.start;
            if left.length > 0.01 {
                left_notes.push(left);
            }
            let mut right = note;
            right.start = 0.0;
            right.length = note_end - cut_rel;
            if right.length > 0.01 {
                right_notes.push(right);
            }
        }
    }
    clip.notes = left_notes;
    clip.length = cut_rel;
    clip.bump_revision();
    let mut right = Clip::new(right_id, format!("{} · B", clip.name), cut, right_len);
    right.notes = right_notes;
    track.clips.push(right);
    true
}
