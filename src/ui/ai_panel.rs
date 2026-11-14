//! 所属模块：ui / AI 编曲面板
//! 文件用途：
//! - 绘制右侧 AI 编曲工作台：左侧任务栏（新任务与项目内历史对话）与右侧对话区。
//! - 面板以带投影的浮动卡片呈现；输入区只保留输入框、附件、高级选项与发送，其余控件收进高级选项。
//! - 只发出用户意图（发送、停止、添加附件、打开设置、跳转主页），网络与工程写入由 `app::ai` 完成。
//!
//! 函数清单：
//! - `SurfingApp::ui_ai_panel` / `ui_ai_panel_main` / `ui_ai_task_rail`：浮动工作台、对话区与任务栏。
//! - `SurfingApp::ui_ai_composer`：输入卡片、附件、高级选项弹出层（技能/乐器/智能体/模型/推理）与发送/停止。
//! - `ai_task_card` / `paint_task_status`：任务卡片与状态图标。
//! - `paint_ai_message` / `paint_ai_live` / `paint_message_meta` / `paint_thinking_block` /
//!   `paint_change_card` / `paint_ai_empty`：气泡、流式生成、任务信息、折叠思考、工程改动卡片与空状态。
//! - Markdown 富文本渲染已抽到 `ui/markdown.rs`，对话与 Agent 资料库共用。
//!
//! Module: ui / AI composition panel
//! Purpose:
//! - Draw the right-hand AI workspace: a left task rail (new task and project conversation history)
//!   and the conversation area.
//! - The panel is a floating card with a shadow; the composer keeps only the input, attachments,
//!   advanced options and send, with everything else folded into the advanced popup.
//! - Only emits user intents; networking and project writes live in `app::ai`.
//!
//! Functions:
//! - `SurfingApp::ui_ai_panel` / `ui_ai_panel_main` / `ui_ai_task_rail`: floating workspace, conversation and rail.
//! - `SurfingApp::ui_ai_composer`: composer card, attachments, advanced popup and send/stop.
//! - `ai_task_card` / `paint_task_status`: task card and status icon.
//! - `paint_ai_message` / `paint_ai_live` / `paint_message_meta` / `paint_thinking_block` /
//!   `paint_change_card` / `paint_ai_empty`: bubbles, streaming state, metadata, collapsible
//!   reasoning, change card and empty state.
//! - Markdown rendering lives in `ui/markdown.rs` and is shared with the Agent library.

use crate::ai::{AiApplySummary, AiReasoning, AiRole, MODEL_PRESETS, find_preset};
use crate::app::{
    AiChatState, AiLiveMessage, AiMessageMeta, AiTaskConflict, Clipboard, SurfingApp,
};
use crate::ui::theme;
use eframe::egui::{self, RichText};

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
    /// 功能：绘制「任务栏 + 对话区」两栏布局；窄窗口时自动退化为单栏对话。
    /// 输入：`ui` 右侧面板绘制区域。
    /// 输出/副作用：读写任务选择与 `ai_chat` 状态；不直接修改工程。
    /// 复杂度：每帧 O(任务数 + 消息数)。
    ///
    /// Module: ui / AI composition panel
    /// Purpose: Draw the two-column "task rail + conversation" layout and fall back to a single column
    /// on narrow windows.
    /// Inputs: `ui`: right-panel drawing area.
    /// Outputs / side effects: reads/writes task selection and `ai_chat` state; never edits
    /// the project directly.
    /// Complexity: O(tasks + messages) per frame.
    pub fn ui_ai_panel(&mut self, ui: &mut egui::Ui) {
        self.ai_chat.ensure_active_conversation();
        // 侧栏外留出阴影空间，内部再画一张浮动卡片：AI 面板因此从编辑区里“浮”起来。
        // Leave room for the shadow, then draw a floating card inside so the AI workspace lifts off
        // the editor instead of blending into it.
        let outer = ui.available_size();
        theme::floating_frame().show(ui, |ui| {
            ui.set_width((outer.x - theme::CARD_PAD as f32 * 2.0).max(240.0));
            ui.set_height((outer.y - theme::CARD_PAD as f32 * 2.0).max(240.0));
            if ui.available_width() < 520.0 {
                self.ui_ai_panel_main(ui);
                return;
            }
            let height = ui.available_height();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(178.0, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.ui_ai_task_rail(ui),
                );
                ui.add_space(theme::SPACE_SM);
                ui.separator();
                ui.add_space(theme::SPACE_SM);
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.ui_ai_panel_main(ui),
                );
            });
        });
    }

    /// 所属模块：ui / AI 编曲面板 / 任务栏
    /// 功能：列出当前项目窗口内的历史对话；当前查看的对话高亮，没有任务时高亮「新任务」。
    /// 输入：`ui` 左侧任务栏区域。
    /// 输出/副作用：切换对话、清空为新任务；只操作本窗口的 `ai_chat`，不修改工程。
    /// 复杂度：每帧 O(历史对话数)。
    ///
    /// Module: ui / AI composition panel / task rail
    /// Purpose: List historical conversations of this project window; highlight the active one or the
    /// new-task entry when none is active.
    /// Inputs: `ui`: left task-rail area.
    /// Outputs / side effects: switches conversations or starts a new task; touches only this window's
    /// `ai_chat`, never the project.
    /// Complexity: O(conversations) per frame.
    fn ui_ai_task_rail(&mut self, ui: &mut egui::Ui) {
        let active = self.ai_chat.active_conversation();
        let current_count = self.ai_chat.messages.len();
        let current_title = crate::app::AiChatState::conversation_title(&self.ai_chat.messages);
        let pending = self.ai_chat.pending.is_some();
        let tasks: Vec<(u64, String, usize, bool)> = self
            .ai_chat
            .conversations
            .iter()
            .rev()
            .map(|conversation| {
                (
                    conversation.id,
                    conversation.title.clone(),
                    conversation.messages.len(),
                    conversation.pending.is_some(),
                )
            })
            .collect();
        let mut new_task = false;
        let mut open_id = None;

        egui::Frame::NONE
            .inner_margin(egui::Margin::ZERO)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(ui.available_height());
                ui.horizontal(|ui| {
                    let (logo_rect, _) =
                        ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                    crate::app::paint_studio_logo(ui.painter(), logo_rect);
                    ui.label(
                        RichText::new("SurfingStudio")
                            .size(12.5)
                            .strong()
                            .color(theme::TEXT),
                    );
                });
                ui.add_space(6.0);
                let new_selected = active.is_none();
                let ring = if new_selected {
                    egui::Stroke::new(1.5_f32, egui::Color32::WHITE)
                } else {
                    egui::Stroke::NONE
                };
                egui::Frame::NONE
                    .stroke(ring)
                    .corner_radius(9)
                    .inner_margin(2.0)
                    .show(ui, |ui| {
                        let fill = if new_selected {
                            theme::PANEL_2
                        } else {
                            theme::ACCENT_DARK
                        };
                        let stroke = if new_selected {
                            egui::Stroke::new(1.0_f32, theme::LINE)
                        } else {
                            egui::Stroke::new(1.0_f32, theme::ACCENT)
                        };
                        if ui
                            .add_sized(
                                [ui.available_width(), 28.0],
                                egui::Button::new(RichText::new("+  新任务").size(11.0))
                                    .fill(fill)
                                    .stroke(stroke),
                            )
                            .clicked()
                        {
                            new_task = true;
                        }
                    });
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("任务  {}", tasks.len()))
                        .size(9.0)
                        .color(theme::TEXT_FAINT),
                );
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt("ai-task-rail")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (id, title, count, task_running) in &tasks {
                            ui.add_space(4.0);
                            let (title, count) = if active == Some(*id) {
                                (current_title.clone(), current_count)
                            } else {
                                (title.clone(), *count)
                            };
                            if ai_task_card(
                                ui,
                                &format!("ai-task-{id}"),
                                active == Some(*id),
                                if *task_running || (pending && active == Some(*id)) {
                                    AiTaskStatus::Running
                                } else {
                                    AiTaskStatus::Done
                                },
                                &title,
                                &format!("{count} 条消息"),
                            )
                            .clicked()
                            {
                                open_id = Some(*id);
                            }
                        }
                        if tasks.is_empty() && current_count == 0 {
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("暂无历史对话；当前项目内的对话会显示在这里。")
                                    .size(9.0)
                                    .color(theme::TEXT_FAINT),
                            );
                        }
                    });
            });

        if new_task {
            self.ai_chat.clear();
        }
        if let Some(id) = open_id {
            self.ai_chat.open_conversation(id);
        }
    }

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
    fn ui_ai_panel_main(&mut self, ui: &mut egui::Ui) {
        let mut close = false;
        let mut open_settings = false;
        // 顶部：左侧标题与状态，右侧关闭/设置；插件数量降为状态副标题，不再单独占一行。
        // Top bar: title and status on the left, close/settings on the right; the plugin count becomes
        // part of the status line instead of its own row.
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("AI 编曲")
                        .size(theme::FONT_SECTION)
                        .strong()
                        .color(theme::TEXT),
                );
                let status = if self.ai_chat.pending.is_some() {
                    "正在生成"
                } else {
                    "就绪"
                };
                let status_text = if crate::platform::caps().plugin_hosting {
                    let count = self.plugin_cache.len();
                    if count > 0 {
                        format!("{status} · 已连接 {count} 个 VST 插件")
                    } else {
                        format!("{status} · 使用内置音色")
                    }
                } else {
                    status.to_string()
                };
                ui.label(
                    RichText::new(status_text)
                        .size(theme::FONT_MICRO)
                        .color(theme::TEXT_FAINT),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("×").size(15.0).color(theme::TEXT_DIM))
                            .frame(false)
                            .min_size(egui::vec2(24.0, 24.0)),
                    )
                    .on_hover_text("关闭面板")
                    .clicked()
                {
                    close = true;
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new("⚙").size(12.0).color(theme::TEXT_DIM))
                            .frame(false)
                            .min_size(egui::vec2(24.0, 24.0)),
                    )
                    .on_hover_text("AI 设置（模型、API Key、联网检索、采样参数）")
                    .clicked()
                {
                    open_settings = true;
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
            ui.add_space(theme::SPACE_MD);
            // 低饱和琥珀底 + 图标 + 纯文字按钮：提示存在感足够，但不和蓝色主操作抢焦点。
            // Low-saturation amber surface, icon and text button: visible without competing with the
            // single blue primary action.
            egui::Frame::NONE
                .fill(theme::with_alpha(theme::AMBER, 24))
                .corner_radius(theme::RADIUS_CARD)
                .inner_margin(egui::Margin::symmetric(
                    theme::SPACE_MD as i8,
                    theme::SPACE_SM as i8,
                ))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (icon, _) =
                            ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                        ui.painter().circle_filled(
                            icon.center(),
                            9.0,
                            theme::with_alpha(theme::AMBER, 55),
                        );
                        ui.painter().text(
                            icon.center(),
                            egui::Align2::CENTER_CENTER,
                            "!",
                            egui::FontId::proportional(11.0),
                            theme::AMBER,
                        );
                        ui.add_space(theme::SPACE_SM);
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("尚未配置 API Key")
                                    .size(theme::FONT_LABEL)
                                    .strong()
                                    .color(theme::TEXT),
                            );
                            ui.label(
                                RichText::new("配置后即可发送 AI 编曲请求")
                                    .size(theme::FONT_MICRO)
                                    .color(theme::TEXT_DIM),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("配置")
                                            .size(theme::FONT_LABEL)
                                            .color(theme::AMBER),
                                    )
                                    .frame(false)
                                    .min_size(egui::vec2(0.0, 26.0)),
                                )
                                .clicked()
                            {
                                self.open_ai_settings(ui.ctx());
                            }
                        });
                    });
                });
        }
        if let Some(error) = self.ai_chat.error.clone() {
            ui.add_space(theme::SPACE_SM);
            egui::Frame::NONE
                .fill(theme::with_alpha(theme::RED, 24))
                .corner_radius(theme::RADIUS_CARD)
                .inner_margin(egui::Margin::symmetric(
                    theme::SPACE_MD as i8,
                    theme::SPACE_SM as i8,
                ))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(error)
                            .size(theme::FONT_LABEL)
                            .color(theme::RED),
                    );
                });
        }
        ui.add_space(theme::SPACE_SM);

        // 输入区先作为底部停靠面板登记，保证任何窗口高度下都不会被消息区挤掉。
        // Register the composer as a bottom dock first so the transcript can never push it off screen.
        egui::TopBottomPanel::bottom("ai_composer_dock")
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::symmetric(0, 6)),
            )
            .show_inside(ui, |ui| {
                ui.set_width(ui.available_width());
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
            });

        let transcript_h = ui.available_height().max(80.0);
        let mut empty_pick: Option<&'static str> = None;
        // 右侧整块为深色背景，消息与卡片直接悬浮其上。
        // The whole right side is one dark canvas; messages and cards float directly on it.
        // 只在流式生成时自动吸底；否则折叠/展开消息时会把视图拉到最底部，显得页面突然跳远。
        // Only stick to the bottom while streaming; otherwise expanding/collapsing a message would
        // yank the view to the very bottom.
        let streaming = self.ai_chat.live.is_some() || self.ai_chat.pending.is_some();
        egui::ScrollArea::vertical()
            .id_salt("ai_transcript")
            .max_height(transcript_h)
            .auto_shrink([false, false])
            .stick_to_bottom(streaming)
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
        if let Some(prompt) = empty_pick {
            self.ai_chat.input = prompt.to_string();
        }
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
        // 智能体列表来自可编辑资料库；先克隆 id 与名称，避免绘制期间借用 settings。
        // The agent list comes from the editable library; clone ids and names before painting.
        let agents: Vec<(String, String)> = self
            .settings
            .ai
            .library
            .agents
            .iter()
            .map(|agent| (agent.id.clone(), agent.name.clone()))
            .collect();
        let mut selected_agent = self.ai_chat.agent.clone();
        // 技能与乐器选题列表在绘制前克隆，避免在菜单闭包里借用 settings 与插件缓存。
        // Clone skill and instrument choices before painting so the menu closures do not borrow
        // settings or the plugin cache.
        let basic_choices: Vec<(String, String)> = self
            .settings
            .ai
            .library
            .basics
            .iter()
            .map(|skill| (skill.id.clone(), skill.title.clone()))
            .collect();
        let technique_choices: Vec<(String, String)> = self
            .settings
            .ai
            .library
            .techniques
            .iter()
            .map(|skill| (skill.id.clone(), skill.title.clone()))
            .collect();
        let mut instrument_choices: Vec<(String, String)> = vec![
            ("synth".into(), "内置合成器".into()),
            ("drums".into(), "内置鼓机".into()),
        ];
        for entry in crate::core::model::bank_instruments() {
            instrument_choices.push((entry.id.clone(), format!("{}（内置）", entry.name)));
        }
        if crate::platform::caps().plugin_hosting {
            for plugin in &self.plugin_cache {
                if !plugin.name.is_empty() {
                    instrument_choices
                        .push((plugin.name.clone(), format!("{}（VST）", plugin.name)));
                }
            }
        }
        let mut seen_instruments = std::collections::HashSet::new();
        instrument_choices.retain(|(value, _)| seen_instruments.insert(value.clone()));
        let default_skill_ids = self
            .settings
            .ai
            .library
            .agent_skill_ids(&self.ai_chat.agent);
        let default_technique_ids = self
            .settings
            .ai
            .library
            .agent_technique_ids(&self.ai_chat.agent);
        let displayed_skill_count = if self.ai_chat.skills_customized {
            self.ai_chat.selected_skills.len() + self.ai_chat.selected_techniques.len()
        } else {
            default_skill_ids.len() + default_technique_ids.len()
        };
        let usable_count = self.ai_chat.usable_instruments.len();
        let required_count = self.ai_chat.required_instruments.len();
        // 输入框内的 Ctrl+V 在剪贴板是整轨/片段/音符时转成 AI 引用，避免粘贴占位文本。
        // Ctrl+V inside the composer turns a track/clip/note clipboard into AI references instead of
        // pasting the placeholder text.
        let ai_input_id = egui::Id::new("ai_composer_input");
        let rich_clipboard = matches!(
            self.clipboard,
            Clipboard::Track(_) | Clipboard::Clips(_) | Clipboard::Notes(_)
        );
        if rich_clipboard && ui.memory(|memory| memory.has_focus(ai_input_id)) {
            let mut paste_rich = false;
            ui.input_mut(|input| {
                input.events.retain(|event| {
                    if matches!(event, egui::Event::Paste(_)) {
                        paste_rich = true;
                        false
                    } else {
                        true
                    }
                });
                if input.consume_key(egui::Modifiers::CTRL, egui::Key::V) {
                    paste_rich = true;
                }
            });
            if paste_rich {
                self.attach_clipboard_to_ai();
            }
        }
        // 输入卡片：只保留输入框、附件、高级选项与发送；技能/乐器/模型/智能体全部收进高级选项。
        // Composer card: only the input, attachments, advanced options and send stay visible; skills,
        // instruments, model and agent live in the advanced popup.
        let advanced_total = displayed_skill_count + usable_count + required_count;
        let advanced_label = if advanced_total > 0 {
            format!("高级选项 · {advanced_total}")
        } else {
            "高级选项".to_string()
        };
        egui::Frame::NONE
            .fill(theme::PANEL_2)
            .corner_radius(theme::RADIUS_SHEET)
            .inner_margin(egui::Margin::symmetric(
                theme::SPACE_MD as i8,
                theme::SPACE_MD as i8,
            ))
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
                                        RichText::new(format!("{}  ×", attachment.name))
                                            .size(theme::FONT_MICRO),
                                    )
                                    .fill(theme::PANEL_3)
                                    .stroke(egui::Stroke::NONE)
                                    .corner_radius(8)
                                    .min_size(egui::vec2(0.0, 22.0)),
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
                    ui.add_space(theme::SPACE_XS);
                }

                ui.add(
                    egui::TextEdit::multiline(&mut self.ai_chat.input)
                        .id(ai_input_id)
                        .frame(egui::Frame::NONE)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .hint_text("描述你想要的音乐；也可以说“用已安装的 Serum 做一个 Pad”…"),
                );

                ui.add_space(theme::SPACE_SM);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("+").size(16.0).color(theme::TEXT_DIM))
                                .fill(theme::PANEL_3)
                                .stroke(egui::Stroke::NONE)
                                .corner_radius(8)
                                .min_size(egui::vec2(28.0, 26.0)),
                        )
                        .on_hover_text("添加文本 / Markdown / JSON / MIDI 附件")
                        .clicked()
                    {
                        attach = true;
                    }
                    ui.add_space(theme::SPACE_XS);
                    let advanced = ui.add(
                        egui::Button::new(
                            RichText::new(advanced_label)
                                .size(theme::FONT_LABEL)
                                .color(theme::TEXT_DIM),
                        )
                        .fill(theme::PANEL_3)
                        .stroke(egui::Stroke::NONE)
                        .corner_radius(8)
                        .min_size(egui::vec2(0.0, 26.0)),
                    );
                    egui::Popup::menu(&advanced)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show(|ui| {
                            ui.set_min_width(360.0);
                            ui.label(
                                RichText::new("高级选项")
                                    .size(theme::FONT_BODY)
                                    .strong()
                                    .color(theme::TEXT),
                            );
                            ui.label(
                                RichText::new("技能未手动修改时跟随智能体默认")
                                    .size(theme::FONT_MICRO)
                                    .color(theme::TEXT_FAINT),
                            );

                            ui.add_space(theme::SPACE_MD);
                            ui.label(
                                RichText::new(format!("技能 · {displayed_skill_count}"))
                                    .size(theme::FONT_LABEL)
                                    .strong()
                                    .color(theme::TEXT),
                            );
                            ui.add_space(theme::SPACE_XS);
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.ai_chat.skill_filter)
                                        .hint_text("搜索技能")
                                        .desired_width(180.0)
                                        .min_size(egui::vec2(0.0, 28.0)),
                                );
                                let filter = self.ai_chat.skill_filter.trim().to_lowercase();
                                let all_selected = basic_choices
                                    .iter()
                                    .chain(technique_choices.iter())
                                    .filter(|(_, title)| {
                                        filter.is_empty() || title.to_lowercase().contains(&filter)
                                    })
                                    .all(|(id, _)| {
                                        effective_skill_checked(
                                            &self.ai_chat,
                                            &default_skill_ids,
                                            &default_technique_ids,
                                            id,
                                            true,
                                        )
                                    });
                                if theme::quiet_button(
                                    ui,
                                    if all_selected { "清空" } else { "全选" },
                                )
                                .clicked()
                                {
                                    ensure_skill_selection(
                                        &mut self.ai_chat,
                                        &default_skill_ids,
                                        &default_technique_ids,
                                    );
                                    if all_selected {
                                        self.ai_chat.selected_skills.clear();
                                        self.ai_chat.selected_techniques.clear();
                                    } else {
                                        for (id, title) in
                                            basic_choices.iter().chain(technique_choices.iter())
                                        {
                                            if filter.is_empty()
                                                || title.to_lowercase().contains(&filter)
                                            {
                                                let target = if basic_choices
                                                    .iter()
                                                    .any(|(basic, _)| basic == id)
                                                {
                                                    &mut self.ai_chat.selected_skills
                                                } else {
                                                    &mut self.ai_chat.selected_techniques
                                                };
                                                toggle_choice(target, id, true);
                                            }
                                        }
                                    }
                                }
                            });
                            egui::ScrollArea::vertical()
                                .id_salt("ai_advanced_skills")
                                .max_height(170.0)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    let filter = self.ai_chat.skill_filter.trim().to_lowercase();
                                    ui.label(
                                        RichText::new("基础")
                                            .size(theme::FONT_MICRO)
                                            .color(theme::TEXT_FAINT),
                                    );
                                    for (id, title) in &basic_choices {
                                        if !filter.is_empty()
                                            && !title.to_lowercase().contains(&filter)
                                        {
                                            continue;
                                        }
                                        let mut checked = effective_skill_checked(
                                            &self.ai_chat,
                                            &default_skill_ids,
                                            &default_technique_ids,
                                            id,
                                            false,
                                        );
                                        if choice_toggle(ui, title, &mut checked) {
                                            ensure_skill_selection(
                                                &mut self.ai_chat,
                                                &default_skill_ids,
                                                &default_technique_ids,
                                            );
                                            toggle_choice(
                                                &mut self.ai_chat.selected_skills,
                                                id,
                                                checked,
                                            );
                                        }
                                    }
                                    ui.add_space(theme::SPACE_XS);
                                    ui.label(
                                        RichText::new("技巧")
                                            .size(theme::FONT_MICRO)
                                            .color(theme::TEXT_FAINT),
                                    );
                                    for (id, title) in &technique_choices {
                                        if !filter.is_empty()
                                            && !title.to_lowercase().contains(&filter)
                                        {
                                            continue;
                                        }
                                        let mut checked = effective_skill_checked(
                                            &self.ai_chat,
                                            &default_skill_ids,
                                            &default_technique_ids,
                                            id,
                                            true,
                                        );
                                        if choice_toggle(ui, title, &mut checked) {
                                            ensure_skill_selection(
                                                &mut self.ai_chat,
                                                &default_skill_ids,
                                                &default_technique_ids,
                                            );
                                            toggle_choice(
                                                &mut self.ai_chat.selected_techniques,
                                                id,
                                                checked,
                                            );
                                        }
                                    }
                                });

                            for (target, count, label) in [
                                (true, usable_count, "可用乐器"),
                                (false, required_count, "必须乐器"),
                            ] {
                                ui.add_space(theme::SPACE_MD);
                                ui.label(
                                    RichText::new(format!("{label} · {count}"))
                                        .size(theme::FONT_LABEL)
                                        .strong()
                                        .color(theme::TEXT),
                                );
                                ui.label(
                                    RichText::new(if target {
                                        "限制 AI 只能从这些乐器中选择"
                                    } else {
                                        "要求 AI 至少各建一条轨道并使用"
                                    })
                                    .size(theme::FONT_MICRO)
                                    .color(theme::TEXT_FAINT),
                                );
                                ui.add_space(theme::SPACE_XS);
                                let filter_text = if target {
                                    &mut self.ai_chat.usable_filter
                                } else {
                                    &mut self.ai_chat.required_filter
                                };
                                ui.add(
                                    egui::TextEdit::singleline(filter_text)
                                        .hint_text("搜索乐器")
                                        .desired_width(f32::INFINITY)
                                        .min_size(egui::vec2(0.0, 28.0)),
                                );
                                let needle = filter_text.trim().to_lowercase();
                                let selected = if target {
                                    &mut self.ai_chat.usable_instruments
                                } else {
                                    &mut self.ai_chat.required_instruments
                                };
                                let visible: Vec<&(String, String)> = instrument_choices
                                    .iter()
                                    .filter(|(_, name)| {
                                        needle.is_empty() || name.to_lowercase().contains(&needle)
                                    })
                                    .collect();
                                let all_selected = !visible.is_empty()
                                    && visible.iter().all(|(value, _)| {
                                        selected.iter().any(|entry| entry == value)
                                    });
                                if theme::quiet_button(
                                    ui,
                                    if all_selected { "清空" } else { "全选" },
                                )
                                .clicked()
                                {
                                    if all_selected {
                                        selected.clear();
                                    } else {
                                        for (value, _) in &visible {
                                            toggle_choice(selected, value, true);
                                        }
                                    }
                                }
                                egui::ScrollArea::vertical()
                                    .id_salt(("ai_advanced_instruments", target))
                                    .max_height(150.0)
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        for (value, name) in visible {
                                            let mut checked = selected.contains(value);
                                            if choice_toggle(ui, name, &mut checked) {
                                                toggle_choice(selected, value, checked);
                                            }
                                        }
                                    });
                            }

                            ui.add_space(theme::SPACE_MD);
                            ui.label(
                                RichText::new("智能体与模型")
                                    .size(theme::FONT_LABEL)
                                    .strong()
                                    .color(theme::TEXT),
                            );
                            ui.add_space(theme::SPACE_XS);
                            ui.horizontal(|ui| {
                                let agent_label = agents
                                    .iter()
                                    .find(|(id, _)| id == &selected_agent)
                                    .map(|(_, name)| name.as_str())
                                    .unwrap_or("智能体");
                                egui::ComboBox::from_id_salt("ai_agent")
                                    .width(110.0)
                                    .selected_text(
                                        RichText::new(agent_label).size(theme::FONT_LABEL),
                                    )
                                    .show_ui(ui, |ui| {
                                        for (id, name) in &agents {
                                            ui.selectable_value(
                                                &mut selected_agent,
                                                id.clone(),
                                                name,
                                            );
                                        }
                                    });
                                if selected_agent != self.ai_chat.agent {
                                    self.ai_chat.select_agent(&selected_agent);
                                }

                                let mut preset_id = self.settings.ai.model_preset.clone();
                                let preset_label = find_preset(&preset_id)
                                    .map(|preset| preset.label)
                                    .unwrap_or("自定义");
                                egui::ComboBox::from_id_salt("ai_model_preset")
                                    .width(120.0)
                                    .selected_text(
                                        RichText::new(preset_label).size(theme::FONT_LABEL),
                                    )
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

                                let current_reasoning =
                                    AiReasoning::from_id(&self.settings.ai.reasoning);
                                let mut reasoning = current_reasoning;
                                egui::ComboBox::from_id_salt("ai_reasoning")
                                    .width(96.0)
                                    .selected_text(
                                        RichText::new(format!("推理 {}", reasoning.label()))
                                            .size(theme::FONT_LABEL),
                                    )
                                    .show_ui(ui, |ui| {
                                        for entry in AiReasoning::ALL {
                                            ui.selectable_value(
                                                &mut reasoning,
                                                entry,
                                                entry.label(),
                                            );
                                        }
                                    });
                                if reasoning != current_reasoning {
                                    self.settings.ai.reasoning = reasoning.id().to_string();
                                    reasoning_changed = true;
                                }
                            });
                        });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.ai_chat.pending.is_some() {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("■").size(13.0).color(egui::Color32::WHITE),
                                    )
                                    .fill(theme::RED)
                                    .corner_radius(17)
                                    .min_size(egui::vec2(34.0, 34.0)),
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
                                        RichText::new("↑").size(16.0).color(egui::Color32::WHITE),
                                    )
                                    .fill(fill)
                                    .corner_radius(17)
                                    .min_size(egui::vec2(34.0, 34.0)),
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
        if (send || send_shortcut)
            && (!self.ai_chat.input.trim().is_empty() || !self.ai_chat.attachments.is_empty())
        {
            if self.ai_chat.any_task_running() {
                self.ai_chat.conflict_prompt = Some(AiTaskConflict::Send);
            } else {
                self.send_ai_message();
            }
        }
    }
}

/// 已有 AI 任务运行时弹出的冲突确认窗口。
/// Conflict confirmation shown while another AI task is running.
impl SurfingApp {
    /// 所属模块：ui / AI 编曲面板 / 冲突确认
    /// 功能：提示当前项目已有后台任务，并提供「停止任务」或「后台继续」。
    /// 输入：`ctx` 渲染上下文。
    /// 输出/副作用：可能取消当前任务并执行待发送/待切换动作；不直接修改工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ui / AI composition panel / conflict
    /// Purpose: Warn that a background task is running and offer "stop task" or "keep running".
    /// Inputs: `ctx`.
    /// Outputs / side effects: may cancel the task and perform the queued send/switch action; never
    /// edits the project directly.
    /// Complexity: O(1) time and space.
    pub fn ui_ai_task_conflict(&mut self, ctx: &egui::Context) {
        let Some(conflict) = self.ai_chat.conflict_prompt else {
            return;
        };
        if self.ai_chat.pending.is_none() {
            self.ai_chat.conflict_prompt = None;
            return;
        }
        let mut stop = false;
        let mut keep = false;
        let response = egui::Modal::new(egui::Id::new("ai_task_conflict")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.label(
                RichText::new("已有任务正在进行")
                    .size(14.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new("当前项目中已有一个 AI 任务正在后台运行。")
                    .size(11.5)
                    .strong()
                    .color(theme::TEXT),
            );
            let detail = match conflict {
                AiTaskConflict::Send => "发送第二个任务前需要先停止当前任务。",
                AiTaskConflict::Edit => {
                    "在轨道窗编辑前建议先停止当前任务，避免 AI 写入与手动修改互相覆盖。"
                }
            };
            ui.add_space(4.0);
            ui.label(RichText::new(detail).size(10.0).color(theme::TEXT_DIM));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let stop_label = match conflict {
                    AiTaskConflict::Send => "停止并发送",
                    AiTaskConflict::Edit => "停止任务",
                };
                if ui
                    .add(
                        egui::Button::new(RichText::new(stop_label).size(10.5).color(theme::TEXT))
                            .fill(theme::RED)
                            .min_size(egui::vec2(0.0, 26.0)),
                    )
                    .clicked()
                {
                    stop = true;
                }
                if ui
                    .add(egui::Button::new(RichText::new("后台继续运行").size(10.5)))
                    .clicked()
                {
                    keep = true;
                }
            });
        });
        if response.should_close() && !stop && !keep {
            keep = true;
        }
        if stop {
            self.ai_chat.conflict_prompt = None;
            self.cancel_all_ai_tasks();
            if conflict == AiTaskConflict::Send {
                self.send_ai_message();
            }
        } else if keep {
            self.ai_chat.conflict_prompt = None;
            self.ai_chat.edit_guard_dismissed = true;
        }
    }
}

/// 任务状态图标：完成、运行中。
/// Task status icon: done, running.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AiTaskStatus {
    Done,
    Running,
}

/// 所属模块：ui / AI 编曲面板 / 任务栏
/// 功能：绘制一张可点击的任务卡片（状态图标 + 标题 + 副标题 + 选中高亮）。
/// 输入：`ui`、`id` 稳定标识、`selected` 是否选中、`status` 状态、`title`/`subtitle` 文本。
/// 输出/副作用：返回点击响应；只绘制，不修改状态。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: ui / AI composition panel / task rail
/// Purpose: Draw a clickable task card (status icon + title + subtitle + selection highlight).
/// Inputs: `ui`, stable `id`, `selected`, `status`, `title`/`subtitle`.
/// Outputs / side effects: the click response; painting only.
/// Complexity: O(text length) time and O(1) space.
fn ai_task_card(
    ui: &mut egui::Ui,
    id: &str,
    selected: bool,
    status: AiTaskStatus,
    title: &str,
    subtitle: &str,
) -> egui::Response {
    let fill = if selected { theme::PANEL_2 } else { theme::BG };
    let stroke = egui::Stroke::new(1.0_f32, theme::LINE);
    let inner = egui::Frame::NONE
        .fill(fill)
        .stroke(stroke)
        .corner_radius(9)
        .inner_margin(egui::Margin::symmetric(8, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                paint_task_status(ui, status);
                ui.add_space(2.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(10.0).strong().color(if selected {
                        theme::TEXT
                    } else {
                        theme::TEXT_DIM
                    }));
                    ui.label(RichText::new(subtitle).size(8.5).color(theme::TEXT_FAINT));
                });
            });
        });
    if selected {
        // 选中态用外圈白色描边，不改变卡片底色。
        // Selection uses a white outer ring instead of a coloured background.
        ui.painter().rect_stroke(
            inner.response.rect.expand(2.0),
            egui::CornerRadius::from(11),
            egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
            egui::StrokeKind::Outside,
        );
    }
    let response = ui.interact(
        inner.response.rect,
        ui.make_persistent_id(id),
        egui::Sense::click(),
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// 所属模块：ui / AI 编曲面板 / 任务栏
/// 功能：绘制圆形状态图标；完成态为绿底白勾，运行态为蓝色圆环加点。
/// 输入：`ui`、`status` 状态。
/// 输出/副作用：占用 16×16 区域并绘制；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / AI composition panel / task rail
/// Purpose: Draw the circular status icon: a green disc with a white check when done, a blue ring and
/// dot while running.
/// Inputs: `ui`, `status`.
/// Outputs / side effects: allocates 16×16 and paints; no state change.
/// Complexity: O(1) time and space.
fn paint_task_status(ui: &mut egui::Ui, status: AiTaskStatus) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
    let center = rect.center();
    let painter = ui.painter();
    match status {
        AiTaskStatus::Done => {
            painter.circle_filled(center, 7.5, theme::GREEN);
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                "✓",
                egui::FontId::proportional(10.0),
                theme::TEXT,
            );
        }
        AiTaskStatus::Running => {
            painter.circle_stroke(center, 7.0, egui::Stroke::new(1.5_f32, theme::ACCENT));
            painter.circle_filled(center, 2.5, theme::ACCENT);
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
    if is_user {
        let bubble = |ui: &mut egui::Ui| {
            egui::Frame::NONE
                .fill(theme::PANEL_2)
                .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                .corner_radius(8)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.set_max_width((ui.available_width() * 0.94).max(160.0));
                    ui.label(RichText::new(content).size(11.5).color(theme::TEXT));
                });
        };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), bubble);
    } else {
        paint_assistant_header(ui, index, meta, reasoning);
        ui.separator();
        crate::ui::markdown::render(ui, content, theme::FONT_MICRO);
    }
    if let Some(summary) = changes {
        ui.add_space(6.0);
        paint_change_card(ui, summary, index);
    }
}

/// 所属模块：ui / AI 编曲面板 / 助手输出
/// 功能：绘制模型图标、模型名与可折叠「任务耗时」；展开后显示思考与写入过程。
/// 输入：`ui`、消息序号、任务元信息与思考文本。
/// 输出/副作用：只绘制与维护折叠状态；不修改工程。
/// 复杂度：时间 O(过程步骤数 + 思考文本长度)，空间 O(1)。
///
/// Module: ui / AI composition panel / assistant output
/// Purpose: Draw the model icon/name and a collapsible "task time"; expanding shows reasoning and writes.
/// Inputs: `ui`, message index, metadata and reasoning.
/// Outputs / side effects: paints and maintains collapse state; no project change.
/// Complexity: O(steps + reasoning length) time and O(1) space.
fn paint_assistant_header(
    ui: &mut egui::Ui,
    index: usize,
    meta: Option<&AiMessageMeta>,
    reasoning: Option<&str>,
) {
    let state_id = ui.make_persistent_id(("ai-process", index));
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), state_id, false);
    ui.horizontal(|ui| {
        let (icon_rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(icon_rect, egui::CornerRadius::from(4), theme::GREEN);
        painter.text(
            icon_rect.center(),
            egui::Align2::CENTER_CENTER,
            "AI",
            egui::FontId::proportional(8.0),
            theme::BG,
        );
        ui.add_space(4.0);
        let model = meta
            .and_then(|meta| (!meta.model.is_empty()).then_some(meta.model.as_str()))
            .unwrap_or("SurfingStudio AI");
        ui.label(RichText::new(model).size(10.5).strong().color(theme::TEXT));
        let elapsed = meta.map(|meta| meta.elapsed_seconds).unwrap_or(0.0);
        if elapsed > 0.0 {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let arrow = if state.is_open() { "▴" } else { "▾" };
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!("任务耗时 {} {arrow}", format_elapsed(elapsed)))
                                .size(9.0)
                                .color(theme::TEXT_FAINT),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    state.toggle(ui);
                }
            });
        }
    });
    // 折叠状态必须写回 egui 内存，否则收起时不会保存，下一帧又按“展开”加载。
    // Persist the collapse state; without this the collapsed state is never stored and the next
    // frame reloads it as open.
    let open = state.is_open();
    state.store(ui.ctx());
    if open {
        state.show_body_unindented(ui, |ui| {
            ui.add_space(2.0);
            if let Some(reasoning) = reasoning.filter(|text| !text.trim().is_empty()) {
                ui.label(
                    RichText::new("思考")
                        .size(9.0)
                        .strong()
                        .color(theme::TEXT_DIM),
                );
                ui.label(
                    RichText::new(reasoning.trim())
                        .size(9.5)
                        .color(theme::TEXT_FAINT),
                );
                ui.add_space(4.0);
            }
            if let Some(meta) = meta {
                if !meta.steps.is_empty() {
                    ui.label(
                        RichText::new("修改")
                            .size(9.0)
                            .strong()
                            .color(theme::TEXT_DIM),
                    );
                    for step in &meta.steps {
                        ui.label(
                            RichText::new(format!("· {step}"))
                                .size(9.5)
                                .color(theme::GREEN),
                        );
                    }
                    ui.add_space(4.0);
                }
                paint_message_meta(ui, meta);
            }
        });
    }
}

/// 所属模块：ui / AI 编曲面板 / 任务耗时
/// 功能：把秒数格式化为 `6m 31s` 或 `12s`。
/// 输入：`seconds` 秒数。
/// 输出/副作用：格式化文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / AI composition panel / task time
/// Purpose: Format seconds as `6m 31s` or `12s`.
/// Inputs: `seconds`.
/// Outputs / side effects: formatted text; no state change.
/// Complexity: O(1) time and space.
fn format_elapsed(seconds: f32) -> String {
    let total = seconds.max(0.0).round() as u64;
    if total >= 60 {
        format!("{}m {:02}s", total / 60, total % 60)
    } else {
        format!("{total}s")
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
    // 流式输出同样直接绘制在深色背景上，不再套气泡。
    // Streaming output is drawn directly on the dark canvas without a bubble.
    ui.label(RichText::new(text).size(11.5).color(theme::TEXT));
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
/// 输入：`ui` 绘制区域；`summary` 结构化改动；`index` 消息序号（网格 id）。
/// 输出/副作用：只绘制；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / AI composition panel / change card
/// Purpose: Draw the "project updated" card: tracks, clips, notes, effects, VST plugins and clearing.
/// Inputs: `ui`; `summary`; `index` for a unique grid id.
/// Outputs / side effects: paints only; no state change.
/// Complexity: O(1) time and space.
fn paint_change_card(ui: &mut egui::Ui, summary: AiApplySummary, index: usize) {
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
            egui::Grid::new(("ai_change_grid", index))
                .num_columns(2)
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
    if summary.params_changed > 0 {
        row(
            "调整参数",
            format!("{}", summary.params_changed),
            theme::AMBER,
        );
    }
    if summary.controllers_added > 0 {
        row(
            "控制器点",
            format!("+{}", summary.controllers_added),
            theme::GREEN,
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
    ui.add_space(8.0);
    for (index, example) in EMPTY_PROMPTS.iter().enumerate() {
        let inner = egui::Frame::NONE
            .fill(theme::PANEL_2)
            .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("· {example}"))
                            .size(9.8)
                            .color(theme::TEXT_DIM),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new("↗").size(10.0).color(theme::TEXT_FAINT));
                    });
                });
            });
        let response = ui.interact(
            inner.response.rect,
            ui.make_persistent_id(("ai-empty-prompt", index)),
            egui::Sense::click(),
        );
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            picked = Some(*example);
        }
        ui.add_space(6.0);
    }
    picked
}

/// 所属模块：ui / AI 编曲面板 / 输入区
/// 功能：按勾选状态加入或移除一个选项 id。
/// 输入：`items` 选项列表；`id` 目标；`checked` 是否选中。
/// 输出/副作用：原地修改列表；不修改其他状态。
/// 复杂度：时间 O(列表长度)，空间 O(1)。
///
/// Module: ui / AI composition panel / composer
/// Purpose: Add or remove a choice id according to a checkbox.
/// Inputs: `items`; `id`; `checked`.
/// Outputs / side effects: mutates the list in place.
/// Complexity: O(list length) time and O(1) space.
fn toggle_choice(items: &mut Vec<String>, id: &str, checked: bool) {
    if checked {
        if !items.iter().any(|entry| entry == id) {
            items.push(id.to_string());
        }
    } else {
        items.retain(|entry| entry != id);
    }
}

/// 所属模块：ui / AI 编曲面板 / 输入区
/// 功能：绘制紧凑的多选菜单入口按钮。
/// 输入：`ui` 当前布局；`label` 文本。
/// 输出/副作用：返回按钮响应；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / AI composition panel / composer
/// Purpose: Draw a compact multi-select menu trigger.
/// Inputs: `ui`; `label`.
/// Outputs / side effects: the button response; no state change.
/// Complexity: O(1) time and space.
fn compact_menu_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).size(9.0))
            .fill(theme::PANEL_3)
            .corner_radius(6)
            .min_size(egui::vec2(0.0, 20.0)),
    )
}

/// 所属模块：ui / AI 编曲面板 / 输入区
/// 功能：绘制一行“名称 + 右侧开关”，点击名称或开关都可切换。
/// 输入：`ui`；`label` 名称；`value` 当前状态。
/// 输出/副作用：返回是否发生切换；切换时写入 `value`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / AI composition panel / composer
/// Purpose: Draw a row with a label and a right-aligned switch; clicking either toggles it.
/// Inputs: `ui`; `label`; `value`.
/// Outputs / side effects: whether the value changed; mutates `value` on toggle.
/// Complexity: O(1) time and space.
fn choice_toggle(ui: &mut egui::Ui, label: &str, value: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let label_width = (ui.available_width() - 40.0).max(40.0);
        let response = ui.add_sized(
            egui::vec2(label_width, 20.0),
            egui::Label::new(RichText::new(label).size(10.0)).sense(egui::Sense::click()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if theme::toggle_switch(ui, value).clicked() {
                changed = true;
            }
        });
        if response.clicked() {
            *value = !*value;
            changed = true;
        }
    });
    changed
}

/// 所属模块：ui / AI 编曲面板 / 输入区
/// 功能：返回某个技能在当前显示状态下是否勾选（未自定义时用智能体默认）。
/// 输入：`chat` 对话状态；`default_skills` / `default_techniques` 智能体默认；
/// `id` 技能 id；`technique` 是否为技巧。
/// 输出/副作用：布尔值；不修改状态。
/// 复杂度：时间 O(默认技能数 + 已选数)，空间 O(1)。
///
/// Module: ui / AI composition panel / composer
/// Purpose: Whether a skill is checked in the current display state (agent defaults until customised).
/// Inputs: `chat`; `default_skills` / `default_techniques`; `id`; `technique`.
/// Outputs / side effects: a boolean; no state change.
/// Complexity: O(default skills + selected) time and O(1) space.
fn effective_skill_checked(
    chat: &AiChatState,
    default_skills: &[String],
    default_techniques: &[String],
    id: &str,
    technique: bool,
) -> bool {
    if chat.skills_customized {
        if technique {
            chat.selected_techniques.iter().any(|entry| entry == id)
        } else {
            chat.selected_skills.iter().any(|entry| entry == id)
        }
    } else if technique {
        default_techniques.iter().any(|entry| entry == id)
    } else {
        default_skills.iter().any(|entry| entry == id)
    }
}

/// 所属模块：ui / AI 编曲面板 / 输入区
/// 功能：首次手动修改技能前，把智能体默认技能复制为显式选择。
/// 输入：`chat`；`default_skills` / `default_techniques`。
/// 输出/副作用：可能写入两个选择列表并标记已自定义；不修改工程。
/// 复杂度：时间 O(默认技能数)，空间 O(默认技能数)。
///
/// Module: ui / AI composition panel / composer
/// Purpose: Materialise agent defaults into explicit selections before the first manual edit.
/// Inputs: `chat`; `default_skills` / `default_techniques`.
/// Outputs / side effects: may fill both lists and mark them customised; no project change.
/// Complexity: O(default skills) time and space.
fn ensure_skill_selection(
    chat: &mut AiChatState,
    default_skills: &[String],
    default_techniques: &[String],
) {
    if chat.skills_customized {
        return;
    }
    chat.selected_skills = default_skills.to_vec();
    chat.selected_techniques = default_techniques.to_vec();
    chat.skills_customized = true;
}
