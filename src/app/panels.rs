//! 所属模块：app / 面板
//! 文件用途：
//! - 自绘无边框窗口的应用栏（图标、主菜单、项目栏与窗口按钮）与下方工具栏。
//! - 底部状态栏、子轨覆盖确认弹窗与无边框窗口的边缘缩放热区。
//!
//! 函数清单：
//! - `SurfingApp::ui_lane_paste_prompt`：子轨覆盖确认弹窗。
//! - `SurfingApp::app_bar`：应用栏：拖动窗口、隐藏下拉栏、项目栏与窗口按钮。
//! - `SurfingApp::ui_main_menu`：主页 / 文件 / 编辑 / VST 插件 / 帮助 / 设置入口，返回菜单区域用于点击外部收起。
//! - `SurfingApp::main_toolbar`：工具栏：添加轨道、走带、BPM/拍号、循环起止与鼠标模式。
//! - `SurfingApp::window_resize_edges`：无边框窗口的边缘与角落缩放热区。
//! - `SurfingApp::status_bar`：状态、CPU、音频设备与 MIDI 指示。
//! - `apply_chrome_style` / `apply_menu_style`：IDEA 风格的应用栏与弹出菜单样式。
//! - `app_logo` / `hamburger_button` / `bar_text_button` / `project_chip` / `icon_button` / `window_control`：自绘控件。
//! - `menu_item` / `paint_gear` / `paint_minimize` / `paint_maximize` / `paint_close` / `paint_chevron`：菜单项与图标。
//!
//! Module: app / panels
//! Purpose:
//! - Hand-drawn app bar of the borderless window (logo, main menu, project chip, window buttons)
//!   and the toolbar below it.
//! - Bottom status bar, lane overwrite prompt and resize hit areas of the borderless window.
//!
//! Functions:
//! - `SurfingApp::ui_lane_paste_prompt`: child-lane overwrite confirmation.
//! - `SurfingApp::app_bar`: window drag, menu toggle, project chip and window buttons.
//! - `SurfingApp::ui_main_menu`: home / file / edit / VST plugin / help / settings entries; returns
//!   the menu rect for click-outside dismissal.
//! - `SurfingApp::main_toolbar`: add-track, transport, BPM/time signature, loop and pointer tools.
//! - `SurfingApp::window_resize_edges`: resize hit areas along the borderless window edges.
//! - `SurfingApp::status_bar`: status, CPU, audio device and MIDI indicators.
//! - `apply_chrome_style` / `apply_menu_style`: IDEA-like styles for the app bar and its popups.
//! - `app_logo` / `hamburger_button` / `bar_text_button` / `project_chip` / `icon_button` / `window_control`: widgets.
//! - `menu_item` / `paint_gear` / `paint_minimize` / `paint_maximize` / `paint_close` / `paint_chevron`: items and glyphs.

use super::*;

/// 应用栏高度（逻辑点）；窗口按钮与拖动热区都使用该高度。
/// App bar height in logical points; window buttons and the drag region use it.
const APP_BAR_H: f32 = 34.0;
/// 应用栏左右内边距（逻辑点）。
/// Horizontal padding of the app bar in logical points.
const APP_BAR_PAD: f32 = 10.0;
/// 窗口按钮宽度（逻辑点）。
/// Width of a window button in logical points.
const WINDOW_BTN_W: f32 = 30.0;
/// 窗口按钮高度（逻辑点），比应用栏略矮，形成上下留白。
/// Window button height in logical points; shorter than the bar to leave vertical breathing room.
const WINDOW_BTN_H: f32 = 28.0;
/// 无边框窗口边缘缩放热区厚度（逻辑点）。
/// Thickness of the borderless resize hit area in logical points.
const RESIZE_EDGE_W: f32 = 4.0;
/// 无边框窗口角落缩放热区边长（逻辑点）。
/// Side length of the corner resize hit area in logical points.
const RESIZE_CORNER_W: f32 = 10.0;
/// 项目栏文本最大宽度（逻辑点），超出部分用省略号截断。
/// Maximum project-name width in logical points; longer names are truncated with an ellipsis.
const PROJECT_NAME_MAX_W: f32 = 200.0;

impl SurfingApp {
    pub(super) fn ui_lane_paste_prompt(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.lane_paste_prompt.clone() else {
            return;
        };
        let (what, target) = match &prompt.payload {
            Clipboard::ControllerLane { kind, .. } => (
                format!("子轨「{}」", kind.label()),
                self.project
                    .tracks
                    .get(prompt.target_track)
                    .map(|track| track.name.clone())
                    .unwrap_or_default(),
            ),
            Clipboard::EffectChain(effects) => (
                format!("效果器子轨（{} 个）", effects.len()),
                self.project
                    .tracks
                    .get(prompt.target_track)
                    .map(|track| track.name.clone())
                    .unwrap_or_default(),
            ),
            _ => return,
        };
        let mut decision: Option<bool> = None;
        egui::Window::new("覆盖子轨")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!("轨道「{target}」已有{what}，是否覆盖？"));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("覆盖").clicked() {
                        decision = Some(true);
                    }
                    if ui.button("取消").clicked() {
                        decision = Some(false);
                    }
                });
            });
        match decision {
            Some(true) => {
                self.lane_paste_prompt = None;
                self.apply_lane_paste(prompt.target_track, prompt.payload);
            }
            Some(false) => {
                self.lane_paste_prompt = None;
                self.status = "已取消粘贴".into();
            }
            None => {}
        }
    }

    // ----- panels ---------------------------------------------------------

    /// 所属模块：app / 面板
    /// 功能：绘制应用栏。常规状态显示图标、隐藏下拉栏与项目栏；展开下拉栏后左上角只保留
    /// 主页/文件/编辑/VST 插件/帮助/设置，点击其他区域或按 Esc 自动收起。整条栏同时作为
    /// 窗口拖动热区。点击图标请求显示主页窗口（工程窗口保持打开）。
    /// 输入：
    /// - `ctx`：egui 上下文，用于发送视口命令（拖动/最小化/最大化/关闭）。
    /// 输出/副作用：读写 `menu_bar_visible`；可能请求显示主页、打开工程窗口、
    /// 触发插件扫描、打开帮助或设置窗口。
    /// 复杂度：每帧 O(应用栏控件数)。
    ///
    /// Module: app / panels
    /// Purpose: Draw the app bar. Normally it shows the logo, menu toggle and project chip; while the
    /// menu is expanded the top-left keeps home/file/edit/VST/help/settings and any click outside (or
    /// Esc) dismisses it. The whole bar is a window drag region.
    /// Clicking the logo asks the shell to show the home window; the project window stays open.
    /// Inputs:
    /// - `ctx`: egui context used to send viewport commands (drag/minimize/maximize/close).
    /// Outputs / side effects: reads and writes `menu_bar_visible`; may request the home window,
    /// open a project window, start a plugin scan, or open the help/settings window.
    /// Complexity: O(app bar widgets) per frame.
    pub(super) fn app_bar(&mut self, ctx: &egui::Context) {
        self.remember_window_rect(ctx);
        let maximized = self.is_window_maximized(ctx);
        if self.menu_bar_visible && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.menu_bar_visible = false;
        }
        let mut menu_rect = egui::Rect::NOTHING;
        let menu_open = self.menu_bar_visible;

        egui::TopBottomPanel::top("app_bar")
            .exact_height(APP_BAR_H)
            .frame(egui::Frame::NONE.fill(theme::APP_BAR))
            .show(ctx, |ui| {
                apply_chrome_style(ui);

                // 先注册整条应用栏的拖动热区；之后添加的按钮会覆盖它，从而保留按钮点击。
                // Register the drag region first; widgets added later sit on top and keep their clicks.
                let drag = ui.interact(
                    ui.max_rect(),
                    ui.id().with("app_bar_drag"),
                    egui::Sense::click_and_drag(),
                );
                if drag.drag_started_by(egui::PointerButton::Primary) {
                    // 最大化时先还原到指针下方再拖动，复现原生标题栏的行为。
                    // While maximized, restore under the pointer before the OS drag starts.
                    self.begin_window_drag(ctx, APP_BAR_H);
                }
                if drag.double_clicked() {
                    self.toggle_window_maximize(ctx);
                }

                // 固定整条栏的高度并垂直居中，避免左侧控件贴着顶部、与右侧按钮不对称。
                // Use the full bar height and center vertically, so the left side is not glued to the top.
                ui.horizontal_centered(|ui| {
                    ui.add_space(APP_BAR_PAD);
                    // 图标始终保留，展开菜单时只隐藏下拉按钮与项目栏；点击图标返回主页。
                    // The logo always stays; only the menu toggle and project chip are hidden.
                    // Clicking the logo returns to the home page.
                    if app_logo(ui).clicked() {
                        self.open_home();
                    }
                    if self.view == WorkspaceView::Home {
                        // 主页不提供下拉栏与项目列表，只显示欢迎语与当前系统用户。
                        // The home page hides the menu toggle and project chip; it shows a welcome
                        // message with the current OS user instead.
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "欢迎使用SurfingStudio（{}）",
                                current_user_name()
                            ))
                            .size(10.5)
                            .color(theme::APP_BAR_TEXT_DIM),
                        );
                    } else if menu_open {
                        ui.add_space(8.0);
                        menu_rect = self.ui_main_menu(ui);
                    } else {
                        ui.add_space(4.0);
                        if hamburger_button(ui).clicked() {
                            self.menu_bar_visible = true;
                        }
                        ui.add_space(4.0);
                        let chip = project_chip(ui, &self.project.name, self.dirty);
                        egui::Popup::menu(&chip).show(|ui| {
                            apply_menu_style(ui);
                            ui.set_min_width(210.0);
                            ui.label(
                                egui::RichText::new(&self.project.name)
                                    .size(10.5)
                                    .color(theme::APP_BAR_TEXT_DIM),
                            );
                            ui.add_space(2.0);
                            ui.separator();
                            if menu_item(
                                ui,
                                "打开项目…",
                                &self.settings.shortcut_label(ShortcutAction::Open),
                            )
                            .clicked()
                            {
                                self.open_project_dialog();
                                ui.close();
                            }
                            if menu_item(
                                ui,
                                "新建工程",
                                &self.settings.shortcut_label(ShortcutAction::New),
                            )
                            .clicked()
                            {
                                self.request_new_project();
                                ui.close();
                            }
                        });
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        ui.add_space(4.0);
                        let close = window_control(ui, "关闭", theme::CLOSE_HOVER, paint_close);
                        if close.clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        let maximize = window_control(
                            ui,
                            if maximized { "还原" } else { "最大化" },
                            theme::APP_BAR_HOVER,
                            |painter, rect, color| paint_maximize(painter, rect, color, maximized),
                        );
                        if maximize.clicked() {
                            self.toggle_window_maximize(ctx);
                        }
                        let minimize =
                            window_control(ui, "最小化", theme::APP_BAR_HOVER, paint_minimize);
                        if minimize.clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        ui.add_space(6.0);
                    });
                });

                // 展开下方栏后，点击菜单以外的任意位置（含右侧按钮与编辑区）都收起菜单栏。
                // 仅在进入本帧时菜单已展开的情况下检查，避免刚点开就被同一次点击收起。
                // While the menu is open, any click outside the menu buttons dismisses it. Only check
                // when the menu was already open at frame start, so the opening click cannot close it.
                if menu_open && ctx.input(|i| i.pointer.any_click()) {
                    let inside = ctx
                        .input(|i| i.pointer.interact_pos())
                        .is_some_and(|pos| menu_rect.expand(6.0).contains(pos));
                    if !inside {
                        self.menu_bar_visible = false;
                    }
                }
            });
    }

    /// 所属模块：app / 面板 / 应用栏
    /// 功能：绘制主页、文件、编辑、VST 插件、帮助与设置菜单；主页与设置是独立入口，其余菜单项
    /// 复用现有命令，不新增业务逻辑。
    /// 输入：
    /// - `ui`：应用栏的水平布局。
    /// 输出/副作用：按点击执行工程/编辑/插件/帮助/设置命令，并关闭对应菜单；返回菜单按钮的并集矩形。
    /// 复杂度：O(菜单项数)。
    ///
    /// Module: app / panels / app bar
    /// Purpose: Draw home, file, edit, VST plugin, help and settings entries; home and settings are
    /// standalone buttons, the rest reuse existing commands only.
    /// Inputs:
    /// - `ui`: horizontal layout of the app bar.
    /// Outputs / side effects: run project/edit/plugin/help/settings commands and close the menu;
    /// returns the union rect of the menu buttons so clicks outside can dismiss the menu bar.
    /// Complexity: O(menu items).
    fn ui_main_menu(&mut self, ui: &mut egui::Ui) -> egui::Rect {
        let mut rect = egui::Rect::NOTHING;

        // 主页独立为下拉栏第一项：点击直接显示主页窗口，不再藏在「文件」菜单里。
        // Home is a standalone first entry; it no longer lives inside the File menu.
        let home = bar_text_button(ui, "主页", theme::APP_BAR_TEXT_DIM);
        rect = rect.union(home.rect);
        if home.clicked() {
            self.open_home();
        }

        let file = bar_text_button(ui, "文件", theme::APP_BAR_TEXT_DIM);
        rect = rect.union(file.rect);
        egui::Popup::menu(&file).show(|ui| {
            apply_menu_style(ui);
            ui.set_min_width(230.0);
            if menu_item(
                ui,
                "新建工程",
                &self.settings.shortcut_label(ShortcutAction::New),
            )
            .clicked()
            {
                self.request_new_project();
                ui.close();
            }
            if menu_item(
                ui,
                "打开项目…",
                &self.settings.shortcut_label(ShortcutAction::Open),
            )
            .clicked()
            {
                self.open_project_dialog();
                ui.close();
            }
            if menu_item(
                ui,
                "保存",
                &self.settings.shortcut_label(ShortcutAction::Save),
            )
            .clicked()
            {
                self.save_project();
                ui.close();
            }
            if menu_item(ui, "另存为…", "").clicked() {
                self.save_project_as(true);
                ui.close();
            }
            ui.separator();
            if menu_item(
                ui,
                "导出 WAV…",
                &self.settings.shortcut_label(ShortcutAction::Export),
            )
            .clicked()
            {
                self.export_wav_dialog();
                ui.close();
            }
            // 没有系统文件对话框的平台（安卓）用 SAF 把副本写到公共目录。
            // Platforms without a native dialog (Android) write copies to a public folder via SAF.
            if !crate::platform::caps().native_file_dialog {
                ui.separator();
                if menu_item(ui, "工程副本保存到公共目录…", "").clicked() {
                    self.export_project_copy_to_public();
                    ui.close();
                }
                if menu_item(ui, "WAV 导出到公共目录…", "").clicked() {
                    self.export_wav_copy_to_public();
                    ui.close();
                }
            }
        });

        let edit = bar_text_button(ui, "编辑", theme::APP_BAR_TEXT_DIM);
        rect = rect.union(edit.rect);
        egui::Popup::menu(&edit).show(|ui| {
            apply_menu_style(ui);
            ui.set_min_width(210.0);
            if menu_item(
                ui,
                "撤销",
                &self.settings.shortcut_label(ShortcutAction::Undo),
            )
            .clicked()
            {
                self.undo();
                ui.close();
            }
            if menu_item(
                ui,
                "重做",
                &self.settings.shortcut_label(ShortcutAction::Redo),
            )
            .clicked()
            {
                self.redo();
                ui.close();
            }
            ui.separator();
            if menu_item(
                ui,
                "复制",
                &self.settings.shortcut_label(ShortcutAction::Copy),
            )
            .clicked()
            {
                self.copy_selection();
                ui.close();
            }
            if menu_item(
                ui,
                "粘贴",
                &self.settings.shortcut_label(ShortcutAction::Paste),
            )
            .clicked()
            {
                self.paste_clipboard();
                ui.close();
            }
            if menu_item(
                ui,
                "删除",
                &self.settings.shortcut_label(ShortcutAction::Delete),
            )
            .clicked()
            {
                self.delete_selection();
                ui.close();
            }
        });

        // 插件菜单只在支持插件宿主的平台出现（安卓没有 VST 生态）。
        // The plugin menu only exists where plugin hosting is available (Android has no VST).
        if crate::platform::caps().plugin_hosting {
            let vst = bar_text_button(ui, "VST 插件", theme::APP_BAR_TEXT_DIM);
            rect = rect.union(vst.rect);
            egui::Popup::menu(&vst).show(|ui| {
                apply_menu_style(ui);
                ui.set_min_width(240.0);
                if menu_item(
                    ui,
                    "管理 VST 插件…",
                    &self.settings.shortcut_label(ShortcutAction::PluginBrowser),
                )
                .clicked()
                {
                    self.open_browser(crate::ui::PluginTarget::Instrument);
                    ui.close();
                }
                ui.separator();
                if menu_item(ui, "仅扫描 VST3", "").clicked() {
                    self.scan_vst3_only();
                    ui.close();
                }
                if menu_item(ui, "仅扫描 VST2 / VST", "").clicked() {
                    self.scan_legacy_only();
                    ui.close();
                }
            });
        }

        let help = bar_text_button(ui, "帮助", theme::APP_BAR_TEXT_DIM);
        rect = rect.union(help.rect);
        egui::Popup::menu(&help).show(|ui| {
            apply_menu_style(ui);
            ui.set_min_width(210.0);
            if menu_item(
                ui,
                "使用帮助与快捷键",
                &self.settings.shortcut_label(ShortcutAction::Help),
            )
            .clicked()
            {
                self.show_help = true;
                ui.close();
            }
            ui.separator();
            ui.label(
                egui::RichText::new(format!("SurfingStudio {}", env!("CARGO_PKG_VERSION")))
                    .size(10.5)
                    .color(theme::APP_BAR_TEXT_DIM),
            );
        });

        // 设置是独立入口：直接打开（或在单窗口平台弹出）设置面板。
        // Settings is a standalone entry that opens the settings window (or modal on single-window
        // platforms) directly.
        ui.add_space(4.0);
        let settings = bar_text_button(ui, "设置", theme::APP_BAR_TEXT_DIM);
        rect = rect.union(settings.rect);
        if settings.clicked() {
            self.open_settings(ui.ctx());
            self.menu_bar_visible = false;
        }

        rect
    }

    /// 所属模块：app / 面板
    /// 功能：绘制工具栏：添加轨道、走带控制、BPM/拍号、循环起止与鼠标模式；窗口变窄时自动换行。
    /// 输入：
    /// - `ctx`：egui 上下文。
    /// 输出/副作用：修改工程（BPM、拍号、循环）并 `mark_dirty`；切换播放与鼠标模式。
    /// 复杂度：每帧 O(工具栏控件数)。
    ///
    /// Module: app / panels
    /// Purpose: Draw the toolbar: add-track, transport, BPM/time signature, loop range and pointer tools;
    /// controls wrap instead of clipping on narrow windows.
    /// Inputs:
    /// - `ctx`: egui context.
    /// Outputs / side effects: edit the project (BPM, time signature, loop) and `mark_dirty`;
    /// toggle playback and the pointer mode.
    /// Complexity: O(toolbar widgets) per frame.
    pub(super) fn main_toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("main_toolbar")
            .frame(
                egui::Frame::NONE
                    .fill(theme::APP_BAR)
                    .stroke(egui::Stroke::new(1.0_f32, theme::APP_BAR_LINE))
                    .inner_margin(egui::Margin::symmetric(10, 7)),
            )
            .show(ctx, |ui| {
                apply_chrome_style(ui);
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.horizontal_wrapped(|ui| {
                    let playing = self.ui_playing;
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("⏮").size(13.0))
                                .min_size(egui::Vec2::new(26.0, 24.0)),
                        )
                        .on_hover_text("回到最前")
                        .clicked()
                    {
                        self.jump_to_start();
                    }
                    let play_shortcut = self.settings.shortcut_label(ShortcutAction::PlayPause);
                    let (play_icon, play_color, play_tip) = if playing {
                        ("⏸", theme::APP_BAR_TEXT, format!("暂停 ({play_shortcut})"))
                    } else {
                        ("▶", theme::GREEN, format!("播放 ({play_shortcut})"))
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(play_icon).size(14.0).color(play_color),
                            )
                            .min_size(egui::Vec2::new(30.0, 24.0)),
                        )
                        .on_hover_text(play_tip)
                        .clicked()
                    {
                        self.toggle_play();
                    }
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("⏭").size(13.0))
                                .min_size(egui::Vec2::new(26.0, 24.0)),
                        )
                        .on_hover_text("跳到最后一个片段末尾")
                        .clicked()
                    {
                        self.jump_to_end();
                    }

                    let playhead = self.playhead();
                    let position = format_position(playhead, self.project.beats_per_bar());
                    ui.label(
                        egui::RichText::new(position)
                            .monospace()
                            .size(15.0)
                            .color(theme::ACCENT),
                    );

                    toolbar_separator(ui);

                    let mut bpm = self.project.bpm;
                    if ui
                        .add(
                            egui::DragValue::new(&mut bpm)
                                .range(40.0..=240.0)
                                .speed(0.2)
                                .custom_formatter(|value, _| format!("{value:.0} BPM"))
                                .custom_parser(|text| text.trim_end_matches(" BPM").parse().ok()),
                        )
                        .on_hover_text("工程速度（BPM）")
                        .changed()
                    {
                        self.project.bpm = bpm;
                        self.mark_dirty();
                    }

                    let mut num = self.project.time_sig_num as i32;
                    if ui
                        .add(egui::DragValue::new(&mut num).range(1..=12).suffix(" /"))
                        .on_hover_text("每小节拍数")
                        .changed()
                    {
                        self.project.time_sig_num = num as u8;
                        self.mark_dirty();
                    }
                    egui::ComboBox::from_id_salt("timesig_den")
                        .width(44.0)
                        .selected_text(format!("{}", self.project.time_sig_den))
                        .show_ui(ui, |ui| {
                            for den in [1u8, 2, 4, 8, 16] {
                                if ui
                                    .selectable_value(
                                        &mut self.project.time_sig_den,
                                        den,
                                        format!("{den}"),
                                    )
                                    .changed()
                                {
                                    self.mark_dirty();
                                }
                            }
                        });

                    toolbar_separator(ui);

                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("循环").size(11.5))
                                .selected(self.project.loop_enabled)
                                .min_size(egui::Vec2::new(46.0, 24.0)),
                        )
                        .on_hover_text("开启/关闭循环播放")
                        .clicked()
                    {
                        self.project.loop_enabled = !self.project.loop_enabled;
                        self.mark_dirty();
                    }
                    let mut loop_start = self.project.loop_start;
                    let mut loop_end = self.project.loop_end;
                    if ui
                        .add(
                            egui::DragValue::new(&mut loop_start)
                                .range(0.0..=512.0)
                                .speed(0.1)
                                .prefix("起 "),
                        )
                        .on_hover_text("循环起点（beat）")
                        .changed()
                    {
                        self.project.loop_start = loop_start.min(self.project.loop_end - 0.25);
                        self.mark_dirty();
                    }
                    if ui
                        .add(
                            egui::DragValue::new(&mut loop_end)
                                .range(0.25..=512.0)
                                .speed(0.1)
                                .prefix("止 "),
                        )
                        .on_hover_text("循环终点（beat）")
                        .changed()
                    {
                        self.project.loop_end = loop_end.max(self.project.loop_start + 0.25);
                        self.mark_dirty();
                    }

                    // Pointer tool selector (arrangement + piano/controller roll).
                    toolbar_separator(ui);
                    ui.label(
                        egui::RichText::new("鼠标")
                            .size(10.0)
                            .color(theme::APP_BAR_TEXT_DIM),
                    );
                    let shift_temp = ui.input(|i| i.modifiers.shift);
                    for mode in MouseMode::ALL.iter() {
                        let button = egui::Button::new(
                            egui::RichText::new(format!("{} {}", mode.icon(), mode.label()))
                                .size(11.5),
                        )
                        .selected(
                            self.mouse_mode == *mode || (shift_temp && *mode == MouseMode::Draw),
                        )
                        .min_size(egui::Vec2::new(62.0, 24.0));
                        let action = match mode {
                            MouseMode::Select => ShortcutAction::ToolSelect,
                            MouseMode::Draw => ShortcutAction::ToolDraw,
                            MouseMode::Cut => ShortcutAction::ToolCut,
                        };
                        if ui
                            .add(button)
                            .on_hover_text(format!(
                                "{}（快捷键 {}）· 按住 Shift 临时切换为绘制",
                                mode.label(),
                                self.settings.shortcut_label(action)
                            ))
                            .clicked()
                        {
                            self.mouse_mode = *mode;
                        }
                    }
                });
            });
    }

    /// 所属模块：app / 面板 / 无边框窗口
    /// 功能：在窗口四周与四角创建不可见缩放热区；拖拽时把方向交给视口命令，弥补无系统边框后
    /// 缺失的缩放能力。
    /// 输入：
    /// - `ctx`：egui 上下文，用于读取屏幕矩形并发送 `BeginResize`。
    /// 输出/副作用：可能设置鼠标指针形状并发送视口缩放命令；不修改应用状态。
    /// 复杂度：O(1)（固定 8 个热区）。
    ///
    /// Module: app / panels / borderless window
    /// Purpose: Create invisible resize hit areas around the window edges and corners and forward the
    /// drag direction to the viewport, replacing the resize ability lost without OS decorations.
    /// Inputs:
    /// - `ctx`: egui context used to read the screen rect and send `BeginResize`.
    /// Outputs / side effects: may set the cursor icon and send resize commands; no app state change.
    /// Complexity: O(1) (eight fixed hit areas).
    pub(super) fn window_resize_edges(&self, ctx: &egui::Context) {
        if self.is_window_maximized(ctx) {
            return;
        }
        let screen = ctx.screen_rect();
        let edge = RESIZE_EDGE_W;
        let corner = RESIZE_CORNER_W;
        let (left, right, top, bottom) =
            (screen.left(), screen.right(), screen.top(), screen.bottom());
        let edges = [
            (
                egui::viewport::ResizeDirection::North,
                egui::Rect::from_min_max(
                    egui::pos2(left + corner, top),
                    egui::pos2(right - corner, top + edge),
                ),
                egui::CursorIcon::ResizeNorth,
                "north",
            ),
            (
                egui::viewport::ResizeDirection::South,
                egui::Rect::from_min_max(
                    egui::pos2(left + corner, bottom - edge),
                    egui::pos2(right - corner, bottom),
                ),
                egui::CursorIcon::ResizeSouth,
                "south",
            ),
            (
                egui::viewport::ResizeDirection::West,
                egui::Rect::from_min_max(
                    egui::pos2(left, top + corner),
                    egui::pos2(left + edge, bottom - corner),
                ),
                egui::CursorIcon::ResizeWest,
                "west",
            ),
            (
                egui::viewport::ResizeDirection::East,
                egui::Rect::from_min_max(
                    egui::pos2(right - edge, top + corner),
                    egui::pos2(right, bottom - corner),
                ),
                egui::CursorIcon::ResizeEast,
                "east",
            ),
            (
                egui::viewport::ResizeDirection::NorthWest,
                egui::Rect::from_min_size(egui::pos2(left, top), egui::vec2(corner, corner)),
                egui::CursorIcon::ResizeNorthWest,
                "north_west",
            ),
            (
                egui::viewport::ResizeDirection::NorthEast,
                egui::Rect::from_min_size(
                    egui::pos2(right - corner, top),
                    egui::vec2(corner, corner),
                ),
                egui::CursorIcon::ResizeNorthEast,
                "north_east",
            ),
            (
                egui::viewport::ResizeDirection::SouthWest,
                egui::Rect::from_min_size(
                    egui::pos2(left, bottom - corner),
                    egui::vec2(corner, corner),
                ),
                egui::CursorIcon::ResizeSouthWest,
                "south_west",
            ),
            (
                egui::viewport::ResizeDirection::SouthEast,
                egui::Rect::from_min_size(
                    egui::pos2(right - corner, bottom - corner),
                    egui::vec2(corner, corner),
                ),
                egui::CursorIcon::ResizeSouthEast,
                "south_east",
            ),
        ];

        for (direction, rect, cursor, name) in edges {
            let response = egui::Area::new(egui::Id::new(("window_resize", name)))
                .order(egui::Order::Foreground)
                .fixed_pos(rect.min)
                .show(ctx, |ui| {
                    ui.allocate_response(rect.size(), egui::Sense::drag())
                })
                .inner;
            if response.hovered() {
                ctx.set_cursor_icon(cursor);
            }
            if response.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
            }
        }
    }

    /// 所属模块：app / 面板
    /// 功能：绘制底部状态栏：状态文本、CPU、音频设备、MIDI 与未保存标记。
    /// 输入：
    /// - `ctx`：egui 上下文。
    /// 输出/副作用：只读 `ui_cpu_load`、`audio_status`、`midi.status` 与 `dirty`；不修改状态。
    /// 复杂度：O(1)。
    ///
    /// Module: app / panels
    /// Purpose: Draw the bottom status bar: status text, CPU, audio device, MIDI and the dirty marker.
    /// Inputs:
    /// - `ctx`: egui context.
    /// Outputs / side effects: read `ui_cpu_load`, `audio_status`, `midi.status` and `dirty`; no mutation.
    /// Complexity: O(1).
    pub(super) fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("statusbar")
            .exact_height(27.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::APP_BAR)
                    .stroke(egui::Stroke::new(1.0_f32, theme::APP_BAR_LINE))
                    .inner_margin(egui::Margin::symmetric(10, 4)),
            )
            .show(ctx, |ui| {
                apply_chrome_style(ui);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&self.status)
                            .size(10.5)
                            .color(theme::APP_BAR_TEXT_DIM),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let cpu = self.ui_cpu_load;
                        ui.label(
                            egui::RichText::new(format!("CPU {:.1}%", cpu * 100.0))
                                .size(10.5)
                                .color(if cpu > 0.8 {
                                    theme::RED
                                } else {
                                    theme::APP_BAR_TEXT_DIM
                                }),
                        );
                        ui.separator();
                        ui.label(
                            egui::RichText::new(&self.audio_status)
                                .size(10.5)
                                .color(theme::APP_BAR_TEXT_DIM),
                        );
                        ui.separator();
                        ui.label(
                            egui::RichText::new(&self.midi_status)
                                .size(10.5)
                                .color(theme::APP_BAR_TEXT_DIM),
                        );
                        if self.dirty {
                            ui.label(
                                egui::RichText::new("● 未保存")
                                    .size(10.5)
                                    .color(theme::AMBER),
                            );
                        }
                    });
                });
            });
    }
}

/// 所属模块：app / 面板 / 样式
/// 功能：把应用栏与工具栏的控件样式改成 IDEA New UI：透明底、圆角悬停、灰色文字与细分割线。
/// 输入：
/// - `ui`：应用栏或工具栏的 Ui。
/// 输出/副作用：修改该 Ui 的局部样式；弹出菜单会继承这些颜色。
/// 复杂度：O(1)。
///
/// Module: app / panels / style
/// Purpose: Apply an IDEA New UI look to the app bar and toolbar: transparent fills, rounded hover,
/// gray text and hairline separators.
/// Inputs:
/// - `ui`: the app bar or toolbar Ui.
/// Outputs / side effects: mutates the local style; popups inherit these colors.
/// Complexity: O(1).
fn apply_chrome_style(ui: &mut egui::Ui) {
    let visuals = &mut ui.style_mut().visuals;
    visuals.panel_fill = theme::APP_BAR;
    visuals.window_fill = theme::APP_BAR;
    visuals.window_stroke = egui::Stroke::new(1.0_f32, theme::APP_BAR_LINE);
    visuals.window_corner_radius = egui::CornerRadius::from(6);
    visuals.menu_corner_radius = egui::CornerRadius::from(5);
    visuals.extreme_bg_color = theme::APP_BAR_LINE;
    visuals.faint_bg_color = theme::APP_BAR_HOVER;
    visuals.code_bg_color = theme::APP_BAR_LINE;

    visuals.widgets.noninteractive.bg_fill = theme::APP_BAR;
    visuals.widgets.noninteractive.weak_bg_fill = theme::APP_BAR;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, theme::APP_BAR_LINE);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, theme::APP_BAR_TEXT_DIM);

    visuals.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, theme::APP_BAR_TEXT_DIM);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::from(4);

    visuals.widgets.hovered.bg_fill = theme::APP_BAR_HOVER;
    visuals.widgets.hovered.weak_bg_fill = theme::APP_BAR_HOVER;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::from(4);

    visuals.widgets.active.bg_fill = theme::APP_BAR_ACTIVE;
    visuals.widgets.active.weak_bg_fill = theme::APP_BAR_ACTIVE;
    visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    visuals.widgets.active.corner_radius = egui::CornerRadius::from(4);

    visuals.widgets.open.bg_fill = theme::APP_BAR_ACTIVE;
    visuals.widgets.open.weak_bg_fill = theme::APP_BAR_ACTIVE;
    visuals.widgets.open.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    visuals.widgets.open.corner_radius = egui::CornerRadius::from(4);

    visuals.selection.bg_fill = theme::APP_BAR_ACTIVE;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);

    ui.spacing_mut().button_padding = egui::vec2(7.0, 3.0);
    ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
    ui.spacing_mut().interact_size.y = 24.0;
}

/// 所属模块：app / 面板 / 样式
/// 功能：设置弹出菜单样式：深灰底色、细边框、浅色菜单项与更宽松的内边距（IDEA 风格）。
/// 输入：
/// - `ui`：弹出菜单内容区。
/// 输出/副作用：修改该菜单的局部样式。
/// 复杂度：O(1)。
///
/// Module: app / panels / style
/// Purpose: Style popup menus: dark gray background, hairline border, light items and roomier padding.
/// Inputs:
/// - `ui`: content area of the popup menu.
/// Outputs / side effects: mutates the local menu style.
/// Complexity: O(1).
fn apply_menu_style(ui: &mut egui::Ui) {
    let visuals = &mut ui.style_mut().visuals;
    visuals.window_fill = theme::APP_BAR;
    visuals.window_stroke = egui::Stroke::new(1.0_f32, theme::APP_BAR_LINE);
    visuals.window_corner_radius = egui::CornerRadius::from(5);
    visuals.menu_corner_radius = egui::CornerRadius::from(5);
    visuals.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, theme::APP_BAR_TEXT);
    visuals.widgets.hovered.bg_fill = theme::APP_BAR_HOVER;
    visuals.widgets.hovered.weak_bg_fill = theme::APP_BAR_HOVER;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    visuals.widgets.active.bg_fill = theme::APP_BAR_HOVER;
    visuals.widgets.active.weak_bg_fill = theme::APP_BAR_HOVER;
    visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    visuals.selection.bg_fill = theme::APP_BAR_HOVER;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    ui.spacing_mut().button_padding = egui::vec2(10.0, 5.0);
    ui.spacing_mut().item_spacing = egui::vec2(6.0, 2.0);
}

/// 所属模块：app / 面板 / 工具栏
/// 功能：绘制一条 IDEA 风格的细竖分隔线，并留出左右间距。
/// 输入：`ui`：工具栏水平布局。
/// 输出/副作用：分配宽度并绘制；不修改状态。
/// 复杂度：O(1)。
///
/// Module: app / panels / toolbar
/// Purpose: Draw a thin vertical separator with breathing room, IDEA style.
/// Inputs: `ui`: toolbar horizontal layout.
/// Outputs / side effects: allocate width and paint; no state change.
/// Complexity: O(1).
fn toolbar_separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 24.0), egui::Sense::hover());
    let x = rect.center().x;
    ui.painter().vline(
        x,
        (rect.top() + 4.0)..=(rect.bottom() - 4.0),
        egui::Stroke::new(1.0_f32, theme::APP_BAR_LINE),
    );
}

/// 所属模块：app / 面板 / 应用栏
/// 功能：返回当前运行程序的系统用户名；Windows 优先 `USERNAME`，其他平台回退 `USER`。
/// 输入：无。
/// 输出/副作用：返回用户名；都不可用时返回“用户”。
/// 复杂度：时间 O(1)，空间 O(用户名长度)。
///
/// Module: app / panels / app bar
/// Purpose: Return the OS user name running the app; `USERNAME` on Windows, `USER` elsewhere.
/// Inputs: none.
/// Outputs / side effects: the user name, or "用户" when neither variable exists.
/// Complexity: O(1) time and O(name length) space.
fn current_user_name() -> String {
    for variable in ["USERNAME", "USER"] {
        if let Ok(name) = std::env::var(variable) {
            let name = name.trim();
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    "用户".to_string()
}

/// 所属模块：app / 面板 / 应用栏控件
/// 功能：绘制 16 点见方的软件图标：IDEA 蓝圆角方块 + 白色波形折线；点击返回主页。
/// 输入：`ui`：应用栏水平布局。
/// 输出/副作用：分配 16×16 点空间并绘制；悬浮时显示提示并返回可点击响应。
/// 复杂度：O(1)。
///
/// Module: app / panels / app bar widgets
/// Purpose: Draw the 16 pt logo: an IDEA-blue rounded square with a white waveform; clicking it
/// returns to the home page.
/// Inputs: `ui`: horizontal layout of the app bar.
/// Outputs / side effects: allocate 16×16 points and paint; show a tooltip and return a clickable
/// response.
/// Complexity: O(1).
fn app_logo(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
    let logo = egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0));
    let painter = ui.painter();
    painter.rect_filled(logo, egui::CornerRadius::from(3), theme::CHROME_ACCENT);
    let wave = [0.58_f32, 0.30, 0.70, 0.32, 0.60];
    let points: Vec<egui::Pos2> = wave
        .iter()
        .enumerate()
        .map(|(index, y)| {
            egui::pos2(
                logo.left() + logo.width() * (0.16 + 0.17 * index as f32),
                logo.top() + logo.height() * y,
            )
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.3_f32, egui::Color32::WHITE),
    ));
    response
        .on_hover_text(format!(
            "SurfingStudio {} · 点击返回主页",
            env!("CARGO_PKG_VERSION")
        ))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// 所属模块：app / 面板 / 应用栏控件
/// 功能：绘制「隐藏下拉栏」按钮（三条横线）。
/// 输入：`ui`：应用栏水平布局。
/// 输出/副作用：分配 24×24 点空间并绘制按钮；点击由调用方处理。
/// 复杂度：O(1)。
///
/// Module: app / panels / app bar widgets
/// Purpose: Draw the menu-toggle button (three bars).
/// Inputs: `ui`: horizontal layout of the app bar.
/// Outputs / side effects: allocate a 24×24 point button; the caller handles clicks.
/// Complexity: O(1).
fn hamburger_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::from(4), theme::APP_BAR_HOVER);
    }
    let color = if hovered {
        egui::Color32::WHITE
    } else {
        theme::APP_BAR_TEXT_DIM
    };
    let center = rect.center();
    let half = 6.0;
    let stroke = egui::Stroke::new(1.25_f32, color);
    for offset in [-4.0_f32, 0.0, 4.0] {
        ui.painter().line_segment(
            [
                egui::pos2(center.x - half, center.y + offset),
                egui::pos2(center.x + half, center.y + offset),
            ],
            stroke,
        );
    }
    response
        .on_hover_text("显示菜单栏")
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// 所属模块：app / 面板 / 应用栏控件
/// 功能：绘制扁平文本按钮（菜单项与工具栏入口），悬停时显示 IDEA 风格的圆角底色。
/// 输入：
/// - `ui`：应用栏或工具栏的水平布局。
/// - `label`：按钮文本。
/// - `text_color`：非悬停状态下的文字颜色。
/// 输出/副作用：分配文本宽度 + 16 点的空间并绘制；返回响应供弹出菜单使用。
/// 复杂度：O(1)。
///
/// Module: app / panels / app bar widgets
/// Purpose: Draw a flat text button (menu and toolbar entries) with an IDEA-like rounded hover.
/// Inputs:
/// - `ui`: horizontal layout of the app bar or toolbar.
/// - `label`: button text.
/// - `text_color`: text color when not hovered.
/// Outputs / side effects: allocate text width + 16 points and paint; return the response for popups.
/// Complexity: O(1).
fn bar_text_button(ui: &mut egui::Ui, label: &str, text_color: egui::Color32) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(12.5),
        egui::Color32::PLACEHOLDER,
    );
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 16.0, 24.0),
        egui::Sense::click(),
    );
    let hovered = response.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::from(4), theme::APP_BAR_HOVER);
    }
    let color = if hovered {
        egui::Color32::WHITE
    } else {
        text_color
    };
    let text_pos = egui::pos2(
        rect.center().x - galley.size().x / 2.0,
        rect.center().y - galley.size().y / 2.0,
    );
    ui.painter().galley(text_pos, galley, color);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// 所属模块：app / 面板 / 应用栏控件
/// 功能：绘制项目栏：项目名 + 未保存圆点 + 下拉箭头，作为打开/新建工程菜单的触发区域。
/// 输入：
/// - `ui`：应用栏水平布局。
/// - `name`：工程名，过长时按 200 点截断。
/// - `dirty`：工程是否有未保存修改。
/// 输出/副作用：分配固定高度 24 点的空间并绘制；返回响应供弹出菜单使用。
/// 复杂度：O(工程名字符数)。
///
/// Module: app / panels / app bar widgets
/// Purpose: Draw the project chip: project name, dirty dot and a dropdown chevron; it triggers the
/// open/new project menu.
/// Inputs:
/// - `ui`: horizontal layout of the app bar.
/// - `name`: project name, truncated at 200 points.
/// - `dirty`: whether the project has unsaved changes.
/// Outputs / side effects: allocate a 24 point high chip and paint; return the response for the popup.
/// Complexity: O(project-name length).
fn project_chip(ui: &mut egui::Ui, name: &str, dirty: bool) -> egui::Response {
    let galley = egui::WidgetText::from(
        egui::RichText::new(name)
            .size(12.5)
            .color(theme::APP_BAR_TEXT),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        PROJECT_NAME_MAX_W,
        egui::TextStyle::Body,
    );
    let width = (galley.size().x + if dirty { 42.0 } else { 30.0 }).max(96.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 24.0), egui::Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter();
    if hovered {
        painter.rect_filled(rect, egui::CornerRadius::from(4), theme::APP_BAR_HOVER);
    }
    let text_pos = egui::pos2(rect.left() + 8.0, rect.center().y - galley.size().y / 2.0);
    painter.galley(text_pos, galley, theme::APP_BAR_TEXT);
    if dirty {
        painter.circle_filled(
            egui::pos2(rect.right() - 18.0, rect.center().y),
            2.5,
            theme::AMBER,
        );
    }
    paint_chevron(
        painter,
        egui::pos2(rect.right() - 9.0, rect.center().y),
        theme::APP_BAR_TEXT_DIM,
    );
    response.on_hover_text("当前工程 · 点击打开或新建工程")
}

/// 所属模块：app / 面板 / 应用栏控件
/// 功能：绘制方形图标按钮（设置等），悬停时显示圆角底色。
/// 输入：
/// - `ui`：应用栏水平布局。
/// - `tooltip`：按钮提示文本。
/// - `paint`：图标绘制闭包，接收画笔、按钮矩形与图标颜色。
/// 输出/副作用：分配 28×24 点空间并绘制；返回点击响应。
/// 复杂度：O(1)。
///
/// Module: app / panels / app bar widgets
/// Purpose: Draw a square icon button (settings etc.) with a rounded hover background.
/// Inputs:
/// - `ui`: horizontal layout of the app bar.
/// - `tooltip`: tooltip text.
/// - `paint`: glyph painter receiving the painter, button rect and icon color.
/// Outputs / side effects: allocate 28×24 points and paint; return the click response.
/// Complexity: O(1).
pub(super) fn icon_button(
    ui: &mut egui::Ui,
    tooltip: &str,
    paint: impl FnOnce(&egui::Painter, egui::Rect, egui::Color32),
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 24.0), egui::Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::from(4), theme::APP_BAR_HOVER);
    }
    let color = if hovered {
        egui::Color32::WHITE
    } else {
        theme::APP_BAR_TEXT_DIM
    };
    paint(ui.painter(), rect, color);
    response
        .on_hover_text(tooltip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// 所属模块：app / 面板 / 应用栏控件
/// 功能：绘制窗口控制按钮（最小化 / 最大化 / 关闭），悬停时显示 IDEA 风格的圆角底色。
/// 输入：
/// - `ui`：应用栏水平布局。
/// - `tooltip`：按钮提示文本。
/// - `hover_fill`：悬停底色（关闭按钮使用红色）。
/// - `paint`：图标绘制闭包，接收画笔、按钮矩形与图标颜色。
/// 输出/副作用：分配 30×28 点空间并绘制；返回点击响应。
/// 复杂度：O(1)。
///
/// Module: app / panels / app bar widgets
/// Purpose: Draw a window control button (minimize/maximize/close) with an IDEA-like rounded hover.
/// Inputs:
/// - `ui`: horizontal layout of the app bar.
/// - `tooltip`: tooltip text.
/// - `hover_fill`: hover background (red for close).
/// - `paint`: glyph painter receiving the painter, button rect and icon color.
/// Outputs / side effects: allocate 30×28 points and paint; return the click response.
/// Complexity: O(1).
fn window_control(
    ui: &mut egui::Ui,
    tooltip: &str,
    hover_fill: egui::Color32,
    paint: impl FnOnce(&egui::Painter, egui::Rect, egui::Color32),
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(WINDOW_BTN_W, WINDOW_BTN_H), egui::Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::from(5), hover_fill);
    }
    let color = if hovered {
        egui::Color32::WHITE
    } else {
        theme::APP_BAR_TEXT_DIM
    };
    paint(ui.painter(), rect, color);
    response
        .on_hover_text(tooltip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// 所属模块：app / 面板 / 菜单
/// 功能：绘制 IDEA 风格菜单项：左侧标签 + 右对齐快捷键，整行可点击。
/// 输入：
/// - `ui`：弹出菜单内容区。
/// - `label`：菜单项文本。
/// - `shortcut`：右侧快捷键文本，空字符串表示不显示。
/// 输出/副作用：分配整行空间并绘制；返回点击响应。
/// 复杂度：O(1)。
///
/// Module: app / panels / menu
/// Purpose: Draw an IDEA-like menu item: label on the left, right-aligned shortcut, full-row clickable.
/// Inputs:
/// - `ui`: popup menu content area.
/// - `label`: item text.
/// - `shortcut`: right-aligned shortcut; an empty string hides it.
/// Outputs / side effects: allocate a full row and paint; return the click response.
/// Complexity: O(1).
fn menu_item(ui: &mut egui::Ui, label: &str, shortcut: &str) -> egui::Response {
    let label = egui::RichText::new(label).size(12.5);
    let shortcut = egui::RichText::new(shortcut)
        .size(10.5)
        .color(theme::APP_BAR_TEXT_DIM);
    ui.add(egui::Button::new(label).shortcut_text(shortcut))
}

/// 所属模块：app / 面板 / 应用栏控件
/// 功能：绘制设置齿轮图标：圆环 + 6 个短齿。
/// 输入：`painter`、按钮 `rect`、图标 `color`。
/// 输出/副作用：写入绘制命令，不修改状态。
/// 复杂度：O(1)。
///
/// Module: app / panels / app bar widgets
/// Purpose: Paint a settings gear: a ring plus six short teeth.
/// Inputs: `painter`, button `rect`, icon `color`.
/// Outputs / side effects: emit paint commands; no state change.
/// Complexity: O(1).
pub(super) fn paint_gear(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let center = rect.center();
    painter.circle_stroke(center, 3.2, egui::Stroke::new(1.15_f32, color));
    for index in 0..6 {
        let angle = index as f32 * std::f32::consts::TAU / 6.0;
        let dir = egui::vec2(angle.cos(), angle.sin());
        painter.line_segment(
            [center + dir * 4.6, center + dir * 6.2],
            egui::Stroke::new(1.4_f32, color),
        );
    }
}

/// 所属模块：app / 面板 / 应用栏控件
/// 功能：绘制最小化图标（一条水平线）。
/// 输入：`painter`、按钮 `rect`、图标 `color`。
/// 输出/副作用：写入绘制命令，不修改状态。
/// 复杂度：O(1)。
///
/// Module: app / panels / app bar widgets
/// Purpose: Paint the minimize glyph (a single horizontal line).
/// Inputs: `painter`, button `rect`, icon `color`.
/// Outputs / side effects: emit paint commands; no state change.
/// Complexity: O(1).
fn paint_minimize(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let center = rect.center();
    painter.line_segment(
        [
            egui::pos2(center.x - 4.5, center.y + 0.5),
            egui::pos2(center.x + 4.5, center.y + 0.5),
        ],
        egui::Stroke::new(1.1_f32, color),
    );
}

/// 所属模块：app / 面板 / 应用栏控件
/// 功能：绘制最大化/还原图标：未最大化时为一个方框，最大化时为两个错位方框。
/// 输入：`painter`、按钮 `rect`、图标 `color`、`restored`（是否已最大化）。
/// 输出/副作用：写入绘制命令，不修改状态。
/// 复杂度：O(1)。
///
/// Module: app / panels / app bar widgets
/// Purpose: Paint the maximize/restore glyph: a single square when normal, two offset squares when
/// maximized.
/// Inputs: `painter`, button `rect`, icon `color`, `restored` (already maximized).
/// Outputs / side effects: emit paint commands; no state change.
/// Complexity: O(1).
fn paint_maximize(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32, restored: bool) {
    let stroke = egui::Stroke::new(1.1_f32, color);
    let center = rect.center();
    if restored {
        let front = egui::Rect::from_min_size(
            egui::pos2(center.x - 4.5, center.y - 3.0),
            egui::vec2(6.5, 6.5),
        );
        painter.rect_stroke(
            front,
            egui::CornerRadius::from(1),
            stroke,
            egui::StrokeKind::Inside,
        );
        painter.line_segment(
            [
                egui::pos2(center.x - 2.5, center.y - 5.0),
                egui::pos2(center.x + 4.5, center.y - 5.0),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(center.x + 4.5, center.y - 5.0),
                egui::pos2(center.x + 4.5, center.y + 2.0),
            ],
            stroke,
        );
    } else {
        let square = egui::Rect::from_center_size(center, egui::vec2(8.5, 8.5));
        painter.rect_stroke(
            square,
            egui::CornerRadius::from(1),
            stroke,
            egui::StrokeKind::Inside,
        );
    }
}

/// 所属模块：app / 面板 / 应用栏控件
/// 功能：绘制关闭图标（两条对角线）。
/// 输入：`painter`、按钮 `rect`、图标 `color`。
/// 输出/副作用：写入绘制命令，不修改状态。
/// 复杂度：O(1)。
///
/// Module: app / panels / app bar widgets
/// Purpose: Paint the close glyph (two diagonals).
/// Inputs: `painter`, button `rect`, icon `color`.
/// Outputs / side effects: emit paint commands; no state change.
/// Complexity: O(1).
fn paint_close(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let center = rect.center();
    let stroke = egui::Stroke::new(1.1_f32, color);
    painter.line_segment(
        [
            egui::pos2(center.x - 4.0, center.y - 4.0),
            egui::pos2(center.x + 4.0, center.y + 4.0),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(center.x + 4.0, center.y - 4.0),
            egui::pos2(center.x - 4.0, center.y + 4.0),
        ],
        stroke,
    );
}

/// 所属模块：app / 面板 / 应用栏控件
/// 功能：绘制向下的下拉箭头，用于项目栏。
/// 输入：`painter`、箭头中心 `center`、`color`。
/// 输出/副作用：写入绘制命令，不修改状态。
/// 复杂度：O(1)。
///
/// Module: app / panels / app bar widgets
/// Purpose: Paint a downward chevron used by the project chip.
/// Inputs: `painter`, chevron `center`, `color`.
/// Outputs / side effects: emit paint commands; no state change.
/// Complexity: O(1).
fn paint_chevron(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.1_f32, color);
    painter.line_segment(
        [
            egui::pos2(center.x - 3.5, center.y - 1.5),
            egui::pos2(center.x, center.y + 2.0),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(center.x, center.y + 2.0),
            egui::pos2(center.x + 3.5, center.y - 1.5),
        ],
        stroke,
    );
}
