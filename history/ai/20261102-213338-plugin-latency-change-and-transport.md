# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-02 21:33:38 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户反馈“开效果器后仍有延迟，主要出现在 Pro-Q 4”“VOCALOID6 能启动但不跟随项目走条、放音符不出声” |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | audio, plugin, app, docs |
| 关联记录 | `history/ai/20261102-205222-plugin-lookahead-pdc.md`（实时前瞻）、`history/ai/20261102-180343-plugin-delay-compensation.md`（轨道补偿） |

## 2. 任务目标

- 查清 Pro-Q 4 的延迟为何没有进入补偿。
- 让 VST3 乐器的编辑器（VOCALOID 6 的独立窗口）跟随工程走条。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/runtime.rs` | 修改 | 新增 `PluginRuntime::refresh_host_requests`：在插件控制线程上 `service_host_requests`，刷新 `latency_samples` 并报告变化；`process` 改为每块写回 `TransportPosition` |
| `src/audio/engine/mod.rs` | 修改 | 抽出 `apply_latency_compensation`（补偿量 + 前瞻量）；`report_plugin_latency` 改为接收最慢轨名；新增 `Engine::refresh_plugin_latency`（刷新链延迟并立即并入补偿） |
| `src/audio/engine/plugins.rs` | 修改 | `poll_parameter_changes` 每次轮询先调用 `refresh_plugin_latency`（UI 线程，乐器插件的控制线程） |
| `src/app/transport.rs` | 修改 | `run_sync_service` 空闲分支每 200 ms `try_lock` 引擎并调用 `refresh_plugin_latency`（效果器插件的控制线程） |
| `doc/技术文档.md` | 修改 | 4.7 增加“插件延迟变化”“每块写回宿主位置”，替换旧的“跳过块后才纠正位置”描述 |
| `doc/关键常量.md` | 修改 | 插件延迟表补 Pro-Q 4 三档实测值；新增延迟轮询间隔 |
| `doc/框架工作流.md` | 修改 | 5.3 参数轮询流程加入延迟刷新；插件 API 表补充 |
| `doc/项目简介.md` | 修改 | 延迟补偿条目补充运行中延迟变化；新增“走带与插件编辑器同步” |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 插件延迟变化 | 只在工程同步时读取一次 `latency_samples` | 每 200 ms 读取 `restartComponent` 并立即重算补偿/前瞻 | Pro-Q 4 切到线性相位后不再按 0 延迟发声 |
| 插件位置 | 仅当插件内部位置与宿主相差 > 1 采样时才写回 | 每个块都写回宿主位置（只改结构体字段） | 编辑器播放头与宿主严格同步，跳过块也不会累积偏差 |
| 引擎锁占用 | 后台同步线程只在同步时持锁 | 空闲时每 200 ms `try_lock` 一次做延迟刷新 | 只在 `restartComponent` 真正到达时才做停用/重启用，`try_lock` 失败直接跳过 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 延迟变化轮询间隔 | 无 | `200` ms | `src/audio/engine/plugins.rs`、`src/app/transport.rs` | 与参数轮询同频，避免额外定时器 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 4.7 VST3 处理小节 |
| `doc/关键常量.md` | 是 | 第 2 节音频常量表 |
| `doc/框架工作流.md` | 是 | 5.3 插件 GUI 参数变化；插件 API 表 |
| `doc/项目简介.md` | 是 | 第 2 节音频条目 |
| `doc/代码规范.md` | 否 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | 112 passed / 20 ignored（新增 `latency_change_recomputes_compensation_and_ahead`） |
| Pro-Q 4 延迟实测 | 已执行 | 192 kHz 下按参数 `Processing Mode` 采样：`0` → 0 采样、`0.25/0.5` → 320 采样、`0.75/1.0` → 20480 采样（约 107 ms）；`refresh_host_requests` 在值变化时返回 `true` 并同步刷新缓存 |
| VOCALOID 编辑器状态 | 已执行 | 保留编辑器约 10 s（不阻塞音频线程）后，编辑器可完整加载且不再弹“无法初始化VSM”；说明此前的失败与并发实例/初始化时序有关 |
| 内置用例 | 通过 | `latency_change_recomputes_compensation_and_ahead` 覆盖延迟变化后最慢轨、其它轨补偿与前瞻量 |

## 8. 风险与后续

- 风险：每 200 ms 在 UI 线程与后台线程各做一次插件 `try_lock` 轮询；插件忙时直接跳过，开销可忽略，但插件数量很多时仍有少量锁竞争。
- 风险：`restartComponent(kLatencyChanged)` 到达时会按 VST3 要求短暂停用/重启用插件，播放中切换 Pro-Q 4 模式可能出现一次极短的空隙，随后恢复对齐。
- 风险：`service_host_requests` 只能在插件控制线程调用；乐器插件在 UI 线程创建、效果器插件在后台同步线程创建，两处轮询分别覆盖，若将来改变加载线程需要同步调整。
- 后续：VOCALOID 6 的“跟随走条”现在依赖每块写回位置；若插件仍不跟随，需要在其编辑器侧确认是否要求额外的宿主接口。
