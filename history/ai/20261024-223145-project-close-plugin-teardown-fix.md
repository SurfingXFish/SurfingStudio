# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-24 22:31:45 (Asia/Shanghai) |
| AI 工具/模型 | Trae / AI 编码助手 |
| 任务来源 | 修复关闭工程时软件卡死/崩溃（日志显示 `0xC0000005`） |
| 关联分支/提交 | 未知 / 未知 |
| 影响模块 | app, audio, diag, docs |
| 关联记录 | `history/ai/20261024-221100-velocity-bar-diagnostic-log-exit.md` |

## 2. 任务目标

- 定位并修复关闭含 VST 插件的工程时界面卡死、进程崩溃的问题。
- 保留可诊断性：关闭工程的每个析构阶段都能从日志判断停在哪一步。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/mod.rs` | 修改 | `SurfingApp::drop` 改为分阶段写 `window.drop` 日志（`begin` / `audio_deactivated` / `engine_refs` / `workers_stopped` / `editors_destroyed` / `history_leaked` / `save_waited` / `engine_retained` 或 `engine_drop_next`）；当工程含插件实例（`plugin_instruments + plugin_effects > 0`）或强制关闭时，`std::mem::forget` 一个引擎强引用，保留插件实例至进程结束；仅在纯内置乐器时正常析构 |
| `src/audio/backend.rs` | 修改 | `AudioRouter` 新增 `unregister`：窗口关闭且引擎保留时从路由列表移除该引擎并清空激活槽位，避免保留的引擎继续参与路由与暂停遍历 |
| `src/app/files.rs` | 修改 | 应用工程会话时新增 `project.plugins` 日志，记录工程使用的插件实例清单（乐器/效果器/主效果器，名称 + 路径） |
| `src/diag/mod.rs` | 修改 | 新增 `thread_id()`（Win32 线程 id），用于标注 `window.drop` 等跨线程记录 |
| `doc/日志规范.md` | 修改 | 事件目录新增 `window.drop`、`project.plugins`；补充 `tid` 字段说明与“关闭工程卡死/含插件”判读条目 |
| `doc/技术文档.md` | 修改 | 7.1 关闭工程窗口的引擎保留策略与原因；13.4 关闭单个工程窗口的日志与保留行为 |
| `doc/框架工作流.md` | 修改 | 新增 1.10“关闭工程窗口（应用继续运行）”流程；`AudioRouter` 与 `SurfingApp::drop` 函数索引 |
| `doc/项目简介.md` | 修改 | 2.6 关闭工程窗口行为；已知限制补充插件实例保留至退出 |
| `doc/代码规范.md` | 修改 | 8.2 新增禁止在 UI 线程析构插件实例的规则与正确做法 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 关闭含插件的工程窗口 | `SurfingApp::drop` 结束后在 UI 线程析构 `Engine`，逐个析构插件实例 | 先从 `AudioRouter` 注销、暂停、停止后台线程并销毁编辑器；工程含插件实例时保留引擎（不析构），纯内置乐器才正常析构 | 修复 VOCALOID VSTi / ACE Bridge 等插件析构等待 UI 消息循环导致的永久卡死与 `0xC0000005` |
| 关闭工程的诊断 | 无阶段记录，卡死时无法判断位置 | 每个析构阶段写 `window.drop`（含 `phase`、`tid`、插件数量） | 卡死时最后一条 `phase` 即现场；`phase=engine_retained` 表示按设计保留 |
| 工程加载 | 只记录轨道/片段数量 | 额外记录 `project.plugins`（插件名称与路径清单） | 可在关闭卡死/崩溃后直接定位到具体插件 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无 | 无 | 无 | 无 | 未新增或修改常量（沿用 `ENGINE_RELEASE_WAIT`） |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 7.1 关闭工程窗口与插件保留；13.4 关闭单个工程窗口 |
| `doc/关键常量.md` | 否 | 本次未新增/修改常量 |
| `doc/框架工作流.md` | 是 | 新增 1.10；3.2/3.4 函数索引 |
| `doc/项目简介.md` | 是 | 2.6 关闭工程窗口；7 已知限制 |
| `doc/代码规范.md` | 是 | 8.2 插件生命周期规则 |
| `doc/日志规范.md` | 是 | 5.2/5.3 事件目录、7.1 判读条目、5 节 `tid` 说明 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | 58 passed、16 ignored |
| `cargo check --all-targets` | 通过 | 无错误 |
| 手工验证（复现问题） | 通过 | 修复前打开 `光年外的流星.sfsd` 后关闭工程：`window.drop` 停在 `phase=engine_drop_next`，UI 卡死 78 秒以上；日志确认 24 个插件实例（11 乐器 + 13 效果器） |
| 手工验证（修复后） | 通过 | 同一工程关闭耗时约 20 ms：`window.close.project` → `window.drop` 各阶段 → `phase=engine_retained;plugins=24`，界面立即回到主页；随后关闭主页进程即时退出 |
| 手工验证（用户崩溃日志） | 通过 | 用户日志中的 `0xC0000005` 发生在 `window.close.project` 之后，与本次复现的析构路径一致 |

## 8. 风险与后续

- 风险：关闭含插件的工程后，该工程的插件实例保留至进程结束，内存不会立即释放；反复打开/关闭重插件工程会累积占用（已写入项目简介的已知限制）。
- 风险：保留的插件实例仍在进程内，个别插件可能保留后台线程；它们已从音频路由移除，不参与渲染。
- 后续：如需彻底释放，可考虑插件进程外托管（out-of-process hosting），或对已知安全插件按白名单正常析构。
