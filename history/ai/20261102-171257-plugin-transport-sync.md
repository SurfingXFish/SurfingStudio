# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-02 17:12:57 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 反馈「开启效果器后整体发声变晚」「VOCALOID6 独立窗口卡死、不跟随播放」 |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | audio, plugin, docs |
| 关联记录 | `history/ai/20261101-233939-gpu-timbre-training.md` |

## 2. 任务目标

- 排查效果器是否在渲染链上引入额外延迟。
- 让 VSTi 编辑器（VOCALOID 6 等）跟随宿主走带的播放状态与位置。
- 用测试固定「内置效果器不引入干信号延迟」这一结论，防止回归。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/runtime.rs` | 修改 | 新增 `PluginRuntime::set_transport`：用 `try_lock` 写入 `kPlaying` 与 `TransportPosition`（采样位置 + 四分音符位置） |
| `src/audio/engine/render.rs` | 修改 | `render_chunk` 的插件乐器分支每块调用 `set_transport(true, …)`；新增 `send_plugin_transport_stopped`，停止时补发一次 `kPlaying=false` |
| `src/audio/engine/mod.rs` | 修改 | 新增 `plugin_transport_playing` 字段并在构造时初始化 |
| `src/audio/fx/native.rs` | 修改 | 新增测试 `native_effects_pass_dry_signal_without_latency`：脉冲输入下每种内置效果器第 0 个采样都有干信号 |
| `doc/技术文档.md` | 修改 | 4.7 VST3 处理补充走带同步说明 |
| `doc/框架工作流.md` | 修改 | `runtime.rs` 与 `render.rs` 函数表补充 `set_transport` / `send_plugin_transport_stopped` |
| `doc/项目简介.md` | 修改 | 2.4 插件说明补充编辑器跟随播放 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| VST3 走带信息 | 只发送 BPM 与拍号 | 每个 block 发送 `kPlaying` 与工程位置 | VOCALOID 6 等编辑器时间线可跟随播放 |
| 走带停止 | 插件一直停留在“播放中” | 停止后的第一个回调补发一次 `kPlaying=false` | 编辑器播放头随停止复位 |
| 效果器延迟结论 | 无测试约束 | 每种内置效果器的干信号必须从第 0 个采样出现 | 防止后续实现引入隐藏延迟 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无 | 无 | 无 | 无 | 无 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 4.7 插件乐器/效果器处理 |
| `doc/关键常量.md` | 否 | 未新增常量 |
| `doc/框架工作流.md` | 是 | `runtime.rs`、`render.rs` 函数表 |
| `doc/项目简介.md` | 是 | 2.4 插件 |
| `doc/代码规范.md` | 否 | 规范本身无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 `cargo fmt --all` |
| `cargo test` | 通过 | 新增效果器延迟测试与既有引擎/插件测试均通过 |
| 手工验证 | 未执行 | 需在装有 VOCALOID 6 的构建版中确认编辑器跟随播放 |

## 8. 风险与后续

- 风险：`set_transport` 每块调用一次插件接口；插件忙时 `try_lock` 失败会跳过本块，位置在下一块补齐，不影响音频。
- 结论：内置效果器链路不引入延迟（每块直接处理干信号，延迟/混响只是叠加湿声）。
- 后续：若「整体发声变晚」只出现在 VST 效果器上，需要做插件延迟补偿（PDC）；若内置效果器也出现，请先检查音频缓冲与 CPU 占用（设置 → 音频），必要时加入音频回调超时诊断。
