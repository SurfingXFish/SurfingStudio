//! 所属模块：ui / 效果器窗口
//! 文件用途：
//! - 绘制效果器选择窗口：左侧为种类（全部、收藏、EQ、动态、混响、延时、失真、调制、其他），
//!   右侧为按厂商分组的 VST 效果器列表。
//! - 视觉与交互对齐乐器窗口：无系统标题栏、收藏星标、格式/能力徽章与「选择」按钮。
//!
//! 函数清单：
//! - `SurfingApp::ui_effect_picker`：绘制窗口并应用选择结果。
//! - `collect_effect_options`：合并内置效果器与 VST 效果器候选。
//! - `native_effect_category` / `effect_category`：内置/插件效果器归类。
//! - `effect_row` / `effect_category_item` / `effect_favorite_button`：列表行、种类项与收藏按钮。
//!
//! Module: ui / effect window
//! Purpose:
//! - Draw the effect picker: categories on the left (all, favourites, EQ, dynamics, reverb,
//!   delay, distortion, modulation, other) and vendor-grouped effects on the right.
//! - Match the instrument window: no OS title bar, favourite stars, format/capability badges and
//!   a "select" button.
//!
//! Functions:
//! - `SurfingApp::ui_effect_picker`: draw the window and apply the choice.
//! - `collect_effect_options`: merge native and VST effect candidates.
//! - `native_effect_category` / `effect_category`: classify native and plugin effects.
//! - `effect_row` / `effect_category_item` / `effect_favorite_button`: row and sidebar helpers.

use crate::app::SurfingApp;
use crate::plugin::{PluginKind, ScannedPlugin};
use crate::ui::{theme, EffectCategory, PluginTarget};
use eframe::egui::{self, Align, Layout, RichText, Sense, Vec2};

/// 效果器窗口默认尺寸（逻辑点）。
/// Default effect window size in logical points.
const EFFECT_PICKER_W: f32 = 820.0;
/// 效果器窗口默认高度（逻辑点）。
/// Default effect window height in logical points.
const EFFECT_PICKER_H: f32 = 560.0;
/// 左侧种类栏宽度（逻辑点）。
/// Width of the left category column in logical points.
const EFFECT_CATEGORY_W: f32 = 132.0;
/// 种类项高度（逻辑点）。
/// Height of one category entry in logical points.
const EFFECT_CATEGORY_ROW_H: f32 = 30.0;
/// 效果器行高度（逻辑点）。
/// Height of one effect row in logical points.
const EFFECT_ROW_H: f32 = 46.0;

/// 一条可选效果器记录。
/// One selectable effect record.
#[derive(Clone)]
struct EffectOption {
    key: String,
    name: String,
    vendor: String,
    subtitle: String,
    category: EffectCategory,
    format_label: &'static str,
    has_midi: bool,
    has_gui: bool,
    /// 内置效果器类型；插件条目为 `None`。
    /// Built-in effect kind; `None` for plugin entries.
    builtin: Option<crate::core::model::NativeEffectKind>,
    plugin: Option<ScannedPlugin>,
}

impl SurfingApp {
    /// 所属模块：ui / 效果器窗口
    /// 功能：绘制效果器选择窗口；应用收藏、种类与搜索状态，并在点击「选择」后写入效果器链。
    /// 输入：`ctx` egui 上下文。
    /// 输出/副作用：可能把插件写入当前轨道 / 主输出的效果器链，保存收藏并关闭窗口。
    /// 复杂度：每帧 O(效果器候选数 × 可见行)。
    ///
    /// Module: ui / effect window
    /// Purpose: Draw the effect picker; apply favourites, category and search state and append the
    /// chosen plugin to the effect chain.
    /// Inputs: `ctx` — egui context.
    /// Outputs / side effects: may append a plugin to the current track's or master's effect chain,
    /// save favourites and close the window.
    /// Complexity: O(effect candidates × visible rows) per frame.
    pub fn ui_effect_picker(&mut self, ctx: &egui::Context) {
        let Some(mut browser) = self.browser.take() else {
            return;
        };
        if browser.target == PluginTarget::Instrument {
            self.browser = Some(browser);
            return;
        }
        let target = browser.target;
        let target_name = if target == PluginTarget::MasterEffect {
            "主输出".to_string()
        } else {
            self.project
                .tracks
                .get(self.selected_track)
                .map(|track| track.name.clone())
                .unwrap_or_else(|| "当前轨道".into())
        };
        let options = collect_effect_options(&self.plugin_cache);
        let mut favorites = self.settings.favorite_effects.clone();
        let mut category = browser.category;
        let mut search = browser.filter.clone();
        let mut open = true;
        let mut chosen: Option<EffectOption> = None;
        let mut favorites_changed = false;
        let mut scan_requested = false;
        let scanning = self.scanning;

        egui::Window::new("效果器")
            .title_bar(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .default_size([EFFECT_PICKER_W, EFFECT_PICKER_H])
            .min_size([640.0, 420.0])
            .resizable(true)
            .show(ctx, |ui| {
                // 内容首行即窗口顶栏：标题、种类选择器、搜索、数量与关闭。
                // The first content row is the window header: title, category selector, search,
                // count and close.
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("效果器 · {target_name}"))
                            .size(13.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.add_space(8.0);
                    ui.add_sized(
                        [220.0, 24.0],
                        egui::TextEdit::singleline(&mut search).hint_text("搜索效果器或厂商"),
                    );
                    ui.label(
                        RichText::new(format!("{} 个效果器", options.len()))
                            .size(9.5)
                            .color(theme::TEXT_FAINT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("×").on_hover_text("关闭").clicked() {
                            open = false;
                        }
                    });
                });
                ui.add_space(6.0);
                ui.separator();

                if options.is_empty() {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("尚未扫描到效果器插件")
                                .size(13.0)
                                .color(theme::TEXT_DIM),
                        );
                        ui.add_space(6.0);
                        if ui
                            .add_enabled(!scanning, egui::Button::new("扫描全部插件"))
                            .clicked()
                        {
                            scan_requested = true;
                        }
                    });
                    return;
                }

                let body_height = (ui.available_height() - 4.0).max(120.0);
                let mut sorted = options.clone();
                sorted.sort_by(|a, b| {
                    a.vendor
                        .to_lowercase()
                        .cmp(&b.vendor.to_lowercase())
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(EFFECT_CATEGORY_W, body_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(EFFECT_CATEGORY_W);
                            for entry in EffectCategory::ALL {
                                if effect_category_item(ui, entry, category == entry) {
                                    category = entry;
                                }
                            }
                        },
                    );
                    ui.separator();
                    ui.vertical(|ui| {
                        let filter = search.trim().to_lowercase();
                        let visible: Vec<&EffectOption> = sorted
                            .iter()
                            .filter(|option| match category {
                                EffectCategory::All => true,
                                EffectCategory::Favorites => favorites.contains(&option.key),
                                other => option.category == other,
                            })
                            .filter(|option| {
                                filter.is_empty()
                                    || option.name.to_lowercase().contains(&filter)
                                    || option.vendor.to_lowercase().contains(&filter)
                            })
                            .collect();
                        if visible.is_empty() {
                            ui.add_space(40.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("没有匹配的效果器")
                                        .size(12.0)
                                        .color(theme::TEXT_DIM),
                                );
                            });
                            return;
                        }
                        let mut last_vendor: Option<&str> = None;
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .max_height(body_height)
                            .show(ui, |ui| {
                                for option in visible {
                                    if last_vendor != Some(option.vendor.as_str()) {
                                        ui.add_space(4.0);
                                        let count = sorted
                                            .iter()
                                            .filter(|candidate| {
                                                candidate.vendor == option.vendor
                                                    && (match category {
                                                        EffectCategory::All => true,
                                                        EffectCategory::Favorites => {
                                                            favorites.contains(&candidate.key)
                                                        }
                                                        other => candidate.category == other,
                                                    })
                                            })
                                            .count();
                                        ui.label(
                                            RichText::new(format!(
                                                "▼ {} · {} 个",
                                                option.vendor, count
                                            ))
                                            .size(10.5)
                                            .strong()
                                            .color(theme::TEXT_DIM),
                                        );
                                        last_vendor = Some(option.vendor.as_str());
                                    }
                                    let favorite = favorites.contains(&option.key);
                                    let action = effect_row(ui, option, favorite);
                                    if action.favorite_clicked {
                                        if favorite {
                                            favorites.retain(|key| key != &option.key);
                                        } else {
                                            favorites.push(option.key.clone());
                                        }
                                        favorites_changed = true;
                                    }
                                    if action.select_clicked {
                                        chosen = Some(option.clone());
                                    }
                                }
                            });
                    });
                });
            });

        if scan_requested {
            self.start_scan();
        }
        if favorites_changed {
            self.settings.favorite_effects = favorites;
            self.commit_settings();
        }
        if let Some(option) = chosen {
            if let Some(kind) = option.builtin {
                let track_id = if target == PluginTarget::MasterEffect {
                    Some(crate::core::model::MASTER_TRACK_ID)
                } else {
                    self.current_track_id()
                };
                if let Some(track_id) = track_id {
                    self.add_native_effect(track_id, kind);
                }
            } else if let Some(plugin) = option.plugin {
                self.apply_plugin(target, plugin.format, plugin.path, plugin.name, plugin.uid);
            }
        } else if open {
            browser.category = category;
            browser.filter = search;
            self.browser = Some(browser);
        } else {
            self.browser = None;
        }
    }
}

/// 行内点击结果。
/// Row click result.
struct EffectRowAction {
    favorite_clicked: bool,
    select_clicked: bool,
}

/// 所属模块：ui / 效果器窗口 / 列表行
/// 功能：绘制一行效果器：收藏星标、名称/描述、格式与能力徽章、选择按钮。
/// 输入：
/// - `ui`：列表绘制上下文。
/// - `option`：效果器记录。
/// - `favorite`：是否已收藏。
/// 输出/副作用：绘制界面；返回收藏与选择点击状态。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: ui / effect window / row
/// Purpose: Draw one effect row: favourite star, name/description, format and capability badges
/// and a select button.
/// Inputs:
/// - `ui`: list paint context.
/// - `option`: effect record.
/// - `favorite`: whether it is a favourite.
/// Outputs / side effects: paints the row; returns the favourite/select click states.
/// Complexity: O(text length) time and O(1) space.
fn effect_row(ui: &mut egui::Ui, option: &EffectOption, favorite: bool) -> EffectRowAction {
    let mut action = EffectRowAction {
        favorite_clicked: false,
        select_clicked: false,
    };
    egui::Frame::NONE
        .fill(theme::PANEL_2)
        .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if effect_favorite_button(ui, favorite).clicked() {
                    action.favorite_clicked = true;
                }
                ui.allocate_ui_with_layout(
                    Vec2::new(
                        (ui.available_width() - 250.0).max(120.0),
                        EFFECT_ROW_H - 10.0,
                    ),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.label(
                            RichText::new(&option.name)
                                .size(12.0)
                                .color(theme::TEXT)
                                .strong(),
                        );
                        ui.label(
                            RichText::new(&option.subtitle)
                                .size(9.5)
                                .color(theme::TEXT_FAINT),
                        );
                    },
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("选择").size(11.0).color(egui::Color32::WHITE),
                            )
                            .fill(theme::ACCENT)
                            .min_size(Vec2::new(56.0, 24.0)),
                        )
                        .on_hover_text(if option.builtin.is_some() {
                            "把该内置效果器添加到效果器链末尾"
                        } else {
                            "把该效果器插件添加到效果器链末尾"
                        })
                        .clicked()
                    {
                        action.select_clicked = true;
                    }
                    if option.has_gui {
                        theme::badge(ui, "GUI", theme::ACCENT);
                    }
                    if option.has_midi {
                        theme::badge(ui, "MIDI", theme::TEXT_DIM);
                    }
                    theme::badge(
                        ui,
                        option.format_label,
                        match option.format_label {
                            "VST3" => theme::ACCENT,
                            "VST2" => theme::AMBER,
                            _ => theme::VIOLET,
                        },
                    );
                    theme::badge(ui, "效果器", theme::AMBER);
                });
            });
        });
    action
}

/// 所属模块：ui / 效果器窗口 / 侧栏
/// 功能：绘制一个种类项，选中时使用强调色背景。
/// 输入：`ui`、`category`、`selected`。
/// 输出/副作用：绘制并返回是否被点击。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / effect window / sidebar
/// Purpose: Draw one category entry with an accent background when selected.
/// Inputs: `ui`, `category`, `selected`.
/// Outputs / side effects: paints and returns whether it was clicked.
/// Complexity: O(1) time and space.
fn effect_category_item(ui: &mut egui::Ui, category: EffectCategory, selected: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(EFFECT_CATEGORY_W - 6.0, EFFECT_CATEGORY_ROW_H),
        Sense::click(),
    );
    let hovered = response.hovered();
    if selected {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::from(4), theme::ACCENT_DARK);
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::from(4),
            egui::Stroke::new(1.0_f32, theme::ACCENT),
            egui::StrokeKind::Inside,
        );
    } else if hovered {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::from(4), theme::PANEL_3);
    }
    ui.painter().text(
        egui::pos2(rect.left() + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        category.label(),
        egui::FontId::proportional(12.0),
        if selected {
            egui::Color32::WHITE
        } else if hovered {
            theme::TEXT
        } else {
            theme::TEXT_DIM
        },
    );
    response.clicked()
}

/// 所属模块：ui / 效果器窗口 / 列表行
/// 功能：绘制收藏星标按钮。
/// 输入：`ui`、`favorite`。
/// 输出/副作用：绘制并返回点击响应。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / effect window / row
/// Purpose: Draw the favourite star button.
/// Inputs: `ui`, `favorite`.
/// Outputs / side effects: paints and returns the click response.
/// Complexity: O(1) time and space.
fn effect_favorite_button(ui: &mut egui::Ui, favorite: bool) -> egui::Response {
    let label = if favorite { "★" } else { "☆" };
    ui.add(
        egui::Button::new(RichText::new(label).size(14.0).color(if favorite {
            theme::AMBER
        } else {
            theme::TEXT_FAINT
        }))
        .frame(false)
        .min_size(Vec2::new(22.0, 22.0)),
    )
    .on_hover_text(if favorite {
        "取消收藏"
    } else {
        "加入收藏"
    })
}

/// 所属模块：ui / 效果器窗口
/// 功能：从插件缓存筛选效果器插件并生成候选列表。
/// 输入：`plugins` 插件缓存。
/// 输出/副作用：返回候选列表；不修改状态。
/// 复杂度：时间 O(插件数)，空间 O(候选数)。
///
/// Module: ui / effect window
/// Purpose: Filter effect plugins out of the plugin cache and build the candidate list.
/// Inputs: `plugins` — plugin cache.
/// Outputs / side effects: candidate list; no state change.
/// Complexity: O(plugins) time and O(candidates) space.
fn collect_effect_options(plugins: &[ScannedPlugin]) -> Vec<EffectOption> {
    let mut options = Vec::new();
    // 内置效果器与插件效果器同一列表展示，选择后分别走原生效果器或插件加载路径。
    // Native and plugin effects share one list; the selection path differs after choosing.
    for kind in crate::core::model::NativeEffectKind::ALL {
        let category = native_effect_category(kind);
        options.push(EffectOption {
            key: kind.key().to_string(),
            name: kind.label().to_string(),
            vendor: "SurfingStudio".into(),
            subtitle: format!("内置效果器 / {}", category.label()),
            category,
            format_label: "内置",
            has_midi: false,
            has_gui: false,
            builtin: Some(kind),
            plugin: None,
        });
    }
    options.extend(
        plugins
            .iter()
            .filter(|plugin| plugin.kind == PluginKind::Effect)
            .map(|plugin| EffectOption {
                key: format!("plugin:{}", plugin.path.to_string_lossy().to_lowercase()),
                name: plugin.name.clone(),
                vendor: if plugin.vendor.is_empty() {
                    "未知厂商".into()
                } else {
                    plugin.vendor.clone()
                },
                subtitle: if plugin.category.is_empty() {
                    format!("Effect / {}", plugin.format.label())
                } else {
                    plugin.category.clone()
                },
                category: effect_category(plugin),
                format_label: plugin.format.label(),
                has_midi: plugin.has_midi_input,
                has_gui: plugin.has_gui,
                builtin: None,
                plugin: Some(plugin.clone()),
            }),
    );
    options
}

/// 所属模块：ui / 效果器窗口 / 分类
/// 功能：把内置效果器映射到左侧种类。
/// 输入：`kind` 内置效果器类型。
/// 输出/副作用：返回种类；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / effect window / classification
/// Purpose: Map a native effect kind to a left-hand category.
/// Inputs: `kind` — native effect kind.
/// Outputs / side effects: category; no state change.
/// Complexity: O(1) time and space.
fn native_effect_category(kind: crate::core::model::NativeEffectKind) -> EffectCategory {
    use crate::core::model::NativeEffectKind;
    match kind {
        NativeEffectKind::Overdrive | NativeEffectKind::Distortion | NativeEffectKind::Fuzz => {
            EffectCategory::Distortion
        }
        NativeEffectKind::Chorus | NativeEffectKind::Phaser => EffectCategory::Modulation,
        NativeEffectKind::Delay => EffectCategory::Delay,
        NativeEffectKind::Reverb => EffectCategory::Reverb,
        NativeEffectKind::Compressor => EffectCategory::Dynamics,
    }
}

/// 所属模块：ui / 效果器窗口 / 分类
/// 功能：按名称、厂商与分类关键词推断效果器种类。
/// 输入：`plugin` 插件记录。
/// 输出/副作用：返回种类；不修改状态。
/// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
///
/// Module: ui / effect window / classification
/// Purpose: Infer the effect category from name, vendor and category keywords.
/// Inputs: `plugin` — plugin record.
/// Outputs / side effects: category; no state change.
/// Complexity: O(text length) time and space.
fn effect_category(plugin: &ScannedPlugin) -> EffectCategory {
    let haystack = format!("{} {} {}", plugin.name, plugin.vendor, plugin.category).to_lowercase();
    if haystack.contains("eq") || haystack.contains("equalizer") {
        EffectCategory::Eq
    } else if haystack.contains("compress")
        || haystack.contains("dynamics")
        || haystack.contains("limiter")
        || haystack.contains("gate")
        || haystack.contains("de-ess")
        || haystack.contains("deesser")
    {
        EffectCategory::Dynamics
    } else if haystack.contains("reverb") {
        EffectCategory::Reverb
    } else if haystack.contains("delay") || haystack.contains("echo") {
        EffectCategory::Delay
    } else if haystack.contains("distort")
        || haystack.contains("saturat")
        || haystack.contains("drive")
        || haystack.contains("fuzz")
        || haystack.contains("amp")
    {
        EffectCategory::Distortion
    } else if haystack.contains("chorus")
        || haystack.contains("phaser")
        || haystack.contains("flanger")
        || haystack.contains("modulat")
    {
        EffectCategory::Modulation
    } else {
        EffectCategory::Other
    }
}
