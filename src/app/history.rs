//! 所属模块：app / 编辑历史
//! 文件用途：
//! - 选择辅助、听感 revision 递增与撤销/重做历史。
//!
//! 函数清单：
//! - `current_track_id` / `current_track` / `current_track_mut` / `selected_clip` / `selected_clip_mut`：当前选择访问。
//! - `mark_dirty` / `bump_track_revision` / `bump_clip_revision` / `bump_selected_clip_revision`：脏标记与听感版本。
//! - `controller_lane_mut`：访问轨道/主输出自动化轨。
//! - `begin_edit_gesture` / `commit_edit_gesture` / `end_edit_gesture`：指针手势撤销。
//! - `push_undo_snapshot` / `push_undo_now` / `undo` / `redo` / `clear_history`：历史栈。
//! - `reconcile_focus_after_history` / `after_history_apply`：撤销后校正与清理。
//! - `effects_panel_min_height` / `focus_piano_on_selection`：UI 辅助。
//!
//! Module: app / edit history
//! Purpose:
//! - Selection helpers, audible revision bumps and undo/redo history.
//!
//! Functions:
//! - `current_track_id` / `current_track` / `current_track_mut` / `selected_clip` / `selected_clip_mut`: current selection access.
//! - `mark_dirty` / `bump_track_revision` / `bump_clip_revision` / `bump_selected_clip_revision`: dirty flag and audible revisions.
//! - `controller_lane_mut`: access a track/master automation lane.
//! - `begin_edit_gesture` / `commit_edit_gesture` / `end_edit_gesture`: pointer-gesture undo.
//! - `push_undo_snapshot` / `push_undo_now` / `undo` / `redo` / `clear_history`: history stack.
//! - `reconcile_focus_after_history` / `after_history_apply`: post-undo fixes and cleanup.
//! - `effects_panel_min_height` / `focus_piano_on_selection`: UI helpers.

use super::*;

impl SurfingApp {
    // ----- project helpers ------------------------------------------------

    pub fn current_track_id(&self) -> Option<TrackId> {
        self.project.tracks.get(self.selected_track).map(|t| t.id)
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.project.tracks.get(self.selected_track)
    }

    pub fn current_track_mut(&mut self) -> Option<&mut Track> {
        self.project.tracks.get_mut(self.selected_track)
    }

    pub fn selected_clip(&self) -> Option<&Clip> {
        let track = self.current_track()?;
        track.clips.get(self.selected_clip?)
    }

    pub fn selected_clip_mut(&mut self) -> Option<&mut Clip> {
        let index = self.selected_clip?;
        self.current_track_mut()?.clips.get_mut(index)
    }

    pub fn mark_dirty(&mut self) {
        if self.engine_stale {
            self.begin_engine_load();
        }
        self.edit_seq = self.edit_seq.wrapping_add(1);
        self.dirty = true;
        self.needs_sync = true;
    }

    /// Mutable automation lane for a track, or the pinned master volume lane
    /// when `track_id` is [`crate::core::model::MASTER_TRACK_ID`].
    pub fn controller_lane_mut(
        &mut self,
        track_id: TrackId,
        kind: ControllerKind,
    ) -> Option<&mut Vec<AutomationPoint>> {
        if track_id == crate::core::model::MASTER_TRACK_ID {
            return Some(self.project.master_controllers.get_mut(kind));
        }
        let track = self.project.tracks.iter_mut().find(|t| t.id == track_id)?;
        track.bump_revision();
        Some(track.controllers.get_mut(kind))
    }

    /// Bump the render revision of a whole track (instrument/effects/controllers).
    pub fn bump_track_revision(&mut self, track_index: usize) {
        if self.engine_stale {
            self.begin_engine_load();
        }
        if let Some(track) = self.project.tracks.get_mut(track_index) {
            track.bump_revision();
            self.edit_seq = self.edit_seq.wrapping_add(1);
            self.dirty = true;
            self.needs_sync = true;
        }
    }

    /// Bump the render revision of a single MIDI clip.
    pub fn bump_clip_revision(&mut self, track_index: usize, clip_index: usize) {
        if self.engine_stale {
            self.begin_engine_load();
        }
        if let Some(clip) = self
            .project
            .tracks
            .get_mut(track_index)
            .and_then(|track| track.clips.get_mut(clip_index))
        {
            clip.bump_revision();
            self.edit_seq = self.edit_seq.wrapping_add(1);
            self.dirty = true;
            self.needs_sync = true;
        }
    }

    // ----- undo / redo ----------------------------------------------------

    /// Capture the project before a pointer gesture (click or drag) begins.
    pub fn begin_edit_gesture(&mut self) {
        if self.pending_undo.is_none() {
            self.pending_undo = Some(self.project.clone());
            self.pending_undo_seq = self.edit_seq;
        }
    }

    /// Commit an undo step once the gesture actually changed the project.
    pub fn commit_edit_gesture(&mut self) {
        if self.pending_undo.is_some() && self.edit_seq != self.pending_undo_seq {
            let snapshot = self.pending_undo.take().expect("snapshot present");
            self.push_undo_snapshot(snapshot);
        }
    }

    pub fn end_edit_gesture(&mut self) {
        self.commit_edit_gesture();
        self.pending_undo = None;
    }

    fn push_undo_snapshot(&mut self, snapshot: Project) {
        let now = Instant::now();
        if let Some(last) = self.last_undo_at {
            if now.duration_since(last) < Duration::from_millis(180) {
                return;
            }
        }
        self.undo_stack.push(snapshot);
        if self.undo_stack.len() > 64 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.last_undo_at = Some(now);
    }

    /// Immediate snapshot for keyboard-only edits (e.g. Delete).
    pub fn push_undo_now(&mut self) {
        self.commit_edit_gesture();
        self.undo_stack.push(self.project.clone());
        if self.undo_stack.len() > 64 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.pending_undo = None;
        self.last_undo_at = Some(Instant::now());
    }

    pub fn undo(&mut self) {
        let Some(previous) = self.undo_stack.pop() else {
            self.status = "没有可撤销的操作".into();
            return;
        };
        let focus_clip = self.selected_clip().map(|clip| clip.id);
        self.redo_stack
            .push(std::mem::replace(&mut self.project, previous));
        self.after_history_apply();
        self.reconcile_focus_after_history(focus_clip);
        self.status = "已撤销".into();
    }

    pub fn redo(&mut self) {
        let Some(next) = self.redo_stack.pop() else {
            self.status = "没有可重做的操作".into();
            return;
        };
        let focus_clip = self.selected_clip().map(|clip| clip.id);
        self.undo_stack
            .push(std::mem::replace(&mut self.project, next));
        self.after_history_apply();
        self.reconcile_focus_after_history(focus_clip);
        self.status = "已重做".into();
    }

    /// Close the bottom roll when its content vanished with the history step
    /// (e.g. Ctrl+Z right after adding a controller or a MIDI clip).
    fn reconcile_focus_after_history(&mut self, focus_clip: Option<u64>) {
        if self.bottom_open {
            let alive = match self.bottom_view {
                BottomView::Controller { track_id, kind } => {
                    if track_id == crate::core::model::MASTER_TRACK_ID {
                        true
                    } else {
                        self.project
                            .tracks
                            .iter()
                            .find(|track| track.id == track_id)
                            .map(|track| track.visible_controllers.contains(&kind))
                            .unwrap_or(false)
                    }
                }
                BottomView::Effects { track_id } => {
                    self.project.tracks.iter().any(|track| track.id == track_id)
                        || track_id == crate::core::model::MASTER_TRACK_ID
                }
                // The roll now shows every clip of the track, so it only
                // closes when the focused clip itself disappeared.
                BottomView::PianoRoll => match focus_clip {
                    Some(id) => self
                        .project
                        .tracks
                        .iter()
                        .any(|track| track.clips.iter().any(|clip| clip.id == id)),
                    None => true,
                },
            };
            if !alive {
                self.bottom_open = false;
            }
        }
        if let Some(id) = focus_clip {
            let alive = self
                .project
                .tracks
                .iter()
                .any(|track| track.clips.iter().any(|clip| clip.id == id));
            if !alive {
                self.selected_clip = None;
                self.selected_clips.clear();
                self.selected_note = None;
                self.selected_notes.clear();
            }
        }
    }

    fn after_history_apply(&mut self) {
        self.release_held_notes();
        // 撤销会替换工程内容：先停止音频处理，再 detach 插件编辑器。
        // Undo replaces project content: stop processing before detaching editors.
        if let Ok(mut engine) = self.engine.try_lock() {
            engine.reset_playback_state();
        }
        self.ui_playing = false;
        self.close_all_editors();
        self.selected_track = self
            .selected_track
            .min(self.project.tracks.len().saturating_sub(1));
        self.selected_note = None;
        self.selected_notes.clear();
        self.selected_clips.clear();
        self.selected_effect = None;
        self.selected_controller = None;
        self.selected_effects_lane = None;
        self.lane_paste_prompt = None;
        self.controller_selection.clear();
        self.controller_marquee_start = None;
        self.arr_drag = ArrangementDrag::None;
        self.piano_drag = PianoDrag::None;
        self.effect_widths.clear();
        self.effect_can_resize.clear();
        self.effect_drag_from = None;
        self.effect_drag_target = None;
        self.pending_undo = None;
        self.edit_seq = self.edit_seq.wrapping_add(1);
        self.dirty = true;
        self.needs_sync = true;
        self.last_undo_at = Some(Instant::now());
    }

    pub(super) fn clear_history(&mut self) {
        // Dropping deep undo stacks (full project snapshots) takes time; free
        // them off the UI thread so opening a project never freezes.
        let undo = std::mem::take(&mut self.undo_stack);
        let redo = std::mem::take(&mut self.redo_stack);
        let pending = self.pending_undo.take();
        if !undo.is_empty() || !redo.is_empty() || pending.is_some() {
            std::thread::spawn(move || {
                drop(undo);
                drop(redo);
                drop(pending);
            });
        }
        self.last_undo_at = None;
        self.plugin_param_cache.clear();
        self.plugin_param_cache_at.clear();
        self.effect_widths.clear();
        self.effect_can_resize.clear();
        self.inline_failed.clear();
        self.effect_editor_flags.clear();
        self.background_synced.clear();
        self.effect_drag_from = None;
        self.effect_drag_target = None;
    }

    pub fn bump_selected_clip_revision(&mut self) {
        if let Some(clip_index) = self.selected_clip {
            let track_index = self.selected_track;
            self.bump_clip_revision(track_index, clip_index);
        }
    }

    /// Centre the piano roll on the notes inside the selected clip. Only the
    /// roll that is currently being edited is affected, so selecting clips on
    /// other tracks never moves an open roll.
    pub fn focus_piano_on_selection(&mut self) {
        let Some(track_id) = self.current_track_id() else {
            return;
        };
        if self.piano_track != Some(track_id) {
            return;
        }
        let max_pitch = self
            .selected_clip()
            .and_then(|clip| clip.notes.iter().map(|note| note.pitch).max());
        if let Some(max_pitch) = max_pitch {
            self.piano_top_pitch = (max_pitch as i32 + 8).clamp(40, 108);
            self.piano_views.entry(track_id).or_default().top_pitch = self.piano_top_pitch;
        }
    }
}
