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
| `src/app/settings_models.rs` | 新增 | 模型页与弹窗：分组列表、服务商网格、通过服务商添加/编辑/删除、表单草稿与本地校验、连通性测试后台线程与单帧轮询、模型标识与细线图标 |
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
