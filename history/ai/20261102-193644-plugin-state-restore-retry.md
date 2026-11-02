# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-02 19:36:44 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户确认“VOCALOID6 既不跟随播放也不发声，其他 DAW 正常”；用独立探针复现 `setState` 失败 |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | plugin, audio, app, docs |
| 关联记录 | `history/ai/20261102-192048-wpf-dispatcher-pump.md`、`history/ai/20261102-180343-plugin-delay-compensation.md` |

## 2. 任务目标

- 找到 VOCALOID6 在宿主里“既不出声也不跟随”的根因并修复。
- 让插件拒绝的状态恢复可被观察、可重试。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/runtime.rs` | 修改 | `Vst3Runtime` 新增 `pending_state`；`restore_state` 检查 `load_state` 结果，失败时保存字节并写 `plugin.state`（`status=declined`）；新增 `retry_state_restore`；新增 `transport_snapshot` |
| `src/audio/engine/plugins.rs` | 修改 | 新增 `retry_all_state_restores`（每秒最多一次）与 `log_playback_probe`（每 5 秒记录插件内部走带与各轨电平） |
| `src/app/plugins.rs` | 修改 | 插件自建窗口存在时调用 `retry_all_state_restores` 与 `service_all_editor_feedback`（上一轮已加入 WPF 消息派发） |
| `src/app/transport.rs` | 修改 | `refresh_engine_snapshot` 调用 `log_playback_probe` |
| `src/plugin/backends/vst3.rs` | 修改 | 初始化 `pending_state` |
| `doc/技术文档.md` | 修改 | 6.6 新增“插件状态恢复可能被拒绝” |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 状态恢复失败 | `let _ = plugin.load_state(...)` 静默丢弃 | 记录待恢复字节 + `plugin.state` 警告 | 失败可见、可重试 |
| 重试时机 | 无 | 插件自建窗口出现后每秒重试一次 | VOCALOID 编辑器就绪后可真正载入歌曲/歌手 |
| 播放探查 | 无 | 每 5 秒 `engine.plugin.transport` / `engine.track.levels` | 可判断插件是否跟随、某轨是否有输出 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 状态恢复重试间隔 | 无 | `1000` ms | `src/audio/engine/plugins.rs` | 状态块可达数十 KB，不能每帧重试 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.6 插件编辑器线程协作 |
| `doc/关键常量.md` | 否 | 仅为局部重试间隔，未列入全局常量表 |
| `doc/框架工作流.md` | 否 | 函数职责已在相邻条目覆盖 |
| `doc/项目简介.md` | 否 | 用户可见行为在修复确认后补充 |
| `doc/代码规范.md` | 否 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| 探针复现 | 已完成 | 用工程里 46568 字节的 VOCALOID 状态调用 `load_state`：`load_state FAILED: Failed to restore component state (setState: 0x1)`；`0x1` 即 `kResultFalse` |
| 工程数据核对 | 已完成 | `洛天依_V5` 轨 `clips=0 notes=0`（人声实际以音频片段存在），因此插件没有宿主 MIDI；`洛天依_ACE`、`Vsinger合唱` 为 `muted=true` |
| `cargo fmt` / `check` | 通过 | 仅余既有告警 |
| `cargo test` | 通过 | 110 passed / 20 ignored |
| 手工验证 | 未执行 | 需用户打开工程后观察 `plugin.state`（`phase=retry`）与 `engine.track.levels` 中洛天依_V5 是否出现电平 |

## 8. 风险与后续

- 风险：若插件始终拒绝状态，重试会每秒发生一次（每次 `setState` 数十 KB），日志只记录首次失败，不会刷屏。
- 事实：VOCALOID 轨没有 MIDI 片段；若用户期望它按宿主 MIDI 演唱，需要在工程里补音符；若期望它播放编辑器内歌曲，则本修复的“编辑器就绪后重试状态”是必要条件。
- 后续：若 `engine.track.levels` 显示洛天依_V5 已有电平而音频仍不跟随，则问题在插件内部同步，需按其桥接设置处理。
