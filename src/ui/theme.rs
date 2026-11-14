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
//! - 设计令牌：`SPACE_*` / `FONT_*` / `RADIUS_*` / `CARD_PAD` / `CONTROL_H` / `FIELD_H`。
//! - 复用表面与控件：`card_frame` / `sheet_frame` / `floating_frame` / `field_label` /
//!   `section_title` / `primary_button` / `quiet_button` / `danger_button` / `chip`。
//! - `level_meter` / `badge` / `toggle_switch` / `menu_button`：主题内的复用绘制。
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
//! - Design tokens: `SPACE_*` / `FONT_*` / `RADIUS_*` / `CARD_PAD` / `CONTROL_H` / `FIELD_H`.
//! - Shared surfaces and controls: `card_frame` / `sheet_frame` / `floating_frame` / `field_label` /
//!   `section_title` / `primary_button` / `quiet_button` / `danger_button` / `chip`.
//! - `level_meter` / `badge` / `toggle_switch` / `menu_button`: reusable painters owned by the theme.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, Stroke, Vec2};
use std::sync::Arc;

// ----- 全局调色板（IDEA New UI Dark） --------------------------------------
// Global palette (IDEA New UI Dark).

/// 编辑区最底层：卷帘背景、输入框深底。
/// Deepest editor background: roll canvases and dark input backgrounds.
pub const BG_DEEP: Color32 = Color32::from_rgb(24, 25, 28);
/// 软件图标底色：深灰，与 `assets/icon.svg` 的背景一致。
/// App-logo background: dark grey, matching the background of `assets/icon.svg`.
pub const LOGO_BG: Color32 = Color32::from_rgb(46, 46, 51);
/// 软件图标音柱蓝色，与 `assets/icon.svg` 一致（#0078D4）。
/// App-logo bar blue, matching `assets/icon.svg` (#0078D4).
pub const LOGO_BLUE: Color32 = Color32::from_rgb(0, 120, 212);
/// 软件图标描边，与 `assets/icon.svg` 一致（#5C5C66）：避免深灰底与深色面板融为一体。
/// App-logo border, matching `assets/icon.svg` (#5C5C66); keeps the dark grey tile from blending
/// into dark panels.
pub const LOGO_BORDER: Color32 = Color32::from_rgb(92, 92, 102);
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

// ----- 设计令牌：间距 / 字号 / 圆角 ----------------------------------------
// Design tokens: spacing, type scale and radii.
// 主页、Agent 资料库与 AI 面板统一按这套令牌取间距与字号，避免同类页面各自写死数值。
// The home pages, Agent library and AI panel share these tokens so similar pages cannot drift.

/// 间距阶梯：4 / 8 / 12 / 16 / 24 / 32。
/// Spacing scale: 4 / 8 / 12 / 16 / 24 / 32.
pub const SPACE_XS: f32 = 4.0;
pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 12.0;
pub const SPACE_LG: f32 = 16.0;
pub const SPACE_XL: f32 = 24.0;
pub const SPACE_2XL: f32 = 32.0;

/// 字号阶梯：小字 / 标签 / 正文 / 小节标题 / 页面标题。
/// Type scale: micro / label / body / section / page title.
pub const FONT_MICRO: f32 = 10.5;
pub const FONT_LABEL: f32 = 11.0;
pub const FONT_BODY: f32 = 12.5;
pub const FONT_SECTION: f32 = 15.0;
pub const FONT_TITLE: f32 = 18.0;
pub const FONT_PAGE: f32 = 22.0;

/// 卡片与容器圆角、内边距。
/// Card and sheet radii plus inner padding.
pub const RADIUS_CARD: u8 = 10;
pub const RADIUS_SHEET: u8 = 12;
pub const CARD_PAD: i8 = 16;

/// 标准控件与表单输入高度。
/// Standard control and form-field heights.
pub const CONTROL_H: f32 = 28.0;
pub const FIELD_H: f32 = 36.0;

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
    // egui 在 debug 构建中默认开启“同一矩形在两帧间改变 id”的警告，并在下一帧绘制
    // 2 点红色边框；页面/按钮重排时用户会看到红框一闪而过。这里关闭该调试叠加与
    // 未对齐提示，避免把调试告警当成界面缺陷（Release 没有这些字段）。
    // egui enables a debug-only warning when a rect changes id between frames and paints a
    // 2-point red outline for one frame; page/button relayout made that flash visible. Disable
    // that overlay and the unaligned hint so debug warnings never look like UI defects
    // (release builds have no such fields).
    #[cfg(debug_assertions)]
    {
        style.debug.warn_if_rect_changes_id = false;
        style.debug.show_unaligned = false;
    }
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
        // 深/浅两套主题都关闭调试叠加，避免切换主题后又出现红色边框。
        // Disable the debug overlay for both themes so switching themes cannot bring the red
        // outline back.
        #[cfg(debug_assertions)]
        {
            style.debug.warn_if_rect_changes_id = false;
            style.debug.show_unaligned = false;
        }
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

/// 所属模块：ui / 主题 / 卡片
/// 功能：返回统一的行/卡片表面（`PANEL_2`、10 点圆角、16 点内边距、无边框）。
/// 输入：无。
/// 输出/副作用：新的 `Frame`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / theme / card
/// Purpose: Return the shared row/card surface (`PANEL_2`, 10 pt radius, 16 pt padding, no border).
/// Inputs: none.
/// Outputs / side effects: a new `Frame`; no state change.
/// Complexity: O(1) time and space.
pub fn card_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(PANEL_2)
        .corner_radius(RADIUS_CARD)
        .inner_margin(egui::Margin::same(CARD_PAD))
}

/// 所属模块：ui / 主题 / 卡片
/// 功能：返回页面级容器表面（`PANEL`、12 点圆角、16 点内边距、无边框），用于列表与编辑区。
/// 输入：无。
/// 输出/副作用：新的 `Frame`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / theme / card
/// Purpose: Return the page-level sheet surface (`PANEL`, 12 pt radius, 16 pt padding, no border)
/// used by lists and editors.
/// Inputs: none.
/// Outputs / side effects: a new `Frame`; no state change.
/// Complexity: O(1) time and space.
pub fn sheet_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(PANEL)
        .corner_radius(RADIUS_SHEET)
        .inner_margin(egui::Margin::same(CARD_PAD))
}

/// 所属模块：ui / 主题 / 卡片
/// 功能：返回浮动面板表面：`PANEL` 底、12 点圆角与柔和投影，让 AI 面板从编辑区里“浮”起来。
/// 输入：无。
/// 输出/副作用：新的 `Frame`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / theme / card
/// Purpose: Return the floating panel surface: `PANEL` fill, 12 pt radius and a soft drop shadow so
/// the AI panel lifts off the editor.
/// Inputs: none.
/// Outputs / side effects: a new `Frame`; no state change.
/// Complexity: O(1) time and space.
pub fn floating_frame() -> egui::Frame {
    sheet_frame().shadow(egui::epaint::Shadow {
        offset: [0, 6],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(140),
    })
}

/// 所属模块：ui / 主题 / 文字
/// 功能：绘制表单字段标签（11 点、次要色，位于输入框上方）。
/// 输入：`ui`；`text` 标签文本。
/// 输出/副作用：只绘制；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / theme / text
/// Purpose: Draw a form-field label (11 pt, secondary color, above the input).
/// Inputs: `ui`; `text`.
/// Outputs / side effects: paint only; no state change.
/// Complexity: O(1) time and space.
pub fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(FONT_LABEL).color(TEXT_DIM));
}

/// 所属模块：ui / 主题 / 文字
/// 功能：绘制小节标题（15 点、主文本、字重加强）。
/// 输入：`ui`；`text` 标题文本。
/// 输出/副作用：只绘制；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / theme / text
/// Purpose: Draw a section heading (15 pt, primary text, strong).
/// Inputs: `ui`; `text`.
/// Outputs / side effects: paint only; no state change.
/// Complexity: O(1) time and space.
pub fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(FONT_SECTION)
            .strong()
            .color(TEXT),
    );
}

/// 所属模块：ui / 主题 / 按钮
/// 功能：绘制主按钮：蓝色实底、白字、8 点圆角，一屏只保留一个。
/// 输入：`ui`；`label` 文本。
/// 输出/副作用：返回点击响应；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / theme / button
/// Purpose: Draw the primary button: solid accent fill, white text, 8 pt radius; one per view.
/// Inputs: `ui`; `label`.
/// Outputs / side effects: the click response; no state change.
/// Complexity: O(1) time and space.
pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .size(FONT_BODY)
                .color(Color32::WHITE),
        )
        .fill(ACCENT)
        .corner_radius(8)
        .min_size(egui::Vec2::new(72.0, CONTROL_H)),
    )
}

/// 所属模块：ui / 主题 / 按钮
/// 功能：绘制文字按钮：无背景、次要色，用于取消/导出等次要操作。
/// 输入：`ui`；`label` 文本。
/// 输出/副作用：返回点击响应；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / theme / button
/// Purpose: Draw a text button (no fill, secondary color) for cancel/export-style actions.
/// Inputs: `ui`; `label`.
/// Outputs / side effects: the click response; no state change.
/// Complexity: O(1) time and space.
pub fn quiet_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(FONT_LABEL).color(TEXT_DIM))
            .frame(false)
            .min_size(egui::Vec2::new(0.0, CONTROL_H)),
    )
}

/// 所属模块：ui / 主题 / 按钮
/// 功能：绘制危险文字按钮（红色文字、无背景）。
/// 输入：`ui`；`label` 文本。
/// 输出/副作用：返回点击响应；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / theme / button
/// Purpose: Draw a danger text button (red text, no fill).
/// Inputs: `ui`; `label`.
/// Outputs / side effects: the click response; no state change.
/// Complexity: O(1) time and space.
pub fn danger_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(FONT_LABEL).color(RED))
            .frame(false)
            .min_size(egui::Vec2::new(0.0, CONTROL_H)),
    )
}

/// 所属模块：ui / 主题 / 按钮
/// 功能：绘制筛选胶囊：选中时蓝底白字，未选中时浅表面、次要文字，无边框。
/// 输入：`ui`；`label` 文本；`selected` 是否选中。
/// 输出/副作用：返回点击响应；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / theme / chip
/// Purpose: Draw a filter chip: accent fill with white text when selected, a quiet surface otherwise,
/// and no border.
/// Inputs: `ui`; `label`; `selected`.
/// Outputs / side effects: the click response; no state change.
/// Complexity: O(1) time and space.
pub fn chip(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let (fill, color) = if selected {
        (ACCENT_DARK, Color32::WHITE)
    } else {
        (PANEL_2, TEXT_DIM)
    };
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(FONT_LABEL).color(color))
            .fill(fill)
            .stroke(egui::Stroke::NONE)
            .corner_radius(8)
            .min_size(egui::Vec2::new(0.0, 26.0)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
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
    // 标签只做低饱和底色，不再加描边：避免页面出现一圈圈细边框。
    // Tags use a low-saturation fill without a stroke so pages do not fill up with hairlines.
    egui::Frame::NONE
        .fill(with_alpha(color, 28))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).color(color).size(10.0));
        });
}

/// 所属模块：ui / 主题 / 菜单
/// 功能：绘制 IDEA 风格菜单项：左侧文字、右侧快捷键、整行可点击。
/// 输入：
/// - `ui`：弹出菜单内容区。
/// - `label`：菜单项文字。
/// - `shortcut`：右对齐的快捷键文本；空字符串表示不显示。
/// 输出/副作用：分配整行按钮并返回其响应；不修改应用状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / theme / menu
/// Purpose: Draw an IDEA-like menu item: label on the left, shortcut on the right, full-row click.
/// Inputs:
/// - `ui`: popup menu content area.
/// - `label`: item text.
/// - `shortcut`: right-aligned shortcut text; an empty string hides it.
/// Outputs / side effects: allocates a full-row button and returns its response; no app state.
/// Complexity: O(1) time and space.
pub fn menu_button(ui: &mut egui::Ui, label: &str, shortcut: &str) -> egui::Response {
    let label = egui::RichText::new(label).size(12.5);
    let shortcut = egui::RichText::new(shortcut).size(10.5).color(TEXT_DIM);
    ui.add(egui::Button::new(label).shortcut_text(shortcut))
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
