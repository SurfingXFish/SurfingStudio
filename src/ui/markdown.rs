//! 所属模块：ui / Markdown 预览
//! 文件用途：
//! - 在 AI 对话与 Agent 资料编辑器中把 Markdown 渲染成带层级的富文本：标题、列表、代码块、
//!   加粗与行内代码。
//! - 只依赖 egui 与主题色，不做任何文件或网络操作。
//!
//! 函数清单：
//! - `render`：按基础字号逐行渲染 Markdown。
//! - `render_inline` / `parse_inline` / `split_numbered`：行内片段与有序列表解析。
//!
//! Module: ui / Markdown preview
//! Purpose:
//! - Render Markdown as structured rich text in the AI chat and the Agent editor: headings, lists,
//!   code blocks, bold and inline code.
//! - Depends on egui and theme colors only; never touches files or the network.
//!
//! Functions:
//! - `render`: render Markdown line by line at a base font size.
//! - `render_inline` / `parse_inline` / `split_numbered`: inline spans and ordered-list parsing.

use crate::ui::theme;
use eframe::egui::{self, RichText};

/// 所属模块：ui / Markdown 预览
/// 功能：逐行渲染 Markdown；标题按基础字号递增，代码块使用浅表面底，列表带缩进。
/// 输入：
/// - `ui`：绘制区域。
/// - `text`：Markdown 原文。
/// - `base`：正文字号（逻辑点）。
/// 输出/副作用：只绘制；不修改状态。
/// 复杂度：时间 O(文本长度)，空间 O(行数)。
///
/// Module: ui / Markdown preview
/// Purpose: Render Markdown line by line; headings scale from the base size, code blocks get a
/// raised surface and lists are indented.
/// Inputs:
/// - `ui`: drawing area.
/// - `text`: Markdown source.
/// - `base`: body font size in logical points.
/// Outputs / side effects: paint only; no state change.
/// Complexity: O(text length) time and O(lines) space.
pub fn render(ui: &mut egui::Ui, text: &str, base: f32) {
    let mut in_code = false;
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim_start().starts_with("```") {
            in_code = !in_code;
            ui.add_space(theme::SPACE_XS);
            continue;
        }
        if in_code {
            egui::Frame::NONE
                .fill(theme::PANEL_2)
                .corner_radius(6)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(line)
                            .monospace()
                            .size(base - 0.5)
                            .color(theme::TEXT_DIM),
                    );
                });
            continue;
        }
        let trimmed_start = trimmed.trim_start();
        if trimmed.trim().is_empty() {
            ui.add_space(theme::SPACE_SM);
            continue;
        }
        if trimmed_start == "---" || trimmed_start == "***" {
            ui.add_space(theme::SPACE_SM);
            ui.separator();
            ui.add_space(theme::SPACE_SM);
            continue;
        }
        if let Some(rest) = trimmed_start.strip_prefix("### ") {
            ui.add_space(theme::SPACE_SM);
            render_inline(ui, rest, base + 1.0, true);
            continue;
        }
        if let Some(rest) = trimmed_start.strip_prefix("## ") {
            ui.add_space(theme::SPACE_SM);
            render_inline(ui, rest, base + 2.5, true);
            continue;
        }
        if let Some(rest) = trimmed_start.strip_prefix("# ") {
            ui.add_space(theme::SPACE_MD);
            render_inline(ui, rest, base + 4.0, true);
            continue;
        }
        if let Some(rest) = trimmed_start
            .strip_prefix("- ")
            .or_else(|| trimmed_start.strip_prefix("* "))
        {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(theme::SPACE_MD);
                ui.label(RichText::new("•").size(base - 1.0).color(theme::TEXT_DIM));
                render_inline(ui, rest, base, false);
            });
            continue;
        }
        if let Some((number, rest)) = split_numbered(trimmed_start) {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(theme::SPACE_MD);
                ui.label(
                    RichText::new(format!("{number}."))
                        .size(base - 1.0)
                        .color(theme::TEXT_DIM),
                );
                render_inline(ui, rest, base, false);
            });
            continue;
        }
        render_inline(ui, trimmed_start, base, false);
    }
}

/// 所属模块：ui / Markdown 预览
/// 功能：解析 `**加粗**` 与 `` `行内代码` `` 后按行内片段绘制。
/// 输入：`ui`、文本、字号、是否整体加粗。
/// 输出/副作用：只绘制；不修改状态。
/// 复杂度：时间 O(字符数)，空间 O(片段数)。
///
/// Module: ui / Markdown preview
/// Purpose: Parse `**bold**` and `` `inline code` `` then draw inline spans.
/// Inputs: `ui`, text, size, whole-line bold flag.
/// Outputs / side effects: paint only; no state change.
/// Complexity: O(characters) time and O(spans) space.
fn render_inline(ui: &mut egui::Ui, text: &str, size: f32, bold: bool) {
    let spans = parse_inline(text);
    ui.horizontal_wrapped(|ui| {
        for (span, span_bold, span_code) in spans {
            let mut rich = RichText::new(span).size(size).color(theme::TEXT);
            if bold || span_bold {
                rich = rich.strong();
            }
            if span_code {
                rich = rich.monospace().background_color(theme::PANEL_2);
            }
            ui.label(rich);
        }
    });
}

/// 所属模块：ui / Markdown 预览
/// 功能：把一行文本拆成普通、加粗与行内代码片段。
/// 输入：`text` 单行文本。
/// 输出/副作用：片段列表；不修改输入。
/// 复杂度：时间 O(字符数)，空间 O(片段数)。
///
/// Module: ui / Markdown preview
/// Purpose: Split one line into normal, bold and inline-code spans.
/// Inputs: `text`.
/// Outputs / side effects: spans; the input is unchanged.
/// Complexity: O(characters) time and O(spans) space.
fn parse_inline(text: &str) -> Vec<(String, bool, bool)> {
    let mut spans = Vec::new();
    let mut buffer = String::new();
    let mut bold = false;
    let mut code = false;
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '`' {
            if !buffer.is_empty() {
                spans.push((std::mem::take(&mut buffer), bold, code));
            }
            code = !code;
            index += 1;
            continue;
        }
        if chars[index] == '*' && index + 1 < chars.len() && chars[index + 1] == '*' {
            if !buffer.is_empty() {
                spans.push((std::mem::take(&mut buffer), bold, code));
            }
            bold = !bold;
            index += 2;
            continue;
        }
        buffer.push(chars[index]);
        index += 1;
    }
    if !buffer.is_empty() {
        spans.push((buffer, bold, code));
    }
    spans
}

/// 所属模块：ui / Markdown 预览
/// 功能：识别 `1. ` 形式的有序列表前缀。
/// 输入：`text` 单行文本。
/// 输出/副作用：序号与剩余文本；不是有序列表返回 `None`。
/// 复杂度：时间 O(数字位数)，空间 O(1)。
///
/// Module: ui / Markdown preview
/// Purpose: Recognise a `1. ` ordered-list prefix.
/// Inputs: `text`.
/// Outputs / side effects: number and remainder, or `None`.
/// Complexity: O(digits) time and O(1) space.
fn split_numbered(text: &str) -> Option<(&str, &str)> {
    let dot = text.find(". ")?;
    let (number, rest) = text.split_at(dot);
    if !number.is_empty() && number.chars().all(|character| character.is_ascii_digit()) {
        Some((number, &rest[2..]))
    } else {
        None
    }
}
