//! 所属模块：ui / AI 编曲面板
//! 文件用途：
//! - 在工作区上方绘制 AI 编曲浮层：半透明遮罩 + 右侧 700 点工作台（240 点任务栏 + 对话区）。
//! - 输入卡片沿用聊天应用结构：上方快捷指令胶囊，下方工具栏含附件、智能体、模型、推理、技能乐器与发送。
//! - 所有下拉入口都绘制可见箭头：选择菜单固定向下，折叠入口展开向下、收起按左右位置取向右/左，
//!   展开高度与箭头旋转使用 0.18 秒补间，不依赖字体中的 ▾ 字形。
//! - 只发出用户意图（发送、停止、添加附件、打开设置、跳转主页），网络与工程写入由 `app::ai` 完成。
//!
//! 函数清单：
//! - `SurfingApp::ui_ai_overlay`：遮罩、浮动工作台与关闭逻辑。
//! - `SurfingApp::ui_ai_sidebar` / `ui_ai_panel_main`：左侧任务栏与右侧对话区。
//! - `SurfingApp::ui_ai_composer`：快捷指令、输入卡片、下拉选择器、技能/乐器弹出层与发送/停止。
//! - `ai_task_card` / `relative_time`：任务行与相对时间。
//! - `paint_ai_message` / `paint_ai_live` / `paint_message_meta` / `paint_thinking_block` /
//!   `paint_change_card` / `paint_ai_empty`：气泡、流式生成、任务信息、折叠思考、工程改动卡片与空状态。
//! - Markdown 富文本渲染已抽到 `ui/markdown.rs`，对话与 Agent 资料库共用。
//!
//! Module: ui / AI composition panel
//! Purpose:
//! - Draw the AI overlay above the workspace: a translucent scrim plus a 700 pt panel on the right
//!   (240 pt task rail + conversation).
//! - The composer follows a chat-app layout: suggestion pills above, then a toolbar with attachment,
//!   agent, model, reasoning, skills/instruments and send.
//! - Every dropdown paints a visible chevron: select menus always point down; disclosures point down
//!   when open and right/left by edge position when closed, with 0.18 s tweens for both height and
//!   rotation. Never depends on the ▾ glyph.
//! - Only emits user intents; networking and project writes live in `app::ai`.
//!
//! Functions:
//! - `SurfingApp::ui_ai_overlay`: scrim, floating workspace and close handling.
//! - `SurfingApp::ui_ai_sidebar` / `ui_ai_panel_main`: task rail and conversation.
//! - `SurfingApp::ui_ai_composer`: suggestions, composer card, dropdowns, skills/instruments popup and send/stop.
//! - `ai_task_card` / `relative_time`: task rows and relative time.
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

/// 空对话时的快捷指令：左侧为胶囊短标签，右侧为点击后填入的完整提示词。
/// Empty-chat suggestions: short pill label on the left, full prompt filled on click on the right.
const EMPTY_SUGGESTIONS: [(&str, &str); 4] = [
    (
        "钢琴与弦乐轻音乐",
        "写一段 90 秒的钢琴与弦乐轻音乐，C 大调，84 BPM，温柔一点。",
    ),
    (
        "长笛 + 竖琴琶音",
        "在现有工程上加入长笛主旋律和竖琴琶音，8 小节。",
    ),
    (
        "改成 3/4 拍圆舞曲",
        "把这段旋律改成 3/4 拍的圆舞曲，配弦乐拨奏。",
    ),
    (
        "中国风：古筝二胡",
        "来一段中国风轻音乐：古筝、二胡、钢琴，五声音阶。",
    ),
];
/// 有对话后展示的快捷指令。
/// Quick prompts shown once the conversation has started.
const QUICK_PROMPTS: [(&str, &str); 3] = [
    ("继续发展这段旋律", "继续发展这段旋律"),
    ("加一段弦乐铺底", "加一段弦乐铺底"),
    ("换成 3/4 拍", "换成 3/4 拍"),
];
/// AI 浮层默认宽度（逻辑点）。
/// Default AI overlay width in logical points.
const AI_PANEL_W: f32 = 700.0;
/// 左侧任务栏宽度（逻辑点）。
/// Width of the left task rail in logical points.
const AI_SIDEBAR_W: f32 = 240.0;
/// 单条任务行高度（逻辑点）。
/// Height of one task row in logical points.
const AI_TASK_H: f32 = 52.0;

impl SurfingApp {
    /// 所属模块：ui / AI 编曲面板 / 浮层
    /// 功能：在工作区上绘制 AI 浮层：半透明遮罩 + 右侧 700 点工作台（左侧任务栏 / 右侧对话）。
    /// 输入：`ctx`：egui 上下文；`rect`：工作区（应用栏、工具栏与状态栏之外）。
    /// 输出/副作用：点击遮罩或按 Esc 收起面板；绘制与交互只读写本窗口 UI 状态。
    /// 复杂度：每帧 O(任务数 + 消息数)。
    ///
    /// Module: ui / AI composition panel / overlay
    /// Purpose: Draw the AI workspace above the editor: a translucent scrim plus a 700 pt panel on
    /// the right (task rail on the left, conversation on the right).
    /// Inputs: `ctx`; `rect` (workspace below the app bar/toolbar/status bar).
    /// Outputs / side effects: the scrim or Esc closes the panel; only this window's UI state changes.
    /// Complexity: O(tasks + messages) per frame.
    pub fn ui_ai_overlay(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        self.ai_chat.ensure_active_conversation();
        let mut close = ctx.input(|input| input.key_pressed(egui::Key::Escape))
            && !egui::Popup::is_any_open(ctx);
        let panel_w = AI_PANEL_W.min((rect.width() - 48.0).max(480.0));
        let panel_pos = egui::pos2(rect.right() - panel_w, rect.top());

        // 遮罩：只盖住工作区，保留应用栏与工具栏，窗口按钮仍可用。
        // Scrim: covers the workspace only, so the app bar and toolbar stay usable.
        egui::Area::new(egui::Id::new("ai_workspace_scrim"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                let (scrim, response) = ui.allocate_exact_size(rect.size(), egui::Sense::click());
                ui.painter()
                    .rect_filled(scrim, 0.0, egui::Color32::from_black_alpha(145));
                if response.clicked() {
                    close = true;
                }
            });

        egui::Area::new(egui::Id::new("ai_workspace_panel"))
            .order(egui::Order::Foreground)
            .fixed_pos(panel_pos)
            .show(ctx, |ui| {
                ui.set_width(panel_w);
                ui.set_height(rect.height());
                egui::Frame::NONE
                    .fill(theme::BG)
                    .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                    .shadow(egui::epaint::Shadow {
                        offset: [-12, 0],
                        blur: 32,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(160),
                    })
                    .show(ui, |ui| {
                        ui.set_width(panel_w - 1.0);
                        ui.set_height(rect.height() - 1.0);
                        ui.horizontal_top(|ui| {
                            let height = ui.available_height();
                            ui.allocate_ui_with_layout(
                                egui::vec2(AI_SIDEBAR_W, height),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| self.ui_ai_sidebar(ui),
                            );
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), height),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| self.ui_ai_panel_main(ui),
                            );
                        });
                    });
            });

        if close {
            self.close_side_panel();
        }
    }

    /// 所属模块：ui / AI 编曲面板 / 任务栏
    /// 功能：绘制左侧任务栏：图标 + 标题 + 设置、整宽「新任务」按钮、按今天/更早分组的历史对话、
    /// 底部 VST 连接状态。
    /// 输入：`ui`：左侧任务栏区域。
    /// 输出/副作用：切换对话、开始新任务或打开 AI 设置；不修改工程。
    /// 复杂度：每帧 O(历史对话数)。
    ///
    /// Module: ui / AI composition panel / task rail
    /// Purpose: Draw the left rail: logo + title + settings, a full-width new-task button, history
    /// grouped into today/earlier, and the VST connection status at the bottom.
    /// Inputs: `ui`: task-rail area.
    /// Outputs / side effects: switches conversations, starts a new task or opens AI settings; never
    /// edits the project.
    /// Complexity: O(conversations) per frame.
    fn ui_ai_sidebar(&mut self, ui: &mut egui::Ui) {
        // 强制占满 240 点：只写子控件宽度会让父布局按内容收缩，导致侧栏与主区比例失真。
        // Reserve the full 240 pt: sizing only the children lets the parent shrink the rail and
        // distorts the sidebar/main ratio.
        ui.set_min_width(AI_SIDEBAR_W);
        ui.set_min_height(ui.available_height());
        let active = self.ai_chat.active_conversation();
        let current_count = self.ai_chat.messages.len();
        let current_title = crate::app::AiChatState::conversation_title(&self.ai_chat.messages);
        let pending = self.ai_chat.pending.is_some();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or(0.0);
        let tasks: Vec<(u64, String, usize, bool, f64)> = self
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
                    conversation.updated_at,
                )
            })
            .collect();
        let plugin_note = if crate::platform::caps().plugin_hosting {
            let count = self.plugin_cache.len();
            if count > 0 {
                format!("已连接 {count} 个 VST 插件")
            } else {
                "使用内置音色".to_string()
            }
        } else {
            "使用内置音色".to_string()
        };
        let mut new_task = false;
        let mut open_id = None;
        let mut open_settings = false;

        egui::Frame::NONE.fill(theme::BG_DEEP).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(ui.available_height());
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                let (logo, _) =
                    ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                crate::app::paint_studio_logo(ui.painter(), logo);
                ui.add_space(8.0);
                ui.label(
                    RichText::new("AI 编曲")
                        .size(14.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("⚙").size(12.0).color(theme::TEXT_DIM))
                                .frame(false)
                                .min_size(egui::vec2(24.0, 24.0)),
                        )
                        .on_hover_text("AI 设置")
                        .clicked()
                    {
                        open_settings = true;
                    }
                    ui.add_space(8.0);
                });
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                let width = (ui.available_width() - 14.0).max(80.0);
                if ui
                    .add_sized(
                        [width, 38.0],
                        egui::Button::new(
                            RichText::new("+  新任务")
                                .size(13.0)
                                .color(egui::Color32::WHITE),
                        )
                        .fill(theme::ACCENT)
                        .stroke(egui::Stroke::NONE)
                        .corner_radius(8),
                    )
                    .clicked()
                {
                    new_task = true;
                }
            });
            ui.add_space(10.0);
            egui::ScrollArea::vertical()
                .id_salt("ai-task-rail")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(2.0);
                    let mut section = |ui: &mut egui::Ui, label: &str| {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            ui.label(
                                RichText::new(label)
                                    .size(theme::FONT_MICRO)
                                    .color(theme::TEXT_FAINT),
                            );
                        });
                        ui.add_space(2.0);
                    };
                    let today: Vec<_> = tasks
                        .iter()
                        .filter(|task| now - task.4 < 86_400.0)
                        .collect();
                    let earlier: Vec<_> = tasks
                        .iter()
                        .filter(|task| now - task.4 >= 86_400.0)
                        .collect();
                    for (label, group) in [("今天", &today), ("更早", &earlier)] {
                        if group.is_empty() {
                            continue;
                        }
                        section(ui, label);
                        for task in group.iter().copied() {
                            let (id, title, count, task_running, updated) = task;
                            let running = *task_running || (pending && active == Some(*id));
                            let (title, count) = if active == Some(*id) {
                                (current_title.clone(), current_count)
                            } else {
                                (title.clone(), *count)
                            };
                            let subtitle = if running {
                                format!("生成中 · {}", relative_time(now - *updated))
                            } else {
                                format!("已完成 · {}", relative_time(now - *updated))
                            };
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(ui.available_width() - 8.0, AI_TASK_H),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        if ai_task_card(
                                            ui,
                                            &format!("ai-task-{id}"),
                                            active == Some(*id),
                                            if running {
                                                AiTaskStatus::Running
                                            } else {
                                                AiTaskStatus::Done
                                            },
                                            &title,
                                            &subtitle,
                                        )
                                        .clicked()
                                        {
                                            open_id = Some(*id);
                                        }
                                    },
                                );
                            });
                            ui.add_space(2.0);
                        }
                    }
                    if tasks.is_empty() && current_count == 0 {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            ui.label(
                                RichText::new("暂无历史对话")
                                    .size(theme::FONT_MICRO)
                                    .color(theme::TEXT_FAINT),
                            );
                        });
                    }
                });
            let remaining = ui.available_height() - 34.0;
            if remaining > 0.0 {
                ui.add_space(remaining);
            }
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                ui.label(
                    RichText::new(plugin_note)
                        .size(theme::FONT_MICRO)
                        .color(theme::TEXT_FAINT),
                );
            });
            ui.add_space(10.0);
        });

        if new_task {
            self.ai_chat.clear();
        }
        if let Some(id) = open_id {
            self.ai_chat.open_conversation(id);
        }
        if open_settings {
            self.open_ai_settings(ui.ctx());
        }
    }

    /// 所属模块：ui / AI 编曲面板
    /// 功能：绘制对话区：会话标题 + 关闭、低饱和密钥/错误提示条、消息滚动区与底部输入卡片。
    /// 输入：`ui`：右侧对话区。
    /// 输出/副作用：读写 `ai_chat`；可能发送消息、打开设置或附件选择器；不直接修改工程。
    /// 复杂度：每帧 O(消息数 + 消息文本长度)。
    ///
    /// Module: ui / AI composition panel
    /// Purpose: Draw the conversation: title + close, low-saturation key/error strips, the transcript
    /// and the bottom composer.
    /// Inputs: `ui`: conversation area.
    /// Outputs / side effects: reads/writes `ai_chat`; may send, open settings or the attachment picker;
    /// never edits the project directly.
    /// Complexity: O(messages + message length) per frame.
    fn ui_ai_panel_main(&mut self, ui: &mut egui::Ui) {
        let mut close = false;
        let title = if self.ai_chat.messages.is_empty() {
            "新任务".to_string()
        } else {
            crate::app::AiChatState::conversation_title(&self.ai_chat.messages)
        };
        let header = egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(title)
                                .size(theme::FONT_SECTION)
                                .strong()
                                .color(theme::TEXT),
                        )
                        .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("×").size(15.0).color(theme::TEXT_DIM),
                                )
                                .frame(false)
                                .min_size(egui::vec2(24.0, 24.0)),
                            )
                            .on_hover_text("关闭面板")
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });
            });
        ui.painter().hline(
            header.response.rect.left()..=header.response.rect.right(),
            header.response.rect.bottom(),
            egui::Stroke::new(1.0_f32, theme::LINE),
        );
        if close {
            self.close_side_panel();
            return;
        }

        // 缺少密钥时显示整宽提示条：低饱和琥珀底 + 图标 + 右侧「去配置」文字链接。
        // Missing key: a full-width low-saturation amber strip with an icon and a text link.
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
            let strip = egui::Frame::NONE
                .fill(theme::with_alpha(theme::AMBER, 26))
                .inner_margin(egui::Margin::symmetric(16, 9))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (icon, _) =
                            ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                        ui.painter().circle_filled(
                            icon.center(),
                            8.0,
                            theme::with_alpha(theme::AMBER, 60),
                        );
                        ui.painter().text(
                            icon.center(),
                            egui::Align2::CENTER_CENTER,
                            "!",
                            egui::FontId::proportional(10.5),
                            theme::AMBER,
                        );
                        ui.add_space(theme::SPACE_SM);
                        ui.label(
                            RichText::new("尚未配置 API Key，AI 编曲无法发送请求")
                                .size(theme::FONT_LABEL)
                                .color(theme::AMBER),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("去配置")
                                            .size(theme::FONT_LABEL)
                                            .color(theme::AMBER),
                                    )
                                    .frame(false)
                                    .min_size(egui::vec2(0.0, 24.0)),
                                )
                                .clicked()
                            {
                                self.open_ai_settings(ui.ctx());
                            }
                        });
                    });
                });
            ui.painter().hline(
                strip.response.rect.left()..=strip.response.rect.right(),
                strip.response.rect.bottom(),
                egui::Stroke::new(1.0_f32, theme::with_alpha(theme::AMBER, 70)),
            );
        }
        if let Some(error) = self.ai_chat.error.clone() {
            let strip = egui::Frame::NONE
                .fill(theme::with_alpha(theme::RED, 24))
                .inner_margin(egui::Margin::symmetric(16, 9))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(error)
                            .size(theme::FONT_LABEL)
                            .color(theme::RED),
                    );
                });
            ui.painter().hline(
                strip.response.rect.left()..=strip.response.rect.right(),
                strip.response.rect.bottom(),
                egui::Stroke::new(1.0_f32, theme::with_alpha(theme::RED, 70)),
            );
        }

        // 输入区先作为底部停靠面板登记，保证任何窗口高度下都不会被消息区挤掉。
        // Register the composer as a bottom dock first so the transcript can never push it off screen.
        egui::TopBottomPanel::bottom("ai_composer_dock")
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::symmetric(16, 10)),
            )
            .show_inside(ui, |ui| {
                ui.set_width(ui.available_width());
                self.ui_ai_composer(ui);
            });

        let transcript_h = ui.available_height().max(80.0);
        let mut empty_pick: Option<&'static str> = None;
        let streaming = self.ai_chat.live.is_some() || self.ai_chat.pending.is_some();
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(16, 0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical()
                    .id_salt("ai_transcript")
                    .max_height(transcript_h)
                    .auto_shrink([false, false])
                    .stick_to_bottom(streaming)
                    .show(ui, |ui| {
                        ui.add_space(12.0);
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
                                        .size(theme::FONT_LABEL)
                                        .color(theme::TEXT_DIM),
                                );
                            });
                        }
                        ui.add_space(12.0);
                    });
            });
        if let Some(prompt) = empty_pick {
            self.ai_chat.input = prompt.to_string();
        }
    }

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
        let selected_agent = self.ai_chat.agent.clone();
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

        // 快捷指令胶囊：空对话用短标签，已有对话用继续指令；换行而非横向滚动，避免首项被裁掉。
        // Suggestion pills: short labels while empty, continue-prompts afterwards; they wrap instead
        // of scrolling horizontally so the first pill is never clipped.
        let prompts: &[(&str, &str)] = if self.ai_chat.messages.is_empty() {
            &EMPTY_SUGGESTIONS
        } else {
            &QUICK_PROMPTS
        };
        let mut picked: Option<&str> = None;
        ui.horizontal_wrapped(|ui| {
            for (label, prompt) in prompts {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(*label)
                                .size(theme::FONT_MICRO)
                                .color(theme::TEXT_DIM),
                        )
                        .fill(theme::PANEL)
                        .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                        .corner_radius(13)
                        .min_size(egui::vec2(0.0, 26.0)),
                    )
                    .clicked()
                {
                    picked = Some(*prompt);
                }
            }
        });
        if let Some(prompt) = picked {
            self.ai_chat.input = prompt.to_string();
        }
        ui.add_space(theme::SPACE_SM);

        let focused = ui.memory(|memory| memory.has_focus(ai_input_id));
        let mut agent_selected = selected_agent.clone();
        let mut preset_id = self.settings.ai.model_preset.clone();
        let current_reasoning = AiReasoning::from_id(&self.settings.ai.reasoning);
        let mut reasoning = current_reasoning;

        // 输入卡片：边框焦点态 + 底部工具栏（附件 / 智能体 / 模型 / 推理 / 技能乐器 / 发送）。
        // Composer card: focus border and a toolbar with attachment, agent, model, reasoning,
        // skills/instruments and send.
        egui::Frame::NONE
            .fill(theme::PANEL_2)
            .stroke(egui::Stroke::new(
                1.0_f32,
                if focused { theme::ACCENT } else { theme::LINE },
            ))
            .corner_radius(14)
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
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
                        .hint_text("描述你想要的音乐，也可以说“用已安装的 Serum 做一个 Pad”…"),
                );

                ui.add_space(theme::SPACE_SM);
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("+").size(16.0).color(theme::TEXT_DIM),
                            )
                            .frame(false)
                            .min_size(egui::vec2(26.0, 26.0)),
                        )
                        .on_hover_text("添加文本 / Markdown / JSON / MIDI 附件")
                        .clicked()
                    {
                        attach = true;
                    }

                    let agent_label = agents
                        .iter()
                        .find(|(id, _)| id == &agent_selected)
                        .map(|(_, name)| name.as_str())
                        .unwrap_or("智能体");
                    let agent_response = theme::dropdown_button(ui, agent_label, true);
                    egui::Popup::menu(&agent_response).show(|ui| {
                        ui.set_min_width(180.0);
                        if agents.is_empty() {
                            ui.label(
                                RichText::new("暂无智能体")
                                    .size(theme::FONT_LABEL)
                                    .color(theme::TEXT_FAINT),
                            );
                        }
                        for (id, name) in &agents {
                            if ui
                                .selectable_label(agent_selected == *id, name.as_str())
                                .clicked()
                            {
                                agent_selected = id.clone();
                            }
                        }
                    });
                    if agent_selected != self.ai_chat.agent {
                        self.ai_chat.select_agent(&agent_selected);
                    }

                    let preset_label = find_preset(&preset_id)
                        .map(|preset| preset.label)
                        .unwrap_or("自定义");
                    let model_response = theme::dropdown_button(ui, preset_label, false);
                    egui::Popup::menu(&model_response).show(|ui| {
                        ui.set_min_width(160.0);
                        for entry in MODEL_PRESETS {
                            if ui
                                .selectable_label(preset_id == entry.id, entry.label)
                                .clicked()
                            {
                                preset_id = entry.id.to_string();
                            }
                        }
                    });
                    if preset_id != self.settings.ai.model_preset {
                        self.settings.ai.model_preset = preset_id;
                        preset_changed = true;
                    }

                    let reasoning_response =
                        theme::dropdown_button(ui, &format!("推理  {}", reasoning.label()), false);
                    egui::Popup::menu(&reasoning_response).show(|ui| {
                        ui.set_min_width(120.0);
                        for entry in AiReasoning::ALL {
                            if ui
                                .selectable_label(reasoning == entry, entry.label())
                                .clicked()
                            {
                                reasoning = entry;
                            }
                        }
                    });
                    if reasoning != current_reasoning {
                        self.settings.ai.reasoning = reasoning.id().to_string();
                        reasoning_changed = true;
                    }

                    let advanced = theme::dropdown_button(ui, "…", false);
                    let advanced = advanced.on_hover_text(format!(
                        "技能 {displayed_skill_count} · 可用乐器 {usable_count} · 必须乐器 {required_count}"
                    ));
                    egui::Popup::menu(&advanced)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show(|ui| {
                            ui.set_min_width(360.0);
                            ui.label(
                                RichText::new("技能与乐器")
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
                                        filter.is_empty()
                                            || title.to_lowercase().contains(&filter)
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
                                    let filter =
                                        self.ai_chat.skill_filter.trim().to_lowercase();
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
                        });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.ai_chat.pending.is_some() {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("■")
                                            .size(13.0)
                                            .color(egui::Color32::WHITE),
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
                                        RichText::new("↑")
                                            .size(16.0)
                                            .color(egui::Color32::WHITE),
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
/// 功能：绘制一张任务行：选中时整行加深并在左侧画 3 点强调条；标题 12.5、状态副标题 10.5，
/// 运行中在副标题前加绿色圆点。
/// 输入：`ui`、`id` 稳定标识、`selected` 是否选中、`status` 状态、`title`/`subtitle` 文本。
/// 输出/副作用：返回点击响应；只绘制，不修改状态。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: ui / AI composition panel / task rail
/// Purpose: Draw one task row: selected rows get a deeper fill and a 3 pt accent bar; the title is
/// 12.5 pt and the status subtitle 10.5 pt, with a green dot while running.
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
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, AI_TASK_H), egui::Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter().with_clip_rect(rect);
    if selected {
        painter.rect_filled(rect, egui::CornerRadius::from(8), theme::PANEL_3);
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.top() + 8.0),
                egui::vec2(3.0, rect.height() - 16.0),
            ),
            egui::CornerRadius::from(2),
            theme::ACCENT,
        );
    } else if hovered {
        painter.rect_filled(rect, egui::CornerRadius::from(8), theme::PANEL_2);
    }
    let title_galley =
        egui::WidgetText::from(egui::RichText::new(title).size(theme::FONT_BODY).color(
            if selected {
                egui::Color32::WHITE
            } else {
                theme::TEXT_DIM
            },
        ))
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            rect.width() - 24.0,
            egui::TextStyle::Button,
        );
    painter.galley(
        egui::pos2(rect.left() + 12.0, rect.top() + 10.0),
        title_galley,
        theme::TEXT,
    );
    let running = matches!(status, AiTaskStatus::Running);
    if running {
        painter.circle_filled(
            egui::pos2(rect.left() + 15.0, rect.top() + 34.0),
            3.0,
            theme::GREEN,
        );
    }
    painter.text(
        egui::pos2(
            rect.left() + if running { 23.0 } else { 12.0 },
            rect.top() + 34.0,
        ),
        egui::Align2::LEFT_CENTER,
        subtitle,
        egui::FontId::proportional(theme::FONT_MICRO),
        theme::TEXT_FAINT,
    );
    ui.interact(rect, ui.make_persistent_id(id), egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
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
                let response = ui.add(
                    egui::Button::new(
                        RichText::new(format!("任务耗时 {}", format_elapsed(elapsed)))
                            .size(9.0)
                            .color(theme::TEXT_FAINT),
                    )
                    .frame(false),
                );
                let openness = state.openness(ui.ctx());
                theme::paint_dropdown_angle(
                    ui.painter(),
                    egui::pos2(response.rect.right() + 6.0, response.rect.center().y),
                    std::f32::consts::FRAC_PI_2 * (1.0 - openness),
                    theme::TEXT_FAINT,
                );
                ui.add_space(10.0);
                if response.clicked() {
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

/// 所属模块：ui / AI 编曲面板 / 任务栏
/// 功能：把“距今秒数”格式化为刚刚 / N 分钟前 / N 小时前 / N 天前；过旧或缺失时间显示“更早”。
/// 输入：`age` 距今秒数。
/// 输出/副作用：展示文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / AI composition panel / task rail
/// Purpose: Format an age in seconds as just now / N minutes / N hours / N days ago; missing or very
/// old timestamps read "更早".
/// Inputs: `age` seconds since the update.
/// Outputs / side effects: display text; no state change.
/// Complexity: O(1) time and space.
fn relative_time(age: f64) -> String {
    if !age.is_finite() || age >= 30.0 * 86_400.0 {
        return "更早".to_string();
    }
    if age < 60.0 {
        "刚刚".to_string()
    } else if age < 3_600.0 {
        format!("{} 分钟前", (age / 60.0).floor() as i64)
    } else if age < 86_400.0 {
        format!("{} 小时前", (age / 3_600.0).floor() as i64)
    } else {
        format!("{} 天前", (age / 86_400.0).floor() as i64)
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
/// 功能：绘制空对话引导：居中的 AI 方块与一行说明；示例问题已移到输入区上方的胶囊行。
/// 输入：`ui` 绘制区域。
/// 输出/副作用：只绘制；返回 `None`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / AI composition panel / empty state
/// Purpose: Draw the empty-chat guidance: a centered AI tile and one line of copy; example prompts
/// now live in the pill row above the composer.
/// Inputs: `ui` drawing area.
/// Outputs / side effects: paint only; returns `None`.
/// Complexity: O(1) time and space.
fn paint_ai_empty(ui: &mut egui::Ui) -> Option<&'static str> {
    ui.add_space(56.0);
    ui.vertical_centered(|ui| {
        let (tile, _) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(tile, egui::CornerRadius::from(12), theme::PANEL_2);
        ui.painter().text(
            tile.center(),
            egui::Align2::CENTER_CENTER,
            "AI",
            egui::FontId::proportional(14.0),
            theme::TEXT_DIM,
        );
        ui.add_space(12.0);
        ui.label(
            RichText::new("开始描述你想要的音乐，或点击左上角「+ 新任务」")
                .size(theme::FONT_BODY)
                .color(theme::TEXT_FAINT),
        );
    });
    None
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
