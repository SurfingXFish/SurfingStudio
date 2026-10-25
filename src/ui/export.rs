//! 所属模块：ui / 导出窗口
//! 文件用途：
//! - 绘制独立导出窗口：格式、采样率、位深、声道布局、渲染范围、尾音、GPU 与目标路径。
//! - 桌面使用独立视口，单窗口平台（安卓）退回窗口内模态面板。
//! - 只读取/修改 `ExportState` 并调用 `start_export` / `export_pick_path`，不直接做渲染或文件 IO。
//!
//! 函数清单：
//! - `SurfingApp::ui_export_window` / `ui_export_modal`：创建出口窗口或模态面板。
//! - `export_viewport_id` / `export_title_bar`：视口 id 与自绘标题栏。
//! - `SurfingApp::ui_export_content` / `export_combo`：参数表单与紧凑下拉框。
//!
//! Module: ui / export window
//! Purpose:
//! - Paint the standalone export window: format, sample rate, bit depth, channel layout, render
//!   range, tail, GPU and destination.
//! - Desktops use a separate viewport; single-window platforms (Android) fall back to a modal panel.
//! - Only reads/writes `ExportState` and calls `start_export` / `export_pick_path`; it never renders
//!   or touches files directly.
//!
//! Functions:
//! - `SurfingApp::ui_export_window` / `ui_export_modal`: create the export viewport or modal panel.
//! - `export_viewport_id` / `export_title_bar`: viewport id and custom title bar.
//! - `SurfingApp::ui_export_content` / `export_combo`: options form and compact dropdown.

use crate::app::{ExportChannels, ExportFormat, SurfingApp};
use crate::audio::io::AudioBitDepth;
use crate::ui::theme;
use eframe::egui;

/// 导出窗口标题。
/// Export window title.
const EXPORT_WINDOW_TITLE: &str = "导出音频 · SurfingStudio";
/// 导出窗口默认宽度（点）。
/// Default export-window width in points.
const EXPORT_WINDOW_W: f32 = 580.0;
/// 导出窗口默认高度（点）。
/// Default export-window height in points.
const EXPORT_WINDOW_H: f32 = 560.0;
/// 导出窗口最小尺寸（点）。
/// Minimum export-window size in points.
const EXPORT_MIN_W: f32 = 480.0;
const EXPORT_MIN_H: f32 = 440.0;
/// 自绘标题栏高度（点）。
/// Custom title-bar height in points.
const EXPORT_TITLE_H: f32 = 38.0;
/// 导出采样率选项。
/// Export sample-rate options.
const SAMPLE_RATES: [u32; 5] = [44_100, 48_000, 88_200, 96_000, 192_000];

/// 所属模块：ui / 导出窗口
/// 功能：返回本窗口导出视口的稳定 id。
/// 输入：`window_id`：工程窗口编号。
/// 输出/副作用：视口 id；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / export window
/// Purpose: Return the stable viewport id of this window's export viewport.
/// Inputs: `window_id`: project-window id.
/// Outputs / side effects: the viewport id; no state change.
/// Complexity: O(1) time and space.
fn export_viewport_id(window_id: u64) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("surfing_export", window_id))
}

impl SurfingApp {
    /// 所属模块：ui / 导出窗口
    /// 功能：绘制独立导出窗口；桌面使用视口，单窗口平台使用模态面板。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：读写 `export.open` / `export.focus`；可能启动后台导出。
    /// 复杂度：每帧 O(导出控件数)。
    ///
    /// Module: ui / export window
    /// Purpose: Draw the standalone export window; desktops use a viewport and single-window
    /// platforms use a modal panel.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: reads/writes `export.open`/`export.focus`; may start a background export.
    /// Complexity: O(export widgets) per frame.
    pub fn ui_export_window(&mut self, ctx: &egui::Context) {
        if !self.export.open {
            return;
        }
        if !crate::platform::caps().multi_window {
            self.ui_export_modal(ctx);
            return;
        }
        let viewport_id = export_viewport_id(self.window_id);
        if self.export.focus {
            self.export.focus = false;
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Focus);
        }
        let mut open = true;
        ctx.show_viewport_immediate(
            viewport_id,
            egui::ViewportBuilder::default()
                .with_title(EXPORT_WINDOW_TITLE)
                .with_inner_size([EXPORT_WINDOW_W, EXPORT_WINDOW_H])
                .with_min_inner_size([EXPORT_MIN_W, EXPORT_MIN_H])
                .with_resizable(true)
                // 与工程窗口一致：不显示系统标题栏，改由应用自绘标题栏。
                // Like project windows: no OS title bar; the app draws its own.
                .with_decorations(false),
            |ui, _class| {
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    open = false;
                    return;
                }
                ui.painter()
                    .rect_filled(ui.max_rect(), egui::CornerRadius::from(0), theme::PANEL);
                if export_title_bar(ui) {
                    open = false;
                }
                self.ui_export_content(ui);
            },
        );
        if !open {
            self.export.open = false;
        }
    }

    /// 所属模块：ui / 导出窗口
    /// 功能：在单窗口平台把导出窗口绘制成根窗口内的模态面板。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：读写 `export.open`；点击遮罩、Esc 或关闭按钮结束面板。
    /// 复杂度：每帧 O(导出控件数)。
    ///
    /// Module: ui / export window
    /// Purpose: On single-window platforms draw the export window as a modal panel in the root window.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: reads/writes `export.open`; a backdrop click, Esc or the close button
    /// closes it.
    /// Complexity: O(export widgets) per frame.
    fn ui_export_modal(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let screen = ctx.screen_rect();
        let width = EXPORT_WINDOW_W.min((screen.width() - 32.0).max(320.0));
        let height = EXPORT_WINDOW_H.min((screen.height() - 32.0).max(320.0));
        let response = egui::Modal::new(egui::Id::new("export_modal")).show(ctx, |ui| {
            ui.set_width(width);
            ui.set_height(height);
            ui.painter()
                .rect_filled(ui.max_rect(), egui::CornerRadius::from(0), theme::PANEL);
            if export_title_bar(ui) {
                open = false;
            }
            self.ui_export_content(ui);
        });
        if response.should_close() {
            open = false;
        }
        if !open {
            self.export.open = false;
        }
    }

    /// 所属模块：ui / 导出窗口
    /// 功能：绘制导出参数表单与底部操作按钮。
    /// 输入：`ui`：窗口内容区。
    /// 输出/副作用：修改导出状态；可能打开路径对话框或启动后台导出。
    /// 复杂度：每帧 O(导出控件数)。
    ///
    /// Module: ui / export window
    /// Purpose: Draw the export options form and the bottom action buttons.
    /// Inputs: `ui`: window content area.
    /// Outputs / side effects: edits the export state; may open the path dialog or start an export.
    /// Complexity: O(export widgets) per frame.
    fn ui_export_content(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("格式")
                        .color(theme::TEXT_DIM)
                        .size(11.0)
                        .strong(),
                );
                ui.horizontal(|ui| {
                    ui.label("容器");
                    let mut format = self.export.format;
                    export_combo(ui, "export_format", format.label(), |ui| {
                        for option in ExportFormat::ALL {
                            ui.selectable_value(&mut format, option, option.label());
                        }
                    });
                    if format != self.export.format {
                        self.export.format = format;
                        self.export.ensure_extension();
                    }
                    ui.add_space(14.0);
                    ui.label("位深");
                    let mut depth = self.export.bit_depth;
                    export_combo(ui, "export_depth", depth.label(), |ui| {
                        for option in AudioBitDepth::ALL {
                            ui.selectable_value(&mut depth, option, option.label());
                        }
                    });
                    if depth != self.export.bit_depth {
                        self.export.bit_depth = depth;
                    }
                });
                if self.export.format == ExportFormat::Aiff
                    && self.export.bit_depth == AudioBitDepth::Float32
                {
                    ui.label(
                        egui::RichText::new("AIFF 不支持浮点，将按 24-bit PCM 写出。")
                            .color(theme::AMBER)
                            .size(10.5),
                    );
                }

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("采样与声道")
                        .color(theme::TEXT_DIM)
                        .size(11.0)
                        .strong(),
                );
                ui.horizontal(|ui| {
                    ui.label("采样率");
                    let mut rate = self.export.sample_rate;
                    export_combo(ui, "export_rate", format!("{rate} Hz"), |ui| {
                        for option in SAMPLE_RATES {
                            ui.selectable_value(&mut rate, option, format!("{option} Hz"));
                        }
                    });
                    if rate != self.export.sample_rate {
                        self.export.sample_rate = rate;
                    }
                    ui.add_space(14.0);
                    ui.label("声道");
                    let mut channels = self.export.channels;
                    export_combo(ui, "export_channels", channels.label(), |ui| {
                        for option in ExportChannels::ALL {
                            ui.selectable_value(&mut channels, option, option.label());
                        }
                    });
                    if channels != self.export.channels {
                        self.export.channels = channels;
                    }
                });

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("范围与渲染")
                        .color(theme::TEXT_DIM)
                        .size(11.0)
                        .strong(),
                );
                ui.horizontal(|ui| {
                    ui.radio_value(
                        &mut self.export.range,
                        crate::audio::engine::RenderRange::Project,
                        "整个工程",
                    );
                    ui.radio_value(
                        &mut self.export.range,
                        crate::audio::engine::RenderRange::Loop,
                        "循环区间",
                    );
                    ui.add_space(14.0);
                    ui.add_enabled_ui(
                        self.export.range == crate::audio::engine::RenderRange::Project,
                        |ui| {
                            ui.label("尾音");
                            ui.add(
                                egui::DragValue::new(&mut self.export.tail_beats)
                                    .range(0.0..=16.0)
                                    .speed(0.25)
                                    .suffix(" beat"),
                            );
                        },
                    );
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.export.use_gpu, "使用 GPU 执行离线软限幅");
                    if self.settings.gpu_audio_enabled {
                        ui.label(
                            egui::RichText::new("（设置中已启用 GPU 音频）")
                                .color(theme::TEXT_FAINT)
                                .size(10.5),
                        );
                    }
                });

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("导出路径")
                        .color(theme::TEXT_DIM)
                        .size(11.0)
                        .strong(),
                );
                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.export.path)
                            .desired_width(ui.available_width() - 84.0)
                            .hint_text("选择导出的文件路径"),
                    );
                    if response.lost_focus() {
                        self.export.ensure_extension();
                    }
                    if ui.button("选择…").clicked() {
                        self.export_pick_path();
                    }
                });
                if !crate::platform::caps().native_file_dialog {
                    ui.label(
                        egui::RichText::new("当前平台没有系统文件对话框，将写入应用工程目录。")
                            .color(theme::TEXT_FAINT)
                            .size(10.5),
                    );
                }

                ui.add_space(8.0);
                ui.separator();
                if !self.export.status.is_empty() {
                    let color = if self.export.status.contains("失败") {
                        theme::RED
                    } else {
                        theme::TEXT_DIM
                    };
                    ui.label(
                        egui::RichText::new(&self.export.status)
                            .color(color)
                            .size(11.0),
                    );
                }
                ui.horizontal(|ui| {
                    let start = egui::Button::new(
                        egui::RichText::new("开始导出")
                            .color(egui::Color32::WHITE)
                            .size(12.0),
                    )
                    .fill(theme::ACCENT)
                    .min_size(egui::vec2(104.0, 28.0));
                    if ui.add_enabled(!self.export.busy, start).clicked() {
                        self.start_export();
                    }
                    if self.export.busy {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new("正在渲染…")
                                .color(theme::TEXT_DIM)
                                .size(11.0),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("关闭").clicked() {
                            self.export.open = false;
                        }
                    });
                });
                ui.add_space(6.0);
            });
    }
}

/// 所属模块：ui / 导出窗口
/// 功能：绘制导出窗口的自绘标题栏；标题区域可拖动整个窗口。
/// 输入：`ui`：窗口内容区。
/// 输出/副作用：返回是否点击了关闭按钮；可能发送窗口拖动命令。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / export window
/// Purpose: Draw the custom title bar; dragging the title area moves the whole window.
/// Inputs: `ui`: window content area.
/// Outputs / side effects: whether the close button was clicked; may send a window-drag command.
/// Complexity: O(1) time and space.
fn export_title_bar(ui: &mut egui::Ui) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), EXPORT_TITLE_H),
        egui::Sense::click_and_drag(),
    );
    if response.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    ui.painter().rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), EXPORT_TITLE_H)),
        egui::CornerRadius::from(0),
        theme::PANEL,
    );
    ui.painter().line_segment(
        [
            egui::pos2(rect.min.x, rect.min.y + EXPORT_TITLE_H),
            egui::pos2(rect.max.x, rect.min.y + EXPORT_TITLE_H),
        ],
        egui::Stroke::new(1.0_f32, theme::APP_BAR_LINE),
    );
    ui.painter().text(
        egui::pos2(rect.min.x + 12.0, rect.min.y + EXPORT_TITLE_H * 0.5),
        egui::Align2::LEFT_CENTER,
        "导出音频",
        egui::FontId::proportional(13.0),
        theme::TEXT,
    );
    let close = egui::Rect::from_center_size(
        egui::pos2(rect.max.x - 20.0, rect.min.y + EXPORT_TITLE_H * 0.5),
        egui::vec2(30.0, 28.0),
    );
    let close_response = ui.interact(
        close,
        ui.id().with("export_title_close"),
        egui::Sense::click(),
    );
    if close_response.clicked() {
        return true;
    }
    let fill = if close_response.hovered() {
        theme::CLOSE_HOVER
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(close, egui::CornerRadius::from(4), fill);
    ui.painter().text(
        close.center(),
        egui::Align2::CENTER_CENTER,
        "×",
        egui::FontId::proportional(16.0),
        theme::TEXT,
    );
    false
}

/// 所属模块：ui / 导出窗口
/// 功能：绘制固定宽度的紧凑下拉框。
/// 输入：
/// - `ui`：父界面。
/// - `id`：控件 id。
/// - `selected`：当前展示文本。
/// - `body`：下拉内容闭包。
/// 输出/副作用：绘制菜单；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / export window
/// Purpose: Draw a compact dropdown with a fixed width.
/// Inputs:
/// - `ui`: parent UI.
/// - `id`: widget id.
/// - `selected`: current label.
/// - `body`: dropdown body closure.
/// Outputs / side effects: paints the menu; no state change.
/// Complexity: O(1) time and space.
fn export_combo(
    ui: &mut egui::Ui,
    id: &str,
    selected: impl Into<egui::WidgetText>,
    body: impl FnOnce(&mut egui::Ui),
) {
    egui::ComboBox::from_id_salt(id)
        .width(150.0)
        .selected_text(selected)
        .show_ui(ui, body);
}
