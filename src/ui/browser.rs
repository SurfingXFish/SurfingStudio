//! Plugin browser with VST3 + legacy VST scanning, type filters and vendor groups.

use crate::app::SurfingApp;
use crate::core::model::PluginFormat;
use crate::plugin::{PluginKind, ScannedPlugin};
use crate::ui::{theme, PluginFilter, PluginTarget};
use eframe::egui::{self, RichText};
use std::collections::BTreeMap;

impl SurfingApp {
    pub fn ui_browser(&mut self, ctx: &egui::Context) {
        let Some(mut browser) = self.browser.take() else {
            return;
        };
        // 效果器选择复用乐器窗口的布局；乐器目标仍使用本浏览器的扫描与管理界面。
        // Effect selection reuses the instrument-window layout; the instrument target keeps this
        // browser for scanning and management.
        if browser.target != PluginTarget::Instrument {
            self.browser = Some(browser);
            self.ui_effect_picker(ctx);
            return;
        }
        let mut open = true;
        let mut chosen: Option<(PluginFormat, std::path::PathBuf, String, String)> = None;

        let target_text = match browser.target {
            PluginTarget::Instrument => "当前轨道 → 乐器插槽",
            PluginTarget::Effect => "当前轨道 → 效果器链末尾",
            PluginTarget::MasterEffect => "主输出 → 效果器插槽",
        };

        egui::Window::new("插件浏览器")
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::new(0.0, -10.0))
            .default_width(720.0)
            .default_height(540.0)
            .min_width(560.0)
            .min_height(380.0)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("目标").size(11.0).color(theme::TEXT_DIM));
                    ui.label(RichText::new(target_text).size(11.0).color(theme::ACCENT));
                });
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    if ui.button("扫描全部").clicked() {
                        self.start_scan();
                    }
                    if ui.button("仅扫描 VST3").clicked() {
                        self.scan_vst3_only();
                    }
                    if ui.button("扫描 VST / VST2").clicked() {
                        self.scan_legacy_only();
                    }
                    if ui.button("扫描文件夹…").clicked() {
                        let request =
                            crate::platform::FileRequest::titled("选择插件文件夹");
                        if let Some(folder) = crate::platform::pick_folder(&request) {
                            self.start_scan_with(crate::app::ScanMode::All, vec![folder]);
                        }
                    }
                    if ui.button("加载单个 VST2 DLL…").clicked() {
                        let filters: &[(&str, &[&str])] = &[("VST / VST2 插件", &["dll"])];
                        let request = crate::platform::FileRequest {
                            title: "选择 VST2 插件",
                            filters,
                            start_dir: None,
                            default_name: None,
                            parent: None,
                        };
                        if let Some(path) = crate::platform::pick_file(&request) {
                            let name = path
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| "VST 插件".into());
                            chosen = Some((PluginFormat::Vst2, path, name, String::new()));
                        }
                    }
                    if self.scanning {
                        ui.spinner();
                        ui.label(RichText::new("扫描中…").size(11.0).color(theme::TEXT_DIM));
                    } else if self.classifying {
                        ui.spinner();
                        ui.label(
                            RichText::new("正在识别插件类型…")
                                .size(11.0)
                                .color(theme::TEXT_DIM),
                        );
                    }
                });
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("类型").size(10.0).color(theme::TEXT_DIM));
                    for filter in [
                        PluginFilter::All,
                        PluginFilter::Instruments,
                        PluginFilter::Effects,
                        PluginFilter::Unknown,
                    ] {
                        if ui
                            .selectable_label(browser.kind_filter == filter, filter.label())
                            .clicked()
                        {
                            browser.kind_filter = filter;
                        }
                    }
                    ui.separator();
                    ui.add(
                        egui::TextEdit::singleline(&mut browser.filter)
                            .desired_width(220.0)
                            .hint_text("按名称或厂商过滤"),
                    );
                    ui.label(
                        RichText::new(format!("{} 个插件", self.plugin_cache.len()))
                            .size(10.0)
                            .color(theme::TEXT_FAINT),
                    );
                });

                ui.separator();

                if self.plugin_cache.is_empty() {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("尚未扫描到插件")
                                .size(14.0)
                                .color(theme::TEXT_DIM),
                        );
                    });
                } else {
                    let filter = browser.filter.to_lowercase();
                    let selected_kind = browser.kind_filter;
                    let mut groups: BTreeMap<String, Vec<&ScannedPlugin>> = BTreeMap::new();
                    for plugin in &self.plugin_cache {
                        if !matches_filter(plugin, selected_kind) {
                            continue;
                        }
                        if !filter.is_empty()
                            && !plugin.name.to_lowercase().contains(&filter)
                            && !plugin.vendor.to_lowercase().contains(&filter)
                        {
                            continue;
                        }
                        let vendor = if plugin.vendor.is_empty() {
                            "未分类".to_string()
                        } else {
                            plugin.vendor.clone()
                        };
                        groups.entry(vendor).or_default().push(plugin);
                    }

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(400.0)
                        .show(ui, |ui| {
                            if groups.is_empty() {
                                ui.add_space(20.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        RichText::new("当前筛选没有匹配的插件")
                                            .size(12.0)
                                            .color(theme::TEXT_DIM),
                                    );
                                });
                                return;
                            }
                            for (vendor, plugins) in groups {
                                let title =
                                    format!("{}  ·  {} 个", vendor, plugins.len());
                                egui::CollapsingHeader::new(
                                    RichText::new(title).size(12.0).color(theme::TEXT),
                                )
                                .default_open(true)
                                .show(ui, |ui| {
                                    for plugin in plugins {
                                        egui::Frame::NONE
                                            .fill(theme::PANEL_2)
                                            .corner_radius(6)
                                            .inner_margin(egui::Margin::symmetric(9, 6))
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.vertical(|ui| {
                                                        ui.label(
                                                            RichText::new(&plugin.name)
                                                                .strong()
                                                                .size(12.0)
                                                                .color(theme::TEXT),
                                                        );
                                                        let meta = if plugin.category.is_empty()
                                                        {
                                                            plugin
                                                                .path
                                                                .to_string_lossy()
                                                                .to_string()
                                                        } else {
                                                            plugin.category.clone()
                                                        };
                                                        ui.label(
                                                            RichText::new(meta)
                                                                .size(9.0)
                                                                .color(theme::TEXT_FAINT),
                                                        );
                                                    });
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            if ui.button("选择").clicked() {
                                                                chosen = Some((
                                                                    plugin.format,
                                                                    plugin.path.clone(),
                                                                    plugin.name.clone(),
                                                                    plugin.uid.clone(),
                                                                ));
                                                            }
                                                            if plugin.has_gui {
                                                                theme::badge(
                                                                    ui,
                                                                    "GUI",
                                                                    theme::ACCENT,
                                                                );
                                                            }
                                                            if plugin.has_midi_input {
                                                                theme::badge(
                                                                    ui,
                                                                    "MIDI",
                                                                    theme::VIOLET,
                                                                );
                                                            }
                                                            theme::badge(
                                                                ui,
                                                                plugin.format.label(),
                                                                if plugin.format
                                                                    == PluginFormat::Vst3
                                                                {
                                                                    theme::ACCENT
                                                                } else {
                                                                    theme::AMBER
                                                                },
                                                            );
                                                            theme::badge(
                                                                ui,
                                                                plugin.kind.label(),
                                                                match plugin.kind {
                                                                    PluginKind::Instrument => {
                                                                        theme::VIOLET
                                                                    }
                                                                    PluginKind::Effect => {
                                                                        theme::AMBER
                                                                    }
                                                                    PluginKind::Unknown => {
                                                                        theme::TEXT_FAINT
                                                                    }
                                                                },
                                                            );
                                                        },
                                                    );
                                                });
                                            });
                                        ui.add_space(3.0);
                                    }
                                });
                            }
                        });
                }

                ui.add_space(4.0);
                ui.separator();
                ui.label(
                    RichText::new(
                        "VST3 扫描只读取 moduleinfo.json；VST2/VST1 扫描只列出 64 位 DLL，选中后才加载并识别类型。",
                    )
                    .size(9.0)
                    .color(theme::TEXT_FAINT),
                );
            });

        if let Some((format, path, name, uid)) = chosen {
            self.apply_plugin(browser.target, format, path, name, uid);
        } else if open {
            self.browser = Some(browser);
        }
    }
}

fn matches_filter(plugin: &ScannedPlugin, filter: PluginFilter) -> bool {
    // 类型筛选严格匹配：未知类型只在“未知”里出现，避免把效果器当成乐器列出。
    // Type filters match strictly: unknown plugins only appear under “unknown”, so
    // effects are never listed as instruments.
    match filter {
        PluginFilter::All => true,
        PluginFilter::Instruments => plugin.kind == PluginKind::Instrument,
        PluginFilter::Effects => plugin.kind == PluginKind::Effect,
        PluginFilter::Unknown => plugin.kind == PluginKind::Unknown,
    }
}
