# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-02 18:13:50 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户反馈“两个问题还是都没解决”；用 18:07 的日志（`plugin_lock_miss=56`）定位到跳过块导致的累积偏移 |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | audio, plugin, docs |
| 关联记录 | `history/ai/20261102-180343-plugin-delay-compensation.md` |

## 2. 任务目标

- 解释并消除“越放越晚”的累积延迟：插件被跳过一块后内部走带不会自己补回。
- 降低 192 kHz 下对插件的调用频率，避免桥接插件（VOCALOID / ACE Bridge）被高频 `process` 拖慢。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/audio/engine/mod.rs` | 修改 | 新增 `render_block_size(sample_rate)`（约 10 ms，`512~4096` 帧）；`Engine::build` 与 `set_sample_rate` 使用它；`EffectRuntime::process` 传入 `None` 走带 |
| `src/audio/engine/render.rs` | 修改 | 乐器插件每块都构造宿主 `TransportPosition` 传给 `PluginRuntime::process` |
| `src/plugin/runtime.rs` | 修改 | `process` 新增 `transport` 参数：跳过块时记录待发送位置；成功时**仅当插件内部位置与宿主相差 > 1 采样**才写回 `ProcessContext`，正常连播不做重置 |
| `src/plugin/mod.rs` | 修改 | 诊断测试调用点补 `None` |
| `doc/技术文档.md` | 修改 | 4.7 新增“跳过块后的位置纠正”“内部块长”，更新缓冲复用说明 |
| `doc/关键常量.md` | 修改 | 分开记录“内部渲染块”和“设备缓冲” |
| `doc/框架工作流.md` | 修改 | 渲染函数表补充块长与走带对齐 |
| `doc/项目简介.md` | 修改 | 新增“高采样率块长与走带对齐” |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 内部块长 | 恒定 512 帧（192 kHz 下 2.67 ms） | 约 10 ms：192 kHz → 1920 帧 | 插件调用次数 375/s → 100/s，块长恒定、无尾块 |
| 跳块后的走带 | 插件永久落后一块并累积 | 下一块检测到差异后拉回宿主位置 | 不再“越放越晚”，编辑器位置不漂移 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `Engine::block_size` | 固定 `512` | `render_block_size(采样率)`：48 kHz → 512、96 kHz → 960、192 kHz → 1920（限制 512~4096） | `src/audio/engine/mod.rs` | 采样率变化时同步更新并强制重载插件 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 4.7 插件处理三项说明 |
| `doc/关键常量.md` | 是 | 第 2 节音频常量表 |
| `doc/框架工作流.md` | 是 | 1.3 渲染流程函数表 |
| `doc/项目简介.md` | 是 | 第 2 节音频条目 |
| `doc/代码规范.md` | 否 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | 110 passed / 20 ignored（新增 `render_block_size_tracks_the_sample_rate`） |
| 日志依据 | 已确认 | 18:07 日志：`plugin_lock_miss=56`（单窗口）、`engine.lock`/渲染均正常、`out_latency_ms=31`；`engine.latency comp_samples=6720 comp_ms=35 slowest=…合唱轨(3)` 证明补偿已生效 |

## 8. 风险与后续

- 风险：块长变大后单块的 `max_us` 可能变高，但每秒总工作量下降；`render_max_us` 与 `over_budget` 仍需观察。
- 风险：`set_transport_position` 只在差异 > 1 采样时调用，若某插件自己改写 `projectTimeSamples` 会被纠正回宿主位置（这是期望行为）。
- 后续：若仍有延迟，用新日志的 `plugin_lock_miss`（是否仍高）、`render_avg_us`（是否下降）与 `engine.latency` 判断是插件内部缓冲还是宿主问题。
