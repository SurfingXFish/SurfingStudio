//! 所属模块：surfing-ai-data / Markdown 技能文件格式
//! 文件用途：
//! - 把一条技能序列化为带 YAML 风格头部的 Markdown（id / title / summary / kind）。
//! - 解析用户手写或导入的 Markdown 技能；缺失字段给出空值，由调用方补默认值。
//!
//! 函数清单：
//! - `encode`：把技能字段编码为 Markdown 文本。
//! - `decode`：解析 Markdown 头部与正文。
//! - `Header`：解析结果的头部字段集合。
//!
//! Module: surfing-ai-data / Markdown skill format
//! Purpose:
//! - Serialize one skill into Markdown with a YAML-like header (id / title / summary / kind).
//! - Parse hand-written or imported Markdown skills; missing fields stay empty for the caller.
//!
//! Functions:
//! - `encode`: encode skill fields into Markdown text.
//! - `decode`: parse the Markdown header and body.
//! - `Header`: parsed header fields.

/// Markdown 技能头部字段。
/// Header fields of a Markdown skill file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub id: String,
    pub title: String,
    pub summary: String,
    /// `basic` / `technique` / `instrument`；用于导入时自动分类。
    /// `basic` / `technique` / `instrument`; used to classify imported files automatically.
    pub kind: String,
}

/// 所属模块：surfing-ai-data / Markdown 技能文件格式
/// 功能：把一条技能编码为带头部的 Markdown。
/// 输入：`id` / `title` / `summary` / `kind` / `body`。
/// 输出/副作用：Markdown 文本；不修改输入。
/// 复杂度：时间 O(正文长度)，空间 O(正文长度)。
///
/// Module: surfing-ai-data / Markdown skill format
/// Purpose: Encode one skill as Markdown with a header.
/// Inputs: `id` / `title` / `summary` / `kind` / `body`.
/// Outputs / side effects: Markdown text; the inputs are unchanged.
/// Complexity: O(body length) time and space.
pub fn encode(id: &str, title: &str, summary: &str, kind: &str, body: &str) -> String {
    let mut text = String::from("---\n");
    push_field(&mut text, "id", id);
    push_field(&mut text, "title", title);
    push_field(&mut text, "summary", summary);
    push_field(&mut text, "kind", kind);
    text.push_str("---\n\n");
    text.push_str(body.trim_end());
    text.push('\n');
    text
}

/// 所属模块：surfing-ai-data / Markdown 技能文件格式
/// 功能：解析 Markdown 头部与正文；没有合法头部时把全文当作正文。
/// 输入：`text` 文件内容。
/// 输出/副作用：头部与正文；不修改输入。
/// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
///
/// Module: surfing-ai-data / Markdown skill format
/// Purpose: Parse the header and body; treat the whole file as body when no valid header exists.
/// Inputs: `text`: file contents.
/// Outputs / side effects: header and body; the input is unchanged.
/// Complexity: O(text length) time and space.
pub fn decode(text: &str) -> (Header, String) {
    let normalized = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = normalized.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return (Header::default(), String::new());
    };
    if first.trim() != "---" {
        return (Header::default(), normalized.trim().to_string());
    }
    let mut header = Header::default();
    let mut cursor = first.len();
    let mut body = String::new();
    for line in lines {
        let end = (cursor + line.len()).min(normalized.len());
        let content = line.trim_end_matches(['\r', '\n']);
        if content.trim() == "---" {
            body = normalized
                .get(end..)
                .unwrap_or_default()
                .trim_start_matches(['\r', '\n'])
                .trim_end()
                .to_string();
            break;
        }
        if let Some((key, value)) = content.split_once(':') {
            let value = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            match key.trim().to_ascii_lowercase().as_str() {
                "id" => header.id = value,
                "title" => header.title = value,
                "summary" => header.summary = value,
                "kind" => header.kind = value.to_ascii_lowercase(),
                _ => {}
            }
        }
        cursor = end;
    }
    (header, body)
}

/// 所属模块：surfing-ai-data / Markdown 技能文件格式
/// 功能：追加一个单行头部字段，把换行折叠为空格以避免破坏头部结构。
/// 输入：`text` 目标缓冲；`key` 字段名；`value` 字段值。
/// 输出/副作用：原地追加一行；不修改其他状态。
/// 复杂度：时间 O(值长度)，空间 O(值长度)。
///
/// Module: surfing-ai-data / Markdown skill format
/// Purpose: Append one single-line header field, folding newlines into spaces.
/// Inputs: `text`; `key`; `value`.
/// Outputs / side effects: appends one line in place.
/// Complexity: O(value length) time and space.
fn push_field(text: &mut String, key: &str, value: &str) {
    let value = value.replace(['\r', '\n'], " ");
    text.push_str(key);
    text.push_str(": ");
    text.push_str(value.trim());
    text.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：带头部的 Markdown 往返保持 id、标题、摘要、类型与正文。
    /// Covers: a headed Markdown round-trip keeps id, title, summary, kind and body.
    #[test]
    fn markdown_round_trip_keeps_header_and_body() {
        let text = encode(
            "music-theory",
            "基础乐理",
            "和声与节奏",
            "basic",
            "# 正文\n内容",
        );
        let (header, body) = decode(&text);
        assert_eq!(header.id, "music-theory");
        assert_eq!(header.title, "基础乐理");
        assert_eq!(header.summary, "和声与节奏");
        assert_eq!(header.kind, "basic");
        assert_eq!(body, "# 正文\n内容");
    }

    /// 覆盖：没有头部的普通 Markdown 整篇作为正文。
    /// Covers: Markdown without a header treats the whole file as body.
    #[test]
    fn plain_markdown_becomes_body() {
        let (header, body) = decode("# 标题\n正文");
        assert!(header.id.is_empty());
        assert_eq!(body, "# 标题\n正文");
    }
}
