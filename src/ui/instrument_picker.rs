//! 所属模块：ui / 乐器窗口
//! 文件用途：
//! - 绘制乐器选择窗口：左侧为乐器种类（全部、收藏、钢琴、木吉他、电吉他、弦乐、其他），
//!   右侧为按厂商分组的乐器列表（内置乐器与 VST 乐器插件）。
//! - 每行提供收藏、名称/描述、格式与能力徽章以及「选择」按钮，样式参考商业插件浏览器。
//!
//! 函数清单：
//! - `SurfingApp::open_instrument_picker`：打开指定轨道的乐器窗口。
//! - `SurfingApp::ui_instrument_picker`：绘制窗口并应用选择结果。
//! - `collect_instrument_options`：合并内置乐器与 VST 乐器插件。
//! - `plugin_category` / `native_category`：把插件/原生乐器归入左侧种类。
//! - `instrument_subtitle` / `category_item` / `favorite_button`：行与侧栏辅助。
//!
//! Module: ui / instrument window
//! Purpose:
//! - Draw the instrument picker: categories on the left (all, favourites, piano, acoustic guitar,
//!   electric guitar, strings, other) and vendor-grouped instruments on the right (built-in and
//!   VST instrument plugins).
//! - Each row offers a favourite toggle, name/description, format/capability badges and a
//!   "select" button, styled after commercial plugin browsers.
//!
//! Functions:
//! - `SurfingApp::open_instrument_picker`: open the window for one track.
//! - `SurfingApp::ui_instrument_picker`: draw the window and apply the choice.
//! - `collect_instrument_options`: merge built-in instruments and VST instrument plugins.
//! - `plugin_category` / `native_category`: map plugins/native instruments to left categories.
//! - `instrument_subtitle` / `category_item` / `favorite_button`: row and sidebar helpers.

use crate::app::{InstrumentCategory, InstrumentPicker, SurfingApp};
use crate::core::model::{
    DrumSettings, InstrumentRef, NativeInstrument, NativeInstrumentKind, SynthSettings,
};
use crate::plugin::{PluginKind, ScannedPlugin};
use crate::ui::theme;
use eframe::egui::{self, Align, Layout, RichText, Sense, Vec2};

/// 乐器窗口默认尺寸（逻辑点）。
/// Default instrument window size in logical points.
const PICKER_W: f32 = 820.0;
/// 乐器窗口默认高度（逻辑点）。
/// Default instrument window height in logical points.
const PICKER_H: f32 = 560.0;
/// 左侧种类栏宽度（逻辑点）。
/// Width of the left category column in logical points.
const CATEGORY_W: f32 = 132.0;
/// 种类项高度（逻辑点）。
/// Height of one category entry in logical points.
const CATEGORY_ROW_H: f32 = 30.0;
/// 乐器行高度（逻辑点）。
/// Height of one instrument row in logical points.
const OPTION_ROW_H: f32 = 46.0;

/// 一条可选乐器记录（内置或插件）。
/// One selectable instrument (built-in or plugin).
#[derive(Clone)]
struct InstrumentOption {
    /// 收藏与去重使用的稳定键。
    /// Stable key used for favourites and de-duplication.
    key: String,
    name: String,
    vendor: String,
    subtitle: String,
    category: InstrumentCategory,
    format_label: &'static str,
    has_midi: bool,
    has_gui: bool,
    builtin: Option<InstrumentRef>,
    plugin: Option<ScannedPlugin>,
}

impl SurfingApp {
    /// 所属模块：ui / 乐器窗口
    /// 功能：为指定轨道打开乐器窗口并选中该轨道。
    /// 输入：`track_index` 轨道索引。
    /// 输出/副作用：修改 `selected_track` 与 `instrument_picker`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ui / instrument window
    /// Purpose: Open the instrument window for one track and select that track.
    /// Inputs: `track_index` — track index.
    /// Outputs / side effects: updates `selected_track` and `instrument_picker`.
    /// Complexity: O(1) time and space.
    pub fn open_instrument_picker(&mut self, track_index: usize) {
        let Some(track) = self.project.tracks.get(track_index) else {
            return;
        };
        self.selected_track = track_index;
        self.instrument_picker = Some(InstrumentPicker::new(track.id));
    }

    /// 所属模块：ui / 乐器窗口
    /// 功能：绘制乐器窗口；应用收藏、种类与搜索状态，并在点击「选择」后切换轨道乐器。
    /// 输入：`ctx` egui 上下文。
    /// 输出/副作用：可能修改工程乐器、关闭乐器编辑器、保存收藏与状态栏。
    /// 复杂度：每帧 O(乐器选项数 × 可见行)。
    ///
    /// Module: ui / instrument window
    /// Purpose: Draw the instrument window; apply favourites, category and search state and switch
    /// the track instrument when "select" is clicked.
    /// Inputs: `ctx` — egui context.
    /// Outputs / side effects: may change the track instrument, close an instrument editor, save
    /// favourites and update the status bar.
    /// Complexity: O(instrument options × visible rows) per frame.
    pub fn ui_instrument_picker(&mut self, ctx: &egui::Context) {
        let Some(picker) = self.instrument_picker.clone() else {
            return;
        };
        let Some(track_index) = self
            .project
            .tracks
            .iter()
            .position(|track| track.id == picker.track_id)
        else {
            self.instrument_picker = None;
            return;
        };
        let track_name = self.project.tracks[track_index].name.clone();
        let is_audio_lane = self.project.tracks[track_index].is_audio_lane();
        let current_key = self.project.tracks[track_index].instrument.key();
        let current_plugin_path = match &self.project.tracks[track_index].instrument {
            InstrumentRef::Plugin(plugin) => Some(plugin.path.clone()),
            _ => None,
        };
        let options = collect_instrument_options(&self.plugin_cache);
        let mut favorites = self.settings.favorite_instruments.clone();
        let mut category = picker.category;
        let mut search = picker.search.clone();
        let mut open = true;
        let mut chosen: Option<InstrumentOption> = None;
        let mut favorites_changed = false;

        egui::Window::new("乐器")
            .title_bar(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .default_size([PICKER_W, PICKER_H])
            .min_size([640.0, 420.0])
            .resizable(true)
            .show(ctx, |ui| {
                // 内容首行即窗口顶栏：标题、种类选择器、搜索与关闭。
                // The first content row is the window header: title, category selector, search and
                // close.
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("乐器 · {track_name}"))
                            .size(13.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    if is_audio_lane {
                        ui.label(
                            RichText::new("音频轨道不能加载乐器")
                                .size(10.0)
                                .color(theme::AMBER),
                        );
                    }
                    ui.add_space(8.0);
                    ui.add_sized(
                        [220.0, 24.0],
                        egui::TextEdit::singleline(&mut search).hint_text("搜索乐器或厂商"),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} 个乐器",
                            options
                                .iter()
                                .filter(|option| match category {
                                    InstrumentCategory::All => true,
                                    InstrumentCategory::Favorites =>
                                        favorites.contains(&option.key),
                                    other => option.category == other,
                                })
                                .count()
                        ))
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
                        Vec2::new(CATEGORY_W, body_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(CATEGORY_W);
                            for entry in InstrumentCategory::ALL {
                                if category_item(ui, entry, category == entry) {
                                    category = entry;
                                }
                            }
                        },
                    );
                    ui.separator();
                    ui.vertical(|ui| {
                        let filter = search.trim().to_lowercase();
                        let visible: Vec<&InstrumentOption> = sorted
                            .iter()
                            .filter(|option| match category {
                                InstrumentCategory::All => true,
                                InstrumentCategory::Favorites => favorites.contains(&option.key),
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
                                    RichText::new("没有匹配的乐器")
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
                                                        InstrumentCategory::All => true,
                                                        InstrumentCategory::Favorites => {
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
                                    // 插件的运行时身份键包含格式与 UID，这里按路径判断“当前”。
                                    // A plugin's runtime key includes format and UID, so compare
                                    // the path to detect the current instrument.
                                    let is_current = match &option.plugin {
                                        Some(plugin) => {
                                            current_plugin_path.as_ref() == Some(&plugin.path)
                                        }
                                        None => option.key == current_key,
                                    };
                                    let row = instrument_row(
                                        ui,
                                        option,
                                        favorite,
                                        is_current,
                                        is_audio_lane,
                                    );
                                    if row.favorite_clicked {
                                        if favorite {
                                            favorites.retain(|key| key != &option.key);
                                        } else {
                                            favorites.push(option.key.clone());
                                        }
                                        favorites_changed = true;
                                    }
                                    if row.select_clicked && !is_audio_lane {
                                        chosen = Some(option.clone());
                                    }
                                }
                            });
                    });
                });
            });

        self.instrument_picker = if open {
            Some(InstrumentPicker {
                track_id: picker.track_id,
                category,
                search,
            })
        } else {
            None
        };
        if favorites_changed {
            self.settings.favorite_instruments = favorites;
            self.commit_settings();
        }
        let Some(option) = chosen else {
            return;
        };
        if let Some(instrument) = option.builtin {
            let label = instrument.label();
            self.close_editor(picker.track_id, crate::audio::SlotKind::Instrument);
            if let Some(track) = self.project.tracks.get_mut(track_index) {
                track.instrument = instrument;
            }
            self.invalidate_plugin_params(picker.track_id);
            self.bump_track_revision(track_index);
            self.status = format!("已切换乐器：{label}");
            self.instrument_picker = None;
        } else if let Some(plugin) = option.plugin {
            // 插件加载必须走现有的 apply_plugin 流程（UI 线程创建实例）。
            // Plugin loading must go through the existing apply_plugin flow (UI-thread instance).
            self.selected_track = track_index;
            self.apply_plugin(
                crate::ui::PluginTarget::Instrument,
                plugin.format,
                plugin.path,
                plugin.name,
                plugin.uid,
            );
            self.instrument_picker = None;
        }
    }
}

/// 行内点击结果：收藏与选择分开处理，避免整行点击误触发选择。
/// Row click result: favourite and select are handled separately so a row click cannot select.
struct RowAction {
    favorite_clicked: bool,
    select_clicked: bool,
}

/// 所属模块：ui / 乐器窗口 / 列表行
/// 功能：绘制一行乐器：收藏星标、名称/描述、格式与能力徽章、选择按钮。
/// 输入：
/// - `ui`：列表绘制上下文。
/// - `option`：乐器记录。
/// - `favorite`：是否已收藏。
/// - `is_current`：是否为当前轨道乐器。
/// - `disabled`：是否禁用选择（音频轨道）。
/// 输出/副作用：绘制界面；返回本行的收藏/选择点击状态。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: ui / instrument window / row
/// Purpose: Draw one instrument row: favourite star, name/description, format and capability
/// badges and a select button.
/// Inputs:
/// - `ui`: list paint context.
/// - `option`: instrument record.
/// - `favorite`: whether it is a favourite.
/// - `is_current`: whether it is the track's current instrument.
/// - `disabled`: whether selection is disabled (audio track).
/// Outputs / side effects: paints the row; returns the favourite/select click states.
/// Complexity: O(text length) time and O(1) space.
fn instrument_row(
    ui: &mut egui::Ui,
    option: &InstrumentOption,
    favorite: bool,
    is_current: bool,
    disabled: bool,
) -> RowAction {
    let mut action = RowAction {
        favorite_clicked: false,
        select_clicked: false,
    };
    egui::Frame::NONE
        .fill(if is_current {
            theme::with_alpha(theme::ACCENT, 24)
        } else {
            theme::PANEL_2
        })
        .stroke(egui::Stroke::new(
            1.0_f32,
            if is_current {
                theme::ACCENT
            } else {
                theme::LINE
            },
        ))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if favorite_button(ui, favorite).clicked() {
                    action.favorite_clicked = true;
                }
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width() - 250.0, OPTION_ROW_H - 10.0),
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
                    let button = egui::Button::new(
                        RichText::new(if is_current { "当前" } else { "选择" })
                            .size(11.0)
                            .color(if is_current {
                                theme::TEXT_DIM
                            } else {
                                egui::Color32::WHITE
                            }),
                    )
                    .fill(if is_current {
                        theme::PANEL_3
                    } else {
                        theme::ACCENT
                    })
                    .min_size(Vec2::new(56.0, 24.0));
                    if ui
                        .add_enabled(!disabled && !is_current, button)
                        .on_hover_text(if disabled {
                            "音频轨道不能加载乐器"
                        } else if is_current {
                            "当前轨道正在使用该乐器"
                        } else {
                            "把该乐器加载到当前轨道"
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
                        if option.format_label == "VST3" {
                            theme::ACCENT
                        } else if option.format_label == "VST2" {
                            theme::AMBER
                        } else {
                            theme::VIOLET
                        },
                    );
                    theme::badge(ui, "乐器", theme::VIOLET);
                });
            });
        });
    action
}

/// 所属模块：ui / 乐器窗口 / 侧栏
/// 功能：绘制一个种类项，选中时使用强调色背景。
/// 输入：`ui`、`category`、`selected`。
/// 输出/副作用：绘制并返回是否被点击。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / instrument window / sidebar
/// Purpose: Draw one category entry with an accent background when selected.
/// Inputs: `ui`, `category`, `selected`.
/// Outputs / side effects: paints and returns whether it was clicked.
/// Complexity: O(1) time and space.
fn category_item(ui: &mut egui::Ui, category: InstrumentCategory, selected: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(CATEGORY_W - 6.0, CATEGORY_ROW_H), Sense::click());
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

/// 所属模块：ui / 乐器窗口 / 列表行
/// 功能：绘制收藏星标按钮。
/// 输入：`ui`、`favorite`。
/// 输出/副作用：绘制并返回点击响应。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / instrument window / row
/// Purpose: Draw the favourite star button.
/// Inputs: `ui`, `favorite`.
/// Outputs / side effects: paints and returns the click response.
/// Complexity: O(1) time and space.
fn favorite_button(ui: &mut egui::Ui, favorite: bool) -> egui::Response {
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

/// 所属模块：ui / 乐器窗口
/// 功能：合并内置乐器与已扫描的 VST 乐器插件，生成按厂商分组的候选列表。
/// 输入：
/// - `plugins`：插件缓存。
/// 输出/副作用：返回候选列表；不修改状态。
/// 复杂度：时间 O(插件数 + 内置乐器数)，空间 O(候选数)。
///
/// Module: ui / instrument window
/// Purpose: Merge built-in instruments and scanned VST instrument plugins into a candidate list.
/// Inputs:
/// - `plugins`: plugin cache.
/// Outputs / side effects: candidate list; no state change.
/// Complexity: O(plugins + built-ins) time and O(candidates) space.
fn collect_instrument_options(plugins: &[ScannedPlugin]) -> Vec<InstrumentOption> {
    let mut options = Vec::new();
    options.push(InstrumentOption {
        key: "builtin:synth".into(),
        name: "内置合成器".into(),
        vendor: "SurfingStudio".into(),
        subtitle: "Instrument / Synth".into(),
        category: InstrumentCategory::Other,
        format_label: "内置",
        has_midi: true,
        has_gui: false,
        builtin: Some(InstrumentRef::Synth(SynthSettings::default())),
        plugin: None,
    });
    options.push(InstrumentOption {
        key: "builtin:drums".into(),
        name: "内置鼓机".into(),
        vendor: "SurfingStudio".into(),
        subtitle: "Instrument / Drums".into(),
        category: InstrumentCategory::Other,
        format_label: "内置",
        has_midi: true,
        has_gui: false,
        builtin: Some(InstrumentRef::Drums(DrumSettings::default())),
        plugin: None,
    });
    for kind in NativeInstrumentKind::ALL {
        let instrument = InstrumentRef::Native(NativeInstrument::new(kind));
        options.push(InstrumentOption {
            key: instrument.key(),
            name: kind.label().into(),
            vendor: "SurfingStudio".into(),
            subtitle: format!("Instrument / {}", native_category(kind).label()),
            category: native_category(kind),
            format_label: "内置",
            has_midi: true,
            has_gui: false,
            builtin: Some(instrument),
            plugin: None,
        });
    }
    for plugin in plugins {
        if plugin.kind != PluginKind::Instrument {
            continue;
        }
        options.push(InstrumentOption {
            key: format!("plugin:{}", plugin.path.to_string_lossy().to_lowercase()),
            name: plugin.name.clone(),
            vendor: if plugin.vendor.is_empty() {
                "未知厂商".into()
            } else {
                plugin.vendor.clone()
            },
            subtitle: if plugin.category.is_empty() {
                format!("Instrument / {}", plugin.format.label())
            } else {
                plugin.category.clone()
            },
            category: plugin_category(plugin),
            format_label: plugin.format.label(),
            has_midi: plugin.has_midi_input,
            has_gui: plugin.has_gui,
            builtin: None,
            plugin: Some(plugin.clone()),
        });
    }
    options
}

/// 所属模块：ui / 乐器窗口 / 分类
/// 功能：把原生乐器映射到左侧种类。
/// 输入：`kind` 原生乐器类型。
/// 输出/副作用：返回种类；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / instrument window / classification
/// Purpose: Map a native instrument kind to a left-hand category.
/// Inputs: `kind` — native instrument kind.
/// Outputs / side effects: category; no state change.
/// Complexity: O(1) time and space.
fn native_category(kind: NativeInstrumentKind) -> InstrumentCategory {
    match kind {
        NativeInstrumentKind::Piano => InstrumentCategory::Piano,
        NativeInstrumentKind::AcousticGuitar => InstrumentCategory::AcousticGuitar,
        NativeInstrumentKind::ElectricGuitar
        | NativeInstrumentKind::DistortedGuitar
        | NativeInstrumentKind::BassGuitar => InstrumentCategory::ElectricGuitar,
        NativeInstrumentKind::Violin
        | NativeInstrumentKind::StringStaccato
        | NativeInstrumentKind::StringPizzicato
        | NativeInstrumentKind::StringLegato => InstrumentCategory::Strings,
    }
}

/// 所属模块：ui / 乐器窗口 / 分类
/// 功能：按插件名称、厂商与分类关键词推断左侧种类。
/// 输入：`plugin` 插件记录。
/// 输出/副作用：返回种类；不修改状态。
/// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
///
/// Module: ui / instrument window / classification
/// Purpose: Infer the left-hand category from the plugin name, vendor and category keywords.
/// Inputs: `plugin` — plugin record.
/// Outputs / side effects: category; no state change.
/// Complexity: O(text length) time and space.
fn plugin_category(plugin: &ScannedPlugin) -> InstrumentCategory {
    let name = plugin.name.to_lowercase();
    let vendor = plugin.vendor.to_lowercase();
    let category = plugin.category.to_lowercase();
    let haystack = format!("{name} {vendor} {category}");
    // 桥接器：ACE Studio / VOCALOID / XStudio 等外部合成软件的桥接插件优先归类。
    // Bridges to external synthesis apps (ACE Studio / VOCALOID / XStudio) come first.
    if name.contains("bridge") {
        return InstrumentCategory::Bridge;
    }
    if haystack.contains("piano") || haystack.contains("keyscape") {
        return InstrumentCategory::Piano;
    }
    // 先排除明确的电吉他型号，再把 Ample Sound 的 M/L 等木吉他型号归入木吉他。
    // Exclude explicit electric models first, then treat Ample Sound M/L models as acoustic.
    let electric_model = name.contains("les paul")
        || name.contains("strat")
        || name.contains("tele")
        || name.contains("guitar lp")
        || name.contains("guitar sc")
        || name.contains("guitar tc")
        || name.contains("electric")
        || name.contains("distort");
    if !electric_model
        && (name.contains("acoustic")
            || name.contains("realguitar")
            || name.contains("nylon")
            || name.contains("steel string")
            || name.contains("classical guitar")
            || name.contains("fingerstyle")
            || (vendor.contains("ample sound") && name.contains("guitar")))
    {
        return InstrumentCategory::AcousticGuitar;
    }
    if haystack.contains("guitar")
        || haystack.contains("bass")
        || haystack.contains("electric")
        || electric_model
    {
        return InstrumentCategory::ElectricGuitar;
    }
    if haystack.contains("string")
        || haystack.contains("violin")
        || haystack.contains("cello")
        || haystack.contains("orchestra")
        || haystack.contains("symphony")
    {
        return InstrumentCategory::Strings;
    }
    InstrumentCategory::Other
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 构造测试用插件记录。
    /// Build a plugin record for tests.
    fn plugin(name: &str, vendor: &str, category: &str) -> ScannedPlugin {
        ScannedPlugin {
            format: crate::core::model::PluginFormat::Vst3,
            path: PathBuf::from(format!("C:/VST3/{name}.vst3")),
            name: name.into(),
            vendor: vendor.into(),
            category: category.into(),
            kind: PluginKind::Instrument,
            uid: String::new(),
            has_midi_input: true,
            has_gui: true,
        }
    }

    /// 覆盖：Ample Sound 木吉他型号归入木吉他，LP/SC 等电吉他型号归入电吉他。
    /// Covers: Ample Sound acoustic models map to acoustic guitar while LP/SC map to electric.
    #[test]
    fn ample_sound_guitars_are_split_by_model() {
        assert_eq!(
            plugin_category(&plugin(
                "Ample Guitar M",
                "Ample Sound",
                "Instrument / Guitar"
            )),
            InstrumentCategory::AcousticGuitar
        );
        assert_eq!(
            plugin_category(&plugin(
                "Ample Guitar L",
                "Ample Sound",
                "Instrument / Guitar"
            )),
            InstrumentCategory::AcousticGuitar
        );
        assert_eq!(
            plugin_category(&plugin(
                "Ample Guitar LP",
                "Ample Sound",
                "Instrument / Guitar"
            )),
            InstrumentCategory::ElectricGuitar
        );
        assert_eq!(
            plugin_category(&plugin(
                "Ample Guitar SC",
                "Ample Sound",
                "Instrument / Guitar"
            )),
            InstrumentCategory::ElectricGuitar
        );
    }

    /// 覆盖：桥接插件与 MusicLab 木吉他分别归入桥接器与木吉他。
    /// Covers: bridge plugins and MusicLab RealGuitar map to bridge and acoustic respectively.
    #[test]
    fn bridge_and_realguitar_are_classified() {
        assert_eq!(
            plugin_category(&plugin("ACE Bridge 2", "ACE Studio", "Instrument / Synth")),
            InstrumentCategory::Bridge
        );
        assert_eq!(
            plugin_category(&plugin("RealGuitar", "MusicLab", "Instrument / Guitar")),
            InstrumentCategory::AcousticGuitar
        );
    }
}
