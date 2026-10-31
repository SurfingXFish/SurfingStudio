# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-31 19:56:19 (Asia/Shanghai) |
| AI 工具/模型 | TraeCode / 未标注模型 |
| 任务来源 | 1.1.5a 实测关闭工程窗口时闪退（0xC0000005） |
| 关联分支/提交 | `feature/audio-import-surround-gpu-export` / 待提交 |
| 影响模块 | audio, app, docs |
| 关联记录 | `20261031-195108-plugin-editor-process-cleanup.md` |

## 2. 日志分析（1.1.5a / pid 12412）

- 关闭工程：
  - `plugin_windows_visible titles=VOCALOID6 Editor`
  - `plugin_windows_closed windows=1 all_closed=true`（39921 ms）
  - 约 2 秒后 `app.crash/unhandled_exception exception_code=0xC0000005`（41916 ms）
  - 日志中没有 `plugins_terminated`，说明崩溃发生在 `Engine::shutdown_plugins` 内、某个插件的 `IComponent::terminate`。
- 同一流程在 1.1.4a 曾成功，属于 VOCALOID / ACE Bridge terminate 的非确定性崩溃。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/mod.rs` | 修改 | `terminate_engine_plugins` 新增 `terminate_components` 参数；工程窗口 `Drop` 传 `false`：只关闭插件窗口并结束外部编辑进程，不调用 `IComponent::terminate`，避免关闭闪退；`plugins_terminated` 日志增加 `terminated=false` |
| `src/app/files.rs` | 修改 | 同一窗口切换工程仍传 `true`，调用 `shutdown_plugins` 以便重建实例 |
| `src/audio/engine/mod.rs` | 修改 | `shutdown_plugins` 为每个乐器/效果器写 `engine.plugin.shutdown` 开始/结束日志；若 terminate 崩溃，最后一条 begin 即故障插件 |
| `doc/技术文档.md` | 修改 | 12.3 工程窗口关闭不 terminate；9.2 切换工程仍 terminate |
| `doc/框架工作流.md` | 修改 | 1.10 关闭流程与 `SurfingApp::drop` |
| `doc/项目简介.md` | 修改 | 关闭工程窗口说明 |
| `doc/代码规范.md` | 修改 | 工程窗口关闭不得 terminate |
| `doc/日志规范.md` | 修改 | `plugins_terminated terminated=false`、`engine.plugin.shutdown` |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 工程窗口关闭 | 调用 `shutdown_plugins`（terminate 24 个插件） | 不 terminate；关闭窗口 + 结束外部编辑进程，实例/模块保留 | 消除关闭时 terminate 触发的 0xC0000005 |
| 同一窗口切换工程 | terminate | 保持 terminate（需要重建实例） | 行为不变，并新增逐插件日志 |
| 诊断 | 无法定位 terminate 崩溃插件 | `engine.plugin.shutdown` begin/end | 下次崩溃可直接定位 |

## 5. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 9.2、12.3 |
| `doc/框架工作流.md` | 是 | 1.10、`SurfingApp::drop` |
| `doc/项目简介.md` | 是 | 2.6 |
| `doc/代码规范.md` | 是 | 8.2 |
| `doc/日志规范.md` | 是 | 5.2、5.5 |

## 6. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行 |
| `cargo check --all-targets --offline` | 通过 | 无警告 |
| `cargo test --lib --offline` | 通过 | 73 通过，0 失败，16 忽略 |
| 实测 | 待用户确认 | 关闭工程应不再闪退；日志应出现 `plugins_terminated terminated=false` 与 `plugin_editor_processes_terminated` |

## 7. 风险与后续

- 工程窗口关闭后旧插件实例仍驻留（引擎外壳泄漏），且外部编辑进程已结束；重新打开工程时若仍提示单实例冲突，说明检测依据是进程内组件而非外部应用，需要改为“复用旧实例”方案。
- 同一窗口切换工程仍会 terminate，若该路径崩溃，`engine.plugin.shutdown` 的 begin 日志可定位具体插件。
