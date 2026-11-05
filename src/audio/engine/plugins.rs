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

    /// 所属模块：audio::engine / 插件编辑器
    /// 功能：给**所有**带编辑器的插件转发一次处理器输出参数反馈。
    /// 输入：`self`。
    /// 输出/副作用：可能调用 `setParamNormalized`；插件忙时跳过本轮。
    /// 复杂度：时间 O(轨道数 + 效果器数)，空间 O(1)。
    ///
    /// Module: audio::engine / plugin editors
    /// Purpose: Forward processor output-parameter feedback for **every** plugin that has an editor.
    /// Inputs: `self`.
    /// Outputs / side effects: may call `setParamNormalized`; skips plugins currently busy.
    /// Complexity: O(tracks + effects) time and O(1) space.
    ///
    /// 插件**自己创建**的编辑器窗口（VOCALOID6 Editor 等）不在宿主的编辑器槽位表里，
    /// 只按槽位转发会让这类窗口收不到反馈而停在上一帧。
    /// Windows created by plugins themselves (VOCALOID6 Editor and friends) are not in the host's
    /// editor-slot table; forwarding by slot alone leaves such windows without feedback, stuck on
    /// their previous frame.
    pub fn service_all_editor_feedback(&mut self) {
        for track in &mut self.tracks {
            if let InstrumentRuntime::Plugin(plugin) = &mut track.instrument {
                plugin.pump_editor_feedback();
            }
            for effect in &mut track.effects {
                if let EffectRuntime::Plugin(plugin) = effect {
                    plugin.pump_editor_feedback();
                }
            }
        }
        if let Some(EffectRuntime::Plugin(plugin)) = &mut self.master_effect {
            plugin.pump_editor_feedback();
        }
    }

    /// 所属模块：audio::engine / 插件生命周期
    /// 功能：重试所有插件被拒绝的状态恢复（VOCALOID 这类插件要等自己的编辑器就绪才接受）。
    /// 输入：`self`。
    /// 输出/副作用：可能再次调用插件 `setState`；成功后写 `plugin.state`。
    /// 复杂度：时间 O(轨道数 + 效果器数)，空间 O(1)。
    ///
    /// Module: audio::engine / plugin lifecycle
    /// Purpose: Retry every state restore a plugin declined (plugins such as VOCALOID accept it only
    /// once their own editor is ready).
    /// Inputs: `self`.
    /// Outputs / side effects: may call the plugin's `setState` again; logs `plugin.state` on success.
    /// Complexity: O(tracks + effects) time and O(1) space.
    pub fn retry_all_state_restores(&mut self) {
        // 每秒最多重试一次：状态块可能有几十 KB，不能每帧都塞给插件。
        // At most one retry per second: the state blob can be tens of KB and must not be pushed
        // every frame.
        static LAST: std::sync::OnceLock<std::sync::Mutex<Option<Instant>>> =
            std::sync::OnceLock::new();
        {
            let last = LAST.get_or_init(|| std::sync::Mutex::new(None));
            let mut guard = match last.lock() {
                Ok(guard) => guard,
                Err(poison) => poison.into_inner(),
            };
            let now = Instant::now();
            if let Some(previous) = *guard {
                if now.duration_since(previous) < Duration::from_millis(1000) {
                    return;
                }
            }
            *guard = Some(now);
        }
        for track in &mut self.tracks {
            if let InstrumentRuntime::Plugin(plugin) = &mut track.instrument {
                plugin.retry_state_restore();
            }
            for effect in &mut track.effects {
                if let EffectRuntime::Plugin(plugin) = effect {
                    plugin.retry_state_restore();
                }
            }
        }
        if let Some(EffectRuntime::Plugin(plugin)) = &mut self.master_effect {
            plugin.retry_state_restore();
        }
    }

    /// 所属模块：audio::engine / 播放探查
    /// 功能：每 5 秒写一条播放探查：插件乐器的内部走带位置与各轨电平。
    /// 输入：`self`。
    /// 输出/副作用：在播放中写 `engine.plugin.transport` 与 `engine.track.levels`；不改引擎状态。
    /// 复杂度：时间 O(轨道数)，空间 O(轨道数)。
    ///
    /// Module: audio::engine / playback probe
    /// Purpose: Every 5 s log each plugin instrument's internal transport position and every track's
    /// level.
    /// Inputs: `self`.
    /// Outputs / side effects: writes `engine.plugin.transport` and `engine.track.levels` while
    /// playing; no engine state changes.
    /// Complexity: O(tracks) time and space.
    ///
    /// 这两条记录用来回答两个问题：插件自己的走带有没有跟着宿主走、某一轨到底有没有出声。
    /// These two records answer two questions: does the plugin's own transport follow the host, and
    /// does a given track produce any level at all.
    pub fn log_playback_probe(&self) {
        if !self.playing {
            return;
        }
        static LAST: std::sync::OnceLock<std::sync::Mutex<Option<Instant>>> =
            std::sync::OnceLock::new();
        let last = LAST.get_or_init(|| std::sync::Mutex::new(None));
        {
            let mut guard = match last.lock() {
                Ok(guard) => guard,
                Err(poison) => poison.into_inner(),
            };
            let now = Instant::now();
            if let Some(previous) = *guard {
                if now.duration_since(previous) < Duration::from_secs(5) {
                    return;
                }
            }
            *guard = Some(now);
        }
        let host_beats = self.position_beats;
        for track in &self.tracks {
            let InstrumentRuntime::Plugin(plugin) = &track.instrument else {
                continue;
            };
            let name = plugin.display_name().to_string();
            let (samples, music) = plugin
                .transport_snapshot()
                .map(|(samples, music)| (samples.to_string(), format!("{music:.3}")))
                .unwrap_or_else(|| ("?".into(), "?".into()));
            let host = format!("{host_beats:.3}");
            crate::diag::info(
                "app.engine",
                "engine.plugin.transport",
                &[
                    ("name", &name),
                    ("plugin_samples", &samples),
                    ("plugin_music", &music),
                    ("host_beats", &host),
                ],
            );
        }
        let levels: Vec<String> = self
            .tracks
            .iter()
            .enumerate()
            .map(|(index, runtime)| {
                let name = self
                    .project
                    .tracks
                    .get(index)
                    .map(|track| track.name.as_str())
                    .unwrap_or("?");
                format!("{name}={:.4}", runtime.level)
            })
            .collect();
        let levels_text = levels.join(" ");
        crate::diag::info(
            "app.engine",
            "engine.track.levels",
            &[("levels", &levels_text)],
        );
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
        // 顺手处理插件用 restartComponent 提出的延迟/总线变化（Pro-Q 4 切线性相位等）：
        // 不处理会一直按旧延迟补偿。
        // Also service latency/bus changes plugins raise through restartComponent (Pro-Q 4 when
        // switching to linear phase, for example); otherwise the old latency keeps being used.
        self.refresh_plugin_latency();
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
                    let (state, params) = capture_plugin_snapshot(plugin);
                    pref.state = state;
                    pref.params = params;
                }
            }
            for (index, effect_ref) in track.effects.iter_mut().enumerate() {
                let Some(pref) = effect_ref.plugin_mut() else {
                    continue;
                };
                if let Some(effect) = runtime.effects.get(index).and_then(EffectRuntime::plugin) {
                    let (state, params) = capture_plugin_snapshot(effect);
                    pref.state = state;
                    pref.params = params;
                }
            }
        }
        if let (Some(effect_ref), Some(runtime)) = (&mut project.master_effect, &self.master_effect)
        {
            if let (Some(pref), Some(effect)) = (effect_ref.plugin_mut(), runtime.plugin()) {
                let (state, params) = capture_plugin_snapshot(effect);
                pref.state = state;
                pref.params = params;
            }
        }
    }

    /// 所属模块：audio::engine / 插件生命周期
    /// 功能：把单实例插件（VOCALOID / ACE Bridge 等）登记到进程级复用缓存。
    /// 输入：`self`。
    /// 输出/副作用：暂停引擎，缓存实例 Arc；其它插件忽略。
    /// 复杂度：时间 O(轨道数 + 效果器数)，空间 O(1)。
    ///
    /// Module: audio::engine / plugin lifecycle
    /// Purpose: Register single-instance plugins (VOCALOID / ACE Bridge, ...) in the process-wide
    /// reusable cache.
    /// Inputs: `self`.
    /// Outputs / side effects: pauses the engine and caches instance Arcs; other plugins are ignored.
    /// Complexity: O(tracks + effects) time and O(1) space.
    pub fn stash_single_instance_plugins(&mut self) {
        self.pause();
        for track in &mut self.tracks {
            if let InstrumentRuntime::Plugin(plugin) = &track.instrument {
                crate::plugin::stash_reusable(plugin);
            }
            for effect in &track.effects {
                if let EffectRuntime::Plugin(plugin) = effect {
                    crate::plugin::stash_reusable(plugin);
                }
            }
        }
        if let Some(EffectRuntime::Plugin(plugin)) = &self.master_effect {
            crate::plugin::stash_reusable(plugin);
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
                let (state, params) = capture_plugin_snapshot(plugin);
                pref.state = state;
                pref.params = params;
            }
        }
        for (index, effect_ref) in track.effects.iter_mut().enumerate() {
            let Some(pref) = effect_ref.plugin_mut() else {
                continue;
            };
            if let Some(effect) = runtime.effects.get(index).and_then(EffectRuntime::plugin) {
                let (state, params) = capture_plugin_snapshot(effect);
                pref.state = state;
                pref.params = params;
            }
        }
    }
}

/// 所属模块：audio::engine / 插件状态
/// 功能：抓取插件状态并打包 ARA 归档，供 `.sfsd` 保存。
/// 输入：`plugin` 插件运行时。
/// 输出/副作用：先尝试在控制器线程刷新 ARA 归档（失败时沿用缓存），再返回（状态块、参数）。
/// 复杂度：时间 O(状态 + 归档大小)，空间 O(状态 + 归档大小)。
///
/// Module: audio::engine / plugin state
/// Purpose: Capture the plug-in state and pack the ARA archive for `.sfsd`.
/// Inputs: `plugin`.
/// Outputs / side effects: first tries to refresh the ARA archive on the controller thread (falls
/// back to the cache on failure), then returns (state blob, parameters).
/// Complexity: O(state + archive size) time and space.
fn capture_plugin_snapshot(plugin: &PluginRuntime) -> (std::sync::Arc<[u8]>, Vec<f32>) {
    let (state, params) = plugin.capture_state();
    let _ = plugin.refresh_ara_archive();
    let ara = plugin.ara_archive_snapshot().unwrap_or_default();
    (
        crate::plugin::pack_plugin_state(&state, &ara).into(),
        params,
    )
}
