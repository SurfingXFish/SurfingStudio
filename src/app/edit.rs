//! Editing commands: delete, copy, paste and lane replacement.

use super::*;

impl SurfingApp {
    pub fn delete_selection(&mut self) {
        // 1. Controller points when the controller roll is open.
        if self.bottom_open {
            if let BottomView::Controller { track_id, kind } = self.bottom_view {
                if !self.controller_selection.is_empty() {
                    self.push_undo_now();
                    let mut selected = self.controller_selection.clone();
                    selected.sort_unstable();
                    selected.dedup();
                    if let Some(lane) = self.controller_lane_mut(track_id, kind) {
                        for index in selected.iter().rev() {
                            if *index < lane.len() {
                                lane.remove(*index);
                            }
                        }
                    }
                    self.controller_selection.clear();
                    self.mark_dirty();
                    self.status = "已删除控制器节点".into();
                    return;
                }
            }
        }

        // 2. Selection made directly inside the bottom editor.
        if self.bottom_open {
            match self.bottom_view {
                BottomView::Effects { .. } => {
                    if let Some((track_id, index)) = self.selected_effect {
                        self.remove_effect_at(track_id, index);
                        return;
                    }
                }
                BottomView::Controller { .. } => {
                    if let Some((track_id, kind)) = self.selected_controller {
                        self.remove_controller(track_id, kind);
                        return;
                    }
                }
                BottomView::PianoRoll => {}
            }
        }

        // 3. Notes.
        let note_refs = if !self.selected_notes.is_empty() {
            self.selected_notes.clone()
        } else {
            self.selected_note.into_iter().collect()
        };
        if !note_refs.is_empty() {
            self.push_undo_now();
            let mut changed = false;
            if let Some(track) = self.current_track_mut() {
                // Remove notes per clip, highest index first so indices hold.
                for clip_index in 0..track.clips.len() {
                    let mut indices: Vec<usize> = note_refs
                        .iter()
                        .filter(|(clip, _)| *clip == clip_index)
                        .map(|(_, note)| *note)
                        .filter(|note| *note < track.clips[clip_index].notes.len())
                        .collect();
                    indices.sort_unstable();
                    indices.dedup();
                    let mut clip_changed = false;
                    for note in indices.into_iter().rev() {
                        track.clips[clip_index].notes.remove(note);
                        clip_changed = true;
                    }
                    if clip_changed {
                        track.clips[clip_index].bump_revision();
                        changed = true;
                    }
                }
            }
            self.selected_note = None;
            self.selected_notes.clear();
            if changed {
                self.mark_dirty();
                self.status = "已删除音符".into();
            }
            return;
        }

        // 3. Effect.
        if let Some((track_id, index)) = self.selected_effect {
            self.remove_effect_at(track_id, index);
            return;
        }

        // 4. Controller lane.
        if let Some((track_id, kind)) = self.selected_controller {
            self.remove_controller(track_id, kind);
            return;
        }

        // 5. Clips (may span tracks).
        let clip_refs: Vec<(usize, usize)> = if !self.selected_clips.is_empty() {
            self.selected_clips.clone()
        } else {
            self.selected_clip
                .map(|clip| (self.selected_track, clip))
                .into_iter()
                .collect()
        };
        if !clip_refs.is_empty() {
            self.push_undo_now();
            let mut changed = false;
            // Per track, highest index first so indices hold.
            for track_index in 0..self.project.tracks.len() {
                let mut indices: Vec<usize> = clip_refs
                    .iter()
                    .filter(|(track, _)| *track == track_index)
                    .map(|(_, clip)| *clip)
                    .filter(|clip| *clip < self.project.tracks[track_index].clips.len())
                    .collect();
                indices.sort_unstable();
                indices.dedup();
                if indices.is_empty() {
                    continue;
                }
                if let Some(track) = self.project.tracks.get_mut(track_index) {
                    for clip in indices.into_iter().rev() {
                        track.clips.remove(clip);
                        changed = true;
                    }
                    track.bump_revision();
                }
            }
            self.selected_clip = None;
            self.selected_clips.clear();
            self.selected_note = None;
            self.selected_notes.clear();
            if changed {
                self.mark_dirty();
                self.status = "已删除片段".into();
            }
        }
    }

    /// Remove one insert effect from a track.
    pub fn remove_effect_at(&mut self, track_id: TrackId, index: usize) {
        self.push_undo_now();
        self.invalidate_plugin_params(track_id);
        let is_master = track_id == crate::core::model::MASTER_TRACK_ID;
        if is_master {
            self.editors
                .retain(|((id, slot), _)| !(*id == track_id && *slot == SlotKind::Master));
        } else {
            self.editors.retain(|((id, slot), _)| {
                !(*id == track_id && matches!(slot, SlotKind::Effect(_)))
            });
        }
        let mut changed = false;
        if is_master {
            if self.project.master_effect.is_some() {
                self.project.master_effect = None;
                changed = true;
            }
        } else if let Some(track) = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
        {
            if index < track.effects.len() {
                track.effects.remove(index);
                track.bump_revision();
                changed = true;
            }
        }
        if changed {
            self.effect_widths.retain(|(id, _), _| *id != track_id);
            self.effect_can_resize.retain(|(id, _), _| *id != track_id);
            self.inline_failed.retain(|(id, _), _| *id != track_id);
            self.effect_editor_flags
                .retain(|(id, _), _| *id != track_id);
            self.selected_effect = None;
            self.mark_dirty();
            self.status = "已删除效果器".into();
        }
    }

    /// 所属模块：app / 编辑命令
    /// 功能：向轨道或主输出效果链追加一个内置原生效果器。
    /// 输入：
    /// - `track_id`：目标轨道；`MASTER_TRACK_ID` 表示主输出（单槽位，直接替换）。
    /// - `kind`：内置效果器类型。
    /// 输出/副作用：修改工程并标记脏；主输出会关闭旧效果器编辑器并打开效果器卷帘。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / editing commands
    /// Purpose: Append a built-in native effect to a track or the master chain.
    /// Inputs:
    /// - `track_id`: target track; `MASTER_TRACK_ID` selects the single master slot (replaced).
    /// - `kind`: native effect kind.
    /// Outputs / side effects: edits the project and marks it dirty; the master path closes the
    /// old editor and opens the effect roll.
    /// Complexity: O(1) time and space.
    pub fn add_native_effect(
        &mut self,
        track_id: TrackId,
        kind: crate::core::model::NativeEffectKind,
    ) {
        self.push_undo_now();
        let effect =
            crate::core::model::EffectRef::Native(crate::core::model::NativeEffect::new(kind));
        if track_id == crate::core::model::MASTER_TRACK_ID {
            self.close_editor(track_id, SlotKind::Master);
            self.project.master_effect = Some(effect);
            self.bottom_view = BottomView::Effects { track_id };
            self.bottom_open = true;
            self.effects_scroll = 0.0;
        } else if let Some(track) = self.project.tracks.iter_mut().find(|t| t.id == track_id) {
            track.effects.push(effect);
            track.bump_revision();
            self.effects_scroll = 0.0;
        } else {
            return;
        }
        self.invalidate_plugin_params(track_id);
        self.mark_dirty();
        self.status = format!("已添加内置效果器 · {}", kind.label());
    }

    /// Hide a controller lane and reset its automation to the default value.
    pub fn remove_controller(&mut self, track_id: TrackId, kind: ControllerKind) {
        self.push_undo_now();
        if self.bottom_open {
            if let BottomView::Controller {
                track_id: open_track,
                kind: open_kind,
            } = self.bottom_view
            {
                if open_track == track_id && open_kind == kind {
                    self.bottom_open = false;
                }
            }
        }
        if track_id == crate::core::model::MASTER_TRACK_ID {
            // The master lane only hosts volume; it can be cleared but never removed.
            self.project.master_controllers.get_mut(kind).clear();
        } else if let Some(track) = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
        {
            track.controllers.get_mut(kind).clear();
            track.visible_controllers.retain(|value| *value != kind);
            track.bump_revision();
        }
        self.selected_controller = None;
        self.controller_selection.clear();
        self.controller_marquee_start = None;
        self.mark_dirty();
        self.status = "已删除控制器".into();
    }

    /// Copy an effect card or a whole controller/effects child lane when one is
    /// selected. Returns `true` when the clipboard was updated.
    fn copy_child_lane_selection(&mut self) -> bool {
        // In the piano roll, a note selection must win over a stale child-lane
        // selection made earlier in the arrangement.
        if self.bottom_open
            && matches!(self.bottom_view, BottomView::PianoRoll)
            && (!self.selected_notes.is_empty() || self.selected_note.is_some())
        {
            return false;
        }

        // 1. Individual effect card selected in the effects roll (or still
        // selected after the roll was closed).
        if let Some((track_id, index)) = self.selected_effect {
            let pref = self
                .project
                .tracks
                .iter()
                .find(|track| track.id == track_id)
                .and_then(|track| track.effects.get(index))
                .map(|pref| pref.clone());
            if let Some(pref) = pref {
                self.clipboard = Clipboard::Effect(pref);
                self.status = "已复制效果器".into();
                return true;
            }
        }

        // 2. Controller child lane selected in the arrangement.
        if let Some((track_id, kind)) = self.selected_controller {
            let points = self.lane_points(track_id, kind);
            self.clipboard = Clipboard::ControllerLane { kind, points };
            self.status = format!("已复制子轨 · {}", kind.label());
            return true;
        }

        // 3. Effects child lane selected in the arrangement.
        if let Some(track_id) = self.selected_effects_lane {
            if let Some(track) = self
                .project
                .tracks
                .iter()
                .find(|track| track.id == track_id)
            {
                let chain = track.effects.clone();
                self.status = format!("已复制效果器链（{} 个）", chain.len());
                self.clipboard = Clipboard::EffectChain(chain);
                return true;
            }
        }

        // 4. Nothing selected: copy whatever the open roll is showing.
        if self.bottom_open {
            match self.bottom_view {
                BottomView::Effects { track_id } => {
                    if track_id == crate::core::model::MASTER_TRACK_ID {
                        if let Some(pref) = self.project.master_effect.clone() {
                            self.clipboard = Clipboard::Effect(pref);
                            self.status = "已复制效果器".into();
                            return true;
                        }
                        return false;
                    }
                    if let Some(track) = self
                        .project
                        .tracks
                        .iter()
                        .find(|track| track.id == track_id)
                    {
                        let chain = track.effects.clone();
                        self.status = format!("已复制效果器链（{} 个）", chain.len());
                        self.clipboard = Clipboard::EffectChain(chain);
                        return true;
                    }
                }
                BottomView::Controller { track_id, kind } => {
                    let points = self.lane_points(track_id, kind);
                    self.clipboard = Clipboard::ControllerLane { kind, points };
                    self.status = format!("已复制子轨 · {}", kind.label());
                    return true;
                }
                BottomView::PianoRoll => {}
            }
        }
        false
    }

    fn lane_points(&self, track_id: TrackId, kind: ControllerKind) -> Vec<AutomationPoint> {
        if track_id == crate::core::model::MASTER_TRACK_ID {
            self.project.master_controllers.get(kind).to_vec()
        } else {
            self.project
                .tracks
                .iter()
                .find(|track| track.id == track_id)
                .map(|track| track.controllers.get(kind).to_vec())
                .unwrap_or_default()
        }
    }

    /// Apply a child-lane paste, replacing the content on `target_track`.
    pub(super) fn apply_lane_paste(&mut self, target_track: usize, payload: Clipboard) {
        let Some(track_id) = self.project.tracks.get(target_track).map(|track| track.id) else {
            return;
        };
        match payload {
            Clipboard::ControllerLane { kind, mut points } => {
                self.push_undo_now();
                points.sort_by(|a, b| a.beat.total_cmp(&b.beat));
                if let Some(track) = self.project.tracks.get_mut(target_track) {
                    track.controllers.get_mut(kind).clear();
                    track.controllers.get_mut(kind).extend(points);
                    if !track.visible_controllers.contains(&kind) {
                        track.visible_controllers.push(kind);
                    }
                    track.expanded = true;
                    track.bump_revision();
                }
                self.selected_track = target_track;
                self.selected_controller = Some((track_id, kind));
                self.selected_effects_lane = None;
                self.controller_selection.clear();
                self.controller_marquee_start = None;
                self.mark_dirty();
                self.status = format!("已粘贴子轨 · {}", kind.label());
            }
            Clipboard::EffectChain(effects) => {
                self.push_undo_now();
                self.editors.retain(|((id, slot), _)| {
                    !(*id == track_id && matches!(slot, SlotKind::Effect(_)))
                });
                let count = effects.len();
                if let Some(track) = self.project.tracks.get_mut(target_track) {
                    track.effects = effects;
                    track.bump_revision();
                }
                self.effect_widths.retain(|(id, _), _| *id != track_id);
                self.effect_can_resize.retain(|(id, _), _| *id != track_id);
                self.inline_failed.retain(|(id, _), _| *id != track_id);
                self.effect_editor_flags
                    .retain(|(id, _), _| *id != track_id);
                self.selected_track = target_track;
                self.selected_effects_lane = Some(track_id);
                self.selected_effect = None;
                self.mark_dirty();
                self.status = format!("已粘贴效果器链（{count} 个）");
            }
            _ => {}
        }
    }

    /// Ctrl+C: copy clips, notes or controller points depending on the focus.
    pub fn copy_selection(&mut self) {
        if self.bottom_open {
            if let BottomView::Controller { track_id, kind } = self.bottom_view {
                if !self.controller_selection.is_empty() {
                    let lane: Vec<AutomationPoint> =
                        if track_id == crate::core::model::MASTER_TRACK_ID {
                            self.project.master_controllers.get(kind).to_vec()
                        } else {
                            self.project
                                .tracks
                                .iter()
                                .find(|track| track.id == track_id)
                                .map(|track| track.controllers.get(kind).to_vec())
                                .unwrap_or_default()
                        };
                    let points: Vec<AutomationPoint> = lane
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| self.controller_selection.contains(index))
                        .map(|(_, point)| point.clone())
                        .collect();
                    if !points.is_empty() {
                        self.clipboard = Clipboard::ControllerPoints(points);
                        self.status = "已复制控制器点位".into();
                        return;
                    }
                }
            }
        }

        // Whole child lanes and individual effects take priority over notes.
        if self.copy_child_lane_selection() {
            return;
        }

        let note_refs = if !self.selected_notes.is_empty() {
            self.selected_notes.clone()
        } else {
            self.selected_note.into_iter().collect()
        };
        if self.bottom_open
            && matches!(self.bottom_view, BottomView::PianoRoll)
            && !note_refs.is_empty()
        {
            if let Some(track) = self.current_track() {
                let mut notes: Vec<(f64, Note)> = note_refs
                    .iter()
                    .filter_map(|(clip, note)| {
                        let target = track.clips.get(*clip)?;
                        let note = target.notes.get(*note)?;
                        // Keep positions relative to the earliest selected note
                        // so multi-clip copies keep their spacing.
                        Some((target.start + note.start, note.clone()))
                    })
                    .collect();
                if !notes.is_empty() {
                    let base = notes
                        .iter()
                        .map(|(abs, _)| *abs)
                        .fold(f64::INFINITY, f64::min);
                    let list: Vec<Note> = notes
                        .drain(..)
                        .map(|(abs, mut note)| {
                            note.start = abs - base;
                            note
                        })
                        .collect();
                    self.clipboard = Clipboard::Notes(list);
                    self.status = "已复制音符".into();
                    return;
                }
            }
        }

        let clip_refs: Vec<(usize, usize)> = if !self.selected_clips.is_empty() {
            self.selected_clips.clone()
        } else {
            self.selected_clip
                .map(|clip| (self.selected_track, clip))
                .into_iter()
                .collect()
        };
        let mut clips: Vec<(usize, Clip)> = Vec::new();
        for (track_index, clip_index) in clip_refs {
            if let Some(clip) = self
                .project
                .tracks
                .get(track_index)
                .and_then(|track| track.clips.get(clip_index))
            {
                clips.push((track_index, clip.clone()));
            }
        }
        if !clips.is_empty() {
            let count = clips.len();
            self.clipboard = Clipboard::Clips(clips);
            self.status = format!("已复制 {count} 个 MIDI 片段");
        }
    }

    /// Ctrl+V: paste the clipboard at the playhead.
    pub fn paste_clipboard(&mut self) {
        match self.clipboard.clone() {
            Clipboard::Empty => {
                self.status = "剪贴板为空".into();
            }
            Clipboard::ControllerLane { kind, points } => {
                if self.project.tracks.is_empty() {
                    return;
                }
                let target = self.selected_track.min(self.project.tracks.len() - 1);
                let occupied = self
                    .project
                    .tracks
                    .get(target)
                    .map(|track| {
                        track.visible_controllers.contains(&kind)
                            || !track.controllers.get(kind).is_empty()
                    })
                    .unwrap_or(false);
                if occupied {
                    self.lane_paste_prompt = Some(LanePastePrompt {
                        target_track: target,
                        payload: Clipboard::ControllerLane { kind, points },
                    });
                } else {
                    self.apply_lane_paste(target, Clipboard::ControllerLane { kind, points });
                }
            }
            Clipboard::Effect(pref) => {
                if self.project.tracks.is_empty() {
                    return;
                }
                let target = self.selected_track.min(self.project.tracks.len() - 1);
                let track_id = self.project.tracks[target].id;
                self.push_undo_now();
                let mut index = 0;
                if let Some(track) = self.project.tracks.get_mut(target) {
                    track.effects.push(pref);
                    index = track.effects.len() - 1;
                    track.bump_revision();
                }
                self.selected_track = target;
                self.selected_effects_lane = Some(track_id);
                self.selected_effect = Some((track_id, index));
                self.mark_dirty();
                self.status = "已粘贴效果器".into();
            }
            Clipboard::EffectChain(effects) => {
                if self.project.tracks.is_empty() {
                    return;
                }
                let target = self.selected_track.min(self.project.tracks.len() - 1);
                let occupied = self
                    .project
                    .tracks
                    .get(target)
                    .map(|track| !track.effects.is_empty())
                    .unwrap_or(false);
                if occupied {
                    self.lane_paste_prompt = Some(LanePastePrompt {
                        target_track: target,
                        payload: Clipboard::EffectChain(effects),
                    });
                } else {
                    self.apply_lane_paste(target, Clipboard::EffectChain(effects));
                }
            }
            Clipboard::Clips(clips) => {
                if self.project.tracks.is_empty() {
                    return;
                }
                let grid = if self.snap_beats > 0.0 {
                    self.snap_beats
                } else {
                    0.25
                };
                let base = (self.playhead() / grid).round() * grid;
                let first = clips
                    .iter()
                    .map(|(_, clip)| clip.start)
                    .fold(f64::INFINITY, f64::min);
                // Pasting exactly over the source would look like nothing happened.
                let base = if (base - first).abs() < 1e-6 {
                    base + self.project.beats_per_bar().max(1.0)
                } else {
                    base
                };
                let mut new_indices = Vec::new();
                let mut rejected = 0usize;
                for (track_index, clip) in clips {
                    // Clips always paste back onto their original track, so a
                    // cross-track copy keeps its arrangement.
                    let target = track_index.min(self.project.tracks.len() - 1);
                    let id = self.project.next_clip_id();
                    let mut clip = clip;
                    clip.id = id;
                    clip.start = (base + (clip.start - first)).max(0.0);
                    clip.bump_revision();
                    if let Some(track) = self.project.tracks.get_mut(target) {
                        if track.accepts_clip(&clip) {
                            track.clips.push(clip);
                            new_indices.push((target, track.clips.len() - 1));
                        } else {
                            rejected += 1;
                        }
                    }
                }
                if !new_indices.is_empty() {
                    self.selected_track = new_indices.last().map(|(track, _)| *track).unwrap_or(0);
                    self.selected_clip = new_indices.last().map(|(_, clip)| *clip);
                    self.selected_clips = new_indices;
                    self.mark_dirty();
                    self.status = if rejected > 0 {
                        format!("已粘贴部分片段（{rejected} 个因轨道类型冲突被跳过）")
                    } else {
                        "已粘贴 MIDI 片段".into()
                    };
                } else if rejected > 0 {
                    self.status = "目标轨道已有另一种类型的片段，无法粘贴".into();
                }
            }
            Clipboard::Notes(notes) => {
                let playhead = self.playhead();
                let mut pasted: Vec<(usize, usize)> = Vec::new();
                if let Some(clip_index) = self.selected_clip {
                    if let Some(track) = self.current_track_mut() {
                        if let Some(clip) = track.clips.get_mut(clip_index) {
                            let first = notes
                                .iter()
                                .map(|note| note.start)
                                .fold(f64::INFINITY, f64::min);
                            let base_abs = if (playhead - clip.start - first).abs() < 1e-6 {
                                playhead + 1.0
                            } else {
                                playhead
                            };
                            for note in notes {
                                let mut note = note;
                                let abs = base_abs + (note.start - first);
                                note.start = (abs - clip.start).clamp(0.0, clip.length);
                                let max_start = (clip.length - note.length).max(0.0);
                                note.start = note.start.min(max_start);
                                clip.notes.push(note);
                                pasted.push((clip_index, clip.notes.len() - 1));
                            }
                            clip.bump_revision();
                        }
                    }
                }
                if !pasted.is_empty() {
                    self.selected_note = pasted.last().copied();
                    self.selected_notes = pasted;
                    self.bump_selected_clip_revision();
                    self.status = "已粘贴音符".into();
                }
            }
            Clipboard::ControllerPoints(points) => {
                if !self.bottom_open {
                    return;
                }
                if let BottomView::Controller { track_id, kind } = self.bottom_view {
                    let playhead = self.playhead();
                    let first = points
                        .iter()
                        .map(|point| point.beat)
                        .fold(f64::INFINITY, f64::min);
                    let base = if (playhead - first).abs() < 1e-6 {
                        playhead + 1.0
                    } else {
                        playhead
                    };
                    if let Some(lane) = self.controller_lane_mut(track_id, kind) {
                        for point in points {
                            let mut point = point;
                            point.beat = (base + (point.beat - first)).max(0.0);
                            lane.push(point);
                        }
                        lane.sort_by(|a, b| a.beat.total_cmp(&b.beat));
                    }
                    self.mark_dirty();
                    self.status = "已粘贴控制器点位".into();
                }
            }
        }
    }

    pub(super) fn validate_selection(&mut self) {
        if self.selected_track >= self.project.tracks.len() {
            self.selected_track = self.project.tracks.len().saturating_sub(1);
        }
        if let Some(clip) = self.selected_clip {
            let valid = self
                .project
                .tracks
                .get(self.selected_track)
                .map(|t| clip < t.clips.len())
                .unwrap_or(false);
            if !valid {
                self.selected_clip = None;
                self.selected_note = None;
                self.selected_notes.clear();
            }
        }
        self.selected_clips.retain(|(track, clip)| {
            self.project
                .tracks
                .get(*track)
                .map(|t| *clip < t.clips.len())
                .unwrap_or(false)
        });
        let mut alive: Vec<(usize, usize)> = Vec::new();
        if let Some(track) = self.current_track() {
            for (clip_index, clip) in track.clips.iter().enumerate() {
                for note_index in 0..clip.notes.len() {
                    alive.push((clip_index, note_index));
                }
            }
        }
        if let Some(note) = self.selected_note {
            if !alive.contains(&note) {
                self.selected_note = None;
            }
        }
        self.selected_notes.retain(|note| alive.contains(note));
    }
}
