# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-02 19:12:05 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户确认“VOCALOID 不点击就完全在后台不反应”，并指出 19:03 日志中 `editors=0` |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | plugin, app, docs |
| 关联记录 | `history/ai/20261102-181350-plugin-block-size-and-transport-drift.md` |

## 2. 任务目标

- 修正插件**自建**窗口（VOCALOID6 Editor）在后台完全不刷新、不跟随播放的问题。
- 为“打开 VST 效果器后整体后延”提供可核对的日志证据。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/plugins.rs` | 修改 | `service_editors` **无条件**调用 `pump_editor_timers()`（原来仅在 `self.editors` 非空时调用），并调用新增的 `nudge_external_windows()` |
| `src/plugin/editor.rs` | 修改 | 新增 `nudge_external_windows()`：每 200 ms 枚举本进程可见的非宿主顶层窗口，调用 `RedrawWindow(RDW_INVALIDATE \| RDW_ALLCHILDREN)`，并在窗口集合变化时写 `plugin.window.external`（`titles` / `threads`）；新增非 Windows 空实现 |
| `src/plugin/mod.rs` | 修改 | 导出 `nudge_external_windows` |
| `doc/技术文档.md` | 修改 | 6.6 说明无条件派发与插件自建窗口重绘、线程 id 记录 |
| `doc/框架工作流.md` | 修改 | `src/plugin/editor.rs` 函数表新增两行 |
| `doc/项目简介.md` | 修改 | 新增“插件自建窗口不再不点就不动” |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 定时器派发 | 只有宿主创建的编辑器存在时才派发 `WM_TIMER` | 每帧无条件派发 | 插件自建窗口的定时器开始运行 |
| 插件自建窗口 | 后台不重绘，只有输入唤醒 | 每 200 ms 强制失效一次 | 窗口持续刷新，播放头可跟随 |
| 诊断 | 无窗口线程信息 | `plugin.window.external` 记录标题与线程 id | 可确认窗口由哪个线程的消息循环驱动 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 外部窗口重绘间隔 | 无 | `200` ms（`INTERVAL_MS`） | `src/plugin/editor.rs` | 与后台编辑器重绘间隔一致 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.6 插件编辑器线程协作 |
| `doc/框架工作流.md` | 是 | `src/plugin/editor.rs` 函数表 |
| `doc/项目简介.md` | 是 | 第 2 节编辑器条目 |
| `doc/关键常量.md` | 否 | 无新增全局常量表项 |
| `doc/代码规范.md` | 否 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo check --all-targets` | 通过 | 仅余既有告警 |
| `cargo test` | 通过 | 110 passed / 20 ignored |
| 手工验证 | 未执行 | 需用户打开 VOCALOID6 Editor 观察是否持续刷新，并提供 `plugin.window.external` 的线程 id |

## 8. 风险与后续

- 风险：对非宿主窗口每 200 ms 强制失效会增加少量绘制；若插件窗口不是本线程创建的，失效请求不会生效（此时其线程 id 与 UI 线程不同，可从日志确认）。
- 事实核对：19:03 日志 `editors=0`，说明插件自建窗口从未进入 `self.editors`，因此旧实现完全跳过了定时器派发；这与用户描述的“不点击就完全没反应”一致。
- 后续：若 `plugin.window.external` 的 `threads` 与 UI 线程不同，需要改为在插件线程上驱动（例如让同步线程泵消息，或使用插件的 run loop 定时器）。
