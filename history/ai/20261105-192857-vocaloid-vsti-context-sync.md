# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-05 19:28:57 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | “VOCALOID6 VSTi 模式走带时插件不跟随；暂停时点击编辑器内音符也没有声音，网络检索后修复” |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | audio, plugin, app, docs |
| 关联记录 | `history/ai/20261105-162207-ara-bridge-sync-only.md` |

## 2. 任务目标

- 让 VOCALOID 6 VSTi 在宿主播放时跟随走带。
- 让走带暂停时，VOCALOID 编辑器内点击音符的试听能正常发声。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `vendor/vst3-host/src/internal/plugin_impl.rs` | 修改 | `process_context_needs` 把 `IProcessContextRequirements` 返回 `Some(0)` 与未实现同等视为 legacy 完整上下文；`PROCESS_CONTEXT_STATE` 增加 `kBarPositionValid` / `kSystemTimeValid`；`process_context_state` 为 0 时返回完整 valid 位并补充 `kNeedBarPositionMusic` 映射；初始化、`set_transport_position` 与 `advance_process_context` 同步写入/推进 `barPositionMusic`；新增 `zero_requirements_fall_back_to_the_legacy_context` 回归测试 |
| `src/audio/engine/render.rs` | 修改 | 非播放状态下，`live_timeout` 或乐器编辑器打开时继续 `render_live`；试听路径不再发送 `kPlaying` / 重新定位，也不再置位 `plugin_transport_playing` |
| `src/audio/engine/mod.rs` | 修改 | 新增 `plugin_editor_active` 字段与 `Engine::set_plugin_editor_active`，控制暂停时是否继续处理插件链 |
| `src/app/mod.rs` | 修改 | `ui_frame` 每帧非阻塞地把“是否有乐器编辑器打开”写入引擎 |
| `doc/技术文档.md` | 修改 | 4.7 增加“进程上下文按需提供”；6.6 增加“停止时的编辑器试听” |
| `doc/框架工作流.md` | 修改 | 回调流程图、`src/audio/engine/render.rs` 与 `src/audio/engine/mod.rs` 函数表、插件走带表同步更新 |
| `doc/项目简介.md` | 修改 | “走带与插件编辑器同步”补充 0 需求位回退；新增“编辑器内试听” |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 插件 ProcessContext 提供策略 | `IProcessContextRequirements` 返回 0 时不提供任何有效字段 | 0 与未实现都按 legacy 提供完整上下文并置位 valid 标志 | VOCALOID 6 能读到速度、拍号、音乐位置与播放状态，编辑器跟随走带并渲染音频 |
| 暂停时的音频回调 | 只有宿主试听超时内才 `render_live`，否则直接衰减电平 | 乐器编辑器打开时也持续 `render_live` | 编辑器内点击音符的试听有输出；关闭编辑器后恢复静音衰减 |
| 试听与播放状态 | `render_chunk` 在 `advance == false` 时也会置位播放标志并可能重新定位 | 只有 `advance == true` 才发送 `kPlaying`、定位与置位播放标志 | 试听不会把插件误标为“播放中”，也不反复重置插件内部时间轴 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无 | 无 | 无 | 无 | 无 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 4.7 插件处理与 6.6 插件编辑器 |
| `doc/关键常量.md` | 否 | 未新增或修改常量 |
| `doc/框架工作流.md` | 是 | 1.3 回调流程图、`src/audio/engine/` 与 `src/plugin/` 函数表 |
| `doc/项目简介.md` | 是 | 走带同步与编辑器试听两条用户可见行为 |
| `doc/代码规范.md` | 否 | 无规范变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 `cargo fmt --all` |
| `cargo test` | 通过 | `cargo test --lib`：116 passed / 0 failed / 23 ignored |
| 手工验证 | 未执行 | 需在装有 VOCALOID 6 的机器上播放并暂停后点击编辑器音符确认 |

## 8. 风险与后续

- 风险：仅在检测到乐器编辑器打开时持续处理，若插件在编辑器关闭后仍需后台处理（例如长尾音），会在暂停后停止；可在需要时扩展为“编辑器关闭后再保持短暂处理窗口”。
- 后续：VOCALOID 混音器若把轨道输出路由到 `Bus 1…16`，本宿主当前只读取 `Main`（Bus 0），需要用户把 `OUTPUT` 改回 `Main`，或后续实现多输出总线选择。
