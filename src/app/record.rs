//! 所属模块：app / MIDI 录制
//! 文件用途：
//! - 维护录音状态：开始/结束录音、按轨道收集输入音符，并在停止后生成新的 MIDI 片段。
//! - 决定 MIDI 输入的目标轨道：显式绑定优先，默认设备回退到当前选中轨道。
//!
//! 函数清单：
//! - `SurfingApp::toggle_record` / `start_recording` / `finish_recording`：录音开关与收尾。
//! - `SurfingApp::record_note_on` / `record_note_off`：把输入音符写入录音缓冲。
//! - `SurfingApp::transport_beat`：读取当前听感播放位置。
//! - `SurfingApp::midi_target_tracks` / `is_default_midi_device`：输入事件的目标轨道选择。
//! - `TrackTake::note_on` / `note_off` / `close_all` / `into_clip`：单轨录音缓冲与片段生成。
//!
//! Module: app / MIDI recording
//! Purpose:
//! - Hold the recording state: start/stop, collect notes per track and create new MIDI clips on stop.
//! - Resolve MIDI input targets: explicit track bindings win, the default device falls back to the
//!   currently selected track.
//!
//! Functions:
//! - `SurfingApp::toggle_record` / `start_recording` / `finish_recording`: recording control.
//! - `SurfingApp::record_note_on` / `record_note_off`: write input notes into the recording buffers.
//! - `SurfingApp::transport_beat`: read the audible transport position.
//! - `SurfingApp::midi_target_tracks` / `is_default_midi_device`: pick the destination tracks.
//! - `TrackTake::note_on` / `note_off` / `close_all` / `into_clip`: per-track buffer and clip creation.

use super::*;
use crate::core::midi_input::MidiMessage;

/// 录制音符的最短长度（beat）；过短的点击按该长度保存。
/// Minimum recorded note length in beats; very short taps are stored with this length.
const MIN_RECORDED_NOTE_BEATS: f64 = 1.0 / 32.0;
/// 录制片段的最短长度（beat）；避免“无输入”或极短输入生成零长度片段。
/// Minimum recorded clip length in beats; prevents zero-length clips.
const MIN_RECORDED_CLIP_BEATS: f64 = 0.25;

/// 一个正在按下的录音音符。
/// One note currently held while recording.
#[derive(Clone, Copy, Debug)]
struct ActiveNote {
    start_abs: f64,
    pitch: u8,
    velocity: u8,
}

/// 一个已结束的录音音符，时间使用工程绝对 beat。
/// One finished recorded note, using absolute project beats.
#[derive(Clone, Copy, Debug)]
struct RecordedNote {
    start_abs: f64,
    end_abs: f64,
    pitch: u8,
    velocity: u8,
}

/// 单条轨道的录音缓冲。
/// Recording buffer of one track.
#[derive(Clone, Debug, Default)]
struct TrackTake {
    notes: Vec<RecordedNote>,
    active: HashMap<u8, ActiveNote>,
}

impl TrackTake {
    /// 所属模块：app / MIDI 录制 / 单轨缓冲
    /// 功能：记录一个 Note On；同一音高尚未释放时先按当前位置收尾。
    /// 输入：
    /// - `pitch`：音高。
    /// - `velocity`：7 位力度。
    /// - `beat`：工程绝对位置。
    /// 输出/副作用：更新 `active` 与 `notes`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / MIDI recording / track buffer
    /// Purpose: Record a Note On; an unreleased pitch of the same key is closed first.
    /// Inputs:
    /// - `pitch`: note pitch.
    /// - `velocity`: 7-bit velocity.
    /// - `beat`: absolute project position.
    /// Outputs / side effects: updates `active` and `notes`.
    /// Complexity: O(1) time and space.
    fn note_on(&mut self, pitch: u8, velocity: u8, beat: f64) {
        if let Some(previous) = self.active.remove(&pitch) {
            self.push_note(previous, beat);
        }
        self.active.insert(
            pitch,
            ActiveNote {
                start_abs: beat,
                pitch,
                velocity: velocity.clamp(1, 127),
            },
        );
    }

    /// 所属模块：app / MIDI 录制 / 单轨缓冲
    /// 功能：记录一个 Note Off；没有对应 Note On 时忽略。
    /// 输入：
    /// - `pitch`：音高。
    /// - `beat`：工程绝对位置。
    /// 输出/副作用：把结束音符写入 `notes`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / MIDI recording / track buffer
    /// Purpose: Record a Note Off; ignored when no Note On is active.
    /// Inputs:
    /// - `pitch`: note pitch.
    /// - `beat`: absolute project position.
    /// Outputs / side effects: appends the finished note to `notes`.
    /// Complexity: O(1) time and space.
    fn note_off(&mut self, pitch: u8, beat: f64) {
        if let Some(active) = self.active.remove(&pitch) {
            self.push_note(active, beat);
        }
    }

    /// 所属模块：app / MIDI 录制 / 单轨缓冲
    /// 功能：在录音停止时结束全部仍按下的音符。
    /// 输入：`beat`：工程绝对位置。
    /// 输出/副作用：清空 `active` 并写入 `notes`。
    /// 复杂度：时间 O(按下音高数)，空间 O(1)。
    ///
    /// Module: app / MIDI recording / track buffer
    /// Purpose: Close every still-held note when recording stops.
    /// Inputs: `beat`: absolute project position.
    /// Outputs / side effects: clears `active` and appends to `notes`.
    /// Complexity: O(held pitches) time and O(1) space.
    fn close_all(&mut self, beat: f64) {
        let active = std::mem::take(&mut self.active);
        for (_, note) in active {
            self.push_note(note, beat);
        }
    }

    /// 所属模块：app / MIDI 录制 / 单轨缓冲
    /// 功能：把一个按下音符转换为结束音符，保证最短长度。
    /// 输入：
    /// - `active`：按下的音符。
    /// - `end_abs`：结束位置。
    /// 输出/副作用：追加到 `notes`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / MIDI recording / track buffer
    /// Purpose: Turn a held note into a finished note with the minimum length.
    /// Inputs:
    /// - `active`: held note.
    /// - `end_abs`: end position.
    /// Outputs / side effects: appends to `notes`.
    /// Complexity: O(1) time and space.
    fn push_note(&mut self, active: ActiveNote, end_abs: f64) {
        let end_abs = end_abs.max(active.start_abs + MIN_RECORDED_NOTE_BEATS);
        self.notes.push(RecordedNote {
            start_abs: active.start_abs,
            end_abs,
            pitch: active.pitch,
            velocity: active.velocity,
        });
    }

    /// 所属模块：app / MIDI 录制 / 单轨缓冲
    /// 功能：把录音缓冲转换为从 `start_beat` 开始的新 MIDI 片段；没有音符时返回 `None`。
    /// 输入：
    /// - `id`：新片段 id。
    /// - `start_beat`：录音起点（片段起点）。
    /// 输出/副作用：返回片段；不修改工程。
    /// 复杂度：时间 O(n log n)，空间 O(n)，`n` = 录音音符数。
    ///
    /// Module: app / MIDI recording / track buffer
    /// Purpose: Convert the buffer into a new MIDI clip starting at `start_beat`; `None` when empty.
    /// Inputs:
    /// - `id`: new clip id.
    /// - `start_beat`: recording start (clip start).
    /// Outputs / side effects: returns the clip; the project is untouched.
    /// Complexity: O(n log n) time and O(n) space, `n` = recorded notes.
    fn into_clip(self, id: u64, start_beat: f64) -> Option<Clip> {
        if self.notes.is_empty() {
            return None;
        }
        let mut notes = self.notes;
        notes.sort_by(|left, right| {
            left.start_abs
                .partial_cmp(&right.start_abs)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let end_beat = notes
            .iter()
            .map(|note| note.end_abs)
            .fold(start_beat + MIN_RECORDED_CLIP_BEATS, f64::max);
        let mut clip = Clip::new(id, "录音片段", start_beat, end_beat - start_beat);
        clip.notes = notes
            .into_iter()
            .map(|note| Note {
                start: (note.start_abs - start_beat).max(0.0),
                length: (note.end_abs - note.start_abs).max(MIN_RECORDED_NOTE_BEATS),
                pitch: note.pitch,
                velocity: note.velocity,
            })
            .collect();
        Some(clip)
    }
}

/// 录制状态；仅 UI 状态，不写入工程文件。
/// Recording state; UI-only and never persisted into the project file.
#[derive(Clone, Debug, Default)]
pub struct RecordState {
    active: bool,
    start_beat: f64,
    takes: HashMap<TrackId, TrackTake>,
}

impl RecordState {
    /// 所属模块：app / MIDI 录制 / 状态
    /// 功能：是否正在录音。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / MIDI recording / state
    /// Purpose: Whether recording is active.
    /// Inputs: `self`.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(1) time and space.
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl SurfingApp {
    /// 所属模块：app / MIDI 录制
    /// 功能：切换录音：未录音时开始，录音中则停止并生成片段。
    /// 输入：`self`。
    /// 输出/副作用：可能开始播放、创建 MIDI 片段、更新选择与状态栏。
    /// 复杂度：时间 O(1)（开始）或 O(录音音符数 log 音符数)（收尾），空间 O(录音音符数)。
    ///
    /// Module: app / MIDI recording
    /// Purpose: Toggle recording: start when idle, stop and create clips while recording.
    /// Inputs: `self`.
    /// Outputs / side effects: may start playback, create MIDI clips and update selection/status.
    /// Complexity: O(1) to start, O(n log n) to finish with `n` recorded notes.
    pub fn toggle_record(&mut self) {
        if self.record.active {
            self.finish_recording();
        } else {
            self.start_recording();
        }
    }

    /// 所属模块：app / MIDI 录制
    /// 功能：开始录音；未播放时先开始播放，并临时关闭自动停止。
    /// 输入：`self`。
    /// 输出/副作用：设置录音起点、清空缓冲、关闭引擎自动停止并更新状态栏。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / MIDI recording
    /// Purpose: Start recording; playback starts first when stopped, and auto-stop is suspended.
    /// Inputs: `self`.
    /// Outputs / side effects: sets the start beat, clears the buffers, disables engine auto-stop
    /// and updates the status bar.
    /// Complexity: O(1) time and space.
    fn start_recording(&mut self) {
        if self.record.active {
            return;
        }
        if self.export.is_rendering() {
            self.status = "正在导出音频，无法录音".into();
            return;
        }
        if self.instrument_load_pending {
            self.status = "正在加载插件，无法录音".into();
            return;
        }
        if !self.ui_playing {
            self.toggle_play();
            if !self.ui_playing {
                return;
            }
        }
        let start_beat = self.transport_beat().max(0.0);
        self.record = RecordState {
            active: true,
            start_beat,
            takes: HashMap::new(),
        };
        if let Ok(mut engine) = self.engine.lock() {
            // 录音期间不允许自动停止，否则空工程在输入前就会停下。
            // Auto-stop is suspended while recording, otherwise an empty project stops before input.
            engine.set_auto_stop(false, &self.project);
        }
        self.status = "正在录音…".into();
    }

    /// 所属模块：app / MIDI 录制
    /// 功能：结束录音：关闭仍按下的音符，为有输入的轨道各生成一个新 MIDI 片段。
    /// 输入：`self`。
    /// 输出/副作用：可能创建片段、记录撤销、标记工程已修改并恢复自动停止；无输入时不创建片段。
    /// 复杂度：时间 O(轨道数 + 录音音符数 log 音符数)，空间 O(录音音符数)。
    ///
    /// Module: app / MIDI recording
    /// Purpose: Stop recording: close held notes and create one new MIDI clip per track with input.
    /// Inputs: `self`.
    /// Outputs / side effects: may create clips, push an undo step, mark the project dirty and
    /// restore auto-stop; no input creates no clip.
    /// Complexity: O(tracks + n log n) time and O(n) space, `n` = recorded notes.
    pub(super) fn finish_recording(&mut self) {
        if !self.record.active {
            return;
        }
        let end_beat = self.transport_beat().max(self.record.start_beat);
        let state = std::mem::take(&mut self.record);
        let start_beat = state.start_beat;
        let mut takes = state.takes;
        for take in takes.values_mut() {
            take.close_all(end_beat);
        }
        let mut ordered: Vec<(TrackId, TrackTake)> = takes.into_iter().collect();
        ordered.sort_by_key(|(track_id, _)| *track_id);
        let has_notes = ordered.iter().any(|(_, take)| !take.notes.is_empty());
        let mut next_clip_id = self.project.next_clip_id();
        let mut pending: Vec<(usize, Clip)> = Vec::new();
        for (track_id, take) in ordered {
            if take.notes.is_empty() {
                continue;
            }
            let Some(track_index) = self
                .project
                .tracks
                .iter()
                .position(|track| track.id == track_id)
            else {
                continue;
            };
            if self.project.tracks[track_index].has_audio_clips() {
                continue;
            }
            let Some(clip) = take.into_clip(next_clip_id, start_beat) else {
                continue;
            };
            next_clip_id += 1;
            pending.push((track_index, clip));
        }
        let mut created = 0usize;
        let mut first_selection: Option<(usize, usize)> = None;
        if has_notes && !pending.is_empty() {
            self.push_undo_now();
            for (track_index, clip) in pending {
                let clip_index = self.project.tracks[track_index].clips.len();
                self.project.tracks[track_index].clips.push(clip);
                created += 1;
                if first_selection.is_none() {
                    first_selection = Some((track_index, clip_index));
                }
            }
        }
        if let Some((track_index, clip_index)) = first_selection {
            self.selected_track = track_index;
            self.selected_clip = Some(clip_index);
            self.selected_clips = vec![(track_index, clip_index)];
            self.selected_note = None;
        }
        if created > 0 {
            self.mark_dirty();
        }
        if let Ok(mut engine) = self.engine.lock() {
            // 用新工程内容重新计算自动停止点，避免收尾瞬间按旧内容停止。
            // Recompute the auto-stop point from the new content so the transport cannot stop at
            // the stale limit right after recording.
            engine.set_auto_stop(true, &self.project);
        }
        self.status = if created > 0 {
            format!("录音结束：已创建 {created} 个 MIDI 片段")
        } else {
            "录音结束：未收到 MIDI 输入，未创建片段".into()
        };
    }

    /// 所属模块：app / MIDI 录制
    /// 功能：把一个 Note On 写入指定轨道的录音缓冲。
    /// 输入：
    /// - `track_id`：目标轨道 id。
    /// - `pitch`：音高。
    /// - `velocity`：7 位力度。
    /// 输出/副作用：录音未激活时忽略；否则更新对应轨道缓冲。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / MIDI recording
    /// Purpose: Write a Note On into the recording buffer of the target track.
    /// Inputs:
    /// - `track_id`: target track id.
    /// - `pitch`: note pitch.
    /// - `velocity`: 7-bit velocity.
    /// Outputs / side effects: ignored when recording is off; otherwise updates the track buffer.
    /// Complexity: O(1) time and space.
    pub(super) fn record_note_on(&mut self, track_id: TrackId, pitch: u8, velocity: u8) {
        if !self.record.active {
            return;
        }
        let beat = self.transport_beat();
        self.record
            .takes
            .entry(track_id)
            .or_default()
            .note_on(pitch, velocity, beat);
    }

    /// 所属模块：app / MIDI 录制
    /// 功能：把一个 Note Off 写入指定轨道的录音缓冲。
    /// 输入：
    /// - `track_id`：目标轨道 id。
    /// - `pitch`：音高。
    /// 输出/副作用：录音未激活时忽略；否则结束对应音符。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / MIDI recording
    /// Purpose: Write a Note Off into the recording buffer of the target track.
    /// Inputs:
    /// - `track_id`: target track id.
    /// - `pitch`: note pitch.
    /// Outputs / side effects: ignored when recording is off; otherwise closes the note.
    /// Complexity: O(1) time and space.
    pub(super) fn record_note_off(&mut self, track_id: TrackId, pitch: u8) {
        if !self.record.active {
            return;
        }
        let beat = self.transport_beat();
        if let Some(take) = self.record.takes.get_mut(&track_id) {
            take.note_off(pitch, beat);
        }
    }

    /// 所属模块：app / MIDI 录制
    /// 功能：读取当前听感播放位置（优先引擎，锁忙时回退 UI 快照）。
    /// 输入：`self`。
    /// 输出/副作用：beat 位置；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / MIDI recording
    /// Purpose: Read the audible transport position (engine first, UI snapshot as fallback).
    /// Inputs: `self`.
    /// Outputs / side effects: position in beats; no state change.
    /// Complexity: O(1) time and space.
    pub(super) fn transport_beat(&self) -> f64 {
        self.engine
            .try_lock()
            .map(|engine| engine.position_beats())
            .unwrap_or(self.ui_playhead)
    }

    /// 所属模块：app / MIDI 录制 / 输入路由
    /// 功能：决定一个输入设备的事件应送到哪些轨道：显式绑定优先，默认设备回退到选中轨道。
    /// 输入：`device`：事件来源端口名。
    /// 输出/副作用：目标轨道 id 列表；没有匹配时为空。
    /// 复杂度：时间 O(轨道数)，空间 O(目标轨道数)。
    ///
    /// Module: app / MIDI recording / input routing
    /// Purpose: Decide which tracks receive an input device event: explicit bindings first, the
    /// default device falls back to the selected track.
    /// Inputs: `device`: source port name.
    /// Outputs / side effects: target track ids; empty when nothing matches.
    /// Complexity: O(tracks) time and O(targets) space.
    pub(super) fn midi_target_tracks(&self, device: &str) -> Vec<TrackId> {
        let bound: Vec<TrackId> = self
            .project
            .tracks
            .iter()
            .filter(|track| {
                track
                    .input_device
                    .as_deref()
                    .is_some_and(|bound| bound.eq_ignore_ascii_case(device))
            })
            .map(|track| track.id)
            .collect();
        if !bound.is_empty() {
            return bound;
        }
        if !self.is_default_midi_device(device) {
            return Vec::new();
        }
        let Some(track) = self.current_track() else {
            return Vec::new();
        };
        // 选中轨道已绑定其他设备时，默认设备不再介入（显式绑定优先级更高）。
        // When the selected track is bound to another device the default device stays out of the
        // way, because explicit bindings have higher priority.
        if track.input_device.is_some() {
            return Vec::new();
        }
        vec![track.id]
    }

    /// 所属模块：app / MIDI 录制 / 输入路由
    /// 功能：判断端口是否为当前默认 MIDI 设备。
    /// 输入：`device`：端口名。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(端口名长度)，空间 O(1)。
    ///
    /// Module: app / MIDI recording / input routing
    /// Purpose: Whether the port is the current default MIDI device.
    /// Inputs: `device`: port name.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(port name length) time and O(1) space.
    fn is_default_midi_device(&self, device: &str) -> bool {
        self.shared
            .midi_default_device()
            .is_some_and(|default| default.eq_ignore_ascii_case(device))
    }
}

/// 返回事件是否为音符；控制器与弯音目前不参与录音。
/// Whether an event is a note; controllers and pitch bend do not participate in recording yet.
pub(super) fn note_from_message(message: &MidiMessage) -> Option<(bool, u8, u8)> {
    let (on, pitch, velocity) = message.note()?;
    if on {
        Some((true, pitch, MidiMessage::velocity7(velocity)))
    } else {
        Some((false, pitch, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：录音片段从录音起点开始，音符位置相对片段，末尾覆盖最后一个音符。
    /// Covers: the recorded clip starts at the recording start, notes are relative and the end
    /// covers the last note.
    #[test]
    fn recorded_take_becomes_relative_clip() {
        let mut take = TrackTake::default();
        take.note_on(60, 100, 10.0);
        take.note_off(60, 11.0);
        take.note_on(64, 90, 12.5);
        take.close_all(13.0);
        let clip = take.into_clip(7, 8.0).expect("clip with notes");
        assert_eq!(clip.id, 7);
        assert!((clip.start - 8.0).abs() < 1e-9);
        assert!((clip.length - 5.0).abs() < 1e-9);
        assert_eq!(clip.notes.len(), 2);
        assert!((clip.notes[0].start - 2.0).abs() < 1e-9);
        assert!((clip.notes[1].start - 4.5).abs() < 1e-9);
        assert!((clip.notes[1].length - 0.5).abs() < 1e-9);
    }

    /// 覆盖：没有任何输入时不创建片段。
    /// Covers: no input creates no clip.
    #[test]
    fn empty_take_creates_no_clip() {
        let take = TrackTake::default();
        assert!(take.into_clip(1, 4.0).is_none());
    }

    /// 覆盖：极短音符与极短片段仍满足最小时长。
    /// Covers: very short notes and clips still satisfy the minimum lengths.
    #[test]
    fn short_input_uses_the_minimum_lengths() {
        let mut take = TrackTake::default();
        take.note_on(60, 80, 2.0);
        take.note_off(60, 2.0);
        let clip = take.into_clip(1, 1.9).expect("clip");
        assert!((clip.length - MIN_RECORDED_CLIP_BEATS).abs() < 1e-9);
        assert!(clip.notes[0].length >= MIN_RECORDED_NOTE_BEATS - 1e-9);
    }
}
