//! 所属模块：ai / OpenAI 兼容后端
//! 文件用途：
//! - 通过 HTTPS 调用 OpenAI 兼容的 `/chat/completions` 接口（DeepSeek / MiMo / 自定义网关）。
//! - 统一处理鉴权头、超时、HTTP 错误与响应体解析。
//!
//! 函数清单：
//! - `OpenAiCompatible::info`：返回协议描述。
//! - `OpenAiCompatible::complete`：构造请求、发送并解析助手回复。
//! - `OpenAiCompatible::stream`：SSE 流式读取并推送思考/内容增量。
//! - `build_payload` / `fallback_complete`：请求体构造与流式不可用时的整体回退。
//! - `endpoint`：把 Base URL 规范化为对话补全地址。
//! - `error_from_body`：从错误响应体提取可读信息。
//!
//! Module: ai / OpenAI-compatible backend
//! Purpose:
//! - Call an OpenAI-compatible `/chat/completions` endpoint over HTTPS (DeepSeek / MiMo / custom).
//! - Handle auth headers, timeouts, HTTP errors and response parsing uniformly.
//!
//! Functions:
//! - `OpenAiCompatible::info`: return the protocol descriptor.
//! - `OpenAiCompatible::complete`: build the request, send it and parse the assistant reply.
//! - `OpenAiCompatible::stream`: read SSE and forward reasoning/content increments.
//! - `build_payload` / `fallback_complete`: request-body construction and whole-reply fallback.
//! - `endpoint`: normalise a base URL into the chat-completion URL.
//! - `error_from_body`: extract readable text from an error response body.

use super::super::{
    AiBackend, AiBackendInfo, AiProvider, AiRequest, AiResponse, AiStreamEvent, BodyMessage,
    RequestBody, ResponseFormat, truncate_for_error,
};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};

/// OpenAI 兼容后端（无状态）。
/// Stateless OpenAI-compatible backend.
pub(super) struct OpenAiCompatible;

impl AiBackend for OpenAiCompatible {
    /// 所属模块：ai / OpenAI 兼容后端
    /// 功能：返回协议标识与展示名。
    /// 输入：`self`。
    /// 输出/副作用：能力描述；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / OpenAI-compatible backend
    /// Purpose: Return the protocol id and display name.
    /// Inputs: `self`.
    /// Outputs / side effects: descriptor; no state change.
    /// Complexity: O(1) time and space.
    fn info(&self) -> AiBackendInfo {
        AiBackendInfo {
            id: "openai-compatible",
            label: "OpenAI 兼容接口",
        }
    }

    /// 所属模块：ai / OpenAI 兼容后端
    /// 功能：发送一次对话补全请求并解析 `choices[0].message.content`；HTTP 错误统一转为中文原因。
    /// 输入：`request`：服务商、Base URL、密钥、模型、消息、温度、输出上限与超时。
    /// 输出/副作用：助手回复或错误；阻塞调用，必须在后台线程执行。
    /// 复杂度：时间 O(网络延迟 + 响应大小)，空间 O(响应大小)。
    ///
    /// Module: ai / OpenAI-compatible backend
    /// Purpose: Send one chat completion and parse `choices[0].message.content`; HTTP errors become
    /// Chinese reasons.
    /// Inputs: `request`: provider, base URL, key, model, messages, temperature, output limit and timeout.
    /// Outputs / side effects: assistant reply or error; blocking, so it must run on a worker thread.
    /// Complexity: O(network latency + response size) time and O(response size) space.
    fn complete(&self, request: &AiRequest) -> Result<AiResponse, String> {
        if request.api_key.trim().is_empty() {
            return Err(format!(
                "尚未配置 {} 的 API Key，请在 设置 → 模型中填写",
                request.provider_label
            ));
        }
        if request.model.trim().is_empty() {
            return Err("尚未选择 AI 模型".into());
        }
        let url = endpoint(&request.base_url)?;
        let payload_text = build_payload(request, false)?;
        let agent = ureq::AgentBuilder::new().timeout(request.timeout).build();
        // 同时发送 Bearer 与 api-key：DeepSeek 使用前者，MiMo 文档示例使用后者，两个头并存互不影响。
        // Send both Bearer and api-key: DeepSeek uses the former and MiMo's examples use the latter.
        let response = agent
            .post(&url)
            .set("Content-Type", "application/json")
            .set("Accept", "application/json")
            .set(
                "Authorization",
                &format!("Bearer {}", request.api_key.trim()),
            )
            .set("api-key", request.api_key.trim())
            .send_string(&payload_text);
        let (status, text) = match response {
            Ok(response) => {
                let status = response.status();
                (
                    status,
                    response.into_string().map_err(|error| error.to_string()),
                )
            }
            Err(ureq::Error::Status(status, response)) => (
                status,
                response.into_string().map_err(|error| error.to_string()),
            ),
            Err(ureq::Error::Transport(error)) => {
                return Err(format!("AI 网络请求失败：{error}"));
            }
        };
        let text = text.map_err(|error| format!("读取 AI 响应失败：{error}"))?;
        if !(200..300).contains(&status) {
            return Err(format!(
                "AI 接口返回 HTTP {status}：{}",
                error_from_body(&text)
            ));
        }
        parse_response(&text)
    }

    /// 所属模块：ai / OpenAI 兼容后端
    /// 功能：用 `stream: true` 请求 SSE，边读边把思考与内容增量交给 `sink`；
    /// 网关拒绝流式时自动退回一次普通补全。
    /// 输入：`request`：服务商、Base URL、密钥、模型、消息、温度、输出上限与超时；
    /// `sink`：增量回调。
    /// 输出/副作用：完整助手回复或错误；阻塞调用，必须在后台线程执行。
    /// 复杂度：时间 O(网络延迟 + 响应大小)，空间 O(响应大小)。
    ///
    /// Module: ai / OpenAI-compatible backend
    /// Purpose: Request SSE with `stream: true` and forward reasoning/content increments to `sink`
    /// while reading; fall back to one non-streaming completion when a gateway rejects streaming.
    /// Inputs: `request`: provider, base URL, key, model, messages, temperature, output limit and
    /// timeout; `sink`: increment callback.
    /// Outputs / side effects: the complete assistant reply or an error; blocking, so it must run on a
    /// worker thread.
    /// Complexity: O(network latency + response size) time and O(response size) space.
    fn stream(
        &self,
        request: &AiRequest,
        sink: &mut dyn FnMut(AiStreamEvent),
    ) -> Result<AiResponse, String> {
        if request.api_key.trim().is_empty() {
            return Err(format!(
                "尚未配置 {} 的 API Key，请在 设置 → 模型中填写",
                request.provider_label
            ));
        }
        if request.model.trim().is_empty() {
            return Err("尚未选择 AI 模型".into());
        }
        let url = endpoint(&request.base_url)?;
        let payload_text = build_payload(request, true)?;
        // 流式响应可能持续较久：使用读超时而不是整体超时，只要持续有数据就不会被截断。
        // Streaming can last long: use a read timeout instead of an overall timeout so a steady
        // stream is never cut off.
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(request.timeout)
            .timeout_read(request.timeout)
            .build();
        let response = match agent
            .post(&url)
            .set("Content-Type", "application/json")
            .set("Accept", "text/event-stream")
            .set(
                "Authorization",
                &format!("Bearer {}", request.api_key.trim()),
            )
            .set("api-key", request.api_key.trim())
            .send_string(&payload_text)
        {
            Ok(response) => response,
            Err(ureq::Error::Status(_, _)) => {
                // 部分网关不接受 `stream` 或 `response_format` 组合：退回普通补全并整体推送。
                // Some gateways reject `stream` or its combination with `response_format`: fall back
                // to a plain completion and push the whole reply at once.
                return fallback_complete(self, request, sink);
            }
            Err(ureq::Error::Transport(error)) => {
                return Err(format!("AI 网络请求失败：{error}"));
            }
        };
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut truncated = false;
        let mut reader = BufReader::new(response.into_reader());
        let mut line = String::new();
        loop {
            line.clear();
            let read = reader
                .read_line(&mut line)
                .map_err(|error| format!("读取 AI 流式响应失败：{error}"))?;
            if read == 0 {
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Some(data) = trimmed.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                break;
            }
            let value: Value = match serde_json::from_str(data) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if let Some(error) = value.get("error") {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("未知错误");
                return Err(format!("AI 接口错误：{}", truncate_for_error(message, 400)));
            }
            let Some(choice) = value
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| choices.first())
            else {
                continue;
            };
            if let Some(delta) = choice.get("delta") {
                if let Some(text) = delta
                    .get("reasoning_content")
                    .or_else(|| delta.get("reasoning"))
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                {
                    reasoning.push_str(text);
                    sink(AiStreamEvent::Reasoning(text.to_string()));
                }
                if let Some(text) = delta
                    .get("content")
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                {
                    content.push_str(text);
                    sink(AiStreamEvent::Content(text.to_string()));
                }
            }
            if choice.get("finish_reason").and_then(Value::as_str) == Some("length") {
                truncated = true;
            }
        }
        if content.trim().is_empty() {
            if reasoning.trim().is_empty() {
                // 服务端可能忽略了 `stream` 并直接返回普通 JSON；再请求一次以兼容。
                // The server may have ignored `stream` and returned plain JSON; request once more.
                return fallback_complete(self, request, sink);
            }
            return Err("AI 返回了空内容，请重试或更换模型".into());
        }
        Ok(AiResponse {
            content,
            reasoning: (!reasoning.trim().is_empty()).then_some(reasoning),
            truncated,
        })
    }
}

/// 所属模块：ai / OpenAI 兼容后端 / 流式回退
/// 功能：流式不可用时执行一次普通补全，并把整段思考与内容作为单次增量推送。
/// 输入：`backend` 后端；`request` 请求；`sink` 增量回调。
/// 输出/副作用：完整回复或错误；阻塞调用。
/// 复杂度：时间 O(网络延迟 + 响应大小)，空间 O(响应大小)。
///
/// Module: ai / OpenAI-compatible backend / streaming fallback
/// Purpose: Run one plain completion when streaming is unavailable and push the whole reasoning and
/// content as single increments.
/// Inputs: `backend` the backend; `request` the request; `sink` the increment callback.
/// Outputs / side effects: the complete reply or an error; blocking.
/// Complexity: O(network latency + response size) time and space.
fn fallback_complete(
    backend: &OpenAiCompatible,
    request: &AiRequest,
    sink: &mut dyn FnMut(AiStreamEvent),
) -> Result<AiResponse, String> {
    let response = backend.complete(request)?;
    if let Some(reasoning) = response
        .reasoning
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        sink(AiStreamEvent::Reasoning(reasoning.to_string()));
    }
    if !response.content.is_empty() {
        sink(AiStreamEvent::Content(response.content.clone()));
    }
    Ok(response)
}

/// 所属模块：ai / OpenAI 兼容后端 / 请求体
/// 功能：构造对话补全请求体：消息、采样参数、JSON 输出、思考开关与推理强度。
/// 输入：`request` 请求；`stream` 是否请求 SSE。
/// 输出/副作用：序列化后的 JSON 文本或中文错误；不修改请求。
/// 复杂度：时间 O(消息长度)，空间 O(消息长度)。
///
/// Module: ai / OpenAI-compatible backend / payload
/// Purpose: Build the chat-completion payload: messages, sampling parameters, JSON output, thinking
/// switch and reasoning effort.
/// Inputs: `request` the request; `stream` whether to ask for SSE.
/// Outputs / side effects: serialised JSON text or a Chinese error; the request is unchanged.
/// Complexity: O(message length) time and space.
fn build_payload(request: &AiRequest, stream: bool) -> Result<String, String> {
    let body_messages: Vec<BodyMessage> = request
        .messages
        .iter()
        .map(|message| BodyMessage {
            role: message.role.as_str(),
            content: message.content.as_str(),
        })
        .collect();
    let body = RequestBody {
        model: &request.model,
        messages: body_messages,
        temperature: request.temperature.clamp(0.0, 2.0),
        top_p: request.top_p.map(|value| value.clamp(0.0, 1.0)),
        top_k: request.top_k.map(|value| value.clamp(1, 100)),
        max_tokens: request.max_tokens.max(256),
        stream,
        response_format: request.json_output.then_some(ResponseFormat {
            kind: "json_object",
        }),
    };
    let mut payload =
        serde_json::to_value(&body).map_err(|error| format!("序列化 AI 请求失败：{error}"))?;
    // DeepSeek 与 MiMo 的思考模式由 `thinking.type` 控制；DeepSeek 另接受
    // `reasoning_effort`（low / medium / high），用于“推理强度”选择。
    // DeepSeek and MiMo control thinking through `thinking.type`; DeepSeek also accepts
    // `reasoning_effort` (low / medium / high) for the reasoning selector.
    if matches!(request.provider, AiProvider::DeepSeek | AiProvider::Mimo) {
        if let Some(effort) = request.reasoning.effort() {
            payload["thinking"] = json!({ "type": "enabled" });
            if request.provider == AiProvider::DeepSeek {
                payload["reasoning_effort"] = json!(effort);
            }
        } else {
            payload["thinking"] = json!({ "type": "disabled" });
        }
    }
    // MiMo 的 OpenAI 兼容示例使用 `max_completion_tokens`；DeepSeek 与通用网关使用 `max_tokens`。
    // MiMo's OpenAI-compatible examples use `max_completion_tokens`; DeepSeek and generic
    // gateways use `max_tokens`.
    if request.provider == AiProvider::Mimo {
        if let Some(object) = payload.as_object_mut() {
            if let Some(value) = object.remove("max_tokens") {
                object.insert("max_completion_tokens".into(), value);
            }
        }
    }
    Ok(payload.to_string())
}

/// 所属模块：ai / OpenAI 兼容后端 / 地址
/// 功能：把 Base URL 规范化为对话补全地址：已以 `/chat/completions` 结尾时原样使用，
/// 否则在末尾追加该路径。
/// 输入：`base_url`：DeepSeek / MiMo / 自定义网关的 Base URL。
/// 输出/副作用：完整地址或中文错误；不修改输入。
/// 复杂度：时间 O(地址长度)，空间 O(地址长度)。
///
/// Module: ai / OpenAI-compatible backend / URL
/// Purpose: Normalise a base URL into the chat-completion URL: keep it when it already ends with
/// `/chat/completions`, otherwise append that path.
/// Inputs: `base_url`: DeepSeek / MiMo / custom gateway base URL.
/// Outputs / side effects: full URL or a Chinese error; the input is unchanged.
/// Complexity: O(URL length) time and space.
fn endpoint(base_url: &str) -> Result<String, String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("尚未配置 AI 接口地址，请在 设置 → 模型中填写 Base URL".into());
    }
    if !base.starts_with("https://") && !base.starts_with("http://") {
        return Err("AI 接口地址必须以 http:// 或 https:// 开头".into());
    }
    if base.ends_with("/chat/completions") {
        Ok(base.to_string())
    } else {
        Ok(format!("{base}/chat/completions"))
    }
}

/// 所属模块：ai / OpenAI 兼容后端 / 错误
/// 功能：从错误响应体提取 `error.message`，缺失时退回裁剪后的原文。
/// 输入：`text`：接口返回的错误响应体。
/// 输出/副作用：可读错误文本；不修改输入。
/// 复杂度：时间 O(响应体长度)，空间 O(响应体长度)。
///
/// Module: ai / OpenAI-compatible backend / error
/// Purpose: Extract `error.message` from an error body; fall back to trimmed raw text.
/// Inputs: `text`: error body returned by the API.
/// Outputs / side effects: readable error text; the input is unchanged.
/// Complexity: O(body length) time and space.
fn error_from_body(text: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        if let Some(message) = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
        {
            return truncate_for_error(message, 400);
        }
        if let Some(message) = value.get("message").and_then(Value::as_str) {
            return truncate_for_error(message, 400);
        }
    }
    truncate_for_error(text, 400)
}

/// 所属模块：ai / OpenAI 兼容后端 / 解析
/// 功能：解析补全响应，取出首条助手消息与截断标记。
/// 输入：`text`：成功响应的 JSON 文本。
/// 输出/副作用：`AiResponse` 或中文错误；不修改输入。
/// 复杂度：时间 O(响应体长度)，空间 O(回应文本长度)。
///
/// Module: ai / OpenAI-compatible backend / parsing
/// Purpose: Parse a completion response into the first assistant message and truncation flag.
/// Inputs: `text`: JSON text of a successful response.
/// Outputs / side effects: `AiResponse` or a Chinese error; the input is unchanged.
/// Complexity: O(body length) time and O(reply length) space.
fn parse_response(text: &str) -> Result<AiResponse, String> {
    let value: Value = serde_json::from_str(text).map_err(|error| {
        format!(
            "解析 AI 响应失败：{error}；原始响应：{}",
            truncate_for_error(text, 200)
        )
    })?;
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("未知错误");
        return Err(format!("AI 接口错误：{}", truncate_for_error(message, 400)));
    }
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| format!("AI 响应缺少 choices：{}", truncate_for_error(text, 300)))?;
    let message = choice
        .get("message")
        .ok_or_else(|| "AI 响应缺少 message".to_string())?;
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if content.is_empty() {
        return Err("AI 返回了空内容，请重试或更换模型".into());
    }
    let reasoning = message
        .get("reasoning_content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    let truncated = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .is_some_and(|reason| reason == "length");
    Ok(AiResponse {
        content,
        reasoning,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：Base URL 自动补全 `/chat/completions`，已含路径时不重复追加。
    /// Covers: the base URL gets `/chat/completions` appended once.
    #[test]
    fn endpoint_appends_chat_completions_once() {
        assert_eq!(
            endpoint("https://api.deepseek.com").expect("url"),
            "https://api.deepseek.com/chat/completions"
        );
        assert_eq!(
            endpoint("https://api.xiaomimimo.com/v1/").expect("url"),
            "https://api.xiaomimimo.com/v1/chat/completions"
        );
        assert_eq!(
            endpoint("https://example.com/v1/chat/completions").expect("url"),
            "https://example.com/v1/chat/completions"
        );
        assert!(endpoint("  ").is_err());
        assert!(endpoint("api.deepseek.com").is_err());
    }

    /// 覆盖：成功响应解析出内容与截断标记，空内容报错。
    /// Covers: a successful response yields content and truncation; empty content errors.
    #[test]
    fn response_parsing_reads_content_and_finish_reason() {
        let ok = r#"{"choices":[{"message":{"content":"hi","reasoning_content":"think"},"finish_reason":"length"}]}"#;
        let parsed = parse_response(ok).expect("response");
        assert_eq!(parsed.content, "hi");
        assert_eq!(parsed.reasoning.as_deref(), Some("think"));
        assert!(parsed.truncated);
        assert!(parse_response(r#"{"choices":[]}"#).is_err());
        assert!(parse_response(r#"{"choices":[{"message":{"content":"  "}}]}"#).is_err());
    }

    /// 覆盖：流式请求体打开 `stream`、思考模式与推理强度。
    /// Covers: the streaming payload enables `stream`, thinking and the reasoning effort.
    #[test]
    fn payload_enables_streaming_and_reasoning() {
        use crate::ai::{AiMessage, AiReasoning, AiRole};
        let request = AiRequest {
            provider: AiProvider::DeepSeek,
            provider_label: "DeepSeek".into(),
            base_url: "https://api.deepseek.com".into(),
            api_key: "key".into(),
            model: "deepseek-flash".into(),
            messages: vec![AiMessage {
                role: AiRole::User,
                content: "hi".into(),
            }],
            temperature: 0.7,
            top_p: Some(0.9),
            top_k: Some(40),
            max_tokens: 1024,
            timeout: std::time::Duration::from_secs(30),
            reasoning: AiReasoning::Medium,
            json_output: true,
        };
        let payload: Value =
            serde_json::from_str(&build_payload(&request, true).expect("payload")).expect("json");
        assert_eq!(payload["stream"], json!(true));
        assert_eq!(payload["thinking"]["type"], json!("enabled"));
        assert_eq!(payload["reasoning_effort"], json!("medium"));
        assert_eq!(payload["response_format"]["type"], json!("json_object"));
        assert!((payload["top_p"].as_f64().expect("top_p") - 0.9).abs() < 1e-5);
        assert_eq!(payload["top_k"], json!(40));
    }

    /// 覆盖：错误体优先读取 `error.message`。
    /// Covers: error bodies prefer `error.message`.
    #[test]
    fn error_body_reads_nested_message() {
        assert_eq!(
            error_from_body(r#"{"error":{"message":"bad key"}}"#),
            "bad key"
        );
        assert_eq!(error_from_body("plain text"), "plain text");
    }
}
