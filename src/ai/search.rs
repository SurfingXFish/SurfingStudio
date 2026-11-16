//! 所属模块：ai / 联网检索
//! 文件用途：
//! - 使用自建 Bing HTML 检索（无需第三方 Search API Key）为 AI 编曲检索插件资料。
//! - 解析搜索结果页，把标题、站点、摘要与链接整理成可读文本，供第二次模型调用使用。
//!
//! 函数清单：
//! - `AiSearchConfig`：搜索开关与超时。
//! - `search`：执行 1~3 条查询并合并结果。
//! - `search_url` / `encode_query`：构造检索地址与百分号编码。
//! - `parse_results` / `extract_results` / `parse_block`：解析 Bing HTML 结果。
//! - `strip_tags` / `decode_entities` / `host_from_url`：HTML 文本清理。
//!
//! Module: ai / web research
//! Purpose:
//! - Use a self-hosted Bing HTML search (no third-party Search API key) to research plugin
//!   characteristics for AI composition.
//! - Parse result pages and merge titles, sites, summaries and links into readable text for a second
//!   model call.
//!
//! Functions:
//! - `AiSearchConfig`: search switch and timeout.
//! - `search`: run 1..=3 queries and merge the results.
//! - `search_url` / `encode_query`: build the search URL and percent-encode the query.
//! - `parse_results` / `extract_results` / `parse_block`: parse Bing HTML results.
//! - `strip_tags` / `decode_entities` / `host_from_url`: clean HTML text.

use super::truncate_for_error;
use std::time::Duration;

/// 单次检索最多使用的查询数。
/// Maximum number of queries used per search run.
const SEARCH_MAX_QUERIES: usize = 3;
/// 每条查询保留的结果数。
/// Number of results kept per query.
const SEARCH_MAX_RESULTS: usize = 5;
/// 单条摘要的最大字符数。
/// Maximum characters kept per summary.
const SEARCH_SUMMARY_LIMIT: usize = 900;
/// 检索页地址；不依赖任何厂商 Search API。
/// Search page URL; no vendor Search API is used.
const SEARCH_BASE: &str = "https://cn.bing.com/search";
/// 浏览器 User-Agent；Bing 对默认客户端会返回精简页。
/// Browser User-Agent; Bing returns a reduced page for default clients.
const SEARCH_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// 一次搜索的配置。
/// Configuration for one search run.
#[derive(Clone, Debug)]
pub struct AiSearchConfig {
    pub enabled: bool,
    pub timeout: Duration,
}

impl Default for AiSearchConfig {
    /// 所属模块：ai / 联网检索
    /// 功能：构造默认配置：关闭与 20 秒超时。
    /// 输入：无。
    /// 输出/副作用：默认配置；不访问网络。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / web research
    /// Purpose: Build the default config: disabled with a 20-second timeout.
    /// Inputs: none.
    /// Outputs / side effects: defaults; no network access.
    /// Complexity: O(1) time and space.
    fn default() -> Self {
        Self {
            enabled: false,
            timeout: Duration::from_secs(20),
        }
    }
}

/// 一条检索结果。
/// One search result.
struct SearchHit {
    title: String,
    site: String,
    summary: String,
    url: String,
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
        return Err("联网搜索未启用".into());
    }
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
        if used > SEARCH_MAX_QUERIES {
            break;
        }
        text.push_str(&format!("\n### 查询：{trimmed}\n"));
        let url = search_url(trimmed);
        let response = agent
            .get(&url)
            .set("Accept", "text/html,application/xhtml+xml")
            .set("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .set("User-Agent", SEARCH_USER_AGENT)
            .call();
        let (status, body) = match response {
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
        let body = match body {
            Ok(body) => body,
            Err(error) => {
                failures += 1;
                text.push_str(&format!("读取搜索结果失败：{error}\n"));
                continue;
            }
        };
        if !(200..300).contains(&status) {
            failures += 1;
            text.push_str(&format!(
                "搜索页返回 HTTP {status}：{}\n",
                truncate_for_error(&body, 300)
            ));
            continue;
        }
        match parse_results(&body) {
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
/// 功能：构造 Bing 检索地址，固定中文界面并限制每条查询的结果数量。
/// 输入：`query`：用户/模型给出的检索词。
/// 输出/副作用：完整 URL；不访问网络。
/// 复杂度：时间 O(查询长度)，空间 O(查询长度)。
///
/// Module: ai / web research / URL
/// Purpose: Build the Bing search URL with a Chinese UI and a per-query result count.
/// Inputs: `query`: the query text.
/// Outputs / side effects: the complete URL; no network access.
/// Complexity: O(query length) time and space.
pub(crate) fn search_url(query: &str) -> String {
    format!(
        "{SEARCH_BASE}?q={}&count={SEARCH_MAX_RESULTS}&setlang=zh-Hans",
        encode_query(query)
    )
}

/// 所属模块：ai / 联网检索 / 编码
/// 功能：按 URL 百分号编码 UTF-8 查询，空格编码为 `%20`。
/// 输入：`text`：原始查询。
/// 输出/副作用：编码文本；不修改输入。
/// 复杂度：时间 O(字节数)，空间 O(字节数)。
///
/// Module: ai / web research / encoding
/// Purpose: Percent-encode a UTF-8 query, encoding spaces as `%20`.
/// Inputs: `text`: raw query.
/// Outputs / side effects: the encoded text; the input is unchanged.
/// Complexity: O(bytes) time and space.
pub(crate) fn encode_query(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// 所属模块：ai / 联网检索 / 解析
/// 功能：解析 Bing 结果页，生成标题、站点、摘要与链接；没有结果时返回空字符串。
/// 输入：`html`：搜索结果页。
/// 输出/副作用：整理后的文本；不修改输入。
/// 复杂度：时间 O(HTML 长度)，空间 O(整理文本长度)。
///
/// Module: ai / web research / parsing
/// Purpose: Parse a Bing result page into titles, sites, summaries and links; returns an empty
/// string when there are no results.
/// Inputs: `html`: the result page.
/// Outputs / side effects: formatted text; the input is unchanged.
/// Complexity: O(HTML length) time and O(formatted text) space.
pub(crate) fn parse_results(html: &str) -> Result<String, String> {
    let hits = extract_results(html);
    let mut text = String::new();
    for (index, hit) in hits.iter().enumerate() {
        text.push_str(&format!(
            "{}. {}{}\n{}\n{}\n",
            index + 1,
            hit.title,
            if hit.site.is_empty() {
                String::new()
            } else {
                format!("（{}）", hit.site)
            },
            truncate_for_error(&hit.summary, SEARCH_SUMMARY_LIMIT),
            hit.url
        ));
    }
    Ok(text)
}

/// 所属模块：ai / 联网检索 / 解析
/// 功能：从 Bing HTML 中提取最多 5 条 `b_algo` 结果。
/// 输入：`html`：结果页。
/// 输出/副作用：结果列表；不修改输入。
/// 复杂度：时间 O(HTML 长度)，空间 O(结果数)。
///
/// Module: ai / web research / parsing
/// Purpose: Extract at most five `b_algo` results from Bing HTML.
/// Inputs: `html`: the result page.
/// Outputs / side effects: the result list; the input is unchanged.
/// Complexity: O(HTML length) time and O(results) space.
fn extract_results(html: &str) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let mut rest = html;
    while let Some(index) = rest.find("<li class=\"b_algo\"") {
        rest = &rest[index..];
        let end = rest.find("</li>").map(|end| end + 5).unwrap_or(rest.len());
        let block = &rest[..end];
        if let Some(hit) = parse_block(block) {
            hits.push(hit);
        }
        rest = &rest[end..];
        if hits.len() >= SEARCH_MAX_RESULTS {
            break;
        }
    }
    hits
}

/// 所属模块：ai / 联网检索 / 解析
/// 功能：解析单个 Bing 结果块：标题链接、摘要与站点。
/// 输入：`block`：一个 `b_algo` 结果块。
/// 输出/副作用：结果或 `None`；不修改输入。
/// 复杂度：时间 O(块长度)，空间 O(文本长度)。
///
/// Module: ai / web research / parsing
/// Purpose: Parse one Bing result block: title link, summary and site.
/// Inputs: `block`: one `b_algo` block.
/// Outputs / side effects: the hit or `None`; the input is unchanged.
/// Complexity: O(block length) time and O(text length) space.
fn parse_block(block: &str) -> Option<SearchHit> {
    let h2 = block.find("<h2")?;
    let after_h2 = &block[h2..];
    let anchor_start = after_h2.find("<a")? + h2;
    let anchor = &block[anchor_start..];
    let tag_end = anchor.find('>')?;
    let open_tag = &anchor[..tag_end];
    let url = find_attribute(open_tag, "href")?;
    let title_html = &anchor[tag_end + 1..];
    let title_end = title_html.find("</a>")?;
    let title = clean_text(&title_html[..title_end]);
    if title.is_empty() || url.is_empty() {
        return None;
    }
    let summary = block
        .find("<p class=\"b_lineclamp")
        .and_then(|start| paragraph_text(&block[start..]))
        .or_else(|| {
            block
                .find("<p")
                .and_then(|start| paragraph_text(&block[start..]))
        })
        .unwrap_or_default();
    let site = block
        .find("class=\"tilk\"")
        .and_then(|start| find_attribute(&block[start..block.len().min(start + 600)], "aria-label"))
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| host_from_url(&url));
    Some(SearchHit {
        title,
        site,
        summary,
        url,
    })
}

/// 所属模块：ai / 联网检索 / 解析
/// 功能：从标签文本中读取 `name="value"` 属性。
/// 输入：`tag`：标签文本；`name`：属性名。
/// 输出/副作用：属性值或 `None`；不修改输入。
/// 复杂度：时间 O(标签长度)，空间 O(值长度)。
///
/// Module: ai / web research / parsing
/// Purpose: Read a `name="value"` attribute from a tag.
/// Inputs: `tag`: tag text; `name`: attribute name.
/// Outputs / side effects: the value or `None`; the input is unchanged.
/// Complexity: O(tag length) time and O(value length) space.
fn find_attribute(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    Some(decode_entities(&tag[start..end]))
}

/// 所属模块：ai / 联网检索 / 解析
/// 功能：读取 `<p ...>...</p>` 中的摘要并清理 HTML。
/// 输入：`text`：从 `<p` 开始的片段。
/// 输出/副作用：纯文本或 `None`；不修改输入。
/// 复杂度：时间 O(片段长度)，空间 O(文本长度)。
///
/// Module: ai / web research / parsing
/// Purpose: Read the summary inside `<p ...>...</p>` and clean its HTML.
/// Inputs: `text`: a fragment starting at `<p`.
/// Outputs / side effects: plain text or `None`; the input is unchanged.
/// Complexity: O(fragment length) time and O(text length) space.
fn paragraph_text(text: &str) -> Option<String> {
    let open_end = text.find('>')?;
    let body = &text[open_end + 1..];
    let close = body.find("</p>")?;
    Some(clean_text(&body[..close]))
}

/// 所属模块：ai / 联网检索 / 文本
/// 功能：移除 HTML 标签、解码实体、折叠空白。
/// 输入：`html`：含标签的文本。
/// 输出/副作用：纯文本；不修改输入。
/// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
///
/// Module: ai / web research / text
/// Purpose: Remove HTML tags, decode entities and collapse whitespace.
/// Inputs: `html`: text containing tags.
/// Outputs / side effects: plain text; the input is unchanged.
/// Complexity: O(text length) time and space.
fn clean_text(html: &str) -> String {
    decode_entities(&strip_tags(html))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 所属模块：ai / 联网检索 / 文本
/// 功能：移除所有 `<...>` 标签。
/// 输入：`html`：含标签的文本。
/// 输出/副作用：去标签文本；不修改输入。
/// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
///
/// Module: ai / web research / text
/// Purpose: Remove every `<...>` tag.
/// Inputs: `html`: text containing tags.
/// Outputs / side effects: tag-free text; the input is unchanged.
/// Complexity: O(text length) time and space.
fn strip_tags(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
}

/// 所属模块：ai / 联网检索 / 文本
/// 功能：解码常见 HTML 实体与十进制/十六进制字符引用。
/// 输入：`text`：含实体的文本。
/// 输出/副作用：解码文本；不修改输入。
/// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
///
/// Module: ai / web research / text
/// Purpose: Decode common HTML entities and decimal/hex character references.
/// Inputs: `text`: text containing entities.
/// Outputs / side effects: decoded text; the input is unchanged.
/// Complexity: O(text length) time and space.
fn decode_entities(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find(';') else {
            output.push_str(rest);
            return output;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| {
                    entity
                        .strip_prefix('#')
                        .and_then(|digits| digits.parse::<u32>().ok())
                })
                .and_then(char::from_u32),
        };
        if let Some(ch) = decoded {
            output.push(ch);
            rest = &rest[end + 1..];
        } else {
            output.push('&');
            rest = &rest[1..];
        }
    }
    output.push_str(rest);
    output
}

/// 所属模块：ai / 联网检索 / 文本
/// 功能：从 URL 提取站点主机名，去掉 `www.` 前缀。
/// 输入：`url`：完整链接。
/// 输出/副作用：主机名；不修改输入。
/// 复杂度：时间 O(地址长度)，空间 O(地址长度)。
///
/// Module: ai / web research / text
/// Purpose: Extract the host name from a URL and strip a `www.` prefix.
/// Inputs: `url`: the complete link.
/// Outputs / side effects: the host name; the input is unchanged.
/// Complexity: O(URL length) time and space.
fn host_from_url(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let host = without_scheme.split(['/', '?', '#']).next().unwrap_or("");
    host.trim_start_matches("www.").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：UTF-8 查询被百分号编码，空格为 `%20`。
    /// Covers: UTF-8 queries are percent-encoded and spaces become `%20`.
    #[test]
    fn queries_are_percent_encoded() {
        assert_eq!(encode_query("Serum synth"), "Serum%20synth");
        assert_eq!(encode_query("插件"), "%E6%8F%92%E4%BB%B6");
        let url = search_url("Serum 插件");
        assert!(url.starts_with("https://cn.bing.com/search?q=Serum%20"));
        assert!(url.contains("count=5"));
    }

    /// 覆盖：Bing 结果块被解析为标题、站点、摘要与链接，实体被解码。
    /// Covers: Bing result blocks become title/site/summary/link and entities are decoded.
    #[test]
    fn bing_results_are_parsed() {
        let html = r#"
        <ol id="b_results">
        <li class="b_algo" data-id><h2><a target="_blank" href="https://xferrecords.com/products/serum-2">Serum <strong>2</strong>: Advanced &amp; Hybrid</a></h2><div class="b_caption"><p class="b_lineclamp2">Serum 2 ships with a massive &amp; original library.</p></div></li>
        <li class="b_algo" data-id><div class="b_tpcn"><a class="tilk" aria-label="soundon.global" href="https://soundon.global/x">SoundOn</a></div><h2><a href="https://soundon.global/x">Serum 教程</a></h2><div class="b_caption"><p>从&#35013;&#32622;到音色设计。</p></div></li>
        </ol>"#;
        let text = parse_results(html).expect("parse");
        assert!(text.contains("Serum 2: Advanced & Hybrid"));
        assert!(text.contains("https://xferrecords.com/products/serum-2"));
        assert!(text.contains("massive & original library"));
        assert!(text.contains("（soundon.global）"));
        assert!(text.contains("Serum 教程"));
        assert!(text.contains("从装置到音色设计"));
    }

    /// 覆盖：没有 `b_algo` 的结果页返回空文本。
    /// Covers: a result page without `b_algo` returns empty text.
    #[test]
    fn pages_without_results_are_empty() {
        assert!(
            parse_results("<html><body>no results</body></html>")
                .expect("parse")
                .is_empty()
        );
    }

    /// 覆盖：站点名从 URL 提取并去掉 `www.`。
    /// Covers: the site name is extracted from the URL without `www.`.
    #[test]
    fn hosts_are_extracted() {
        assert_eq!(
            host_from_url("https://www.example.com/a/b?q=1"),
            "example.com"
        );
        assert_eq!(host_from_url("http://example.com"), "example.com");
    }
}
