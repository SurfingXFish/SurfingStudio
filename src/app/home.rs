//! 所属模块：app / 主页
//! 文件用途：
//! - 绘制主页：左侧为软件图标/名称/版本、项目/模板/插件/提示菜单与底部设置入口；右侧为具体内容。
//! - 项目页按最近保存时间列出全部打开过的工程，自动清理失效路径，并可下拉查看工程信息与历史版本。
//! - 插件页按品牌下拉分组（默认展开），乐器在上、效果器在下，行内显示类型标签并可下拉查看详情。
//! - 模板页与提示页保持原有内容。
//!
//! 函数清单：
//! - `SurfingApp::open_home` / `open_studio`：请求显示主页窗口 / 回到最近工程窗口。
//! - `SurfingApp::ui_home` / `ui_home_sidebar`：主页两栏布局与左侧菜单。
//! - `SurfingApp::ui_home_projects` / `ui_home_templates` / `ui_home_plugins` / `ui_home_tips`：右侧内容页。
//! - `SurfingApp::ui_home_project_row` / `ui_home_project_details`：项目行与展开的工程信息/历史版本。
//! - `SurfingApp::request_home_project_info` / `poll_home_project_info`：后台读取工程摘要。
//! - `SurfingApp::open_recent_project` / `request_project_from_template`：打开项目 / 从模板创建。
//! - `load_project_summary` / `format_bytes` / `format_version_name`：工程摘要与展示文本。
//! - `home_logo` / `home_nav_item` / `home_nav_icon` / `home_page_label`：侧栏控件。
//! - `ui_home_project_row` / `plugin_brand_header` / `home_plugin_row` / `paint_chevron` / `path_hash` / `project_display_name`：列表行与命名。
//! - `template_card` / `example_card` / `template_family_filter` / `template_filter_chip`：模板与范例卡片。
//!
//! Module: app / home page
//! Purpose:
//! - Draw the home page: the left column holds the logo/name/version, the projects/templates/plugins/
//!   tips menu and a settings entry at the bottom; the right column shows the selected content.
//! - The projects page lists every opened project by last-saved time, prunes missing paths and
//!   exposes project info plus history versions in a dropdown.
//! - The plugins page groups plugins by brand (expanded by default), instruments first and effects
//!   below, with type tags and per-plugin detail dropdowns.
//! - The templates and tips pages keep their previous content.
//!
//! Functions:
//! - `SurfingApp::open_home` / `open_studio`: request the home window / the most recent project window.
//! - `SurfingApp::ui_home` / `ui_home_sidebar`: two-column home layout and the left menu.
//! - `SurfingApp::ui_home_projects` / `ui_home_templates` / `ui_home_plugins` / `ui_home_tips`: content pages.
//! - `SurfingApp::ui_home_project_row` / `ui_home_project_details`: project row and its expanded info/versions.
//! - `SurfingApp::request_home_project_info` / `poll_home_project_info`: background project summary.
//! - `SurfingApp::open_recent_project` / `request_project_from_template`: open a project / create from a template.
//! - `load_project_summary` / `format_bytes` / `format_version_name`: project summary and display text.
//! - `home_logo` / `home_nav_item` / `home_nav_icon` / `home_page_label`: sidebar widgets.
//! - `home_project_row` / `plugin_brand_header` / `home_plugin_row` / `plugin_tag` / `path_hash` / `project_display_name`: rows and names.
//! - `template_card` / `example_card` / `template_family_filter` / `template_filter_chip`: template and example cards.

use super::panels::{icon_button, paint_gear};
use super::time_fmt;
use super::*;
use crate::core::model::template::{
    self, ExampleProject, MusicTemplate, TemplateFamily, TemplateId,
};
use crate::core::model::InstrumentRef;
use crate::core::project_runtime::ProjectRuntime;
use crate::plugin::{PluginKind, ScannedPlugin};
use std::path::Path;
use std::time::{Duration, SystemTime};

/// 主页左侧菜单栏宽度（逻辑点）。
/// Width of the home sidebar in logical points.
const SIDEBAR_W: f32 = 220.0;
/// 左侧菜单项高度（逻辑点）。
/// Height of one sidebar menu entry in logical points.
const NAV_H: f32 = 34.0;
/// 主页内容区左右内边距（逻辑点）。
/// Horizontal padding of the home content area in logical points.
const HOME_PAD: i8 = 24;
/// 项目行高度（逻辑点）。
/// Height of one project row in logical points.
const PROJECT_ROW_H: f32 = 52.0;
/// 插件品牌行高度（逻辑点）。
/// Height of one plugin brand header in logical points.
const PLUGIN_BRAND_H: f32 = 34.0;
/// 插件行高度（逻辑点）。
/// Height of one plugin row in logical points.
const PLUGIN_ROW_H: f32 = 40.0;
/// 插件行名称列宽（逻辑点）：固定宽度让右侧标签纵向对齐。
/// Width of the plugin name column in logical points; a fixed width keeps tags aligned.
const PLUGIN_NAME_W: f32 = 210.0;
/// 主页顶部标题字号（逻辑点）。
/// Home page heading size in logical points.
const HOME_TITLE_SIZE: f32 = 19.0;
/// 侧栏软件图标边长（逻辑点）。
/// Sidebar logo side length in logical points.
const HOME_LOGO_SIZE: f32 = 32.0;
/// 模板卡片最小宽度（逻辑点）。
/// Minimum template-card width in logical points.
const TEMPLATE_CARD_MIN_W: f32 = 258.0;
/// 模板卡片高度（逻辑点）。
/// Template-card height in logical points.
const TEMPLATE_CARD_H: f32 = 188.0;
/// 范例卡片高度（逻辑点）。
/// Example-card height in logical points.
const EXAMPLE_CARD_H: f32 = 112.0;
/// 模板/范例卡片间距（逻辑点）。
/// Template/example card gap in logical points.
const TEMPLATE_CARD_GAP: f32 = 12.0;
/// 模板网格最多列数，避免超宽窗口把卡片压得过窄。
/// Maximum template-grid columns so very wide windows do not squeeze the cards.
const TEMPLATE_MAX_COLUMNS: usize = 5;
/// 为竖向滚动条预留的宽度（逻辑点）。
/// Width reserved for the vertical scrollbar in logical points.
const TEMPLATE_SCROLL_RESERVE: f32 = 12.0;
/// 失效项目清理间隔：避免每帧对全部路径做存在性检查。
/// Prune interval for missing projects; avoids checking every path every frame.
const PROJECT_SWEEP_INTERVAL: Duration = Duration::from_secs(2);
/// 项目详情中最多列出的历史版本数量。
/// Maximum number of history versions listed in a project dropdown.
const PROJECT_VERSION_LIMIT: usize = 20;
/// 未标注厂商的插件品牌名。
/// Brand label used for plugins without a vendor.
const UNLABELED_VENDOR: &str = "未标注厂商";

/// 主页项目下拉返回的动作。
/// Action returned by one home project row.
enum ProjectRowAction {
    Open,
    Toggle,
    RefreshInfo,
}

/// 后台解析出的工程摘要。
/// Project summary parsed in the background.
#[derive(Clone, Debug)]
pub(super) struct HomeProjectSummary {
    pub bpm: f64,
    pub time_sig: (u8, u8),
    pub beats_per_bar: f64,
    pub tracks: usize,
    pub clips: usize,
    pub notes: usize,
    pub plugins: usize,
    pub length_beats: f64,
}

impl SurfingApp {
    /// 所属模块：app / 主页
    /// 功能：请求显示主页窗口（工程窗口保持打开且不暂停）。
    /// 输入：`self`。
    /// 输出/副作用：工程窗口追加 `ShowHome` 动作；主页窗口本身不做任何事。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / home page
    /// Purpose: Ask the shell to show the home window (the project window stays open and playing).
    /// Inputs: `self`.
    /// Outputs / side effects: a project window pushes a `ShowHome` action; the home window does
    /// nothing.
    /// Complexity: O(1) time and space.
    pub(super) fn open_home(&mut self) {
        if self.view == WorkspaceView::Home {
            return;
        }
        self.pending_window_actions.push(WindowAction::ShowHome);
        self.menu_bar_visible = false;
    }

    /// 所属模块：app / 主页
    /// 功能：请求回到最近使用的工程窗口。
    /// 输入：`self`。
    /// 输出/副作用：追加 `ResumeProject` 动作；不修改工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / home page
    /// Purpose: Ask the shell to return to the most recently used project window.
    /// Inputs: `self`.
    /// Outputs / side effects: pushes a `ResumeProject` action; the project is untouched.
    /// Complexity: O(1) time and space.
    pub(super) fn open_studio(&mut self) {
        self.pending_window_actions
            .push(WindowAction::ResumeProject);
    }

    /// 所属模块：app / 主页
    /// 功能：绘制主页两栏布局：左侧菜单栏固定在 220 点，右侧为当前页面内容。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：绘制界面；页面切换与命令由子方法处理。
    /// 复杂度：每帧 O(可见控件数)。
    ///
    /// Module: app / home page
    /// Purpose: Draw the two-column home layout: a fixed 220 pt sidebar and the current page.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: paints the UI; page switching and commands are handled by children.
    /// Complexity: O(visible widgets) per frame.
    pub(super) fn ui_home(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("home_sidebar")
            .exact_width(SIDEBAR_W)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::symmetric(12, 14)),
            )
            .show(ctx, |ui| self.ui_home_sidebar(ui));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::symmetric(HOME_PAD, 16)),
            )
            .show(ctx, |ui| {
                // 不支持插件宿主的平台（安卓）没有插件页：落回项目页。
                // Platforms without plugin hosting (Android) have no plugin page: fall back to
                // projects.
                let page = if self.home_page == HomePage::Plugins
                    && !crate::platform::caps().plugin_hosting
                {
                    HomePage::Projects
                } else {
                    self.home_page
                };
                match page {
                    HomePage::Projects => self.ui_home_projects(ui),
                    HomePage::Templates => self.ui_home_templates(ui),
                    HomePage::Plugins => self.ui_home_plugins(ui),
                    HomePage::Tips => self.ui_home_tips(ui),
                }
            });
    }

    /// 所属模块：app / 主页 / 左侧菜单
    /// 功能：绘制图标+名称+版本、分隔线、项目/模板/插件/提示菜单，并在最下方放置设置图标。
    /// 输入：`ui`：侧栏绘制上下文。
    /// 输出/副作用：切换 `home_page`；点击设置时打开独立设置窗口。
    /// 复杂度：每帧 O(菜单项数)。
    ///
    /// Module: app / home page / sidebar
    /// Purpose: Draw the logo + name + version, a divider, the projects/templates/plugins/tips menu
    /// and the settings icon at the bottom.
    /// Inputs: `ui`: sidebar paint context.
    /// Outputs / side effects: switches `home_page`; opens the settings window from the icon.
    /// Complexity: O(menu items) per frame.
    fn ui_home_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            home_logo(ui, HOME_LOGO_SIZE);
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.add_space(1.0);
                ui.label(
                    egui::RichText::new("SurfingStudio")
                        .size(13.5)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.label(
                    egui::RichText::new(crate::version_label())
                        .size(10.0)
                        .color(theme::TEXT_FAINT),
                );
            });
        });
        ui.add_space(12.0);
        let line = ui.available_rect_before_wrap();
        ui.painter().hline(
            line.left()..=line.right(),
            line.top(),
            egui::Stroke::new(1.0_f32, theme::LINE),
        );
        ui.add_space(10.0);

        for page in [
            HomePage::Projects,
            HomePage::Templates,
            HomePage::Plugins,
            HomePage::Tips,
        ] {
            // 不支持插件宿主的平台（安卓）不显示插件页。
            // Platforms without plugin hosting (Android) hide the plugin page.
            if page == HomePage::Plugins && !crate::platform::caps().plugin_hosting {
                continue;
            }
            if home_nav_item(ui, page, self.home_page == page).clicked() {
                self.home_page = page;
            }
            ui.add_space(2.0);
        }

        // 菜单下方留白，把设置图标压到侧栏最下方。
        // Leave the gap below the menu so the settings icon sits at the bottom.
        let reserved = 40.0;
        let remaining = ui.available_height() - reserved;
        if remaining > 0.0 {
            ui.add_space(remaining);
        }
        if icon_button(ui, "设置", paint_gear).clicked() {
            self.open_settings(ui.ctx());
        }
    }

    /// 所属模块：app / 主页 / 项目页
    /// 功能：绘制搜索栏、新建/打开/继续按钮与全部项目列表；按最近保存时间排序，并清理失效路径。
    /// 输入：`ui`：右侧内容区。
    /// 输出/副作用：修改搜索文本与工程状态；可能启动新建、打开对话框、加载项目或清理列表。
    /// 复杂度：每帧 O(可见项目数 + 清理时 O(项目数))。
    ///
    /// Module: app / home page / projects
    /// Purpose: Draw the search field, new/open/continue actions and every project, sorted by last
    /// saved time, pruning paths that no longer exist.
    /// Inputs: `ui`: right-hand content area.
    /// Outputs / side effects: edits the search text and project state; may create/open a project,
    /// load one or prune the list.
    /// Complexity: O(visible projects) per frame, O(projects) while pruning.
    fn ui_home_projects(&mut self, ui: &mut egui::Ui) {
        self.poll_home_project_info();
        self.prune_missing_projects();
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("项目")
                .size(HOME_TITLE_SIZE)
                .strong()
                .color(theme::TEXT),
        );
        ui.label(
            egui::RichText::new(
                "按最近保存时间列出打开过的工程，点击展开可查看工程信息与历史版本。",
            )
            .size(10.5)
            .color(theme::TEXT_DIM),
        );
        if self.open_project_count > 0 {
            ui.label(
                egui::RichText::new(format!(
                    "当前已打开 {} 个工程窗口，点击图标或“文件 → 主页”可随时返回",
                    self.open_project_count
                ))
                .size(10.0)
                .color(theme::TEXT_FAINT),
            );
        }
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            // 先放搜索框（占据按钮左侧空间），按钮组再按右对齐布局，避免 RTL 影响输入框文本。
            // Place the search field first (it takes the space left of the buttons); the button
            // group is then right-aligned so the RTL layout cannot affect the text input.
            let buttons_width = 306.0;
            let search_width = (ui.available_width() - buttons_width - 12.0).max(160.0);
            ui.add_sized(
                [search_width, 28.0],
                egui::TextEdit::singleline(&mut self.home_search)
                    .hint_text("搜索项目")
                    .desired_width(search_width),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let new_project = egui::Button::new(
                    egui::RichText::new("新建项目")
                        .size(12.0)
                        .color(egui::Color32::WHITE),
                )
                .fill(theme::ACCENT)
                .min_size(egui::vec2(84.0, 28.0));
                if ui
                    .add(new_project)
                    .on_hover_text(self.settings.shortcut_label(ShortcutAction::New))
                    .clicked()
                {
                    self.request_new_project();
                }
                if ui
                    .add(egui::Button::new("打开项目").min_size(egui::vec2(84.0, 28.0)))
                    .on_hover_text(self.settings.shortcut_label(ShortcutAction::Open))
                    .clicked()
                {
                    self.open_project_dialog();
                }
                if ui
                    .add_enabled(
                        self.can_resume_project,
                        egui::Button::new(
                            egui::RichText::new("继续当前工程")
                                .size(11.0)
                                .color(theme::TEXT_DIM),
                        ),
                    )
                    .on_hover_text(if self.can_resume_project {
                        "返回最近使用的工程窗口"
                    } else {
                        "当前没有打开的工程窗口"
                    })
                    .clicked()
                {
                    self.open_studio();
                }
            });
        });
        ui.add_space(10.0);

        let filter = self.home_search.trim().to_lowercase();
        let mut entries: Vec<(PathBuf, SystemTime)> = self
            .settings
            .recent_projects
            .iter()
            .filter(|path| {
                if filter.is_empty() {
                    return true;
                }
                let name = project_display_name(path).to_lowercase();
                name.contains(&filter) || path.to_string_lossy().to_lowercase().contains(&filter)
            })
            .map(|path| {
                let modified = std::fs::metadata(path)
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                (path.clone(), modified)
            })
            .collect();
        // 按最近保存时间从新到旧排序；同一时间按名称稳定排序。
        // Sort newest save time first; ties keep a stable name order.
        entries.sort_by(|left, right| {
            right
                .1
                .cmp(&left.1)
                .then_with(|| project_display_name(&left.0).cmp(&project_display_name(&right.0)))
        });

        if entries.is_empty() {
            ui.add_space(50.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(if self.settings.recent_projects.is_empty() {
                        "暂无项目"
                    } else {
                        "没有匹配的项目"
                    })
                    .size(13.0)
                    .color(theme::TEXT_DIM),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("打开或新建工程后会自动出现在这里。")
                        .size(10.5)
                        .color(theme::TEXT_FAINT),
                );
            });
            return;
        }

        let mut open: Option<PathBuf> = None;
        let mut toggle: Option<PathBuf> = None;
        let mut refresh: Option<PathBuf> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (path, modified) in &entries {
                    let expanded = self.home_project_expanded.contains(path);
                    match self.ui_home_project_row(ui, path, *modified, expanded) {
                        Some(ProjectRowAction::Open) => open = Some(path.clone()),
                        Some(ProjectRowAction::Toggle) => toggle = Some(path.clone()),
                        Some(ProjectRowAction::RefreshInfo) => refresh = Some(path.clone()),
                        None => {}
                    }
                }
            });
        if let Some(path) = toggle {
            if !self.home_project_expanded.remove(&path) {
                self.home_project_expanded.insert(path);
            }
        }
        if let Some(path) = refresh {
            self.home_project_info.remove(&path);
            self.request_home_project_info(path);
        }
        if let Some(path) = open {
            self.open_recent_project(path);
        }
    }

    /// 所属模块：app / 主页 / 项目页
    /// 功能：按固定间隔清理已不存在的工程路径，并同步清理展开状态与摘要缓存。
    /// 输入：`self`。
    /// 输出/副作用：可能移除 `recent_projects` 条目并保存设置。
    /// 复杂度：时间 O(项目数)，空间 O(1)。
    ///
    /// Module: app / home page / projects
    /// Purpose: Prune project paths that no longer exist on a fixed interval and clear their
    /// expanded state and summary cache.
    /// Inputs: `self`.
    /// Outputs / side effects: may remove `recent_projects` entries and save the settings.
    /// Complexity: O(projects) time and O(1) space.
    fn prune_missing_projects(&mut self) {
        if self.home_project_sweep_at.elapsed() < PROJECT_SWEEP_INTERVAL {
            return;
        }
        self.home_project_sweep_at = Instant::now();
        let before = self.settings.recent_projects.len();
        self.settings.recent_projects.retain(|path| path.is_file());
        if self.settings.recent_projects.len() == before {
            return;
        }
        self.home_project_expanded.retain(|path| path.is_file());
        self.home_project_info.retain(|path, _| path.is_file());
        self.commit_settings();
    }

    /// 所属模块：app / 主页 / 项目页
    /// 功能：绘制一行项目（图标、名称、路径、最近保存时间与展开箭头），展开时追加详情区。
    /// 输入：
    /// - `ui`：项目列表。
    /// - `path`：项目路径。
    /// - `modified`：工程文件最近保存时间。
    /// - `expanded`：是否展开详情。
    /// 输出/副作用：返回点击动作；展开时可能请求后台摘要。
    /// 复杂度：时间 O(路径长度 + 版本数)，空间 O(路径长度)。
    ///
    /// Module: app / home page / projects
    /// Purpose: Draw one project row (icon, name, path, last-saved time and expand chevron) and the
    /// detail area when expanded.
    /// Inputs:
    /// - `ui`: project list.
    /// - `path`: project path.
    /// - `modified`: file modification time.
    /// - `expanded`: whether the detail panel is open.
    /// Outputs / side effects: the click action; may request the background summary when expanded.
    /// Complexity: O(path length + versions) time and O(path length) space.
    fn ui_home_project_row(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        modified: SystemTime,
        expanded: bool,
    ) -> Option<ProjectRowAction> {
        let width = ui.available_width();
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, PROJECT_ROW_H), egui::Sense::click());
        let hovered = response.hovered();
        let chevron_rect = egui::Rect::from_min_size(
            egui::pos2(rect.right() - 30.0, rect.center().y - 10.0),
            egui::vec2(22.0, 20.0),
        );
        let mut action = None;
        if response.clicked() {
            let pointer = response.interact_pointer_pos().unwrap_or(rect.center());
            action = Some(if chevron_rect.contains(pointer) {
                ProjectRowAction::Toggle
            } else {
                ProjectRowAction::Open
            });
        }
        let painter = ui.painter();
        if hovered {
            painter.rect_filled(rect, egui::CornerRadius::from(4), theme::PANEL_2);
        }
        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 6.0, rect.center().y - 12.0),
            egui::vec2(24.0, 24.0),
        );
        let color = theme::palette_color(path_hash(path));
        painter.rect_filled(icon_rect, egui::CornerRadius::from(4), color);
        let name = project_display_name(path);
        let initial = name
            .chars()
            .next()
            .map(|character| character.to_uppercase().to_string())
            .unwrap_or_else(|| "S".into());
        painter.text(
            icon_rect.center(),
            egui::Align2::CENTER_CENTER,
            initial,
            egui::FontId::proportional(11.0),
            egui::Color32::WHITE,
        );

        let text_x = icon_rect.right() + 10.0;
        let max_width = (rect.right() - text_x - 110.0).max(40.0);
        let name_galley =
            egui::WidgetText::from(egui::RichText::new(&name).size(12.5).color(theme::TEXT))
                .into_galley(
                    ui,
                    Some(egui::TextWrapMode::Truncate),
                    max_width,
                    egui::TextStyle::Body,
                );
        painter.galley(
            egui::pos2(text_x, rect.center().y - 15.0),
            name_galley,
            theme::TEXT,
        );
        let path_text = path.to_string_lossy().to_string();
        let path_galley = egui::WidgetText::from(
            egui::RichText::new(path_text)
                .size(10.5)
                .color(theme::TEXT_FAINT),
        )
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            max_width,
            egui::TextStyle::Small,
        );
        painter.galley(
            egui::pos2(text_x, rect.center().y + 1.0),
            path_galley,
            theme::TEXT_FAINT,
        );
        painter.text(
            egui::pos2(rect.right() - 38.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            time_fmt::format_relative(modified),
            egui::FontId::proportional(10.0),
            theme::TEXT_FAINT,
        );
        paint_chevron(
            painter,
            chevron_rect.center(),
            expanded,
            if hovered {
                theme::TEXT
            } else {
                theme::TEXT_DIM
            },
        );
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(format!(
                "{}\n最近保存：{}\n点击展开工程信息与历史版本",
                path.display(),
                time_fmt::format_datetime(modified)
            ));
        if expanded {
            self.ui_home_project_details(ui, path, modified, &mut action);
        }
        action
    }

    /// 所属模块：app / 主页 / 项目页
    /// 功能：绘制展开后的工程信息：文件事实、解析摘要、历史版本列表与操作按钮。
    /// 输入：
    /// - `ui`：项目列表。
    /// - `path`：项目路径。
    /// - `modified`：文件最近保存时间。
    /// - `action`：本行的点击动作（打开 / 刷新）。
    /// 输出/副作用：无摘要时启动后台加载；按钮写入 `action`。
    /// 复杂度：时间 O(版本数 + 文本长度)，空间 O(版本数)。
    ///
    /// Module: app / home page / projects
    /// Purpose: Draw the expanded project info: file facts, parsed summary, history versions and
    /// action buttons.
    /// Inputs:
    /// - `ui`: project list.
    /// - `path`: project path.
    /// - `modified`: file modification time.
    /// - `action`: click action of this row (open / refresh).
    /// Outputs / side effects: starts a background load when no summary exists; buttons write `action`.
    /// Complexity: O(versions + text length) time and O(versions) space.
    fn ui_home_project_details(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        modified: SystemTime,
        action: &mut Option<ProjectRowAction>,
    ) {
        if !self.home_project_info.contains_key(path) {
            self.request_home_project_info(path.to_path_buf());
        }
        let metadata = std::fs::metadata(path).ok();
        let bytes = metadata
            .as_ref()
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let saved_at = metadata
            .and_then(|metadata| metadata.modified().ok())
            .unwrap_or(modified);
        let versions = super::backup::backup_versions(path);
        let summary = self.home_project_info.get(path).cloned();
        let mut open_version: Option<PathBuf> = None;
        egui::Frame::NONE
            .fill(theme::PANEL)
            .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
            .corner_radius(5)
            .inner_margin(egui::Margin::symmetric(12, 9))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("工程信息")
                            .size(11.5)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("打开工程")
                            .on_hover_text("在工程窗口中打开")
                            .clicked()
                        {
                            *action = Some(ProjectRowAction::Open);
                        }
                        if ui
                            .small_button("刷新信息")
                            .on_hover_text("重新读取工程文件")
                            .clicked()
                        {
                            *action = Some(ProjectRowAction::RefreshInfo);
                        }
                    });
                });
                ui.add_space(4.0);
                project_detail_row(ui, "路径", &path.display().to_string());
                project_detail_row(ui, "大小", &format_bytes(bytes));
                project_detail_row(
                    ui,
                    "最近保存",
                    &format!(
                        "{}（{}）",
                        time_fmt::format_datetime(saved_at),
                        time_fmt::format_relative(saved_at)
                    ),
                );
                match summary {
                    Some(Ok(summary)) => {
                        project_detail_row(
                            ui,
                            "工程",
                            &format!(
                                "{} BPM · {}/{} · {} 小节",
                                summary.bpm.round() as i32,
                                summary.time_sig.0,
                                summary.time_sig.1,
                                (summary.length_beats / summary.beats_per_bar).ceil() as i32
                            ),
                        );
                        project_detail_row(
                            ui,
                            "内容",
                            &format!(
                                "{} 轨 · {} 片段 · {} 音符 · {} 插件",
                                summary.tracks, summary.clips, summary.notes, summary.plugins
                            ),
                        );
                    }
                    Some(Err(error)) => {
                        project_detail_row(ui, "读取失败", &error);
                    }
                    None => {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new().size(12.0));
                            ui.label(
                                egui::RichText::new("正在读取工程信息…")
                                    .size(10.5)
                                    .color(theme::TEXT_DIM),
                            );
                        });
                    }
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(5.0);
                ui.label(
                    egui::RichText::new(format!("历史版本（{}）", versions.len()))
                        .size(11.0)
                        .strong()
                        .color(theme::TEXT),
                );
                if versions.is_empty() {
                    ui.label(
                        egui::RichText::new(if self.settings.auto_backup {
                            "保存后会自动在工程目录的 Backups 文件夹保留版本。"
                        } else {
                            "自动备份已关闭，可在设置 → 存储中开启后保存一次。"
                        })
                        .size(10.0)
                        .color(theme::TEXT_FAINT),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("版本按保存时间从新到旧排列，可直接打开只读查看。")
                            .size(9.5)
                            .color(theme::TEXT_FAINT),
                    );
                    for version in versions.iter().take(PROJECT_VERSION_LIMIT) {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format_version_name(&version.path))
                                    .size(10.5)
                                    .monospace()
                                    .color(theme::TEXT_DIM),
                            );
                            ui.label(
                                egui::RichText::new(time_fmt::format_datetime(version.saved_at))
                                    .size(9.5)
                                    .color(theme::TEXT_FAINT),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .small_button("打开")
                                        .on_hover_text("打开该历史版本（另存为不会覆盖原工程）")
                                        .clicked()
                                    {
                                        open_version = Some(version.path.clone());
                                    }
                                    ui.label(
                                        egui::RichText::new(format_bytes(version.bytes))
                                            .size(9.5)
                                            .color(theme::TEXT_FAINT),
                                    );
                                },
                            );
                        });
                    }
                }
            });
        if let Some(version) = open_version {
            // 历史版本走普通打开流程：工程运行时按扩展名分派，不会改写原工程。
            // Open the version through the normal flow; the runtime dispatches by extension.
            self.pending_window_actions
                .push(WindowAction::OpenProject { path: version });
        }
        ui.add_space(6.0);
    }

    /// 所属模块：app / 主页 / 项目加载
    /// 功能：在后台读取工程摘要（BPM、拍号、轨道/片段/音符数与插件数）。
    /// 输入：`path` 工程路径。
    /// 输出/副作用：设置 `home_project_info_rx`；已有加载任务时忽略。
    /// 复杂度：时间 O(1)（解析在后台），空间 O(1)。
    ///
    /// Module: app / home page / project loading
    /// Purpose: Read a project summary (BPM, meter, track/clip/note counts and plugin count) in the
    /// background.
    /// Inputs: `path` — project path.
    /// Outputs / side effects: sets `home_project_info_rx`; ignored while another load is running.
    /// Complexity: O(1) time (parsing runs in the background) and O(1) space.
    fn request_home_project_info(&mut self, path: PathBuf) {
        if self.home_project_info_rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let thread_path = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(load_project_summary(&thread_path));
        });
        self.home_project_info_rx = Some((path, rx));
    }

    /// 所属模块：app / 主页 / 项目加载
    /// 功能：非阻塞接收后台工程摘要并写入缓存。
    /// 输入：`self`。
    /// 输出/副作用：可能写入 `home_project_info` 并清空 `home_project_info_rx`。
    /// 复杂度：时间 O(1)，空间 O(摘要大小)。
    ///
    /// Module: app / home page / project loading
    /// Purpose: Non-blocking receive of the background project summary and cache it.
    /// Inputs: `self`.
    /// Outputs / side effects: may fill `home_project_info` and clear `home_project_info_rx`.
    /// Complexity: O(1) time and O(summary) space.
    fn poll_home_project_info(&mut self) {
        let Some((path, receiver)) = &self.home_project_info_rx else {
            return;
        };
        let outcome = match receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("工程信息读取线程中断".to_string())
            }
        };
        let path = path.clone();
        self.home_project_info_rx = None;
        self.home_project_info.insert(path, outcome);
    }

    /// 所属模块：app / 主页 / 模板页
    /// 功能：绘制主流音乐风格模板与范例工程卡片，支持搜索与家族筛选，单击即创建新工程窗口。
    /// 输入：`ui`：右侧内容区。
    /// 输出/副作用：修改模板搜索文本与家族筛选；可能追加 `NewProject` 窗口动作。
    /// 复杂度：每帧 O(模板数 + 范例数)。
    ///
    /// Module: app / home page / templates
    /// Purpose: Draw mainstream-style template and example cards with search and family filters;
    /// clicking a card creates a new project window.
    /// Inputs: `ui`: right-hand content area.
    /// Outputs / side effects: edits the template search text and family filter; may push a
    /// `NewProject` window action.
    /// Complexity: O(templates + examples) per frame.
    fn ui_home_templates(&mut self, ui: &mut egui::Ui) {
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("模板工程")
                .size(HOME_TITLE_SIZE)
                .strong()
                .color(theme::TEXT),
        );
        ui.label(
            egui::RichText::new("从主流音乐风格模板快速开始，或打开更完整的范例工程继续修改。")
                .size(10.5)
                .color(theme::TEXT_DIM),
        );
        ui.add_space(12.0);

        let search_width = ui.available_width().max(160.0);
        ui.add_sized(
            [search_width, 28.0],
            egui::TextEdit::singleline(&mut self.home_template_search)
                .hint_text("搜索风格、标签或音色")
                .desired_width(search_width),
        );
        ui.add_space(8.0);
        template_family_filter(ui, &mut self.home_template_family);
        ui.add_space(10.0);

        let filter = self.home_template_search.trim().to_lowercase();
        let family = self.home_template_family;
        let filtered: Vec<&MusicTemplate> = template::templates()
            .iter()
            .filter(|item| family.is_none_or(|family| item.family == family))
            .filter(|item| {
                filter.is_empty()
                    || item.name.to_lowercase().contains(&filter)
                    || item.style.to_lowercase().contains(&filter)
                    || item.description.to_lowercase().contains(&filter)
                    || item
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&filter))
            })
            .collect();

        let mut create: Option<TemplateId> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if filtered.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("没有匹配的模板")
                                .size(13.0)
                                .color(theme::TEXT_DIM),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("换一个风格关键词，或选择“全部”家族。")
                                .size(10.5)
                                .color(theme::TEXT_FAINT),
                        );
                    });
                } else {
                    let width = (ui.available_width() - TEMPLATE_SCROLL_RESERVE).max(180.0);
                    let columns = (((width + TEMPLATE_CARD_GAP)
                        / (TEMPLATE_CARD_MIN_W + TEMPLATE_CARD_GAP))
                        .floor() as usize)
                        .clamp(1, TEMPLATE_MAX_COLUMNS);
                    let card_width = ((width - TEMPLATE_CARD_GAP * (columns - 1) as f32)
                        / columns as f32)
                        .max(180.0);
                    for row in filtered.chunks(columns) {
                        ui.horizontal(|ui| {
                            // 卡片间距由 add_space 显式控制，关闭默认 item_spacing 以免行宽溢出。
                            // Gaps are added explicitly; disable the default item spacing to keep
                            // the row within the viewport.
                            ui.spacing_mut().item_spacing.x = 0.0;
                            for (index, item) in row.iter().enumerate() {
                                if template_card(ui, item, card_width).clicked() {
                                    create = Some(item.id);
                                }
                                if index + 1 < row.len() {
                                    ui.add_space(TEMPLATE_CARD_GAP);
                                }
                            }
                        });
                        ui.add_space(TEMPLATE_CARD_GAP);
                    }
                }

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("范例工程")
                        .size(15.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.label(
                    egui::RichText::new("16 小节的完整编曲，可直接打开继续修改。")
                        .size(10.5)
                        .color(theme::TEXT_DIM),
                );
                ui.add_space(8.0);
                let width = (ui.available_width() - TEMPLATE_SCROLL_RESERVE).max(180.0);
                let columns = (((width + TEMPLATE_CARD_GAP)
                    / (TEMPLATE_CARD_MIN_W + TEMPLATE_CARD_GAP))
                    .floor() as usize)
                    .clamp(1, TEMPLATE_MAX_COLUMNS);
                let card_width = ((width - TEMPLATE_CARD_GAP * (columns - 1) as f32)
                    / columns as f32)
                    .max(180.0);
                for row in template::example_projects().chunks(columns) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for (index, example) in row.iter().enumerate() {
                            if example_card(ui, example, card_width).clicked() {
                                create = Some(example.id);
                            }
                            if index + 1 < row.len() {
                                ui.add_space(TEMPLATE_CARD_GAP);
                            }
                        }
                    });
                    ui.add_space(TEMPLATE_CARD_GAP);
                }
            });

        if let Some(template) = create {
            self.request_project_from_template(template);
        }
    }

    /// 所属模块：app / 主页 / 模板页
    /// 功能：请求外壳从模板/范例创建新工程窗口。
    /// 输入：`template` 模板或范例标识。
    /// 输出/副作用：追加 `NewProject` 动作并更新状态栏。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / home page / templates
    /// Purpose: Ask the shell to create a new project window from a template/example.
    /// Inputs: `template` template or example identifier.
    /// Outputs / side effects: pushes a `NewProject` action and updates the status bar.
    /// Complexity: O(1) time and space.
    pub(super) fn request_project_from_template(&mut self, template: TemplateId) {
        self.pending_window_actions.push(WindowAction::NewProject {
            template: Some(template),
        });
        let name = crate::core::model::template::display_name(template).unwrap_or("模板工程");
        self.status = format!("正在从「{name}」创建工程…");
    }

    /// 所属模块：app / 主页 / 插件页
    /// 功能：绘制插件搜索栏、扫描按钮与按品牌分组的插件列表；乐器在上、效果器在下。
    /// 输入：`ui`：右侧内容区。
    /// 输出/副作用：修改搜索文本与展开状态；可能启动插件扫描。
    /// 复杂度：每帧 O(可见插件数 + 品牌数)。
    ///
    /// Module: app / home page / plugins
    /// Purpose: Draw the plugin search field, scan actions and the brand-grouped plugin list with
    /// instruments first and effects below.
    /// Inputs: `ui`: right-hand content area.
    /// Outputs / side effects: edits the search text and expansion state; may start a plugin scan.
    /// Complexity: O(visible plugins + brands) per frame.
    fn ui_home_plugins(&mut self, ui: &mut egui::Ui) {
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("插件")
                .size(HOME_TITLE_SIZE)
                .strong()
                .color(theme::TEXT),
        );
        ui.label(
            egui::RichText::new(
                "按品牌查看已扫描的插件：乐器在上、效果器在下，点击“详情”查看路径与格式。",
            )
            .size(10.5)
            .color(theme::TEXT_DIM),
        );
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            // 搜索框在左、扫描按钮在右，与项目页保持一致。
            // The search field sits on the left and the scan buttons on the right, matching the
            // projects page.
            let buttons_width = 300.0;
            let search_width = (ui.available_width() - buttons_width - 12.0).max(160.0);
            ui.add_sized(
                [search_width, 28.0],
                egui::TextEdit::singleline(&mut self.home_plugin_search)
                    .hint_text("搜索插件、品牌或类型")
                    .desired_width(search_width),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!self.scanning, egui::Button::new("扫描全部"))
                    .clicked()
                {
                    self.start_scan();
                }
                if ui
                    .add_enabled(!self.scanning, egui::Button::new("仅 VST3"))
                    .clicked()
                {
                    self.scan_vst3_only();
                }
                if ui
                    .add_enabled(!self.scanning, egui::Button::new("仅 VST2 / VST"))
                    .clicked()
                {
                    self.scan_legacy_only();
                }
                if self.scanning {
                    ui.add(egui::Spinner::new().size(14.0));
                }
            });
        });
        ui.add_space(8.0);

        let filter = self.home_plugin_search.trim().to_lowercase();
        let plugins: Vec<ScannedPlugin> = self
            .plugin_cache
            .iter()
            .filter(|plugin| {
                if filter.is_empty() {
                    return true;
                }
                plugin.name.to_lowercase().contains(&filter)
                    || plugin.vendor.to_lowercase().contains(&filter)
                    || plugin.category.to_lowercase().contains(&filter)
            })
            .cloned()
            .collect();
        let instruments = plugins
            .iter()
            .filter(|plugin| plugin.kind == PluginKind::Instrument)
            .count();
        let effects = plugins
            .iter()
            .filter(|plugin| plugin.kind == PluginKind::Effect)
            .count();

        ui.label(
            egui::RichText::new(format!(
                "共 {} 个插件：乐器 {instruments} · 效果器 {effects} · 未知 {}",
                plugins.len(),
                plugins.len().saturating_sub(instruments + effects)
            ))
            .size(10.0)
            .color(theme::TEXT_FAINT),
        );
        ui.add_space(4.0);

        if plugins.is_empty() {
            ui.add_space(50.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(if self.scanning {
                        "正在扫描插件…"
                    } else {
                        "还没有插件记录"
                    })
                    .size(13.0)
                    .color(theme::TEXT_DIM),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("点击右上角按钮扫描默认插件目录。")
                        .size(10.5)
                        .color(theme::TEXT_FAINT),
                );
            });
            return;
        }

        // 按品牌分组：品牌内再分乐器、效果器与未知，满足“乐器在上、效果器在下”。
        // Group by brand, then split into instruments, effects and unknown so instruments stay on top.
        struct BrandGroup {
            brand: String,
            instruments: Vec<ScannedPlugin>,
            effects: Vec<ScannedPlugin>,
            unknown: Vec<ScannedPlugin>,
        }
        let mut groups: Vec<BrandGroup> = Vec::new();
        for plugin in plugins {
            let brand = if plugin.vendor.trim().is_empty() {
                UNLABELED_VENDOR.to_string()
            } else {
                plugin.vendor.trim().to_string()
            };
            let index = groups
                .iter()
                .position(|group| group.brand.eq_ignore_ascii_case(&brand))
                .unwrap_or_else(|| {
                    groups.push(BrandGroup {
                        brand,
                        instruments: Vec::new(),
                        effects: Vec::new(),
                        unknown: Vec::new(),
                    });
                    groups.len() - 1
                });
            match plugin.kind {
                PluginKind::Instrument => groups[index].instruments.push(plugin),
                PluginKind::Effect => groups[index].effects.push(plugin),
                PluginKind::Unknown => groups[index].unknown.push(plugin),
            }
        }
        for group in &mut groups {
            group
                .instruments
                .sort_by_key(|plugin| plugin.name.to_lowercase());
            group
                .effects
                .sort_by_key(|plugin| plugin.name.to_lowercase());
            group
                .unknown
                .sort_by_key(|plugin| plugin.name.to_lowercase());
        }
        groups.sort_by_key(|group| group.brand.to_lowercase());

        let mut toggle_brand: Option<String> = None;
        let mut toggle_plugin: Vec<PathBuf> = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for group in &groups {
                    let collapsed = self.home_plugin_collapsed.contains(&group.brand);
                    if plugin_brand_header(
                        ui,
                        &group.brand,
                        group.instruments.len(),
                        group.effects.len(),
                        group.unknown.len(),
                        collapsed,
                    )
                    .clicked()
                    {
                        toggle_brand = Some(group.brand.clone());
                    }
                    if collapsed {
                        continue;
                    }
                    for (label, entries) in [
                        ("乐器", &group.instruments),
                        ("效果器", &group.effects),
                        ("未知", &group.unknown),
                    ] {
                        if entries.is_empty() {
                            continue;
                        }
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(format!("{label} · {}", entries.len()))
                                .size(10.5)
                                .strong()
                                .color(if label == "乐器" {
                                    theme::ACCENT
                                } else {
                                    theme::TEXT_DIM
                                }),
                        );
                        for plugin in entries {
                            let expanded = self.home_plugin_expanded.contains(&plugin.path);
                            if home_plugin_row(ui, plugin, expanded) {
                                toggle_plugin.push(plugin.path.clone());
                            }
                        }
                    }
                    ui.add_space(8.0);
                }
            });
        if let Some(brand) = toggle_brand {
            if !self.home_plugin_collapsed.remove(&brand) {
                self.home_plugin_collapsed.insert(brand);
            }
        }
        for path in toggle_plugin {
            if !self.home_plugin_expanded.remove(&path) {
                self.home_plugin_expanded.insert(path);
            }
        }
    }

    /// 所属模块：app / 主页 / 提示页
    /// 功能：绘制常用操作与快捷键提示卡片。
    /// 输入：`ui`：右侧内容区。
    /// 输出/副作用：只读静态文案；不修改状态。
    /// 复杂度：每帧 O(提示条数)。
    ///
    /// Module: app / home page / tips
    /// Purpose: Draw cards with common operations and shortcuts.
    /// Inputs: `ui`: right-hand content area.
    /// Outputs / side effects: paints static text only; no state change.
    /// Complexity: O(tips) per frame.
    fn ui_home_tips(&mut self, ui: &mut egui::Ui) {
        let tips: [(String, &str); 6] = [
            (
                self.settings.shortcut_label(ShortcutAction::PlayPause),
                "播放 / 暂停。焦点在插件窗口或浮动卷帘时同样有效。",
            ),
            (
                format!(
                    "{} / {} / {}",
                    self.settings.shortcut_label(ShortcutAction::Save),
                    self.settings.shortcut_label(ShortcutAction::Open),
                    self.settings.shortcut_label(ShortcutAction::New)
                ),
                "保存、打开与新建工程。",
            ),
            (
                self.settings.shortcut_label(ShortcutAction::PluginBrowser),
                "打开插件浏览器，管理 VST3 / VST2 插件。",
            ),
            (
                format!(
                    "{} / {}",
                    self.settings.shortcut_label(ShortcutAction::ControllerRoll),
                    self.settings.shortcut_label(ShortcutAction::EffectsRoll)
                ),
                "打开当前轨道的控制器卷帘 / 效果器链卷帘。",
            ),
            (
                "双击片段或子轨".to_string(),
                "进入钢琴卷帘、控制器或效果器编辑；单击只选中。",
            ),
            (
                "拖入 WAV".to_string(),
                "在轨道上创建音频片段；拖到空白处会自动新建音频轨道。",
            ),
        ];
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("提示")
                .size(HOME_TITLE_SIZE)
                .strong()
                .color(theme::TEXT),
        );
        ui.label(
            egui::RichText::new(format!(
                "常用操作与快捷键，完整说明见帮助窗口（{}）。",
                self.settings.shortcut_label(ShortcutAction::Help)
            ))
            .size(10.5)
            .color(theme::TEXT_DIM),
        );
        ui.add_space(12.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (title, description) in &tips {
                    egui::Frame::NONE
                        .fill(theme::PANEL_2)
                        .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                        .corner_radius(6)
                        .inner_margin(egui::Margin::symmetric(12, 9))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                egui::RichText::new(title)
                                    .size(12.0)
                                    .strong()
                                    .color(theme::ACCENT),
                            );
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new(*description)
                                    .size(11.0)
                                    .color(theme::TEXT_DIM),
                            );
                        });
                    ui.add_space(8.0);
                }
            });
    }

    /// 所属模块：app / 主页 / 项目加载
    /// 功能：校验项目路径后请求外壳打开工程窗口；文件缺失时从列表移除。
    /// 输入：`path`：用户单击的项目路径（也可能是历史版本）。
    /// 输出/副作用：可能移除项目并保存设置；追加 `OpenProject` 动作。
    /// 复杂度：时间 O(项目数)（缺失时），空间 O(1)。
    ///
    /// Module: app / home page / project loading
    /// Purpose: Validate the path and ask the shell to open a project window; missing files are
    /// removed from the list.
    /// Inputs: `path`: the clicked project path (possibly a history version).
    /// Outputs / side effects: may remove and save the entry; pushes an `OpenProject` action.
    /// Complexity: O(projects) time (only when missing) and O(1) space.
    pub(super) fn open_recent_project(&mut self, path: PathBuf) {
        if self.load_rx.is_some() || self.dialog_rx.is_some() {
            self.status = "正在加载工程，请稍候…".into();
            return;
        }
        if !path.is_file() {
            self.settings.remove_recent(&path);
            self.commit_settings();
            self.status = format!("项目不存在：{}", path.display());
            return;
        }
        self.pending_window_actions
            .push(WindowAction::OpenProject { path });
    }
}

/// 所属模块：app / 主页 / 项目页
/// 功能：读取工程并提取摘要：BPM、拍号、轨道数、片段数、音符数、插件数与长度。
/// 输入：`path` 工程路径。
/// 输出/副作用：返回摘要或中文错误；只读文件。
/// 复杂度：时间 O(工程大小)，空间 O(工程大小)。
///
/// Module: app / home page / projects
/// Purpose: Load a project and extract its summary: BPM, meter, tracks, clips, notes, plugins and length.
/// Inputs: `path` — project path.
/// Outputs / side effects: the summary or a Chinese error; reads the file only.
/// Complexity: O(project size) time and space.
fn load_project_summary(path: &Path) -> Result<HomeProjectSummary, String> {
    let session = ProjectRuntime::open_project(path)?;
    let project = &session.project;
    let tracks = project.tracks.len();
    let clips = project
        .tracks
        .iter()
        .map(|track| track.clips.len())
        .sum::<usize>();
    let notes = project
        .tracks
        .iter()
        .flat_map(|track| track.clips.iter())
        .map(|clip| clip.notes.len())
        .sum::<usize>();
    let plugins = project
        .tracks
        .iter()
        .filter(|track| matches!(track.instrument, InstrumentRef::Plugin(_)))
        .count()
        + project
            .tracks
            .iter()
            .flat_map(|track| track.effects.iter())
            .filter(|effect| effect.plugin().is_some())
            .count()
        + usize::from(
            project
                .master_effect
                .as_ref()
                .and_then(|effect| effect.plugin())
                .is_some(),
        );
    Ok(HomeProjectSummary {
        bpm: project.bpm,
        time_sig: (project.time_sig_num, project.time_sig_den),
        beats_per_bar: project.beats_per_bar(),
        tracks,
        clips,
        notes,
        plugins,
        length_beats: project.length_beats,
    })
}

/// 所属模块：app / 主页 / 项目页
/// 功能：绘制一行“字段名 + 值”。
/// 输入：
/// - `ui`：详情区。
/// - `label`：字段名。
/// - `value`：字段值。
/// 输出/副作用：只绘制文本；不修改状态。
/// 复杂度：时间 O(值长度)，空间 O(1)。
///
/// Module: app / home page / projects
/// Purpose: Draw one “field + value” row.
/// Inputs:
/// - `ui`: detail area.
/// - `label`: field name.
/// - `value`: field value.
/// Outputs / side effects: paints text only; no state change.
/// Complexity: O(value length) time and O(1) space.
fn project_detail_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(10.0)
                .color(theme::TEXT_FAINT),
        );
        ui.label(egui::RichText::new(value).size(10.5).color(theme::TEXT_DIM));
    });
}

/// 所属模块：app / 主页 / 项目页
/// 功能：把字节数格式化为 B / KB / MB / GB。
/// 输入：`bytes` 字节数。
/// 输出/副作用：返回展示文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home page / projects
/// Purpose: Format a byte count as B / KB / MB / GB.
/// Inputs: `bytes` — byte count.
/// Outputs / side effects: display text; no state change.
/// Complexity: O(1) time and space.
fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let value = bytes as f64;
    if value >= GB {
        format!("{:.1} GB", value / GB)
    } else if value >= MB {
        format!("{:.1} MB", value / MB)
    } else if value >= KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{bytes} B")
    }
}

/// 所属模块：app / 主页 / 项目页
/// 功能：返回历史版本的展示名（去掉扩展名）。
/// 输入：`path` 版本文件路径。
/// 输出/副作用：返回文件名；不修改状态。
/// 复杂度：时间 O(路径长度)，空间 O(名称长度)。
///
/// Module: app / home page / projects
/// Purpose: Return the display name of a history version (without the extension).
/// Inputs: `path` — version file path.
/// Outputs / side effects: the file name; no state change.
/// Complexity: O(path length) time and O(name length) space.
fn format_version_name(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "版本".into())
}

/// 所属模块：app / 主页 / 侧栏
/// 功能：绘制主页左上角的软件图标：IDEA 蓝圆角方块 + 白色波形。
/// 输入：
/// - `ui`：侧栏水平布局。
/// - `size`：图标边长（逻辑点）。
/// 输出/副作用：分配空间并绘制；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home page / sidebar
/// Purpose: Draw the home logo: an IDEA-blue rounded square with a white waveform.
/// Inputs:
/// - `ui`: sidebar horizontal layout.
/// - `size`: icon side length in logical points.
/// Outputs / side effects: allocates space and paints; no state change.
/// Complexity: O(1) time and space.
fn home_logo(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::from(6), theme::ACCENT);
    let wave = [0.58_f32, 0.30, 0.70, 0.32, 0.60];
    let points: Vec<egui::Pos2> = wave
        .iter()
        .enumerate()
        .map(|(index, y)| {
            egui::pos2(
                rect.left() + rect.width() * (0.16 + 0.17 * index as f32),
                rect.top() + rect.height() * y,
            )
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.6_f32, egui::Color32::WHITE),
    ));
}

/// 所属模块：app / 主页
/// 功能：绘制展开/收起箭头（实心三角），避免依赖可能缺失的 Unicode 字形。
/// 输入：
/// - `painter`：绘制器。
/// - `center`：箭头中心。
/// - `expanded`：展开时为向下箭头，否则向右。
/// - `color`：填充色。
/// 输出/副作用：写入绘制命令；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home page
/// Purpose: Draw the expand/collapse chevron as a filled triangle so it never depends on a
/// possibly missing Unicode glyph.
/// Inputs:
/// - `painter`: painter.
/// - `center`: chevron center.
/// - `expanded`: down when expanded, right otherwise.
/// - `color`: fill color.
/// Outputs / side effects: emits paint commands; no state change.
/// Complexity: O(1) time and space.
fn paint_chevron(
    painter: &egui::Painter,
    center: egui::Pos2,
    expanded: bool,
    color: egui::Color32,
) {
    let half = 4.0;
    let points = if expanded {
        vec![
            egui::pos2(center.x - half, center.y - half * 0.5),
            egui::pos2(center.x + half, center.y - half * 0.5),
            egui::pos2(center.x, center.y + half * 0.7),
        ]
    } else {
        vec![
            egui::pos2(center.x - half * 0.5, center.y - half),
            egui::pos2(center.x - half * 0.5, center.y + half),
            egui::pos2(center.x + half * 0.7, center.y),
        ]
    };
    painter.add(egui::Shape::convex_polygon(
        points,
        color,
        egui::Stroke::NONE,
    ));
}

/// 所属模块：app / 主页 / 侧栏
/// 功能：返回主页菜单项的中文名称。
/// 输入：`page`：菜单页面。
/// 输出/副作用：返回静态文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home page / sidebar
/// Purpose: Return the Chinese label of a home menu entry.
/// Inputs: `page`: menu page.
/// Outputs / side effects: static text; no state change.
/// Complexity: O(1) time and space.
fn home_page_label(page: HomePage) -> &'static str {
    match page {
        HomePage::Projects => "项目",
        HomePage::Templates => "模板",
        HomePage::Plugins => "插件",
        HomePage::Tips => "提示",
    }
}

/// 所属模块：app / 主页 / 侧栏
/// 功能：绘制一个主页菜单项（图标 + 文本），包含悬停、选中与按下状态。
/// 输入：
/// - `ui`：侧栏绘制上下文。
/// - `page`：菜单页面。
/// - `selected`：是否为当前页面。
/// 输出/副作用：分配整行空间并绘制；点击由调用方处理。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home page / sidebar
/// Purpose: Draw one home menu entry (icon + label) with hover and selected states.
/// Inputs:
/// - `ui`: sidebar paint context.
/// - `page`: menu page.
/// - `selected`: whether this page is active.
/// Outputs / side effects: allocates the full row and paints; the caller handles clicks.
/// Complexity: O(1) time and space.
fn home_nav_item(ui: &mut egui::Ui, page: HomePage, selected: bool) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, NAV_H), egui::Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, egui::CornerRadius::from(4), theme::ACCENT_DARK);
        painter.rect_stroke(
            rect,
            egui::CornerRadius::from(4),
            egui::Stroke::new(1.0_f32, theme::ACCENT),
            egui::StrokeKind::Inside,
        );
    } else if hovered {
        painter.rect_filled(rect, egui::CornerRadius::from(4), theme::PANEL_3);
    }
    let color = if selected {
        egui::Color32::WHITE
    } else if hovered {
        theme::TEXT
    } else {
        theme::TEXT_DIM
    };
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 18.0, rect.center().y),
        egui::vec2(14.0, 14.0),
    );
    home_nav_icon(painter, icon_rect, page, color);
    painter.text(
        egui::pos2(rect.left() + 32.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        home_page_label(page),
        egui::FontId::proportional(12.5),
        color,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// 所属模块：app / 主页 / 侧栏
/// 功能：绘制主页菜单的细线图标：项目为列表、模板为缩略图列、插件为四宫格、提示为灯泡。
/// 输入：
/// - `painter`：绘制器。
/// - `rect`：图标矩形。
/// - `page`：菜单页面。
/// - `color`：线条颜色。
/// 输出/副作用：写入绘制命令；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home page / sidebar
/// Purpose: Draw thin-line icons: a list for projects, a thumbnail column for templates, a grid
/// for plugins and a bulb for tips.
/// Inputs:
/// - `painter`: painter.
/// - `rect`: icon rect.
/// - `page`: menu page.
/// - `color`: line color.
/// Outputs / side effects: emits paint commands; no state change.
/// Complexity: O(1) time and space.
fn home_nav_icon(painter: &egui::Painter, rect: egui::Rect, page: HomePage, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.1_f32, color);
    match page {
        HomePage::Projects => {
            for row in 0..3 {
                let y = rect.top() + 3.0 + row as f32 * 4.0;
                painter.line_segment(
                    [
                        egui::pos2(rect.left() + 1.0, y),
                        egui::pos2(rect.right() - 1.0, y),
                    ],
                    stroke,
                );
            }
        }
        HomePage::Templates => {
            // 模板图标：左侧一列缩略块 + 右侧三条内容行，与插件四宫格区分。
            // Template icon: a left thumbnail column plus three content rows, distinct from the
            // plugin grid.
            painter.rect_stroke(
                egui::Rect::from_min_size(
                    egui::pos2(rect.left() + 1.0, rect.top() + 1.5),
                    egui::vec2(4.5, 11.0),
                ),
                egui::CornerRadius::from(1),
                stroke,
                egui::StrokeKind::Inside,
            );
            for row in 0..3 {
                let y = rect.top() + 2.5 + row as f32 * 4.5;
                painter.line_segment(
                    [
                        egui::pos2(rect.left() + 7.5, y),
                        egui::pos2(rect.right() - 0.5, y),
                    ],
                    stroke,
                );
            }
        }
        HomePage::Plugins => {
            let cell = 5.0;
            for (x, y) in [(0.0, 0.0), (7.0, 0.0), (0.0, 7.0), (7.0, 7.0)] {
                painter.rect_stroke(
                    egui::Rect::from_min_size(
                        egui::pos2(rect.left() + 1.0 + x, rect.top() + 1.0 + y),
                        egui::vec2(cell, cell),
                    ),
                    egui::CornerRadius::from(1),
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        HomePage::Tips => {
            painter.circle_stroke(egui::pos2(rect.center().x, rect.top() + 5.5), 4.0, stroke);
            painter.line_segment(
                [
                    egui::pos2(rect.center().x, rect.top() + 9.5),
                    egui::pos2(rect.center().x, rect.bottom() - 1.5),
                ],
                stroke,
            );
        }
    }
}

/// 所属模块：app / 主页 / 模板页
/// 功能：绘制模板家族筛选条（全部 / 流行 / 摇滚 / 电子 / 嘻哈 / 爵士 / 古典 / 根源 / 其他）。
/// 输入：
/// - `ui`：筛选条上下文。
/// - `selected`：当前选中家族；`None` 表示全部。
/// 输出/副作用：点击时写入 `selected`；不修改其他状态。
/// 复杂度：每帧 O(家族数)。
///
/// Module: app / home page / templates
/// Purpose: Draw the family filter row (all / pop / rock / electronic / urban / jazz / classical /
/// roots / other).
/// Inputs:
/// - `ui`: filter row context.
/// - `selected`: current family; `None` means all.
/// Outputs / side effects: writes `selected` on click.
/// Complexity: O(families) per frame.
fn template_family_filter(ui: &mut egui::Ui, selected: &mut Option<TemplateFamily>) {
    ui.horizontal_wrapped(|ui| {
        if template_filter_chip(ui, "全部", selected.is_none()).clicked() {
            *selected = None;
        }
        for family in TemplateFamily::ALL {
            if template_filter_chip(ui, family.label(), *selected == Some(family)).clicked() {
                *selected = Some(family);
            }
        }
    });
}

/// 所属模块：app / 主页 / 模板页
/// 功能：绘制单个筛选按钮，使用 IDEA 风格的四态配色。
/// 输入：
/// - `ui`：筛选条上下文。
/// - `label`：按钮文本。
/// - `selected`：是否选中。
/// 输出/副作用：返回按钮响应；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / home page / templates
/// Purpose: Draw one filter button using the IDEA-style four-state palette.
/// Inputs:
/// - `ui`: filter row context.
/// - `label`: button text.
/// - `selected`: whether the filter is active.
/// Outputs / side effects: the button response; no state change.
/// Complexity: O(1) time and space.
fn template_filter_chip(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let fill = if selected {
        theme::ACCENT_DARK
    } else {
        theme::PANEL_2
    };
    let stroke = if selected {
        egui::Stroke::new(1.0_f32, theme::ACCENT)
    } else {
        egui::Stroke::new(1.0_f32, theme::LINE)
    };
    let color = if selected {
        egui::Color32::WHITE
    } else {
        theme::TEXT_DIM
    };
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(11.0).color(color))
            .fill(fill)
            .stroke(stroke)
            .min_size(egui::vec2(52.0, 22.0)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// 所属模块：app / 主页 / 模板页
/// 功能：绘制一张模板卡片（名称、风格/速度/拍号/小节、描述、标签、轨道摘要与创建按钮）。
/// 输入：
/// - `ui`：卡片容器。
/// - `template`：模板。
/// - `width`：卡片宽度，逻辑点。
/// 输出/副作用：分配卡片矩形并绘制；返回整卡点击响应。
/// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
///
/// Module: app / home page / templates
/// Purpose: Draw one template card (name, style/tempo/meter/bars, description, tags, track summary
/// and a create button).
/// Inputs:
/// - `ui`: card container.
/// - `template`: template.
/// - `width`: card width in logical points.
/// Outputs / side effects: allocates and paints the card; returns the whole-card click response.
/// Complexity: O(text length) time and space.
fn template_card(ui: &mut egui::Ui, template: &MusicTemplate, width: f32) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, TEMPLATE_CARD_H), egui::Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        egui::CornerRadius::from(6),
        if hovered {
            theme::PANEL_3
        } else {
            theme::PANEL_2
        },
    );
    painter.rect_stroke(
        rect,
        egui::CornerRadius::from(6),
        egui::Stroke::new(1.0_f32, if hovered { theme::ACCENT } else { theme::LINE }),
        egui::StrokeKind::Inside,
    );
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(rect.left() + 10.0, rect.top() + 12.0),
            egui::vec2(4.0, 18.0),
        ),
        egui::CornerRadius::from(2),
        theme::ACCENT,
    );
    painter.text(
        egui::pos2(rect.left() + 22.0, rect.top() + 10.0),
        egui::Align2::LEFT_TOP,
        template.name,
        egui::FontId::proportional(13.0),
        theme::TEXT,
    );
    painter.text(
        egui::pos2(rect.left() + 22.0, rect.top() + 29.0),
        egui::Align2::LEFT_TOP,
        format!(
            "{} · {} BPM · 4/4 · {} 小节",
            template.style,
            template.bpm.round() as i32,
            template.bars
        ),
        egui::FontId::proportional(9.5),
        theme::TEXT_FAINT,
    );
    let description = egui::WidgetText::from(
        egui::RichText::new(template.description)
            .size(10.5)
            .color(theme::TEXT_DIM),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        width - 24.0,
        egui::TextStyle::Small,
    );
    painter.galley(
        egui::pos2(rect.left() + 12.0, rect.top() + 48.0),
        description,
        theme::TEXT_DIM,
    );
    painter.text(
        egui::pos2(rect.left() + 12.0, rect.bottom() - 46.0),
        egui::Align2::LEFT_CENTER,
        template.tags.join(" · "),
        egui::FontId::proportional(9.0),
        theme::ACCENT,
    );
    painter.text(
        egui::pos2(rect.left() + 12.0, rect.bottom() - 26.0),
        egui::Align2::LEFT_CENTER,
        template.track_summary(),
        egui::FontId::proportional(9.5),
        theme::TEXT_FAINT,
    );
    let button = egui::Rect::from_min_size(
        egui::pos2(rect.right() - 72.0, rect.bottom() - 36.0),
        egui::vec2(60.0, 24.0),
    );
    painter.rect_filled(
        button,
        egui::CornerRadius::from(4),
        if hovered {
            theme::ACCENT
        } else {
            theme::ACCENT_DARK
        },
    );
    painter.text(
        button.center(),
        egui::Align2::CENTER_CENTER,
        "创建",
        egui::FontId::proportional(10.5),
        egui::Color32::WHITE,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!(
            "{} · {} · {} BPM",
            template.name, template.key_label, template.bpm
        ))
}

/// 所属模块：app / 主页 / 模板页
/// 功能：绘制一张范例工程卡片（名称、来源模板、小节数与描述）。
/// 输入：
/// - `ui`：卡片容器。
/// - `example`：范例。
/// - `width`：卡片宽度，逻辑点。
/// 输出/副作用：分配卡片矩形并绘制；返回整卡点击响应。
/// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
///
/// Module: app / home page / templates
/// Purpose: Draw one example-project card (name, source template, bar count and description).
/// Inputs:
/// - `ui`: card container.
/// - `example`: example.
/// - `width`: card width in logical points.
/// Outputs / side effects: allocates and paints the card; returns the whole-card click response.
/// Complexity: O(text length) time and space.
fn example_card(ui: &mut egui::Ui, example: &ExampleProject, width: f32) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, EXAMPLE_CARD_H), egui::Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        egui::CornerRadius::from(6),
        if hovered {
            theme::PANEL_3
        } else {
            theme::PANEL_2
        },
    );
    painter.rect_stroke(
        rect,
        egui::CornerRadius::from(6),
        egui::Stroke::new(1.0_f32, if hovered { theme::ACCENT } else { theme::LINE }),
        egui::StrokeKind::Inside,
    );
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(rect.left() + 10.0, rect.top() + 12.0),
            egui::vec2(4.0, 16.0),
        ),
        egui::CornerRadius::from(2),
        theme::AMBER,
    );
    let template_name = template::find_template(example.template_id)
        .map(|template| template.name)
        .unwrap_or("模板");
    painter.text(
        egui::pos2(rect.left() + 22.0, rect.top() + 10.0),
        egui::Align2::LEFT_TOP,
        example.name,
        egui::FontId::proportional(12.5),
        theme::TEXT,
    );
    painter.text(
        egui::pos2(rect.left() + 22.0, rect.top() + 28.0),
        egui::Align2::LEFT_TOP,
        format!("{template_name} · {} 小节 · 范例", example.bars),
        egui::FontId::proportional(9.5),
        theme::TEXT_FAINT,
    );
    let description = egui::WidgetText::from(
        egui::RichText::new(example.description)
            .size(10.0)
            .color(theme::TEXT_DIM),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        width - 96.0,
        egui::TextStyle::Small,
    );
    painter.galley(
        egui::pos2(rect.left() + 12.0, rect.top() + 46.0),
        description,
        theme::TEXT_DIM,
    );
    let button = egui::Rect::from_min_size(
        egui::pos2(rect.right() - 72.0, rect.bottom() - 34.0),
        egui::vec2(60.0, 22.0),
    );
    painter.rect_filled(
        button,
        egui::CornerRadius::from(4),
        if hovered {
            theme::ACCENT
        } else {
            theme::ACCENT_DARK
        },
    );
    painter.text(
        button.center(),
        egui::Align2::CENTER_CENTER,
        "打开",
        egui::FontId::proportional(10.5),
        egui::Color32::WHITE,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(example.description)
}

/// 所属模块：app / 主页 / 插件页
/// 功能：绘制一个插件品牌标题行：展开箭头、品牌名与各类数量；默认展开。
/// 输入：
/// - `ui`：插件列表。
/// - `brand`：品牌名。
/// - `instruments` / `effects` / `unknown`：三类插件数量。
/// - `collapsed`：是否收起。
/// 输出/副作用：分配整行并绘制；返回点击响应。
/// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
///
/// Module: app / home page / plugins
/// Purpose: Draw one plugin brand header: chevron, brand name and per-kind counts; expanded by default.
/// Inputs:
/// - `ui`: plugin list.
/// - `brand`: brand name.
/// - `instruments` / `effects` / `unknown`: plugin counts per kind.
/// - `collapsed`: whether the brand is collapsed.
/// Outputs / side effects: allocates and paints the row; returns the click response.
/// Complexity: O(text length) time and space.
fn plugin_brand_header(
    ui: &mut egui::Ui,
    brand: &str,
    instruments: usize,
    effects: usize,
    unknown: usize,
    collapsed: bool,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, PLUGIN_BRAND_H), egui::Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter();
    if hovered {
        painter.rect_filled(rect, egui::CornerRadius::from(4), theme::PANEL_2);
    }
    paint_chevron(
        painter,
        egui::pos2(rect.left() + 12.0, rect.center().y),
        !collapsed,
        theme::TEXT_DIM,
    );
    painter.text(
        egui::pos2(rect.left() + 26.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        brand,
        egui::FontId::proportional(12.5),
        theme::TEXT,
    );
    let mut counts = format!("乐器 {instruments} · 效果器 {effects}");
    if unknown > 0 {
        counts.push_str(&format!(" · 未知 {unknown}"));
    }
    painter.text(
        egui::pos2(rect.right() - 12.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        counts,
        egui::FontId::proportional(10.0),
        theme::TEXT_FAINT,
    );
    painter.hline(
        rect.left()..=rect.right(),
        rect.bottom(),
        egui::Stroke::new(1.0_f32, theme::LINE),
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if collapsed {
            "点击展开该品牌的插件"
        } else {
            "点击收起该品牌的插件"
        })
}

/// 所属模块：app / 主页 / 插件页
/// 功能：绘制一条插件：类型、分类、格式等标签 + 名称，右侧“详情”按钮展开路径与元数据。
/// 输入：
/// - `ui`：插件列表。
/// - `plugin`：扫描记录。
/// - `expanded`：是否展开详情。
/// 输出/副作用：分配卡片并绘制；返回是否点击了“详情”。
/// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
///
/// Module: app / home page / plugins
/// Purpose: Draw one plugin: kind/category/format tags + name, with a “detail” button that expands
/// the path and metadata.
/// Inputs:
/// - `ui`: plugin list.
/// - `plugin`: scan record.
/// - `expanded`: whether the details are open.
/// Outputs / side effects: allocates and paints the card; returns whether “detail” was clicked.
/// Complexity: O(text length) time and space.
fn home_plugin_row(ui: &mut egui::Ui, plugin: &ScannedPlugin, expanded: bool) -> bool {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, PLUGIN_ROW_H), egui::Sense::click());
    let hovered = response.hovered();
    if hovered || expanded {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::from(4), theme::PANEL_2);
    }
    let content_rect = rect.shrink2(egui::vec2(8.0, 0.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(content_rect), |ui| {
        ui.horizontal_centered(|ui| {
            let kind_color = match plugin.kind {
                PluginKind::Instrument => theme::ACCENT,
                PluginKind::Effect => theme::AMBER,
                PluginKind::Unknown => theme::TEXT_FAINT,
            };
            // 名称在前、标签在后：先读插件名，再读它的类型信息，符合从左到右的阅读顺序。
            // Name first, tags afterwards: the plugin is identified before its metadata is read.
            ui.allocate_ui_with_layout(
                egui::vec2(PLUGIN_NAME_W, ui.available_height()),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&plugin.name)
                                .size(11.5)
                                .color(theme::TEXT),
                        )
                        .truncate(),
                    );
                },
            );
            theme::badge(ui, plugin.kind.label(), kind_color);
            theme::badge(
                ui,
                plugin.format.label(),
                if plugin.format == crate::core::model::PluginFormat::Vst3 {
                    theme::ACCENT
                } else {
                    theme::AMBER
                },
            );
            let category = if plugin.category.trim().is_empty() {
                "未分类"
            } else {
                plugin.category.trim()
            };
            theme::badge(ui, category, theme::TEXT_DIM);
            if plugin.has_midi_input {
                theme::badge(ui, "MIDI", theme::TEXT_DIM);
            }
            if plugin.has_gui {
                theme::badge(ui, "GUI", theme::ACCENT);
            }
        });
    });
    paint_chevron(
        ui.painter(),
        egui::pos2(rect.right() - 14.0, rect.center().y),
        expanded,
        theme::TEXT_DIM,
    );
    let toggled = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!(
            "{}\n{}\n点击查看插件详情",
            plugin.name,
            plugin.path.display()
        ))
        .clicked();
    if expanded {
        egui::Frame::NONE
            .fill(theme::PANEL)
            .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                project_detail_row(ui, "路径", &plugin.path.display().to_string());
                project_detail_row(
                    ui,
                    "厂商",
                    if plugin.vendor.is_empty() {
                        "未标注"
                    } else {
                        &plugin.vendor
                    },
                );
                project_detail_row(
                    ui,
                    "分类",
                    if plugin.category.is_empty() {
                        "未分类"
                    } else {
                        &plugin.category
                    },
                );
                project_detail_row(ui, "格式", plugin.format.label());
                project_detail_row(
                    ui,
                    "能力",
                    &format!(
                        "MIDI 输入：{} · 原生界面：{}",
                        if plugin.has_midi_input { "是" } else { "否" },
                        if plugin.has_gui { "是" } else { "否" }
                    ),
                );
                if !plugin.uid.is_empty() {
                    project_detail_row(ui, "标识", &plugin.uid);
                }
            });
    }
    ui.add_space(2.0);
    toggled
}

/// 所属模块：app / 主页 / 项目页
/// 功能：把项目路径映射到 0..=255 的稳定色板索引，让同一项目图标颜色保持一致。
/// 输入：`path`：项目路径。
/// 输出/副作用：返回色板索引；不修改状态。
/// 复杂度：时间 O(路径长度)，空间 O(1)。
///
/// Module: app / home page / projects
/// Purpose: Map a project path to a stable 0..=255 palette index so its icon keeps one color.
/// Inputs: `path`: project path.
/// Outputs / side effects: palette index; no state change.
/// Complexity: O(path length) time and O(1) space.
fn path_hash(path: &std::path::Path) -> u8 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in path.to_string_lossy().to_lowercase().as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (hash & 0xff) as u8
}

/// 所属模块：app / 主页 / 项目页
/// 功能：返回项目的显示名称：优先文件名（去掉扩展名），为空时回退完整路径。
/// 输入：`path`：项目路径。
/// 输出/副作用：返回展示文本；不修改状态。
/// 复杂度：时间 O(路径长度)，空间 O(名称长度)。
///
/// Module: app / home page / projects
/// Purpose: Return the display name of a project: the file stem, or the full path when empty.
/// Inputs: `path`: project path.
/// Outputs / side effects: display text; no state change.
/// Complexity: O(path length) time and O(name length) space.
fn project_display_name(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}
