//! 所属模块：ai / AI 编曲领域
//! 文件用途：
//! - 定义 AI 编曲的运行时框架（`AiRuntime` + `AiBackend` 后端）与模型预设。
//! - 定义会话消息、请求、响应与错误文本；网络请求只在后台线程调用。
//! - 聚合可编辑资料库（智能体/基础技能/技巧/乐器资料）、工程提示词、编曲计划、流式计划提取、MIDI 附件与联网检索子模块。
//!
//! 函数清单：
//! - `AiProvider::id` / `label` / `protocol` / `default_base_url` / `default_model` / `preset`：服务商信息与预设。
//! - `AiModelPreset` / `model_presets` / `find_preset`：对话下方“模型”下拉的选项。
//! - `AiRole::as_str`：消息角色到接口字段的映射。
//! - `AiRuntime::standard` / `complete` / `stream`：按协议分派到后端并返回助手回复。
//! - `AiStreamEvent`：流式补全过程中推送的思考与内容增量。
//! - `truncate_for_error`：把错误响应体裁剪成可读文本。
//!
//! Module: ai / AI composition domain
//! Purpose:
//! - Define the AI-composition runtime framework (`AiRuntime` + `AiBackend` backends) and model presets.
//! - Define chat messages, requests, responses and error text; network calls only run on worker threads.
//! - Aggregate embedded skills (markdown), prompt building, composition plans, streaming plan
//!   extraction and MIDI attachment parsing.
//!
//! Functions:
//! - `AiProvider::id` / `label` / `protocol` / `default_base_url` / `default_model` / `preset`: provider data.
//! - `AiModelPreset` / `model_presets` / `find_preset`: options of the model selector below the chat input.
//! - `AiRole::as_str`: message role to API field mapping.
//! - `AiRuntime::standard` / `complete`: dispatch to a backend by protocol and return the assistant reply.
//! - `truncate_for_error`: trim an error response body into readable text.

mod backend;
mod backends;
pub mod library;
pub mod midi_file;
pub mod plan;
pub mod prompt;
pub mod search;
pub mod skills;
pub mod stream;

pub use backend::{AiBackend, AiBackendInfo};
pub use library::{AiAgentProfile, AiInstrumentDoc, AiLibrary, AiSkillDoc, make_id, unique_id};
pub use plan::{AiApplySummary, AiOperation, AiPlan, AiPluginInfo, AiPluginKind};
pub use search::AiSearchConfig;
pub use skills::{AiSkill, AiTechnique, instrument_catalog};
pub use stream::PlanStream;

use serde::Serialize;
use std::time::Duration;

/// AI 服务商标识。
/// AI provider identifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiProvider {
    DeepSeek,
    Mimo,
    /// 任意 OpenAI 兼容服务（自建网关、其它厂商）。
    /// Any OpenAI-compatible service (self-hosted gateway or another vendor).
    Custom,
}

impl AiProvider {
    /// 所属模块：ai / 服务商
    /// 功能：返回持久化标识（写入全局设置）。
    /// 输入：`self`。
    /// 输出/副作用：静态字符串；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / provider
    /// Purpose: Return the persisted id written into global settings.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn id(self) -> &'static str {
        match self {
            AiProvider::DeepSeek => "deepseek",
            AiProvider::Mimo => "mimo",
            AiProvider::Custom => "custom",
        }
    }

    /// 所属模块：ai / 服务商
    /// 功能：返回界面展示名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / provider
    /// Purpose: Return the display name.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            AiProvider::DeepSeek => "DeepSeek",
            AiProvider::Mimo => "MiMo",
            AiProvider::Custom => "自定义",
        }
    }

    /// 所属模块：ai / 服务商
    /// 功能：返回接口协议标识；三种服务商都使用 OpenAI 兼容的对话补全协议。
    /// 输入：`self`。
    /// 输出/副作用：协议标识；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / provider
    /// Purpose: Return the protocol id; all three providers use OpenAI-compatible chat completions.
    /// Inputs: `self`.
    /// Outputs / side effects: protocol id; no state change.
    /// Complexity: O(1) time and space.
    pub fn protocol(self) -> &'static str {
        "openai-compatible"
    }

    /// 所属模块：ai / 服务商
    /// 功能：返回默认 Base URL（可被全局设置覆盖）。
    /// 输入：`self`。
    /// 输出/副作用：静态地址；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / provider
    /// Purpose: Return the default base URL (the global settings may override it).
    /// Inputs: `self`.
    /// Outputs / side effects: static URL; no state change.
    /// Complexity: O(1) time and space.
    pub fn default_base_url(self) -> &'static str {
        match self {
            AiProvider::DeepSeek => "https://api.deepseek.com",
            AiProvider::Mimo => "https://api.xiaomimimo.com/v1",
            AiProvider::Custom => "",
        }
    }

    /// 所属模块：ai / 服务商
    /// 功能：返回默认模型名。
    /// 输入：`self`。
    /// 输出/副作用：静态模型名；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / provider
    /// Purpose: Return the default model id.
    /// Inputs: `self`.
    /// Outputs / side effects: static model id; no state change.
    /// Complexity: O(1) time and space.
    pub fn default_model(self) -> &'static str {
        match self {
            AiProvider::DeepSeek => "deepseek-flash",
            AiProvider::Mimo => "mimo-v2.6-flash",
            AiProvider::Custom => "",
        }
    }

    /// 所属模块：ai / 服务商
    /// 功能：按持久化标识解析服务商；未知标识回退到自定义。
    /// 输入：`id`：`deepseek` / `mimo` / `custom`。
    /// 输出/副作用：服务商枚举；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / provider
    /// Purpose: Resolve a provider from its persisted id; unknown ids fall back to custom.
    /// Inputs: `id`: `deepseek` / `mimo` / `custom`.
    /// Outputs / side effects: provider enum; no state change.
    /// Complexity: O(1) time and space.
    pub fn from_id(id: &str) -> Self {
        match id {
            "deepseek" => AiProvider::DeepSeek,
            "mimo" => AiProvider::Mimo,
            _ => AiProvider::Custom,
        }
    }
}

/// 对话下方“模型”下拉中的一个预设。
/// One preset in the model selector below the chat input.
#[derive(Clone, Copy, Debug)]
pub struct AiModelPreset {
    /// 稳定 id（写入全局设置）。
    /// Stable id persisted in global settings.
    pub id: &'static str,
    /// 下拉展示文本。
    /// Text shown in the selector.
    pub label: &'static str,
    pub provider: AiProvider,
    pub base_url: &'static str,
    pub model: &'static str,
}

/// 全部内置模型预设：两个 DeepSeek 模型、两个 MiMo 模型与自定义入口。
/// Every built-in model preset: two DeepSeek models, two MiMo models and a custom entry.
pub const MODEL_PRESETS: [AiModelPreset; 5] = [
    AiModelPreset {
        id: "deepseek-flash",
        label: "DeepSeek Flash",
        provider: AiProvider::DeepSeek,
        base_url: "https://api.deepseek.com",
        model: "deepseek-flash",
    },
    AiModelPreset {
        id: "deepseek-v4-pro",
        label: "DeepSeek V4 Pro",
        provider: AiProvider::DeepSeek,
        base_url: "https://api.deepseek.com",
        model: "deepseek-v4-pro",
    },
    AiModelPreset {
        id: "mimo-v2.6-flash",
        label: "MiMo V2.6 Flash",
        provider: AiProvider::Mimo,
        base_url: "https://api.xiaomimimo.com/v1",
        model: "mimo-v2.6-flash",
    },
    AiModelPreset {
        id: "mimo-v2.6-pro",
        label: "MiMo V2.6 Pro",
        provider: AiProvider::Mimo,
        base_url: "https://api.xiaomimimo.com/v1",
        model: "mimo-v2.6-pro",
    },
    AiModelPreset {
        id: "custom",
        label: "自定义",
        provider: AiProvider::Custom,
        base_url: "",
        model: "",
    },
];

/// 所属模块：ai / 模型预设
/// 功能：按 id 查找内置预设。
/// 输入：`id`：预设 id。
/// 输出/副作用：预设或 `None`；不修改状态。
/// 复杂度：时间 O(预设数)，空间 O(1)。
///
/// Module: ai / model presets
/// Purpose: Find a built-in preset by id.
/// Inputs: `id`: preset id.
/// Outputs / side effects: the preset or `None`; no state change.
/// Complexity: O(presets) time and O(1) space.
pub fn find_preset(id: &str) -> Option<AiModelPreset> {
    MODEL_PRESETS.iter().copied().find(|preset| preset.id == id)
}

/// 对话消息角色。
/// Chat message roles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiRole {
    System,
    User,
    Assistant,
}

impl AiRole {
    /// 所属模块：ai / 消息
    /// 功能：返回 OpenAI 兼容接口使用的角色字段。
    /// 输入：`self`。
    /// 输出/副作用：静态字符串；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / message
    /// Purpose: Return the role field used by the OpenAI-compatible API.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn as_str(self) -> &'static str {
        match self {
            AiRole::System => "system",
            AiRole::User => "user",
            AiRole::Assistant => "assistant",
        }
    }
}

/// 一条对话消息（不含推理内容，界面与请求共用）。
/// One chat message (no reasoning content; shared by the UI and the request).
#[derive(Clone, Debug)]
pub struct AiMessage {
    pub role: AiRole,
    pub content: String,
}

/// 推理强度：关闭思考或使用低/中/高推理预算。
/// Reasoning strength: thinking off, or low/medium/high effort.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AiReasoning {
    Off,
    Low,
    #[default]
    Medium,
    High,
}

impl AiReasoning {
    pub const ALL: [AiReasoning; 4] = [
        AiReasoning::Off,
        AiReasoning::Low,
        AiReasoning::Medium,
        AiReasoning::High,
    ];

    /// 所属模块：ai / 推理强度
    /// 功能：返回持久化标识。
    /// 输入：`self`。
    /// 输出/副作用：静态字符串；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / reasoning
    /// Purpose: Return the persisted id.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn id(self) -> &'static str {
        match self {
            AiReasoning::Off => "off",
            AiReasoning::Low => "low",
            AiReasoning::Medium => "medium",
            AiReasoning::High => "high",
        }
    }

    /// 所属模块：ai / 推理强度
    /// 功能：返回界面展示名称。
    /// 输入：`self`。
    /// 输出/副作用：静态文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / reasoning
    /// Purpose: Return the display label.
    /// Inputs: `self`.
    /// Outputs / side effects: static text; no state change.
    /// Complexity: O(1) time and space.
    pub fn label(self) -> &'static str {
        match self {
            AiReasoning::Off => "关闭",
            AiReasoning::Low => "低",
            AiReasoning::Medium => "中",
            AiReasoning::High => "高",
        }
    }

    /// 所属模块：ai / 推理强度
    /// 功能：返回服务商使用的 `reasoning_effort` 值；关闭时返回 `None`。
    /// 输入：`self`。
    /// 输出/副作用：静态字符串或 `None`；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / reasoning
    /// Purpose: Return the provider `reasoning_effort` value; `None` when thinking is off.
    /// Inputs: `self`.
    /// Outputs / side effects: static text or `None`; no state change.
    /// Complexity: O(1) time and space.
    pub fn effort(self) -> Option<&'static str> {
        match self {
            AiReasoning::Off => None,
            AiReasoning::Low => Some("low"),
            AiReasoning::Medium => Some("medium"),
            AiReasoning::High => Some("high"),
        }
    }

    /// 所属模块：ai / 推理强度
    /// 功能：按持久化标识解析；未知值回退到中。
    /// 输入：`id`：`off` / `low` / `medium` / `high`。
    /// 输出/副作用：枚举；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / reasoning
    /// Purpose: Resolve a persisted id; unknown values fall back to medium.
    /// Inputs: `id`: `off` / `low` / `medium` / `high`.
    /// Outputs / side effects: enum; no state change.
    /// Complexity: O(1) time and space.
    pub fn from_id(id: &str) -> Self {
        match id {
            "off" => AiReasoning::Off,
            "low" => AiReasoning::Low,
            "high" => AiReasoning::High,
            _ => AiReasoning::Medium,
        }
    }
}

/// 一次补全请求：服务商、密钥、模型与完整消息列表。
/// One completion request: provider, key, model and the complete message list.
#[derive(Clone, Debug)]
pub struct AiRequest {
    pub provider: AiProvider,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub messages: Vec<AiMessage>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub timeout: Duration,
    pub reasoning: AiReasoning,
}

/// 流式补全过程中推送的增量。
/// Incremental event pushed while a completion is streaming.
#[derive(Clone, Debug)]
pub enum AiStreamEvent {
    /// 新增的思考内容（服务商的 `reasoning_content`）。
    /// Newly received reasoning text (the provider's `reasoning_content`).
    Reasoning(String),
    /// 新增的回复内容（最终仍是完整 JSON 计划的一部分）。
    /// Newly received reply text (still part of the complete JSON plan).
    Content(String),
}

/// 补全结果。
/// Completion result.
#[derive(Clone, Debug, Default)]
pub struct AiResponse {
    pub content: String,
    /// 思考过程（`reasoning_content`）；关闭思考时为空。
    /// Reasoning trace (`reasoning_content`); empty when thinking is off.
    pub reasoning: Option<String>,
    /// 输出是否因 `max_tokens` 被截断。
    /// Whether the output was truncated by `max_tokens`.
    pub truncated: bool,
}

/// 请求体的公开视图：仅供后端序列化，避免后端直接拼装消息结构。
/// Public request-body view used by backends, so they never assemble message structure themselves.
#[derive(Serialize)]
pub(crate) struct RequestBody<'a> {
    pub model: &'a str,
    pub messages: Vec<BodyMessage<'a>>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub stream: bool,
    pub response_format: ResponseFormat,
}

#[derive(Serialize)]
pub(crate) struct BodyMessage<'a> {
    pub role: &'a str,
    pub content: &'a str,
}

#[derive(Serialize)]
pub(crate) struct ResponseFormat {
    #[serde(rename = "type")]
    pub kind: &'static str,
}

/// AI 编曲运行时：持有后端注册表，应用层只调用这里。
/// AI-composition runtime: owns the backend registry; the app layer only calls this.
pub struct AiRuntime {
    backends: Vec<Box<dyn AiBackend>>,
}

impl AiRuntime {
    /// 所属模块：ai / 运行时
    /// 功能：构造带全部标准后端的运行时（当前只有 OpenAI 兼容后端）。
    /// 输入：无。
    /// 输出/副作用：返回运行时；不访问网络。
    /// 复杂度：时间 O(后端数)，空间 O(后端数)。
    ///
    /// Module: ai / runtime
    /// Purpose: Build the runtime with every standard backend (only the OpenAI-compatible one today).
    /// Inputs: none.
    /// Outputs / side effects: the runtime; no network access.
    /// Complexity: O(backends) time and space.
    pub fn standard() -> Self {
        Self {
            backends: backends::standard(),
        }
    }

    /// 所属模块：ai / 运行时
    /// 功能：按请求协议选择后端并执行一次对话补全；网络错误转成中文原因。
    /// 输入：`request`：服务商、模型、消息与超时。
    /// 输出/副作用：返回助手回复或错误；阻塞调用，只能在后台线程使用。
    /// 复杂度：时间 O(网络延迟 + 响应大小)，空间 O(响应大小)。
    ///
    /// Module: ai / runtime
    /// Purpose: Pick a backend by the request protocol and run one chat completion; network errors
    /// become Chinese reasons.
    /// Inputs: `request`: provider, model, messages and timeout.
    /// Outputs / side effects: assistant reply or error; blocking, so it must run on a worker thread.
    /// Complexity: O(network latency + response size) time and O(response size) space.
    pub fn complete(&self, request: &AiRequest) -> Result<AiResponse, String> {
        let protocol = request.provider.protocol();
        let backend = self
            .backends
            .iter()
            .find(|backend| backend.info().id == protocol)
            .ok_or_else(|| format!("没有可用的 AI 协议后端：{protocol}"))?;
        backend.complete(request)
    }

    /// 所属模块：ai / 运行时
    /// 功能：按请求协议选择后端并执行一次流式对话补全；每个增量通过 `sink` 立即回传。
    /// 输入：
    /// - `request`：服务商、模型、消息与超时；
    /// - `sink`：接收思考/内容增量的回调（在后台线程同步调用）。
    /// 输出/副作用：返回完整助手回复或错误；阻塞调用，只能在后台线程使用。
    /// 复杂度：时间 O(网络延迟 + 响应大小)，空间 O(响应大小)。
    ///
    /// Module: ai / runtime
    /// Purpose: Pick a backend by the request protocol and run one streaming completion; every
    /// increment is forwarded through `sink` immediately.
    /// Inputs:
    /// - `request`: provider, model, messages and timeout;
    /// - `sink`: callback receiving reasoning/content increments (called synchronously on the worker).
    /// Outputs / side effects: the complete assistant reply or an error; blocking, so it must run on a
    /// worker thread.
    /// Complexity: O(network latency + response size) time and O(response size) space.
    pub fn stream(
        &self,
        request: &AiRequest,
        sink: &mut dyn FnMut(AiStreamEvent),
    ) -> Result<AiResponse, String> {
        let protocol = request.provider.protocol();
        let backend = self
            .backends
            .iter()
            .find(|backend| backend.info().id == protocol)
            .ok_or_else(|| format!("没有可用的 AI 协议后端：{protocol}"))?;
        backend.stream(request, sink)
    }
}

/// 所属模块：ai / 错误提示
/// 功能：把接口返回的长错误体裁剪到可读长度，避免状态栏被整页 HTML 占满。
/// 输入：`text`：原始响应体；`limit`：保留的最大字符数。
/// 输出/副作用：裁剪后的文本；不修改输入。
/// 复杂度：时间 O(limit)，空间 O(limit)。
///
/// Module: ai / error text
/// Purpose: Trim a long error body so the status bar is not filled with an HTML page.
/// Inputs: `text`: raw response body; `limit`: maximum retained characters.
/// Outputs / side effects: trimmed text; the input is unchanged.
/// Complexity: O(limit) time and space.
pub fn truncate_for_error(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    let mut result: String = trimmed.chars().take(limit).collect();
    if trimmed.chars().count() > limit {
        result.push('…');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：预设 id 与模型名一一对应，未知 id 返回 `None`。
    /// Covers: preset ids map to their model names and unknown ids return `None`.
    #[test]
    fn model_presets_resolve_by_id() {
        let preset = find_preset("deepseek-flash").expect("preset");
        assert_eq!(preset.model, "deepseek-flash");
        assert_eq!(preset.provider, AiProvider::DeepSeek);
        assert!(find_preset("missing").is_none());
    }

    /// 覆盖：服务商默认地址与协议。
    /// Covers: provider default URLs and protocol.
    #[test]
    fn providers_expose_defaults() {
        assert_eq!(
            AiProvider::DeepSeek.default_base_url(),
            "https://api.deepseek.com"
        );
        assert_eq!(
            AiProvider::Mimo.default_base_url(),
            "https://api.xiaomimimo.com/v1"
        );
        assert_eq!(AiProvider::Mimo.protocol(), "openai-compatible");
        assert_eq!(AiProvider::from_id("mimo"), AiProvider::Mimo);
        assert_eq!(AiProvider::from_id("unknown"), AiProvider::Custom);
    }

    /// 覆盖：错误文本按字符裁剪并附加省略号。
    /// Covers: error text is trimmed by characters and gets an ellipsis.
    #[test]
    fn error_text_is_trimmed() {
        assert_eq!(truncate_for_error("abcdef", 3), "abc…");
        assert_eq!(truncate_for_error("abc", 3), "abc");
    }

    /// 覆盖：推理强度 id 往返，关闭时没有 effort。
    /// Covers: reasoning ids round-trip and "off" has no effort.
    #[test]
    fn reasoning_ids_round_trip() {
        for reasoning in AiReasoning::ALL {
            assert_eq!(AiReasoning::from_id(reasoning.id()), reasoning);
        }
        assert_eq!(AiReasoning::from_id("unknown"), AiReasoning::Medium);
        assert!(AiReasoning::Off.effort().is_none());
        assert_eq!(AiReasoning::High.effort(), Some("high"));
    }
}
