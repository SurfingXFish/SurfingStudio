//! 所属模块：app / 主页 / Agent 资料库
//! 文件用途：
//! - 绘制主页 Agent 栏的四个页面：智能体、基础乐理、编曲技巧与乐器资料。
//! - 提供列表、搜索、新建、编辑、保存与删除；保存后写入全局设置并同步到所有窗口。
//! - 支持从已扫描的 VST 插件或内置乐器库选择名称，也允许直接输入自定义名称。
//!
//! 函数清单：
//! - `SurfingApp::ui_home_agents` / `ui_home_agent_basics` / `ui_home_agent_techniques` /
//!   `ui_home_agent_instruments`：四个页面的入口。
//! - `SurfingApp::ui_agent_library_page`：列表 + 编辑器的通用布局。
//! - `SurfingApp::agent_library_list`：按类型绘制左侧列表。
//! - `SurfingApp::agent_library_editor`：绘制右侧编辑器并收集保存/删除动作。
//! - `SurfingApp::save_agent_library_draft`：校验并写回资料库。
//! - `SurfingApp::delete_agent_library_entry`：删除条目并清理智能体默认引用。
//! - `SurfingApp::begin_agent_library_draft`：按类型创建空白草稿。
//! - `agent_field` / `agent_body_field` / `agent_section_title` / `agent_primary_button`：控件。
//!
//! Module: app / home / Agent library
//! Purpose:
//! - Draw the four home Agent pages: agents, theory basics, arranging techniques and instrument notes.
//! - Provide list, search, create, edit, save and delete; saves go to global settings and reach every
//!   window.
//! - Create instrument notes quickly from already scanned VST plugins.
//!
//! Functions:
//! - `SurfingApp::ui_home_agents` / `ui_home_agent_basics` / `ui_home_agent_techniques` /
//!   `ui_home_agent_instruments`: the four page entries.
//! - `SurfingApp::ui_agent_library_page`: shared list + editor layout.
//! - `SurfingApp::agent_library_list`: draw the left list for one entry kind.
//! - `SurfingApp::agent_library_editor`: draw the editor and collect save/delete actions.
//! - `SurfingApp::save_agent_library_draft`: validate and write the draft back.
//! - `SurfingApp::delete_agent_library_entry`: delete an entry and clean agent defaults.
//! - `SurfingApp::begin_agent_library_draft`: create a blank draft of one kind.
//! - `agent_field` / `agent_body_field` / `agent_section_title` / `agent_primary_button`: widgets.

use super::*;
use crate::ai::library::{AiInstrumentDoc, AiSkillDoc, unique_id};
use crate::ai::{AiAgentProfile, AiLibrary};

/// Agent 页左侧列表宽度（逻辑点）。
/// Width of the Agent page list column in logical points.
const AGENT_LIST_W: f32 = 280.0;
/// Agent 页编辑器与列表之间的间距（逻辑点）。
/// Gap between the list and the editor in logical points.
const AGENT_GAP: f32 = 16.0;
/// 列表行高度（逻辑点）；用于虚拟化滚动。
/// List row height in logical points used by virtual scrolling.
const AGENT_ROW_H: f32 = 56.0;
/// 列表搜索框高度（逻辑点）。
/// Search-field height in logical points.
const AGENT_SEARCH_H: f32 = 26.0;

impl SurfingApp {
    /// 所属模块：app / 主页 / Agent 资料库
    /// 功能：绘制智能体管理页。
    /// 输入：`ui`：主页右侧内容区。
    /// 输出/副作用：读写资料库草稿；保存时写全局设置。
    /// 复杂度：每帧 O(智能体数)。
    ///
    /// Module: app / home / Agent library
    /// Purpose: Draw the agent management page.
    /// Inputs: `ui`: home content area.
    /// Outputs / side effects: edits drafts; saving writes global settings.
    /// Complexity: O(agents) per frame.
    pub(super) fn ui_home_agents(&mut self, ui: &mut egui::Ui) {
        self.ui_agent_library_page(ui, AiLibraryKind::Agent);
    }

    /// 所属模块：app / 主页 / Agent 资料库
    /// 功能：绘制基础乐理技能页。
    /// 输入：`ui`：主页右侧内容区。
    /// 输出/副作用：读写资料库草稿；保存时写全局设置。
    /// 复杂度：每帧 O(技能数)。
    ///
    /// Module: app / home / Agent library
    /// Purpose: Draw the theory-skill page.
    /// Inputs: `ui`: home content area.
    /// Outputs / side effects: edits drafts; saving writes global settings.
    /// Complexity: O(skills) per frame.
    pub(super) fn ui_home_agent_basics(&mut self, ui: &mut egui::Ui) {
        self.ui_agent_library_page(ui, AiLibraryKind::Basic);
    }

    /// 所属模块：app / 主页 / Agent 资料库
    /// 功能：绘制编曲技巧页。
    /// 输入：`ui`：主页右侧内容区。
    /// 输出/副作用：读写资料库草稿；保存时写全局设置。
    /// 复杂度：每帧 O(技巧数)。
    ///
    /// Module: app / home / Agent library
    /// Purpose: Draw the arranging-technique page.
    /// Inputs: `ui`: home content area.
    /// Outputs / side effects: edits drafts; saving writes global settings.
    /// Complexity: O(techniques) per frame.
    pub(super) fn ui_home_agent_techniques(&mut self, ui: &mut egui::Ui) {
        self.ui_agent_library_page(ui, AiLibraryKind::Technique);
    }

    /// 所属模块：app / 主页 / Agent 资料库
    /// 功能：绘制乐器资料页。
    /// 输入：`ui`：主页右侧内容区。
    /// 输出/副作用：读写资料库草稿；保存时写全局设置。
    /// 复杂度：每帧 O(乐器数 + 插件数)。
    ///
    /// Module: app / home / Agent library
    /// Purpose: Draw the instrument-note page.
    /// Inputs: `ui`: home content area.
    /// Outputs / side effects: edits drafts; saving writes global settings.
    /// Complexity: O(instruments + plugins) per frame.
    pub(super) fn ui_home_agent_instruments(&mut self, ui: &mut egui::Ui) {
        self.ui_agent_library_page(ui, AiLibraryKind::Instrument);
    }

    /// 所属模块：app / 主页 / Agent 资料库
    /// 功能：绘制通用布局：标题、搜索、新建按钮、左侧列表与右侧编辑器。
    /// 输入：`ui`：内容区；`kind`：条目类型。
    /// 输出/副作用：可能开始新草稿、切换选择或保存/删除；不直接修改工程。
    /// 复杂度：每帧 O(条目数)。
    ///
    /// Module: app / home / Agent library
    /// Purpose: Draw the shared layout: title, search, create button, left list and right editor.
    /// Inputs: `ui`: content area; `kind`: entry kind.
    /// Outputs / side effects: may start a draft, change selection, save or delete; no project change.
    /// Complexity: O(entries) per frame.
    fn ui_agent_library_page(&mut self, ui: &mut egui::Ui, kind: AiLibraryKind) {
        let library = self.settings.ai.library.clone();
        let (title, subtitle) = agent_page_titles(kind);
        let count = match kind {
            AiLibraryKind::Agent => library.agents.len(),
            AiLibraryKind::Basic => library.basics.len(),
            AiLibraryKind::Technique => library.techniques.len(),
            AiLibraryKind::Instrument => library.instruments.len(),
        };
        ui.add_space(2.0);
        // 页头：小号眉题 + 大标题 + 统计徽章，形成资料库的“索引册”气质。
        // Page header: eyebrow label, large title and stat badges for a library-index feel.
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("AGENT / LIBRARY")
                        .size(9.0)
                        .strong()
                        .color(theme::ACCENT),
                );
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(title)
                        .size(30.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.label(
                    egui::RichText::new(subtitle)
                        .size(11.5)
                        .color(theme::TEXT_DIM),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                if agent_primary_button(ui, "新建").clicked() {
                    self.begin_agent_library_draft(kind);
                }
                ui.add_space(6.0);
                stat_pill(ui, count.to_string(), "条目");
                ui.add_space(6.0);
                stat_pill(
                    ui,
                    match kind {
                        AiLibraryKind::Agent => library.basics.len() + library.techniques.len(),
                        AiLibraryKind::Basic | AiLibraryKind::Technique => {
                            library.instruments.len()
                        }
                        AiLibraryKind::Instrument => library.techniques.len(),
                    }
                    .to_string(),
                    match kind {
                        AiLibraryKind::Agent => "可绑定技能",
                        AiLibraryKind::Basic | AiLibraryKind::Technique => "关联乐器",
                        AiLibraryKind::Instrument => "技巧",
                    },
                );
            });
        });
        ui.add_space(12.0);
        egui::Frame::NONE
            .fill(theme::PANEL)
            .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("搜索")
                            .size(10.0)
                            .color(theme::TEXT_FAINT),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.agent_library.search)
                            .frame(egui::Frame::NONE)
                            .hint_text("按名称、说明或标识过滤…")
                            .desired_width(f32::INFINITY)
                            .min_size(egui::vec2(0.0, AGENT_SEARCH_H)),
                    );
                });
            });
        if kind == AiLibraryKind::Instrument {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                for category in [
                    "全部", "键盘", "吉他", "弦乐", "管乐", "打击", "合成", "民乐", "其他",
                ] {
                    let selected = self.agent_library.instrument_category_filter == category;
                    if agent_filter_chip(ui, category, selected).clicked() {
                        self.agent_library.instrument_category_filter = category.to_string();
                    }
                }
            });
        }
        ui.add_space(8.0);
        if let Some(notice) = &self.agent_library.notice {
            ui.label(
                egui::RichText::new(notice)
                    .size(10.5)
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(4.0);
        }

        let available = ui.available_size();
        ui.horizontal_top(|ui| {
            ui.set_height(available.y);
            ui.allocate_ui(egui::vec2(AGENT_LIST_W, available.y), |ui| {
                // allocate_ui 会继承外层的水平布局；显式切回纵向，列表行才不会被排成一行。
                // allocate_ui inherits the horizontal layout; switch back to vertical so list rows
                // stack instead of lining up side by side.
                ui.vertical(|ui| {
                    ui.set_width(AGENT_LIST_W);
                    egui::Frame::NONE
                        .fill(theme::BG_DEEP)
                        .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                        .corner_radius(12)
                        .inner_margin(egui::Margin::symmetric(10, 10))
                        .show(ui, |ui| {
                            ui.set_width(AGENT_LIST_W - 20.0);
                            ui.set_height(available.y - 20.0);
                            self.agent_library_list(ui, kind, &library);
                        });
                });
            });
            ui.add_space(AGENT_GAP);
            ui.vertical(|ui| {
                ui.set_width((available.x - AGENT_LIST_W - AGENT_GAP).max(280.0));
                self.agent_library_editor(ui, kind, &library);
            });
        });
    }

    /// 所属模块：app / 主页 / Agent 资料库
    /// 功能：按条目类型绘制左侧列表；点击行开始编辑。
    /// 输入：`ui` 列表区；`kind` 条目类型；`library` 当前资料库快照。
    /// 输出/副作用：可能设置 `selected` 与编辑草稿；不修改资料库。
    /// 复杂度：每帧 O(条目数)。
    ///
    /// Module: app / home / Agent library
    /// Purpose: Draw the left list for one entry kind; clicking a row starts editing.
    /// Inputs: `ui`; `kind`; `library` snapshot.
    /// Outputs / side effects: may set the selection and draft; the library is unchanged.
    /// Complexity: O(entries) per frame.
    fn agent_library_list(&mut self, ui: &mut egui::Ui, kind: AiLibraryKind, library: &AiLibrary) {
        let search = self.agent_library.search.trim().to_lowercase();
        let selected = self.agent_library.selected.clone();
        let category_filter = self.agent_library.instrument_category_filter.clone();
        let mut rows: Vec<(String, String, String, String)> = Vec::new();
        match kind {
            AiLibraryKind::Agent => {
                for agent in &library.agents {
                    if !matches_search(&search, &[&agent.name, &agent.description, &agent.id]) {
                        continue;
                    }
                    rows.push((
                        agent.id.clone(),
                        agent.name.clone(),
                        agent.description.clone(),
                        "AGENT".into(),
                    ));
                }
            }
            AiLibraryKind::Basic | AiLibraryKind::Technique => {
                let entries = if kind == AiLibraryKind::Basic {
                    &library.basics
                } else {
                    &library.techniques
                };
                for skill in entries {
                    if !matches_search(&search, &[&skill.title, &skill.summary, &skill.id]) {
                        continue;
                    }
                    rows.push((
                        skill.id.clone(),
                        skill.title.clone(),
                        skill.summary.clone(),
                        if kind == AiLibraryKind::Basic {
                            "THEORY".into()
                        } else {
                            "TECHNIQUE".into()
                        },
                    ));
                }
            }
            AiLibraryKind::Instrument => {
                for instrument in &library.instruments {
                    if !matches_search(
                        &search,
                        &[
                            &instrument.name,
                            &instrument.category,
                            &instrument.aliases,
                            &instrument.id,
                        ],
                    ) {
                        continue;
                    }
                    if !instrument_category_matches(&instrument.category, &category_filter) {
                        continue;
                    }
                    rows.push((
                        instrument.id.clone(),
                        instrument.name.clone(),
                        if instrument.aliases.is_empty() {
                            instrument.category.clone()
                        } else {
                            format!("{} · {}", instrument.category, instrument.aliases)
                        },
                        instrument.category.clone(),
                    ));
                }
            }
        }
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("索引")
                    .size(9.0)
                    .strong()
                    .color(theme::TEXT_FAINT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{} 项", rows.len()))
                        .size(9.0)
                        .color(theme::TEXT_FAINT),
                );
            });
        });
        ui.add_space(6.0);
        let total = rows.len();
        let mut clicked: Option<String> = None;
        egui::ScrollArea::vertical()
            .id_salt(("agent_library_rows", agent_kind_id(kind)))
            .auto_shrink([false, false])
            .show_rows(ui, AGENT_ROW_H, total, |ui, range| {
                for index in range {
                    let (id, title, subtitle, badge) = &rows[index];
                    let active = selected.as_deref() == Some(id.as_str());
                    if agent_list_row(ui, index + 1, title, subtitle, badge, active).clicked() {
                        clicked = Some(id.clone());
                    }
                }
            });
        if let Some(id) = clicked {
            if let Some(draft) = draft_for_entry(library, kind, &id) {
                self.agent_library.selected = Some(id);
                self.agent_library.draft = Some(draft);
                self.agent_library.notice = None;
            }
        }
    }

    /// 所属模块：app / 主页 / Agent 资料库
    /// 功能：绘制右侧编辑器；字段按条目类型显示，并提供保存、取消与删除。
    /// 输入：`ui` 编辑区；`kind` 条目类型；`library` 当前资料库快照。
    /// 输出/副作用：修改草稿；保存/删除时写全局设置。
    /// 复杂度：每帧 O(默认技能数)。
    ///
    /// Module: app / home / Agent library
    /// Purpose: Draw the right editor; fields depend on the entry kind and offer save/cancel/delete.
    /// Inputs: `ui`; `kind`; `library` snapshot.
    /// Outputs / side effects: edits the draft; save/delete writes global settings.
    /// Complexity: O(default skills) per frame.
    fn agent_library_editor(
        &mut self,
        ui: &mut egui::Ui,
        kind: AiLibraryKind,
        library: &AiLibrary,
    ) {
        let mut pick_filter = self.agent_library.instrument_pick_filter.clone();
        let Some(draft) = self.agent_library.draft.as_mut() else {
            ui.add_space(48.0);
            ui.vertical_centered(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(64.0, 64.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, egui::CornerRadius::from(16), theme::PANEL_2);
                ui.painter().rect_stroke(
                    rect,
                    egui::CornerRadius::from(16),
                    egui::Stroke::new(1.0_f32, theme::LINE),
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "A",
                    egui::FontId::proportional(26.0),
                    theme::TEXT_DIM,
                );
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("选择一条资料开始编辑")
                        .size(13.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.label(
                    egui::RichText::new("左侧是索引，右侧是工作台；也可以点击「新建」。")
                        .size(10.5)
                        .color(theme::TEXT_FAINT),
                );
            });
            return;
        };
        let current_id = draft_id(draft);
        let mut save = false;
        let mut cancel = false;
        let mut ask_delete = false;
        egui::Frame::NONE
            .fill(theme::PANEL)
            .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
            .corner_radius(12)
            .inner_margin(egui::Margin::symmetric(18, 14))
            .show(ui, |ui| {
                let is_new = match draft {
                    AiLibraryDraft::Agent { original, .. }
                    | AiLibraryDraft::Basic { original, .. }
                    | AiLibraryDraft::Technique { original, .. }
                    | AiLibraryDraft::Instrument { original, .. } => original.is_none(),
                };
                ui.label(
                    egui::RichText::new("WORKBENCH")
                        .size(8.5)
                        .strong()
                        .color(theme::ACCENT),
                );
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(if is_new {
                            "新建条目"
                        } else {
                            "编辑条目"
                        })
                        .size(18.0)
                        .strong()
                        .color(theme::TEXT),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if agent_primary_button(ui, "保存").clicked() {
                            save = true;
                        }
                        if agent_ghost_button(ui, "取消").clicked() {
                            cancel = true;
                        }
                        if !is_new && agent_danger_button(ui, "删除").clicked() {
                            ask_delete = true;
                        }
                    });
                });
                ui.add_space(8.0);
                let line = ui.available_rect_before_wrap();
                ui.painter().hline(
                    line.left()..=line.right(),
                    line.top(),
                    egui::Stroke::new(1.0_f32, theme::LINE),
                );
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .id_salt(("agent_library_editor", agent_kind_id(kind)))
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - 12.0).max(200.0))
                    .show(ui, |ui| match draft {
                        AiLibraryDraft::Agent { entry, .. } => {
                            agent_field(ui, "名称", &mut entry.name);
                            agent_field(ui, "说明", &mut entry.description);
                            ui.label(
                                egui::RichText::new(format!("标识：{}（创建后固定）", entry.id))
                                    .size(10.0)
                                    .color(theme::TEXT_FAINT),
                            );
                            agent_body_field(ui, "角色补充说明 / 创作准则", 5, &mut entry.style);
                            agent_section_title(ui, "默认加载的基础技能");
                            for skill in &library.basics {
                                let mut checked = entry.default_skill_ids.contains(&skill.id);
                                if editor_toggle_row(ui, &skill.title, &mut checked) {
                                    toggle_id(&mut entry.default_skill_ids, &skill.id, checked);
                                }
                            }
                            agent_section_title(ui, "默认加载的编曲技巧（正文会直接进入提示词）");
                            for technique in &library.techniques {
                                let mut checked =
                                    entry.default_technique_ids.contains(&technique.id);
                                if editor_toggle_row(ui, &technique.title, &mut checked) {
                                    toggle_id(
                                        &mut entry.default_technique_ids,
                                        &technique.id,
                                        checked,
                                    );
                                }
                            }
                            agent_section_title(ui, "默认加载的乐器资料");
                            default_instrument_selector(ui, entry, library);
                        }
                        AiLibraryDraft::Basic { entry, .. }
                        | AiLibraryDraft::Technique { entry, .. } => {
                            agent_field(ui, "标题", &mut entry.title);
                            agent_field(ui, "一句话特点", &mut entry.summary);
                            ui.label(
                                egui::RichText::new(format!("标识：{}（创建后固定）", entry.id))
                                    .size(10.0)
                                    .color(theme::TEXT_FAINT),
                            );
                            agent_body_field(ui, "正文（Markdown）", 18, &mut entry.body);
                        }
                        AiLibraryDraft::Instrument { entry, .. } => {
                            let mut chosen: Option<AiInstrumentDoc> = None;
                            ui.horizontal(|ui| {
                                let response = ui.add(
                                    egui::Button::new(egui::RichText::new("选择乐器…").size(10.5))
                                        .fill(theme::PANEL_2)
                                        .corner_radius(8)
                                        .min_size(egui::vec2(0.0, 26.0)),
                                );
                                egui::Popup::menu(&response)
                                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                                    .show(|ui| {
                                        ui.set_min_width(300.0);
                                        ui.add(
                                            egui::TextEdit::singleline(&mut pick_filter)
                                                .hint_text("搜索内置乐器或 VST")
                                                .desired_width(f32::INFINITY),
                                        );
                                        let needle = pick_filter.trim().to_lowercase();
                                        // 选项只在弹出层打开时构建，避免每帧克隆 130 条资料。
                                        // Build options only while the popup is open, not every frame.
                                        let mut options: Vec<AiInstrumentDoc> =
                                            library.instruments.clone();
                                        if crate::platform::caps().plugin_hosting {
                                            for plugin in &self.plugin_cache {
                                                if plugin.name.is_empty()
                                                    || options
                                                        .iter()
                                                        .any(|option| option.name == plugin.name)
                                                {
                                                    continue;
                                                }
                                                let catalogue =
                                                    crate::plugin::lookup_catalog(&plugin.name);
                                                options.push(AiInstrumentDoc {
                                                    id: String::new(),
                                                    name: plugin.name.clone(),
                                                    aliases: plugin.name.clone(),
                                                    category: if plugin.category.is_empty() {
                                                        catalogue
                                                            .map(|entry| entry.category.to_string())
                                                            .unwrap_or_default()
                                                    } else {
                                                        plugin.category.clone()
                                                    },
                                                    range: String::new(),
                                                    description: catalogue
                                                        .map(|entry| entry.description.to_string())
                                                        .unwrap_or_default(),
                                                    usage: String::new(),
                                                    techniques: String::new(),
                                                    body: String::new(),
                                                });
                                            }
                                        }
                                        egui::ScrollArea::vertical()
                                            .max_height(340.0)
                                            .auto_shrink([false, false])
                                            .show_rows(ui, 24.0, options.len(), |ui, range| {
                                                for index in range {
                                                    let Some(option) = options.get(index) else {
                                                        continue;
                                                    };
                                                    let label = if option.category.is_empty() {
                                                        option.name.clone()
                                                    } else {
                                                        format!(
                                                            "{}（{}）",
                                                            option.name, option.category
                                                        )
                                                    };
                                                    if !needle.is_empty()
                                                        && !label.to_lowercase().contains(&needle)
                                                    {
                                                        continue;
                                                    }
                                                    if ui
                                                        .add(
                                                            egui::Button::new(
                                                                egui::RichText::new(&label)
                                                                    .size(10.0),
                                                            )
                                                            .fill(egui::Color32::TRANSPARENT)
                                                            .stroke(egui::Stroke::NONE)
                                                            .min_size(egui::vec2(
                                                                ui.available_width(),
                                                                22.0,
                                                            )),
                                                        )
                                                        .clicked()
                                                    {
                                                        chosen = Some(option.clone());
                                                    }
                                                }
                                            });
                                    });
                            });
                            if let Some(option) = chosen {
                                entry.name = option.name;
                                entry.aliases = option.aliases;
                                entry.category = option.category;
                                entry.range = option.range;
                                if entry.description.trim().is_empty() {
                                    entry.description = option.description;
                                }
                                if entry.usage.trim().is_empty() {
                                    entry.usage = option.usage;
                                }
                                if entry.techniques.trim().is_empty() {
                                    entry.techniques = option.techniques;
                                }
                            }
                            agent_field(ui, "名称（可直接自定义）", &mut entry.name);
                            agent_field(ui, "别称 / 插件名（逗号分隔）", &mut entry.aliases);
                            agent_field(ui, "分类", &mut entry.category);
                            agent_field(ui, "音域", &mut entry.range);
                            agent_field(ui, "音色与角色说明", &mut entry.description);
                            agent_field(ui, "使用场景", &mut entry.usage);
                            agent_field(ui, "写作 / 演奏技巧", &mut entry.techniques);
                            agent_body_field(ui, "详细正文（Markdown，可空）", 10, &mut entry.body);
                        }
                    });
            });

        self.agent_library.instrument_pick_filter = pick_filter;

        if cancel {
            self.agent_library.draft = None;
            self.agent_library.notice = None;
        }
        if ask_delete {
            if let Some(id) = current_id {
                self.agent_library.confirm_delete = Some(id);
            }
        }
        if save {
            self.save_agent_library_draft();
        }
        self.agent_library_confirm_delete(ui, kind);
    }

    /// 所属模块：app / 主页 / Agent 资料库
    /// 功能：绘制删除确认条；确认后删除条目。
    /// 输入：`ui` 内容区；`kind` 条目类型。
    /// 输出/副作用：可能删除条目并写全局设置。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / home / Agent library
    /// Purpose: Draw the delete confirmation bar and delete the entry once confirmed.
    /// Inputs: `ui`; `kind`.
    /// Outputs / side effects: may delete an entry and write global settings.
    /// Complexity: O(1) time and space.
    fn agent_library_confirm_delete(&mut self, ui: &mut egui::Ui, kind: AiLibraryKind) {
        let Some(id) = self.agent_library.confirm_delete.clone() else {
            return;
        };
        ui.add_space(8.0);
        egui::Frame::NONE
            .fill(theme::PANEL_2)
            .stroke(egui::Stroke::new(1.0_f32, theme::RED))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("确定删除「{id}」？此操作立即保存。"))
                            .size(11.0)
                            .color(theme::TEXT),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(egui::RichText::new("删除").color(theme::RED))
                            .clicked()
                        {
                            self.delete_agent_library_entry(kind, &id);
                        }
                        if ui.button("取消").clicked() {
                            self.agent_library.confirm_delete = None;
                        }
                    });
                });
            });
    }

    /// 所属模块：app / 主页 / Agent 资料库
    /// 功能：校验当前草稿并写回资料库；清空草稿并保存全局设置。
    /// 输入：`self`。
    /// 输出/副作用：修改 `settings.ai.library`、选中项与提示；写盘。
    /// 复杂度：时间 O(条目数)，空间 O(条目数)。
    ///
    /// Module: app / home / Agent library
    /// Purpose: Validate the current draft and write it back; clear the draft and save settings.
    /// Inputs: `self`.
    /// Outputs / side effects: mutates the library, selection and notice; writes the file.
    /// Complexity: O(entries) time and space.
    fn save_agent_library_draft(&mut self) {
        let Some(draft) = self.agent_library.draft.take() else {
            return;
        };
        let mut library = self.settings.ai.library.clone();
        let saved_id = match draft {
            AiLibraryDraft::Agent {
                original,
                mut entry,
            } => {
                entry.name = entry.name.trim().to_string();
                entry.description = entry.description.trim().to_string();
                if entry.name.is_empty() {
                    self.agent_library.notice = Some("名称不能为空".into());
                    return;
                }
                let id = entry.id.clone();
                if let Some(original) = original {
                    if let Some(slot) = library.agents.iter_mut().find(|agent| agent.id == original)
                    {
                        *slot = entry;
                    } else {
                        library.agents.push(entry);
                    }
                } else {
                    library.agents.push(entry);
                }
                id
            }
            AiLibraryDraft::Basic {
                original,
                mut entry,
            } => {
                if entry.title.trim().is_empty() {
                    self.agent_library.notice = Some("标题不能为空".into());
                    return;
                }
                entry.title = entry.title.trim().to_string();
                let id = entry.id.clone();
                if let Some(original) = original {
                    upsert_skill(&mut library.basics, original, entry);
                } else {
                    library.basics.push(entry);
                }
                id
            }
            AiLibraryDraft::Technique {
                original,
                mut entry,
            } => {
                if entry.title.trim().is_empty() {
                    self.agent_library.notice = Some("标题不能为空".into());
                    return;
                }
                entry.title = entry.title.trim().to_string();
                let id = entry.id.clone();
                if let Some(original) = original {
                    upsert_skill(&mut library.techniques, original, entry);
                } else {
                    library.techniques.push(entry);
                }
                id
            }
            AiLibraryDraft::Instrument {
                original,
                mut entry,
            } => {
                if entry.name.trim().is_empty() {
                    self.agent_library.notice = Some("名称不能为空".into());
                    return;
                }
                entry.name = entry.name.trim().to_string();
                let id = entry.id.clone();
                if let Some(original) = original {
                    if let Some(slot) = library
                        .instruments
                        .iter_mut()
                        .find(|instrument| instrument.id == original)
                    {
                        *slot = entry;
                    } else {
                        library.instruments.push(entry);
                    }
                } else {
                    library.instruments.push(entry);
                }
                id
            }
        };
        library.normalise();
        self.settings.ai.library = library;
        self.commit_settings();
        self.agent_library.selected = Some(saved_id);
        self.agent_library.notice = Some("已保存".into());
        self.status = "Agent 资料已保存".into();
    }

    /// 所属模块：app / 主页 / Agent 资料库
    /// 功能：删除条目并清理智能体对它的默认引用；至少保留一个智能体。
    /// 输入：`kind` 条目类型；`id` 条目标识。
    /// 输出/副作用：修改资料库、清空草稿与选中项；写盘。
    /// 复杂度：时间 O(条目数 × 默认技能数)，空间 O(条目数)。
    ///
    /// Module: app / home / Agent library
    /// Purpose: Delete an entry and remove agent default references; keep at least one agent.
    /// Inputs: `kind`; `id`.
    /// Outputs / side effects: mutates the library, clears draft/selection; writes the file.
    /// Complexity: O(entries × default skills) time and O(entries) space.
    fn delete_agent_library_entry(&mut self, kind: AiLibraryKind, id: &str) {
        let mut library = self.settings.ai.library.clone();
        match kind {
            AiLibraryKind::Agent => {
                if library.agents.len() <= 1 {
                    self.agent_library.notice = Some("至少保留一个智能体".into());
                    self.agent_library.confirm_delete = None;
                    return;
                }
                library.agents.retain(|agent| agent.id != id);
            }
            AiLibraryKind::Basic => {
                library.basics.retain(|skill| skill.id != id);
                for agent in &mut library.agents {
                    agent.default_skill_ids.retain(|entry| entry != id);
                }
            }
            AiLibraryKind::Technique => {
                library.techniques.retain(|skill| skill.id != id);
                for agent in &mut library.agents {
                    agent.default_technique_ids.retain(|entry| entry != id);
                }
            }
            AiLibraryKind::Instrument => {
                library.instruments.retain(|instrument| instrument.id != id);
                for agent in &mut library.agents {
                    agent.default_instrument_ids.retain(|entry| entry != id);
                }
            }
        }
        library.normalise();
        self.settings.ai.library = library;
        self.commit_settings();
        self.agent_library.draft = None;
        self.agent_library.selected = None;
        self.agent_library.confirm_delete = None;
        self.agent_library.notice = Some("已删除".into());
        self.status = "Agent 资料已删除".into();
    }

    /// 所属模块：app / 主页 / Agent 资料库
    /// 功能：创建指定类型的空白草稿，并生成不冲突的稳定 id。
    /// 输入：`kind` 条目类型。
    /// 输出/副作用：设置 `draft` 与选中项；不修改资料库。
    /// 复杂度：时间 O(条目数)，空间 O(1)。
    ///
    /// Module: app / home / Agent library
    /// Purpose: Create a blank draft of one kind with a stable, non-conflicting id.
    /// Inputs: `kind`.
    /// Outputs / side effects: sets the draft and selection; the library is unchanged.
    /// Complexity: O(entries) time and O(1) space.
    fn begin_agent_library_draft(&mut self, kind: AiLibraryKind) {
        let library = &self.settings.ai.library;
        let draft = match kind {
            AiLibraryKind::Agent => {
                let id = unique_id("agent", |candidate| {
                    library.agents.iter().any(|agent| agent.id == candidate)
                });
                let entry = AiAgentProfile::new(&id);
                self.agent_library.selected = Some(id);
                AiLibraryDraft::Agent {
                    original: None,
                    entry,
                }
            }
            AiLibraryKind::Basic => {
                let id = unique_id("basic", |candidate| {
                    library.basics.iter().any(|skill| skill.id == candidate)
                });
                let entry = AiSkillDoc::new(&id);
                self.agent_library.selected = Some(id);
                AiLibraryDraft::Basic {
                    original: None,
                    entry,
                }
            }
            AiLibraryKind::Technique => {
                let id = unique_id("technique", |candidate| {
                    library.techniques.iter().any(|skill| skill.id == candidate)
                });
                let entry = AiSkillDoc::new(&id);
                self.agent_library.selected = Some(id);
                AiLibraryDraft::Technique {
                    original: None,
                    entry,
                }
            }
            AiLibraryKind::Instrument => {
                let id = unique_id("instrument", |candidate| {
                    library
                        .instruments
                        .iter()
                        .any(|instrument| instrument.id == candidate)
                });
                let entry = AiInstrumentDoc::new(&id);
                self.agent_library.selected = Some(id);
                AiLibraryDraft::Instrument {
                    original: None,
                    entry,
                }
            }
        };
        self.agent_library.draft = Some(draft);
        self.agent_library.notice = Some("尚未保存".into());
    }
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：返回页面标题与说明。
/// 输入：`kind` 条目类型。
/// 输出/副作用：静态文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Return the page title and subtitle.
/// Inputs: `kind`.
/// Outputs / side effects: static text; no state change.
/// Complexity: O(1) time and space.
fn agent_page_titles(kind: AiLibraryKind) -> (&'static str, &'static str) {
    match kind {
        AiLibraryKind::Agent => (
            "Agent",
            "管理编曲智能体；智能体决定风格取向与默认加载的技能。",
        ),
        AiLibraryKind::Basic => ("基础", "乐理、和声、曲式与风格技能，可被智能体默认加载。"),
        AiLibraryKind::Technique => (
            "技能",
            "编曲技巧（织体、和弦走向、律动等），AI 可按需调用。",
        ),
        AiLibraryKind::Instrument => ("乐器", "各乐器与 VST 插件的说明、使用场景与写作技巧。"),
    }
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：返回条目类型的稳定 id 片段，用于 egui 控件 id。
/// 输入：`kind` 条目类型。
/// 输出/副作用：静态文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Return a stable id fragment for egui widget ids.
/// Inputs: `kind`.
/// Outputs / side effects: static text; no state change.
/// Complexity: O(1) time and space.
fn agent_kind_id(kind: AiLibraryKind) -> &'static str {
    match kind {
        AiLibraryKind::Agent => "agent",
        AiLibraryKind::Basic => "basic",
        AiLibraryKind::Technique => "technique",
        AiLibraryKind::Instrument => "instrument",
    }
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：判断条目是否匹配搜索文本（大小写不敏感）。
/// 输入：`search` 小写搜索词；`fields` 参与匹配的字段。
/// 输出/副作用：是否匹配；不修改状态。
/// 复杂度：时间 O(字段数 × 文本长度)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Whether an entry matches the lower-case search text.
/// Inputs: `search`; `fields`.
/// Outputs / side effects: a boolean; no state change.
/// Complexity: O(fields × text length) time and O(1) space.
fn matches_search(search: &str, fields: &[&str]) -> bool {
    search.is_empty()
        || fields
            .iter()
            .any(|field| field.to_lowercase().contains(search))
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：绘制一行列表项（标题 + 副标题），支持悬停与选中。
/// 输入：`ui` 列表；`title` 标题；`subtitle` 副标题；`selected` 是否选中。
/// 输出/副作用：分配整行并绘制；点击由调用方处理。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Draw one list row (title + subtitle) with hover and selection states.
/// Inputs: `ui`; `title`; `subtitle`; `selected`.
/// Outputs / side effects: allocates and paints the row; the caller handles clicks.
/// Complexity: O(1) time and space.
fn agent_list_row(
    ui: &mut egui::Ui,
    index: usize,
    title: &str,
    subtitle: &str,
    badge: &str,
    selected: bool,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, AGENT_ROW_H), egui::Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter().with_clip_rect(rect);
    if selected {
        painter.rect_filled(rect, egui::CornerRadius::from(8), theme::ACCENT_DARK);
        painter.rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
            egui::CornerRadius::from(2),
            theme::ACCENT,
        );
    } else if hovered {
        painter.rect_filled(rect, egui::CornerRadius::from(8), theme::PANEL_2);
    }
    // 字母/汉字徽章：让每条资料有一个可识别的“封面”。
    // Monogram tile: gives every entry a recognisable cover.
    let tile = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 10.0, rect.top() + 13.0),
        egui::vec2(30.0, 30.0),
    );
    painter.rect_filled(
        tile,
        egui::CornerRadius::from(7),
        if selected {
            theme::ACCENT
        } else {
            theme::PANEL_3
        },
    );
    let monogram = title
        .chars()
        .next()
        .unwrap_or('·')
        .to_uppercase()
        .to_string();
    painter.text(
        tile.center(),
        egui::Align2::CENTER_CENTER,
        monogram,
        egui::FontId::proportional(13.0),
        egui::Color32::WHITE,
    );
    let text_left = rect.left() + 50.0;
    let title_color = if selected {
        egui::Color32::WHITE
    } else {
        theme::TEXT
    };
    painter.text(
        egui::pos2(text_left, rect.top() + 18.0),
        egui::Align2::LEFT_CENTER,
        truncate_list_text(title, 26),
        egui::FontId::proportional(12.5),
        title_color,
    );
    painter.text(
        egui::pos2(text_left, rect.top() + 36.0),
        egui::Align2::LEFT_CENTER,
        truncate_list_text(subtitle, 30),
        egui::FontId::proportional(9.5),
        theme::TEXT_DIM,
    );
    let index_text = format!("{index:02}");
    painter.text(
        egui::pos2(rect.right() - 10.0, rect.top() + 13.0),
        egui::Align2::RIGHT_CENTER,
        index_text,
        egui::FontId::proportional(8.5),
        theme::TEXT_FAINT,
    );
    if !badge.is_empty() {
        painter.text(
            egui::pos2(rect.right() - 10.0, rect.top() + 38.0),
            egui::Align2::RIGHT_CENTER,
            truncate_list_text(&badge.to_uppercase(), 12),
            egui::FontId::proportional(8.0),
            if selected {
                egui::Color32::from_rgb(200, 216, 255)
            } else {
                theme::TEXT_FAINT
            },
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：绘制页头统计徽章（数字 + 说明）。
/// 输入：`ui`；`value` 数字文本；`label` 说明。
/// 输出/副作用：写入绘制命令；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Draw a header stat badge (number plus label).
/// Inputs: `ui`; `value`; `label`.
/// Outputs / side effects: emits paint commands; no state change.
/// Complexity: O(1) time and space.
fn stat_pill(ui: &mut egui::Ui, value: String, label: &str) {
    egui::Frame::NONE
        .fill(theme::PANEL)
        .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(value)
                        .size(13.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.label(
                    egui::RichText::new(label)
                        .size(9.0)
                        .color(theme::TEXT_FAINT),
                );
            });
        });
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：绘制乐器分类筛选胶囊。
/// 输入：`ui`；`label` 分类名；`selected` 是否选中。
/// 输出/副作用：返回点击响应；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Draw an instrument category filter chip.
/// Inputs: `ui`; `label`; `selected`.
/// Outputs / side effects: the click response; no state change.
/// Complexity: O(1) time and space.
fn agent_filter_chip(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let fill = if selected {
        theme::ACCENT_DARK
    } else {
        theme::PANEL
    };
    let color = if selected {
        egui::Color32::WHITE
    } else {
        theme::TEXT_DIM
    };
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(9.5).color(color))
            .fill(fill)
            .stroke(egui::Stroke::new(
                1.0_f32,
                if selected { theme::ACCENT } else { theme::LINE },
            ))
            .corner_radius(10)
            .min_size(egui::vec2(0.0, 22.0)),
    )
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：判断乐器分类是否匹配筛选胶囊。
/// 输入：`category` 乐器分类；`filter` 筛选名。
/// 输出/副作用：布尔值；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Whether an instrument category matches the filter chip.
/// Inputs: `category`; `filter`.
/// Outputs / side effects: a boolean; no state change.
/// Complexity: O(1) time and space.
fn instrument_category_matches(category: &str, filter: &str) -> bool {
    if filter == "全部" {
        return true;
    }
    let category = category.to_lowercase();
    match filter {
        "键盘" => category.contains("键") || category.contains("钢琴"),
        "吉他" => category.contains("吉他") || category.contains("贝斯"),
        "弦乐" => category.contains("弦") || category.contains("提琴"),
        "管乐" => {
            category.contains("管")
                || category.contains("笛")
                || category.contains("萨克斯")
                || category.contains("铜")
                || category.contains("号")
        }
        "打击" => category.contains("鼓") || category.contains("打击"),
        "合成" => category.contains("合成") || category.contains("电子"),
        "民乐" => {
            category.contains("民乐")
                || category.contains("古")
                || category.contains("琵琶")
                || category.contains("二胡")
        }
        "其他" => {
            !category.contains("键")
                && !category.contains("钢琴")
                && !category.contains("吉他")
                && !category.contains("贝斯")
                && !category.contains("弦")
                && !category.contains("提琴")
                && !category.contains("管")
                && !category.contains("笛")
                && !category.contains("萨克斯")
                && !category.contains("铜")
                && !category.contains("号")
                && !category.contains("鼓")
                && !category.contains("打击")
                && !category.contains("合成")
                && !category.contains("电子")
                && !category.contains("民乐")
                && !category.contains("古")
                && !category.contains("琵琶")
                && !category.contains("二胡")
        }
        _ => true,
    }
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：按字符数截断列表文字并附加省略号，避免副标题越过卡片边界。
/// 输入：`text` 文本；`max` 最大字符数。
/// 输出/副作用：截断文本；不修改输入。
/// 复杂度：时间 O(字符数)，空间 O(字符数)。
///
/// Module: app / home / Agent library
/// Purpose: Truncate list text by characters so subtitles never overflow the card.
/// Inputs: `text`; `max`.
/// Outputs / side effects: truncated text; the input is unchanged.
/// Complexity: O(characters) time and space.
fn truncate_list_text(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut result: String = text.chars().take(max.saturating_sub(1)).collect();
    result.push('…');
    result
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：按类型与 id 生成编辑草稿。
/// 输入：`library` 资料库；`kind` 类型；`id` 条目 id。
/// 输出/副作用：草稿或 `None`；不修改资料库。
/// 复杂度：时间 O(条目数)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Build an edit draft for one entry.
/// Inputs: `library`; `kind`; `id`.
/// Outputs / side effects: the draft or `None`; the library is unchanged.
/// Complexity: O(entries) time and O(1) space.
fn draft_for_entry(library: &AiLibrary, kind: AiLibraryKind, id: &str) -> Option<AiLibraryDraft> {
    match kind {
        AiLibraryKind::Agent => library
            .agents
            .iter()
            .find(|agent| agent.id == id)
            .cloned()
            .map(|entry| AiLibraryDraft::Agent {
                original: Some(id.to_string()),
                entry,
            }),
        AiLibraryKind::Basic => library
            .basics
            .iter()
            .find(|skill| skill.id == id)
            .cloned()
            .map(|entry| AiLibraryDraft::Basic {
                original: Some(id.to_string()),
                entry,
            }),
        AiLibraryKind::Technique => library
            .techniques
            .iter()
            .find(|skill| skill.id == id)
            .cloned()
            .map(|entry| AiLibraryDraft::Technique {
                original: Some(id.to_string()),
                entry,
            }),
        AiLibraryKind::Instrument => library
            .instruments
            .iter()
            .find(|instrument| instrument.id == id)
            .cloned()
            .map(|entry| AiLibraryDraft::Instrument {
                original: Some(id.to_string()),
                entry,
            }),
    }
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：返回草稿对应的条目 id。
/// 输入：`draft` 草稿。
/// 输出/副作用：id 文本；不修改草稿。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Return the entry id of a draft.
/// Inputs: `draft`.
/// Outputs / side effects: id text; the draft is unchanged.
/// Complexity: O(1) time and space.
fn draft_id(draft: &AiLibraryDraft) -> Option<String> {
    match draft {
        AiLibraryDraft::Agent { entry, .. } => Some(entry.id.clone()),
        AiLibraryDraft::Basic { entry, .. } | AiLibraryDraft::Technique { entry, .. } => {
            Some(entry.id.clone())
        }
        AiLibraryDraft::Instrument { entry, .. } => Some(entry.id.clone()),
    }
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：更新技能条目：存在则替换，否则追加。
/// 输入：`items` 技能列表；`original` 原 id；`entry` 新条目。
/// 输出/副作用：原地修改列表；不修改其他状态。
/// 复杂度：时间 O(条目数)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Replace a skill entry or append it.
/// Inputs: `items`; `original`; `entry`.
/// Outputs / side effects: mutates the list in place.
/// Complexity: O(entries) time and O(1) space.
fn upsert_skill(items: &mut Vec<AiSkillDoc>, original: String, entry: AiSkillDoc) {
    if let Some(slot) = items.iter_mut().find(|skill| skill.id == original) {
        *slot = entry;
    } else {
        items.push(entry);
    }
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：按勾选状态加入或移除一个 id。
/// 输入：`ids` id 列表；`id` 目标；`checked` 是否选中。
/// 输出/副作用：原地修改列表；不修改其他状态。
/// 复杂度：时间 O(列表长度)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Add or remove an id according to a checkbox.
/// Inputs: `ids`; `id`; `checked`.
/// Outputs / side effects: mutates the list in place.
/// Complexity: O(list length) time and O(1) space.
fn toggle_id(ids: &mut Vec<String>, id: &str, checked: bool) {
    if checked {
        if !ids.iter().any(|entry| entry == id) {
            ids.push(id.to_string());
        }
    } else {
        ids.retain(|entry| entry != id);
    }
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：绘制一个带标签的单行输入框。
/// 输入：`ui`；`label` 标签；`value` 待编辑文本。
/// 输出/副作用：修改 `value`；不修改其他状态。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Draw one labeled single-line field.
/// Inputs: `ui`; `label`; `value`.
/// Outputs / side effects: edits `value`; nothing else changes.
/// Complexity: O(text length) time and O(1) space.
fn agent_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(label).size(10.5).color(theme::TEXT_DIM));
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(f32::INFINITY)
            .min_size(egui::vec2(0.0, 26.0)),
    );
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：绘制一个带标签的多行 Markdown 输入框。
/// 输入：`ui`；`label` 标签；`rows` 可见行数；`value` 待编辑文本。
/// 输出/副作用：修改 `value`；不修改其他状态。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Draw one labeled multiline Markdown field.
/// Inputs: `ui`; `label`; `rows`; `value`.
/// Outputs / side effects: edits `value`; nothing else changes.
/// Complexity: O(text length) time and O(1) space.
fn agent_body_field(ui: &mut egui::Ui, label: &str, rows: usize, value: &mut String) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(label).size(10.5).color(theme::TEXT_DIM));
    ui.add(
        egui::TextEdit::multiline(value)
            .desired_rows(rows)
            .desired_width(f32::INFINITY)
            .code_editor(),
    );
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：绘制小节标题。
/// 输入：`ui`；`title` 标题。
/// 输出/副作用：写入绘制命令；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Draw a section heading.
/// Inputs: `ui`; `title`.
/// Outputs / side effects: emits paint commands; no state change.
/// Complexity: O(1) time and space.
fn agent_section_title(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(title)
            .size(11.5)
            .strong()
            .color(theme::TEXT),
    );
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：绘制主按钮（强调色）。
/// 输入：`ui`；`label` 文本。
/// 输出/副作用：返回点击响应；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Draw a primary (accent) button.
/// Inputs: `ui`; `label`.
/// Outputs / side effects: the click response; no state change.
/// Complexity: O(1) time and space.
fn agent_primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .size(11.5)
                .color(egui::Color32::WHITE),
        )
        .fill(theme::ACCENT)
        .corner_radius(6)
        .min_size(egui::vec2(64.0, 26.0)),
    )
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：绘制编辑器中的开关行（名称 + 右侧开关）。
/// 输入：`ui`；`label` 名称；`value` 当前状态。
/// 输出/副作用：返回是否切换；切换时写入 `value`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Draw an editor toggle row (label plus right-aligned switch).
/// Inputs: `ui`; `label`; `value`.
/// Outputs / side effects: whether the value changed; mutates `value` on toggle.
/// Complexity: O(1) time and space.
fn editor_toggle_row(ui: &mut egui::Ui, label: &str, value: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 40.0).max(40.0);
        let response = ui.add_sized(
            egui::vec2(width, 22.0),
            egui::Label::new(egui::RichText::new(label).size(10.5)).sense(egui::Sense::click()),
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

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：绘制智能体默认加载乐器资料的选择器与已选标签。
/// 输入：`ui`；`entry` 智能体草稿；`library` 资料库。
/// 输出/副作用：修改 `entry.default_instrument_ids`；不修改资料库。
/// 复杂度：每帧 O(已选数)；弹出列表按可见行 O(可见行数)。
///
/// Module: app / home / Agent library
/// Purpose: Draw the default-instrument selector and selected chips for an agent.
/// Inputs: `ui`; `entry`; `library`.
/// Outputs / side effects: mutates `entry.default_instrument_ids`; the library is unchanged.
/// Complexity: O(selected) per frame; the popup renders O(visible rows).
fn default_instrument_selector(
    ui: &mut egui::Ui,
    entry: &mut crate::ai::AiAgentProfile,
    library: &AiLibrary,
) {
    let selected: Vec<&str> = entry
        .default_instrument_ids
        .iter()
        .filter_map(|id| {
            library
                .instrument(id)
                .map(|instrument| instrument.name.as_str())
        })
        .collect();
    if !selected.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for name in selected.iter().take(6) {
                egui::Frame::NONE
                    .fill(theme::PANEL_2)
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(*name).size(9.5).color(theme::TEXT_DIM));
                    });
            }
            if selected.len() > 6 {
                ui.label(
                    egui::RichText::new(format!("+{}", selected.len() - 6))
                        .size(9.5)
                        .color(theme::TEXT_FAINT),
                );
            }
        });
        ui.add_space(4.0);
    }
    let response = ui.add(
        egui::Button::new(
            egui::RichText::new(format!(
                "选择默认乐器…（{}）",
                entry.default_instrument_ids.len()
            ))
            .size(10.5),
        )
        .fill(theme::PANEL_2)
        .corner_radius(8)
        .min_size(egui::vec2(0.0, 26.0)),
    );
    egui::Popup::menu(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(300.0);
            ui.label(
                egui::RichText::new("默认加载的乐器资料")
                    .size(9.5)
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .max_height(340.0)
                .auto_shrink([false, false])
                .show_rows(ui, 24.0, library.instruments.len(), |ui, range| {
                    for index in range {
                        let Some(instrument) = library.instruments.get(index) else {
                            continue;
                        };
                        let mut checked = entry.default_instrument_ids.contains(&instrument.id);
                        if editor_toggle_row(ui, &instrument.name, &mut checked) {
                            toggle_id(&mut entry.default_instrument_ids, &instrument.id, checked);
                        }
                    }
                });
        });
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：绘制次要操作按钮（描边、无填充）。
/// 输入：`ui`；`label` 文本。
/// 输出/副作用：返回点击响应；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Draw a secondary outlined action button.
/// Inputs: `ui`; `label`.
/// Outputs / side effects: the click response; no state change.
/// Complexity: O(1) time and space.
fn agent_ghost_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(10.5))
            .fill(egui::Color32::TRANSPARENT)
            .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
            .corner_radius(6)
            .min_size(egui::vec2(56.0, 26.0)),
    )
}

/// 所属模块：app / 主页 / Agent 资料库
/// 功能：绘制危险操作按钮（红色文字）。
/// 输入：`ui`；`label` 文本。
/// 输出/副作用：返回点击响应；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home / Agent library
/// Purpose: Draw a danger action button with red text.
/// Inputs: `ui`; `label`.
/// Outputs / side effects: the click response; no state change.
/// Complexity: O(1) time and space.
fn agent_danger_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(10.5).color(theme::RED))
            .fill(egui::Color32::TRANSPARENT)
            .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
            .corner_radius(6)
            .min_size(egui::vec2(56.0, 26.0)),
    )
}
