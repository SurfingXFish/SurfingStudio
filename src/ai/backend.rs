//! 所属模块：ai / 后端接口
//! 文件用途：
//! - 定义 AI 协议后端的能力描述与统一补全入口。
//! - 应用层只依赖 `AiRuntime`，不直接引用具体后端。
//!
//! 函数清单：
//! - `AiBackendInfo`：后端的稳定标识与展示名称。
//! - `AiBackend::info` / `AiBackend::complete` / `AiBackend::stream`：后端自描述、执行一次补全与一次流式补全。
//!
//! Module: ai / backend interface
//! Purpose:
//! - Define the capability descriptor and the unified completion entry of an AI protocol backend.
//! - The app layer depends on `AiRuntime` only and never references a concrete backend.
//!
//! Functions:
//! - `AiBackendInfo`: stable id and display name of a backend.
//! - `AiBackend::info` / `AiBackend::complete` / `AiBackend::stream`: backend description, one
//!   completion call and one streaming completion call.

use super::{AiRequest, AiResponse, AiStreamEvent};

/// 后端能力描述。
/// Backend capability descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiBackendInfo {
    /// 协议标识，与 `AiProvider::protocol` 对应。
    /// Protocol id matching `AiProvider::protocol`.
    pub id: &'static str,
    /// 展示名称。
    /// Display name.
    pub label: &'static str,
}

/// 一种 AI 接口协议。实现必须线程安全，因为补全在后台线程执行。
/// One AI API protocol. Implementations must be thread-safe because completions run on a worker.
pub trait AiBackend: Send + Sync {
    /// 所属模块：ai / 后端接口
    /// 功能：返回后端协议描述。
    /// 输入：`self`。
    /// 输出/副作用：能力描述；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / backend interface
    /// Purpose: Return the protocol descriptor.
    /// Inputs: `self`.
    /// Outputs / side effects: descriptor; no state change.
    /// Complexity: O(1) time and space.
    fn info(&self) -> AiBackendInfo;

    /// 所属模块：ai / 后端接口
    /// 功能：执行一次同步对话补全。
    /// 输入：`request`：服务商、模型、消息、温度、输出上限与超时。
    /// 输出/副作用：助手回复或中文错误；阻塞调用。
    /// 复杂度：时间 O(网络延迟 + 响应大小)，空间 O(响应大小)。
    ///
    /// Module: ai / backend interface
    /// Purpose: Run one synchronous chat completion.
    /// Inputs: `request`: provider, model, messages, temperature, output limit and timeout.
    /// Outputs / side effects: assistant reply or a Chinese error; blocking call.
    /// Complexity: O(network latency + response size) time and O(response size) space.
    fn complete(&self, request: &AiRequest) -> Result<AiResponse, String>;

    /// 所属模块：ai / 后端接口
    /// 功能：执行一次流式对话补全，把思考与内容增量逐个交给 `sink`。
    /// 输入：`request`：服务商、模型、消息与超时；`sink`：增量回调。
    /// 输出/副作用：完整助手回复或中文错误；阻塞调用，必须在后台线程执行。
    /// 复杂度：时间 O(网络延迟 + 响应大小)，空间 O(响应大小)。
    ///
    /// 默认实现退化为一次同步补全：先推送全部思考，再推送全部内容，保证不实现 SSE
    /// 的后端仍能工作。
    ///
    /// Module: ai / backend interface
    /// Purpose: Run one streaming completion, forwarding reasoning and content increments to `sink`.
    /// Inputs: `request`: provider, model, messages and timeout; `sink`: increment callback.
    /// Outputs / side effects: the complete assistant reply or a Chinese error; blocking, so it must
    /// run on a worker thread.
    /// Complexity: O(network latency + response size) time and O(response size) space.
    ///
    /// The default implementation degrades to one synchronous completion: it forwards all reasoning
    /// first and then all content, so backends without SSE still work.
    fn stream(
        &self,
        request: &AiRequest,
        sink: &mut dyn FnMut(AiStreamEvent),
    ) -> Result<AiResponse, String> {
        let response = self.complete(request)?;
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
}
