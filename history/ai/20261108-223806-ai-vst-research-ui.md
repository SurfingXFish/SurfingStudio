# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-08 22:38:06 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 按图片重做 AI 对话框；支持 AI 调用第三方 VST 插件并联网检索插件特点；扩充现代乐理（桥段等） |
| 关联分支/提交 | feature/ai-compose-inspector / 未知 |
| 影响模块 | ai, app, ui, docs |
| 关联记录 | `history/ai/20261108-220553-ai-compose-inspector.md` |

## 2. 任务目标

- AI 对话面板改成图片中的聊天式布局：圆角输入区、底部模型/智能体/附件/发送按钮、快捷指令与“工程已改动”卡片。
- 允许 AI 读取已安装 VST3 / VST2 插件，并把插件写入轨道（乐器与效果器）。
- 允许 AI 在不确定插件特点时联网检索，再生成最终编曲计划。
- 扩充乐理技能：现代曲式、桥段、和声扩展、Groove、Hook、能量曲线等。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/ai/plan.rs` | 修改 | 新增 `AiPluginInfo` / `AiPluginKind` / `resolve_plugin` / `plugin_ref`；新增 `research` 字段与 `add_plugin_track` / `set_plugin_instrument` / `add_plugin_effect`；`apply` 接收插件清单；摘要新增 `plugins_added` |
| `src/ai/prompt.rs` | 修改 | 输出协议加入 `research`；加入 VST 操作说明、插件清单段落与 `plugin_catalog`；工程快照加入轨道效果器；系统提示词接收插件清单 |
| `src/ai/search.rs` | 新增 | DeepSeek Search API 联网检索：`AiSearchConfig`、`search`、`search_endpoint`、`parse_results` |
| `src/ai/mod.rs` | 修改 | 声明并导出 `search` / `AiSearchConfig` / `AiPluginInfo` / `AiPluginKind`；更新模块说明 |
| `src/ai/skills.rs` | 修改 | 新增 `MODERN_ARRANGEMENT` 技能并加入智能体；更新技能数量测试 |
| `src/ai/skills/modern-arrangement.md` | 新增 | 现代段落表、能量曲线、桥段、现代和声、Groove、Hook、频段分层、过渡与风格配方 |
| `src/ai/skills/music-theory.md` | 修改 | 新增调式互换、副属、三全音替代、重配和声、半音中音、线式手法、负和声、Groove、Hook/段落功能与桥段 |
| `src/app/ai.rs` | 修改 | 新增 `ai_plugin_catalog`、`run_ai_worker`（补全 → 检索 → 补全，最多 3 轮）；`AiChatMessage` 增加 `changes`；`handle_ai_content` 支持检索提示与结构化摘要；`apply_ai_plan` 传入插件清单并清理插件缓存 |
| `src/app/settings.rs` | 修改 | `AiSettings` 新增 `search_enabled`（默认开启）；AI 设置页新增“联网搜索”开关与说明；补测试 |
| `src/ui/ai_panel.rs` | 修改 | 聊天式布局重做：消息气泡、工程改动卡片、快捷指令、圆角输入区、`+` 附件、模型/智能体下拉与圆形发送/停止 |
| `doc/技术文档.md` | 修改 | 7.10 AI 页、7.14 面板/流程、VST 流程、联网检索、技能清单与模块表 |
| `doc/关键常量.md` | 修改 | 新增插件清单上限、检索查询/摘要上限、`search_enabled`、`AI_INPUT_H` 176 等 |
| `doc/框架工作流.md` | 修改 | 模块地图、文件索引、5.8 调用链与快速入口加入 VST/检索/改动卡片 |
| `doc/项目简介.md` | 修改 | 2.7 AI 编曲加入聊天界面、VST、联网检索与扩充技能；更新已知限制 |
| `doc/代码规范.md` | 修改 | 13.11 增加联网检索约束 |
| `README.md` | 修改 | AI 编曲功能说明加入 VST 与 DeepSeek Search |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| AI 后台任务 | 单次 `AiRuntime::complete` | `run_ai_worker`：首轮 → 按 `research` 检索 → 再次补全，最多 3 轮 | 可先查插件资料再编曲 |
| AI 计划 | 内置乐器与内置效果器 | 增加 VST 乐器/效果器操作 | AI 可使用第三方插件 |
| 插件清单 | 不进入提示词 | 从 `plugin_cache` 生成 `AiPluginInfo`（不含路径） | 模型可选择已安装插件 |
| AI 面板输入区 | 文本框 + 下方独立下拉行 | 圆角输入区，底部 `+` / 智能体 / 模型 / 圆形发送 | 接近聊天应用 |
| AI 写入反馈 | 纯文本“已写入…” | 助手气泡 + “工程已更新”卡片 | 改动信息更清晰 |
| 设置 → AI | 模型、密钥、接口、采样 | 增加“联网搜索”开关 | 可控制是否检索 |
| 技能 | 乐理、轻音乐、八类配器 | 增加“现代编曲与曲式”，乐理补充现代和声 | 桥段等知识进入必选上下文 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `AI_INPUT_H` | `168` | `176` | `src/ui/ai_panel.rs` | 新输入区高度 |
| 提示词插件清单上限 | 无 | `300` | `src/ai/prompt.rs` | 超出省略 |
| 联网检索查询上限 | 无 | `3` | `src/ai/search.rs` | 每条 count 5 |
| 联网检索摘要上限 | 无 | `900` 字符/条 | `src/ai/search.rs` | 控制上下文 |
| `ai.search_enabled` | 无 | 默认 `true` | `src/app/settings.rs` | 需 DeepSeek Key 才实际检索 |
| `MAX_POSITION_BEATS` | `100_000` | 不变 | `src/ai/plan.rs` | 仍作为片段/音符上限 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 模块表、7.10 AI 页、7.14 面板/VST/检索/技能 |
| `doc/关键常量.md` | 是 | 第 12 节 AI 编曲 |
| `doc/框架工作流.md` | 是 | 模块地图、3.4 / 3.8 文件索引、5.8 调用链、快速入口 |
| `doc/项目简介.md` | 是 | 2.7 AI 编曲与已知限制 |
| `doc/代码规范.md` | 是 | 13.11 联网检索约束 |
| `README.md` | 是 | AI 编曲功能列表 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行 |
| `cargo test` | 通过 | 157 passed / 0 failed / 23 ignored；AI 模块 24 个测试 |
| 手工验证 | 部分执行 | 已启动应用做冒烟检查，窗口正常出现且响应；未连接真实 DeepSeek / MiMo API Key，未逐项点击新输入区与 VST 插件加载 |

## 8. 风险与后续

- 风险：VST 只能按名称选择并设置工厂程序号，不能自动编辑插件内部参数；插件重名时需要模型写“厂商 名称”。联网检索依赖 DeepSeek Key 且按次计费；未用真实 Key 验证 Search API 的响应格式。模型可能返回不符合计划的 JSON，此时不写工程。
- 后续：可增加插件参数读取与 `set_plugin_param`，让 AI 调整音色；可在面板中显示检索来源链接；可为更多智能体（流行、管弦、中国风）复用现代编曲技能。
