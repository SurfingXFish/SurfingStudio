# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-23 22:29:29 (Asia/Shanghai) |
| AI 工具/模型 | Trae / Claude |
| 任务来源 | 增加自动保存、关闭时未保存确认，以及崩溃时保存工程的独立线程 |
| 关联分支/提交 | 未知 / 未知 |
| 影响模块 | app, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 增加自动保存：工程已有路径且存在未保存修改时按间隔写回原文件。
- 关闭窗口时若存在未保存修改，询问是否保存（保存并关闭 / 不保存 / 取消）。
- 增加独立线程用于崩溃时保存工程文件；工程从未保存时在系统临时目录创建保存文件。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/autosave.rs` | 新增 | 自动保存定时器、崩溃恢复线程（`RecoveryWorker` + `run_recovery_worker`）、恢复文件路径规则、未保存关闭确认对话框 `ui_close_prompt`、`has_unsaved_changes`、恢复文件清理与工程切换重置 |
| `src/app/mod.rs` | 修改 | 声明 `mod autosave`；`SurfingApp` 新增 `close_prompt` / `pending_close` / `last_autosave` / `recovery` / `recovery_path` / `last_recovery_marker` / `last_recovery_push` / `recovery_notice_shown`；`SaveOutcome` 增加 `autosave`；`ui_frame` 增加关闭拦截、`poll_autosave`、`pending_close` 与确认框绘制；`Drop` 停用恢复线程 |
| `src/app/files.rs` | 修改 | 保存流程抽出 `save_project_in_background(force_dialog, autosave)`；`poll_save_project` 处理自动保存状态文本、删除恢复文件并在关闭确认中继续关闭；`collect_peak_entries` 提升为 `pub(super)`；新建/加载后重置自动保存与确认状态 |
| `src/app/shell.rs` | 修改 | 根工程有未保存修改时取消系统关闭并交由工程窗口确认；工程视口在未保存时不移除窗口 |
| `doc/技术文档.md` | 修改 | 模块职责、线程模型、9.1 保存/自动保存/关闭确认/崩溃恢复、性能清单 |
| `doc/关键常量.md` | 修改 | 新增 `AUTOSAVE_INTERVAL`、`RECOVERY_SAVE_INTERVAL`、`RECOVERY_SNAPSHOT_INTERVAL` 与恢复文件位置 |
| `doc/框架工作流.md` | 修改 | 模块地图、`ui_frame` 与关闭流程、1.5 保存/自动保存/崩溃恢复流程、`app/autosave.rs` 函数索引、5.4 并发保护 |
| `doc/项目简介.md` | 修改 | 用户可见的自动保存、崩溃恢复与关闭前确认说明，目录树补充 `autosave.rs` |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 每帧编排 | 轮询加载/保存/引擎同步 | 额外调用 `poll_autosave`（恢复结果、快照推送、自动保存），并在保存完成或放弃修改后发送 `ViewportCommand::Close` | 自动保存与崩溃备份不阻塞绘制 |
| 保存流程 | `save_project` / `save_project_as` 直接后台写盘 | 统一走 `save_project_in_background(force_dialog, autosave)`；自动保存复用同一流程且不写最近项目 | 保存结果统一在 `poll_save_project` 收尾 |
| 关闭窗口 | 直接关闭并销毁插件 GUI | 存在未保存修改时 `CancelClose` 并弹出模态确认；保存成功或放弃修改后下一帧关闭 | 避免误关导致修改丢失 |
| 工程生命周期 | 无备份 | 每个工程窗口启动崩溃恢复线程；工程保存成功、放弃修改或替换工程时清理恢复文件 | 崩溃时最多丢失 `RECOVERY_SAVE_INTERVAL` 内的编辑 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `AUTOSAVE_INTERVAL` | 无 | `60` 秒 | `src/app/autosave.rs` | 自动保存间隔 |
| `RECOVERY_SAVE_INTERVAL` | 无 | `20` 秒 | `src/app/autosave.rs` | 崩溃恢复线程写盘间隔 |
| `RECOVERY_SNAPSHOT_INTERVAL` | 无 | `5` 秒 | `src/app/autosave.rs` | 恢复快照最小推送间隔 |
| 恢复文件位置 | 无 | 已保存工程同目录 `<名称>.autosave.sfsd`；未保存工程临时目录 `SurfingStudio/<工程名>-<窗口编号>.sfsd` | `src/app/autosave.rs` | 保存成功或放弃修改后删除 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 模块职责、2.1 线程表、9.1 保存（自动保存/关闭确认/崩溃恢复）、10 性能清单 |
| `doc/关键常量.md` | 是 | 第 5 节 App 节流、异步与交互阈值 |
| `doc/框架工作流.md` | 是 | 1.1 关闭与每帧编排、1.5 保存/自动保存/崩溃恢复、2 模块地图、3 文件与函数索引、5.4 并发保护 |
| `doc/项目简介.md` | 是 | 2.6 工作流与目录树 |
| `doc/代码规范.md` | 否 | 规范本身未变化；新增代码遵循既有分层与注释规范 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 无格式差异 |
| `cargo test` | 通过 | 36 通过、12 忽略；新增 3 个恢复文件路径/文件名测试 |
| 手工验证 | 未执行 | 需要真实窗口与音频设备，未在本次会话中启动应用 |

## 8. 风险与后续

- 风险：恢复快照不额外抓取插件运行时状态，插件状态以最近一次保存时写回的值为准。
- 风险：自动保存与崩溃恢复都在后台写盘；恢复文件使用独立文件名（`.autosave.sfsd`），不会与手动保存的 `.tmp` 临时文件冲突。
- 后续：如需让用户配置自动保存间隔或关闭自动保存，可在全局设置中增加对应字段。
