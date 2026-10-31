//! 所属模块：ui / 导出窗口
//! 文件用途：
//! - 绘制独立导出窗口：摘要、格式、采样与声道、范围、选项与目标路径。
//! - 桌面使用独立视口，单窗口平台（安卓）退回窗口内模态面板。
//! - 只读取/修改 `ExportState` 并调用 `start_export` / `export_pick_path`，不直接做渲染或文件 IO。
//!
//! 函数清单：
//! - `SurfingApp::ui_export_window` / `ui_export_modal`：创建导出视口或模态面板。
//! - `SurfingApp::ui_export_content` / `ui_export_footer`：内容区滚动布局与底部操作栏。
//! - `SurfingApp::export_summary` / `export_duration`：摘要卡片与时长计算。
//! - `export_title_bar` / `export_title_button`：自绘标题栏与窗口按钮。
//! - `export_section` / `export_field_label` / `export_combo` / `export_choice`：卡片、字段与选项控件。
//!
//! Module: ui / export window
//! Purpose:
//! - Paint the standalone export window: summary, format, sampling/channels, range, options and
//!   destination.
//! - Desktops use a separate viewport; single-window platforms (Android) fall back to a modal panel.
//! - Only reads/writes `ExportState` and calls `start_export` / `export_pick_path`; it never renders
//!   or touches files directly.
//!
//! Functions:
//! - `SurfingApp::ui_export_window` / `ui_export_modal`: create the export viewport or modal panel.
//! - `SurfingApp::ui_export_content` / `ui_export_footer`: scrolling content and bottom action bar.
//! - `SurfingApp::export_summary` / `export_duration`: summary card and duration helper.
//! - `export_title_bar` / `export_title_button`: custom title bar and window buttons.
//! - `export_section` / `export_field_label` / `export_combo` / `export_choice`: cards and controls.

use crate::app::{ExportChannels, ExportFormat, SurfingApp};
use crate::audio::engine::RenderRange;
use crate::audio::io::AudioBitDepth;
use crate::ui::theme;
use eframe::egui;

/// 导出窗口标题。
/// Export window title.
const EXPORT_WINDOW_TITLE: &str = "导出音频 · SurfingStudio";
/// 导出窗口默认宽度（点）。
/// Default export-window width in points.
const EXPORT_WINDOW_W: f32 = 620.0;
/// 导出窗口默认高度（点）。
/// Default export-window height in points.
const EXPORT_WINDOW_H: f32 = 690.0;
/// 导出窗口最小尺寸（点）。
/// Minimum export-window size in points.
const EXPORT_MIN_W: f32 = 520.0;
const EXPORT_MIN_H: f32 = 560.0;
/// 自绘标题栏高度（点）。
/// Custom title-bar height in points.
const EXPORT_TITLE_H: f32 = 40.0;
/// 底部操作栏高度（点）。
/// Bottom action-bar height in points.
const EXPORT_FOOTER_H: f32 = 104.0;
/// 标题栏窗口按钮尺寸（点）。
/// Title-bar window button size in points.
const EXPORT_TITLE_BUTTON: [f32; 2] = [34.0, 26.0];
/// 选项卡片高度（点）。
/// Choice-card height in points.
const EXPORT_CHOICE_H: f32 = 54.0;
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

/// 自绘标题栏按钮的图标。
/// Icon of a custom title-bar button.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ExportTitleIcon {
    Close,
    Minimize,
}

impl SurfingApp {
    /// 所属模块：ui / 导出窗口
    /// 功能：绘制独立导出窗口；桌面使用视口，单窗口平台使用模态面板。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：读写 `export.open` / `export.focus`；关闭时取消仍在渲染的导出。
    /// 复杂度：每帧 O(导出控件数)。
    ///
    /// Module: ui / export window
    /// Purpose: Draw the standalone export window; desktops use a viewport and single-window
    /// platforms use a modal panel.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: reads/writes `export.open`/`export.focus`; closing cancels a render.
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
                if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
                    open = false;
                }
                ui.painter()
                    .rect_filled(ui.max_rect(), egui::CornerRadius::from(0), theme::PANEL);
                if export_title_bar(ui) {
                    open = false;
                }
                if open {
                    self.ui_export_content(ui);
                }
            },
        );
        if !open {
            self.close_export_window();
        }
    }

    /// 所属模块：ui / 导出窗口
    /// 功能：在单窗口平台把导出窗口绘制成根窗口内的模态面板。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：读写 `export.open`；点击遮罩、Esc 或关闭按钮结束面板并取消渲染。
    /// 复杂度：每帧 O(导出控件数)。
    ///
    /// Module: ui / export window
    /// Purpose: On single-window platforms draw the export window as a modal panel in the root window.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: reads/writes `export.open`; a backdrop click, Esc or the close button
    /// closes the panel and cancels the render.
    /// Complexity: O(export widgets) per frame.
    fn ui_export_modal(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let screen = ctx.screen_rect();
        let width = EXPORT_WINDOW_W.min((screen.width() - 32.0).max(320.0));
        let height = EXPORT_WINDOW_H.min((screen.height() - 32.0).max(400.0));
        let response = egui::Modal::new(egui::Id::new("export_modal")).show(ctx, |ui| {
            ui.set_width(width);
            ui.set_height(height);
            ui.painter()
                .rect_filled(ui.max_rect(), egui::CornerRadius::from(0), theme::PANEL);
            if export_title_bar(ui) {
                open = false;
            }
            if open {
                self.ui_export_content(ui);
            }
        });
        if response.should_close() {
            open = false;
        }
        if !open {
            self.close_export_window();
        }
    }

    /// 所属模块：ui / 导出窗口
    /// 功能：关闭导出窗口；仍在逐帧渲染时先取消并恢复实时引擎。
    /// 输入：`self`。
    /// 输出/副作用：可能恢复引擎状态；`export.open = false`。
    /// 复杂度：时间 O(1)（取消时另有工程同步），空间 O(1)。
    ///
    /// Module: ui / export window
    /// Purpose: Close the export window; cancel and restore the live engine when a step render is
    /// still running.
    /// Inputs: `self`.
    /// Outputs / side effects: may restore the engine; sets `export.open = false`.
    /// Complexity: O(1) time (plus a project sync when cancelling) and O(1) space.
    pub(crate) fn close_export_window(&mut self) {
        if self.export.is_rendering() {
            self.cancel_export();
        }
        self.export.open = false;
    }

    /// 所属模块：ui / 导出窗口
    /// 功能：绘制导出内容区（可滚动）与固定底部操作栏。
    /// 输入：`ui`：窗口内容区。
    /// 输出/副作用：修改导出状态；可能打开路径对话框、启动或取消导出。
    /// 复杂度：每帧 O(导出控件数)。
    ///
    /// Module: ui / export window
    /// Purpose: Draw the scrolling export content and the fixed bottom action bar.
    /// Inputs: `ui`: window content area.
    /// Outputs / side effects: edits the export state; may open the path dialog, start or cancel an
    /// export.
    /// Complexity: O(export widgets) per frame.
    fn ui_export_content(&mut self, ui: &mut egui::Ui) {
        let busy = self.export.busy;
        let content_height = (ui.available_height() - EXPORT_FOOTER_H).max(140.0);
        ui.allocate_ui(egui::vec2(ui.available_width(), content_height), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("export_content")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.vertical(|ui| {
                            ui.set_width(ui.available_width() - 16.0);
                            self.export_summary(ui);
                            ui.add_space(10.0);
                            ui.add_enabled_ui(!busy, |ui| {
                                self.export_format_card(ui);
                                ui.add_space(10.0);
                                self.export_channels_card(ui);
                                ui.add_space(10.0);
                                self.export_range_card(ui);
                                ui.add_space(10.0);
                                self.export_options_card(ui);
                                ui.add_space(10.0);
                                self.export_destination_card(ui);
                            });
                            ui.add_space(10.0);
                        });
                    });
                });
        });
        ui.add_space(4.0);
        self.export_footer(ui);
    }

    /// 所属模块：ui / 导出窗口
    /// 功能：绘制顶部摘要卡片：工程名、格式摘要与实际输出时长。
    /// 输入：`ui`：内容区。
    /// 输出/副作用：只绘制文本；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ui / export window
    /// Purpose: Draw the summary card: project name, format summary and output duration.
    /// Inputs: `ui`: content area.
    /// Outputs / side effects: paints text only; no state change.
    /// Complexity: O(1) time and space.
    fn export_summary(&self, ui: &mut egui::Ui) {
        let layout = self.export.channels.resolve(self.project.output_layout);
        let summary = format!(
            "{} · {} kHz · {} · {}",
            self.export.format.label(),
            self.export.sample_rate as f32 / 1000.0,
            self.export.bit_depth.label(),
            layout.label()
        );
        let name = if self.project.name.trim().is_empty() {
            "未命名工程"
        } else {
            self.project.name.as_str()
        };
        let duration = export_duration_label(self.export_duration_seconds());
        egui::Frame::NONE
            .fill(theme::with_alpha(theme::ACCENT, 26))
            .stroke(egui::Stroke::new(
                1.0_f32,
                theme::with_alpha(theme::ACCENT, 96),
            ))
            .corner_radius(8)
            .inner_margin(egui::Margin::symmetric(16, 14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(name)
                                .size(15.0)
                                .strong()
                                .color(theme::TEXT),
                        );
                        ui.add_space(3.0);
                        ui.label(
                            egui::RichText::new(summary)
                                .size(11.0)
                                .color(theme::TEXT_DIM),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(duration)
                                    .size(17.0)
                                    .strong()
                                    .color(theme::ACCENT),
                            );
                            ui.label(
                                egui::RichText::new("输出时长")
                                    .size(10.0)
                                    .color(theme::TEXT_FAINT),
                            );
                        });
                    });
                });
            });
    }

    /// 所属模块：ui / 导出窗口
    /// 功能：绘制格式与质量卡片（容器、位深、采样率）。
    /// 输入：`ui`：内容区。
    /// 输出/副作用：修改导出格式相关字段；容器变化时同步扩展名。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ui / export window
    /// Purpose: Draw the format card (container, bit depth, sample rate).
    /// Inputs: `ui`: content area.
    /// Outputs / side effects: edits the format fields and keeps the path extension in sync.
    /// Complexity: O(1) time and space.
    fn export_format_card(&mut self, ui: &mut egui::Ui) {
        export_section(ui, "格式与质量", |ui| {
            ui.horizontal(|ui| {
                let column = (ui.available_width() - 14.0) / 2.0;
                ui.vertical(|ui| {
                    ui.set_width(column);
                    export_field_label(ui, "容器");
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
                });
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.set_width(column);
                    export_field_label(ui, "位深");
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
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let column = (ui.available_width() - 14.0) / 2.0;
                ui.vertical(|ui| {
                    ui.set_width(column);
                    export_field_label(ui, "采样率");
                    let mut rate = self.export.sample_rate;
                    export_combo(ui, "export_rate", format!("{rate} Hz"), |ui| {
                        for option in SAMPLE_RATES {
                            ui.selectable_value(&mut rate, option, format!("{option} Hz"));
                        }
                    });
                    if rate != self.export.sample_rate {
                        self.export.sample_rate = rate;
                    }
                });
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.set_width(column);
                    export_field_label(ui, "编码");
                    ui.label(
                        egui::RichText::new(match self.export.format {
                            ExportFormat::Wav => "无压缩 PCM / 浮点",
                            ExportFormat::Aiff => "无压缩 PCM",
                        })
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                    );
                });
            });
            if self.export.format == ExportFormat::Aiff
                && self.export.bit_depth == AudioBitDepth::Float32
            {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("AIFF 不支持浮点，将按 24-bit PCM 写出。")
                        .color(theme::AMBER)
                        .size(10.5),
                );
            }
        });
    }

    /// 所属模块：ui / 导出窗口
    /// 功能：绘制声道布局卡片：跟随工程 / 立体声 / 5.1 / 7.1。
    /// 输入：`ui`：内容区。
    /// 输出/副作用：修改 `export.channels`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ui / export window
    /// Purpose: Draw the channel-layout card: follow project / stereo / 5.1 / 7.1.
    /// Inputs: `ui`: content area.
    /// Outputs / side effects: edits `export.channels`.
    /// Complexity: O(1) time and space.
    fn export_channels_card(&mut self, ui: &mut egui::Ui) {
        let project_layout = self.project.output_layout;
        export_section(ui, "声道布局", |ui| {
            let spacing = ui.spacing().item_spacing.x;
            let width = ((ui.available_width() - spacing * 3.0) / 4.0).max(84.0);
            ui.horizontal(|ui| {
                for option in ExportChannels::ALL {
                    let layout = option.resolve(project_layout);
                    if export_choice(
                        ui,
                        width,
                        self.export.channels == option,
                        option.label(),
                        &format!("{} 声道", layout.channels()),
                    )
                    .clicked()
                    {
                        self.export.channels = option;
                    }
                }
            });
        });
    }

    /// 所属模块：ui / 导出窗口
    /// 功能：绘制渲染范围卡片：整个工程 / 循环区间与尾音长度。
    /// 输入：`ui`：内容区。
    /// 输出/副作用：修改 `export.range` 与 `export.tail_beats`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ui / export window
    /// Purpose: Draw the render-range card: whole project / loop range and the tail length.
    /// Inputs: `ui`: content area.
    /// Outputs / side effects: edits `export.range` and `export.tail_beats`.
    /// Complexity: O(1) time and space.
    fn export_range_card(&mut self, ui: &mut egui::Ui) {
        let project_duration =
            export_duration_label(self.range_duration_seconds(RenderRange::Project));
        let loop_duration = export_duration_label(self.range_duration_seconds(RenderRange::Loop));
        let loop_bounds = format!(
            "{:.2} – {:.2} beat",
            self.project.loop_start.max(0.0),
            self.project
                .loop_end
                .max(self.project.loop_start.max(0.0) + 0.25)
        );
        export_section(ui, "渲染范围", |ui| {
            let spacing = ui.spacing().item_spacing.x;
            let width = ((ui.available_width() - spacing) / 2.0).max(140.0);
            ui.horizontal(|ui| {
                if export_choice(
                    ui,
                    width,
                    self.export.range == RenderRange::Project,
                    "整个工程",
                    &format!("{project_duration} · 含尾音"),
                )
                .clicked()
                {
                    self.export.range = RenderRange::Project;
                }
                if export_choice(
                    ui,
                    width,
                    self.export.range == RenderRange::Loop,
                    "循环区间",
                    &format!("{loop_duration} · {loop_bounds}"),
                )
                .clicked()
                {
                    self.export.range = RenderRange::Loop;
                }
            });
            ui.add_space(8.0);
            ui.add_enabled_ui(self.export.range == RenderRange::Project, |ui| {
                ui.horizontal(|ui| {
                    export_field_label(ui, "尾音");
                    ui.add(
                        egui::DragValue::new(&mut self.export.tail_beats)
                            .range(0.0..=16.0)
                            .speed(0.25)
                            .suffix(" beat"),
                    );
                    ui.label(
                        egui::RichText::new("保留混响与延音的收尾")
                            .size(10.5)
                            .color(theme::TEXT_FAINT),
                    );
                });
            });
        });
    }

    /// 所属模块：ui / 导出窗口
    /// 功能：绘制渲染选项卡片：GPU 离线软限幅。
    /// 输入：`ui`：内容区。
    /// 输出/副作用：修改 `export.use_gpu`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ui / export window
    /// Purpose: Draw the render-options card: GPU offline soft limiter.
    /// Inputs: `ui`: content area.
    /// Outputs / side effects: edits `export.use_gpu`.
    /// Complexity: O(1) time and space.
    fn export_options_card(&mut self, ui: &mut egui::Ui) {
        export_section(ui, "渲染选项", |ui| {
            ui.horizontal(|ui| {
                theme::toggle_switch(ui, &mut self.export.use_gpu);
                ui.add_space(2.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("使用 GPU 执行主总线软限幅")
                            .size(11.5)
                            .color(theme::TEXT),
                    );
                    ui.label(
                        egui::RichText::new(if self.settings.gpu_audio_enabled {
                            "设置中已启用 GPU 音频；不可用时自动回退 CPU"
                        } else {
                            "不可用时自动回退 CPU，导出结果不受影响"
                        })
                        .size(10.5)
                        .color(theme::TEXT_FAINT),
                    );
                });
            });
        });
    }

    /// 所属模块：ui / 导出窗口
    /// 功能：绘制目标路径卡片：路径输入与系统文件对话框。
    /// 输入：`ui`：内容区。
    /// 输出/副作用：修改 `export.path`；可能打开后台保存对话框。
    /// 复杂度：时间 O(1)，空间 O(路径长度)。
    ///
    /// Module: ui / export window
    /// Purpose: Draw the destination card: path input and the system file dialog.
    /// Inputs: `ui`: content area.
    /// Outputs / side effects: edits `export.path`; may open the background save dialog.
    /// Complexity: O(1) time and O(path length) space.
    fn export_destination_card(&mut self, ui: &mut egui::Ui) {
        export_section(ui, "导出到", |ui| {
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.export.path)
                        .desired_width((ui.available_width() - 84.0).max(120.0))
                        .hint_text("选择导出的文件路径"),
                );
                if response.lost_focus() {
                    self.export.ensure_extension();
                }
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("浏览…").size(11.5))
                            .min_size(egui::vec2(72.0, 24.0)),
                    )
                    .clicked()
                {
                    self.export_pick_path();
                }
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(if crate::platform::caps().native_file_dialog {
                    "导出完成后会覆盖同名文件；目录不存在时自动创建。"
                } else {
                    "当前平台没有系统文件对话框，将写入应用工程目录。"
                })
                .size(10.5)
                .color(theme::TEXT_FAINT),
            );
        });
    }

    /// 所属模块：ui / 导出窗口
    /// 功能：绘制固定底部操作栏：状态、进度条与开始/取消/关闭按钮。
    /// 输入：`ui`：窗口内容区。
    /// 输出/副作用：可能启动、取消或关闭导出。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: ui / export window
    /// Purpose: Draw the fixed bottom bar: status, progress and start/cancel/close buttons.
    /// Inputs: `ui`: window content area.
    /// Outputs / side effects: may start, cancel or close an export.
    /// Complexity: O(1) time and space.
    fn export_footer(&mut self, ui: &mut egui::Ui) {
        let busy = self.export.busy;
        let rendering = self.export.is_rendering();
        ui.separator();
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            // 左侧状态/进度占用剩余宽度，右侧固定留给按钮，避免进度条把按钮挤出窗口。
            // The left status/progress takes the remaining width; the right side is reserved for
            // the buttons so the progress bar never pushes them out of the window.
            let buttons_width = 340.0;
            ui.vertical(|ui| {
                ui.set_width((ui.available_width() - buttons_width).max(180.0));
                let (status, color) = export_status_text(&self.export.status);
                ui.add(
                    egui::Label::new(egui::RichText::new(status).size(11.0).color(color))
                        .truncate(),
                )
                .on_hover_text(status);
                if busy {
                    ui.add_space(5.0);
                    export_progress_bar(ui, self.export.progress);
                    ui.add_space(3.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "{:.0}%",
                            (self.export.progress * 100.0).clamp(0.0, 100.0)
                        ))
                        .size(10.0)
                        .color(theme::TEXT_FAINT),
                    );
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(16.0);
                let start = egui::Button::new(
                    egui::RichText::new("开始导出")
                        .color(egui::Color32::WHITE)
                        .size(12.0),
                )
                .fill(theme::ACCENT)
                .min_size(egui::vec2(112.0, 30.0));
                if ui.add_enabled(!busy, start).clicked() {
                    self.start_export();
                }
                if rendering {
                    ui.add_space(6.0);
                    let cancel = egui::Button::new(
                        egui::RichText::new("取消")
                            .size(11.5)
                            .color(theme::TEXT_DIM),
                    )
                    .min_size(egui::vec2(76.0, 28.0));
                    if ui.add(cancel).clicked() {
                        self.cancel_export();
                    }
                }
                ui.add_space(6.0);
                let close = egui::Button::new(
                    egui::RichText::new("关闭")
                        .size(11.5)
                        .color(theme::TEXT_DIM),
                )
                .min_size(egui::vec2(76.0, 28.0));
                if ui.add(close).clicked() {
                    self.close_export_window();
                }
            });
        });
    }

    /// 所属模块：ui / 导出窗口
    /// 功能：返回当前导出范围的时长（秒）。
    /// 输入：`self`。
    /// 输出/副作用：时长；不修改状态。
    /// 复杂度：时间 O(片段数)，空间 O(1)。
    ///
    /// Module: ui / export window
    /// Purpose: Return the duration (seconds) of the current export range.
    /// Inputs: `self`.
    /// Outputs / side effects: the duration; no state change.
    /// Complexity: O(clip count) time and O(1) space.
    fn export_duration_seconds(&self) -> f64 {
        self.range_duration_seconds(self.export.range)
    }

    /// 所属模块：ui / 导出窗口
    /// 功能：按给定范围计算时长（秒）；循环区间忽略尾音。
    /// 输入：`range`：渲染范围。
    /// 输出/副作用：时长；不修改状态。
    /// 复杂度：时间 O(片段数)，空间 O(1)。
    ///
    /// Module: ui / export window
    /// Purpose: Compute the duration (seconds) for a range; the loop range ignores the tail.
    /// Inputs: `range`: render range.
    /// Outputs / side effects: the duration; no state change.
    /// Complexity: O(clip count) time and O(1) space.
    fn range_duration_seconds(&self, range: RenderRange) -> f64 {
        let (start, end) = match range {
            RenderRange::Project => (
                0.0,
                self.project.content_end_beat().max(4.0) + self.export.tail_beats.max(0.0),
            ),
            RenderRange::Loop => {
                let start = self.project.loop_start.max(0.0);
                (start, self.project.loop_end.max(start + 0.25))
            }
        };
        ((end - start).max(0.0) * 60.0 / self.project.bpm.max(1.0)).max(0.0)
    }
}

/// 所属模块：ui / 导出窗口
/// 功能：绘制导出窗口的自绘标题栏；标题区域可拖动整个窗口。
/// 输入：`ui`：窗口内容区。
/// 输出/副作用：返回是否点击了关闭按钮；可能发送窗口拖动/最小化命令。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / export window
/// Purpose: Draw the custom title bar; dragging the title area moves the whole window.
/// Inputs: `ui`: window content area.
/// Outputs / side effects: whether the close button was clicked; may send drag/minimize commands.
/// Complexity: O(1) time and space.
fn export_title_bar(ui: &mut egui::Ui) -> bool {
    let (rect, drag) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), EXPORT_TITLE_H),
        egui::Sense::click_and_drag(),
    );
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::from(0), theme::PANEL);
    painter.rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(3.0, EXPORT_TITLE_H)),
        egui::CornerRadius::from(0),
        theme::ACCENT,
    );
    painter.text(
        egui::pos2(rect.left() + 16.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        "导出音频",
        egui::FontId::proportional(14.5),
        theme::TEXT,
    );
    painter.text(
        egui::pos2(rect.left() + 90.0, rect.center().y + 1.0),
        egui::Align2::LEFT_CENTER,
        "SurfingStudio",
        egui::FontId::proportional(10.5),
        theme::TEXT_FAINT,
    );
    painter.hline(
        rect.left()..=rect.right(),
        rect.bottom(),
        egui::Stroke::new(1.0_f32, theme::APP_BAR_LINE),
    );

    let [button_w, button_h] = EXPORT_TITLE_BUTTON;
    let button_top = rect.center().y - button_h / 2.0;
    let close_rect = egui::Rect::from_min_size(
        egui::pos2(rect.right() - 6.0 - button_w, button_top),
        egui::vec2(button_w, button_h),
    );
    let minimize_rect = egui::Rect::from_min_size(
        egui::pos2(close_rect.left() - 2.0 - button_w, button_top),
        egui::vec2(button_w, button_h),
    );
    let close = export_title_button(ui, close_rect, ExportTitleIcon::Close, "关闭");
    if export_title_button(ui, minimize_rect, ExportTitleIcon::Minimize, "最小化") {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
    close
}

/// 所属模块：ui / 导出窗口
/// 功能：绘制一个标题栏按钮（悬停底色 + 细线图标），关闭按钮使用红色悬停。
/// 输入：
/// - `ui`：窗口内容区。
/// - `rect`：按钮矩形。
/// - `icon`：关闭或最小化。
/// - `tooltip`：悬停提示。
/// 输出/副作用：返回是否被点击；不修改其他状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / export window
/// Purpose: Draw one title-bar button (hover fill + thin icon); close uses a red hover.
/// Inputs:
/// - `ui`: window content area.
/// - `rect`: button rect.
/// - `icon`: close or minimize.
/// - `tooltip`: hover text.
/// Outputs / side effects: whether it was clicked; no other state change.
/// Complexity: O(1) time and space.
fn export_title_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    icon: ExportTitleIcon,
    tooltip: &str,
) -> bool {
    let response = ui.interact(
        rect,
        ui.id().with(("export_title_button", icon as u8)),
        egui::Sense::click(),
    );
    let hovered = response.hovered();
    let fill = match (icon, hovered) {
        (ExportTitleIcon::Close, true) => theme::CLOSE_HOVER,
        (_, true) => theme::APP_BAR_HOVER,
        (_, false) => egui::Color32::TRANSPARENT,
    };
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::from(4), fill);
    let color = if hovered && icon == ExportTitleIcon::Close {
        egui::Color32::WHITE
    } else {
        theme::TEXT_DIM
    };
    let center = rect.center();
    let stroke = egui::Stroke::new(1.2_f32, color);
    match icon {
        ExportTitleIcon::Close => {
            let half = 4.5;
            painter.line_segment(
                [
                    egui::pos2(center.x - half, center.y - half),
                    egui::pos2(center.x + half, center.y + half),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x + half, center.y - half),
                    egui::pos2(center.x - half, center.y + half),
                ],
                stroke,
            );
        }
        ExportTitleIcon::Minimize => {
            painter.line_segment(
                [
                    egui::pos2(center.x - 5.0, center.y),
                    egui::pos2(center.x + 5.0, center.y),
                ],
                stroke,
            );
        }
    }
    response.on_hover_text(tooltip).clicked()
}

/// 所属模块：ui / 导出窗口
/// 功能：绘制一个导出分区卡片（标题 + 内容），与设置页分区保持一致。
/// 输入：
/// - `ui`：内容区。
/// - `title`：分区标题。
/// - `add`：分区内容闭包。
/// 输出/副作用：绘制卡片；不修改状态（闭包内自行处理）。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / export window
/// Purpose: Draw one export section card (title + content), matching the settings sections.
/// Inputs:
/// - `ui`: content area.
/// - `title`: section title.
/// - `add`: closure that draws the content.
/// Outputs / side effects: paints the card; no state change (the closure owns its changes).
/// Complexity: O(1) time and space.
fn export_section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(theme::PANEL)
        .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(title)
                    .size(12.5)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.add_space(8.0);
            add(ui);
        });
}

/// 所属模块：ui / 导出窗口
/// 功能：绘制固定宽度的小号字段标签。
/// 输入：
/// - `ui`：内容区。
/// - `text`：字段名。
/// 输出/副作用：绘制文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / export window
/// Purpose: Draw a small fixed-width field label.
/// Inputs:
/// - `ui`: content area.
/// - `text`: field name.
/// Outputs / side effects: paints text; no state change.
/// Complexity: O(1) time and space.
fn export_field_label(ui: &mut egui::Ui, text: &str) {
    ui.add_space(1.0);
    ui.label(
        egui::RichText::new(text)
            .size(10.5)
            .color(theme::TEXT_FAINT),
    );
    ui.add_space(2.0);
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
        .width(ui.available_width().max(120.0))
        .selected_text(selected)
        .show_ui(ui, body);
}

/// 所属模块：ui / 导出窗口
/// 功能：绘制一个可点击的选项卡片（标题 + 副标题），选中时使用强调色。
/// 输入：
/// - `ui`：父界面。
/// - `width`：卡片宽度（点）。
/// - `selected`：是否为当前选项。
/// - `title`：主文本。
/// - `subtitle`：副文本。
/// 输出/副作用：返回响应；点击由调用方写入状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / export window
/// Purpose: Draw one clickable choice card (title + subtitle); the selected one uses the accent.
/// Inputs:
/// - `ui`: parent UI.
/// - `width`: card width in points.
/// - `selected`: whether this option is active.
/// - `title`: primary text.
/// - `subtitle`: secondary text.
/// Outputs / side effects: the response; the caller stores the click.
/// Complexity: O(1) time and space.
fn export_choice(
    ui: &mut egui::Ui,
    width: f32,
    selected: bool,
    title: &str,
    subtitle: &str,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, EXPORT_CHOICE_H), egui::Sense::click());
    let hovered = response.hovered();
    let fill = if selected {
        theme::with_alpha(theme::ACCENT, 44)
    } else if hovered {
        theme::PANEL_3
    } else {
        theme::PANEL_2
    };
    let stroke_color = if selected {
        theme::ACCENT
    } else if hovered {
        theme::TEXT_FAINT
    } else {
        theme::LINE
    };
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::from(6), fill);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::from(6),
        egui::Stroke::new(1.0_f32, stroke_color),
        egui::StrokeKind::Inside,
    );
    painter.text(
        egui::pos2(rect.center().x, rect.top() + 20.0),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(12.0),
        if selected {
            theme::TEXT
        } else {
            theme::TEXT_DIM
        },
    );
    painter.text(
        egui::pos2(rect.center().x, rect.top() + 38.0),
        egui::Align2::CENTER_CENTER,
        subtitle,
        egui::FontId::proportional(9.5),
        theme::TEXT_FAINT,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// 所属模块：ui / 导出窗口
/// 功能：绘制细进度条（6 点高、强调色填充）。
/// 输入：
/// - `ui`：内容区。
/// - `progress`：0..1 的进度。
/// 输出/副作用：绘制图形；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / export window
/// Purpose: Draw a thin progress bar (6 points high, accent fill).
/// Inputs:
/// - `ui`: content area.
/// - `progress`: 0..1 progress.
/// Outputs / side effects: paints shapes; no state change.
/// Complexity: O(1) time and space.
fn export_progress_bar(ui: &mut egui::Ui, progress: f32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 6.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::from(3), theme::BG_DEEP);
    let filled_width = rect.width() * progress.clamp(0.0, 1.0);
    if filled_width > 0.0 {
        let filled = egui::Rect::from_min_size(rect.min, egui::vec2(filled_width, rect.height()));
        painter.rect_filled(filled, egui::CornerRadius::from(3), theme::ACCENT);
    }
}

/// 所属模块：ui / 导出窗口
/// 功能：把秒数格式化为 `m:ss`。
/// 输入：`seconds`：时长（秒）。
/// 输出/副作用：文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / export window
/// Purpose: Format seconds as `m:ss`.
/// Inputs: `seconds`: duration in seconds.
/// Outputs / side effects: the text; no state change.
/// Complexity: O(1) time and space.
fn export_duration_label(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

/// 所属模块：ui / 导出窗口
/// 功能：根据导出状态文本返回展示文本与颜色。
/// 输入：`status`：状态文本。
/// 输出/副作用：`(文本, 颜色)`；不修改状态。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: ui / export window
/// Purpose: Map the export status text to its display text and color.
/// Inputs: `status`: status text.
/// Outputs / side effects: `(text, color)`; no state change.
/// Complexity: O(status length) time and O(1) space.
fn export_status_text(status: &str) -> (&str, egui::Color32) {
    if status.is_empty() {
        return ("准备就绪，选择参数后开始导出", theme::TEXT_FAINT);
    }
    if status.contains("失败") {
        (status, theme::RED)
    } else if status.starts_with("已导出") {
        (status, theme::GREEN)
    } else if status.contains("正在") {
        (status, theme::TEXT_DIM)
    } else if status.contains("取消") {
        (status, theme::AMBER)
    } else {
        (status, theme::TEXT_DIM)
    }
}
