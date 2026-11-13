# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-13 22:53:03 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 未标注模型 |
| 任务来源 | 主页 Agent 栏管理智能体/skill/技巧/乐器；AI 参数与控制器控制；输入区技能与乐器选择；复制轨道或 MIDI 片段到 AI 输入窗 |
| 关联分支/提交 | feature/ai-agent-hub / 未知 |
| 影响模块 | ai, app, ui, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 在主页新增 Agent 栏（下拉含基础、技能、乐器），可创建、删除、修改编曲智能体（轻音乐、J-Pop），并可设置智能体默认使用的 skill。
- 基础栏管理乐理 skill；技能栏管理编曲技巧；乐器栏管理各乐器（含 VST）的说明、使用场景与技巧。
- 让 AI 能操控内置乐器、内置效果器与 VST 插件的参数，并写入控制器子轨。
- 输入框可选择本次加载的 skill、可使用的乐器与必须使用的乐器。
- 支持把轨道或编曲区 MIDI 片段复制到 AI 交互输入窗。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/ai/library.rs` | 新增 | `AiLibrary` / `AiAgentProfile` / `AiSkillDoc` / `AiInstrumentDoc`：内置智能体、基础技能、技巧与乐器资料；`normalise`、按 id 查询、默认集合、正文加载（技巧最多 6 个）、技巧索引、乐器资料正文、`make_id` / `unique_id` |
| `src/ai/skills.rs` | 修改 | 只保留内嵌 Markdown 常量、14 条技巧原始正文与 `instrument_catalog`；删除 `AiAgentKind`、`technique_catalog`、`load_techniques`、`skill_catalog`、`agent_skills`、`skill_prompt` |
| `src/ai/mod.rs` | 修改 | 声明并导出 `library` 模块；更新文件头 |
| `src/ai/prompt.rs` | 修改 | `system_prompt` 改为资料库 + 智能体 id + `AiPromptSelection` + 工程 + 附件 + 插件 + `AiPluginParamInfo`；新增 `instrument_constraints`、`parameter_context`、`native_instrument_params`、`plugin_param_context`；操作文档加入参数与控制器；更新文件头与测试 |
| `src/ai/plan.rs` | 修改 | 新增 `SetInstrumentParams` / `SetEffectParams` / `SetPluginParams` / `SetController` 操作与 `AiParamSet` / `AiControllerPoint`；新增参数写入、效果器定位、控制器解析、VST 参数写入；`AiApplySummary` 增加 `params_changed` / `controllers_added`；更新文件头与测试 |
| `src/app/ai.rs` | 修改 | `AiChatState.agent` 改为字符串并新增技能/乐器选择、过滤字段；`select_agent`；发送时拼装资料库提示词；`ai_plugin_param_catalog`；`apply_ai_operation_runtime` / `write_plugin_param_now` / `flush_pending_plugin_params`；`attach_track_to_ai` / `attach_clip_to_ai` / `attach_clips_to_ai` 与文本快照辅助函数；更新文件头 |
| `src/app/agent_library.rs` | 新增 | 主页 Agent 四页：列表 + 编辑器、搜索、新建、保存、删除确认、从 VST 生成乐器资料；所有函数带双语函数头 |
| `src/app/state.rs` | 修改 | `HomePage` 增加 Agents / AgentBasics / AgentTechniques / AgentInstruments；新增 `AiLibraryKind`、`AiLibraryUi`、`AiLibraryDraft` |
| `src/app/home.rs` | 修改 | 侧栏增加 Agent 下拉与子项、箭头提示；页面分派与标签/图标；文件头 |
| `src/app/mod.rs` | 修改 | 新增 `agent_library` 模块、`agent_library` 与 `pending_plugin_params` 字段及初始化；每帧同步后重试待写插件参数 |
| `src/app/settings.rs` | 修改 | `AiSettings` 新增 `library: AiLibrary`（serde 默认内置资料） |
| `src/ui/ai_panel.rs` | 修改 | 智能体下拉改为资料库列表；输入区增加「技能 / 可用乐器 / 必须乐器」多选菜单与搜索；改动卡片显示参数调整与控制器点；新增 `toggle_choice`；更新文件头 |
| `src/ui/arrangement/mod.rs` | 修改 | 轨道右键「复制到 AI 输入」；片段右键「复制片段到 AI 输入」 |
| `doc/项目简介.md` | 修改 | 主页 Agent 栏、智能体/技能/乐器资料、输入区选择、参数与控制器、复制轨道/片段 |
| `doc/技术文档.md` | 修改 | 主页 Agent 页、AI 流程、资料库、输入区选择、插件参数上下文、复制引用、内置素材说明 |
| `doc/关键常量.md` | 修改 | 新增资料库正文/数量上限、插件参数清单上限、复制附件上限与 Agent 页布局常量 |
| `doc/框架工作流.md` | 修改 | 模块地图、home/app agent_library/ai/plan/prompt/library/skills 索引、数据流映射 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 智能体来源 | 编译期 `AiAgentKind` 两枚枚举 | `settings.ai.library.agents` 可编辑列表，提示词按 id 解析 | 用户可增删改智能体，切换智能体清空临时技能选择 |
| 技能/配器来源 | 固定 `agent_skills` 全集常驻 | 智能体默认 id + 输入区显式选择，资料库按 id 加载 | 提示词可定制，未知 id 跳过 |
| 编曲操作 | 速度/轨道/片段/效果器/VST 插件 | 追加内置乐器参数、内置效果器参数、VST 参数、控制器子轨 | AI 可调音色与自动化；VST 参数在实例就绪后补写 |
| 提示词上下文 | 工程摘要 + 插件清单 | 追加资料库技能/乐器资料、内置参数、VST 参数清单、可用/必须乐器约束 | 模型能按参数序号操作，并遵守乐器限制 |
| AI 输入区 | 附件 + 智能体/模型 | 追加技能/可用乐器/必须乐器多选菜单 | 选择只影响本次请求 |
| 编曲区右键 | 渲染/组合/合并等 | 追加轨道与片段复制到 AI 输入 | 生成附件并自动打开 AI 面板 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `MAX_LOADED_TECHNIQUES` | 无（旧上限 4） | 6 | `src/ai/library.rs` | 单轮技巧正文加载上限 |
| `MAX_AGENT_SKILLS` | 无 | 12 | `src/ai/library.rs` | 智能体默认基础技能上限 |
| `MAX_SKILL_CHARS` / `MAX_TECHNIQUE_CHARS` / `MAX_INSTRUMENT_CHARS` | 无 | 24000 / 24000 / 16000 字符 | `src/ai/library.rs` | 单条正文进入提示词的上限 |
| `MAX_PLUGIN_SLOTS` / `MAX_PLUGIN_PARAMS` / `MAX_PARAM_TRACKS` | 无 | 12 / 32 / 24 | `src/ai/prompt.rs` | 参数清单上限 |
| `AI_PLUGIN_PARAM_SLOTS` / `AI_PLUGIN_PARAMS_PER_SLOT` | 无 | 12 / 32 | `src/app/ai.rs` | 插件参数快照上限 |
| `AI_REFERENCE_NOTES` | 无 | 256 | `src/app/ai.rs` | 复制轨道/片段附件的音符上限 |
| `AGENT_LIST_W` / `AGENT_GAP` / `AGENT_SEARCH_H` | 无 | 280 / 16 / 26 点 | `src/app/agent_library.rs` | Agent 页布局 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 7.10 主页 Agent 页、7.14 AI 流程/资料库/输入区选择/参数/复制引用、内置素材说明 |
| `doc/关键常量.md` | 是 | 第 12 节 AI 编曲新增资料库、参数清单、复制附件与 Agent 页常量 |
| `doc/框架工作流.md` | 是 | 2 模块地图、3.8 ai 与 app/ui 索引、4 数据流映射 |
| `doc/项目简介.md` | 是 | 2.6 主页界面、2.7 AI 编曲 |
| `doc/代码规范.md` | 不涉及 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行 |
| `cargo test` | 通过 | 171 passed / 0 failed / 23 ignored；新增 library、参数与控制器测试 |
| 手工验证 | 通过 | 启动 GUI 检查：侧栏 Agent 下拉展开/收起、智能体/基础/技能/乐器四页列表与编辑器、Agent 默认技能勾选、乐器页「从 VST 插件…」入口、AI 输入区「技能/可用乐器/必须乐器」菜单与搜索。修复了两处布局问题（列表行被横向排布、首次进入 Agent 自动收起下拉）；未实际发送 AI 请求（本机未配置 Key），也未连接真实 VST 验证参数写回 |

## 8. 风险与后续

- 风险：资料库正文保存在 `settings.json`，文件会明显变大；用户删除内置条目后不会自动恢复（需手工新建或清空 `ai.library` 后重启）。
- 风险：VST 参数按参数列表序号写入；插件升级后参数顺序变化时，已保存的 `PluginRef::params` 可能对应到别的参数。
- 后续：如需导出/导入资料库，可在 Agent 页增加 JSON 导入导出；如需按名称而非序号操作 VST 参数，可在运行时建立名称到序号的映射后再扩展操作协议。
