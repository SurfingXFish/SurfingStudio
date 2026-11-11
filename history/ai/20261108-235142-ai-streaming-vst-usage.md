# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-08 23:51:42 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 思考过程改为实时显示；回复改为流式输出并逐步创作/修改片段；修正 AI 很少使用外部 VST 的问题 |
| 关联分支/提交 | feature/ai-compose-inspector / 未知 |
| 影响模块 | ai, app, ui, docs |
| 关联记录 | `history/ai/20261108-232643-ai-reasoning-jpop-techniques.md` |

## 2. 任务目标

- 思考过程随模型输出实时显示，而不是等整轮结束才出现。
- 回复不再一次性返回：使用 SSE 流式输出，`reply` 逐字显示，编曲操作边解析边写入工程。
- 让 AI 更主动地使用已安装的 VST 乐器与效果器。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/ai/mod.rs` | 修改 | 新增 `AiStreamEvent`（Reasoning / Content）与 `AiRuntime::stream`；导出 `AiOperation`、`PlanStream`；更新模块说明 |
| `src/ai/backend.rs` | 修改 | `AiBackend` 增加 `stream`，默认退化为一次 `complete` 并整体推送，保证旧后端可用 |
| `src/ai/backends/openai_compat.rs` | 修改 | 抽出 `build_payload(request, stream)`；实现 SSE 流式读取：解析 `data:` 行、`delta.reasoning_content` / `delta.content`、`finish_reason`；流式请求用 `timeout_connect` + `timeout_read`；网关拒绝流式时 `fallback_complete` 退回普通补全；新增请求体测试 |
| `src/ai/stream.rs` | 新增 | `PlanStream`：累计流式 JSON，解析顶层 `research` / `skills` / `reply`；只有研究/技巧都为空数组时才放行 `operations`；逐个完整对象返回；补充 5 个单元测试 |
| `src/ai/plan.rs` | 修改 | `AiOperation::apply` 改为公开；新增 `describe()` 生成写入步骤文案；`AiPluginInfo` 增加 `description` 并进入 `prompt_line`；`resolve_plugin` 增加名称规范化（忽略空格/标点/版本后缀）；补名称容错测试 |
| `src/ai/prompt.rs` | 修改 | 输出协议要求字段顺序 `reply → research → skills → operations`；硬性规则新增“音色优先 VST”与“操作分段输出”；插件清单增加说明列、插件总数与优先候选乐器/效果器 |
| `src/app/ai.rs` | 修改 | 新增 `AiWorkerEvent` 与 `AiLiveMessage`；`send_ai_message` 建立事件通道并初始化实时消息；`poll_ai_chat` 排空事件；新增 `apply_ai_operation`（首个操作前压一次撤销、逐步写入、失败停止）与 `finish_ai_task` / `fail_ai_task`；取消时保留已写入部分；`run_ai_worker` 改为流式并补发尾部操作；`ai_plugin_catalog` 用 `plugin::lookup_catalog` 补全厂商/分类/说明 |
| `src/app/mod.rs` | 修改 | 导出 `AiLiveMessage`；AI 等待期间以 33 ms 重绘，保证流式内容实时可见 |
| `src/ui/ai_panel.rs` | 修改 | 新增 `paint_ai_live`：耗时/已写入数量、“思考中…”吸底卡片、逐字 `reply` 与最近 6 条写入步骤；`paint_thinking_block` 支持默认展开与流式吸底；顶部显示已连接 VST 插件数量 |
| `doc/技术文档.md` | 修改 | 模块表、AI 面板、流式流程、逐步写入、VST 优先策略与名称规范化 |
| `doc/关键常量.md` | 修改 | 新增实时思考上限、SSE 流式说明与插件清单说明列 |
| `doc/框架工作流.md` | 修改 | 模块地图、`app/ai.rs` 函数索引、新增 `ai/stream.rs` 索引、5.8 调用链 |
| `doc/项目简介.md` | 修改 | 2.7 增加流式生成、逐步写入与 VST 优先；目录树补充 `ai/stream.rs` |
| `doc/代码规范.md` | 修改 | 13.11 新增流式后端、增量解析与 UI 线程写入约束 |
| `README.md` | 修改 | AI 编曲条目与依赖说明加入 SSE 流式与 VST 优先 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 补全方式 | `AiRuntime::complete`，整段返回 | `AiRuntime::stream`，SSE 增量推送 | 思考与回复实时可见 |
| 思考展示 | 整轮结束后一次性出现 | 逐段追加到 `AiChatState::live` | 生成中即可阅读 |
| 回复展示 | 任务结束后显示完整 `reply` | 流式解析 `reply` 字段，逐字显示并带光标 | 更接近聊天应用 |
| 工程写入 | 全部操作成功后才替换工程副本 | 每个完整操作立即写入，任务开始时压一次撤销 | 片段/轨道逐步出现 |
| 失败处理 | 计划失败不留下任何改动 | 保留已写入部分、停止后续写入并提示可撤销 | 更符合流式语义 |
| 撤销 | 一次任务一步撤销 | 不变（首个操作前 `push_undo_now`） | 行为保持 |
| VST 选择 | 清单只有名称/厂商/分类/格式 | 增加中文用途说明、总数与优先候选 | 模型更愿意、也更准确地选择 VST |
| 插件名称 | 精确/厂商/子串匹配 | 先规范化（去掉空格、标点、版本后缀）再匹配 | `Serum (64 bit)`、`Pro-Q 4 VST3` 也能命中 |
| 面板提示 | 无插件使用提示 | 顶部显示已连接插件数量 | 用户可确认 AI 确实有 VST 可用 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 流式思考显示上限 `AI_LIVE_REASONING_LIMIT` | 无 | `96` KB | `src/app/ai.rs` | 流式阶段停止追加的字节上限 |
| 流式请求超时 | 整体超时 `timeout` | `timeout_connect` + `timeout_read` | `src/ai/backends/openai_compat.rs` | 持续有数据不会被整体超时截断 |
| 插件清单列 | 角色/名称/厂商/分类/格式/MIDI | 增加“说明” | `src/ai/plan.rs` | 说明来自离线资料库 |
| 提示词字段顺序 | 未约定 | `reply → research → skills → operations` | `src/ai/prompt.rs` | 保证流式阶段能先确认研究/技巧为空 |
| 其余常量 | — | 不变 | — | `AI_INPUT_H` 176、技巧 14 个、单轮加载 4 个等保持 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 模块表、7.14 面板与 AI 流程、VST 流程 |
| `doc/关键常量.md` | 是 | 第 12 节 AI 编曲 |
| `doc/框架工作流.md` | 是 | 模块地图、`app/ai.rs` 与 `ai/stream.rs` 索引、5.8 调用链 |
| `doc/项目简介.md` | 是 | 2.7 AI 编曲与目录树 |
| `doc/代码规范.md` | 是 | 13.11 流式与增量写入约束 |
| `README.md` | 是 | AI 编曲条目与技术栈 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 已执行 |
| `cargo check --all-targets` | 通过 | 仅保留仓库原有警告（`neural.rs` dead_code、`plugin/mod.rs` unused_mut、`shortcuts.rs` unused_must_use） |
| `cargo clippy --all-targets` | 未通过（非本次改动） | 被既有 `src/audio/synth/neural.rs` 的 `approx_constant` 拒绝级 lint 阻断，与本次 AI 改动无关 |
| `cargo test --all-targets` | 通过 | 166 passed / 0 failed / 23 ignored |
| 冒烟启动 | 通过 | 启动 `target/debug/surfing-studio.exe`，8 秒后窗口 `SurfingStudio` 正常响应 |
| 真实 API 验证 | 未执行 | 未使用真实 DeepSeek / MiMo Key 验证 SSE 增量与思考实时性 |

## 8. 风险与后续

- 风险：部分网关可能拒绝 `stream` 或 `stream + response_format` 组合，此时自动回退普通补全，流式效果退化为一次性；SSE 读取按行解析，若服务商返回非标准事件格式会被跳过。增量写入意味着中途失败会保留已写入部分（可用 Ctrl+Z 撤销），与旧版“全有或全无”不同。
- 风险：VST 使用率仍取决于模型是否遵守“音色优先 VST”规则；插件角色被扫描为“未知”时模型可能不确定用途，需继续完善离线资料库。
- 后续：可在实时卡片中展示每轮耗时与 token 用量；可为流式操作增加“暂停写入”按钮；可把插件清单做成按角色分组的摘要，进一步降低提示词长度。
