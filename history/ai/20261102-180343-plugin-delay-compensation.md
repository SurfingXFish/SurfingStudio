# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-02 18:03:43 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户反馈“开启效果器后声音延迟、VOCALOID 不跟随”，且 192 kHz 必须保留；用新日志与插件延迟实测定位 |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | audio, plugin, docs |
| 关联记录 | `history/ai/20261102-175054-audio-playback-telemetry.md` |

## 2. 任务目标

- 用播放期遥测判断延迟来自设备缓冲、引擎超预算还是锁竞争。
- 依据实测的插件延迟，修正“开启效果器后该轨整体变晚”的行为。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/audio/engine/mod.rs` | 修改 | 新增 `CompDelay`（轨道延迟补偿环形缓冲）、`InstrumentRuntime::latency_samples`、`EffectRuntime::latency_samples`、`TrackEngine::chain_latency` / `comp_delay`、`Engine::report_plugin_latency`；同步时统计每条链的插件延迟并把各轨补齐到最慢的一条，延迟变化时写 `engine.latency` |
| `src/audio/engine/render.rs` | 修改 | `render_chunk` 在效果链之后应用延迟补偿；静音轨道清空补偿缓冲 |
| `src/plugin/runtime.rs` | 修改 | 新增 `PluginRuntime::latency_samples` 与 `Vst3Runtime::latency_samples` 缓存（插件忙时沿用上次值） |
| `src/plugin/backends/vst3.rs` | 修改 | 新建 `Vst3Runtime` 时初始化延迟缓存 |
| `doc/技术文档.md` | 修改 | 4.2 渲染流程图加入延迟补偿；4.7 更新缓冲容量复用、走带补发与延迟补偿说明 |
| `doc/关键常量.md` | 修改 | 新增“插件延迟（实测 192 kHz）”参考行 |
| `doc/框架工作流.md` | 修改 | 渲染流程图与函数表加入延迟补偿 |
| `doc/项目简介.md` | 修改 | 新增“延迟补偿”条目 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 轨道对齐 | 各轨按插件实际延迟各自发声 | 全部补齐到最慢的链 | 开启 Pro-MB / Pro-DS 后该轨不再单独变晚 |
| 延迟日志 | 无 | 延迟变化时写 `engine.latency`（`comp_samples` / `comp_ms` / `master_ms` / `slowest`） | 可直接看出是哪个插件、哪条轨带来的延迟 |
| 暂停/定位 | 补偿缓冲会残留旧音频 | `panic` 与静音时清空 | 恢复播放不冒出上一段声音 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无新增常量 | — | — | — | 补偿长度由插件上报，等于“最慢链延迟 − 本轨延迟” |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 4.2 每块处理流程；4.7 插件乐器/效果器处理 |
| `doc/关键常量.md` | 是 | 第 2 节音频常量表 |
| `doc/框架工作流.md` | 是 | 1.3 回调流程；`src/audio/engine/render.rs` 函数表 |
| `doc/项目简介.md` | 是 | 第 2 节音频条目 |
| `doc/代码规范.md` | 否 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | 109 passed / 20 ignored（新增 2 个 `CompDelay` 用例） |
| 插件延迟实测 | 已执行 | 192 kHz / 512 帧下逐个加载：Pro-MB `3840` 采样（20 ms）、Pro-DS `2880` 采样（15 ms），VOCALOID / ACE Bridge / TDR Nova / Pro-Q 4 / MT-PowerDrumKit / Serum 2 均为 `0`，Keyzone Classic 为 VST2（无接口） |
| 播放遥测实测 | 已执行 | 新日志（release，17:54）设备 `192000 Hz · 2 ch · 1152 帧`，实际 `cb_frames=1920`（10 ms），`out_latency_ms≈31`；播放期 `engine_lock_miss=0`、`plugin_lock_miss=0`，`render_avg_us` 随编曲密度从 273 升到 5364（预算 10 ms） |

## 8. 风险与后续

- 风险：延迟补偿让所有轨道等最慢的一条，整段混音相对播放头后移 `最大链延迟`（本工程约 35 ms）；离线导出同样会多出这段前导静音，属于已知后续项（导出可加等长预卷/裁剪）。
- 风险：插件在 GUI 里自行切换相位模式导致延迟变化时，要等下一次工程同步才重新计算。
- 结论：设备缓冲（31 ms）与引擎渲染（5.4 ms / 10 ms、零缺块）都不是“半秒延迟”的来源；此前测得的最大事件级偏差来自插件延迟未补偿（15–20 ms/插件）与走带状态被丢弃。
- 后续：若仍有明显延迟，看新日志里的 `out_latency_ms`、`over_budget`、`engine_lock_miss`、`plugin_lock_miss` 与 `engine.latency` 四项即可定位方向。
