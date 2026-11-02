# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-02 19:20:48 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 19:16 日志确认 `plugin.window.external threads=15232`（等于 UI 线程），但 VOCALOID 编辑器仍冻结 |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | plugin, app, audio, docs |
| 关联记录 | `history/ai/20261102-191205-plugin-external-window-refresh.md` |

## 2. 任务目标

- 让插件在本线程创建的 WPF 窗口（VOCALOID6 Editor）真正运行起来，而不是只收到输入才刷新。
- 让插件自建窗口持续收到处理器输出参数反馈。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/editor.rs` | 修改 | 新增 `pump_plugin_thread_windows()`：每帧 `EnumThreadWindows` 本线程窗口，只对类名属于 WPF（`HwndWrapper…` / `MS.Win32…`）的窗口取出并派发全部消息；`nudge_external_windows` 改为返回是否存在插件自建窗口（节流期间沿用上次结果） |
| `src/plugin/mod.rs` | 修改 | 导出 `pump_plugin_thread_windows` |
| `src/audio/engine/plugins.rs` | 修改 | 新增 `Engine::service_all_editor_feedback()`：给所有带编辑器的插件转发输出参数反馈 |
| `src/app/plugins.rs` | 修改 | `service_editors` 每帧调用 WPF 消息泵；存在插件自建窗口时调用 `service_all_editor_feedback` |
| `doc/技术文档.md` | 修改 | 6.6 新增“WPF 窗口的 Dispatcher”说明 |
| `doc/框架工作流.md` | 修改 | `src/plugin/editor.rs` 函数表新增一行并补充返回值说明 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| WPF 窗口消息 | 只派发 `WM_TIMER`（winit 只处理自己的窗口） | 每帧派发本线程 WPF 类名窗口的全部消息 | WPF Dispatcher 的排队工作项可执行 |
| 参数反馈 | 仅按宿主编辑器槽位转发 | 存在插件自建窗口时对全部插件转发 | 插件自建窗口可持续刷新 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无新增 | — | — | — | 沿用 200 ms 的外部窗口重绘间隔 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.6 插件编辑器线程协作 |
| `doc/框架工作流.md` | 是 | `src/plugin/editor.rs` 函数表 |
| `doc/项目简介.md` | 否 | 上一条记录已描述自建窗口行为 |
| `doc/关键常量.md` | 否 | 无新增常量 |
| `doc/代码规范.md` | 否 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo check --all-targets` | 通过 | 仅余既有告警 |
| `cargo test` | 通过 | 110 passed / 20 ignored |
| 日志证据 | 已确认 | `plugin.window.external titles=VOCALOID6 Editor threads=15232`；播放期 `plugin_lock_miss=0`、`render_avg_us` 1.3–2.3 ms、`out_latency_ms=32`、`engine.latency comp_ms=35`（全程未变化） |
| 手工验证 | 未执行 | 需用户打开 VOCALOID6 Editor 后不点击观察是否自行刷新/跟随播放 |

## 8. 风险与后续

- 风险：派发插件窗口消息会在帧内重入插件代码（与该窗口的消息循环等价）；只匹配 WPF 类名，避免抢走 winit 的输入消息。
- 若 VOCALOID 编辑器仍冻结，则说明其刷新依赖插件内部的 IPC/音频渲染而非窗口消息，需要在插件侧（或通过其桥接设置）解决；可通过同一工程在其它宿主中的表现对比确认。
- “打开效果器后整体后延”：日志中 `engine.latency` 全程只有一条（35 ms，加载时），说明补偿未在打开编辑器时跳变；Pro-MB/Pro-DS 的前视 20/15 ms 属插件固有延迟。
