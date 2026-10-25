//! Scale tables, note helpers and split operations for the piano roll.

pub(super) const KEYBOARD_W: f32 = 64.0;
pub(super) const RULER_H: f32 = 20.0;
pub(super) const DEFAULT_NOTE_H: f32 = 15.0;
/// 底部力度控制器高度（逻辑点）。
/// Height of the velocity lane at the bottom of the piano roll in points.
pub(super) const VELOCITY_LANE_H: f32 = 84.0;
/// 力度控制器单侧内边距：避免最高/最低力度的圆点贴住边线。
/// Padding inside the velocity lane so the loudest/quietest knobs never touch the border.
pub(super) const VELOCITY_LANE_PAD: f32 = 8.0;
/// 力度控制器圆点半径。
/// Radius of a velocity knob.
pub(super) const VELOCITY_KNOB_R: f32 = 3.0;
/// 命中力度圆点的横向容差（逻辑点）。
/// Horizontal tolerance for hitting a velocity knob in points.
pub(super) const VELOCITY_HIT_W: f32 = 8.0;
/// 力度圆点后方横线的线宽（逻辑点）；横线长度与对应音符等长。
/// Stroke width of the bar trailing a velocity knob in points; its length matches the note.
pub(super) const VELOCITY_BAR_H: f32 = 1.6;

pub(super) const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
pub(super) const BLACK_KEYS: [bool; 12] = [
    false, true, false, true, false, false, true, false, true, false, true, false,
];

pub(super) fn is_black_key(pitch: u8) -> bool {
    BLACK_KEYS[(pitch % 12) as usize]
}

/// Scale names and interval masks (semitone degrees), for the roll's
/// scale-aware key shading.
pub const SCALES: [(&str, [bool; 12]); 13] = [
    (
        "自然大调",
        [
            true, false, true, false, true, true, false, true, false, true, false, true,
        ],
    ),
    (
        "自然小调",
        [
            true, false, true, true, false, true, false, true, true, false, true, false,
        ],
    ),
    (
        "和声小调",
        [
            true, false, true, true, false, true, false, true, true, false, false, true,
        ],
    ),
    (
        "旋律小调",
        [
            true, false, true, true, false, true, false, true, true, true, true, true,
        ],
    ),
    (
        "多利亚",
        [
            true, false, true, true, false, true, false, true, false, true, true, false,
        ],
    ),
    (
        "弗里几亚",
        [
            true, true, false, true, false, true, false, true, true, false, true, false,
        ],
    ),
    (
        "利底亚",
        [
            true, false, true, false, true, false, true, true, false, true, false, true,
        ],
    ),
    (
        "混合利底亚",
        [
            true, false, true, false, true, true, false, true, false, true, true, false,
        ],
    ),
    (
        "洛克里亚",
        [
            true, true, false, true, false, true, true, false, true, false, true, false,
        ],
    ),
    (
        "大调五声",
        [
            true, false, true, false, true, false, false, true, false, true, false, false,
        ],
    ),
    (
        "小调五声",
        [
            true, false, false, true, false, true, false, true, false, false, true, false,
        ],
    ),
    (
        "布鲁斯",
        [
            true, false, false, true, false, true, true, true, false, false, true, false,
        ],
    ),
    (
        "全音阶",
        [
            true, false, true, false, true, false, true, false, true, false, true, false,
        ],
    ),
];

/// Whether a pitch belongs to the selected scale.
pub(super) fn in_scale(pitch: u8, root: i32, mode: usize) -> bool {
    let mask = SCALES[mode.min(SCALES.len() - 1)].1;
    let degree = (pitch as i32 - root).rem_euclid(12) as usize;
    mask[degree]
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NoteEdge {
    Body,
    Left,
    Right,
}

pub(super) fn snap(value: f64, grid: f64) -> f64 {
    if grid <= 0.0 {
        value
    } else {
        (value / grid).round() * grid
    }
}

pub(super) fn snap_label(value: f64) -> &'static str {
    match value {
        0.25 => "1/16",
        0.5 => "1/8",
        1.0 => "1/4",
        2.0 => "1/2",
        _ => "关闭",
    }
}

/// Split a note at `cut` (clip-relative beats), keeping the original velocity.
pub(super) fn split_note(clip: &mut crate::core::model::Clip, index: usize, cut: f64) -> bool {
    let Some(note) = clip.notes.get(index).cloned() else {
        return false;
    };
    if cut <= note.start + 1e-6 || cut >= note.end() - 1e-6 {
        return false;
    }
    let left_len = cut - note.start;
    let right = crate::core::model::Note {
        start: cut,
        length: note.length - left_len,
        pitch: note.pitch,
        velocity: note.velocity,
    };
    clip.notes[index].length = left_len;
    clip.notes.push(right);
    true
}
