//! Plugin parameter, editor and state APIs exposed by the engine.

use super::*;

impl Engine {
    // ----- plugin parameter / editor API -------------------------------

    pub(super) fn runtime_for(&self, track_id: TrackId, slot: SlotKind) -> Option<&PluginRuntime> {
        match slot {
            SlotKind::Master => self.master_effect.as_ref().and_then(EffectRuntime::plugin),
            SlotKind::Instrument => self
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .and_then(TrackEngine::plugin),
            SlotKind::Effect(index) => self
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .and_then(|t| t.effects.get(index))
                .and_then(EffectRuntime::plugin),
        }
    }

    pub(super) fn runtime_for_mut(
        &mut self,
        track_id: TrackId,
        slot: SlotKind,
    ) -> Option<&mut PluginRuntime> {
        match slot {
            SlotKind::Master => self
                .master_effect
                .as_mut()
                .and_then(EffectRuntime::plugin_mut),
            SlotKind::Instrument => self
                .tracks
                .iter_mut()
                .find(|t| t.id == track_id)
                .and_then(TrackEngine::plugin_mut),
            SlotKind::Effect(index) => self
                .tracks
                .iter_mut()
                .find(|t| t.id == track_id)
                .and_then(|t| t.effects.get_mut(index))
                .and_then(EffectRuntime::plugin_mut),
        }
    }

    pub fn plugin_params(&self, track_id: TrackId, slot: SlotKind) -> Vec<ParamView> {
        self.runtime_for(track_id, slot)
            .map(|runtime| runtime.params().to_vec())
            .unwrap_or_default()
    }

    pub fn set_plugin_param(
        &mut self,
        track_id: TrackId,
        slot: SlotKind,
        index: usize,
        value: f64,
    ) -> Result<(), String> {
        self.runtime_for_mut(track_id, slot)
            .ok_or_else(|| "没有找到插件".to_string())?
            .set_param(index, value)
    }

    pub fn refresh_plugin_params(&mut self, track_id: TrackId, slot: SlotKind) {
        if let Some(runtime) = self.runtime_for_mut(track_id, slot) {
            runtime.refresh_params();
        }
    }

    /// 所属模块：audio::engine / 插件编辑器
    /// 功能：把处理器输出的参数反馈转发给插件控制器，驱动分析器/电平显示。
    /// 输入：
    /// - `track_id` / `slot`：插件所在槽位（主输出使用 `SlotKind::Master`）。
    /// 输出/副作用：可能调用 `setParamNormalized`；插件忙时跳过本轮。
    /// 复杂度：时间 O(轨道数 + 效果器数 + 反馈点数)，空间 O(1)。
    ///
    /// Module: audio::engine / plugin editors
    /// Purpose: Forward processor output-parameter feedback to the plugin controller so
    /// analysers and level displays keep refreshing.
    /// Inputs:
    /// - `track_id` / `slot`: plugin slot (`SlotKind::Master` for the master bus).
    /// Outputs / side effects: may call `setParamNormalized`; skips while the plugin is busy.
    /// Complexity: O(tracks + effects + feedback points) time and O(1) space.
    pub fn service_editor_feedback(&mut self, track_id: TrackId, slot: SlotKind) {
        if let Some(runtime) = self.runtime_for_mut(track_id, slot) {
            runtime.pump_editor_feedback();
        }
    }

    pub fn has_editor(&self, track_id: TrackId, slot: SlotKind) -> bool {
        self.runtime_for(track_id, slot)
            .map(|runtime| runtime.has_editor())
            .unwrap_or(false)
    }

    pub fn vst3_plugin(
        &self,
        track_id: TrackId,
        slot: SlotKind,
    ) -> Option<Arc<Mutex<vst3_host::Plugin>>> {
        self.runtime_for(track_id, slot)
            .and_then(|runtime| runtime.vst3_plugin())
    }

    /// 所属模块：audio::engine / 插件编辑器
    /// 功能：取出 VST1/VST2 编辑器对象，供应用层创建独立窗口。
    /// 输入：
    /// - `track_id` / `slot`：插件所在槽位（主输出使用 `SlotKind::Master`）。
    /// 输出/副作用：返回编辑器对象；非 VST2 或已取出时返回 `None`。
    /// 复杂度：时间 O(轨道数 + 效果器数)，空间 O(1)。
    ///
    /// Module: audio::engine / plugin editors
    /// Purpose: Take the VST1/VST2 editor object so the app can open a standalone window.
    /// Inputs:
    /// - `track_id` / `slot`: plugin slot (`SlotKind::Master` for the master bus).
    /// Outputs / side effects: returns the editor; `None` for non-VST2 or when already taken.
    /// Complexity: O(tracks + effects) time and O(1) space.
    pub fn take_plugin_editor(
        &mut self,
        track_id: TrackId,
        slot: SlotKind,
    ) -> Option<Box<dyn vst::editor::Editor>> {
        self.runtime_for_mut(track_id, slot)
            .and_then(|runtime| runtime.take_editor())
    }

    pub fn plugin_display_name(&self, track_id: TrackId, slot: SlotKind) -> Option<String> {
        self.runtime_for(track_id, slot)
            .map(|runtime| runtime.display_name().to_string())
    }

    /// Editor size reported by the plugin, in physical pixels.
    pub fn plugin_editor_size(&self, track_id: TrackId, slot: SlotKind) -> Option<(f32, f32)> {
        self.runtime_for(track_id, slot)
            .and_then(|runtime| runtime.editor_size())
    }

    /// Whether the plugin editor accepts host-driven resizing. Returns `None`
    /// while the plugin instance is busy so the UI keeps its cached value.
    pub fn plugin_editor_can_resize(&self, track_id: TrackId, slot: SlotKind) -> Option<bool> {
        self.runtime_for(track_id, slot)
            .and_then(|runtime| runtime.editor_can_resize())
    }

    pub fn is_plugin_failed(&self, track_id: TrackId, slot: SlotKind) -> Option<String> {
        if slot == SlotKind::Instrument {
            if let Some(track) = self.tracks.iter().find(|t| t.id == track_id) {
                if let InstrumentRuntime::Failed(error) = &track.instrument {
                    return Some(error.clone());
                }
            }
        }
        let runtime = self.runtime_for(track_id, slot)?;
        match runtime {
            PluginRuntime::Failed(error) => Some(error.clone()),
            _ => None,
        }
    }

    /// Poll plugin editors for parameter gestures made behind our back.
    pub fn poll_parameter_changes(&mut self) -> Vec<TrackId> {
        // Do not even take the engine lock every UI frame: a 5 Hz poll is
        // enough and keeps the audio callback's try_lock uncontended.
        if self.last_parameter_poll.elapsed() < std::time::Duration::from_millis(200) {
            return Vec::new();
        }
        self.last_parameter_poll = Instant::now();
        let playing = self.playing;
        let count = self.tracks.len();
        if count == 0 {
            return Vec::new();
        }
        // 每次只轮询一条轨道。一次扫描 20 多个插件的全部参数会在 UI 线程上
        // 卡住数百毫秒（加载工程后的首次扫描尤其明显）。
        // Poll a single track per tick: scanning every parameter of 20+ plugins
        // on the UI thread stalls it for hundreds of milliseconds, especially on
        // the first scan after a project finishes loading.
        let index = self.parameter_poll_cursor % count;
        self.parameter_poll_cursor = (index + 1) % count;
        if self.tracks[index].poll_parameters(playing) {
            vec![self.tracks[index].id]
        } else {
            Vec::new()
        }
    }

    /// Store plugin state/parameter snapshots back into a project before saving.
    pub fn capture_plugin_states(&self, project: &mut Project) {
        for track in &mut project.tracks {
            let Some(runtime) = self.tracks.iter().find(|t| t.id == track.id) else {
                continue;
            };
            if let InstrumentRef::Plugin(pref) = &mut track.instrument {
                if let Some(plugin) = runtime.plugin() {
                    let (state, params) = plugin.capture_state();
                    pref.state = state.into();
                    pref.params = params;
                }
            }
            for (index, effect_ref) in track.effects.iter_mut().enumerate() {
                let Some(pref) = effect_ref.plugin_mut() else {
                    continue;
                };
                if let Some(effect) = runtime.effects.get(index).and_then(EffectRuntime::plugin) {
                    let (state, params) = effect.capture_state();
                    pref.state = state.into();
                    pref.params = params;
                }
            }
        }
        if let (Some(effect_ref), Some(runtime)) = (&mut project.master_effect, &self.master_effect)
        {
            if let (Some(pref), Some(effect)) = (effect_ref.plugin_mut(), runtime.plugin()) {
                let (state, params) = effect.capture_state();
                pref.state = state.into();
                pref.params = params;
            }
        }
    }

    /// 所属模块：audio::engine / 插件编辑器
    /// 功能：丢弃一条轨道（或主输出）的插件运行时，强制下一次同步重建。
    /// 输入：`track_id` 轨道 id；`MASTER_TRACK_ID` 表示主效果器链。
    /// 输出/副作用：清空运行时与键；下一次 `sync_project` 会重新加载插件。
    /// 复杂度：时间 O(效果器数量)，空间 O(1)。
    ///
    /// Module: audio::engine / plugin editors
    /// Purpose: Drop the plugin runtimes of one track (or the master chain) so the next sync
    /// rebuilds them.
    /// Inputs: `track_id`; `MASTER_TRACK_ID` selects the master effect.
    /// Outputs / side effects: clears runtimes and keys; the next `sync_project` reloads them.
    /// Complexity: O(effect count) time and O(1) space.
    pub fn invalidate_track_plugins(&mut self, track_id: TrackId) {
        if track_id == crate::core::model::MASTER_TRACK_ID {
            self.master_effect = None;
            self.master_effect_key = None;
            return;
        }
        if let Some(track) = self.tracks.iter_mut().find(|track| track.id == track_id) {
            track.effect_keys.clear();
            track.effects.clear();
            // 乐器没有“空态”，旧实例会在下一次同步时按新键替换。
            // Instruments have no empty state; the old instance is replaced by the next sync.
            track.instrument_key.clear();
        }
    }

    /// 所属模块：audio::engine / 插件运行时
    /// 功能：只丢弃指定槽位的插件运行时，下一次同步时按工程重建该实例。
    /// 输入：
    /// - `track_id`：轨道 id（主输出传 `MASTER_TRACK_ID`）。
    /// - `slot`：乐器 / 第几个效果器 / 主效果器。
    /// 输出/副作用：清空该槽位的身份键并移除旧实例；其它槽位保持不动。
    /// 复杂度：时间 O(轨道数 + 效果器数)，空间 O(1)。
    ///
    /// Module: audio::engine / plugin runtimes
    /// Purpose: Drop only the runtime of one slot so the next sync rebuilds that instance.
    /// Inputs:
    /// - `track_id`: track id (`MASTER_TRACK_ID` for the master bus).
    /// - `slot`: instrument / effect index / master effect.
    /// Outputs / side effects: clears that slot's identity key and removes its runtime; other slots
    /// are untouched.
    /// Complexity: O(tracks + effects) time and O(1) space.
    pub fn invalidate_plugin_slot(&mut self, track_id: TrackId, slot: SlotKind) {
        match slot {
            SlotKind::Master => {
                self.master_effect = None;
                self.master_effect_key = None;
            }
            SlotKind::Instrument => {
                if let Some(track) = self.tracks.iter_mut().find(|track| track.id == track_id) {
                    // 乐器没有空态；清空身份键后，下一次同步会重新创建。
                    // Instruments have no empty state; clearing the key makes the next sync reload.
                    track.instrument_key.clear();
                    track.instrument = InstrumentRuntime::Failed("正在重建插件实例…".into());
                }
            }
            SlotKind::Effect(index) => {
                if let Some(track) = self.tracks.iter_mut().find(|track| track.id == track_id) {
                    if let Some(key) = track.effect_keys.get_mut(index) {
                        // 只让该下标无法复用旧运行时，其它效果器仍按身份键复用。
                        // Only this index fails to reuse its old runtime; the others still match.
                        key.clear();
                    }
                }
            }
        }
    }

    /// 把单条轨道的插件状态写回工程（参数编辑结束后调用）。
    /// Capture one track's plugin state back into the project (after an edit settles).
    pub fn capture_track_plugin_states(&self, track_id: TrackId, project: &mut Project) {
        let Some(runtime) = self.tracks.iter().find(|runtime| runtime.id == track_id) else {
            return;
        };
        let Some(track) = project.tracks.iter_mut().find(|track| track.id == track_id) else {
            return;
        };
        if let InstrumentRef::Plugin(pref) = &mut track.instrument {
            if let Some(plugin) = runtime.plugin() {
                let (state, params) = plugin.capture_state();
                pref.state = state.into();
                pref.params = params;
            }
        }
        for (index, effect_ref) in track.effects.iter_mut().enumerate() {
            let Some(pref) = effect_ref.plugin_mut() else {
                continue;
            };
            if let Some(effect) = runtime.effects.get(index).and_then(EffectRuntime::plugin) {
                let (state, params) = effect.capture_state();
                pref.state = state.into();
                pref.params = params;
            }
        }
    }
}
