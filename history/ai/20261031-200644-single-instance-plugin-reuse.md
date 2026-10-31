# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-31 20:06:44 (Asia/Shanghai) |
| AI 工具/模型 | TraeCode / 未标注模型 |
| 任务来源 | 1.1.6a 实测重新打开工程仍提示 VOCALOID 单实例冲突 |
| 关联分支/提交 | `feature/audio-import-surround-gpu-export` / 待提交 |
| 影响模块 | plugin, audio, app, docs |
| 关联记录 | `20261031-195619-no-terminate-on-window-close.md` |

## 2. 日志分析（1.1.6a / pid 20872）

- 关闭工程：`plugin_windows_closed windows=1 all_closed=true` → `plugins_terminated terminated=false`，**没有** `plugin_editor_processes_terminated`。
- 重新加载：`engine.instrument.load name=VOCALOID VSTi dur=0.608ms status=ok`，随后提示“已经有一个启动了”。
- 结论：VOCALOID6.exe 不是本进程树的子进程（进程树清理没命中）；旧实例被泄漏在进程内、组件未终止，单实例检测命中的是进程内旧组件/模块状态。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/runtime.rs` | 修改 | 新增单实例插件复用缓存 `ReusableVst3` 与 `stash_reusable` / `PluginRuntime::take_reusable`；`Vst3Runtime` 新增 `path_key`；`plugin.reuse` 日志记录 stash/take |
| `src/plugin/backends/vst3.rs` | 修改 | 加载前按 `path_key` 取重复用实例，命中时不调用 `createInstance`；新建实例写入 `path_key` |
| `src/plugin/mod.rs` | 修改 | 导出 `stash_reusable` 供引擎调用 |
| `src/audio/engine/plugins.rs` | 修改 | 新增 `Engine::stash_single_instance_plugins`：关闭工程时把单实例插件登记到复用缓存 |
| `src/app/mod.rs` | 修改 | 工程窗口关闭（`terminate_components=false`）时调用 `stash_single_instance_plugins`；外部编辑器进程清理改为启动时快照，只结束会话期间新出现的进程 |
| `src/app/shell.rs` | 修改 | 应用启动时记录已存在的编辑器进程 |
| `doc/技术文档.md` / `框架工作流.md` / `项目简介.md` / `代码规范.md` / `日志规范.md` / `关键常量.md` | 修改 | 同步复用缓存与进程清理说明 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 关闭工程后的 VOCALOID | 泄漏旧实例，重新加载时重新 createInstance → 单实例冲突 | 关闭时登记到复用缓存，重新加载直接取回同一实例 | 不再提示“已经有一个启动了” |
| 单实例插件缓存 | 无 | 进程级 `Vec<ReusableVst3>`，同一路径可多份（ACE 多轨） | 重新打开工程复用，不重新拉起编辑器 |
| 外部编辑器进程清理 | 只处理本进程树 | 启动时快照 + 只结束会话期间新出现的进程 | 覆盖 COM/启动器代持的进程，同时不误杀用户单独打开的应用 |
| 工程窗口关闭 | 不 terminate（避免 0xC0000005） | 保持不 terminate，增加 stash | 关闭不再闪退 |

## 5. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 9.2 加载复用；12.3 关闭流程 |
| `doc/框架工作流.md` | 是 | 1.10 关闭流程 |
| `doc/项目简介.md` | 是 | 2.6 关闭工程窗口 |
| `doc/代码规范.md` | 是 | 8.2 生命周期 |
| `doc/日志规范.md` | 是 | `plugin.reuse`、`plugin_editor_processes` |
| `doc/关键常量.md` | 是 | 复用缓存与进程清理 |

## 6. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行 |
| `cargo check --all-targets --offline` | 通过 | 无警告 |
| `cargo test --lib --offline` | 通过 | 73 通过，0 失败，16 忽略 |
| 实测 | 待用户确认 | 关闭后日志应出现 `plugin.reuse phase=stash`；重新打开应出现 `phase=take`，且不再提示 VOCALOID 已在运行 |

## 7. 风险与后续

- 复用实例的采样率/缓冲沿用上一次加载；若新工程改变了设备采样率，需要重新初始化（当前设备采样率固定为 192 kHz）。
- 复用实例的编辑器视图在关闭工程时已关闭；若打开新编辑器失败，需要在复用前调用 `Plugin::close_editor` 或在 `take_reusable` 中重建视图。
- 同一窗口内切换工程仍使用 terminate；若该路径崩溃，`engine.plugin.shutdown` 的 begin 日志可定位插件。
