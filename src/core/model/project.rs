//! 所属模块：core::model / 工程容器
//! 文件用途：
//! - 定义 `Project` 容器、默认值与内置 Demo 编曲。
//! - 提供轨道/片段的增删、组合轨道重排与子树查询等纯规则。
//!
//! 函数清单：
//! - `Project::beats_per_bar` / `end_beat` / `content_end_beat` / `auto_stop_beat` / `any_solo`：工程级派生规则。
//! - `Project::next_track_id` / `next_clip_id` / `insert_track` / `add_track` / `add_audio_track`：轨道与片段 id/插入。
//! - `Project::track_index_of` / `track_depth` / `subtree_end` / `subtree_content_end`：组合层级与子树查询。
//! - `Project::collapsed_ancestor` / `track_is_hidden` / `subtree_track_count`：折叠组合的可见性与成员数量。
//! - `Project::create_group` / `remove_track_at` / `remove_subtree`：创建/删除组合轨道并维护成员连续与父级。
//! - `Project::master_volume_at`：主音量自动化。
//! - `Project::demo` / `empty`：Demo 与空工程构造。
//!
//! Module: core::model / project container
//! Purpose:
//! - Define the `Project` container, defaults and the built-in demo arrangement.
//! - Provide pure rules for track/clip insertion, group reordering and subtree queries.
//!
//! Functions:
//! - `Project::beats_per_bar` / `end_beat` / `content_end_beat` / `auto_stop_beat` / `any_solo`.
//! - `Project::next_track_id` / `next_clip_id` / `insert_track` / `add_track` / `add_audio_track`.
//! - `Project::track_index_of` / `track_depth` / `subtree_end` / `subtree_content_end`.
//! - `Project::create_group` / `remove_track_at`: create/remove groups and keep members contiguous.
//! - `Project::master_volume_at`, `Project::demo`, `Project::empty`.

use super::channel::{ChannelLayout, default_layout};
use super::clip::{Clip, Note};
use super::controller::{ControllerKind, ControllerLanes, evaluate_automation};
use super::effect::EffectRef;
use super::instrument::{DrumSettings, InstrumentRef, SynthSettings, Waveform};
use super::track::{Track, TrackKind};
use super::{CURRENT_PROJECT_VERSION, TrackId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    #[serde(default = "default_project_name")]
    pub name: String,
    pub bpm: f64,
    #[serde(default = "default_time_sig_num")]
    pub time_sig_num: u8,
    #[serde(default = "default_time_sig_den")]
    pub time_sig_den: u8,
    #[serde(default)]
    pub tracks: Vec<Track>,
    #[serde(default = "default_master_volume")]
    pub master_volume: f32,
    /// Automation for the master bus. The main master lane is a volume
    /// controller, so its points are edited in the controller roll.
    #[serde(default)]
    pub master_controllers: ControllerLanes,
    #[serde(default)]
    pub master_effect: Option<EffectRef>,
    #[serde(default)]
    pub loop_enabled: bool,
    #[serde(default)]
    pub loop_start: f64,
    #[serde(default = "default_loop_end")]
    pub loop_end: f64,
    /// Minimum arrangement length in beats (can grow automatically).
    #[serde(default = "default_length")]
    pub length_beats: f64,
    /// 工程输出声道布局：决定主总线、导出与设备映射的声道数。放在结构结尾，旧工程按默认立体声解码。
    /// Project output layout: decides the master bus, export and device-mapping channel count. Kept
    /// at the end so older projects decode as stereo.
    #[serde(default = "default_layout")]
    pub output_layout: ChannelLayout,
}

fn default_project_name() -> String {
    "未命名工程".into()
}
fn default_time_sig_num() -> u8 {
    4
}
fn default_time_sig_den() -> u8 {
    4
}
fn default_master_volume() -> f32 {
    0.85
}
fn default_loop_end() -> f64 {
    16.0
}
fn default_length() -> f64 {
    32.0
}

impl Default for Project {
    fn default() -> Self {
        Self {
            version: CURRENT_PROJECT_VERSION,
            name: "未命名工程".into(),
            bpm: 120.0,
            time_sig_num: 4,
            time_sig_den: 4,
            tracks: Vec::new(),
            master_volume: 0.85,
            master_controllers: ControllerLanes::default(),
            master_effect: None,
            output_layout: ChannelLayout::Stereo,
            loop_enabled: false,
            loop_start: 0.0,
            loop_end: 16.0,
            length_beats: 32.0,
        }
    }
}

impl Project {
    pub fn beats_per_bar(&self) -> f64 {
        self.time_sig_num as f64 * 4.0 / self.time_sig_den as f64
    }

    pub fn next_track_id(&self) -> TrackId {
        self.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1
    }

    pub fn next_clip_id(&self) -> u64 {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(|c| c.id))
            .max()
            .unwrap_or(0)
            + 1
    }

    /// 所属模块：core / 工程模型
    /// 功能：按 id 查找轨道下标。
    /// 输入：`id` 轨道 id。
    /// 输出/副作用：下标；找不到返回 `None`，不修改工程。
    /// 复杂度：时间 O(轨道数)，空间 O(1)。
    ///
    /// Module: core / project model
    /// Purpose: Find a track index by id.
    /// Inputs: `id` — track id.
    /// Outputs / side effects: the index, or `None`; no project change.
    /// Complexity: O(tracks) time and O(1) space.
    pub fn track_index_of(&self, id: TrackId) -> Option<usize> {
        self.tracks.iter().position(|track| track.id == id)
    }

    /// 所属模块：core / 工程模型
    /// 功能：返回轨道所在的组合层级（0 表示直接挂在主输出下）。
    /// 输入：`index` 轨道下标。
    /// 输出/副作用：层级深度；越界或父级缺失时停止向上，不修改工程。
    /// 复杂度：时间 O(层级 × 轨道数)，空间 O(1)。
    ///
    /// Module: core / project model
    /// Purpose: Return the group nesting depth of a track (0 = directly on the master bus).
    /// Inputs: `index` — track index.
    /// Outputs / side effects: the depth; out-of-range or missing parents stop the walk.
    /// Complexity: O(depth × tracks) time and O(1) space.
    pub fn track_depth(&self, index: usize) -> usize {
        let mut depth = 0usize;
        let mut current = index;
        while depth < self.tracks.len() {
            let Some(parent_id) = self.tracks.get(current).and_then(|track| track.group) else {
                break;
            };
            let Some(parent_index) = self.track_index_of(parent_id) else {
                break;
            };
            // 成员必须排在组合轨道之后；父级下标不小于当前下标说明数据异常，避免死循环。
            // Members must follow their group; a parent at or after the child is invalid data.
            if parent_index >= current {
                break;
            }
            depth += 1;
            current = parent_index;
        }
        depth
    }

    /// 所属模块：core / 工程模型
    /// 功能：返回轨道子树的结束下标（不含），子树包含其下所有组合成员。
    /// 输入：`index` 轨道下标。
    /// 输出/副作用：结束下标；越界时返回轨道总数，不修改工程。
    /// 复杂度：时间 O(轨道数²)（最坏嵌套），空间 O(1)。
    ///
    /// Module: core / project model
    /// Purpose: Return the exclusive end index of a track subtree, including nested group members.
    /// Inputs: `index` — track index.
    /// Outputs / side effects: the end index, or the track count when out of range.
    /// Complexity: O(tracks²) worst case with deep nesting and O(1) space.
    pub fn subtree_end(&self, index: usize) -> usize {
        if index >= self.tracks.len() {
            return self.tracks.len();
        }
        let mut end = index + 1;
        while end < self.tracks.len() {
            let Some(parent_id) = self.tracks[end].group else {
                break;
            };
            // 子树在向量中连续排列，父级一定已包含在当前区间内。
            // Subtrees are contiguous, so the parent is always inside the current range.
            if self.tracks[index..end]
                .iter()
                .any(|track| track.id == parent_id)
            {
                end += 1;
            } else {
                break;
            }
        }
        end
    }

    /// 所属模块：core / 工程模型
    /// 功能：返回最近一个折叠了成员的上层组合轨道下标。
    /// 输入：`index` 轨道下标。
    /// 输出/副作用：折叠的祖先组合下标；没有则返回 `None`，不修改工程。
    /// 复杂度：时间 O(层级 × 轨道数)，空间 O(1)。
    ///
    /// Module: core / project model
    /// Purpose: Return the nearest ancestor group that folded its members.
    /// Inputs: `index` — track index.
    /// Outputs / side effects: the folded ancestor index, or `None`; no project change.
    /// Complexity: O(depth × tracks) time and O(1) space.
    pub fn collapsed_ancestor(&self, index: usize) -> Option<usize> {
        let mut current = self.tracks.get(index).and_then(|track| track.group);
        let mut guard = 0usize;
        while let Some(group_id) = current {
            let group_index = self.track_index_of(group_id)?;
            if self.tracks[group_index].collapsed {
                return Some(group_index);
            }
            current = self.tracks[group_index].group;
            guard += 1;
            if guard > self.tracks.len() {
                break;
            }
        }
        None
    }

    /// 所属模块：core / 工程模型
    /// 功能：判断轨道是否因为某个上层组合折叠而在编曲区隐藏。
    /// 输入：`index` 轨道下标。
    /// 输出/副作用：布尔值；不修改工程。
    /// 复杂度：时间 O(层级 × 轨道数)，空间 O(1)。
    ///
    /// Module: core / project model
    /// Purpose: Whether a track is hidden in the arrangement because an ancestor group is folded.
    /// Inputs: `index` — track index.
    /// Outputs / side effects: the boolean; no project change.
    /// Complexity: O(depth × tracks) time and O(1) space.
    pub fn track_is_hidden(&self, index: usize) -> bool {
        self.collapsed_ancestor(index).is_some()
    }

    /// 所属模块：core / 工程模型
    /// 功能：返回组合轨道下的成员轨道数量（含嵌套成员，不含自身）。
    /// 输入：`index` 轨道下标。
    /// 输出/副作用：成员数量；越界返回 0，不修改工程。
    /// 复杂度：时间 O(轨道数²)，空间 O(1)。
    ///
    /// Module: core / project model
    /// Purpose: Count member tracks under a group (including nested members, excluding itself).
    /// Inputs: `index` — track index.
    /// Outputs / side effects: the member count; 0 when out of range.
    /// Complexity: O(tracks²) time and O(1) space.
    pub fn subtree_track_count(&self, index: usize) -> usize {
        self.subtree_end(index).saturating_sub(index + 1)
    }

    /// 所属模块：core / 工程模型
    /// 功能：返回轨道子树内最后一个片段的结束 beat。
    /// 输入：`index` 轨道下标。
    /// 输出/副作用：结束 beat（无内容为 0）；不修改工程。
    /// 复杂度：时间 O(子树轨道数 × 片段数)，空间 O(1)。
    ///
    /// Module: core / project model
    /// Purpose: Return the end beat of the last clip inside a track subtree.
    /// Inputs: `index` — track index.
    /// Outputs / side effects: the end beat (0 when empty); no project change.
    /// Complexity: O(subtree tracks × clips) time and O(1) space.
    pub fn subtree_content_end(&self, index: usize) -> f64 {
        let end = self.subtree_end(index);
        self.tracks[index.min(self.tracks.len())..end]
            .iter()
            .flat_map(|track| track.clips.iter())
            .map(|clip| clip.end())
            .fold(0.0_f64, f64::max)
    }

    /// 所属模块：core / 工程模型
    /// 功能：把选中的轨道移动到新建的组合轨道下，形成（可嵌套的）组合轨道。
    /// 输入：
    /// - `members`：选中的轨道下标；包含彼此祖先关系时只保留上层轨道。
    /// - `name`：组合轨道名称；`None` 时按 id 自动命名。
    /// 输出/副作用：重排 `tracks` 并在第一个成员的位置插入组合轨道；成员按原顺序紧跟其后；
    /// 返回组合轨道下标。成员不足两个、层级不同或下标无效时返回中文错误且不修改工程。
    /// 复杂度：时间 O(轨道数 + 选中数 × 轨道数)，空间 O(轨道数)。
    ///
    /// Module: core / project model
    /// Purpose: Move the selected tracks under a new group track, allowing nested groups.
    /// Inputs:
    /// - `members`: selected track indices; descendants of another selected track are dropped.
    /// - `name`: group name; `None` derives one from the id.
    /// Outputs / side effects: reorders `tracks`, inserts the group at the first member position and
    /// keeps members in their original order right after it; returns the group index. Returns a
    /// Chinese error without touching the project when fewer than two members remain, the parents
    /// differ or an index is invalid.
    /// Complexity: O(tracks + members × tracks) time and O(tracks) space.
    pub fn create_group(&mut self, members: &[usize], name: Option<&str>) -> Result<usize, String> {
        let mut roots: Vec<usize> = members
            .iter()
            .copied()
            .filter(|index| *index < self.tracks.len())
            .collect();
        roots.sort_unstable();
        roots.dedup();
        // 去掉已被另一个选中轨道包含的后代，避免把同一条轨道重复移入。
        // Drop descendants of another selected track so no track is moved twice.
        let selected = roots.clone();
        roots.retain(|index| {
            !selected
                .iter()
                .any(|other| other != index && *other < *index && *index < self.subtree_end(*other))
        });
        if roots.len() < 2 {
            return Err("请选择两条及以上轨道再创建组合轨道".into());
        }
        let parent = self.tracks[roots[0]].group;
        if roots
            .iter()
            .any(|index| self.tracks[*index].group != parent)
        {
            return Err("请选择同一层级的轨道再创建组合轨道".into());
        }
        let ranges: Vec<(usize, usize)> = roots
            .iter()
            .map(|index| (*index, self.subtree_end(*index)))
            .collect();
        let group_id = self.next_track_id();
        let palette: [[u8; 3]; 6] = [
            [45, 212, 191],
            [251, 191, 36],
            [129, 140, 248],
            [244, 114, 182],
            [52, 211, 153],
            [251, 146, 60],
        ];
        let color = palette[self.tracks.len() % palette.len()];
        let group_name = name
            .map(|value| value.to_string())
            .unwrap_or_else(|| format!("组合轨道 {group_id}"));
        let group_track = Track::new_group(group_id, &group_name, color, parent);

        let mut slots: Vec<Option<Track>> = self.tracks.drain(..).map(Some).collect();
        let mut reordered: Vec<Track> = Vec::with_capacity(slots.len() + 1);
        let first_start = ranges[0].0;
        // 组合轨道占用第一个成员的位置，其余成员块紧随其后；原本夹在成员之间的轨道后移，
        // 保证成员子树在向量中连续（引擎与子树命中都依赖这一不变量）。
        // The group takes the first member's slot and every member block follows it; tracks that sat
        // between members move after the block so member subtrees stay contiguous (the engine and
        // subtree hit testing rely on that invariant).
        for slot in slots.iter_mut().take(first_start) {
            if let Some(track) = slot.take() {
                reordered.push(track);
            }
        }
        reordered.push(group_track.clone());
        for (start, end) in &ranges {
            for slot in slots.iter_mut().take(*end).skip(*start) {
                if let Some(mut track) = slot.take() {
                    // 只有块的根轨道挂到新组合下，块内后代保留原有父级。
                    // Only the block root joins the new group; descendants keep their parents.
                    if track.group == parent {
                        track.group = Some(group_id);
                    }
                    reordered.push(track);
                }
            }
        }
        for slot in slots.iter_mut().skip(first_start) {
            if let Some(track) = slot.take() {
                reordered.push(track);
            }
        }
        self.tracks = reordered;
        Ok(self.track_index_of(group_id).unwrap_or(first_start))
    }

    /// 所属模块：core / 工程模型
    /// 功能：删除指定轨道；被删除的组合轨道会把直接成员提升到它的父级。
    /// 输入：`index` 轨道下标。
    /// 输出/副作用：移除轨道并修正成员父级；返回被删除的轨道，越界返回 `None`。
    /// 复杂度：时间 O(轨道数)，空间 O(1)。
    ///
    /// Module: core / project model
    /// Purpose: Remove a track; when it is a group its direct members move up to its parent.
    /// Inputs: `index` — track index.
    /// Outputs / side effects: removes the track, fixes member parents and returns the removed track;
    /// `None` when out of range.
    /// Complexity: O(tracks) time and O(1) space.
    pub fn remove_track_at(&mut self, index: usize) -> Option<Track> {
        if index >= self.tracks.len() {
            return None;
        }
        let removed = self.tracks.remove(index);
        if removed.is_group() {
            for track in &mut self.tracks {
                if track.group == Some(removed.id) {
                    track.group = removed.group;
                }
            }
        }
        Some(removed)
    }

    /// 所属模块：core / 工程模型
    /// 功能：删除轨道及其整棵子树（组合轨道会连同全部成员一起删除）。
    /// 输入：`index` 轨道下标。
    /// 输出/副作用：从 `tracks` 移除子树并返回被删除的轨道；越界返回空列表，不修改工程。
    /// 复杂度：时间 O(轨道数)，空间 O(被删除轨道数)。
    ///
    /// Module: core / project model
    /// Purpose: Remove a track and its whole subtree (a group takes every member with it).
    /// Inputs: `index` — track index.
    /// Outputs / side effects: removes the subtree and returns the removed tracks; an out-of-range
    /// index returns an empty list.
    /// Complexity: O(tracks) time and O(removed tracks) space.
    pub fn remove_subtree(&mut self, index: usize) -> Vec<Track> {
        if index >= self.tracks.len() {
            return Vec::new();
        }
        let end = self.subtree_end(index);
        self.tracks.drain(index..end).collect()
    }

    /// 所属模块：core / 工程模型
    /// 功能：在指定下标插入一条新轨道，返回其最终索引；越界下标按末尾处理。
    /// 输入：
    /// - `index`：插入位置，`0` 表示最前；大于轨道数时追加到末尾。
    /// - `instrument`：新轨道使用的乐器。
    /// 输出/副作用：修改 `tracks` 并返回新轨道索引；不修改其它字段。
    /// 复杂度：时间 O(轨道数)，空间 O(1)。
    ///
    /// Module: core / project model
    /// Purpose: Insert a new track at `index` and return its final index; out-of-range appends.
    /// Inputs:
    /// - `index`: insertion position, `0` means first; beyond the end appends.
    /// - `instrument`: instrument of the new track.
    /// Outputs / side effects: mutates `tracks` and returns the new index; other fields untouched.
    /// Complexity: O(tracks) time and O(1) space.
    pub fn insert_track(&mut self, index: usize, instrument: InstrumentRef) -> usize {
        let id = self.next_track_id();
        let palette: [[u8; 3]; 6] = [
            [45, 212, 191],
            [251, 191, 36],
            [129, 140, 248],
            [244, 114, 182],
            [52, 211, 153],
            [251, 146, 60],
        ];
        let color = palette[(self.tracks.len()) % palette.len()];
        let name = match &instrument {
            InstrumentRef::Drums(_) => format!("鼓组 {}", id),
            InstrumentRef::Native(native) => format!("{} {}", native.kind.label(), id),
            InstrumentRef::Sample(sample) => format!("{} {}", sample.name, id),
            InstrumentRef::Plugin(_) => format!("插件轨道 {}", id),
            InstrumentRef::Synth(_) => format!("MIDI 轨道 {}", id),
        };
        let index = index.min(self.tracks.len());
        self.tracks
            .insert(index, Track::new(id, &name, color, instrument));
        index
    }

    /// 所属模块：core / 工程模型
    /// 功能：在末尾追加一条新轨道，返回其索引。
    /// 输入：`instrument` 新轨道乐器。
    /// 输出/副作用：修改 `tracks` 并返回新轨道索引。
    /// 复杂度：时间 O(轨道数)，空间 O(1)。
    ///
    /// Module: core / project model
    /// Purpose: Append a new track at the end and return its index.
    /// Inputs: `instrument` — instrument of the new track.
    /// Outputs / side effects: mutates `tracks` and returns the new index.
    /// Complexity: O(tracks) time and O(1) space.
    pub fn add_track(&mut self, instrument: InstrumentRef) -> usize {
        let index = self.tracks.len();
        self.insert_track(index, instrument)
    }

    /// Create a main lane intended for audio clips. It still hosts the same
    /// instrument/controller model, so MIDI clips can be added later.
    pub fn add_audio_track(&mut self) -> usize {
        let index = self.add_track(InstrumentRef::Synth(SynthSettings::default()));
        let id = self.tracks[index].id;
        let track = &mut self.tracks[index];
        track.kind = TrackKind::Audio;
        track.name = format!("音频轨道 {id}");
        index
    }

    /// Master gain at a given beat: volume automation if present, otherwise
    /// the static master volume.
    pub fn master_volume_at(&self, beat: f64) -> f32 {
        evaluate_automation(self.master_controllers.get(ControllerKind::Volume), beat)
            .map(|value| value * 1.5)
            .unwrap_or(self.master_volume)
            .clamp(0.0, 1.5)
    }

    pub fn end_beat(&self) -> f64 {
        let content = self
            .tracks
            .iter()
            .flat_map(|t| t.clips.iter())
            .map(|c| c.end())
            .fold(0.0_f64, f64::max);
        content.max(self.length_beats).max(4.0)
    }

    pub fn content_end_beat(&self) -> f64 {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter())
            .map(|c| c.end())
            .fold(0.0_f64, f64::max)
    }

    /// Playback stops automatically two bars after the final MIDI clip.
    pub fn auto_stop_beat(&self) -> f64 {
        let end = self.content_end_beat();
        if end <= 0.0 {
            0.0
        } else {
            end + self.beats_per_bar() * 2.0
        }
    }

    pub fn any_solo(&self) -> bool {
        self.tracks.iter().any(|t| t.solo)
    }

    /// A short demo arrangement so the first launch makes sound immediately.
    pub fn demo() -> Self {
        let mut project = Project {
            name: "Surfing Demo".into(),
            bpm: 110.0,
            loop_enabled: false,
            loop_start: 0.0,
            loop_end: 16.0,
            length_beats: 32.0,
            ..Default::default()
        };

        // Lead
        let mut lead = Track::new(
            1,
            "Surf Lead",
            [45, 212, 191],
            InstrumentRef::Synth(SynthSettings {
                waveform: Waveform::Saw,
                cutoff: 6200.0,
                resonance: 0.22,
                level: 0.55,
                ..Default::default()
            }),
        );
        let melody: [(f64, f64, u8); 12] = [
            (0.0, 0.5, 69),
            (0.5, 0.5, 72),
            (1.0, 1.0, 76),
            (2.0, 0.5, 74),
            (2.5, 0.5, 72),
            (3.0, 1.0, 69),
            (4.0, 0.5, 67),
            (4.5, 0.5, 69),
            (5.0, 1.0, 72),
            (6.0, 0.5, 74),
            (6.5, 0.5, 76),
            (7.0, 1.0, 79),
        ];
        let mut notes = Vec::new();
        for bar in 0..4 {
            for (start, len, pitch) in melody {
                notes.push(Note {
                    start: start + bar as f64 * 8.0,
                    length: len,
                    pitch,
                    velocity: 98,
                });
            }
        }
        let mut lead_clip = Clip::new(1, "Lead Pattern", 0.0, 32.0);
        lead_clip.notes = notes;
        lead.clips.push(lead_clip);
        lead.volume = 0.7;
        lead.expanded = true;
        project.tracks.push(lead);

        // Chords
        let mut chords = Track::new(
            2,
            "Warm Chords",
            [251, 191, 36],
            InstrumentRef::Synth(SynthSettings {
                waveform: Waveform::Triangle,
                attack: 0.03,
                release: 0.45,
                cutoff: 3400.0,
                level: 0.36,
                ..Default::default()
            }),
        );
        let progression: [[u8; 3]; 4] = [[57, 60, 64], [53, 57, 60], [48, 52, 55], [55, 59, 62]];
        let mut chord_notes = Vec::new();
        for bar in 0..4 {
            let chord = progression[bar];
            for pitch in chord {
                chord_notes.push(Note {
                    start: bar as f64 * 8.0,
                    length: 3.75,
                    pitch,
                    velocity: 72,
                });
            }
        }
        let mut chord_clip = Clip::new(2, "Am - F - C - G", 0.0, 32.0);
        chord_clip.notes = chord_notes;
        chords.clips.push(chord_clip);
        chords.volume = 0.62;
        project.tracks.push(chords);

        // Bass
        let mut bass = Track::new(
            3,
            "Sub Bass",
            [129, 140, 248],
            InstrumentRef::Synth(SynthSettings {
                waveform: Waveform::Square,
                attack: 0.004,
                decay: 0.12,
                sustain: 0.5,
                release: 0.12,
                cutoff: 900.0,
                level: 0.5,
                octave: -1,
                ..Default::default()
            }),
        );
        let roots = [45u8, 41, 36, 43];
        let mut bass_notes = Vec::new();
        for bar in 0..4 {
            for step in 0..8 {
                bass_notes.push(Note {
                    start: bar as f64 * 8.0 + step as f64 * 0.5,
                    length: 0.42,
                    pitch: roots[bar],
                    velocity: if step % 2 == 0 { 108 } else { 82 },
                });
            }
        }
        let mut bass_clip = Clip::new(3, "Bass Pulse", 0.0, 32.0);
        bass_clip.notes = bass_notes;
        bass.clips.push(bass_clip);
        bass.volume = 0.68;
        project.tracks.push(bass);

        // Drums
        let mut drums = Track::new(
            4,
            "Chip Drums",
            [244, 114, 182],
            InstrumentRef::Drums(DrumSettings::default()),
        );
        let mut drum_notes = Vec::new();
        for bar in 0..4 {
            let base = bar as f64 * 8.0;
            for beat in 0..8 {
                // Kick on every beat, extra on the "and" of 3.
                drum_notes.push(Note {
                    start: base + beat as f64,
                    length: 0.12,
                    pitch: 36,
                    velocity: if beat % 2 == 0 { 120 } else { 95 },
                });
                // Hats on eighths.
                drum_notes.push(Note {
                    start: base + beat as f64 + 0.5,
                    length: 0.08,
                    pitch: 42,
                    velocity: if beat % 2 == 0 { 75 } else { 58 },
                });
            }
            // Snare on beats 2 and 4.
            for beat in [1.0, 3.0, 5.0, 7.0] {
                drum_notes.push(Note {
                    start: base + beat,
                    length: 0.12,
                    pitch: 38,
                    velocity: 105,
                });
            }
        }
        let mut drum_clip = Clip::new(4, "Four On The Floor", 0.0, 32.0);
        drum_clip.notes = drum_notes;
        drums.clips.push(drum_clip);
        drums.volume = 0.8;
        project.tracks.push(drums);

        project
    }

    pub fn empty() -> Self {
        let mut project = Project {
            name: "新工程".into(),
            bpm: 120.0,
            ..Default::default()
        };
        project.add_track(InstrumentRef::Synth(SynthSettings::default()));
        project
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_round_trips_through_json() {
        let project = Project::demo();
        let json = serde_json::to_string(&project).expect("serialize");
        let loaded: Project = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(loaded.tracks.len(), project.tracks.len());
        assert_eq!(loaded.bpm, project.bpm);
        assert_eq!(
            loaded.tracks[0].clips[0].notes.len(),
            project.tracks[0].clips[0].notes.len()
        );
    }

    /// 覆盖：在指定轨道下方插入的新轨道位置正确、id 唯一，越界时追加到末尾。
    /// Covers: inserting below a track keeps the position, allocates a unique id and appends when
    /// the index is out of range.
    #[test]
    fn insert_track_keeps_position_and_unique_ids() {
        let mut project = Project::empty();
        project.add_track(InstrumentRef::Synth(SynthSettings::default()));
        let existing: Vec<TrackId> = project.tracks.iter().map(|track| track.id).collect();
        let index = project.insert_track(1, InstrumentRef::Synth(SynthSettings::default()));
        assert_eq!(index, 1);
        assert_eq!(project.tracks.len(), 3);
        assert_eq!(project.tracks[0].id, existing[0]);
        assert_eq!(project.tracks[2].id, existing[1]);
        assert!(!existing.contains(&project.tracks[1].id));
        assert_eq!(
            project.insert_track(99, InstrumentRef::Synth(SynthSettings::default()),),
            3
        );
        assert_eq!(project.tracks.len(), 4);
    }

    /// 覆盖：选中的两条轨道被移入新组合轨道，成员紧随组合轨道且父级正确。
    /// Covers: two selected tracks move under a new group which is placed before them.
    #[test]
    fn create_group_nests_members_after_the_group() {
        let mut project = Project::empty();
        project.add_track(InstrumentRef::Synth(SynthSettings::default()));
        project.add_track(InstrumentRef::Synth(SynthSettings::default()));
        let group_index = project.create_group(&[0, 2], None).expect("group");
        assert_eq!(group_index, 0);
        assert!(project.tracks[0].is_group());
        assert_eq!(project.tracks.len(), 4);
        let group_id = project.tracks[0].id;
        assert_eq!(project.tracks[1].group, Some(group_id));
        assert_eq!(project.tracks[2].group, Some(group_id));
        assert_eq!(project.track_depth(1), 1);
        assert_eq!(project.subtree_end(0), 3);
    }

    /// 覆盖：组合轨道可以再次与另一条轨道组成外层组合，层级与子树范围正确。
    /// Covers: a group can be nested into an outer group with the expected depth and subtree range.
    #[test]
    fn groups_can_be_nested() {
        let mut project = Project::empty();
        project.add_track(InstrumentRef::Synth(SynthSettings::default()));
        project.add_track(InstrumentRef::Synth(SynthSettings::default()));
        project.create_group(&[0, 1], None).expect("inner");
        project.add_track(InstrumentRef::Synth(SynthSettings::default()));
        let outer = project.create_group(&[0, 3], None).expect("outer");
        assert_eq!(outer, 0);
        assert_eq!(project.tracks.len(), 6);
        assert_eq!(project.track_depth(1), 1);
        assert_eq!(project.track_depth(2), 2);
        assert_eq!(project.track_depth(3), 2);
        assert_eq!(project.track_depth(4), 1);
        assert_eq!(project.track_depth(5), 0);
        assert_eq!(project.subtree_end(0), 5);
    }

    /// 覆盖：组合关系与渲染来源字段可往返序列化。
    /// Covers: group parents and render-source links survive a serialization round trip.
    #[test]
    fn group_links_round_trip_through_json() {
        let mut project = Project::empty();
        project.add_track(InstrumentRef::Synth(SynthSettings::default()));
        project.create_group(&[0, 1], None).expect("group");
        let group_id = project.tracks[0].id;
        project.tracks[1].rendered_from = Some(project.tracks[1].id);
        project.tracks[0].collapsed = true;
        let json = serde_json::to_string(&project).expect("serialize");
        let loaded: Project = serde_json::from_str(&json).expect("deserialize");
        assert!(loaded.tracks[0].is_group());
        assert!(loaded.tracks[0].collapsed);
        assert_eq!(loaded.tracks[1].group, Some(group_id));
        assert_eq!(loaded.tracks[1].rendered_from, Some(loaded.tracks[1].id));
    }

    /// 覆盖：折叠的组合隐藏成员、成员计数正确，展开后恢复可见。
    /// Covers: a folded group hides its members, reports the member count and shows them again when
    /// unfolded.
    #[test]
    fn collapsing_a_group_hides_members() {
        let mut project = Project::empty();
        project.add_track(InstrumentRef::Synth(SynthSettings::default()));
        project.create_group(&[0, 1], None).expect("group");
        assert_eq!(project.subtree_track_count(0), 2);
        assert!(!project.track_is_hidden(1));
        project.tracks[0].collapsed = true;
        assert_eq!(project.collapsed_ancestor(1), Some(0));
        assert!(project.track_is_hidden(1));
        assert!(project.track_is_hidden(2));
        project.tracks[0].collapsed = false;
        assert!(!project.track_is_hidden(1));
    }

    /// 覆盖：整棵删除组合子树会连同全部成员一起移除。
    /// Covers: removing a group subtree takes every member with it.
    #[test]
    fn removing_a_group_subtree_removes_members() {
        let mut project = Project::empty();
        project.add_track(InstrumentRef::Synth(SynthSettings::default()));
        project.add_track(InstrumentRef::Synth(SynthSettings::default()));
        let group_index = project.create_group(&[0, 1, 2], None).expect("group");
        let removed = project.remove_subtree(group_index);
        assert_eq!(removed.len(), 4);
        assert!(removed[0].is_group());
        assert!(project.tracks.is_empty());
    }

    /// 覆盖：删除组合轨道时直接成员回到它的父级，不会留下失效父级。
    /// Covers: removing a group reparents its direct members to the removed group's parent.
    #[test]
    fn removing_a_group_reparents_members() {
        let mut project = Project::empty();
        project.add_track(InstrumentRef::Synth(SynthSettings::default()));
        project.add_track(InstrumentRef::Synth(SynthSettings::default()));
        let group_index = project.create_group(&[0, 1], None).expect("group");
        let removed = project.remove_track_at(group_index).expect("removed");
        assert!(removed.is_group());
        assert!(project.tracks.iter().all(|track| track.group.is_none()));
        assert_eq!(project.tracks.len(), 3);
    }
}
