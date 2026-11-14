# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-14 22:23:15 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 统一主页（项目 / 模板 / 插件）与 Agent 资料库、AI 编曲面板的视觉规范 |
| 关联分支/提交 | feature/ui-visual-consistency / 本次提交 |
| 影响模块 | ui, app |
| 关联记录 | 20261113-234752-ai-agent-library-redesign.md |

## 2. 任务目标

- 解决主页、Agent 资料库与 AI 面板“有的好看有的难看”的风格割裂问题。
- 以项目页/模板页的留白、卡片化与克制配色为基准，统一间距、字号、圆角与按钮层级。
- 插件页降低信息密度：类别筛选、紧凑行、能力标签收进右侧详情抽屉。
- Agent 三页统一为「索引 + 工作台」模板，Markdown 正文默认富文本预览，编辑时切回源码。
- AI 面板改为浮动卡片，提示条降饱和，输入区只保留输入框、附件、高级选项与发送。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/ui/theme.rs` | 修改 | 新增间距/字号/圆角/控件高度设计令牌；新增 `card_frame` / `sheet_frame` / `floating_frame` / `field_label` / `section_title` / `primary_button` / `quiet_button` / `danger_button` / `chip`；`badge` 改为无描边低饱和标签；模块文档同步 |
| `src/ui/markdown.rs` | 新增 | 从 `ui/ai_panel.rs` 抽出轻量 Markdown 渲染（标题、列表、代码块、加粗、行内代码），支持基础字号参数，供对话与 Agent 正文预览共用 |
| `src/ui/mod.rs` | 修改 | 声明 `markdown` 模块 |
| `src/ui/ai_panel.rs` | 修改 | 面板改为带投影的浮动卡片；顶栏合并标题/状态；API Key 警告改为低饱和琥珀底 + 图标 + 文字按钮，错误条同步改为红底；输入卡片只保留附件「+」、「高级选项」与发送/停止，技能/可用乐器/必须乐器/智能体/模型/推理全部收进高级选项；Markdown 调用改为 `ui::markdown::render` |
| `src/app/state.rs` | 修改 | `AiLibraryUi` 新增 `body_preview`，默认开启预览 |
| `src/app/mod.rs` | 修改 | 主页插件页状态由 `home_plugin_expanded` 改为 `home_plugin_selected` 与 `home_plugin_kind`；构造初始化同步 |
| `src/app/home.rs` | 修改 | 项目/模板页统一使用间距令牌、32 点搜索框、无边框卡片与更柔和的筛选胶囊；项目行 56 点、详情卡无描边；插件页重做为类别胶囊 + 单一扫描菜单 + 全宽搜索 + 品牌分组紧凑行 + 右侧详情抽屉；新增 `plugin_kind_color` / `plugin_detail_drawer` / `plugin_detail_field`；删除旧的行内展开标签墙 |
| `src/app/agent_library.rs` | 修改 | 三页页头统一 22 点；搜索条改为无边框浅表面；列表与工作台改为无边框 `sheet_frame`；表单标签 11 点、输入 36 点、字段间距 16 点；正文默认 Markdown 预览并可在「预览 / 源码」间切换；操作按钮统一为 `theme` 主/文字/危险按钮；删除页内重复的按钮与筛选胶囊实现；模块文档同步 |
| `doc/代码规范.md` | 修改 | 7.6.3 增加设计令牌表与卡片/标签/表单/按钮规范；7.6.4 增加内容型页面字号阶梯；7.6.7 增加检查项 |
| `doc/关键常量.md` | 修改 | 新增设计令牌、浮动面板投影、插件详情抽屉/列表最小宽度；更新主页标题、项目行、插件品牌行与插件行高度；移除 `PLUGIN_NAME_W` |
| `doc/技术文档.md` | 修改 | 7.10 主页面板（Agent 表单、项目行、插件页与抽屉）；7.14 AI 浮动面板、提示条与高级选项；Markdown 渲染位置 |
| `doc/框架工作流.md` | 修改 | 主页/Agent/AI 模块条目与函数表；主题与 AI 面板函数表；Markdown 移至 `ui/markdown.rs` |
| `doc/项目简介.md` | 修改 | 插件页抽屉、Agent 表单与正文预览、AI 浮动面板与高级选项、全局间距/字号规范 |
| `history/ai/20261114-222315-ui-visual-consistency.md` | 新增 | 本记录 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 插件浏览 | 品牌分组 + 每条 40 点行 + 最多 5 个标签 + 行内展开键值面板 | 类别胶囊 + 52 点紧凑行（名称 / 厂商·格式 / 一个类别标签）+ 右侧 280~340 点详情抽屉 | 一屏信息单元减少，路径与能力不再打断列表 |
| 扫描入口 | 顶部并排三个扫描按钮 + 搜索框 | 一个「扫描插件」菜单 + 整行搜索框 | 工具栏不再拥挤 |
| Agent 表单 | 有描边搜索/列表/编辑卡片，10.5 点标签紧贴 26 点输入，Markdown 源码常驻 | 无边框卡片、11 点标签 + 36 点输入 + 16 点字段间距，正文默认预览 | 三个页面结构一致，长文案可读性提升 |
| Agent 按钮 | 页内自定义主/描边/危险按钮 | `theme::primary_button` / `quiet_button` / `danger_button` | 同类操作位置与视觉一致 |
| AI 面板 | 与编辑区同底色，顶部警示条蓝黄抢焦，底部 6 个控件 | 带投影浮动卡片，低饱和提示条，底部仅附件 / 高级选项 / 发送 | 主界面聚焦输入与回复 |
| Markdown | 只在 AI 对话渲染 | 抽出 `ui/markdown.rs`，对话与 Agent 正文共用 | 避免两套排版逻辑漂移 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `SPACE_XS` ~ `SPACE_2XL` | 无 | `4 / 8 / 12 / 16 / 24 / 32` 点 | `src/ui/theme.rs` | 内容型页面间距阶梯 |
| `FONT_MICRO` ~ `FONT_PAGE` | 无 | `10.5 / 11 / 12.5 / 15 / 18 / 22` 点 | `src/ui/theme.rs` | 内容型页面字号阶梯 |
| `RADIUS_CARD` / `RADIUS_SHEET` | 无 | `10 / 12` 点 | `src/ui/theme.rs` | 卡片与页面容器圆角 |
| `CARD_PAD` | 无 | `16` 点 | `src/ui/theme.rs` | 卡片内边距 |
| `CONTROL_H` / `FIELD_H` | 无 | `28 / 36` 点 | `src/ui/theme.rs` | 控件与表单输入高度 |
| `HOME_TITLE_SIZE` | `19` | `22` | `src/app/home.rs` | 与 Agent 页头统一 |
| `PROJECT_ROW_H` | `52` | `56` | `src/app/home.rs` | 项目行留白 |
| `PLUGIN_BRAND_H` | `34` | `40` | `src/app/home.rs` | 品牌行 |
| `PLUGIN_ROW_H` | `40` | `52` | `src/app/home.rs` | 紧凑插件行 |
| `PLUGIN_NAME_W` | `210` | 删除 | `src/app/home.rs` | 不再使用固定名称列 |
| `PLUGIN_DRAWER_MIN_W` / `PLUGIN_DRAWER_MAX_W` | 无 | `280 / 340` | `src/app/home.rs` | 详情抽屉宽度 |
| `PLUGIN_LIST_MIN_W` | 无 | `320` | `src/app/home.rs` | 抽屉打开时的列表下限 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/代码规范.md` | 是 | 7.6.3 设计令牌与卡片规范；7.6.4 字号阶梯；7.6.7 检查清单 |
| `doc/关键常量.md` | 是 | 第 8 节主题与窗口：设计令牌、投影、主页/插件常量 |
| `doc/技术文档.md` | 是 | 7.10 主页、7.14 AI 面板与 7.14 编曲流程 |
| `doc/框架工作流.md` | 是 | 模块地图、Agent/主题/AI 面板函数表、Markdown 新位置 |
| `doc/项目简介.md` | 是 | 2.4 插件、2.6 全局界面、2.7 AI 编曲与 Agent 资料编辑器 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 格式化后无差异 |
| `cargo check --all-targets` | 通过 | 无编译错误 |
| `cargo test` | 通过 | `surfing-studio` 172 passed / 23 ignored |
| 手工验证 | 未执行 | 界面改动需在桌面环境启动后目视确认；编译与单元测试已覆盖结构性问题 |

## 8. 风险与后续

- 风险：egui 的 `SidePanel` 是并排停靠而非真正的模态浮层，无法实现网页式背景模糊；本次用圆角、投影与外侧留白表达“浮起来”，没有加入会遮挡编曲区的遮罩。
- 风险：AI「高级选项」内嵌 ComboBox 的弹出层依赖 egui 嵌套 Popup 行为；若后续 egui 升级改变关闭策略，需要改为内联单选列表。
- 后续：如需进一步降低插件页密度，可默认收起品牌组或增加“收藏/最近使用”页签。
