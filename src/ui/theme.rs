//! 所属模块：ui / 主题
//! 文件用途：
//! - 全局 IDEA New UI 深色主题：中性灰表面、IDEA 蓝强调色、扁平控件与统一圆角。
//! - 中文字体加载、全局样式与复用绘制（电平表、徽章、片段调色板）。
//!
//! 函数清单：
//! - `setup`：加载系统中文字体、设置全局缩放并应用主题。
//! - `apply_style`：把 IDEA New UI 的配色、圆角、间距与文字尺寸写入全局样式。
//! - `track_color` / `palette_color` / `hsv_to_rgb`：轨道色与 16×16 片段调色板。
//! - `with_alpha`：给颜色附加透明度。
//! - `level_meter` / `badge` / `toggle_switch`：主题内的复用绘制。
//!
//! Module: ui / theme
//! Purpose:
//! - Global IDEA New UI dark theme: neutral gray surfaces, IDEA blue accent, flat widgets and
//!   consistent corner radii.
//! - CJK font setup, global style and shared painters (level meter, badge, clip palette).
//!
//! Functions:
//! - `setup`: load a system CJK font, set the global zoom and apply the theme.
//! - `apply_style`: write the IDEA New UI colors, radii, spacing and text sizes into the global style.
//! - `track_color` / `palette_color` / `hsv_to_rgb`: track colors and the 16×16 clip palette.
//! - `with_alpha`: attach alpha to a color.
//! - `level_meter` / `badge` / `toggle_switch`: reusable painters owned by the theme.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, Stroke, Vec2};
use std::sync::Arc;

// ----- 全局调色板（IDEA New UI Dark） --------------------------------------
// Global palette (IDEA New UI Dark).

/// 编辑区最底层：卷帘背景、输入框深底。
/// Deepest editor background: roll canvases and dark input backgrounds.
pub const BG_DEEP: Color32 = Color32::from_rgb(24, 25, 28);
/// 编辑区/中央面板底色（#1E1F22）。
/// Editor and central panel background (#1E1F22).
pub const BG: Color32 = Color32::from_rgb(30, 31, 34);
/// 面板、工具栏与检查器底色（#2B2D30）。
/// Panels, toolbars and inspector background (#2B2D30).
pub const PANEL: Color32 = Color32::from_rgb(43, 45, 48);
/// 行、卡片与输入控件的浅表面（#313336）。
/// Rows, cards and input surfaces (#313336).
pub const PANEL_2: Color32 = Color32::from_rgb(49, 51, 54);
/// 悬停/选中表面（#3C3F41）。
/// Hover/selected surface (#3C3F41).
pub const PANEL_3: Color32 = Color32::from_rgb(60, 63, 65);
/// 边框、分隔线与网格线（#393B3D）。
/// Borders, separators and grid lines (#393B3D).
pub const LINE: Color32 = Color32::from_rgb(57, 59, 61);
/// 主文本（#DFE1E5）。
/// Primary text (#DFE1E5).
pub const TEXT: Color32 = Color32::from_rgb(223, 225, 229);
/// 次要文本（#A8ADB2）。
/// Secondary text (#A8ADB2).
pub const TEXT_DIM: Color32 = Color32::from_rgb(168, 173, 178);
/// 最弱文本（#7A7E85）。
/// Faintest text (#7A7E85).
pub const TEXT_FAINT: Color32 = Color32::from_rgb(122, 126, 133);
/// 强调蓝（#3574F0），用于选中、播放头与焦点。
/// Accent blue (#3574F0) for selection, playhead and focus.
pub const ACCENT: Color32 = Color32::from_rgb(53, 116, 240);
/// 选中底色（#2E436E），配合强调蓝描边。
/// Selection background (#2E436E), paired with the accent stroke.
pub const ACCENT_DARK: Color32 = Color32::from_rgb(46, 67, 110);
/// 警告琥珀。
/// Warning amber.
pub const AMBER: Color32 = Color32::from_rgb(229, 165, 10);
/// 运行/成功绿。
/// Run/success green.
pub const GREEN: Color32 = Color32::from_rgb(98, 181, 67);
/// 错误红。
/// Error red.
pub const RED: Color32 = Color32::from_rgb(219, 92, 92);
/// 次强调紫。
/// Secondary violet.
pub const VIOLET: Color32 = Color32::from_rgb(177, 138, 230);

// ----- 窗口 chrome 别名 ----------------------------------------------------
// Window chrome aliases. They intentionally reuse the IDE palette so the custom title bar,
// toolbar and status bar share the same surfaces as the panels below them.

/// 自绘应用栏/工具栏/状态栏底色。
/// Background of the app bar, toolbar and status bar.
pub const APP_BAR: Color32 = PANEL;
/// 应用栏控件悬停底色。
/// Hover background of chrome widgets.
pub const APP_BAR_HOVER: Color32 = PANEL_3;
/// 应用栏选中/按下底色。
/// Selected/pressed background of chrome widgets.
pub const APP_BAR_ACTIVE: Color32 = Color32::from_rgb(74, 77, 80);
/// 应用栏分隔线与窗口边框。
/// Separator and border color of the chrome.
pub const APP_BAR_LINE: Color32 = Color32::from_rgb(30, 31, 34);
/// 应用栏主文本。
/// Primary chrome text.
pub const APP_BAR_TEXT: Color32 = TEXT;
/// 应用栏次要文本。
/// Secondary chrome text.
pub const APP_BAR_TEXT_DIM: Color32 = TEXT_DIM;
/// IDEA 强调蓝，用于焦点与选中描边。
/// IDEA accent blue used for focus and selection strokes.
pub const CHROME_ACCENT: Color32 = ACCENT;
/// 关闭按钮悬停底色（IDEA 红）。
/// Close button hover background (IDEA red).
pub const CLOSE_HOVER: Color32 = Color32::from_rgb(196, 43, 28);

pub fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for path in [
        "C:/Windows/Fonts/msyh.ttc",
        "C:/Windows/Fonts/msyh.ttf",
        "C:/Windows/Fonts/simhei.ttf",
        "C:/Windows/Fonts/simsun.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        // 安卓系统自带的简体中文字体：缺失时中文会显示为方块。
        // The Simplified-Chinese fonts shipped with Android; without them CJK text renders as tofu.
        "/system/fonts/NotoSansCJK-Regular.ttc",
        "/system/fonts/NotoSansSC-Regular.otf",
        "/system/fonts/NotoSansHans-Regular.otf",
        "/system/fonts/DroidSansFallbackFull.ttf",
        "/system/fonts/DroidSansFallback.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            let data = egui::FontData::from_owned(bytes);
            fonts.font_data.insert("cjk".into(), Arc::new(data));
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .push("cjk".into());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push("cjk".into());
            break;
        }
    }
    ctx.set_fonts(fonts);
    // Slightly larger UI so small labels stay legible.
    ctx.set_zoom_factor(1.15);
    apply_style(ctx);
}

/// 所属模块：ui / 主题
/// 功能：把 IDEA New UI 的配色、圆角、间距与字号写入全局样式；应用栏/工具栏会在此基础上
/// 再做局部扁平化。
/// 输入：`ctx`：egui 上下文。
/// 输出/副作用：替换全局 `Style` 与全部主题样式；不修改应用数据。
/// 复杂度：O(1)。
///
/// Module: ui / theme
/// Purpose: Write the IDEA New UI colors, radii, spacing and font sizes into the global style; the
/// app bar and toolbar flatten it further in their local scopes.
/// Inputs: `ctx`: egui context.
/// Outputs / side effects: replace the global `Style` and all theme styles; no app data changes.
/// Complexity: O(1).
pub fn apply_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = BG_DEEP;
    visuals.faint_bg_color = PANEL_2;
    visuals.code_bg_color = BG_DEEP;
    visuals.window_corner_radius = CornerRadius::from(8);
    visuals.menu_corner_radius = CornerRadius::from(6);
    visuals.selection.bg_fill = ACCENT_DARK;
    visuals.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.hyperlink_color = ACCENT;
    visuals.window_stroke = Stroke::new(1.0_f32, LINE);

    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::from(4);

    visuals.widgets.inactive.bg_fill = PANEL_2;
    visuals.widgets.inactive.weak_bg_fill = PANEL_2;
    visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.inactive.corner_radius = CornerRadius::from(4);

    visuals.widgets.hovered.bg_fill = PANEL_3;
    visuals.widgets.hovered.weak_bg_fill = PANEL_3;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, LINE);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.hovered.corner_radius = CornerRadius::from(4);

    visuals.widgets.active.bg_fill = ACCENT_DARK;
    visuals.widgets.active.weak_bg_fill = ACCENT_DARK;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.active.corner_radius = CornerRadius::from(4);

    visuals.widgets.open.bg_fill = PANEL_3;
    visuals.widgets.open.weak_bg_fill = PANEL_3;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.open.corner_radius = CornerRadius::from(4);

    visuals.slider_trailing_fill = true;
    visuals.handle_shape = egui::style::HandleShape::Circle;

    let mut style = (*ctx.global_style()).clone();
    style.visuals = visuals.clone();
    style.spacing.item_spacing = Vec2::new(9.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 4.0);
    style.spacing.slider_width = 150.0;
    style.spacing.slider_rail_height = 4.0;
    style.spacing.interact_size.y = 24.0;
    // IDEA 风格的细滚动条：常驻、灰色滑块，不使用白色前景色。
    // IDEA-like thin scrollbars: always visible with a gray handle instead of a white foreground.
    let mut scroll = egui::style::ScrollStyle::solid();
    scroll.bar_width = 8.0;
    scroll.bar_inner_margin = 2.0;
    scroll.foreground_color = false;
    style.spacing.scroll = scroll;
    style.text_styles = [
        (egui::TextStyle::Heading, egui::FontId::proportional(16.0)),
        (egui::TextStyle::Body, egui::FontId::proportional(12.5)),
        (egui::TextStyle::Monospace, egui::FontId::monospace(13.0)),
        (egui::TextStyle::Button, egui::FontId::proportional(12.0)),
        (egui::TextStyle::Small, egui::FontId::proportional(10.5)),
    ]
    .into();
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_global_style(style);
    ctx.all_styles_mut(|style| {
        style.visuals = visuals.clone();
    });
}

pub fn track_color(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// One color out of the 16 × 16 MIDI-clip palette (256 colors).
pub fn palette_color(index: u8) -> Color32 {
    let i = index as u32;
    let column = (i % 16) as f32;
    let row = (i / 16) as f32;
    let hue = column / 16.0;
    let saturation = (0.95 - row / 42.0).clamp(0.55, 1.0);
    let value = 0.34 + (row / 15.0) * 0.66;
    hsv_to_rgb(hue, saturation, value)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Color32 {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Color32::from_rgb(
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

pub fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// Draw a horizontal or vertical peak meter.
pub fn level_meter(painter: &egui::Painter, rect: egui::Rect, level: f32, vertical: bool) {
    painter.rect_filled(rect, CornerRadius::from(2), BG_DEEP);
    let level = level.clamp(0.0, 1.0).powf(0.65);
    let color = if level > 0.92 {
        RED
    } else if level > 0.72 {
        AMBER
    } else {
        GREEN
    };
    if vertical {
        let filled = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.bottom() - rect.height() * level),
            rect.max,
        );
        painter.rect_filled(filled, CornerRadius::from(2), color);
    } else {
        let filled = egui::Rect::from_min_max(
            rect.min,
            egui::pos2(rect.left() + rect.width() * level, rect.bottom()),
        );
        painter.rect_filled(filled, CornerRadius::from(2), color);
    }
}

pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::NONE
        .fill(with_alpha(color, 32))
        .stroke(Stroke::new(1.0_f32, with_alpha(color, 130)))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).color(color).size(10.0));
        });
}

/// 所属模块：ui / 主题
/// 功能：绘制 28×14 的二态开关（效果器卡片、设置页与钢琴卷帘共用）。
/// 输入：
/// - `ui`：当前布局。
/// - `value`：开关状态；点击时取反。
/// 输出/副作用：返回开关的响应；点击时写入 `value`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / theme
/// Purpose: Draw the 28×14 two-state switch shared by effect cards, settings and the piano roll.
/// Inputs:
/// - `ui`: current layout.
/// - `value`: switch state; toggled on click.
/// Outputs / side effects: the switch response; mutates `value` when clicked.
/// Complexity: O(1) time and space.
pub fn toggle_switch(ui: &mut egui::Ui, value: &mut bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 14.0), egui::Sense::click());
    let enabled = *value;
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::from(7),
        if enabled { ACCENT } else { PANEL_3 },
    );
    let knob_x = if enabled {
        rect.right() - 7.0
    } else {
        rect.left() + 7.0
    };
    painter.circle_filled(
        egui::pos2(knob_x, rect.center().y),
        5.0,
        if enabled { Color32::WHITE } else { TEXT_DIM },
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        *value = !*value;
    }
    response
}
