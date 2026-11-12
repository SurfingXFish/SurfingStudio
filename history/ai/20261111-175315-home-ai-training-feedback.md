# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-11 17:53:15 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 未标注模型 |
| 任务来源 | 在主页左侧新增 AI 栏，把训练 AI 移到该栏；训练结果与 MIDI 片段写回模型训练以持续提升编曲能力 |
| 关联分支/提交 | feature/nextmusic-compose-loop / 未知 |
| 影响模块 | ai, app, ui, docs |
| 关联记录 | `history/ai/20261111-173119-nextmusic-compose-loop.md` |

## 2. 任务目标

- 主页左侧菜单新增「AI」页，把训练模式从工程窗口 AI 面板移到这里。
- 训练每首完成/中断时，把评分结果与 MIDI 片段写回本地训练语料。
- 从高分工程提取和弦循环、节奏型与动机片段并维护风格先验，让下一次生成实际复用训练结果。
- 在主页 AI 页展示训练语料统计、配置、启停、进度与日志。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/ai/compose/learning.rs` | 新增 | 训练语料库：`TrainingRecord` / `LearningProfile` / `StyleProfile`；`LearningStore::load` / `record` / `save` / `stats`；提取和弦循环、节奏型与动机片段，EMA 更新密度/音区偏置 |
| `src/ai/compose/mod.rs` | 修改 | 声明并导出 `learning` 模块 |
| `src/ai/compose/generator.rs` | 修改 | `ComposeRequest` 增加 `learning` 先验；生成时调整风格密度与旋律音区，复用高分副歌和弦循环、节奏型与动机片段 |
| `src/ai/compose/session.rs` | 修改 | 新增 `with_learning`、`record_learning`；训练每首完成/中断与输出达标时写回语料；下一首生成读取更新后的 `profile` |
| `src/app/compose.rs` | 修改 | 使用 `with_learning` 与 `training_directory`；新增 `refresh_learning_stats` 与面板语料统计；训练结束刷新统计 |
| `src/app/state.rs` | 修改 | `HomePage` 新增 `Ai` |
| `src/app/home.rs` | 修改 | 侧栏新增「AI」菜单与图标；点击时刷新训练语料统计；右侧改绘 `ui_home_ai_page` |
| `src/ui/compose_panel.rs` | 修改 | 新增主页 AI 训练页与语料统计卡片；训练启停、配置、进度与日志 |
| `src/ui/ai_panel.rs` | 修改 | 工程窗口 AI 面板只保留「对话 / 输出模式」，训练模式移到主页 AI 页 |
| `doc/项目简介.md` | 修改 | 主页菜单、2.7.1 训练模式位置、训练语料反哺、目录与已知限制 |
| `doc/技术文档.md` | 修改 | 7.10 主页 AI 页、7.15 训练语料模块与数据流、扩展点与已知限制 |
| `doc/框架工作流.md` | 修改 | 模块地图、主页与作曲循环文件索引、5.9 调用链 |
| `doc/关键常量.md` | 修改 | 训练语料目录、记录上限、最低分、EMA 系数与片段上限 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 训练入口 | 工程窗口 AI 面板的「训练模式」 | 主页左侧「AI」页 | 训练与工程编辑分离，主页可独立运行训练 |
| 训练反馈 | 仅在会话内修订，进程结束后不保留经验 | 每首完成/中断写 `corpus.jsonl`，高分工程更新 `profile.json` | 下一次训练复用和弦循环、节奏型、动机与密度/音区先验 |
| 生成流程 | 纯规则 + seed | 规则 + 训练先验（副歌和弦循环、节奏型、动机、密度/音区） | 训练结果实际改变后续编曲 |
| 训练停止 | 面板「停止」 | 主页 AI 页「停止训练」 | 训练模式仍只有中断才停止 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `MAX_CORPUS_RECORDS` | 无 | `64` 首 | `src/ai/compose/learning.rs` | 语料超出后按分数保留高分工程 |
| `MIN_LEARNING_SCORE` | 无 | `55` | `src/ai/compose/learning.rs` | 低于该分只入语料，不更新先验 |
| `EMA_ALPHA` | 无 | `0.35` | `src/ai/compose/learning.rs` | 新样本权重 |
| `MAX_STYLE_FRAGMENTS` | 无 | `8` 条 | `src/ai/compose/learning.rs` | 每风格每类片段上限 |
| 训练语料目录 | 无 | `ai-compose/training/` | `src/app/compose.rs` | 缓存目录下，含 `corpus.jsonl` / `profile.json` |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 7.10 主页 AI 页、7.15 训练语料与数据流、12.1 扩展点、12.2 已知限制 |
| `doc/关键常量.md` | 是 | 12 AI 编曲新增训练语料常量 |
| `doc/框架工作流.md` | 是 | 2 模块地图、3.2/3.8 文件索引、5.9 调用链 |
| `doc/项目简介.md` | 是 | 主页菜单、2.7.1、目录结构与已知限制 |
| `doc/代码规范.md` | 不涉及 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行 |
| `cargo test` | 通过 | 185 passed / 0 failed / 23 ignored；新增语料落盘/读回与“先验改变生成结果”测试 |
| 手工验证 | 未执行 | 未启动 GUI 点击主页 AI 页；单元测试覆盖语料记录、片段提取、落盘读回与生成差异 |

## 8. 风险与后续

- 风险：训练先验是本地片段与参数偏置，不等同于神经网络权重更新；语料文件会随工程数量增长，已限制为 64 首高分记录；训练仍会持续占用 CPU。
- 后续：可增加训练语料查看/删除与按风格筛选；可把高分片段直接作为模板；可加入更细的节奏/配器先验与可配置的 EMA 系数。
