# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-05 16:33:02 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | “卡死”，附日志 `surfing-20261105-162639-p9748.sflog` |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | app, audio, docs |
| 关联记录 | `history/ai/20261105-162207-ara-bridge-sync-only.md` |

## 2. 任务目标

- 消除打开 VOCALOID Bridge 相关界面后的永久卡死。

## 3. 日志证据

- `ara.playback_region` / `ara.selection` / `ara.session_created` / `ara.bound` 均在 2.8 s 正常完成。
- 之后没有第二次 ARA 会话，也没有 `ara.bridge_processed`、`plugin.transport`，说明没有开始播放。
- UI 帧计数停在 `frame=934`（约 15.2 s），`ui.stall` 连续增长到 67 s，CPU/线程数平稳：UI 线程被阻塞而不是忙等。
- 最后交互是打开插件界面（本轮曾要求用户打开 Bridge 编辑器查看连接状态）。

## 4. 根因

Bridge 的实例由后台同步线程创建，ARA 文档控制器绑定在该线程。打开编辑器时：

1. `mark_background_plugins` 把 Bridge 也登记为“后台实例”；
2. `ensure_ui_thread_instance` 调用 `invalidate_plugin_slot` 清掉 Bridge 的身份键，但 UI 线程的 `ensure_effects` 在 `defer_plugin_effects` 分支直接返回，**并不会真正重建**；
3. 旧实例既没有销毁、也没有重建，函数却返回成功，随后在 UI 线程对后台线程创建的实例调用 `open_editor`（VST3 视图跨线程创建）→ 永久死锁。

同时，清空身份键后 `ensure_effects` 会一直提前返回，该轨道的 ARA 时间线/参数同步也会停止。

## 5. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/transport.rs` | 修改 | `mark_background_plugins` 跳过 `sync_only_passthrough` 的效果器（VOCALOID Bridge），播放前不再把它标记为待重建 |
| `src/audio/engine/plugins.rs` | 修改 | `invalidate_plugin_slot` 对同步桥直接返回，不清空身份键，避免之后的同步一直提前返回 |
| `src/app/plugins.rs` | 修改 | 新增 `slot_is_ara_sync_only`；`ensure_ui_thread_instance`、`open_editor`、`open_inline_editor` 拒绝为同步桥创建视图（内嵌会记入 `inline_failed`，状态栏提示“没有可打开的界面”） |
| `src/plugin/ara/mod.rs` | 修改 | `AraSession::create` 开头新增 `ara.session_begin`，用于区分“绑定挂起”和“绑定完成但后续无声” |
| `doc/技术文档.md` | 修改 | 6.7 增加“Bridge 不参与 UI 线程重建、不打开界面” |
| `doc/框架工作流.md` | 修改 | `prepare_playback_plugins` / `ensure_ui_thread_instance` 说明 |

## 6. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 播放前插件重建 | Bridge 也被登记并在 UI 线程失效 | 跳过同步桥 | 不会出现“失效后永不重建”的空槽位 |
| 打开插件界面 | 可能对后台线程实例创建 VST3 视图 | 同步桥直接拒绝 | 消除跨线程创建视图导致的永久卡死 |
| Bridge 身份键 | 被 `invalidate_plugin_slot` 清空 | 保持不动 | ARA 时间线/参数同步不会因空键提前返回 |

## 7. 关键常量变化

无。

## 8. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.7 |
| `doc/框架工作流.md` | 是 | 插件编辑 / 传输同步表 |
| `doc/关键常量.md` | 否 | 无常量变化 |
| `doc/项目简介.md` | 否 | 无用户可见功能变化 |
| `doc/代码规范.md` | 否 | 无变化 |

## 9. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test --lib` | 通过 | 116 passed / 23 ignored |
| 实机验证 | 待执行 | 打开工程、播放、尝试点击 Bridge 编辑器不应再卡死 |

## 10. 风险与后续

- 风险：Bridge 的界面不再可打开，无法再从它自己的界面读取 “Connected to VOCALOID VSTi/AU” 状态；判断连接改为看编辑器播放头是否跟随。
- 后续：若仍卡死，新日志会出现 `ara.session_begin`（绑定挂起）或停在其它阶段，可据此继续定位。
