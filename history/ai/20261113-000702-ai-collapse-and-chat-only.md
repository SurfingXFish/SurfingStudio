# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-13 00:07:02 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 未标注模型 |
| 任务来源 | 修复任务耗时展开后无法折叠、折叠/展开页面跳动；删除 AI 训练、输出模式与评分，仅保留对话模式 |
| 关联分支/提交 | feature/nextmusic-compose-loop / 未知 |
| 影响模块 | app, ai, ui, docs |
| 关联记录 | `history/ai/20261112-233851-ai-markdown-output.md` |

## 2. 任务目标

- 修复助手消息「任务耗时」展开后无法再次折叠。
- 修复折叠/展开消息时消息区突然跳到最底部的问题。
- 删除 AI 训练模式、输出模式与评分闭环，只保留 AI 对话模式；同步清理主页 AI 页、命令与文档。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/ui/ai_panel.rs` | 修改 | `paint_assistant_header` 每帧把 `CollapsingState` 写回 egui 内存（`store`），收起状态不再丢失；消息滚动区仅在流式生成/等待时 `stick_to_bottom`，折叠或展开历史消息不再被拉到底部；删除输出/训练模式切换块与 `compose` 状态引用，更新文件头说明 |
| `src/app/mod.rs` | 修改 | 删除 `mod compose`、`ComposePanelMode`/`ComposeState` 导出、`compose` 字段与初始化、`poll_compose_loop` 调用与作曲循环重绘请求 |
| `src/app/home.rs` | 修改 | 删除主页「AI」菜单项、图标分支、`HomePage::Ai` 页面分派与 `refresh_learning_stats` 调用 |
| `src/app/state.rs` | 修改 | `HomePage` 移除 `Ai` 变体 |
| `src/ui/mod.rs` | 修改 | 移除 `compose_panel` 模块声明 |
| `src/ai/mod.rs` | 修改 | 移除 `compose` 模块声明 |
| `src/ai/compose/` | 删除 | 作曲循环协议、乐器、乐理、生成、修订、评分、音频分析、训练语料与会话状态机（13 个文件） |
| `src/app/compose.rs` | 删除 | 作曲循环后台线程、离线渲染、结果写回与产物落盘 |
| `src/ui/compose_panel.rs` | 删除 | 输出模式面板与主页 AI 训练页 |
| `doc/项目简介.md` | 修改 | 删除 2.7.1 作曲循环、主页 AI 菜单与训练页、目录树中的 compose 条目、作曲循环限制说明 |
| `doc/技术文档.md` | 修改 | 删除 `app/compose.rs` 文件表、主页 AI 页说明、7.15 作曲循环整节、快速入口与已知限制中的作曲循环条目；补充折叠状态与滚动吸底说明 |
| `doc/关键常量.md` | 修改 | 删除作曲循环、评分、音频分析与训练语料全部常量 |
| `doc/框架工作流.md` | 修改 | 删除 `poll_compose_loop`、主页 AI 页、compose 模块索引与 5.9 调用链 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 任务耗时折叠 | 展开后无法收起 | 每帧写回折叠状态，可正常展开/收起 | 修复交互缺陷 |
| 消息滚动 | 始终吸底 | 仅流式生成/等待时吸底 | 折叠/展开不再跳远 |
| AI 模式 | 对话 / 输出模式，主页 AI 训练页 | 仅对话模式 | 界面与命令减少，行为更单一 |
| 评分与训练反馈 | 七维符号 + 音频评分、`corpus.jsonl` 先验反哺 | 已删除 | 不再后台渲染音频或写训练语料 |
| 主页菜单 | 项目 / 模板 / AI / 插件 / 提示 | 项目 / 模板 / 插件 / 提示 | 菜单项与 `HomePage` 变体减少 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `COMPOSE_SAMPLE_RATE` / `COMPOSE_TAIL_BEATS` / `COMPOSE_LOG_LIMIT` | 22 050 Hz / 1.5 拍 / 240 行 | 已删除 | `src/app/compose.rs` | 作曲循环整体移除 |
| `HARD_MAX_REVISIONS` / `min_accept_score` / `plateau_limit` | 64 版 / 55 / 8 轮 | 已删除 | `src/ai/compose/session.rs` | 训练状态机移除 |
| `FFT_SIZE` / `HOP` / `MAX_ANALYSIS_SECONDS` | 2048 / 1024 / 75 秒 | 已删除 | `src/ai/compose/audio_analysis.rs` | 音频评分移除 |
| `MAX_CORPUS_RECORDS` / `MIN_LEARNING_SCORE` / `EMA_ALPHA` / `MAX_STYLE_FRAGMENTS` | 64 / 55 / 0.35 / 8 | 已删除 | `src/ai/compose/learning.rs` | 训练语料移除 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 文件表、主页、7.14 AI 输出、7.15 删除、快速入口与已知限制 |
| `doc/关键常量.md` | 是 | 删除 AI 作曲循环全部常量 |
| `doc/框架工作流.md` | 是 | 每帧编排、模块地图、文件索引与 5.9 调用链 |
| `doc/项目简介.md` | 是 | 主页、AI 功能、目录树与已知限制 |
| `doc/代码规范.md` | 不涉及 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行 |
| `cargo check` | 通过 | 仅保留 `audio/synth/neural.rs` 既有 `residual` 未读警告 |
| `cargo test` | 通过 | 168 passed / 0 failed / 23 ignored |
| 手工验证 | 未执行 | 未启动 GUI；折叠与滚动的行为修复通过代码路径确认 |

## 8. 风险与后续

- 风险：作曲循环与训练语料目录仍可能存在于用户缓存中，但代码不再读取或写入；历史变更记录保留原样。
- 后续：若要恢复后台生成能力，应在新分支重新设计，不复用已删除的训练/评分状态机。
