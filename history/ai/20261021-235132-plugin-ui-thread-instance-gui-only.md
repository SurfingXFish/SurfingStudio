# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-21 23:51:32 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 播放中新增 VST 并双击打开 GUI 卡死；所有效果器显示没有可用的 GUI |
| 关联分支/提交 | 未知 |
| 影响模块 | app / ui / audio/engine / docs |
| 关联记录 | `history/ai/20261021-231044-effect-roll-gui-only-fit-scale.md`、`history/ai/20261021-224958-inline-popup-async-plugin-apply.md` |

## 2. 任务目标

- 修复播放中新增 VST 效果器后打开 GUI 卡死。
- 修复所有 VST 效果器卡片显示“没有界面”。
- 用真实应用流程验证（不再只依赖库测试）。

## 3. 根因

1. 上一轮把 `apply_plugin` 改成后台同步线程创建实例。VST3 的编辑器视图必须与实例在同一线程创建；Auto-Tune Pro 在 UI 线程 `createView` 时会死锁，整个 UI 线程卡在 `ui_effects`。已在真机复现：最后打印停在 `after toolbars`，未进入 `after bottom panel`。
2. 后台同步服务 `run_sync_service` 每次同步都会 `pause()`，导致“播放中新增插件”直接停播。
3. 卡片能力来自每帧 `engine.try_lock()`；锁忙时整表回退为 `false`，于是全部卡片显示“没有可内嵌的界面”。

## 4. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/plugins.rs` | 修改 | `apply_plugin` 改回 UI 线程 `sync_engine()` 创建实例，再 `finish_plugin_apply`；已有后台同步在跑时经 `pending_plugin_apply` 延后收尾。新增 `ensure_ui_thread_instance`：打开 GUI 前把后台线程创建的实例在 UI 线程重建（状态写回 → 失效运行时 → UI 线程同步）。`open_editor` / `open_inline_editor` 不再阻塞等待引擎锁，同步未完成时提示“正在加载插件”；`inline_failed` 改为记录失败原因 |
| `src/app/transport.rs` | 修改 | 新增 `background_sync_in_flight` 与 `mark_background_plugins`；后台同步完成后标记由后台线程创建的槽位；新增 `engine_sync_pending` |
| `src/app/mod.rs` | 修改 | 新增 `background_synced` / `effect_editor_flags`；`inline_failed` 改为 `HashMap<_, String>` |
| `src/app/history.rs` | 修改 | 清理新增缓存；删除 `effects_panel_min_height`（GUI 自行缩放，面板不再按插件撑高） |
| `src/audio/engine/plugins.rs` | 修改 | 新增 `invalidate_track_plugins(track_id)`：丢弃轨道/主输出运行时并清空键，下一次同步重建 |
| `src/ui/effects.rs` | 修改 | 引擎锁忙时沿用 `effect_editor_flags` 缓存；同步中显示“正在加载插件…”；内嵌失败显示具体原因 |
| `src/app/edit.rs` | 修改 | 删除/粘贴效果器链时清理 `inline_failed` 与能力缓存 |
| `src/app/input.rs` | 修改 | Esc 只关闭独立窗口，没有独立窗口时关闭底部卷帘 |
| `src/plugin/mod.rs` | 修改 | 新增 Auto-Tune Pro 打开耗时/缩放诊断测试（默认 ignore） |
| `doc/技术文档.md` | 修改 | 6.6 线程约束；6.7 内嵌窗口与缩放 |
| `doc/框架工作流.md` | 修改 | 1.7 选择插件/打开工程流程；索引补充 `ensure_ui_thread_instance`、`mark_background_plugins`、`engine_sync_pending` |
| `doc/项目简介.md` | 修改 | 2.4 线程约束与重建说明 |
| `doc/代码规范.md` | 修改 | 8.2 插件实例与编辑器视图必须同线程 |

## 5. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 选择插件 | 后台同步线程创建实例 | UI 线程同步创建实例 | 打开 GUI 不再死锁；播放不会被后台同步的 `pause()` 打断 |
| 打开工程 | 后台线程创建实例后直接用它打开 GUI | 首次打开 GUI 前在 UI 线程重建该轨道/主输出实例（保留状态） | Auto-Tune 等插件可正常打开；重建只影响对应轨道 |
| 卡片能力 | 引擎锁忙时全部显示“没有界面” | 沿用 `effect_editor_flags` 缓存；同步中显示“正在加载插件…” | 不再误报没有 GUI |
| 内嵌失败 | 只写状态栏 | 卡片显示具体失败原因 | 便于定位 |

## 6. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无 | - | - | - | 本次未新增全局常量 |

## 7. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.6、6.7 |
| `doc/关键常量.md` | 否 | 无新增常量 |
| `doc/框架工作流.md` | 是 | 1.7、3.x / 4 索引 |
| `doc/项目简介.md` | 是 | 2.4 |
| `doc/代码规范.md` | 是 | 8.2 |

## 8. 验证（真实应用流程）

使用临时 `diag` 模块在真实 eframe 应用中模拟，验证后已删除：

| 场景 | 结果 |
| --- | --- |
| 播放中新增 Pro-Q 4，卷帘自动内嵌 | 通过：40 帧内 `inline=true`，双击打开独立窗口 171 ms，无卡死 |
| 播放中新增 Auto-Tune Pro，卷帘自动内嵌 | 通过：40 帧内 `inline=true`，桌面截图可见 Auto-Tune GUI 完整显示在卡片内；双击独立窗口 119 ms |
| 模拟“打开已有工程”（实例由后台线程创建）+ Auto-Tune | 通过：首次打开 GUI 触发 UI 线程重建，`inline=true`，无卡死 |
| 修复前同场景 | 卡死：日志停在 bottom panel 之前 |

| 项目 | 结果 |
| --- | --- |
| `cargo fmt --all` | 通过 |
| `cargo check --all-targets` | 通过 |
| `cargo test` | 24 passed / 0 failed / 11 ignored |
| `pro_q_editor_embeds_in_a_host_child_window` | 通过 |
| `auto_tune_editor_open_timing` | 通过（fit 430×240 → 403×240，< 1 ms） |

## 9. 风险与后续

- 风险：打开工程后首次打开某个效果器 GUI 会重建该轨道的全部插件实例（含状态恢复），有一次性加载延迟；只影响该轨道。
- 风险：`ensure_ui_thread_instance` 需要短暂持有引擎锁并重建实例，播放中可能有极短音频中断。
- 风险：后台同步服务仍会 `pause()`；只要用户操作触发的同步走同步路径（UI 线程）就不会停播。
- 后续：继续用真实工程验证多效果器、主输出效果器与多次切换工程。
