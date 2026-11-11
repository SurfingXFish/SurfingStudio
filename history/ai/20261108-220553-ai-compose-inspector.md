# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-08 22:05:53 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 轨道属性面板可关闭并由双击轨道打开；接入 DeepSeek / MiMo 等 AI 模型实现 AI 编曲，内置轻音乐、配器与乐理技能 |
| 关联分支/提交 | feature/ai-compose-inspector / 未知 |
| 影响模块 | core, ai, app, ui, platform, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 轨道属性面板可关闭；右上角“删除轨道”改为“关闭”；双击轨道列表中的轨道打开轨道属性。
- 顶部新增“AI 编曲”入口，在右侧打开对话面板，输入区可选择模型、智能体与附件。
- 接入 DeepSeek、小米 MiMo 与自定义 OpenAI 兼容服务，把 AI 返回的编曲计划直接写入工程。
- 内置轻音乐风格、基础乐理与八类配器技能（Markdown），随智能体一起加载。
- 按协作规范从 `test` 拉出功能分支，并同步技术文档、关键常量、框架工作流、项目简介与代码规范。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `Cargo.toml` | 修改 | 新增 `ureq`（阻塞 HTTPS + rustls/ring），用于 AI 接口请求 |
| `Cargo.lock` | 修改 | 记录 ureq / rustls / ring / webpki-roots 等依赖 |
| `src/lib.rs` | 修改 | 声明 `ai` 模块，并在层说明中加入 AI |
| `src/ai/mod.rs` | 新增 | AI 运行时 `AiRuntime`、`AiBackend` 分派、服务商、模型预设、消息/请求/响应 |
| `src/ai/backend.rs` | 新增 | `AiBackend` 接口与 `AiBackendInfo` |
| `src/ai/backends/mod.rs` | 新增 | 标准后端注册 |
| `src/ai/backends/openai_compat.rs` | 新增 | OpenAI 兼容 HTTPS 请求、鉴权头、超时、响应/错误解析；DeepSeek / MiMo 参数差异 |
| `src/ai/plan.rs` | 新增 | JSON 编曲计划、操作执行、音符数组、音名/MIDI 转换、乐器与效果器解析 |
| `src/ai/prompt.rs` | 新增 | 系统提示词、工程快照、附件上下文 |
| `src/ai/skills.rs` | 新增 | 智能体定义、内嵌技能、乐器清单 |
| `src/ai/skills/music-theory.md` | 新增 | 基础乐理技能 |
| `src/ai/skills/light-music.md` | 新增 | 轻音乐风格技能 |
| `src/ai/skills/instruments/piano.md` | 新增 | 钢琴与键盘配器 |
| `src/ai/skills/instruments/guitar-bass.md` | 新增 | 吉他、贝斯与拨弦配器 |
| `src/ai/skills/instruments/strings.md` | 新增 | 弦乐组配器 |
| `src/ai/skills/instruments/woodwinds.md` | 新增 | 木管组配器 |
| `src/ai/skills/instruments/brass.md` | 新增 | 铜管组配器 |
| `src/ai/skills/instruments/percussion.md` | 新增 | 鼓组与打击乐配器 |
| `src/ai/skills/instruments/synth-electronic.md` | 新增 | 合成器与电子音色配器 |
| `src/ai/skills/instruments/folk-world.md` | 新增 | 民乐与世界乐器配器 |
| `src/ai/midi_file.rs` | 新增 | MIDI 附件解析（SMF 0/1/2、VLQ、运行状态、速度/拍号、音符配对） |
| `src/app/ai.rs` | 新增 | 对话状态、发送、后台补全、计划原子写入、取消、附件选择与读取 |
| `src/app/mod.rs` | 修改 | 新增 `side_panel` / `ai_chat` / 附件通道；每帧轮询 AI；右侧面板按 `SidePanel` 切换 |
| `src/app/settings.rs` | 修改 | 新增 `AiSettings`、设置分类 `AI`、AI 设置页（模型、三组密钥、自定义接口、采样参数） |
| `src/app/state.rs` | 修改 | 新增 `SidePanel` 枚举 |
| `src/app/history.rs` | 修改 | `after_history_apply` 提升为 `pub(super)`，供 AI 写入后复用清理流程 |
| `src/app/panels.rs` | 修改 | 工具栏新增“AI 编曲”按钮 |
| `src/ui/ai_panel.rs` | 新增 | AI 对话面板：消息、密钥提示、附件、输入、模型/智能体/发送/停止 |
| `src/ui/inspector.rs` | 修改 | 右上角“删除轨道”改为“关闭”；删除按钮与逻辑移除 |
| `src/ui/arrangement/mod.rs` | 修改 | 双击轨道主行打开轨道属性面板，不再直接打开卷帘 |
| `src/ui/mod.rs` | 修改 | 声明 `ai_panel` 子模块 |
| `platforms/android/AndroidManifest.xml` | 修改 | 声明 `android.permission.INTERNET` |
| `doc/技术文档.md` | 修改 | 新增 `ai` 层、模块职责、7.14 面板与 AI 流程、设置九类说明 |
| `doc/关键常量.md` | 修改 | 右侧面板宽度、设置九类；新增第 12 节 AI 编曲常量 |
| `doc/框架工作流.md` | 修改 | 模块地图、文件索引、AI 调用链 5.8、快速入口 |
| `doc/项目简介.md` | 修改 | 轨道属性交互、新增 2.7 AI 编曲、设置九类、目录结构、技术栈、已知限制 |
| `doc/代码规范.md` | 修改 | 新增 `ai` 层、依赖规则、模块映射与 13.11 扩展手册 |
| `README.md` | 修改 | 功能列表与目录结构加入 AI 编曲，技术栈加入 ureq |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 右侧面板 | 固定显示检查器 | `side_panel: Option<SidePanel>` 在轨道属性 / AI 对话 / 收起之间切换 | 编曲区可用宽度随面板关闭而增加 |
| 双击轨道主行 | 普通轨道打开卷帘，组合轨道打开音量卷帘 | 打开轨道属性面板 | 卷帘改由双击片段或子轨打开 |
| 轨道属性右上角 | “删除轨道”立即删除 | “关闭”收起面板 | 删除仍走右键与 Delete/Backspace |
| AI 请求 | 无 | `send_ai_message` 后台线程 → `AiRuntime::complete` → `poll_ai_chat` | 网络不阻塞 UI 与音频回调 |
| AI 写入 | 无 | `AiPlan::apply` 在工程副本上执行，成功后替换工程并记一步撤销 | 失败不留半成品 |
| 设置分类 | 八类 | 九类（新增 AI） | 模型、密钥与采样参数可配置 |
| 安卓权限 | 无网络权限 | 声明 INTERNET | AI 接口可在安卓使用 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 右侧面板宽度 | 默认 `340` / `300~460` | 默认 `360` / `300~520` | `src/app/mod.rs` | 轨道属性与 AI 对话共用 |
| `DEFAULT_AI_MODEL_PRESET` | 无 | `deepseek-flash` | `src/app/settings.rs` | 默认模型预设 |
| `DEFAULT_AI_TEMPERATURE` | 无 | `0.7` | `src/app/settings.rs` | 采样温度 |
| `DEFAULT_AI_MAX_TOKENS` | 无 | `16_384` | `src/app/settings.rs` | 单次最大输出 |
| `DEFAULT_AI_TIMEOUT_SECONDS` | 无 | `180` 秒 | `src/app/settings.rs` | 请求超时 |
| `AI_HISTORY_MESSAGES` | 无 | `12` 条 | `src/app/ai.rs` | 发送给模型的历史消息上限 |
| `NOTES_TOTAL` / `NOTES_PER_CLIP` | 无 | `720` / `96` | `src/ai/prompt.rs` | 工程快照音符上限 |
| 附件提示词上限 | 无 | `12_000` 字符 | `src/ai/prompt.rs` | 单附件截断 |
| `AI_TEXT_ATTACHMENT_LIMIT` | 无 | `512` KB | `src/app/ai.rs` | 文本附件读取上限 |
| `AI_MIDI_ATTACHMENT_LIMIT` | 无 | `4` MB | `src/app/ai.rs` | MIDI 附件读取上限 |
| MIDI 附件音符上限 | 无 | 导出 `600` / 解析 `20_000` | `src/ai/midi_file.rs` | 控制上下文与异常文件 |
| `MAX_POSITION_BEATS` | 无 | `100_000` 拍 | `src/ai/plan.rs` | 片段/音符位置上限 |
| `AI_INPUT_H` | 无 | `168` 点 | `src/ui/ai_panel.rs` | AI 输入区预留高度 |
| 设置分类数 | `8` | `9` | `src/app/settings.rs` | 新增 AI 页 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.1 / 1.2 层与模块职责、7.1 每帧绘制、7.10 设置九类、新增 7.14 面板与 AI 流程 |
| `doc/关键常量.md` | 是 | 第 5 节右侧面板宽度、第 8 节设置分类、新增第 12 节 AI 编曲 |
| `doc/框架工作流.md` | 是 | 第 2 节模块地图、第 3.4 / 3.5 / 3.8 文件索引、第 4 节快速入口、新增 5.8 AI 调用链 |
| `doc/项目简介.md` | 是 | 2.1 轨道属性、新增 2.7 AI 编曲、设置九类、目录结构、技术栈、已知限制 |
| `doc/代码规范.md` | 是 | 1.1 / 1.2 / 1.3 / 1.5 增加 `ai` 层与依赖规则、13.11 新增 AI 后端手册 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行，无格式差异残留 |
| `cargo test` | 通过 | 152 passed / 0 failed / 23 ignored；包含 18 个 AI 单元测试与 1 个 AI 设置序列化测试 |
| 手工验证 | 部分执行 | 已启动应用做冒烟检查：工程窗口正常出现且响应（`Responding=True`）；未连接真实 DeepSeek / MiMo API Key，未逐项点击 AI 面板与设置页 |

## 8. 风险与后续

- 风险：不同模型对 `response_format = json_object` 与 `max_completion_tokens` 的兼容性可能不同；模型可能返回不符合计划的 JSON，此时只在对话中显示原文、不写工程。API Key 以明文保存在本机 `settings.json`。安卓 HTTPS 与 INTERNET 权限尚未在真机验证。
- 后续：可在 AI 面板增加“重新生成/继续发展”快捷指令、音轨试听与计划预览；可补充更多智能体（流行、管弦、中国风）与更多乐器的细分技能；可在发布前用真实密钥做一次端到端验证。
