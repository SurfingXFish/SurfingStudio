# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-16 20:55:44 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 代码模型 |
| 任务来源 | 将设置中的「AI」改为「模型」，重写为模型列表管理，并新增服务商选择、模型表单与连通性测试 |
| 关联分支/提交 | feature/ai-model-management / 未知 |
| 影响模块 | ai, app, ui, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 设置左侧分类「AI」改为「模型」，页面改为「内置 / 自定义」分组的模型列表。
- 「+ 添加模型」先打开服务商网格，再进入「通过服务商添加」表单；支持自定义模型。
- 表单包含展示名称、API Key、接口地址、上下文输入/输出、工具调用轮数、思考强度与 Temperature / Top P / Top K。
- 添加/保存前执行一次真实连通性测试，成功后才写入模型列表。
- 兼容旧版 `ai.model_preset` / `api_keys` / `custom_*` 设置，并让 AI 面板与请求链路使用当前模型。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/ai/models.rs` | 新增 | 服务商目录（18 家 + 自定义）、内置模型种子、`AiModelConfig` 模型条目、展示名/地址/协议族解析、校验清洗、`build_request` 与单元测试 |
| `src/ai/mod.rs` | 修改 | 导出模型目录类型与常量；`AiRequest` 增加 `provider_label`、`top_p`、`top_k`、`json_output`；`RequestBody` 支持可选 Top P/Top K 与可选 JSON 输出；移除旧静态模型预设 |
| `src/ai/backends/openai_compat.rs` | 修改 | 请求体写入 Top P/Top K 与可选 `response_format`；错误文案改用模型条目的服务商名称并指向 设置 → 模型 |
| `src/app/settings.rs` | 修改 | `AiSettings` 增加 `models` / `active_model` 与迁移、选择、增删改、启停、服务商 Key 查询、唯一 id 方法；设置分类 `Ai` 改为 `Models`（显示「模型」）；加载时调用 `migrate_models`；移除旧 AI 页；补充迁移与回退测试 |
| `src/app/settings_models.rs` | 新增 | 模型页与弹窗：分组列表、服务商网格、通过服务商添加/编辑/删除、表单草稿与本地校验、连通性测试后台线程（探活关闭思考）与单帧轮询、模型标识与细线图标；内置模型编辑时锁定服务商与模型 ID |
| `src/app/mod.rs` | 修改 | 注册 `settings_models` 模块，`SurfingApp` 增加模型页 UI 状态 |
| `src/app/ai.rs` | 修改 | `send_ai_message` 改用 `settings.ai.active_model` 与 `build_request`；联网检索改用模型列表中的 DeepSeek Key；错误文案与截断提示改为 设置 → 模型 |
| `src/ui/ai_panel.rs` | 修改 | 模型下拉按「内置 / 自定义」列出启用模型并写回 `active_model`；推理下拉写回当前模型；缺少密钥/模型时提示去模型设置 |
| `assets/i18n/en-US.yml` | 修改 | 补充模型管理、服务商、表单、校验与连通性测试的英文译文 |
| `doc/技术文档.md` | 修改 | 更新 AI 领域职责、模块表、设置字段与迁移、模型页、推理强度、模型下拉与请求构造说明 |
| `doc/关键常量.md` | 修改 | 更新设置分类、模型默认值、服务商目录、模型页尺寸与连通性测试常量 |
| `doc/框架工作流.md` | 修改 | 更新全局设置模块、`ai/models.rs`、`settings_models.rs`、AI 面板与发送流程索引 |
| `doc/项目简介.md` | 修改 | 更新模型管理、服务商添加、连通性测试、推理强度与联网检索入口说明 |
| `doc/代码规范.md` | 修改 | 13.11 新增 AI 服务商改为在 `ai/models.rs` 的 `PROVIDERS` 登记，并更新协议族、网络线程与设置入口说明 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 设置分类 | 界面 / 存储 / 音频 / 渲染 / 控制器 / AI / 插件 / 快捷键 / 软件信息 | 界面 / 存储 / 音频 / 渲染 / 控制器 / 模型 / 插件 / 快捷键 / 软件信息 | 设置窗口入口与文案 |
| 模型选择 | `ai.model_preset` 指向 5 个静态预设 | `ai.models` 列表 + `ai.active_model`，内置与自定义条目统一管理 | AI 面板、请求构造、设置页 |
| 配置持久化 | API Key 按服务商保存，自定义接口单独字段 | API Key 内嵌模型条目；旧字段保留用于迁移与兼容 | `settings.json` 结构 |
| 发送请求 | `find_preset` + 全局 temperature/max_tokens/reasoning | `AiModelConfig::build_request`，按条目的输出上限、温度、Top P/Top K 与推理强度构造 | `app/ai.rs`、`ai/backends/openai_compat.rs` |
| 添加模型 | 无独立流程 | 服务商网格 → 表单 → 后台连通性测试 → 写入列表 | `app/settings_models.rs` |
| 推理强度 | 全局 `ai.reasoning` | 每个模型条目的 `reasoning`；AI 面板下拉写回当前模型 | AI 面板与请求体 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `DEFAULT_MODEL_CONTEXT_INPUT` | 无 | `128_000` | `src/ai/models.rs` | 新模型默认上下文输入 |
| `DEFAULT_MODEL_CONTEXT_OUTPUT` | 无 | `32_768` | `src/ai/models.rs` | 新模型默认最大输出 |
| `DEFAULT_MODEL_TOOL_ROUNDS` | 无 | `500` | `src/ai/models.rs` | 工具调用轮数默认值 |
| `DEFAULT_MODEL_TEMPERATURE` | 无 | `0.7` | `src/ai/models.rs` | 模型条目默认采样温度 |
| `MAX_MODEL_OUTPUT_TOKENS` | 无 | `131_072` | `src/ai/models.rs` | 输出上限安全值 |
| `MODELS_ROW_H` / `MODELS_GROUP_H` / `MODELS_HEADER_H` | 无 | `44 / 34 / 28` 点 | `src/app/settings_models.rs` | 模型列表尺寸 |
| `MODELS_ICON` / `MODELS_PROVIDER_W` / `MODELS_OPS_W` | 无 | `26 / 170 / 98` 点 | `src/app/settings_models.rs` | 模型行布局 |
| `MODELS_MODAL_W` / `MODELS_CARD_H` | 无 | `620 / 52` 点 | `src/app/settings_models.rs` | 弹窗与服务商卡片 |
| `MODELS_PROBE_TIMEOUT` / `MODELS_PROBE_REPAINT_MS` | 无 | `40` 秒 / `100` ms | `src/app/settings_models.rs` | 连通性测试等待与重绘 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 AI 领域、模块表、7.10 设置字段与模型页、7.14 推理强度/模型下拉/请求构造、联网检索 |
| `doc/关键常量.md` | 是 | 5 设置分类与配置文件、12 AI 编曲模型目录与模型页常量 |
| `doc/框架工作流.md` | 是 | 2 模块地图、3.3 设置、3.8 `ai/models.rs`、3.11 `settings_models.rs`、5.8 AI 编曲流程 |
| `doc/项目简介.md` | 是 | 2.7 AI 编曲的模型、设置、推理强度与联网检索 |
| `doc/代码规范.md` | 是 | 13.11 新增 AI 服务商/协议后端的登记位置与网络线程说明 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 提交前已执行 |
| `cargo test` | 通过 | `cargo test --lib`：184 passed，0 failed，23 ignored |
| 手工验证 | 通过 | 启动 debug 构建：设置左侧显示「模型」；内置分组展开后四条模型、启用开关与编辑图标正常；「+ 添加模型」打开服务商网格并进入 DeepSeek 表单；展示名称、API Key、获取密钥链接、高级配置、底部提示与按钮均可见；空密钥提交显示「请填写 API 密钥」。未使用真实密钥完成连通性测试成功路径。 |

## 8. 风险与后续

- 风险：服务商模型 id 与计费策略会变化，目录中的常用模型可能过期；表单已支持手动输入 Model ID 与自定义接口地址。
- 风险：部分服务商的 `top_k`、思考字段或 AWS 区域地址并非标准 OpenAI 兼容行为；留空 Top K 时不发送，AWS 需在高级配置中改成实际区域地址。
- 风险：工具调用轮数字段当前只保存配置，编曲流程尚未使用工具循环。
- 后续：根据实际账号验证 18 家服务商的默认地址与模型列表，必要时只保留稳定项或补充 `key_url`。

---

## 9. 追加记录（2026-11-16 21:32:41）

### 9.1 任务目标

1. 去掉所有内置模型，模型列表只包含用户添加的条目。
2. 联网检索不再调用 DeepSeek 官方 Search API，改为自行实现的 Bing HTML 检索。
3. 重新设计「编辑模型」界面，去掉过时的折叠高级配置与拥挤排版。
4. 删除编辑模型中的「思考模式」整块界面。

### 9.2 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/ai/search.rs` | 重写 | 改为直接请求 `https://cn.bing.com/search`：UTF-8 百分号编码、浏览器 UA、`b_algo` 结果块解析、HTML 去标签与实体解码、标题/站点/摘要/链接整理；新增编码、解析与站点提取测试 |
| `src/app/ai.rs` | 修改 | `AiSearchConfig` 只保留 `enabled` 与 `timeout`；不再从 DeepSeek 模型取 Search API Key |
| `src/ai/models.rs` | 修改 | 删除 `builtin_models`、`AiModelConfig::builtin` 字段与构造器；模块文档与测试改为用户模型 |
| `src/ai/mod.rs` | 修改 | 移除 `builtin_models` 导出与文档引用 |
| `src/app/settings.rs` | 修改 | `AiSettings::default` 改为空模型列表；`migrate_models` 只为配置过密钥的 DeepSeek / MiMo / 自定义服务创建用户模型，并清理旧版空密钥内置占位；`remove_model` 允许删除所有模型；更新迁移/回退测试 |
| `src/app/settings_models.rs` | 重写 | 单一模型列表（无内置/自定义分组、无锁定删除）；新增空状态卡片；重设计 680 点宽表单（身份行 + 连接/凭据/上下文/采样四张分区卡片），移除折叠「高级配置」与整块「思考模式」，底部保留连通性提示与操作按钮 |
| `src/ui/ai_panel.rs` | 修改 | 模型下拉改为单一启用列表，不再按内置/自定义分组 |
| `assets/i18n/en-US.yml` | 修改 | 补充新列表/空状态/表单分区与 Bing 检索的英文译文，移除过时的内置模型与思考模式文案 |
| `doc/技术文档.md` / `doc/关键常量.md` / `doc/框架工作流.md` / `doc/项目简介.md` / `doc/代码规范.md` | 修改 | 同步无内置模型、自建 Bing 检索、模型页与编辑表单、思考模式移除 |

### 9.3 关键行为变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 模型列表 | 四条内置种子 + 用户模型，内置不可删除 | 全部由用户添加，可编辑/删除；空列表显示引导 | 设置 → 模型、AI 面板 |
| 旧配置迁移 | 无密钥也生成四条内置条目 | 只为配置过密钥的服务商生成用户模型，空密钥占位被清理 | `settings.json` 升级 |
| 编辑表单 | 折叠高级配置 + 思考模式信息框与推理强度下拉 | 身份行 + 四张分区卡片；无思考模式 | 设置 → 模型 |
| 联网检索 | DeepSeek `/v1/tools/search`，需要 DeepSeek Key | 自建 Bing HTML 检索，无需 Search API Key | `ai/search.rs`、设置页提示 |

### 9.4 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 提交前执行 |
| `cargo test --lib` | 通过 | 187 passed，0 failed，23 ignored |
| `cargo build` | 通过 | debug 构建完成，仅有既有无关警告 |
| 界面手工验证 | 通过 | 启动 debug 构建：设置 → 模型显示空状态卡片与「+ 添加模型」；服务商网格正常；DeepSeek 表单在 620 点高窗口内完整显示标题栏、身份行、连接/凭据/上下文/采样分区与底部按钮；已无「高级配置」折叠与「思考模式」；联网搜索提示为 Bing 检索 |

---

## 10. 追加记录（2026-11-16 21:50:06）

### 10.1 任务目标

- 按用户反馈继续重设计「编辑模型」界面：去掉嵌套分区卡片，改为更克制、更接近 IDE 设置的单平面表单。

### 10.2 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/settings_models.rs` | 重设计 | 表单改为无卡片布局：44 点标识 + 18 点展示名 + 来源副标题的身份行，下方细分割线；主体为「左侧 92 点字段标签 + 右侧控件」的分行布局，分组间只用细分隔线；输入框改用 `PANEL_2` 表面与 `LINE` 描边；底部细线分隔后左侧提示、右侧文字「重置」与强调色主按钮 |
| `src/app/settings_models.rs` | 修改 | 新增 `form_row` / `form_divider` / `form_sub_label`；移除 `form_section` 卡片；采样参数与上下文窗口保留列布局，快捷值仍为文字按钮 |
| `assets/i18n/en-US.yml` | 修改 | 新增「上下文窗口」「采样参数」英文译文，移除不再使用的「连接」「凭据」分区标题译文 |
| `doc/技术文档.md` | 修改 | 模型页说明更新为左侧标签分行表单、细分隔线与无嵌套卡片 |
| `doc/框架工作流.md` | 修改 | `settings_models.rs` 函数表改为 `form_row` / `form_divider` |

### 10.3 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 提交前执行 |
| `cargo test --lib` | 通过 | 187 passed，0 failed，23 ignored |
| `cargo build` | 通过 | debug 构建完成，仅有既有无关警告 |
| 界面手工验证 | 通过 | 启动 debug 构建进入 设置 → 模型 → 添加模型 → DeepSeek：身份行、左侧标签分行表单、上下文窗口双列、采样参数三列、底部提示与按钮均正常；上下滚动可查看全部字段，弹窗在默认窗口内无裁切 |

---

## 11. 追加记录（2026-11-16 22:00:16）

### 11.1 任务目标

- 移除 AI 编曲面板与编曲区之间的黑边：不再压暗编曲区、不再绘制左侧投影，让面板像停靠栏一样贴合工作区右缘。

### 11.2 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/ui/ai_panel.rs` | 修改 | 遮罩层改为透明点击层，只负责点击收起，不再用黑色 145 alpha 压暗编曲区；移除 32 点左侧投影和四边框，只保留左缘 1 点 `LINE` 分割线；面板宽度与高度改为铺满 `panel_w × rect.height()`，右缘与窗口齐平 |
| `doc/技术文档.md` / `doc/项目简介.md` / `doc/框架工作流.md` / `doc/关键常量.md` / `doc/代码规范.md` | 修改 | 同步“透明点击层 + 右侧停靠面板、无遮罩压暗、无投影”的说明，并修正模型页常量与表单布局描述 |

### 11.3 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 提交前执行 |
| `cargo test --lib` | 通过 | 187 passed，0 failed，23 ignored |
| `cargo build` | 通过 | debug 构建完成，仅有既有无关警告 |
| 界面手工验证 | 通过 | 新建工程后打开 AI 编曲：编曲区保持原本亮度，面板与编曲区之间只有 1 点细分隔线，无黑色渐变投影；面板右缘贴合窗口，无额外黑边 |
