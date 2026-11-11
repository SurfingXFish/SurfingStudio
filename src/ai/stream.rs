//! 所属模块：ai / 流式计划
//! 文件用途：
//! - 在模型流式输出 JSON 编曲计划时增量提取 `operations` 数组。
//! - 只有当 `research` 与 `skills` 都已确认为空数组时才放行操作，保证“先研究、后写作”。
//! - 每个完整操作对象一出现就交给调用方写入工程，实现逐步创作/修改片段。
//!
//! 函数清单：
//! - `PlanStream::push`：追加一段流式内容并返回新完成的编曲操作。
//! - `PlanStream::allow_apply` / `emitted` / `content` / `errors`：流式状态查询。
//! - `find_array_start` / `find_string_array`：定位顶层数组与解析字符串数组。
//!
//! Module: ai / streaming plan
//! Purpose:
//! - Extract the `operations` array incrementally while the model streams a JSON plan.
//! - Only release operations once both `research` and `skills` are known to be empty arrays, so
//!   research always happens before writing.
//! - Hand every completed operation object to the caller immediately, enabling step-by-step
//!   creation/modification of clips.
//!
//! Functions:
//! - `PlanStream::push`: append a streaming chunk and return newly completed operations.
//! - `PlanStream::allow_apply` / `emitted` / `content` / `errors`: streaming state queries.
//! - `find_array_start` / `find_string_array`: locate a top-level array and parse a string array.

use super::plan::AiOperation;

/// 流式编曲计划提取器。
/// Incremental composition-plan extractor.
#[derive(Debug, Default)]
pub struct PlanStream {
    /// 已累计的完整 JSON 文本。
    /// Accumulated JSON text.
    content: String,
    /// 操作数组内部的扫描位置（字节索引）。
    /// Byte position scanned inside the operations array.
    scan: usize,
    /// `operations` 数组内容的起始字节位置。
    /// Byte offset where the operations array content begins.
    ops_cursor: Option<usize>,
    /// 当前对象嵌套深度（大括号）。
    /// Current object nesting depth (braces).
    depth: i32,
    /// 是否位于 JSON 字符串中。
    /// Whether the scanner is inside a JSON string.
    in_string: bool,
    /// 字符串中是否遇到转义符。
    /// Whether the previous character escaped the next one.
    escape: bool,
    /// 当前完整对象的起始字节位置。
    /// Byte offset where the current object starts.
    object_start: Option<usize>,
    /// 已解析但尚未放行的操作。
    /// Parsed operations waiting for the gate to open.
    buffered: Vec<AiOperation>,
    /// 顶层 `research` 的值；未解析完成为 `None`。
    /// Top-level `research` value; `None` until parsable.
    research: Option<Vec<String>>,
    /// 顶层 `skills` 的值；未解析完成为 `None`。
    /// Top-level `skills` value; `None` until parsable.
    skills: Option<Vec<String>>,
    /// `research` 与 `skills` 均为空数组时为真。
    /// True once both `research` and `skills` are empty arrays.
    gate_open: bool,
    /// 已放行的操作数量。
    /// Number of released operations.
    emitted: usize,
    /// 操作数组已结束。
    /// The operations array has ended.
    ops_done: bool,
    /// 解析过程中遇到的错误（不中断后续操作）。
    /// Errors met while parsing (later operations continue).
    errors: Vec<String>,
}

impl PlanStream {
    /// 所属模块：ai / 流式计划
    /// 功能：创建一个空的流式提取器。
    /// 输入：无。
    /// 输出/副作用：空状态；不访问网络。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / streaming plan
    /// Purpose: Create an empty streaming extractor.
    /// Inputs: none.
    /// Outputs / side effects: empty state; no network access.
    /// Complexity: O(1) time and space.
    pub fn new() -> Self {
        Self::default()
    }

    /// 所属模块：ai / 流式计划
    /// 功能：追加一段流式内容，返回本次新完成且允许写入的操作。
    /// 输入：`chunk`：模型流式输出的文本片段。
    /// 输出/副作用：追加内部缓冲；可能返回若干操作。
    /// 复杂度：时间 O(新增内容 + 新完成操作大小)，空间 O(内容长度)。
    ///
    /// Module: ai / streaming plan
    /// Purpose: Append a streaming chunk and return operations completed in this call that are
    /// allowed to be applied.
    /// Inputs: `chunk`: a text fragment streamed by the model.
    /// Outputs / side effects: appends to the internal buffer; may return operations.
    /// Complexity: O(new text + new operations) time and O(content length) space.
    pub fn push(&mut self, chunk: &str) -> Vec<AiOperation> {
        if chunk.is_empty() {
            return Vec::new();
        }
        self.content.push_str(chunk);
        self.refresh_arrays();
        self.scan_operations();
        self.update_gate();
        self.release_ready()
    }

    /// 所属模块：ai / 流式计划
    /// 功能：返回累计的完整 JSON 文本。
    /// 输入：`self`。
    /// 输出/副作用：文本引用；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / streaming plan
    /// Purpose: Return the accumulated JSON text.
    /// Inputs: `self`.
    /// Outputs / side effects: text reference; no state change.
    /// Complexity: O(1) time and space.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// 所属模块：ai / 流式计划
    /// 功能：是否已确认可以立即写入（研究/技巧均为空）。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / streaming plan
    /// Purpose: Whether operations may be applied immediately (`research`/`skills` both empty).
    /// Inputs: `self`.
    /// Outputs / side effects: boolean; no state change.
    /// Complexity: O(1) time and space.
    pub fn allow_apply(&self) -> bool {
        self.gate_open
    }

    /// 所属模块：ai / 流式计划
    /// 功能：返回已放行的操作数量。
    /// 输入：`self`。
    /// 输出/副作用：数量；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / streaming plan
    /// Purpose: Return how many operations were released.
    /// Inputs: `self`.
    /// Outputs / side effects: count; no state change.
    /// Complexity: O(1) time and space.
    pub fn emitted(&self) -> usize {
        self.emitted
    }

    /// 所属模块：ai / 流式计划
    /// 功能：返回解析过程中记录的错误。
    /// 输入：`self`。
    /// 输出/副作用：错误列表引用；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / streaming plan
    /// Purpose: Return errors recorded while parsing.
    /// Inputs: `self`.
    /// Outputs / side effects: error list reference; no state change.
    /// Complexity: O(1) time and space.
    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    /// 所属模块：ai / 流式计划
    /// 功能：返回已经流式到达的 `reply` 字符串（未闭合时返回 `None`）。
    /// 输入：`self`。
    /// 输出/副作用：回复文本或 `None`；不修改状态。
    /// 复杂度：时间 O(内容长度)，空间 O(回复长度)。
    ///
    /// Module: ai / streaming plan
    /// Purpose: Return the `reply` string that has streamed in so far (`None` while unclosed).
    /// Inputs: `self`.
    /// Outputs / side effects: reply text or `None`; no state change.
    /// Complexity: O(content length) time and O(reply length) space.
    pub fn partial_reply(&self) -> Option<String> {
        find_string_value(&self.content, "reply")
    }

    /// 所属模块：ai / 流式计划
    /// 功能：尝试解析顶层 `research` 与 `skills` 字符串数组（只解析一次）。
    /// 输入：`self`。
    /// 输出/副作用：填充缓存字段；不修改内容。
    /// 复杂度：时间 O(内容长度)，空间 O(数组大小)。
    ///
    /// Module: ai / streaming plan
    /// Purpose: Try to parse the top-level `research` and `skills` string arrays (once each).
    /// Inputs: `self`.
    /// Outputs / side effects: fills cached fields; content unchanged.
    /// Complexity: O(content length) time and O(array size) space.
    fn refresh_arrays(&mut self) {
        if self.research.is_none() {
            self.research = find_string_array(&self.content, "research");
        }
        if self.skills.is_none() {
            self.skills = find_string_array(&self.content, "skills");
        }
    }

    /// 所属模块：ai / 流式计划
    /// 功能：扫描 `operations` 数组，把新完成的完整对象解析为操作。
    /// 输入：`self`。
    /// 输出/副作用：填充 `buffered` 与 `errors`；推进扫描位置。
    /// 复杂度：时间 O(新增内容)，空间 O(操作数)。
    ///
    /// Module: ai / streaming plan
    /// Purpose: Scan the `operations` array and parse newly completed objects into operations.
    /// Inputs: `self`.
    /// Outputs / side effects: fills `buffered` and `errors`; advances the scan position.
    /// Complexity: O(new text) time and O(operations) space.
    fn scan_operations(&mut self) {
        if self.ops_done {
            return;
        }
        if self.ops_cursor.is_none() {
            let Some(cursor) = find_array_start(&self.content, "operations") else {
                return;
            };
            self.ops_cursor = Some(cursor);
            self.scan = cursor;
        }
        let mut completed: Vec<(usize, usize)> = Vec::new();
        {
            let bytes = self.content.as_bytes();
            let mut index = self.scan.min(bytes.len());
            while index < bytes.len() {
                let byte = bytes[index];
                if self.in_string {
                    if self.escape {
                        self.escape = false;
                    } else if byte == b'\\' {
                        self.escape = true;
                    } else if byte == b'"' {
                        self.in_string = false;
                    }
                    index += 1;
                    continue;
                }
                match byte {
                    b'"' => {
                        self.in_string = true;
                        index += 1;
                    }
                    b'{' => {
                        if self.depth == 0 {
                            self.object_start = Some(index);
                        }
                        self.depth += 1;
                        index += 1;
                    }
                    b'}' => {
                        if self.depth > 0 {
                            self.depth -= 1;
                        }
                        if self.depth == 0 {
                            if let Some(start) = self.object_start.take() {
                                completed.push((start, index));
                            }
                        }
                        index += 1;
                    }
                    b']' if self.depth == 0 => {
                        self.ops_done = true;
                        index += 1;
                        break;
                    }
                    _ => index += 1,
                }
            }
            self.scan = index;
        }
        for (start, end) in completed {
            let Some(text) = self.content.get(start..=end) else {
                continue;
            };
            match serde_json::from_str::<AiOperation>(text) {
                Ok(operation) => self.buffered.push(operation),
                Err(error) => self.errors.push(format!("操作解析失败：{error}")),
            }
        }
    }

    /// 所属模块：ai / 流式计划
    /// 功能：当 `research` 与 `skills` 都为空数组时打开写入闸门。
    /// 输入：`self`。
    /// 输出/副作用：可能置位 `gate_open`；不修改内容。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ai / streaming plan
    /// Purpose: Open the write gate when both `research` and `skills` are empty arrays.
    /// Inputs: `self`.
    /// Outputs / side effects: may set `gate_open`; content unchanged.
    /// Complexity: O(1) time and space.
    fn update_gate(&mut self) {
        if self.gate_open || self.ops_cursor.is_none() {
            return;
        }
        let research_empty = matches!(&self.research, Some(list) if list.is_empty());
        let skills_empty = matches!(&self.skills, Some(list) if list.is_empty());
        if research_empty && skills_empty {
            self.gate_open = true;
        }
    }

    /// 所属模块：ai / 流式计划
    /// 功能：闸门打开后把已解析的操作全部放行。
    /// 输入：`self`。
    /// 输出/副作用：清空缓冲并累计放行数量；返回操作列表。
    /// 复杂度：时间 O(操作数)，空间 O(操作数)。
    ///
    /// Module: ai / streaming plan
    /// Purpose: Release every parsed operation once the gate is open.
    /// Inputs: `self`.
    /// Outputs / side effects: clears the buffer, accumulates `emitted`; returns the operations.
    /// Complexity: O(operations) time and space.
    fn release_ready(&mut self) -> Vec<AiOperation> {
        if !self.gate_open || self.buffered.is_empty() {
            return Vec::new();
        }
        let operations = std::mem::take(&mut self.buffered);
        self.emitted += operations.len();
        operations
    }
}

/// 所属模块：ai / 流式计划 / 定位
/// 功能：定位顶层 `"key": [` 的数组内容起始位置（`[` 之后）。
/// 输入：`content` JSON 文本；`key` 字段名。
/// 输出/副作用：字节位置或 `None`；不修改输入。
/// 复杂度：时间 O(内容长度)，空间 O(1)。
///
/// Module: ai / streaming plan / lookup
/// Purpose: Locate the array content start (right after `[`) of a top-level `"key": [`.
/// Inputs: `content` JSON text; `key` field name.
/// Outputs / side effects: byte offset or `None`; the input is unchanged.
/// Complexity: O(content length) time and O(1) space.
fn find_array_start(content: &str, key: &str) -> Option<usize> {
    let needle = format!("\"{key}\"");
    let bytes = content.as_bytes();
    let mut from = 0usize;
    while from < content.len() {
        let offset = content[from..].find(&needle)?;
        let key_at = from + offset;
        let after = key_at + needle.len();
        let mut index = after;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index < bytes.len() && bytes[index] == b':' {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if index < bytes.len() && bytes[index] == b'[' {
                return Some(index + 1);
            }
        }
        from = after;
    }
    None
}

/// 所属模块：ai / 流式计划 / 定位
/// 功能：解析顶层 `"key": [...]` 的字符串数组；数组未闭合或内容非法时返回 `None`。
/// 输入：`content` JSON 文本；`key` 字段名。
/// 输出/副作用：字符串数组或 `None`；不修改输入。
/// 复杂度：时间 O(内容长度)，空间 O(数组大小)。
///
/// Module: ai / streaming plan / lookup
/// Purpose: Parse the string array of a top-level `"key": [...]`; returns `None` while the array is
/// unclosed or invalid.
/// Inputs: `content` JSON text; `key` field name.
/// Outputs / side effects: string array or `None`; the input is unchanged.
/// Complexity: O(content length) time and O(array size) space.
fn find_string_array(content: &str, key: &str) -> Option<Vec<String>> {
    let start = find_array_start(content, key)?;
    let bytes = content.as_bytes();
    let mut depth = 1i32;
    let mut in_string = false;
    let mut escape = false;
    let mut index = start;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escape {
                escape = false;
            } else if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else {
            match byte {
                b'"' => in_string = true,
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        let text = content.get(start - 1..=index)?;
                        return serde_json::from_str::<Vec<String>>(text).ok();
                    }
                }
                _ => {}
            }
        }
        index += 1;
    }
    None
}

/// 所属模块：ai / 流式计划 / 定位
/// 功能：解析顶层 `"key": "..."` 的字符串值；字符串尚未闭合时返回已到达的部分文本，
/// 非字符串值或字段不存在时返回 `None`。
/// 输入：`content` JSON 文本；`key` 字段名。
/// 输出/副作用：字符串或 `None`；不修改输入。
/// 复杂度：时间 O(内容长度)，空间 O(字符串长度)。
///
/// Module: ai / streaming plan / lookup
/// Purpose: Parse the string value of a top-level `"key": "..."`; an unclosed string returns the text
/// received so far. Returns `None` for non-string values or a missing key.
/// Inputs: `content` JSON text; `key` field name.
/// Outputs / side effects: string or `None`; the input is unchanged.
/// Complexity: O(content length) time and O(string length) space.
fn find_string_value(content: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let bytes = content.as_bytes();
    let mut from = 0usize;
    while from < content.len() {
        let offset = content[from..].find(&needle)?;
        let key_at = from + offset;
        let after = key_at + needle.len();
        let mut index = after;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index < bytes.len() && bytes[index] == b':' {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if index < bytes.len() && bytes[index] == b'"' {
                let start = index;
                index += 1;
                let mut escape = false;
                while index < bytes.len() {
                    let byte = bytes[index];
                    if escape {
                        escape = false;
                    } else if byte == b'\\' {
                        escape = true;
                    } else if byte == b'"' {
                        let text = content.get(start..=index)?;
                        return serde_json::from_str::<String>(text).ok();
                    }
                    index += 1;
                }
                // 字符串尚未闭合：展示已经到达的部分，让回复逐字出现。
                // The string is still open: show what has arrived so the reply types out.
                let partial = decode_partial_string(&content[start + 1..]);
                return (!partial.trim().is_empty()).then_some(partial);
            }
        }
        from = after;
    }
    None
}

/// 所属模块：ai / 流式计划 / 定位
/// 功能：把尚未闭合的 JSON 字符串片段转换为可读文本；遇到不完整的 `\u` 转义时停止。
/// 输入：`raw`：不含起始引号的原始片段。
/// 输出/副作用：可读文本；不修改输入。
/// 复杂度：时间 O(片段长度)，空间 O(片段长度)。
///
/// Module: ai / streaming plan / lookup
/// Purpose: Convert an unclosed JSON string fragment into readable text; stops at an incomplete
/// `\u` escape.
/// Inputs: `raw`: fragment without the opening quote.
/// Outputs / side effects: readable text; the input is unchanged.
/// Complexity: O(fragment length) time and space.
fn decode_partial_string(raw: &str) -> String {
    let mut text = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            text.push(character);
            continue;
        }
        let Some(escaped) = chars.next() else {
            break;
        };
        match escaped {
            'n' => text.push('\n'),
            't' => text.push('\t'),
            'r' => text.push('\r'),
            'b' | 'f' => text.push(' '),
            '"' => text.push('"'),
            '\\' => text.push('\\'),
            '/' => text.push('/'),
            'u' => {
                let hex: String = chars.by_ref().take(4).collect();
                if hex.chars().count() < 4 {
                    break;
                }
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(decoded) => text.push(decoded),
                    None => break,
                }
            }
            other => text.push(other),
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::plan::AiOperation;

    /// 覆盖：分块到达时，只有完整操作才被放行，且逐步返回。
    /// Covers: with chunked input, only complete operations are released, step by step.
    #[test]
    fn complete_operations_are_released_incrementally() {
        let mut stream = PlanStream::new();
        assert!(
            stream
                .push(r#"{"reply":"ok","research":[],"skills":[],"#)
                .is_empty()
        );
        assert_eq!(
            stream
                .push(r#""operations":[{"op":"set_tempo","bpm":88},"#)
                .len(),
            1
        );
        let released =
            stream.push(r#"{"op":"add_track","name":"钢琴","instrument":"GrandPiano"}]}"#);
        assert_eq!(released.len(), 1);
        assert!(matches!(released[0], AiOperation::AddTrack { .. }));
        assert_eq!(stream.emitted(), 2);
        assert!(stream.allow_apply());
    }

    /// 覆盖：请求研究或技巧时不放行任何操作。
    /// Covers: no operation is released while research or techniques are requested.
    #[test]
    fn research_or_skills_keep_the_gate_closed() {
        let mut research = PlanStream::new();
        let released = research.push(
            r#"{"reply":"查一下","research":["Serum review"],"skills":[],"operations":[{"op":"set_tempo","bpm":90}]}"#,
        );
        assert!(released.is_empty());
        assert!(!research.allow_apply());
        assert_eq!(research.emitted(), 0);

        let mut skills = PlanStream::new();
        let released = skills.push(
            r#"{"reply":"学一下","research":[],"skills":["groove-pocket"],"operations":[{"op":"set_tempo","bpm":90}]}"#,
        );
        assert!(released.is_empty());
        assert!(!skills.allow_apply());
    }

    /// 覆盖：字段顺序异常时不提前放行，调用方可在完整解析后补写。
    /// Covers: an unexpected field order keeps the gate closed so the caller can apply the tail later.
    #[test]
    fn operations_before_skills_do_not_leak_early() {
        let mut stream = PlanStream::new();
        let released = stream.push(r#"{"operations":[{"op":"set_tempo","bpm":90}],"skills":[]}"#);
        assert!(released.is_empty());
        assert!(!stream.allow_apply());
    }

    /// 覆盖：字符串数组未闭合时返回 None，闭合后可解析。
    /// Covers: an unclosed string array is `None` and parses once closed.
    #[test]
    fn string_arrays_parse_when_closed() {
        assert_eq!(
            find_string_array(r#"{"research":["a","b"]}"#, "research"),
            Some(vec!["a".to_string(), "b".to_string()])
        );
        assert_eq!(find_string_array(r#"{"research":["a""#, "research"), None);
        assert_eq!(find_string_array("{}", "research"), None);
    }

    /// 覆盖：`reply` 字符串可以边流式到达边展示，含转义与未闭合内容。
    /// Covers: the `reply` string shows while streaming, including escapes and unclosed text.
    #[test]
    fn partial_reply_streams_before_the_closing_quote() {
        let mut stream = PlanStream::new();
        stream.push(r#"{"reply":"正在写一"#);
        assert_eq!(stream.partial_reply().as_deref(), Some("正在写一"));
        stream.push(r#"段旋律\n第二行","research":[]"#);
        assert_eq!(
            stream.partial_reply().as_deref(),
            Some("正在写一段旋律\n第二行")
        );
    }
}
