# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-11 17:31:19 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 未标注模型 |
| 任务来源 | 将 NextMusic 的 MIDI 编曲与自训练循环合并进 SurfingStudio，提供输出模式与训练模式 |
| 关联分支/提交 | feature/nextmusic-compose-loop / 未知 |
| 影响模块 | ai, app, ui, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 在 AI 面板提供输出模式：提示词 + 可用乐器 → MIDI 工程 JSON → DAW 渲染 → 评分 → 修订，直到目标分。
- 提供训练模式：持续生成、渲染、评分、修订；达到目标分只发出新建空项目指令并继续，只有中断才停止。
- 只使用内置采样音色与鼓机，保证后台线程渲染不实例化 VST。
- 复用现有 AI 面板、撤销、工程替换与缓存目录，不新增外部运行时依赖。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/ai/compose/mod.rs` | 新增 | 作曲循环模块入口、导出项与分层说明 |
| `src/ai/compose/random.rs` | 新增 | splitmix64 确定性随机数，保证同 seed 可复现 |
| `src/ai/compose/schema.rs` | 新增 | `surfingstudio.midi.v1` 工程 JSON、调性/段落/和弦/轨道/音符、校验与摘要 |
| `src/ai/compose/theory.rs` | 新增 | 音阶/调式、级数和弦、声部连接、音级移动、调性解析 |
| `src/ai/compose/instruments.rs` | 新增 | 24 件乐器目录、中文别名、角色折叠、通道分配与内置 bank id 映射 |
| `src/ai/compose/generator.rs` | 新增 | 提示词解析、段落表、和声、旋律、和弦、低音、琶音与鼓组生成 |
| `src/ai/compose/revise.rs` | 新增 | 按最弱维度执行量化、和弦音吸附、平滑大跳、动机重复、经过音、根音校正、鼓 fill、段落对比与力度平衡 |
| `src/ai/compose/scoring.rs` | 新增 | 七维符号评分、音频/符号融合、最弱/最强维度与解释 |
| `src/ai/compose/audio_analysis.rs` | 新增 | FFT、色度、起音包络、频段能量与动态统计；最多分析 75 秒高能量片段 |
| `src/ai/compose/midi_io.rs` | 新增 | 标准 MIDI File Format 1 导出 |
| `src/ai/compose/convert.rs` | 新增 | MIDI 工程 JSON → `core::model::Project`；鼓组走内置鼓机，其它走采样音色 |
| `src/ai/compose/session.rs` | 新增 | 输出/训练状态机、事件协议、`begin` / `ingest_render` / `interrupt`、逐首重置最佳版本 |
| `src/app/compose.rs` | 新增 | 后台线程、`Engine::render_offline_options` 渲染、评分、事件/JSON/MIDI/WAV 产物、结果写回 |
| `src/ui/compose_panel.rs` | 新增 | 提示词、乐器多选、小节/目标分/音频权重、进度、评分与日志 |
| `src/ai/mod.rs` | 修改 | 声明 `pub mod compose` |
| `src/app/mod.rs` | 修改 | 声明 `mod compose`、导出 `ComposeState` / `ComposePanelMode`、新增 `compose` 字段、每帧 `poll_compose_loop` 与重绘请求 |
| `src/app/ai.rs` | 修改 | `cleanup_after_ai_write` 提升为 `pub(super)`，供作曲循环写回后复用清理逻辑 |
| `src/ui/mod.rs` | 修改 | 声明 `compose_panel` 子模块 |
| `src/ui/ai_panel.rs` | 修改 | 增加「对话 / 输出模式 / 训练模式」切换，非对话模式委托 `ui_compose_panel` |
| `doc/项目简介.md` | 修改 | 新增 2.7.1 作曲循环、目录结构与已知限制 |
| `doc/技术文档.md` | 修改 | 新增 7.15 作曲循环架构、数据流与约束；扩展点与已知限制 |
| `doc/框架工作流.md` | 修改 | 模块地图、3.8 文件索引、快速入口与 5.9 调用链 |
| `doc/关键常量.md` | 修改 | 新增作曲循环、评分与面板常量 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| AI 面板 | 只有对话编曲 | 顶部增加输出/训练模式切换，共用右侧面板 | 用户可在同一位置启动循环 |
| 生成 → 试听 | 模型直接写音符，用户自行播放 | 后台生成 MIDI JSON，DAW 引擎离线渲染为 PCM，再交给评分 | 形成可自动迭代的闭环 |
| 训练停止 | 不存在 | 达到目标分发 `new_project` 并继续；只有停止/中断才结束 | 符合“直到高分、只有中断才停”的要求 |
| 工程写回 | AI 对话逐个操作写入 | 输出模式完成时整体替换工程，压入一步撤销 | 可 `Ctrl+Z` 回退 |
| 后台渲染 | 导出/轨道渲染在 UI 线程逐帧推进 | 作曲循环在线程内调用静态离线渲染，仅内置音色 | 不阻塞 UI，也不触发 VST 线程限制 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `COMPOSE_SAMPLE_RATE` | 无 | `22_050` Hz | `src/app/compose.rs` | 循环渲染与分析采样率 |
| `COMPOSE_TAIL_BEATS` | 无 | `1.5` 拍 | `src/app/compose.rs` | 渲染尾音 |
| `COMPOSE_LOG_LIMIT` | 无 | `240` 行 | `src/app/compose.rs` | 面板日志上限 |
| `HARD_MAX_REVISIONS` | 无 | `64` 版 | `src/ai/compose/session.rs` | 单工程安全上限 |
| `AUDIO_WEIGHT` | 无 | `0.5` | `src/ai/compose/scoring.rs` | 音频/符号默认权重 |
| `FFT_SIZE` / `HOP` | 无 | `2048` / `1024` | `src/ai/compose/audio_analysis.rs` | 音频分析窗口与帧移 |
| `MAX_ANALYSIS_SECONDS` | 无 | `75` 秒 | `src/ai/compose/audio_analysis.rs` | 高能量片段上限 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 模块职责、7.15 作曲循环、12.1 扩展点、12.2 已知限制 |
| `doc/关键常量.md` | 是 | 12 AI 编曲新增循环与评分常量 |
| `doc/框架工作流.md` | 是 | 1.1 每帧编排、2 模块地图、3.8 文件索引、4 快速入口、5.9 调用链 |
| `doc/项目简介.md` | 是 | 2.7.1 作曲循环、目录结构、已知限制 |
| `doc/代码规范.md` | 不涉及 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行，无格式差异 |
| `cargo test` | 通过 | 183 passed / 0 failed / 23 ignored；含作曲循环渲染→评分集成测试与输出/训练状态转移测试 |
| 手工验证 | 未执行 | 未启动 GUI 进行真实点击与试听；单元测试覆盖生成、转换、渲染、评分与事件文本 |

## 8. 风险与后续

- 风险：训练模式不设总时长上限，会持续占用一个 CPU 核心；音频评分是轻量近似，不等同于人工审美判断；输出模式在目标分不可达时需手动停止。
- 后续：可增加“导入外部渲染音频反馈”入口；可为长训练增加每首时长上限与后台低优先级调度；可把作曲循环结果直接保存为工程文件。
