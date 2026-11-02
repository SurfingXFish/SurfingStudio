# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-02 17:31:37 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 关闭 GPU 音频后 0.5 秒延迟仍在；VOCALOID6 编辑器完全冻结 |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | audio, plugin, docs |
| 关联记录 | `history/ai/20261102-172630-gpu-limiter-nonblocking.md` |

## 2. 任务目标

- 修正上一轮把走带位置**每块**写给插件的做法：这会让 VOCALOID 6 反复重置内部定位，编辑器冻结并带来明显延迟。
- 位置只在时间轴不连续时发送；播放状态只在变化时发送。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/runtime.rs` | 修改 | `set_transport` 拆为 `set_playing`（仅状态变化）与 `seek_transport`（仅定位），两者都 `try_lock` |
| `src/audio/engine/mod.rs` | 修改 | 新增 `plugin_transport_seek`；`play` / `seek` / `sync_project`（本轮新建过插件）置位 |
| `src/audio/engine/render.rs` | 修改 | 插件分支只在 `!plugin_transport_playing` 时发 `kPlaying=true`、只在 seek 置位时发位置；循环回绕置位；块末清除置位；停止时只发 `kPlaying=false` |
| `doc/技术文档.md` | 修改 | 4.7 走带同步规则改为“禁止每块发送位置” |
| `doc/框架工作流.md` | 修改 | `runtime.rs` 函数表 |
| `doc/项目简介.md` | 修改 | 2.4 编辑器跟随时机说明 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 插件位置 | 每个 block 都发 `set_transport_position` | 只在开始播放、定位、循环回绕、新建实例时发一次 | 插件不再被反复 seek；避免编辑器冻结与附加延迟 |
| 播放状态 | 每块都发 `set_playing(true)` | 只在状态变化时发；停止补发一次 false | 减少实时线程上的插件调用 |
| 新插件实例 | 依赖下一块的位置 | 同步阶段检测到新建插件后置 seek | 新实例第一块就有正确位置 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无 | 无 | 无 | 无 | 无 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 4.7 |
| `doc/关键常量.md` | 否 | 无新增常量 |
| `doc/框架工作流.md` | 是 | `runtime.rs` 函数表 |
| `doc/项目简介.md` | 是 | 2.4 |
| `doc/代码规范.md` | 否 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | 106+ 用例通过 |
| 手工验证 | 未执行 | 需在装有 VOCALOID 6 的构建版中确认 |

## 8. 风险与后续

- 风险：若插件依赖每块的位置更新才能计时（不符合 VST3 约定），其时间线可能不跟随；这类插件应按 `ProcessContext` 自行推进。
- 后续：0.5 秒延迟与编辑器冻结若仍存在，需要用 `%LOCALAPPDATA%\SurfingStudio\logs` 的 `.sflog` 判断 UI 线程被哪一段阻塞（看门狗会记录活动名），再决定修复点。
