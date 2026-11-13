# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-13 23:35:03 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 未标注模型 |
| 任务来源 | 根据运行截图与 .sflog 修复 Agent 下拉、资料分类、输入区交互、剪贴板粘贴、关闭卡死与 AI 参数/控制器写入 |
| 关联分支/提交 | feature/ai-agent-hub / 未知 |
| 影响模块 | ai, app, ui, docs |
| 关联记录 | `history/ai/20261113-225303-ai-agent-library-params.md` |

## 2. 任务目标

- 修复 Agent 下拉符号显示为方块，默认收起下拉。
- 轻音乐 / J-Pop 风格从基础技能移入智能体的创作准则。
- 修复列表小字说明溢出卡片。
- 乐器资料改为单件乐器一条 skill，不再多件混用；删除「从 VST 插件…」，改为条目内下拉选择或自定义名称。
- 智能体默认 skill 未自定义时自动在输入区开启；选项改为开关并增加全选。
- 技能/乐器选择菜单点击内部不关闭，只在点击外部时关闭。
- 支持把轨道或 MIDI 片段粘贴到 AI 输入框，不再只粘贴占位文字。
- 修复关闭工程窗口卡死。
- 强化 AI 修改 VST 参数与使用控制器的提示词和上下文。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/ai/library.rs` | 修改 | 新增 `version`（`CURRENT_LIBRARY_VERSION = 3`）与 `migrate`：旧版基础中的轻音乐/J-Pop 风格移入智能体 `style`，旧八条分组配器资料替换为单件乐器；`seeded` 基础技能只保留乐理与现代编曲，智能体默认技能只含这两条；`default_instrument_docs` 为合成器、鼓机与 128 件采样乐器各生成一条资料；新增 `instrument_tips`；测试改为单件乐器并新增迁移测试 |
| `src/app/settings.rs` | 修改 | 加载设置时调用 `AiLibrary::migrate` 而非仅 `normalise` |
| `src/app/state.rs` | 修改 | `Clipboard` 新增 `Track(Track)`；`AiLibraryUi` 默认收起、新增 `instrument_pick_filter`；`AiChatState` 相关字段在 `app/ai.rs` |
| `src/app/home.rs` | 修改 | Agent 下拉符号改为画线折线，不再依赖 `▾` 字形；从其他页面进入 Agent 时自动展开，已在 Agent 栏时点击才切换 |
| `src/app/agent_library.rs` | 修改 | 删除顶部「从 VST 插件…」；乐器编辑器新增「选择乐器…」下拉（内置 + VST，带搜索）并回填名称/别称/分类/音域/说明/场景/技巧；列表行增加裁剪矩形与字符截断，修复小字溢出；删除 `begin_instrument_draft_from_plugin` |
| `src/app/ai.rs` | 修改 | `AiChatState` 新增 `skills_customized`；`select_agent`/清空/切换对话重置；`AiPromptSelection` 传入自定义标记；新增 `attach_clipboard_to_ai` 与 `ai_clip_only_reference`；`copy_selection` 无片段时复制整轨 |
| `src/app/edit.rs` | 修改 | 无片段/音符/控制器可复制时 `Ctrl+C` 写入 `Clipboard::Track`；`paste_clipboard` 对整轨副本提示粘贴到 AI 输入框 |
| `src/ai/prompt.rs` | 修改 | `AiPromptSelection` 新增 `skills_customized`；未自定义时使用智能体默认技能，自定义后只使用显式选择；新增「参数与控制器操作要求」强制实际输出 `set_plugin_params` / `set_controller`；项目上下文列出控制器子轨点数 |
| `src/ui/ai_panel.rs` | 修改 | 菜单改为 `Popup::menu` + `CloseOnClickOutside`；选项改为开关行，新增「全选/清空」；默认技能按 `skills_customized` 显示；输入框固定 id 并拦截 `Ctrl+V` / `Event::Paste` 生成 AI 引用；新增 `compact_menu_button` / `choice_toggle` / `effective_skill_checked` / `ensure_skill_selection` |
| `src/ui/arrangement/mod.rs` | 修改 | 轨道右键新增「复制轨道」 |
| `src/app/mod.rs` | 修改 | `close_plugin_owned_windows` 只投递 `WM_CLOSE` 并立即返回，不再在 `Drop` 中同步 `DispatchMessageW`，避免 VOCALOID6 WPF 关闭阻塞工程窗口 |
| `doc/项目简介.md` | 修改 | Agent 基础/乐器/输入区/复制到 AI 输入的说明 |
| `doc/技术文档.md` | 修改 | Agent 页选择器、资料库迁移与单件乐器、输入区开关/全选/外部关闭、剪贴板粘贴、关闭插件窗口不泵消息 |
| `doc/关键常量.md` | 修改 | 资料库版本 3、默认条目 2/2/14/130 |
| `doc/框架工作流.md` | 修改 | Agent 资料库、library 索引、Clipboard/复制、AI 输入区与关闭流程 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| Agent 下拉 | 默认展开、`▾` 字形缺字显示方块 | 默认收起；折线绘制；进入 Agent 自动展开 | 显示稳定，交互更符合预期 |
| 风格技能 | 轻音乐/J-Pop 在基础技能中，智能体 style 只有短句 | 完整风格正文进入智能体 `style`，基础只保留乐理与曲式 | 旧资料库加载时自动迁移 |
| 乐器资料 | 8 条分组技能（多乐器混用） | 合成器 + 鼓机 + 128 件采样乐器各一条 | 提示词按单件乐器加载；旧分组自动替换 |
| 乐器名称 | 顶部按钮从 VST 生成草稿 | 条目编辑器内下拉选择内置/VST 或自定义 | 创建与编辑路径统一 |
| 技能选择 | 复选菜单，默认显示未勾选 | 开关 + 全选，默认显示智能体默认技能，首次修改时复制默认值 | 默认与发送一致；点击内部不关闭 |
| 剪贴板 | 只能右键复制到 AI，输入框 Ctrl+V 粘贴占位文字 | 轨道右键/ Ctrl+C 复制整轨，输入框 Ctrl+V 转成附件 | 不再出现「SurfingStudio 剪贴板」文字 |
| 工程关闭 | 插件窗口 `WM_CLOSE` 后同步泵消息 | 只投递关闭请求，主循环后续处理 | 避免 WPF 插件关闭卡死 |
| AI 参数/控制器 | 提示词只列参数，未强制使用 | 新增强制段落与示例，项目上下文包含控制器状态 | 提高模型实际写参数/控制器的概率 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `CURRENT_LIBRARY_VERSION` | 无 | 3 | `src/ai/library.rs` | 资料库结构版本与迁移 |
| 内置资料库默认条目 | 2 智能体 / 4 基础 / 14 技巧 / 8 乐器 | 2 / 2 / 14 / 130 | `src/ai/library.rs` | 乐器按单件拆分 |
| `AI_REFERENCE_NOTES` | 256 | 256 | `src/app/ai.rs` | 未变化，剪贴板粘贴复用 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.6 插件窗口关闭、7.10 Agent 页、7.14 资料库/输入区/剪贴板 |
| `doc/关键常量.md` | 是 | 第 12 节资料库版本与条目数 |
| `doc/框架工作流.md` | 是 | 模块地图、app/ai/library/ui/state、关闭流程 |
| `doc/项目简介.md` | 是 | 2.7 AI 编曲的 Agent/乐器/输入区/复制说明 |
| `doc/代码规范.md` | 不涉及 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行 |
| `cargo test` | 通过 | 172 passed / 0 failed / 23 ignored；新增资料库迁移测试 |
| 手工验证 | 通过 | GUI 检查：Agent 下拉折线、默认收起与进入展开；基础仅两条；乐器为单件（合成器/鼓机/大钢琴等）；编辑器乐器选择入口；输入区技能默认显示 2 项、开关可切换、菜单保持打开、全选按钮；复制整轨后在输入框 Ctrl+V 生成「轨道：MIDI 轨道 1」附件。未复现 VOCALOID 关闭场景（需加载原工程），但已按日志定位并移除阻塞点 |
| `.sflog` 分析 | 通过 | 解码 `surfing-20261113-230326-p28044.sflog`：关闭卡在 `window.drop phase=plugin_windows_visible titles=VOCALOID6 Editor` 后的 `DispatchMessageW` |

## 8. 风险与后续

- 风险：资料库迁移会删除旧版 8 条分组乐器资料；若用户曾手工修改这些分组资料，修改内容会丢失（单件资料会重新生成）。
- 风险：不再同步等待插件窗口关闭，VOCALOID6 Editor 等窗口可能在工程关闭后短暂停留，直到主消息循环处理 `WM_CLOSE` 或外部编辑进程退出。
- 风险：VST 参数仍按参数列表序号写入；插件升级改变参数顺序时可能对应错误参数。
- 后续：若需保留用户对旧分组资料的自定义内容，可在迁移时把分组正文附加到该组主乐器资料；VST 参数可按名称建立映射后再扩展协议。
