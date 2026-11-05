//! 所属模块：app / 编辑命令
//! 文件用途：
//! - 全选、删除、复制、粘贴与子轨覆盖替换。
//! - 效果器/控制器子轨的增删与剪贴板应用。
//!
//! 函数清单：
//! - `select_all`：按当前编辑上下文全选控制器点位、音符、效果器链或片段。
//! - `delete_selection`：按优先级删除控制器点、音符、效果器、子轨或片段。
//! - `selected_same_track_clips` / `merge_selected_clips`：同一轨道多选片段的合并判断与执行。
//! - `insert_track_below`：在指定轨道子树下方插入一条同类型新轨道（继承上级组合）。
//! - `is_track_selected` / `toggle_track_selection`：轨道多选查询与切换。
//! - `create_group_from_selection`：把多选轨道创建为组合轨道。
//! - `request_delete_track` / `confirm_group_delete`：删除轨道与组合的“整棵删除 / 释放成员”确认。
//! - `remove_effect_at` / `add_native_effect` / `remove_controller`：效果器与控制器子轨增删。
//! - `copy_child_lane_selection` / `lane_points` / `apply_lane_paste`：子轨复制与覆盖粘贴。
//! - `copy_selection` / `cut_selection` / `copy_selection_synced` / `cut_selection_synced` /
//!   `paste_clipboard`：复制、剪切与粘贴入口（含系统剪贴板同步）。
//! - `validate_selection`：结构变化后校正失效选择（含轨道多选）。
//!
//! Module: app / editing commands
//! Purpose:
//! - Select all, delete, copy, paste and child-lane replacement.
//! - Add/remove effect and controller child lanes and apply lane clipboard data.
//!
//! Functions:
//! - `select_all`: select controller points, notes, the effect chain or clips by context.
//! - `delete_selection`: delete controller points, notes, effects, child lanes or clips in priority order.
//! - `insert_track_below`: insert a new track of the same kind below the given track's subtree
//!   (inheriting its parent group).
//! - `is_track_selected` / `toggle_track_selection`: track multi-selection query and toggle.
//! - `create_group_from_selection`: build a group track from the multi-selection.
//! - `remove_effect_at` / `add_native_effect` / `remove_controller`: effect and controller lane editing.
//! - `copy_child_lane_selection` / `lane_points` / `apply_lane_paste`: child-lane copy and overwrite paste.
//! - `copy_selection` / `cut_selection` / `copy_selection_synced` / `cut_selection_synced` /
//!   `paste_clipboard`: copy, cut and paste entry points (with OS-clipboard sync).
//! - `validate_selection`: fix stale selections after structural changes (including track multi-select).

use super::*;

/// 写入系统剪贴板的占位文本。
/// 说明：egui-winit 只有在系统剪贴板非空时才会把 Ctrl+V 转成 `Event::Paste`，
/// 因此应用内复制/剪切后必须放一段非空文本，键盘粘贴才能被识别。
///
/// Placeholder text written to the OS clipboard.
/// Why: egui-winit turns Ctrl+V into `Event::Paste` only when the OS clipboard is non-empty, so an
/// in-app copy/cut must leave non-empty text for keyboard paste to be recognised.
const SYSTEM_CLIPBOARD_MARKER: &str = "SurfingStudio 剪贴板";

impl SurfingApp {
    /// 所属模块：app / 编辑命令
    /// 功能：Ctrl+A 全选当前编辑上下文：控制器点位、当前卷帘音符或编曲区全部片段。
    /// 输入：`self`。
    /// 输出/副作用：重写选择状态并更新状态栏；不修改工程内容。
    /// 复杂度：时间 O(片段数 + 音符数 + 控制点数量)，空间 O(选中项)。
    ///
    /// Module: app / editing commands
    /// Purpose: Ctrl+A selects everything in the current editing context: controller points, the
    /// notes of the open roll, or every arrangement clip.
    /// Inputs: `self`.
    /// Outputs / side effects: replaces the selection and updates the status bar; the project is
    /// untouched.
    /// Complexity: O(clips + notes + controller points) time and O(selection) space.
    pub fn select_all(&mut self) {
        if self.bottom_open {
            match self.bottom_view {
                BottomView::Controller { track_id, kind } => {
                    let len = if track_id == crate::core::model::MASTER_TRACK_ID {
                        self.project.master_controllers.get(kind).len()
                    } else {
                        self.project
                            .tracks
                            .iter()
                            .find(|track| track.id == track_id)
                            .map(|track| track.controllers.get(kind).len())
                            .unwrap_or(0)
                    };
                    if len > 0 {
                        self.selected_track = self
                            .project
                            .tracks
                            .iter()
                            .position(|track| track.id == track_id)
                            .unwrap_or(self.selected_track);
                        self.selected_controller = Some((track_id, kind));
                        self.selected_effects_lane = None;
                        self.selected_effect = None;
                        self.controller_selection = (0..len).collect();
                        self.selected_note = None;
                        self.selected_notes.clear();
                        self.selected_clip = None;
                        self.selected_clips.clear();
                        self.status = format!("已全选 {} 个控制器节点", len);
                        return;
                    }
                }
                BottomView::PianoRoll => {
                    let track_id = self.piano_track.or_else(|| self.current_track_id());
                    let track_index = track_id
                        .and_then(|id| self.project.tracks.iter().position(|track| track.id == id));
                    if let Some(track_index) = track_index {
                        let selected: Vec<(usize, usize)> = self.project.tracks[track_index]
                            .clips
                            .iter()
                            .enumerate()
                            .flat_map(|(clip_index, clip)| {
                                (0..clip.notes.len())
                                    .map(move |note_index| (clip_index, note_index))
                            })
                            .collect();
                        if !selected.is_empty() {
                            self.selected_track = track_index;
                            self.selected_clip = selected.first().map(|(clip, _)| *clip);
                            self.selected_note = selected.last().copied();
                            self.selected_notes = selected;
                            self.selected_controller = None;
                            self.selected_effects_lane = None;
                            self.selected_effect = None;
                            self.controller_selection.clear();
                            self.status = format!("已全选 {} 个音符", self.selected_notes.len());
                            return;
                        }
                    }
                }
                BottomView::Effects { track_id } => {
                    let has_effects = if track_id == crate::core::model::MASTER_TRACK_ID {
                        self.project.master_effect.is_some()
                    } else {
                        self.project
                            .tracks
                            .iter()
                            .find(|track| track.id == track_id)
                            .map(|track| !track.effects.is_empty())
                            .unwrap_or(false)
                    };
                    if has_effects {
                        self.selected_track = self
                            .project
                            .tracks
                            .iter()
                            .position(|track| track.id == track_id)
                            .unwrap_or(self.selected_track);
                        self.selected_effects_lane = Some(track_id);
                        self.selected_effect = None;
                        self.selected_controller = None;
                        self.controller_selection.clear();
                        self.selected_note = None;
                        self.selected_notes.clear();
                        self.selected_clip = None;
                        self.selected_clips.clear();
                        self.status = "已全选效果器链".into();
                        return;
                    }
                }
            }
        }

        let all: Vec<(usize, usize)> = self
            .project
            .tracks
            .iter()
            .enumerate()
            .flat_map(|(track_index, track)| {
                (0..track.clips.len()).map(move |clip_index| (track_index, clip_index))
            })
            .collect();
        if all.is_empty() {
            self.status = "没有可全选的内容".into();
            return;
        }
        self.selected_track = all[0].0;
        self.selected_clip = Some(all[0].1);
        self.selected_clips = all;
        self.selected_note = None;
        self.selected_notes.clear();
        self.selected_effect = None;
        self.selected_controller = None;
        self.selected_effects_lane = None;
        self.controller_selection.clear();
        self.status = format!("已全选 {} 个片段", self.selected_clips.len());
    }

    /// 所属模块：app / 编辑命令
    /// 功能：Delete 的全局删除入口：按优先级删除控制器点、音符、效果器、控制器子轨、片段；
    /// 没有这些选择时删除当前选中轨道（组合轨道转为确认弹窗）。
    /// 输入：`self`。
    /// 输出/副作用：修改工程或设置 `group_delete_prompt`；更新状态栏。
    /// 复杂度：时间 O(片段数 + 音符数 + 控制点数量)，空间 O(选中项)。
    ///
    /// Module: app / editing commands
    /// Purpose: Global Delete entry: remove controller points, notes, effects, controller lanes or
    /// clips in priority order; with none selected it removes the current track (groups open the
    /// confirmation modal).
    /// Inputs: `self`.
    /// Outputs / side effects: edits the project or sets `group_delete_prompt`; updates the status bar.
    /// Complexity: O(clips + notes + controller points) time and O(selection) space.
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
            return;
        }

        // 6. 没有可删除的片段/音符/效果器/控制器时，Delete 作为全局快捷键删除当前选中轨道。
        // With nothing else selected, Delete acts as the global shortcut and removes the current
        // track; groups open the delete confirmation.
        if !self.project.tracks.is_empty() {
            self.request_delete_track(self.selected_track);
        }
    }

    /// 所属模块：app / 编辑命令
    /// 功能：返回同一轨道上的多选片段索引；跨轨道或不足两个片段时返回 `None`。
    /// 输入：`self`。
    /// 输出/副作用：`(轨道索引, 片段索引列表)`；不修改状态。
    /// 复杂度：时间 O(选中片段数)，空间 O(选中片段数)。
    ///
    /// Module: app / editing commands
    /// Purpose: Return the selected clip indices when every selected clip is on the same track;
    /// `None` for a cross-track selection or fewer than two clips.
    /// Inputs: `self`.
    /// Outputs / side effects: `(track index, clip indices)`; no state change.
    /// Complexity: O(selected clips) time and space.
    pub fn selected_same_track_clips(&self) -> Option<(usize, Vec<usize>)> {
        let (track_index, _) = *self.selected_clips.first()?;
        if self.selected_clips.len() < 2 {
            return None;
        }
        if self
            .selected_clips
            .iter()
            .any(|(track, _)| *track != track_index)
        {
            return None;
        }
        Some((
            track_index,
            self.selected_clips.iter().map(|(_, clip)| *clip).collect(),
        ))
    }

    /// 所属模块：app / 编辑命令
    /// 功能：把同一轨道上选中的多个 MIDI 片段合并为一个片段：范围取并集，音符换算到新起点。
    /// 输入：`self`。
    /// 输出/副作用：记录撤销、删除被合并片段、写入合并片段并更新选择；音频片段拒绝合并。
    /// 复杂度：时间 O(n log n)，空间 O(n)，`n` = 被合并片段的音符总数。
    ///
    /// Module: app / editing commands
    /// Purpose: Merge the selected MIDI clips of one track into a single clip: the range is the
    /// union and notes are rebased to the new start.
    /// Inputs: `self`.
    /// Outputs / side effects: records undo, removes the merged clips, writes the merged clip and
    /// updates the selection; audio clips are rejected.
    /// Complexity: O(n log n) time and O(n) space, `n` = notes in the merged clips.
    pub fn merge_selected_clips(&mut self) {
        let Some((track_index, mut indices)) = self.selected_same_track_clips() else {
            self.status = "请在同一轨道上选中至少两个片段".into();
            return;
        };
        indices.sort_unstable();
        indices.dedup();
        let Some(track) = self.project.tracks.get(track_index) else {
            return;
        };
        let mut clips: Vec<(usize, Clip)> = indices
            .iter()
            .filter_map(|index| track.clips.get(*index).cloned().map(|clip| (*index, clip)))
            .collect();
        if clips.len() < 2 {
            self.status = "请在同一轨道上选中至少两个片段".into();
            return;
        }
        if clips.iter().any(|(_, clip)| clip.is_audio()) {
            self.status = "音频片段暂不支持合并".into();
            return;
        }
        // 以起点最早的片段作为合并基准，保留其 id、名称与颜色。
        // Use the earliest clip as the merge base and keep its id, name and colour.
        clips.sort_by(|left, right| {
            left.1
                .start
                .partial_cmp(&right.1.start)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(left.0.cmp(&right.0))
        });
        let base_start = clips[0].1.start;
        let base_end = clips
            .iter()
            .map(|(_, clip)| clip.end())
            .fold(base_start, f64::max);
        let mut notes: Vec<Note> = Vec::new();
        for (_, clip) in &clips {
            let offset = clip.start - base_start;
            for note in &clip.notes {
                notes.push(Note {
                    start: (note.start + offset).max(0.0),
                    length: note.length,
                    pitch: note.pitch,
                    velocity: note.velocity,
                });
            }
        }
        notes.sort_by(|left, right| {
            left.start
                .partial_cmp(&right.start)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(left.pitch.cmp(&right.pitch))
        });
        let base = &clips[0].1;
        let mut merged = Clip::new(
            base.id,
            base.name.clone(),
            base_start,
            (base_end - base_start).max(1e-6),
        );
        merged.notes = notes;
        merged.color = base.color;
        let merged_len = merged.length;
        let note_count = merged.notes.len();
        self.push_undo_now();
        let Some(track) = self.project.tracks.get_mut(track_index) else {
            return;
        };
        let mut remove: Vec<usize> = clips.iter().map(|(index, _)| *index).collect();
        remove.sort_unstable();
        remove.dedup();
        for index in remove.into_iter().rev() {
            if index < track.clips.len() {
                track.clips.remove(index);
            }
        }
        track.clips.push(merged);
        track.bump_revision();
        let clip_index = track.clips.len() - 1;
        self.selected_track = track_index;
        self.selected_clip = Some(clip_index);
        self.selected_clips = vec![(track_index, clip_index)];
        self.selected_note = None;
        self.selected_notes.clear();
        self.mark_dirty();
        self.status = format!(
            "已合并 {} 个 MIDI 片段（{note_count} 个音符，{merged_len:.2} 拍）",
            clips.len()
        );
    }

    /// 所属模块：app / 编辑命令
    /// 功能：在指定轨道下方插入一条同类型新轨道，并选中新轨道。
    /// 输入：`track_index`：参考轨道索引；越界时追加到末尾。
    /// 输出/副作用：记录撤销、插入轨道、清空片段/音符选择并标记脏。
    /// 复杂度：时间 O(轨道数)，空间 O(1)。
    ///
    /// Module: app / editing commands
    /// Purpose: Insert a new track of the same kind below the given track and select it.
    /// Inputs: `track_index` — reference track index; out-of-range appends.
    /// Outputs / side effects: records undo, inserts the track, clears clip/note selection and marks
    /// the project dirty.
    /// Complexity: O(tracks) time and O(1) space.
    pub fn insert_track_below(&mut self, track_index: usize) {
        let (is_audio, parent, insert_at) = self
            .project
            .tracks
            .get(track_index)
            .map(|track| {
                if track.is_group() {
                    // 在组合轨道下方插入的轨道直接成为它的成员。
                    // A track inserted below a group becomes one of its members.
                    (false, Some(track.id), track_index.saturating_add(1))
                } else {
                    (
                        track.is_audio_lane(),
                        track.group,
                        self.project.subtree_end(track_index),
                    )
                }
            })
            .unwrap_or((false, None, track_index.saturating_add(1)));
        self.push_undo_now();
        let index = self.project.insert_track(
            insert_at,
            crate::core::model::InstrumentRef::Synth(crate::core::model::SynthSettings::default()),
        );
        // 新轨道与参考轨道同属一个上级组合，保持在组合内部。
        // The new track shares the reference track's parent so it stays inside the group.
        if let Some(track) = self.project.tracks.get_mut(index) {
            track.group = parent;
            if is_audio {
                track.kind = crate::core::model::TrackKind::Audio;
                track.name = format!("音频轨道 {}", track.id);
            }
        }
        self.selected_track = index;
        self.selected_tracks = vec![index];
        self.selected_clip = None;
        self.selected_clips.clear();
        self.selected_note = None;
        self.selected_notes.clear();
        self.mark_dirty();
        self.status = "已在下方插入轨道".into();
    }

    /// 所属模块：app / 编辑命令
    /// 功能：判断某条轨道是否在当前多选中。
    /// 输入：`track_index`：轨道下标。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(选中数)，空间 O(1)。
    ///
    /// Module: app / editing commands
    /// Purpose: Whether a track is part of the current multi-selection.
    /// Inputs: `track_index`: track index.
    /// Outputs / side effects: the boolean; no state change.
    /// Complexity: O(selected tracks) time and O(1) space.
    pub fn is_track_selected(&self, track_index: usize) -> bool {
        self.selected_tracks.contains(&track_index)
    }

    /// 所属模块：app / 编辑命令
    /// 功能：Ctrl/Command 单击轨道时切换多选；主选随之更新。
    /// 输入：`track_index`：被点击的轨道下标。
    /// 输出/副作用：修改轨道选择并清理片段/音符选择；不修改工程。
    /// 复杂度：时间 O(选中数)，空间 O(选中数)。
    ///
    /// Module: app / editing commands
    /// Purpose: Toggle a track in the multi-selection on Ctrl/Command click; the primary selection
    /// follows.
    /// Inputs: `track_index`: clicked track index.
    /// Outputs / side effects: updates track selection and clears clip/note selection; the project is
    /// untouched.
    /// Complexity: O(selected tracks) time and space.
    pub fn toggle_track_selection(&mut self, track_index: usize) {
        if track_index >= self.project.tracks.len() {
            return;
        }
        if let Some(position) = self
            .selected_tracks
            .iter()
            .position(|index| *index == track_index)
        {
            // 至少保留一条选中轨道，避免后续命令没有目标。
            // Always keep one selected track so commands still have a target.
            if self.selected_tracks.len() > 1 {
                self.selected_tracks.remove(position);
                self.selected_track = self.selected_tracks[0];
            }
        } else {
            self.selected_tracks.push(track_index);
            self.selected_tracks.sort_unstable();
            self.selected_track = track_index;
        }
        self.selected_clip = None;
        self.selected_clips.clear();
        self.selected_note = None;
        self.selected_notes.clear();
        self.selected_controller = None;
        self.selected_effect = None;
        self.selected_effects_lane = None;
    }

    /// 所属模块：app / 编辑命令
    /// 功能：把当前多选的轨道创建为一个组合轨道，并选中该组合轨道。
    /// 输入：`self`。
    /// 输出/副作用：记录撤销、重排工程并校正选择；失败时只写状态栏。
    /// 复杂度：时间 O(轨道数²)，空间 O(轨道数)。
    ///
    /// Module: app / editing commands
    /// Purpose: Create a group track from the current multi-selection and select it.
    /// Inputs: `self`.
    /// Outputs / side effects: records undo, reorders the project and fixes the selection; failures
    /// only write the status bar.
    /// Complexity: O(tracks²) time and O(tracks) space.
    pub fn create_group_from_selection(&mut self) {
        let members = self.selected_tracks.clone();
        if members.len() < 2 {
            self.status = "请先用 Ctrl 单击选择两条及以上轨道".into();
            return;
        }
        self.push_undo_now();
        match self.project.create_group(&members, None) {
            Ok(index) => {
                self.selected_track = index;
                self.selected_tracks = vec![index];
                self.selected_clip = None;
                self.selected_clips.clear();
                self.selected_note = None;
                self.selected_notes.clear();
                self.selected_controller = None;
                self.selected_effect = None;
                self.selected_effects_lane = None;
                self.mark_dirty();
                self.status = format!(
                    "已创建{}",
                    self.project
                        .tracks
                        .get(index)
                        .map(|track| track.name.clone())
                        .unwrap_or_else(|| "组合轨道".into())
                );
            }
            Err(error) => self.status = error,
        }
    }

    /// 所属模块：app / 编辑命令
    /// 功能：请求删除一条轨道：普通轨道立即删除；组合轨道弹出“整棵删除 / 释放成员”确认。
    /// 输入：`track_index`：目标轨道下标。
    /// 输出/副作用：普通轨道会记录撤销并删除；组合轨道只设置 `group_delete_prompt`。
    /// 复杂度：时间 O(轨道数)，空间 O(1)。
    ///
    /// Module: app / editing commands
    /// Purpose: Request track deletion: ordinary tracks are removed immediately, while a group opens
    /// the "delete subtree / release members" confirmation.
    /// Inputs: `track_index`: target track index.
    /// Outputs / side effects: ordinary tracks are undoably removed; a group only fills
    /// `group_delete_prompt`.
    /// Complexity: O(tracks) time and O(1) space.
    pub fn request_delete_track(&mut self, track_index: usize) {
        if self.track_render.is_rendering() {
            self.status = "正在渲染轨道音频，暂时无法删除轨道".into();
            return;
        }
        let Some(track) = self.project.tracks.get(track_index) else {
            return;
        };
        if track.is_group() {
            self.group_delete_prompt = Some(track.id);
            self.status = format!("删除组合轨道「{}」：请选择删除方式", track.name);
        } else {
            self.delete_track(track.id, GroupDeleteMode::Release);
        }
    }

    /// 所属模块：app / 编辑命令
    /// 功能：按选定方式删除组合轨道并清理界面状态。
    /// 输入：
    /// - `group_id`：组合轨道 id。
    /// - `mode`：整棵删除或只删组合、成员释放到上级。
    /// 输出/副作用：记录撤销、删除轨道、清理编辑器/卷帘/插件缓存并选中邻近轨道。
    /// 复杂度：时间 O(轨道数 + 片段数)，空间 O(被删除轨道数)。
    ///
    /// Module: app / editing commands
    /// Purpose: Delete a group with the chosen mode and clean up UI state.
    /// Inputs:
    /// - `group_id`: group track id.
    /// - `mode`: delete the whole subtree or only the group and release members to its parent.
    /// Outputs / side effects: records undo, removes tracks, clears editors/rolls/plugin caches and
    /// selects a neighbouring track.
    /// Complexity: O(tracks + clips) time and O(removed tracks) space.
    pub fn confirm_group_delete(&mut self, group_id: TrackId, mode: GroupDeleteMode) {
        self.delete_track(group_id, mode);
    }

    /// 所属模块：app / 编辑命令
    /// 功能：删除一条轨道（或整棵组合子树）并统一清理界面状态。
    /// 输入：
    /// - `track_id`：目标轨道 id。
    /// - `mode`：组合轨道使用整棵删除还是释放成员；普通轨道忽略该参数。
    /// 输出/副作用：记录撤销并修改工程；关闭被删除轨道的编辑器、卷帘与缓存。
    /// 复杂度：时间 O(轨道数 + 片段数)，空间 O(被删除轨道数)。
    ///
    /// Module: app / editing commands
    /// Purpose: Remove one track (or a whole group subtree) and clean up UI state.
    /// Inputs:
    /// - `track_id`: target track id.
    /// - `mode`: for groups, delete the subtree or release members; ignored for ordinary tracks.
    /// Outputs / side effects: records undo, edits the project and closes editors/rolls/caches of
    /// removed tracks.
    /// Complexity: O(tracks + clips) time and O(removed tracks) space.
    fn delete_track(&mut self, track_id: TrackId, mode: GroupDeleteMode) {
        let Some(index) = self.project.track_index_of(track_id) else {
            self.group_delete_prompt = None;
            return;
        };
        self.push_undo_now();
        let removed = if mode == GroupDeleteMode::Whole {
            self.project.remove_subtree(index)
        } else {
            self.project
                .remove_track_at(index)
                .into_iter()
                .collect::<Vec<_>>()
        };
        if removed.is_empty() {
            self.group_delete_prompt = None;
            return;
        }
        let name = removed[0].name.clone();
        let member_count = removed.len().saturating_sub(1);
        self.cleanup_removed_tracks(&removed);
        self.selected_track = self
            .selected_track
            .min(self.project.tracks.len().saturating_sub(1));
        self.selected_tracks = vec![self.selected_track];
        self.selected_clip = None;
        self.selected_clips.clear();
        self.selected_note = None;
        self.selected_notes.clear();
        self.selected_controller = None;
        self.selected_effect = None;
        self.selected_effects_lane = None;
        self.group_delete_prompt = None;
        self.mark_dirty();
        self.status = match mode {
            GroupDeleteMode::Whole if member_count > 0 => {
                format!("已删除组合轨道「{name}」及 {member_count} 条成员")
            }
            GroupDeleteMode::Release if member_count > 0 => {
                format!("已删除组合轨道「{name}」，成员已释放到上级")
            }
            _ => format!("已删除轨道「{name}」"),
        };
    }

    /// 所属模块：app / 编辑命令
    /// 功能：关闭并清理被删除轨道相关的编辑器、浮动卷帘、卷帘视图与插件缓存。
    /// 输入：`removed`：被删除的轨道。
    /// 输出/副作用：修改界面状态；不修改工程其余内容。
    /// 复杂度：时间 O(被删除轨道数 × 缓存大小)，空间 O(1)。
    ///
    /// Module: app / editing commands
    /// Purpose: Close and clear editors, floating rolls, roll views and plugin caches of removed
    /// tracks.
    /// Inputs: `removed`: removed tracks.
    /// Outputs / side effects: edits UI state only; the rest of the project is untouched.
    /// Complexity: O(removed tracks × cache size) time and O(1) space.
    fn cleanup_removed_tracks(&mut self, removed: &[Track]) {
        for track in removed {
            let id = track.id;
            self.editors.retain(|((track_id, _), _)| *track_id != id);
            self.roll_windows.retain(|window| window.track_id != id);
            self.piano_views.remove(&id);
            self.invalidate_plugin_params(id);
            self.effect_widths
                .retain(|(track_id, _), _| *track_id != id);
            self.effect_can_resize
                .retain(|(track_id, _), _| *track_id != id);
            self.inline_failed
                .retain(|(track_id, _), _| *track_id != id);
            self.effect_editor_flags
                .retain(|(track_id, _), _| *track_id != id);
            if let BottomView::Controller { track_id, .. } | BottomView::Effects { track_id } =
                self.bottom_view
            {
                if track_id == id {
                    self.bottom_view = BottomView::PianoRoll;
                }
            }
            if self.selected_effects_lane == Some(id) {
                self.selected_effects_lane = None;
            }
            if self
                .selected_controller
                .is_some_and(|(track_id, _)| track_id == id)
            {
                self.selected_controller = None;
            }
            if self
                .selected_effect
                .is_some_and(|(track_id, _)| track_id == id)
            {
                self.selected_effect = None;
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
    ///
    /// 所属模块：app / 编辑命令
    /// 功能：按当前焦点复制控制器点位、子轨/效果器、音符或片段。
    /// 输入：`self`。
    /// 输出/副作用：命中可复制内容时写入 `clipboard` 并更新状态栏；
    /// 返回是否真的更新了剪贴板。
    /// 复杂度：时间 O(选中项数量)，空间 O(选中项数量)。
    ///
    /// Module: app / editing commands
    /// Purpose: Copy controller points, child lanes/effects, notes or clips by focus.
    /// Inputs: `self`.
    /// Outputs / side effects: writes `clipboard` and the status bar when something is copied;
    /// returns whether the clipboard was updated.
    /// Complexity: O(selected items) time and space.
    pub fn copy_selection(&mut self) -> bool {
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
                        return true;
                    }
                }
            }
        }

        // Whole child lanes and individual effects take priority over notes.
        if self.copy_child_lane_selection() {
            return true;
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
                    return true;
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
        if clips.is_empty() {
            return false;
        }
        let count = clips.len();
        self.clipboard = Clipboard::Clips(clips);
        self.status = format!("已复制 {count} 个 MIDI 片段");
        true
    }

    /// 所属模块：app / 编辑命令
    /// 功能：剪切当前选中内容：先复制，再按删除优先级移除。
    /// 输入：`self`。
    /// 输出/副作用：可能替换剪贴板、删除控制器点位/音符/效果器/片段并标记脏；
    /// 无可剪切内容时只更新状态栏；返回是否真的剪贴了内容。
    /// 复杂度：时间 O(选中项数量)，空间 O(选中项数量)。
    ///
    /// Module: app / editing commands
    /// Purpose: Cut the current selection: copy first, then delete in the priority order.
    /// Inputs: `self`.
    /// Outputs / side effects: may replace the clipboard, delete controller points/notes/effects/
    /// clips and mark the project dirty; only the status bar changes when nothing can be cut;
    /// returns whether anything was cut.
    /// Complexity: O(selected items) time and space.
    pub fn cut_selection(&mut self) -> bool {
        if !self.copy_selection() {
            self.status = "没有可剪切的内容".into();
            return false;
        }
        self.delete_selection();
        self.status = "已剪切".into();
        true
    }

    /// 所属模块：app / 编辑命令
    /// 功能：复制选中内容，并把占位文本写入系统剪贴板。
    /// 输入：`ctx`：egui 上下文，用于提交系统剪贴板命令。
    /// 输出/副作用：等同 `copy_selection`；成功时同时写系统剪贴板；返回是否复制成功。
    /// 复杂度：时间 O(选中项数量)，空间 O(选中项数量)。
    ///
    /// Module: app / editing commands
    /// Purpose: Copy the selection and put the placeholder text on the OS clipboard.
    /// Inputs: `ctx`: egui context used to submit the clipboard command.
    /// Outputs / side effects: same as `copy_selection`; also writes the OS clipboard on success;
    /// returns whether the copy succeeded.
    /// Complexity: O(selected items) time and space.
    pub fn copy_selection_synced(&mut self, ctx: &egui::Context) -> bool {
        if !self.copy_selection() {
            return false;
        }
        ctx.copy_text(SYSTEM_CLIPBOARD_MARKER.to_string());
        true
    }

    /// 所属模块：app / 编辑命令
    /// 功能：剪切选中内容，并把占位文本写入系统剪贴板。
    /// 输入：`ctx`：egui 上下文，用于提交系统剪贴板命令。
    /// 输出/副作用：等同 `cut_selection`；成功时同时写系统剪贴板；返回是否剪切成功。
    /// 复杂度：时间 O(选中项数量)，空间 O(选中项数量)。
    ///
    /// Module: app / editing commands
    /// Purpose: Cut the selection and put the placeholder text on the OS clipboard.
    /// Inputs: `ctx`: egui context used to submit the clipboard command.
    /// Outputs / side effects: same as `cut_selection`; also writes the OS clipboard on success;
    /// returns whether anything was cut.
    /// Complexity: O(selected items) time and space.
    pub fn cut_selection_synced(&mut self, ctx: &egui::Context) -> bool {
        if !self.cut_selection() {
            return false;
        }
        ctx.copy_text(SYSTEM_CLIPBOARD_MARKER.to_string());
        true
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
                // 先做类型校验，避免没有实际粘贴时写入空撤销步骤。
                // Validate types first so a no-op paste does not push an empty undo step.
                let mut valid: Vec<(usize, Clip)> = Vec::new();
                let mut rejected = 0usize;
                for (track_index, clip) in clips {
                    // Clips always paste back onto their original track, so a
                    // cross-track copy keeps its arrangement.
                    let target = track_index.min(self.project.tracks.len() - 1);
                    match self.project.tracks.get(target) {
                        Some(track) if track.accepts_clip(&clip) => valid.push((target, clip)),
                        _ => rejected += 1,
                    }
                }
                if valid.is_empty() {
                    self.status = if rejected > 0 {
                        "目标轨道已有另一种类型的片段，无法粘贴".into()
                    } else {
                        "没有可粘贴的片段".into()
                    };
                    return;
                }
                self.push_undo_now();
                let mut new_indices = Vec::new();
                for (target, mut clip) in valid {
                    let id = self.project.next_clip_id();
                    clip.id = id;
                    clip.start = (base + (clip.start - first)).max(0.0);
                    clip.bump_revision();
                    if let Some(track) = self.project.tracks.get_mut(target) {
                        track.clips.push(clip);
                        new_indices.push((target, track.clips.len() - 1));
                    }
                }
                self.selected_track = new_indices.last().map(|(track, _)| *track).unwrap_or(0);
                self.selected_clip = new_indices.last().map(|(_, clip)| *clip);
                self.selected_clips = new_indices;
                self.mark_dirty();
                self.status = if rejected > 0 {
                    format!("已粘贴部分片段（{rejected} 个因轨道类型冲突被跳过）")
                } else {
                    "已粘贴 MIDI 片段".into()
                };
            }
            Clipboard::Notes(notes) => {
                let playhead = self.playhead();
                let mut pasted: Vec<(usize, usize)> = Vec::new();
                if notes.is_empty() {
                    return;
                }
                let Some(clip_index) = self.selected_clip else {
                    return;
                };
                if self
                    .current_track()
                    .and_then(|track| track.clips.get(clip_index))
                    .is_none()
                {
                    return;
                }
                self.push_undo_now();
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
                if !pasted.is_empty() {
                    self.selected_note = pasted.last().copied();
                    self.selected_notes = pasted;
                    self.bump_selected_clip_revision();
                    self.status = "已粘贴音符".into();
                }
            }
            Clipboard::ControllerPoints(points) => {
                if !self.bottom_open || points.is_empty() {
                    return;
                }
                if let BottomView::Controller { track_id, kind } = self.bottom_view {
                    let has_lane = track_id == crate::core::model::MASTER_TRACK_ID
                        || self.project.tracks.iter().any(|track| track.id == track_id);
                    if !has_lane {
                        return;
                    }
                    self.push_undo_now();
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
        // 折叠组合的成员不可见：主选落到折叠的组合轨道上，多选移除隐藏项。
        // Folded group members are invisible: the primary selection moves to the folded group and
        // hidden members drop out of the multi-selection.
        if let Some(group_index) = self.project.collapsed_ancestor(self.selected_track) {
            self.selected_track = group_index;
        }
        // 轨道多选：移除越界项；主选不在其中时退回单选，避免结构变化后残留高亮。
        // Track multi-selection: drop out-of-range entries; fall back to a single selection when the
        // primary track is missing so structural edits never leave stale highlights.
        self.selected_tracks.retain(|index| {
            *index < self.project.tracks.len() && !self.project.track_is_hidden(*index)
        });
        if self.selected_tracks.is_empty() || !self.selected_tracks.contains(&self.selected_track) {
            self.selected_tracks = vec![self.selected_track];
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
            !self.project.track_is_hidden(*track)
                && self
                    .project
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
