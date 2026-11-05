# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-05 16:41:36 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | “依旧卡死”，附日志 `surfing-20261105-163350-p24952.sflog` |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | app, audio, plugin, docs |
| 关联记录 | `history/ai/20261105-163302-ara-bridge-editor-freeze.md` |

## 2. 任务目标

- 修复播放中约 5 秒后界面永久卡死（`frame=2357` 后再不前进，`ui.stall` 一路增长）。

## 3. 日志证据

- Bridge 绑定正常：`ara.session_begin` → `ara.playback_region` → `ara.selection` → `ara.session_created` → `ara.bound`，全部在 12.5 s 完成。
- 26.6 s 开始播放，`ara.bridge_processed samples=0 quarter_notes=0.000` 说明 Bridge 确实被音频线程处理。
- UI 帧在 30.3 s 仍有 2132，34.5 s 报 `ui.stall stall_ms=3005 phase=ui.paint frames=2357`，之后不再前进；CPU 与线程数平稳（阻塞而不是忙等）。
- 卡死发生在 `phase=ui.paint`，即绘制阶段，而不是播放/编辑器消息泵阶段。

## 4. 根因

效果器实例由后台同步线程创建，界面线程默认不创建插件效果器（`defer_plugin_effects`）。打开效果器卡片（或播放前 `prepare_playback_plugins`）时：

1. `invalidate_plugin_slot` 清空该槽位的身份键，期望下次同步在 UI 线程重建；
2. UI 线程的 `ensure_effects` 因“缺键”直接返回，**实例既不销毁也不重建**；
3. `ensure_ui_thread_instance` 却返回成功，随后在 UI 线程对“后台线程创建的实例”调用 `open_editor` / 创建内嵌视图 → 跨线程创建 VST3 视图，永久死锁。

所以本次卡死与 VOCALOID Bridge 无关，而是绘制效果器卡片时触发了这个既有缺陷；播放开始后的 `prepare_playback_plugins` 也会走同一条路径。

## 5. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/audio/engine/mod.rs` | 修改 | 新增 `pending_effect_rebuild`；`sync_project_budgeted` 只在界面线程同步时消费它，并把对应下标传给 `ensure_effects`；`ensure_effects` 在 defer 分支只放行这些槽位；`EffectRuntime::created_on_current_thread` |
| `src/audio/engine/plugins.rs` | 修改 | `invalidate_plugin_slot` 为效果器槽位记入 `pending_effect_rebuild`（同步桥除外）；新增 `plugin_slot_needs_rebuild` / `plugin_slot_on_current_thread` |
| `src/app/plugins.rs` | 修改 | `ensure_ui_thread_instance` 在同步后要求“不再等待重建”且“实例创建于当前线程”，否则保留标记并返回 `false` |
| `src/plugin/runtime.rs` | 修改 | `Vst3Runtime` 新增 `created_thread`（创建线程）；新增 `PluginRuntime::created_on_current_thread` |
| `src/plugin/backends/vst3.rs` | 修改 | 加载时记录创建线程 |
| `src/app/mod.rs` | 修改 | 绘制阶段增加 `ui.paint.bar` / `status` / `bottom` / `inspector` / `arrangement` / `rolls` / `dialogs` 活动标记，下次卡死可直接定位到具体面板 |
| `doc/技术文档.md` | 修改 | 6.7 增加“效果器 GUI 的同线程重建” |
| `doc/框架工作流.md` | 修改 | `ensure_effects`、`invalidate_plugin_slot`、`ensure_ui_thread_instance` 等行 |

## 6. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 效果器槽位失效 | 只清空身份键，界面线程永远补不回来 | 记入 `pending_effect_rebuild`，界面线程同步为该槽位放行创建 | 打开 GUI 前实例真正重建在同一线程 |
| 创建 VST3 视图前 | 只检查同步是否在跑 | 额外检查“仍待重建”与“实例创建线程” | 任何跨线程创建视图的情况都会被拒绝，避免死锁 |
| 卡死定位 | 只能看到 `ui.paint` | 细分为 bar/status/bottom/inspector/arrangement/rolls/dialogs | 可直接判断卡在哪个面板 |

## 7. 关键常量变化

无。

## 8. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.7 |
| `doc/框架工作流.md` | 是 | 插件运行时 / 编辑器表 |
| `doc/关键常量.md` | 否 | 无常量变化 |
| `doc/项目简介.md` | 否 | 无用户可见功能变化 |
| `doc/代码规范.md` | 否 | 无变化 |

## 9. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test --lib` | 通过 | 116 passed / 23 ignored |
| 实机验证 | 待执行 | 播放、打开效果器面板应不再卡死；若仍卡死，`ui.stall` 的 `phase` 会指出具体面板 |

## 10. 风险与后续

- 风险：首次播放/首次打开效果器面板时，可能在 UI 线程同步加载多个效果器实例，出现数秒加载停顿（不再是死锁）；加载提示会显示。
- 风险：若某个插件实例确实无法在 UI 线程创建，其 GUI 将不会打开，但仍会记录 `plugin.editor/rebuild_pending`，不会卡死。
- 后续：若仍有卡死，按新的 `ui.paint.*` 阶段定位到具体面板，再检查该面板内的插件/窗口调用。
