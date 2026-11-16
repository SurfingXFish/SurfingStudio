//! 所属模块：ai / 模型目录与用户模型配置
//! 文件用途：
//! - 维护可添加的服务商目录、默认 Base URL、密钥申请地址与常用模型列表。
//! - 定义用户可启停、可编辑的模型条目（`AiModelConfig`）与内置模型种子。
//! - 把模型条目转换为后端请求，统一处理采样参数、上下文与推理强度。
//!
//! 函数清单：
//! - `provider` / `provider_label` / `default_model`：按 id 查询服务商目录。
//! - `builtin_models`：生成内置模型条目。
//! - `AiModelConfig::display_label` / `resolved_base_url` / `provider_label` / `family` /
//!   `is_ready` / `sanitize` / `build_request`：模型条目的展示、校验与请求构造。
//!
//! Module: ai / model catalogue and user model entries
//! Purpose:
//! - Maintain the provider catalogue, default base URLs, key pages and common model lists.
//! - Define user-managed model entries (`AiModelConfig`) and the built-in model seeds.
//! - Convert an entry into a backend request with sampling, context and reasoning parameters.
//!
//! Functions:
//! - `provider` / `provider_label` / `default_model`: look up the provider catalogue by id.
//! - `builtin_models`: build the built-in model entries.
//! - `AiModelConfig::display_label` / `resolved_base_url` / `provider_label` / `family` /
//!   `is_ready` / `sanitize` / `build_request`: display, validation and request construction.

use super::{AiMessage, AiProvider, AiReasoning, AiRequest};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 自定义服务商标识；Base URL 与模型名由用户填写。
/// Custom-provider id; the base URL and model id are entered by the user.
pub const CUSTOM_PROVIDER_ID: &str = "custom";

/// 默认上下文输入窗口（Token）。
/// Default context input window in tokens.
pub const DEFAULT_MODEL_CONTEXT_INPUT: u32 = 128_000;
/// 默认最大输出（Token）。
/// Default maximum output in tokens.
pub const DEFAULT_MODEL_CONTEXT_OUTPUT: u32 = 32_768;
/// 默认工具调用轮数。
/// Default tool-call rounds.
pub const DEFAULT_MODEL_TOOL_ROUNDS: u32 = 500;
/// 默认采样温度。
/// Default sampling temperature.
pub const DEFAULT_MODEL_TEMPERATURE: f32 = 0.7;
/// 输出上限的安全上限，避免上下文配置误填导致请求被服务商拒绝。
/// Safety cap for the output limit so a wrong context value cannot break requests.
pub const MAX_MODEL_OUTPUT_TOKENS: u32 = 131_072;

/// 服务商目录中的一个常用模型。
/// One common model in a provider catalogue entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiProviderModel {
    /// 接口使用的模型 id。
    /// Model id used by the API.
    pub id: &'static str,
    /// 下拉展示名称。
    /// Label shown in the dropdown.
    pub label: &'static str,
}

/// 服务商目录条目。
/// Provider catalogue entry.
#[derive(Clone, Debug)]
pub struct AiProviderSpec {
    /// 持久化标识。
    /// Persisted id.
    pub id: &'static str,
    /// 界面展示名称。
    /// Display name.
    pub label: &'static str,
    /// 默认 Base URL。
    /// Default base URL.
    pub base_url: &'static str,
    /// 密钥申请页面；空字符串表示不显示链接。
    /// API-key page; empty hides the link.
    pub key_url: &'static str,
    /// 请求体差异所属的协议族。
    /// Protocol family that controls request-body differences.
    pub family: AiProvider,
    /// 常用模型列表；可为空，允许用户手动输入模型 id。
    /// Common models; may be empty, and a manual model id is always allowed.
    pub models: &'static [AiProviderModel],
}

/// 自定义服务商的静态条目。
/// Static entry of the custom provider.
const CUSTOM_PROVIDER: AiProviderSpec = AiProviderSpec {
    id: CUSTOM_PROVIDER_ID,
    label: "自定义",
    base_url: "",
    key_url: "",
    family: AiProvider::Custom,
    models: &[],
};

const DEEPSEEK_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "deepseek-flash",
        label: "DeepSeek Flash",
    },
    AiProviderModel {
        id: "deepseek-v4-pro",
        label: "DeepSeek V4 Pro",
    },
];

const VOLCENGINE_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "doubao-seed-2-1-pro-260915",
        label: "Doubao Seed 2.1 Pro",
    },
    AiProviderModel {
        id: "doubao-seed-2-1-lite-260215",
        label: "Doubao Seed 2.1 Lite",
    },
    AiProviderModel {
        id: "doubao-seed-1-6-250615",
        label: "Doubao Seed 1.6",
    },
];

const MINIMAX_CN_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "MiniMax-M3",
        label: "MiniMax M3",
    },
    AiProviderModel {
        id: "MiniMax-M3.1-Flash",
        label: "MiniMax M3.1 Flash",
    },
];

const MINIMAX_GLOBAL_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "MiniMax-M3",
        label: "MiniMax M3",
    },
    AiProviderModel {
        id: "MiniMax-M3.1-Flash",
        label: "MiniMax M3.1 Flash",
    },
];

const BIGMODEL_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "glm-5.3",
        label: "GLM-5.3",
    },
    AiProviderModel {
        id: "glm-5",
        label: "GLM-5",
    },
    AiProviderModel {
        id: "glm-4.7",
        label: "GLM-4.7",
    },
];

const ALIYUN_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "qwen3.8-max",
        label: "Qwen3.8 Max",
    },
    AiProviderModel {
        id: "qwen3.8-flash",
        label: "Qwen3.8 Flash",
    },
    AiProviderModel {
        id: "qwen3.7-plus",
        label: "Qwen3.7 Plus",
    },
];

const MIMO_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "mimo-v2.6-flash",
        label: "MiMo V2.6 Flash",
    },
    AiProviderModel {
        id: "mimo-v2.6-pro",
        label: "MiMo V2.6 Pro",
    },
    AiProviderModel {
        id: "mimo-v2.6-pro-ultraspeed",
        label: "MiMo V2.6 Pro UltraSpeed",
    },
];

const SILICONFLOW_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "deepseek-ai/DeepSeek-V4-Flash",
        label: "DeepSeek V4 Flash",
    },
    AiProviderModel {
        id: "moonshotai/Kimi-K3",
        label: "Kimi K3",
    },
    AiProviderModel {
        id: "zai-org/GLM-5.3",
        label: "GLM-5.3",
    },
];

const ZAI_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "glm-5.3",
        label: "GLM-5.3",
    },
    AiProviderModel {
        id: "glm-5",
        label: "GLM-5",
    },
    AiProviderModel {
        id: "glm-4.7",
        label: "GLM-4.7",
    },
];

const OPENROUTER_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "openrouter/auto",
        label: "Auto Router",
    },
    AiProviderModel {
        id: "deepseek/deepseek-v4-pro",
        label: "DeepSeek V4 Pro",
    },
    AiProviderModel {
        id: "moonshotai/kimi-k3",
        label: "Kimi K3",
    },
];

const KIMI_CN_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "kimi-k3",
        label: "Kimi K3",
    },
    AiProviderModel {
        id: "kimi-k2.5",
        label: "Kimi K2.5",
    },
];

const KIMI_GLOBAL_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "kimi-k3",
        label: "Kimi K3",
    },
    AiProviderModel {
        id: "kimi-k2.5",
        label: "Kimi K2.5",
    },
];

const BYTEPLUS_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "dola-seed-2.0-pro",
        label: "Dola Seed 2.0 Pro",
    },
    AiProviderModel {
        id: "dola-seed-2.0-lite",
        label: "Dola Seed 2.0 Lite",
    },
    AiProviderModel {
        id: "bytedance-seed-code",
        label: "ByteDance Seed Code",
    },
];

const AWS_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "openai.gpt-oss-120b-1:0",
        label: "GPT-OSS 120B",
    },
    AiProviderModel {
        id: "openai.gpt-oss-20b-1:0",
        label: "GPT-OSS 20B",
    },
];

const TENCENT_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "hy4",
        label: "Hunyuan Hy4",
    },
    AiProviderModel {
        id: "hunyuan-t1-latest",
        label: "Hunyuan T1",
    },
    AiProviderModel {
        id: "hunyuan-turbos-latest",
        label: "Hunyuan TurboS",
    },
];

const MOARK_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "auto",
        label: "智能路由",
    },
    AiProviderModel {
        id: "deepseek-ai/DeepSeek-V4-Flash",
        label: "DeepSeek V4 Flash",
    },
    AiProviderModel {
        id: "zai-org/GLM-5.3",
        label: "GLM-5.3",
    },
];

const PPIO_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "deepseek/deepseek-v4-pro",
        label: "DeepSeek V4 Pro",
    },
    AiProviderModel {
        id: "qwen/qwen3.8-max",
        label: "Qwen3.8 Max",
    },
    AiProviderModel {
        id: "zai-org/glm-5.3",
        label: "GLM-5.3",
    },
];

const TOKENPONY_MODELS: &[AiProviderModel] = &[
    AiProviderModel {
        id: "deepseek-v4-pro",
        label: "DeepSeek V4 Pro",
    },
    AiProviderModel {
        id: "qwen3.8-max",
        label: "Qwen3.8 Max",
    },
];

/// 可添加的服务商目录；顺序即添加界面两列网格的顺序。
/// Provider catalogue; the order matches the two-column grid on the add screen.
pub const PROVIDERS: [AiProviderSpec; 18] = [
    AiProviderSpec {
        id: "deepseek",
        label: "DeepSeek",
        base_url: "https://api.deepseek.com",
        key_url: "https://platform.deepseek.com/api_keys",
        family: AiProvider::DeepSeek,
        models: DEEPSEEK_MODELS,
    },
    AiProviderSpec {
        id: "volcengine",
        label: "火山引擎",
        base_url: "https://ark.cn-beijing.volces.com/api/v3",
        key_url: "https://console.volcengine.com/ark",
        family: AiProvider::Custom,
        models: VOLCENGINE_MODELS,
    },
    AiProviderSpec {
        id: "minimax-cn",
        label: "MiniMax CN",
        base_url: "https://api.minimaxi.com/v1",
        key_url: "https://platform.minimaxi.com",
        family: AiProvider::Custom,
        models: MINIMAX_CN_MODELS,
    },
    AiProviderSpec {
        id: "minimax-global",
        label: "MiniMax Global",
        base_url: "https://api.minimax.io/v1",
        key_url: "https://platform.minimax.io",
        family: AiProvider::Custom,
        models: MINIMAX_GLOBAL_MODELS,
    },
    AiProviderSpec {
        id: "bigmodel",
        label: "Bigmodel",
        base_url: "https://open.bigmodel.cn/api/paas/v4",
        key_url: "https://bigmodel.cn",
        family: AiProvider::Custom,
        models: BIGMODEL_MODELS,
    },
    AiProviderSpec {
        id: "aliyun",
        label: "阿里云",
        base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
        key_url: "https://bailian.console.aliyun.com",
        family: AiProvider::Custom,
        models: ALIYUN_MODELS,
    },
    AiProviderSpec {
        id: "mimo",
        label: "Xiaomi MIMO",
        base_url: "https://api.xiaomimimo.com/v1",
        key_url: "https://mimo.mi.com",
        family: AiProvider::Mimo,
        models: MIMO_MODELS,
    },
    AiProviderSpec {
        id: "siliconflow",
        label: "硅基流动",
        base_url: "https://api.siliconflow.cn/v1",
        key_url: "https://cloud.siliconflow.cn/account/ak",
        family: AiProvider::Custom,
        models: SILICONFLOW_MODELS,
    },
    AiProviderSpec {
        id: "zai",
        label: "Z.ai",
        base_url: "https://api.z.ai/api/paas/v4",
        key_url: "https://z.ai",
        family: AiProvider::Custom,
        models: ZAI_MODELS,
    },
    AiProviderSpec {
        id: "openrouter",
        label: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        key_url: "https://openrouter.ai/keys",
        family: AiProvider::Custom,
        models: OPENROUTER_MODELS,
    },
    AiProviderSpec {
        id: "kimi-cn",
        label: "Kimi CN",
        base_url: "https://api.moonshot.cn/v1",
        key_url: "https://platform.moonshot.cn/console/api-keys",
        family: AiProvider::Custom,
        models: KIMI_CN_MODELS,
    },
    AiProviderSpec {
        id: "kimi-global",
        label: "Kimi Global",
        base_url: "https://api.moonshot.ai/v1",
        key_url: "https://platform.kimi.ai",
        family: AiProvider::Custom,
        models: KIMI_GLOBAL_MODELS,
    },
    AiProviderSpec {
        id: "byteplus",
        label: "BytePlus",
        base_url: "https://ark.ap-southeast.bytepluses.com/api/v3",
        key_url: "https://console.byteplus.com/ark",
        family: AiProvider::Custom,
        models: BYTEPLUS_MODELS,
    },
    AiProviderSpec {
        id: "aws",
        label: "AWS",
        base_url: "https://bedrock-runtime.us-west-2.amazonaws.com/openai/v1",
        key_url: "https://console.aws.amazon.com/bedrock",
        family: AiProvider::Custom,
        models: AWS_MODELS,
    },
    AiProviderSpec {
        id: "tencent",
        label: "腾讯云",
        base_url: "https://api.hunyuan.cloud.tencent.com/v1",
        key_url: "https://console.cloud.tencent.com/hunyuan/api-key",
        family: AiProvider::Custom,
        models: TENCENT_MODELS,
    },
    AiProviderSpec {
        id: "moark",
        label: "模力方舟",
        base_url: "https://ai.gitee.com/v1",
        key_url: "https://ai.gitee.com",
        family: AiProvider::Custom,
        models: MOARK_MODELS,
    },
    AiProviderSpec {
        id: "ppio",
        label: "PPIO",
        base_url: "https://api.ppio.com/openai",
        key_url: "https://ppio.com/docs/support/api-key",
        family: AiProvider::Custom,
        models: PPIO_MODELS,
    },
    AiProviderSpec {
        id: "tokenpony",
        label: "小马算力",
        base_url: "https://api.tokenpony.cn/v1",
        key_url: "https://www.tokenpony.cn",
        family: AiProvider::Custom,
        models: TOKENPONY_MODELS,
    },
];

/// 所属模块：ai / 模型目录
/// 功能：按服务商 id 查询目录条目；未知 id 与 `custom` 返回自定义条目。
/// 输入：`id`：服务商 id。
/// 输出/副作用：静态条目；不修改状态。
/// 复杂度：时间 O(服务商数)，空间 O(1)。
///
/// Module: ai / model catalogue
/// Purpose: Look up a provider by id; unknown ids and `custom` return the custom entry.
/// Inputs: `id`: provider id.
/// Outputs / side effects: the static entry; no state change.
/// Complexity: O(providers) time and O(1) space.
pub fn provider(id: &str) -> &'static AiProviderSpec {
    PROVIDERS
        .iter()
        .find(|provider| provider.id == id)
        .unwrap_or(&CUSTOM_PROVIDER)
}

/// 所属模块：ai / 模型目录
/// 功能：返回服务商的展示名称；未知 id 显示“自定义”。
/// 输入：`id`：服务商 id。
/// 输出/副作用：静态文本；不修改状态。
/// 复杂度：时间 O(服务商数)，空间 O(1)。
///
/// Module: ai / model catalogue
/// Purpose: Return a provider's display name; unknown ids show “自定义”.
/// Inputs: `id`: provider id.
/// Outputs / side effects: static text; no state change.
/// Complexity: O(providers) time and O(1) space.
pub fn provider_label(id: &str) -> &'static str {
    provider(id).label
}

/// 所属模块：ai / 模型目录
/// 功能：返回某服务商的第一个常用模型 id；自定义服务商返回空字符串。
/// 输入：`id`：服务商 id。
/// 输出/副作用：静态模型 id；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / model catalogue
/// Purpose: Return the first common model id of a provider; the custom provider returns an empty
/// string.
/// Inputs: `id`: provider id.
/// Outputs / side effects: static model id; no state change.
/// Complexity: O(1) time and space.
pub fn default_model(id: &str) -> &'static str {
    provider(id)
        .models
        .first()
        .map(|model| model.id)
        .unwrap_or("")
}

/// 所属模块：ai / 模型目录
/// 功能：生成四条内置模型（DeepSeek Flash / V4 Pro、MiMo V2.6 Flash / Pro），id 与旧预设一致，
/// 便于旧配置直接迁移。
/// 输入：无。
/// 输出/副作用：新的模型条目；不读盘。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / model catalogue
/// Purpose: Build the four built-in models (DeepSeek Flash / V4 Pro, MiMo V2.6 Flash / Pro) whose
/// ids match the legacy presets so old settings migrate directly.
/// Inputs: none.
/// Outputs / side effects: new model entries; no disk access.
/// Complexity: O(1) time and space.
pub fn builtin_models() -> Vec<AiModelConfig> {
    vec![
        AiModelConfig::builtin(
            "deepseek-flash",
            "deepseek",
            "deepseek-flash",
            "DeepSeek Flash",
            128_000,
            32_768,
        ),
        AiModelConfig::builtin(
            "deepseek-v4-pro",
            "deepseek",
            "deepseek-v4-pro",
            "DeepSeek V4 Pro",
            128_000,
            32_768,
        ),
        AiModelConfig::builtin(
            "mimo-v2.6-flash",
            "mimo",
            "mimo-v2.6-flash",
            "MiMo V2.6 Flash",
            128_000,
            32_768,
        ),
        AiModelConfig::builtin(
            "mimo-v2.6-pro",
            "mimo",
            "mimo-v2.6-pro",
            "MiMo V2.6 Pro",
            128_000,
            32_768,
        ),
    ]
}

/// 用户可管理的模型条目；内置模型与自定义模型共用同一结构。
/// A user-managed model entry; built-in and custom models share the same shape.
#[derive(Clone, Serialize, Deserialize)]
pub struct AiModelConfig {
    /// 稳定唯一 id（内置模型沿用旧预设 id）。
    /// Stable unique id (built-ins reuse the legacy preset ids).
    #[serde(default)]
    pub id: String,
    /// 服务商目录 id。
    /// Provider catalogue id.
    #[serde(default)]
    pub provider_id: String,
    /// 模型展示名称；为空时显示模型 id。
    /// Display name; falls back to the model id when empty.
    #[serde(default)]
    pub display_name: String,
    /// 接口模型 id。
    /// API model id.
    #[serde(default)]
    pub model: String,
    /// 自定义 Base URL；为空时使用服务商默认地址。
    /// Custom base URL; empty uses the provider default.
    #[serde(default)]
    pub base_url: String,
    /// API Key；只写入本机设置文件，禁止写入工程与日志。
    /// API key; only stored in the local settings file, never in projects or logs.
    #[serde(default)]
    pub api_key: String,
    /// 上下文输入窗口（Token）。
    /// Context input window in tokens.
    #[serde(default = "default_context_input")]
    pub context_input: u32,
    /// 最大输出（Token）。
    /// Maximum output in tokens.
    #[serde(default = "default_context_output")]
    pub context_output: u32,
    /// 工具调用轮数；当前编曲流程未使用工具循环，仅保存供后续能力使用。
    /// Tool-call rounds; the current composition flow has no tool loop and only stores it.
    #[serde(default = "default_tool_rounds")]
    pub tool_rounds: u32,
    /// 推理强度：`off` / `low` / `medium` / `high`。
    /// Reasoning effort: `off` / `low` / `medium` / `high`.
    #[serde(default = "default_reasoning")]
    pub reasoning: String,
    /// 采样温度 0~2。
    /// Sampling temperature 0..=2.
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Top P；`None` 表示留空，不发送。
    /// Top P; `None` means left blank and omitted from the request.
    #[serde(default)]
    pub top_p: Option<f32>,
    /// Top K；`None` 表示留空，不发送。
    /// Top K; `None` means left blank and omitted from the request.
    #[serde(default)]
    pub top_k: Option<u32>,
    /// 是否在模型下拉中启用。
    /// Whether the model is enabled in the model selector.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// 是否内置模型；内置模型不可删除。
    /// Whether this is a built-in model; built-ins cannot be deleted.
    #[serde(default)]
    pub builtin: bool,
}

impl AiModelConfig {
    /// 所属模块：ai / 模型条目
    /// 功能：构造一条内置模型。
    /// 输入：`id` 稳定 id；`provider_id` 服务商；`model` 模型 id；`display_name` 展示名；
    /// `context_input` / `context_output` 上下文与输出上限。
    /// 输出/副作用：启用的内置条目；不读盘。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / model entry
    /// Purpose: Build one built-in model.
    /// Inputs: `id` stable id; `provider_id` provider; `model` model id; `display_name` label;
    /// `context_input` / `context_output` context and output limits.
    /// Outputs / side effects: an enabled built-in entry; no disk access.
    /// Complexity: O(1) time and space.
    pub fn builtin(
        id: &str,
        provider_id: &str,
        model: &str,
        display_name: &str,
        context_input: u32,
        context_output: u32,
    ) -> Self {
        Self {
            id: id.to_string(),
            provider_id: provider_id.to_string(),
            display_name: display_name.to_string(),
            model: model.to_string(),
            base_url: String::new(),
            api_key: String::new(),
            context_input,
            context_output,
            tool_rounds: DEFAULT_MODEL_TOOL_ROUNDS,
            reasoning: "medium".into(),
            temperature: DEFAULT_MODEL_TEMPERATURE,
            top_p: None,
            top_k: None,
            enabled: true,
            builtin: true,
        }
    }

    /// 所属模块：ai / 模型条目
    /// 功能：返回下拉与列表使用的展示名；展示名为空时回退到模型 id。
    /// 输入：`self`。
    /// 输出/副作用：文本引用；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / model entry
    /// Purpose: Return the label used by the selector and list; falls back to the model id.
    /// Inputs: `self`.
    /// Outputs / side effects: text reference; no state change.
    /// Complexity: O(1) time and space.
    pub fn display_label(&self) -> &str {
        let name = self.display_name.trim();
        if name.is_empty() {
            self.model.trim()
        } else {
            name
        }
    }

    /// 所属模块：ai / 模型条目
    /// 功能：返回实际使用的 Base URL：条目覆盖值优先，否则使用服务商默认地址。
    /// 输入：`self`。
    /// 输出/副作用：地址文本；不修改状态。
    /// 复杂度：时间 O(地址长度)，空间 O(地址长度)。
    ///
    /// Module: ai / model entry
    /// Purpose: Return the effective base URL: the entry override first, otherwise the provider
    /// default.
    /// Inputs: `self`.
    /// Outputs / side effects: URL text; no state change.
    /// Complexity: O(URL length) time and space.
    pub fn resolved_base_url(&self) -> String {
        let custom = self.base_url.trim();
        if custom.is_empty() {
            provider(&self.provider_id).base_url.to_string()
        } else {
            custom.to_string()
        }
    }

    /// 所属模块：ai / 模型条目
    /// 功能：返回服务商展示名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(服务商数)，空间 O(1)。
    ///
    /// Module: ai / model entry
    /// Purpose: Return the provider display name.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(providers) time and O(1) space.
    pub fn provider_label(&self) -> &'static str {
        provider_label(&self.provider_id)
    }

    /// 所属模块：ai / 模型条目
    /// 功能：返回请求体差异所属的协议族。
    /// 输入：`self`。
    /// 输出/副作用：协议族枚举；不修改状态。
    /// 复杂度：时间 O(服务商数)，空间 O(1)。
    ///
    /// Module: ai / model entry
    /// Purpose: Return the protocol family controlling request-body differences.
    /// Inputs: `self`.
    /// Outputs / side effects: the family enum; no state change.
    /// Complexity: O(providers) time and O(1) space.
    pub fn family(&self) -> AiProvider {
        provider(&self.provider_id).family
    }

    /// 所属模块：ai / 模型条目
    /// 功能：判断条目是否具备发送请求的基本条件（启用、密钥、模型与地址非空）。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(文本长度)，空间 O(1)。
    ///
    /// Module: ai / model entry
    /// Purpose: Whether the entry can send requests (enabled, key, model and URL present).
    /// Inputs: `self`.
    /// Outputs / side effects: a boolean; no state change.
    /// Complexity: O(text length) time and O(1) space.
    pub fn is_ready(&self) -> bool {
        self.enabled
            && !self.api_key.trim().is_empty()
            && !self.model.trim().is_empty()
            && !self.resolved_base_url().trim().is_empty()
    }

    /// 所属模块：ai / 模型条目
    /// 功能：修剪文本并夹紧数值范围，修正手工编辑或旧版本留下的越界值。
    /// 输入：`self` 可变引用。
    /// 输出/副作用：原地修改；不读盘。
    /// 复杂度：时间 O(文本长度)，空间 O(1)。
    ///
    /// Module: ai / model entry
    /// Purpose: Trim text and clamp numeric fields left out of range by manual edits or old builds.
    /// Inputs: `self`: mutable reference.
    /// Outputs / side effects: mutates in place; no disk access.
    /// Complexity: O(text length) time and O(1) space.
    pub fn sanitize(&mut self) {
        self.id = self.id.trim().to_string();
        self.provider_id = self.provider_id.trim().to_string();
        if self.provider_id.is_empty() {
            self.provider_id = CUSTOM_PROVIDER_ID.into();
        }
        self.display_name = self.display_name.trim().to_string();
        if self.display_name.chars().count() > 64 {
            self.display_name = self.display_name.chars().take(64).collect();
        }
        self.model = self.model.trim().to_string();
        self.base_url = self.base_url.trim().to_string();
        self.api_key = self.api_key.trim().to_string();
        self.context_input = self.context_input.clamp(1_024, 4_000_000);
        self.context_output = self.context_output.clamp(256, MAX_MODEL_OUTPUT_TOKENS);
        self.tool_rounds = self.tool_rounds.clamp(1, 1_000);
        self.reasoning = AiReasoning::from_id(&self.reasoning).id().to_string();
        self.temperature = self.temperature.clamp(0.0, 2.0);
        self.top_p = self.top_p.map(|value| value.clamp(0.0, 1.0));
        self.top_k = self.top_k.map(|value| value.clamp(1, 100));
    }

    /// 所属模块：ai / 模型条目
    /// 功能：按条目构造一次补全请求；输出上限取自条目的上下文输出配置。
    /// 输入：
    /// - `messages`：完整消息列表；
    /// - `timeout`：请求超时；
    /// - `max_tokens`：本次请求的输出上限；
    /// - `json_output`：是否要求 JSON 对象输出。
    /// 输出/副作用：请求结构；不访问网络。
    /// 复杂度：时间 O(消息长度)，空间 O(消息长度)。
    ///
    /// Module: ai / model entry
    /// Purpose: Build one completion request from the entry; the output limit is passed explicitly.
    /// Inputs:
    /// - `messages`: complete message list;
    /// - `timeout`: request timeout;
    /// - `max_tokens`: output limit for this request;
    /// - `json_output`: whether to request a JSON object.
    /// Outputs / side effects: the request; no network access.
    /// Complexity: O(message length) time and space.
    pub fn build_request(
        &self,
        messages: Vec<AiMessage>,
        timeout: Duration,
        max_tokens: u32,
        json_output: bool,
    ) -> AiRequest {
        AiRequest {
            provider: self.family(),
            provider_label: self.provider_label().to_string(),
            base_url: self.resolved_base_url(),
            api_key: self.api_key.trim().to_string(),
            model: self.model.trim().to_string(),
            messages,
            temperature: self.temperature.clamp(0.0, 2.0),
            top_p: self.top_p.map(|value| value.clamp(0.0, 1.0)),
            top_k: self.top_k.map(|value| value.clamp(1, 100)),
            max_tokens: max_tokens.clamp(256, MAX_MODEL_OUTPUT_TOKENS),
            timeout,
            reasoning: AiReasoning::from_id(&self.reasoning),
            json_output,
        }
    }
}

impl Default for AiModelConfig {
    /// 所属模块：ai / 模型条目
    /// 功能：构造空的自定义条目，供表单草稿使用。
    /// 输入：无。
    /// 输出/副作用：默认条目；不读盘。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / model entry
    /// Purpose: Build an empty custom entry for form drafts.
    /// Inputs: none.
    /// Outputs / side effects: a default entry; no disk access.
    /// Complexity: O(1) time and space.
    fn default() -> Self {
        Self {
            id: String::new(),
            provider_id: CUSTOM_PROVIDER_ID.into(),
            display_name: String::new(),
            model: String::new(),
            base_url: String::new(),
            api_key: String::new(),
            context_input: DEFAULT_MODEL_CONTEXT_INPUT,
            context_output: DEFAULT_MODEL_CONTEXT_OUTPUT,
            tool_rounds: DEFAULT_MODEL_TOOL_ROUNDS,
            reasoning: "medium".into(),
            temperature: DEFAULT_MODEL_TEMPERATURE,
            top_p: None,
            top_k: None,
            enabled: true,
            builtin: false,
        }
    }
}

impl std::fmt::Debug for AiModelConfig {
    /// 所属模块：ai / 模型条目
    /// 功能：输出调试信息时隐藏 API Key，避免日志与崩溃报告泄漏密钥。
    /// 输入：`f`：格式化器。
    /// 输出/副作用：写入格式化器；不修改状态。
    /// 复杂度：时间 O(字段数)，空间 O(1)。
    ///
    /// Module: ai / model entry
    /// Purpose: Hide the API key in debug output so logs and crash reports never leak it.
    /// Inputs: `f`: formatter.
    /// Outputs / side effects: writes to the formatter; no state change.
    /// Complexity: O(fields) time and O(1) space.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiModelConfig")
            .field("id", &self.id)
            .field("provider_id", &self.provider_id)
            .field("display_name", &self.display_name)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field(
                "api_key",
                &if self.api_key.trim().is_empty() {
                    "<empty>"
                } else {
                    "***"
                },
            )
            .field("context_input", &self.context_input)
            .field("context_output", &self.context_output)
            .field("tool_rounds", &self.tool_rounds)
            .field("reasoning", &self.reasoning)
            .field("temperature", &self.temperature)
            .field("top_p", &self.top_p)
            .field("top_k", &self.top_k)
            .field("enabled", &self.enabled)
            .field("builtin", &self.builtin)
            .finish()
    }
}

/// 所属模块：ai / 模型条目
/// 功能：上下文输入窗口的 serde 默认值。
/// 输入：无。
/// 输出/副作用：默认数值；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / model entry
/// Purpose: serde default for the context input window.
/// Inputs: none.
/// Outputs / side effects: the default value; no state change.
/// Complexity: O(1) time and space.
fn default_context_input() -> u32 {
    DEFAULT_MODEL_CONTEXT_INPUT
}

/// 所属模块：ai / 模型条目
/// 功能：最大输出的 serde 默认值。
/// 输入：无。
/// 输出/副作用：默认数值；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / model entry
/// Purpose: serde default for the maximum output.
/// Inputs: none.
/// Outputs / side effects: the default value; no state change.
/// Complexity: O(1) time and space.
fn default_context_output() -> u32 {
    DEFAULT_MODEL_CONTEXT_OUTPUT
}

/// 所属模块：ai / 模型条目
/// 功能：工具调用轮数的 serde 默认值。
/// 输入：无。
/// 输出/副作用：默认数值；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / model entry
/// Purpose: serde default for tool-call rounds.
/// Inputs: none.
/// Outputs / side effects: the default value; no state change.
/// Complexity: O(1) time and space.
fn default_tool_rounds() -> u32 {
    DEFAULT_MODEL_TOOL_ROUNDS
}

/// 所属模块：ai / 模型条目
/// 功能：推理强度的 serde 默认值（中）。
/// 输入：无。
/// 输出/副作用：默认字符串；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / model entry
/// Purpose: serde default for the reasoning effort (medium).
/// Inputs: none.
/// Outputs / side effects: the default string; no state change.
/// Complexity: O(1) time and space.
fn default_reasoning() -> String {
    "medium".into()
}

/// 所属模块：ai / 模型条目
/// 功能：采样温度的 serde 默认值。
/// 输入：无。
/// 输出/副作用：默认数值；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / model entry
/// Purpose: serde default for the sampling temperature.
/// Inputs: none.
/// Outputs / side effects: the default value; no state change.
/// Complexity: O(1) time and space.
fn default_temperature() -> f32 {
    DEFAULT_MODEL_TEMPERATURE
}

/// 所属模块：ai / 模型条目
/// 功能：启用状态的 serde 默认值（启用）。
/// 输入：无。
/// 输出/副作用：默认布尔值；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ai / model entry
/// Purpose: serde default for the enabled flag (true).
/// Inputs: none.
/// Outputs / side effects: the default boolean; no state change.
/// Complexity: O(1) time and space.
fn default_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：服务商目录 id 唯一，未知 id 回退到自定义。
    /// Covers: provider ids are unique and unknown ids fall back to custom.
    #[test]
    fn provider_ids_are_unique() {
        let mut ids: Vec<&str> = PROVIDERS.iter().map(|provider| provider.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PROVIDERS.len());
        assert_eq!(provider("missing").id, CUSTOM_PROVIDER_ID);
        assert_eq!(provider("deepseek").family, AiProvider::DeepSeek);
        assert_eq!(provider("mimo").family, AiProvider::Mimo);
    }

    /// 覆盖：内置模型 id 与旧预设一致，展示名回退与地址解析正确。
    /// Covers: built-in ids match the legacy presets, and the display/URL fallbacks work.
    #[test]
    fn builtin_models_keep_legacy_ids() {
        let models = builtin_models();
        assert_eq!(models.len(), 4);
        let flash = models
            .iter()
            .find(|model| model.id == "deepseek-flash")
            .expect("deepseek flash");
        assert_eq!(flash.model, "deepseek-flash");
        assert_eq!(flash.display_label(), "DeepSeek Flash");
        assert_eq!(flash.resolved_base_url(), "https://api.deepseek.com");
        assert!(flash.builtin);
        assert!(flash.enabled);
        assert_eq!(default_model("mimo"), "mimo-v2.6-flash");
        assert_eq!(default_model("custom"), "");
    }

    /// 覆盖：模型条目可序列化往返，缺失字段使用默认值。
    /// Covers: model entries round-trip through JSON and missing fields use defaults.
    #[test]
    fn model_config_serde_roundtrip() {
        let model = builtin_models().remove(0);
        let text = serde_json::to_string(&model).expect("serialize");
        let parsed: AiModelConfig = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed.id, model.id);
        assert_eq!(parsed.context_output, DEFAULT_MODEL_CONTEXT_OUTPUT);
        let minimal: AiModelConfig = serde_json::from_str(r#"{"id":"x"}"#).expect("minimal");
        assert!(minimal.enabled);
        assert_eq!(minimal.provider_id, "");
        assert_eq!(minimal.temperature, DEFAULT_MODEL_TEMPERATURE);
    }

    /// 覆盖：请求构造携带采样参数、JSON 输出与输出上限。
    /// Covers: request construction carries sampling, JSON output and the output limit.
    #[test]
    fn build_request_carries_sampling() {
        let mut model = builtin_models().remove(0);
        model.api_key = "sk-test".into();
        model.top_p = Some(0.9);
        model.top_k = Some(40);
        let request = model.build_request(Vec::new(), Duration::from_secs(30), 64_000, true);
        assert_eq!(request.provider, AiProvider::DeepSeek);
        assert_eq!(request.provider_label, "DeepSeek");
        assert_eq!(request.max_tokens, 64_000);
        assert_eq!(request.top_p, Some(0.9));
        assert_eq!(request.top_k, Some(40));
        assert!(request.json_output);
        assert_eq!(request.base_url, "https://api.deepseek.com");
    }

    /// 覆盖：越界数值会被夹紧，密钥在 Debug 中隐藏。
    /// Covers: out-of-range values are clamped and the key is hidden in Debug.
    #[test]
    fn sanitize_clamps_and_redacts() {
        let mut model = AiModelConfig::default();
        model.model = "  demo  ".into();
        model.api_key = "  secret  ".into();
        model.temperature = 9.0;
        model.top_p = Some(-1.0);
        model.top_k = Some(999);
        model.context_output = 10;
        model.display_name = "x".repeat(80);
        model.sanitize();
        assert_eq!(model.model, "demo");
        assert_eq!(model.api_key, "secret");
        assert_eq!(model.temperature, 2.0);
        assert_eq!(model.top_p, Some(0.0));
        assert_eq!(model.top_k, Some(100));
        assert_eq!(model.context_output, 256);
        assert_eq!(model.display_name.chars().count(), 64);
        assert!(!format!("{model:?}").contains("secret"));
    }
}
