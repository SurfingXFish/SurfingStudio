//! 所属模块：ui / AI 编曲面板
//! 文件用途：
//! - 绘制右侧 AI 编曲对话：消息、工程改动卡片、快捷指令与圆角输入区。
//! - 输入区底部提供模型、智能体、附件与发送/停止按钮，接近聊天应用的编辑器样式。
//! - 只发出用户意图（发送、停止、添加附件、打开设置），网络与工程写入由 `app::ai` 完成。
//!
//! 函数清单：
//! - `SurfingApp::ui_ai_panel`：绘制完整面板。
//! - `SurfingApp::ui_ai_composer`：圆角输入区、附件、模型/智能体/推理选择与发送/停止。
//! - `paint_ai_message` / `paint_ai_live` / `paint_message_meta` / `paint_thinking_block` /
//!   `paint_change_card` / `paint_ai_empty`：气泡、流式生成、任务信息、折叠思考、工程改动卡片与空状态。
//!
//! Module: ui / AI composition panel
//! Purpose:
//! - Draw the right-hand AI chat: messages, reasoning, a project-change card, quick prompts and a
//!   rounded composer.
//! - The composer bottom row offers model, agent, reasoning, attachment and send/stop controls.
//! - Only emits user intents; networking and project writes live in `app::ai`.
//!
//! Functions:
//! - `SurfingApp::ui_ai_panel`: draw the whole panel.
//! - `SurfingApp::ui_ai_composer`: rounded input, attachments, selectors and send/stop.
//! - `paint_ai_message` / `paint_ai_live` / `paint_message_meta` / `paint_thinking_block` /
//!   `paint_change_card` / `paint_ai_empty`: bubbles, streaming state, metadata, collapsible
//!   reasoning, change card and empty state.

use crate::ai::skills::AiAgentKind;
use crate::ai::{AiApplySummary, AiReasoning, AiRole, MODEL_PRESETS, find_preset};
use crate::app::{AiLiveMessage, AiMessageMeta, SurfingApp};
use crate::ui::theme;
use eframe::egui::{self, RichText};

/// 输入区预留高度（逻辑点）：附件条 + 文本框 + 底部选择行。
/// Height reserved for the composer in logical points: attachments + text box + bottom row.
const AI_INPUT_H: f32 = 176.0;
/// 对话为空时展示的示例问题。
/// Example prompts shown while the conversation is empty.
const EMPTY_PROMPTS: [&str; 4] = [
    "写一段 90 秒的钢琴与弦乐轻音乐，C 大调，84 BPM，温柔一点。",
    "在现有工程上加入长笛主旋律和竖琴琶音，8 小节。",
    "把这段旋律改成 3/4 拍的圆舞曲，配弦乐拨奏。",
    "来一段中国风轻音乐：古筝、二胡、钢琴，五声音阶。",
];
/// 有对话后展示的快捷指令。
/// Quick prompts shown once the conversation has started.
const QUICK_PROMPTS: [&str; 3] = ["继续发展这段旋律", "加一段弦乐铺底", "换成 3/4 拍"];

impl SurfingApp {
    /// 所属模块：ui / AI 编曲面板
    /// 功能：绘制 AI 编曲对话：标题栏、密钥提示、消息与改动卡片、快捷指令、圆角输入区。
    /// 输入：`ui`：右侧面板的绘制区域。
    /// 输出/副作用：读写 `ai_chat`；可能发送消息、打开设置或附件选择器；不直接修改工程。
    /// 复杂度：每帧 O(消息数 + 消息文本长度)。
    ///
    /// Module: ui / AI composition panel
    /// Purpose: Draw the AI chat: header, key hint, messages and change cards, quick prompts and the
    /// rounded composer.
    /// Inputs: `ui`: right-panel drawing area.
    /// Outputs / side effects: reads/writes `ai_chat`; may send a message, open settings or the
    /// attachment picker; never edits the project directly.
    /// Complexity: O(messages + message length) per frame.
    pub fn ui_ai_panel(&mut self, ui: &mut egui::Ui) {
        let mut close = false;
        let mut open_settings = false;
        let mut new_chat = false;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("AI 编曲")
                    .color(theme::ACCENT)
                    .strong()
                    .size(14.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("×").size(15.0))
                            .min_size(egui::vec2(24.0, 22.0)),
                    )
                    .on_hover_text("关闭面板")
                    .clicked()
                {
                    close = true;
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new("⚙").size(12.0))
                            .min_size(egui::vec2(24.0, 22.0)),
                    )
                    .on_hover_text("AI 设置（模型、API Key、联网检索、采样参数）")
                    .clicked()
                {
                    open_settings = true;
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new("新对话").size(10.5))
                            .min_size(egui::vec2(52.0, 22.0)),
                    )
                    .on_hover_text("清空当前对话与附件")
                    .clicked()
                {
                    new_chat = true;
                }
            });
        });
        if close {
            self.close_side_panel();
            return;
        }
        if open_settings {
            self.open_ai_settings(ui.ctx());
        }
        if new_chat {
            self.ai_chat.clear();
        }
        ui.add_space(4.0);

        // 让用户看到 AI 实际能调用多少 VST 插件，避免误以为只用了内置音色。
        // Show how many VST plugins the AI can actually call, so built-in-only output is not mistaken
        // for missing support.
        if crate::platform::caps().plugin_hosting {
            let plugin_count = self.plugin_cache.len();
            let (text, color) = if plugin_count > 0 {
                (
                    format!("已连接 {plugin_count} 个 VST 插件 · AI 会优先使用"),
                    theme::GREEN,
                )
            } else {
                (
                    "尚未扫描到 VST 插件 · AI 将使用内置音色".to_string(),
                    theme::TEXT_FAINT,
                )
            };
            ui.label(RichText::new(text).size(9.5).color(color));
            ui.add_space(3.0);
        }

        // 缺少密钥时把入口放在面板内，避免用户只看到失败提示。
        // Show the setup entry inside the panel when the key is missing.
        let missing_key = find_preset(&self.settings.ai.model_preset)
            .map(|preset| {
                self.settings
                    .ai
                    .api_key(preset.provider.id())
                    .trim()
                    .is_empty()
            })
            .unwrap_or(true);
        if missing_key {
            egui::Frame::NONE
                .fill(theme::PANEL_2)
                .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                .corner_radius(8)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new("尚未配置 API Key，AI 编曲无法发送请求")
                            .size(11.5)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new("在 设置 → AI 里填写 DeepSeek / MiMo / 自定义密钥。")
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );
                    ui.add_space(4.0);
                    if ui
                        .add(
                            egui::Button::new(RichText::new("配置 API Key").size(10.5))
                                .fill(theme::ACCENT_DARK)
                                .stroke(egui::Stroke::new(1.0_f32, theme::ACCENT))
                                .min_size(egui::vec2(0.0, 24.0)),
                        )
                        .clicked()
                    {
                        self.open_ai_settings(ui.ctx());
                    }
                });
            ui.add_space(6.0);
        }
        if let Some(error) = self.ai_chat.error.clone() {
            egui::Frame::NONE
                .fill(theme::PANEL_2)
                .stroke(egui::Stroke::new(1.0_f32, theme::RED))
                .corner_radius(6)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(error).size(10.5).color(theme::RED));
                });
            ui.add_space(4.0);
        }

        let quick_extra = if !self.ai_chat.messages.is_empty() && self.ai_chat.pending.is_none() {
            32.0
        } else {
            0.0
        };
        let transcript_h = (ui.available_height() - AI_INPUT_H - quick_extra).max(120.0);
        let mut empty_pick: Option<&'static str> = None;
        // 深色聊天画布：与输入区的浅色卡片形成层级，接近常见聊天应用。
        // A darker chat canvas sets the composer card apart, like common chat apps.
        egui::Frame::NONE
            .fill(theme::BG)
            .corner_radius(8)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_min_height(transcript_h);
                egui::ScrollArea::vertical()
                    .id_salt("ai_transcript")
                    .max_height((transcript_h - 16.0).max(80.0))
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if self.ai_chat.messages.is_empty() {
                            empty_pick = paint_ai_empty(ui);
                        } else {
                            for (index, message) in self.ai_chat.messages.iter().enumerate() {
                                paint_ai_message(
                                    ui,
                                    index,
                                    message.role,
                                    &message.content,
                                    message.reasoning.as_deref(),
                                    message.meta.as_ref(),
                                    message.changes,
                                );
                                ui.add_space(10.0);
                            }
                        }
                        if let Some(live) = self.ai_chat.live.as_ref() {
                            paint_ai_live(ui, live);
                            ui.add_space(10.0);
                        } else if self.ai_chat.pending.is_some() {
                            ui.horizontal(|ui| {
                                ui.add(egui::Spinner::new().size(14.0));
                                ui.label(
                                    RichText::new("AI 正在思考…（可点输入区右侧停止）")
                                        .size(10.5)
                                        .color(theme::TEXT_DIM),
                                );
                            });
                        }
                    });
            });
        if let Some(prompt) = empty_pick {
            self.ai_chat.input = prompt.to_string();
        }

        // 有对话后给出几个可直接修改的快捷指令；点击只填入输入框，不立即发送。
        // Quick prompts after the first turn; clicking fills the composer instead of sending.
        if !self.ai_chat.messages.is_empty() && self.ai_chat.pending.is_none() {
            let mut quick: Option<&str> = None;
            ui.horizontal_wrapped(|ui| {
                for prompt in QUICK_PROMPTS {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(prompt).size(10.0))
                                .fill(theme::PANEL_2)
                                .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                                .corner_radius(12)
                                .min_size(egui::vec2(0.0, 22.0)),
                        )
                        .clicked()
                    {
                        quick = Some(prompt);
                    }
                }
            });
            if let Some(prompt) = quick {
                self.ai_chat.input = prompt.to_string();
            }
            ui.add_space(4.0);
        }

        self.ui_ai_composer(ui);
    }

    /// 所属模块：ui / AI 编曲面板 / 输入区
    /// 功能：绘制圆角输入区：附件条、多行输入、加附件、智能体、模型与发送/停止按钮。
    /// 输入：`ui`：右侧面板的绘制区域。
    /// 输出/副作用：读写 `ai_chat.input`、模型预设与附件；可能发送、停止或打开附件选择器。
    /// 复杂度：每帧 O(附件数 + 模型预设数)。
    ///
    /// Module: ui / AI composition panel / composer
    /// Purpose: Draw the rounded composer: attachments, multiline input, attach, agent, model and
    /// send/stop controls.
    /// Inputs: `ui`: right-panel drawing area.
    /// Outputs / side effects: reads/writes input, model preset and attachments; may send, stop or open
    /// the attachment picker.
    /// Complexity: O(attachments + model presets) per frame.
    fn ui_ai_composer(&mut self, ui: &mut egui::Ui) {
        let mut send = false;
        let mut stop = false;
        let mut attach = false;
        let mut preset_changed = false;
        let mut reasoning_changed = false;
        let send_shortcut =
            ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Enter));
        egui::Frame::NONE
            .fill(theme::PANEL_2)
            .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // 附件条：每个附件可单独移除。
                // Attachment chips: each one can be removed.
                if !self.ai_chat.attachments.is_empty() {
                    let mut remove: Option<usize> = None;
                    ui.horizontal_wrapped(|ui| {
                        for (index, attachment) in self.ai_chat.attachments.iter().enumerate() {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(format!("{}  ×", attachment.name)).size(9.5),
                                    )
                                    .fill(theme::PANEL_3)
                                    .corner_radius(8)
                                    .min_size(egui::vec2(0.0, 20.0)),
                                )
                                .on_hover_text("点击移除该附件")
                                .clicked()
                            {
                                remove = Some(index);
                            }
                        }
                    });
                    if let Some(index) = remove {
                        if index < self.ai_chat.attachments.len() {
                            self.ai_chat.attachments.remove(index);
                        }
                    }
                    ui.add_space(2.0);
                }

                ui.add(
                    egui::TextEdit::multiline(&mut self.ai_chat.input)
                        .frame(egui::Frame::NONE)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .hint_text("描述你想要的音乐；也可以说“用已安装的 Serum 做一个 Pad”…"),
                );

                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("+").size(16.0).color(theme::TEXT_DIM))
                                .fill(theme::PANEL_3)
                                .corner_radius(6)
                                .min_size(egui::vec2(24.0, 24.0)),
                        )
                        .on_hover_text("添加文本 / Markdown / JSON / MIDI 附件")
                        .clicked()
                    {
                        attach = true;
                    }
                    let mut agent = self.ai_chat.agent;
                    egui::ComboBox::from_id_salt("ai_agent")
                        .width(80.0)
                        .selected_text(RichText::new(agent.label()).size(9.5))
                        .show_ui(ui, |ui| {
                            for entry in AiAgentKind::ALL {
                                ui.selectable_value(&mut agent, entry, entry.label());
                            }
                        });
                    if agent != self.ai_chat.agent {
                        self.ai_chat.agent = agent;
                    }

                    let mut preset_id = self.settings.ai.model_preset.clone();
                    let preset_label = find_preset(&preset_id)
                        .map(|preset| preset.label)
                        .unwrap_or("自定义");
                    egui::ComboBox::from_id_salt("ai_model_preset")
                        .width(100.0)
                        .selected_text(RichText::new(preset_label).size(9.5))
                        .show_ui(ui, |ui| {
                            for entry in MODEL_PRESETS {
                                ui.selectable_value(
                                    &mut preset_id,
                                    entry.id.to_string(),
                                    entry.label,
                                );
                            }
                        });
                    if preset_id != self.settings.ai.model_preset {
                        self.settings.ai.model_preset = preset_id;
                        preset_changed = true;
                    }

                    let current_reasoning = AiReasoning::from_id(&self.settings.ai.reasoning);
                    let mut reasoning = current_reasoning;
                    egui::ComboBox::from_id_salt("ai_reasoning")
                        .width(58.0)
                        .selected_text(
                            RichText::new(format!("推理 {}", reasoning.label())).size(9.0),
                        )
                        .show_ui(ui, |ui| {
                            for entry in AiReasoning::ALL {
                                ui.selectable_value(&mut reasoning, entry, entry.label());
                            }
                        });
                    if reasoning != current_reasoning {
                        self.settings.ai.reasoning = reasoning.id().to_string();
                        reasoning_changed = true;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.ai_chat.pending.is_some() {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("■").size(12.0).color(egui::Color32::WHITE),
                                    )
                                    .fill(theme::RED)
                                    .corner_radius(16)
                                    .min_size(egui::vec2(32.0, 32.0)),
                                )
                                .on_hover_text("停止等待当前回复")
                                .clicked()
                            {
                                stop = true;
                            }
                        } else {
                            let can_send = !self.ai_chat.input.trim().is_empty()
                                || !self.ai_chat.attachments.is_empty();
                            let fill = if can_send {
                                theme::ACCENT
                            } else {
                                theme::PANEL_3
                            };
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("↑").size(15.0).color(egui::Color32::WHITE),
                                    )
                                    .fill(fill)
                                    .corner_radius(16)
                                    .min_size(egui::vec2(32.0, 32.0)),
                                )
                                .on_hover_text("发送（Ctrl+Enter）")
                                .clicked()
                                && can_send
                            {
                                send = true;
                            }
                        }
                    });
                });
            });

        if preset_changed || reasoning_changed {
            self.commit_settings();
        }
        if attach {
            self.open_ai_attachment_dialog();
        }
        if stop {
            self.cancel_ai_message();
        }
        if (send || send_shortcut) && self.ai_chat.pending.is_none() {
            self.send_ai_message();
        }
    }
}

/// 所属模块：ui / AI 编曲面板 / 气泡
/// 功能：绘制一条对话气泡：任务元信息、可折叠思考过程、内容与“工程已更新”卡片。
/// 输入：`ui` 绘制区域；`index` 消息序号（用于折叠状态）；`role` 角色；`content` 文本；
/// `reasoning` 思考过程；`meta` 任务信息；`changes` 结构化工程改动。
/// 输出/副作用：只绘制；不修改状态。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: ui / AI composition panel / bubble
/// Purpose: Draw one chat bubble: task metadata, collapsible reasoning, content and the project card.
/// Inputs: `ui` drawing area; `index` message index (collapse state); `role` role; `content` text;
/// `reasoning` trace; `meta` task info; `changes` structured project changes.
/// Outputs / side effects: paints only; no state change.
/// Complexity: O(text length) time and O(1) space.
fn paint_ai_message(
    ui: &mut egui::Ui,
    index: usize,
    role: AiRole,
    content: &str,
    reasoning: Option<&str>,
    meta: Option<&AiMessageMeta>,
    changes: Option<AiApplySummary>,
) {
    let is_user = role == AiRole::User;
    if !is_user {
        if let Some(meta) = meta {
            paint_message_meta(ui, meta);
            ui.add_space(3.0);
        }
        if let Some(reasoning) = reasoning {
            paint_thinking_block(ui, index, reasoning, false, false);
            ui.add_space(4.0);
        }
    }
    let (fill, text_color) = if is_user {
        (theme::ACCENT_DARK, egui::Color32::WHITE)
    } else {
        (theme::PANEL_2, theme::TEXT)
    };
    let bubble = |ui: &mut egui::Ui| {
        egui::Frame::NONE
            .fill(fill)
            .stroke(egui::Stroke::new(
                1.0_f32,
                if is_user {
                    theme::ACCENT_DARK
                } else {
                    theme::LINE
                },
            ))
            .corner_radius(8)
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_max_width((ui.available_width() * 0.94).max(160.0));
                ui.label(RichText::new(content).size(11.5).color(text_color));
            });
    };
    if is_user {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), bubble);
    } else {
        bubble(ui);
    }
    if let Some(summary) = changes {
        ui.add_space(6.0);
        paint_change_card(ui, summary);
    }
}

/// 所属模块：ui / AI 编曲面板 / 任务信息
/// 功能：绘制助手消息顶部的耗时、检索次数与已加载技巧。
/// 输入：`ui` 绘制区域；`meta` 任务信息。
/// 输出/副作用：只绘制；不修改状态。
/// 复杂度：时间 O(技巧数)，空间 O(1)。
///
/// Module: ui / AI composition panel / metadata
/// Purpose: Draw elapsed time, search count and loaded techniques above an assistant message.
/// Inputs: `ui` drawing area; `meta` task info.
/// Outputs / side effects: paints only; no state change.
/// Complexity: O(techniques) time and O(1) space.
fn paint_message_meta(ui: &mut egui::Ui, meta: &AiMessageMeta) {
    let mut parts = Vec::new();
    if meta.elapsed_seconds > 0.0 {
        parts.push(format!("耗时 {:.1}s", meta.elapsed_seconds));
    }
    if meta.searches > 0 {
        parts.push(format!("检索 {} 次", meta.searches));
    }
    if !meta.skills.is_empty() {
        parts.push(format!("加载技巧 {}", meta.skills.join("、")));
    }
    if parts.is_empty() {
        return;
    }
    ui.label(
        RichText::new(parts.join(" · "))
            .size(9.5)
            .color(theme::TEXT_FAINT),
    );
}

/// 所属模块：ui / AI 编曲面板 / 思考
/// 功能：绘制可折叠的“思考”卡片：标题行 + 限高滚动正文；流式时默认展开并吸底。
/// 输入：`ui` 绘制区域；`index` 消息序号；`reasoning` 思考文本；`open` 是否默认展开；
/// `streaming` 是否仍在流式生成。
/// 输出/副作用：只绘制；不修改状态。
/// 复杂度：时间 O(思考文本长度)，空间 O(1)。
///
/// Module: ui / AI composition panel / reasoning
/// Purpose: Draw a collapsible “thinking” card: header row plus a height-limited scrolling body;
/// opened and stuck to the bottom while streaming.
/// Inputs: `ui` drawing area; `index` message index; `reasoning` trace text; `open` whether to expand
/// by default; `streaming` whether generation is still running.
/// Outputs / side effects: paints only; no state change.
/// Complexity: O(reasoning length) time and O(1) space.
fn paint_thinking_block(
    ui: &mut egui::Ui,
    index: usize,
    reasoning: &str,
    open: bool,
    streaming: bool,
) {
    egui::Frame::NONE
        .fill(theme::BG_DEEP)
        .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.set_max_width((ui.available_width() * 0.96).max(200.0));
            let title = if streaming { "思考中…" } else { "思考" };
            egui::CollapsingHeader::new(RichText::new(title).size(10.5).color(theme::TEXT_DIM))
                .id_salt(("ai_thinking", index))
                .default_open(open)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("ai_thinking_body", index))
                        .max_height(220.0)
                        .stick_to_bottom(streaming)
                        .show(ui, |ui| {
                            ui.label(RichText::new(reasoning).size(10.5).color(theme::TEXT_DIM));
                        });
                });
        });
}

/// 所属模块：ui / AI 编曲面板 / 流式
/// 功能：绘制正在生成的助手消息：耗时与写入数量、实时思考、流式回复与最近写入步骤。
/// 输入：`ui` 绘制区域；`live` 流式状态。
/// 输出/副作用：只绘制；不修改状态。
/// 复杂度：时间 O(思考长度 + 步骤数)，空间 O(1)。
///
/// Module: ui / AI composition panel / streaming
/// Purpose: Draw the streaming assistant message: elapsed time and write count, live reasoning,
/// streamed reply and the latest applied steps.
/// Inputs: `ui` drawing area; `live` streaming state.
/// Outputs / side effects: paints only; no state change.
/// Complexity: O(reasoning length + steps) time and O(1) space.
fn paint_ai_live(ui: &mut egui::Ui, live: &AiLiveMessage) {
    let mut parts = vec![format!(
        "生成中 {:.1}s",
        live.started.elapsed().as_secs_f32()
    )];
    if !live.steps.is_empty() {
        parts.push(format!("已写入 {} 项", live.steps.len()));
    }
    ui.label(
        RichText::new(parts.join(" · "))
            .size(9.5)
            .color(theme::TEXT_FAINT),
    );
    ui.add_space(3.0);
    if !live.reasoning.trim().is_empty() {
        paint_thinking_block(ui, usize::MAX, &live.reasoning, true, true);
        ui.add_space(4.0);
    }
    let text = if live.content.trim().is_empty() {
        live.stage.clone()
    } else {
        format!("{}▍", live.content.trim())
    };
    egui::Frame::NONE
        .fill(theme::PANEL_2)
        .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
        .corner_radius(8)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_max_width((ui.available_width() * 0.94).max(160.0));
            ui.label(RichText::new(text).size(11.5).color(theme::TEXT));
        });
    if !live.steps.is_empty() {
        ui.add_space(4.0);
        let first = live.steps.len().saturating_sub(6);
        for step in &live.steps[first..] {
            ui.label(
                RichText::new(format!("· {step}"))
                    .size(9.5)
                    .color(theme::GREEN),
            );
        }
    }
    if let Some(error) = &live.failed {
        ui.label(
            RichText::new(format!("写入中断：{error}"))
                .size(10.0)
                .color(theme::RED),
        );
    }
}

/// 所属模块：ui / AI 编曲面板 / 改动卡片
/// 功能：绘制“工程已更新”卡片：轨道、片段、音符、效果器、VST 插件与清空操作。
/// 输入：`ui` 绘制区域；`summary` 结构化改动。
/// 输出/副作用：只绘制；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / AI composition panel / change card
/// Purpose: Draw the "project updated" card: tracks, clips, notes, effects, VST plugins and clearing.
/// Inputs: `ui` drawing area; `summary` structured changes.
/// Outputs / side effects: paints only; no state change.
/// Complexity: O(1) time and space.
fn paint_change_card(ui: &mut egui::Ui, summary: AiApplySummary) {
    let total = summary.tracks_added
        + summary.tracks_updated
        + summary.clips_added
        + summary.notes_added
        + summary.effects_added
        + summary.plugins_added;
    egui::Frame::NONE
        .fill(theme::PANEL_2)
        .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_max_width((ui.available_width() * 0.96).max(220.0));
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("工程已更新")
                        .size(11.5)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("+{total} 项"))
                            .size(10.0)
                            .color(theme::GREEN),
                    );
                });
            });
            ui.add_space(4.0);
            egui::Grid::new("ai_change_grid")
                .num_columns(3)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    change_rows(ui, &summary);
                });
        });
}

/// 所属模块：ui / AI 编曲面板 / 改动卡片
/// 功能：按非零项输出改动行（名称、数量、颜色）。
/// 输入：`ui` 网格区域；`summary` 结构化改动。
/// 输出/副作用：只绘制；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / AI composition panel / change card
/// Purpose: Emit non-zero change rows (label, count, colour).
/// Inputs: `ui` grid area; `summary` structured changes.
/// Outputs / side effects: paints only; no state change.
/// Complexity: O(1) time and space.
fn change_rows(ui: &mut egui::Ui, summary: &AiApplySummary) {
    let mut row = |label: &str, value: String, color: egui::Color32| {
        ui.label(RichText::new(label).size(10.0).color(theme::TEXT_DIM));
        ui.label(RichText::new(value).size(10.5).strong().color(color));
        ui.end_row();
    };
    if summary.tracks_added > 0 {
        row(
            "新建轨道",
            format!("+{}", summary.tracks_added),
            theme::GREEN,
        );
    }
    if summary.tracks_updated > 0 {
        row(
            "调整轨道",
            format!("{}", summary.tracks_updated),
            theme::AMBER,
        );
    }
    if summary.clips_added > 0 {
        row(
            "写入片段",
            format!("+{}", summary.clips_added),
            theme::GREEN,
        );
    }
    if summary.notes_added > 0 {
        row(
            "写入音符",
            format!("+{}", summary.notes_added),
            theme::GREEN,
        );
    }
    if summary.effects_added > 0 {
        row(
            "效果器",
            format!("+{}", summary.effects_added),
            theme::GREEN,
        );
    }
    if summary.plugins_added > 0 {
        row(
            "VST 插件",
            format!("+{}", summary.plugins_added),
            theme::ACCENT,
        );
    }
    if summary.cleared {
        row("清空工程", "已清空".into(), theme::RED);
    }
    if let Some(bpm) = summary.tempo {
        row("速度", format!("{bpm:.0} BPM"), theme::TEXT);
    }
    if let Some((numerator, denominator)) = summary.time_signature {
        row("拍号", format!("{numerator}/{denominator}"), theme::TEXT);
    }
}

/// 所属模块：ui / AI 编曲面板 / 空状态
/// 功能：绘制空对话引导与可点击的示例问题。
/// 输入：`ui` 绘制区域。
/// 输出/副作用：返回被点击的示例文本；不修改状态。
/// 复杂度：时间 O(示例数)，空间 O(1)。
///
/// Module: ui / AI composition panel / empty state
/// Purpose: Draw empty-conversation guidance and clickable examples.
/// Inputs: `ui` drawing area.
/// Outputs / side effects: the clicked example, if any; no state change.
/// Complexity: O(examples) time and O(1) space.
fn paint_ai_empty(ui: &mut egui::Ui) -> Option<&'static str> {
    let mut picked = None;
    ui.add_space(14.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new("让 AI 帮你写一段音乐")
                .size(14.0)
                .strong()
                .color(theme::TEXT),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "AI 会读取当前工程的速度、轨道与已有音符，把新内容直接写入工程；\n\
                 也会调用你已安装的 VST 乐器与效果器。不满意可按 Ctrl+Z 撤销。",
            )
            .size(10.0)
            .color(theme::TEXT_DIM),
        );
    });
    ui.add_space(12.0);
    for example in EMPTY_PROMPTS {
        if ui
            .add(
                egui::Button::new(
                    RichText::new(format!("· {example}"))
                        .size(10.0)
                        .color(theme::TEXT_DIM),
                )
                .fill(theme::PANEL_2)
                .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                .corner_radius(8)
                .wrap()
                .min_size(egui::vec2(ui.available_width(), 26.0)),
            )
            .on_hover_text("点击填入输入框")
            .clicked()
        {
            picked = Some(example);
        }
        ui.add_space(4.0);
    }
    picked
}
