//! 所属模块：ai / 联网检索
//! 文件用途：
//! - 使用 DeepSeek Search API 为 AI 编曲检索插件资料。
//! - 把多条查询的结果整理成可读文本，供第二次模型调用使用。
//!
//! 函数清单：
//! - `AiSearchConfig`：搜索开关、Base URL、API Key 与超时。
//! - `search`：执行 1~3 条查询并合并结果。
//! - `search_endpoint` / `parse_results`：地址规范化与响应解析。
//!
//! Module: ai / web research
//! Purpose:
//! - Use the DeepSeek Search API to research plugin characteristics for AI composition.
//! - Merge results from several queries into readable text for a second model call.
//!
//! Functions:
//! - `AiSearchConfig`: search switch, base URL, API key and timeout.
//! - `search`: run 1..=3 queries and merge the results.
//! - `search_endpoint` / `parse_results`: URL normalisation and response parsing.

use super::truncate_for_error;
use serde_json::{Value, json};
use std::time::Duration;

/// 一次搜索的配置；使用 DeepSeek 的 API Key 与 Search 接口。
/// Configuration for one search run; uses the DeepSeek API key and Search endpoint.
#[derive(Clone, Debug)]
pub struct AiSearchConfig {
    pub enabled: bool,
    pub base_url: String,
    pub api_key: String,
    pub timeout: Duration,
}

impl Default for AiSearchConfig {
    /// 所属模块：ai / 联网检索
    /// 功能：构造默认配置：关闭、官方地址、空密钥、30 秒超时。
    /// 输入：无。
    /// 输出/副作用：默认配置；不访问网络。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / web research
    /// Purpose: Build the default config: disabled, official URL, empty key and a 30-second timeout.
    /// Inputs: none.
    /// Outputs / side effects: defaults; no network access.
    /// Complexity: O(1) time and space.
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "https://api.deepseek.com".into(),
            api_key: String::new(),
            timeout: Duration::from_secs(30),
        }
    }
}

/// 所属模块：ai / 联网检索
/// 功能：执行最多 3 条查询，把标题、站点、摘要与链接整理成文本；单条失败不丢弃其它结果。
/// 输入：
/// - `config`：搜索配置；
/// - `queries`：模型给出的检索词。
/// 输出/副作用：汇总文本或中文错误；阻塞调用，只能在后台线程执行。
/// 复杂度：时间 O(查询数 × 网络延迟 + 响应大小)，空间 O(汇总文本长度)。
///
/// Module: ai / web research
/// Purpose: Run at most three queries and merge titles, sites, summaries and links; one failed
/// query does not discard the others.
/// Inputs:
/// - `config`: search configuration;
/// - `queries`: queries supplied by the model.
/// Outputs / side effects: merged text or a Chinese error; blocking, so it must run on a worker.
/// Complexity: O(queries × network latency + response size) time and O(merged text) space.
pub fn search(config: &AiSearchConfig, queries: &[String]) -> Result<String, String> {
    if !config.enabled {
        return Err("联网搜索未启用或未配置 DeepSeek API Key".into());
    }
    if config.api_key.trim().is_empty() {
        return Err("联网搜索需要 DeepSeek API Key".into());
    }
    let endpoint = search_endpoint(&config.base_url)?;
    let agent = ureq::AgentBuilder::new().timeout(config.timeout).build();
    let mut text = String::new();
    let mut used = 0usize;
    let mut failures = 0usize;
    let mut seen = Vec::<String>::new();
    for query in queries {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        used += 1;
        if used > 3 {
            break;
        }
        text.push_str(&format!("\n### 查询：{trimmed}\n"));
        let body = json!({
            "query": trimmed,
            "count": 5,
            "need_content": false,
        });
        let response = agent
            .post(&endpoint)
            .set("Content-Type", "application/json")
            .set("Accept", "application/json")
            .set(
                "Authorization",
                &format!("Bearer {}", config.api_key.trim()),
            )
            .send_string(&body.to_string());
        let (status, payload) = match response {
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
                failures += 1;
                text.push_str(&format!("搜索请求失败：{error}\n"));
                continue;
            }
        };
        let payload = match payload {
            Ok(payload) => payload,
            Err(error) => {
                failures += 1;
                text.push_str(&format!("读取搜索结果失败：{error}\n"));
                continue;
            }
        };
        if !(200..300).contains(&status) {
            failures += 1;
            text.push_str(&format!(
                "搜索接口返回 HTTP {status}：{}\n",
                truncate_for_error(&payload, 300)
            ));
            continue;
        }
        match parse_results(&payload) {
            Ok(results) if !results.is_empty() => text.push_str(&results),
            Ok(_) => text.push_str("（没有找到结果）\n"),
            Err(error) => {
                failures += 1;
                text.push_str(&format!("解析搜索结果失败：{error}\n"));
            }
        }
    }
    if used == 0 {
        return Err("没有有效的检索词".into());
    }
    if failures == used {
        return Err(truncate_for_error(&text, 600));
    }
    Ok(text)
}

/// 所属模块：ai / 联网检索 / 地址
/// 功能：把 DeepSeek Base URL 规范化为 Search API 地址。
/// 输入：`base_url`：如 `https://api.deepseek.com` 或 `https://api.deepseek.com/v1`。
/// 输出/副作用：完整地址或中文错误；不修改输入。
/// 复杂度：时间 O(地址长度)，空间 O(地址长度)。
///
/// Module: ai / web research / URL
/// Purpose: Normalise a DeepSeek base URL into the Search API URL.
/// Inputs: `base_url`: e.g. `https://api.deepseek.com` or `https://api.deepseek.com/v1`.
/// Outputs / side effects: full URL or a Chinese error; the input is unchanged.
/// Complexity: O(URL length) time and space.
pub(crate) fn search_endpoint(base_url: &str) -> Result<String, String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("联网搜索的 Base URL 为空".into());
    }
    if !base.starts_with("https://") && !base.starts_with("http://") {
        return Err("联网搜索的 Base URL 必须以 http:// 或 https:// 开头".into());
    }
    if base.ends_with("/v1/tools/search") {
        Ok(base.to_string())
    } else if base.ends_with("/v1") {
        Ok(format!("{base}/tools/search"))
    } else {
        Ok(format!("{base}/v1/tools/search"))
    }
}

/// 所属模块：ai / 联网检索 / 解析
/// 功能：解析 Search API 的 `results` 数组，生成标题、站点、摘要与链接。
/// 输入：`text`：搜索接口返回的 JSON。
/// 输出/副作用：整理后的文本或中文错误；不修改输入。
/// 复杂度：时间 O(响应体长度)，空间 O(整理文本长度)。
///
/// Module: ai / web research / parsing
/// Purpose: Parse the Search API `results` array into titles, sites, summaries and links.
/// Inputs: `text`: JSON returned by the search endpoint.
/// Outputs / side effects: formatted text or a Chinese error; the input is unchanged.
/// Complexity: O(body length) time and O(formatted text) space.
pub(crate) fn parse_results(text: &str) -> Result<String, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|error| format!("搜索响应不是 JSON：{error}"))?;
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("未知错误");
        return Err(truncate_for_error(message, 300));
    }
    let Some(results) = value.get("results").and_then(Value::as_array) else {
        return Ok(String::new());
    };
    let mut text = String::new();
    for (index, result) in results.iter().take(5).enumerate() {
        let title = result
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("未命名");
        let site = result
            .get("site_name")
            .and_then(Value::as_str)
            .unwrap_or("");
        let summary = result
            .get("summary")
            .and_then(Value::as_str)
            .or_else(|| result.get("snippet").and_then(Value::as_str))
            .unwrap_or("");
        let url = result.get("url").and_then(Value::as_str).unwrap_or("");
        text.push_str(&format!(
            "{}. {title}{}\n{}\n{url}\n",
            index + 1,
            if site.is_empty() {
                String::new()
            } else {
                format!("（{site}）")
            },
            truncate_for_error(summary, 900)
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：Base URL 规范化只追加一次 `/v1/tools/search`。
    /// Covers: the base URL gets `/v1/tools/search` appended once.
    #[test]
    fn search_endpoint_is_normalised() {
        assert_eq!(
            search_endpoint("https://api.deepseek.com").expect("url"),
            "https://api.deepseek.com/v1/tools/search"
        );
        assert_eq!(
            search_endpoint("https://api.deepseek.com/v1").expect("url"),
            "https://api.deepseek.com/v1/tools/search"
        );
        assert_eq!(
            search_endpoint("https://api.deepseek.com/v1/tools/search").expect("url"),
            "https://api.deepseek.com/v1/tools/search"
        );
        assert!(search_endpoint("api.deepseek.com").is_err());
    }

    /// 覆盖：搜索响应被整理成可读条目，错误体被识别。
    /// Covers: search responses become readable entries and error bodies are detected.
    #[test]
    fn search_results_are_formatted() {
        let payload = r#"{"results":[{"title":"Serum","site_name":"Xfer","summary":"Wavetable synth","url":"https://example.com/serum"}]}"#;
        let text = parse_results(payload).expect("parse");
        assert!(text.contains("Serum"));
        assert!(text.contains("Wavetable synth"));
        assert!(text.contains("https://example.com/serum"));
        assert!(parse_results(r#"{"error":{"message":"bad key"}}"#).is_err());
    }
}
