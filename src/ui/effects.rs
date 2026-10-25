//! 所属模块：ui / 效果器链卷帘
//! 文件用途：
//! - 绘制效果器链卡片、信号顺序、拖拽排序与末尾「＋ 添加效果器」。
//! - 卡片主体只显示插件原生界面（自动内嵌、等比缩放到卡片大小）；双击标题在独立窗口打开。
//! - 卡片右上角的开关控制该效果器是否生效（旁路），停用时隐藏内嵌界面。
//! 函数清单：
//! - `SurfingApp::ui_effects`：效果器链卷帘的唯一绘制与交互入口。
//!
//! Module: ui / effect chain roll
//! Purpose:
//! - Draw effect cards, signal order, drag-to-reorder and the trailing "add effect" card.
//! - The card body shows only the native editor (embedded automatically and scaled to fit);
//!   double-clicking the header opens it in a standalone window.
//! - The switch at the card's top-right enables/bypasses the effect and hides its embedded GUI
//!   while disabled.
//! Functions:
//! - `SurfingApp::ui_effects`: the single paint/interaction entry point of the effect roll.

use crate::app::{RollKind, SurfingApp};
use crate::audio::SlotKind;
use crate::ui::{theme, PluginTarget};
use eframe::egui::{
    self, Align2, CornerRadius, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};

const DEFAULT_CARD_W: f32 = 430.0;
const MIN_CARD_W: f32 = 300.0;
const GAP: f32 = 10.0;
const HEADER_H: f32 = 30.0;
/// 卡片头部启用开关的宽度（逻辑点）。
/// Width of the enable switch in the card header (logical points).
const TOGGLE_W: f32 = 28.0;
/// 卡片头部启用开关的高度（逻辑点）。
/// Height of the enable switch in the card header (logical points).
const TOGGLE_H: f32 = 14.0;
/// 启用开关的圆角与滑块半径（逻辑点）。
/// Corner radius of the switch track and radius of its knob (logical points).
const TOGGLE_RADIUS: f32 = 7.0;

impl SurfingApp {
    /// 所属模块：ui / 效果器链
    /// 功能：绘制一整条效果器链，返回前应用排序、删除与参数改动。
    /// 输入：
    /// - `ui`：底部面板或浮动窗口的绘制上下文。
    /// - `track_id`：轨道 id；`MASTER_TRACK_ID` 表示主输出链。
    /// - `floating`：是否在独立浮动卷帘中绘制。
    /// 输出/副作用：绘制界面；修改工程效果器链、启用开关、参数与状态栏；可能打开独立插件编辑器。
    /// 复杂度：每帧 O(可见卡片数)，插件尺寸查询另计。
    ///
    /// Module: ui / effect chain
    /// Purpose: Draw one effect chain and apply reorder/remove/enable/parameter edits before
    /// returning.
    /// Inputs:
    /// - `ui`: paint context of the bottom panel or the floating roll.
    /// - `track_id`: track id; `MASTER_TRACK_ID` selects the master chain.
    /// - `floating`: whether this is the detached roll.
    /// Outputs / side effects: paints the UI; modifies the project effect chain, the enable
    /// switches, parameters and the status bar; may open a standalone plugin editor.
    /// Complexity: O(visible cards) per frame plus plugin size queries.
    pub fn ui_effects(&mut self, ui: &mut egui::Ui, track_id: u64, floating: bool) {
        let is_master = track_id == crate::core::model::MASTER_TRACK_ID;
        let track_index = self.project.tracks.iter().position(|t| t.id == track_id);
        if !is_master && track_index.is_none() {
            return;
        }
        let track_index = track_index.unwrap_or(0);
        let slot_of = move |index: usize| {
            if is_master {
                SlotKind::Master
            } else {
                SlotKind::Effect(index)
            }
        };
        let track_name = if is_master {
            "主控 / MASTER".to_string()
        } else {
            self.project.tracks[track_index].name.clone()
        };
        let effect_refs: Vec<crate::core::model::EffectRef> = if is_master {
            self.project.master_effect.iter().cloned().collect()
        } else {
            self.project.tracks[track_index].effects.clone()
        };
        let effect_count = effect_refs.len();
        let title_color = if is_master {
            theme::ACCENT
        } else {
            theme::track_color(self.project.tracks[track_index].color)
        };

        let panel_rect = ui.available_rect_before_wrap();
        let mut controls: Vec<egui::Rect> = Vec::new();
        let row = ui
            .horizontal(|ui| {
                let title_response = ui.add(
                    egui::Label::new(
                        RichText::new("⠿ 效果器链")
                            .color(title_color)
                            .strong()
                            .size(13.0),
                    )
                    .sense(egui::Sense::click_and_drag()),
                );
                controls.push(title_response.rect);
                if !floating && !is_master && title_response.drag_started() {
                    self.begin_roll_drag(ui, track_id, RollKind::Effects, panel_rect);
                }
                title_response.on_hover_text("拖动工具条可把卷帘拖出为独立窗口");
                ui.label(RichText::new(&track_name).size(10.0).color(theme::TEXT_DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !floating {
                        let close = ui.small_button("×").on_hover_text("关闭卷帘");
                        controls.push(close.rect);
                        if close.clicked() {
                            self.bottom_open = false;
                        }
                    }
                });
            })
            .response;
        self.roll_toolbar_drag(
            ui,
            row.rect,
            &controls,
            Some(track_id),
            RollKind::Effects,
            panel_rect,
            !floating && !is_master,
        );
        ui.add_space(4.0);

        let cached_can_resize: Vec<Option<bool>> = (0..effect_count)
            .map(|index| self.effect_can_resize.get(&(track_id, index)).copied())
            .collect();
        let (has_editors, failures, can_resizes) = {
            let engine = self.engine.clone();
            // 绝不让绘制阻塞在后台插件同步上。
            // Never block drawing behind a background plugin sync.
            let guard = engine.try_lock();
            match guard {
                Ok(engine) => {
                    let mut has_editors = Vec::with_capacity(effect_count);
                    let mut failures = Vec::with_capacity(effect_count);
                    let mut can_resizes = Vec::with_capacity(effect_count);
                    for index in 0..effect_count {
                        // 内置效果器没有插件编辑器，避免向引擎查询并误报“没有界面”。
                        // Native effects have no plugin editor, so skip the engine query.
                        if effect_refs[index].plugin().is_none() {
                            has_editors.push(false);
                            failures.push(None);
                            can_resizes.push(Some(false));
                            continue;
                        }
                        let slot = slot_of(index);
                        has_editors.push(engine.has_editor(track_id, slot));
                        failures.push(engine.is_plugin_failed(track_id, slot));
                        can_resizes.push(
                            cached_can_resize[index]
                                .or_else(|| engine.plugin_editor_can_resize(track_id, slot)),
                        );
                    }
                    (has_editors, failures, can_resizes)
                }
                // 引擎锁忙时沿用上一帧的能力缓存，避免误显示“没有界面”。
                // Reuse the cached capabilities while the engine lock is busy so the card
                // does not falsely claim the plugin has no GUI.
                Err(_) => (
                    (0..effect_count)
                        .map(|index| {
                            self.effect_editor_flags
                                .get(&(track_id, slot_of(index)))
                                .copied()
                                .unwrap_or(false)
                        })
                        .collect(),
                    vec![None; effect_count],
                    vec![None; effect_count],
                ),
            }
        };
        for (index, has_editor) in has_editors.iter().enumerate() {
            self.effect_editor_flags
                .insert((track_id, slot_of(index)), *has_editor);
        }

        let ppp = ui.ctx().pixels_per_point();
        for (index, can) in can_resizes.iter().enumerate() {
            if let Some(can) = can {
                self.effect_can_resize.insert((track_id, index), *can);
            }
        }

        let effect_names: Vec<String> = effect_refs.iter().map(|effect| effect.label()).collect();
        let formats: Vec<&'static str> = effect_refs.iter().map(|effect| effect.badge()).collect();
        // 启用状态取自工程模型，卡片开关只切换它，不重建插件实例。
        // Enable flags come from the project model; the card switch only flips them and never
        // rebuilds plugin instances.
        let enabled_flags: Vec<bool> = effect_refs.iter().map(|effect| effect.enabled()).collect();

        // 卡片宽度不再跟随插件声明的界面尺寸：GUI 会按卡片大小自动等比缩放。
        // Card width no longer follows the plugin's declared editor size: the GUI scales
        // itself to fit the card.
        let min_widths: Vec<f32> = vec![MIN_CARD_W; effect_count];
        let resizable: Vec<bool> = (0..effect_count)
            .map(|index| {
                self.effect_can_resize
                    .get(&(track_id, index))
                    .copied()
                    .unwrap_or(true)
            })
            .collect();
        let widths: Vec<f32> = (0..effect_count)
            .map(|index| {
                if !resizable[index] {
                    return min_widths[index];
                }
                let saved = self
                    .effect_widths
                    .get(&(track_id, index))
                    .copied()
                    .unwrap_or(DEFAULT_CARD_W);
                saved.max(min_widths[index]).clamp(MIN_CARD_W, 1600.0)
            })
            .collect();
        let mut card_lefts = Vec::with_capacity(effect_count);
        {
            let mut x = 0.0_f32;
            for width in &widths {
                card_lefts.push(x);
                x += width + GAP;
            }
        }

        let available = ui.available_size();
        if available.y < 50.0 || available.x < 80.0 {
            return;
        }
        let (response, painter) = ui.allocate_painter(available, Sense::click_and_drag());
        let rect = response.rect;
        painter.rect_filled(rect, CornerRadius::from(6), theme::BG_DEEP);

        let plus_width = (DEFAULT_CARD_W * 0.6).max(220.0);
        let total_width = card_lefts.last().copied().unwrap_or(0.0)
            + widths.last().copied().unwrap_or(0.0)
            + GAP
            + plus_width
            + 16.0;
        let max_scroll = (total_width - rect.width()).max(0.0);
        // 用指针位置判断是否在卷帘内，而不是 response.hovered()：
        // 卡片头部与内嵌界面会抢占悬停状态，导致滚轮被忽略。
        // Test the pointer position instead of `response.hovered()`: card headers and embedded
        // editors take over hover, which used to swallow the wheel.
        let pointer_over = ui
            .input(|i| i.pointer.hover_pos())
            .is_some_and(|pos| rect.contains(pos));
        if pointer_over {
            // Shift+滚轮（部分平台只给横向增量）与普通滚轮都用于左右滑动效果器链。
            // Shift+wheel (some platforms only report a horizontal delta) and the plain wheel both
            // scroll the effect chain horizontally.
            let scroll = ui.input(|i| {
                let mut delta = 0.0_f32;
                let mut from_events = false;
                for event in &i.events {
                    if let egui::Event::MouseWheel { delta: wheel, .. } = event {
                        from_events = true;
                        // 同时接受两轴，兼容把 Shift+滚轮报告为横轴的平台。
                        // Accept both axes so platforms that map Shift+wheel to the horizontal
                        // axis keep working.
                        delta += wheel.y + wheel.x;
                    }
                }
                if from_events {
                    delta * 50.0
                } else {
                    i.smooth_scroll_delta.x + i.smooth_scroll_delta.y
                }
            });
            if scroll.abs() > 0.1 {
                self.effects_scroll = (self.effects_scroll - scroll).clamp(0.0, max_scroll);
            }
        }
        self.effects_scroll = self.effects_scroll.clamp(0.0, max_scroll);

        let body_top = rect.top() + 6.0;
        let body_bottom = rect.bottom() - 6.0;
        let margin = 8.0 - self.effects_scroll;

        // 长按并拖动空白区域可把整条效果器链拖出为独立窗口；卡片头部仍用于排序。
        // Long-press and drag empty canvas space detaches the chain; card headers keep
        // their drag-to-reorder.
        if !floating && !is_master {
            let press_on_card = ui
                .input(|i| i.pointer.press_origin())
                .map(|origin| {
                    (0..effect_count).any(|index| {
                        Rect::from_min_size(
                            egui::pos2(rect.left() + margin + card_lefts[index], body_top),
                            Vec2::new(widths[index], body_bottom - body_top),
                        )
                        .contains(origin)
                    })
                })
                .unwrap_or(false);
            if self.try_detach_roll(ui, rect, track_id, RollKind::Effects, press_on_card) {
                return;
            }
        }

        let mut remove_effect: Option<usize> = None;
        let mut open_detached: Option<usize> = None;
        let mut toggle_effect: Option<usize> = None;
        let mut finish_drag = false;
        // 每帧最多创建一个内嵌编辑器：大界面插件初始化需要数百毫秒，
        // 一次创建多个会让界面长时间无响应。
        // Create at most one inline editor per frame: large editors take hundreds of
        // milliseconds to attach, and batching them would stall the UI.
        let mut created_inline = false;
        // 插件尚未同步完成时，运行时不完整；卡片应显示“正在加载”而不是“没有界面”。
        // While a sync is pending the runtime is incomplete; show "loading" instead of "no GUI".
        let loading_plugins = self.engine_sync_pending();
        // 本帧双击位置：卡片顶部栏任意位置（开关与关闭按钮除外）都打开独立界面。
        // Double-click position this frame: anywhere on the card header except the switch and the
        // close button opens the standalone editor.
        let header_double_pos = ui.input(|i| {
            if i.pointer
                .button_double_clicked(egui::PointerButton::Primary)
            {
                i.pointer.interact_pos()
            } else {
                None
            }
        });

        for index in 0..effect_count {
            let width = widths[index];
            let card_x = rect.left() + margin + card_lefts[index];
            let card = Rect::from_min_size(
                egui::pos2(card_x, body_top),
                Vec2::new(width, body_bottom - body_top),
            );
            if card.right() < rect.left() || card.left() > rect.right() {
                continue;
            }
            let header_rect = Rect::from_min_size(card.min, Vec2::new(card.width(), HEADER_H));
            let body_rect =
                Rect::from_min_max(egui::pos2(card.left(), header_rect.bottom()), card.max);
            painter.rect_filled(card, CornerRadius::from(8), theme::PANEL_2);
            let card_enabled = enabled_flags[index];
            painter.rect_filled(
                header_rect,
                CornerRadius::from(8),
                if !card_enabled {
                    theme::with_alpha(theme::TEXT_FAINT, 14)
                } else if self.effect_drag_from == Some(index) {
                    theme::with_alpha(theme::ACCENT, 60)
                } else {
                    theme::with_alpha(theme::ACCENT, 22)
                },
            );
            let selected_card = self.selected_effect == Some((track_id, index));
            painter.rect_stroke(
                card,
                CornerRadius::from(8),
                Stroke::new(
                    if selected_card { 1.8_f32 } else { 1.0_f32 },
                    if selected_card {
                        egui::Color32::WHITE
                    } else {
                        theme::LINE
                    },
                ),
                StrokeKind::Inside,
            );

            // 头部：拖动排序，双击打开独立界面。
            // Header: drag to reorder, double-click to open the standalone UI.
            let header_id = ui.id().with(("fx_header", track_id, index));
            let header_resp = ui.interact(header_rect, header_id, Sense::click_and_drag());
            if header_resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            header_resp.clone().on_hover_text(if has_editors[index] {
                "双击在独立窗口打开插件界面；拖动可排序"
            } else {
                "拖动可排序"
            });
            if header_resp.clicked() {
                self.selected_effect = Some((track_id, index));
                self.selected_controller = None;
            }
            if header_resp.drag_started() {
                self.effect_drag_from = Some(index);
                self.effect_drag_target = Some(index);
            }
            if header_resp.dragged() {
                if let Some(pos) = header_resp.interact_pointer_pos() {
                    let mut target = index;
                    for (candidate, left) in card_lefts.iter().enumerate() {
                        let center = rect.left() + margin + left + widths[candidate] * 0.5;
                        if pos.x <= center {
                            target = candidate;
                            break;
                        }
                        target = candidate;
                    }
                    self.effect_drag_target = Some(target.min(effect_count.saturating_sub(1)));
                }
            }
            if header_resp.drag_stopped() {
                finish_drag = true;
            }

            let mut remove = false;
            let title = format!("{}. {}", index + 1, effect_names[index]);
            let title_color = if card_enabled {
                theme::TEXT
            } else {
                theme::TEXT_FAINT
            };
            let galley = painter.layout_no_wrap(title, FontId::proportional(11.0), title_color);
            let title_pos = egui::pos2(
                header_rect.left() + 8.0,
                header_rect.center().y - galley.size().y * 0.5,
            );
            // 标题不进入开关与关闭按钮的区域。
            // Keep the title clear of the switch and the close button.
            let title_clip = Rect::from_min_max(
                header_rect.min,
                egui::pos2(header_rect.right() - 70.0, header_rect.bottom()),
            );
            painter
                .with_clip_rect(title_clip)
                .galley(title_pos, galley.clone(), title_color);
            let badge_rect = Rect::from_min_size(
                egui::pos2(
                    title_pos.x + galley.size().x + 8.0,
                    header_rect.center().y - 7.0,
                ),
                Vec2::new(38.0, 14.0),
            );
            if badge_rect.right() < header_rect.right() - 70.0 {
                painter.rect_filled(
                    badge_rect,
                    CornerRadius::from(4),
                    theme::with_alpha(
                        if formats[index] == "VST3" {
                            theme::ACCENT
                        } else {
                            theme::AMBER
                        },
                        32,
                    ),
                );
                painter.text(
                    badge_rect.center(),
                    Align2::CENTER_CENTER,
                    formats[index],
                    FontId::monospace(8.0),
                    if formats[index] == "VST3" {
                        theme::ACCENT
                    } else {
                        theme::AMBER
                    },
                );
            }
            // 右上角启用开关：控制该效果器是否参与处理（旁路）。
            // Top-right enable switch: whether this effect processes audio or is bypassed.
            let toggle_rect = Rect::from_center_size(
                egui::pos2(header_rect.right() - 48.0, header_rect.center().y),
                Vec2::new(TOGGLE_W, TOGGLE_H),
            );
            let toggle_resp = ui
                .interact(
                    toggle_rect,
                    ui.id().with(("fx_enabled", track_id, index)),
                    Sense::click(),
                )
                .on_hover_text(if card_enabled {
                    "停用该效果器（旁路）"
                } else {
                    "启用该效果器"
                });
            if toggle_resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            painter.rect_filled(
                toggle_rect,
                CornerRadius::from(TOGGLE_RADIUS),
                if card_enabled {
                    theme::ACCENT
                } else {
                    theme::PANEL_3
                },
            );
            if toggle_resp.hovered() {
                painter.rect_stroke(
                    toggle_rect,
                    CornerRadius::from(TOGGLE_RADIUS),
                    Stroke::new(
                        1.0_f32,
                        if card_enabled {
                            egui::Color32::WHITE
                        } else {
                            theme::LINE
                        },
                    ),
                    StrokeKind::Inside,
                );
            }
            let knob_x = if card_enabled {
                toggle_rect.right() - 7.0
            } else {
                toggle_rect.left() + 7.0
            };
            painter.circle_filled(
                egui::pos2(knob_x, toggle_rect.center().y),
                5.0,
                if card_enabled {
                    egui::Color32::WHITE
                } else {
                    theme::TEXT_FAINT
                },
            );
            if toggle_resp.clicked() {
                toggle_effect = Some(index);
            }

            let close_rect = Rect::from_min_max(
                egui::pos2(header_rect.right() - 30.0, header_rect.top() + 4.0),
                egui::pos2(header_rect.right() - 6.0, header_rect.bottom() - 4.0),
            );
            if ui
                .put(close_rect, egui::Button::new("×").small())
                .on_hover_text("移除效果器")
                .clicked()
            {
                remove = true;
            }
            if remove {
                remove_effect = Some(index);
            }
            // 双击顶部栏任意位置（除开关与关闭按钮）都打开独立界面。
            // Double-clicking anywhere on the header (except the switch and close button) opens the
            // standalone editor.
            if let Some(pos) = header_double_pos {
                if has_editors[index]
                    && header_rect.contains(pos)
                    && !toggle_rect.contains(pos)
                    && !close_rect.contains(pos)
                {
                    open_detached = Some(index);
                }
            }

            // 卡片主体只显示插件原生界面：底部卷帘自动内嵌，浮动卷帘提示用独立窗口。
            // The card body shows only the native GUI: the bottom roll embeds it
            // automatically, while a floating roll points at the standalone window.
            let slot = slot_of(index);
            let inline_failed = self.inline_failed.get(&(track_id, slot)).cloned();
            let is_native = effect_refs[index].native().is_some();
            if is_native {
                // 内置效果器直接在卡片内绘制参数，不创建插件编辑器。
                // Native effects paint their parameters in the card and never create an editor.
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(body_rect.shrink2(Vec2::new(10.0, 8.0)))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                    |ui| {
                        ui.set_clip_rect(body_rect);
                        if !card_enabled {
                            ui.label(
                                RichText::new("效果器已停用，点击标题栏右侧开关启用")
                                    .size(9.0)
                                    .color(theme::TEXT_FAINT),
                            );
                            return;
                        }
                        let changed = if is_master {
                            self.project
                                .master_effect
                                .as_mut()
                                .and_then(|effect| effect.native_mut())
                                .map(|effect| native_effect_parameters_ui(ui, effect))
                                .unwrap_or(false)
                        } else {
                            self.project
                                .tracks
                                .get_mut(track_index)
                                .and_then(|track| track.effects.get_mut(index))
                                .and_then(|effect| effect.native_mut())
                                .map(|effect| native_effect_parameters_ui(ui, effect))
                                .unwrap_or(false)
                        };
                        if changed {
                            if !is_master {
                                if let Some(track) = self.project.tracks.get_mut(track_index) {
                                    track.bump_revision();
                                }
                            }
                            self.mark_dirty();
                            self.status = "已修改内置效果器参数".into();
                        }
                    },
                );
            } else if card_enabled
                && !floating
                && has_editors[index]
                && !self.has_inline_editor(track_id, slot)
                && !self.has_editor_window(track_id, slot)
                && inline_failed.is_none()
                && !created_inline
            {
                created_inline = self.open_inline_editor(track_id, slot);
            }

            // 停用的效果器不摆放内嵌界面，帧末会被隐藏，卡片显示旁路提示。
            // A disabled effect places no inline editor (hidden at frame end) and shows a
            // bypass hint instead.
            if !is_native {
                let inline_active =
                    card_enabled && !floating && self.has_inline_editor(track_id, slot);
                if inline_active {
                    self.place_inline_editor(
                        track_id,
                        slot,
                        body_rect.shrink2(Vec2::new(6.0, 6.0)),
                        rect,
                        ppp,
                    );
                } else {
                    let message = if !card_enabled {
                        "效果器已停用，点击标题栏右侧开关启用".to_string()
                    } else if let Some(error) = &failures[index] {
                        format!("加载失败：{error}")
                    } else if floating {
                        "浮动卷帘不内嵌界面，双击标题可在独立窗口打开".to_string()
                    } else if let Some(error) = &inline_failed {
                        format!("无法内嵌界面：{error}；双击标题可在独立窗口打开")
                    } else if loading_plugins {
                        "正在加载插件…".to_string()
                    } else if has_editors[index] {
                        "正在准备插件界面…".to_string()
                    } else {
                        "该插件没有可内嵌的界面，双击标题可在独立窗口打开".to_string()
                    };
                    let failed = failures[index].is_some();
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(body_rect.shrink2(Vec2::new(8.0, 6.0)))
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                        |ui| {
                            ui.set_clip_rect(body_rect);
                            ui.label(RichText::new(message).size(9.0).color(if failed {
                                theme::RED
                            } else {
                                theme::TEXT_FAINT
                            }));
                        },
                    );
                }
            }

            // 右边缘宽度拖拽手柄（固定尺寸/宽高比插件不显示）。
            // Right-edge resize handle (hidden for fixed-size/aspect plugins).
            if resizable[index] {
                let handle = Rect::from_min_max(
                    egui::pos2(card.right() - 4.0, card.top()),
                    egui::pos2(card.right() + 2.0, card.bottom()),
                );
                let resize_resp = ui.interact(
                    handle,
                    ui.id().with(("fx_resize", track_id, index)),
                    Sense::drag(),
                );
                if resize_resp.hovered() || resize_resp.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
                if resize_resp.dragged() {
                    if let Some(pos) = resize_resp.interact_pointer_pos() {
                        let new_width = (pos.x - card.left()).clamp(min_widths[index], 1600.0);
                        self.effect_widths.insert((track_id, index), new_width);
                    }
                }
                painter.rect_filled(
                    Rect::from_min_max(
                        egui::pos2(card.right() - 2.0, card.top() + 10.0),
                        egui::pos2(card.right(), card.bottom() - 10.0),
                    ),
                    CornerRadius::from(1),
                    if resize_resp.hovered() || resize_resp.dragged() {
                        theme::ACCENT
                    } else {
                        theme::with_alpha(theme::LINE, 160)
                    },
                );
            }

            if index + 1 < effect_count {
                let arrow_x = card.right() + GAP * 0.5;
                painter.text(
                    egui::pos2(arrow_x, card.center().y),
                    Align2::CENTER_CENTER,
                    "→",
                    FontId::proportional(14.0),
                    theme::TEXT_FAINT,
                );
            }
        }

        // 拖拽排序时的插入位置标记。
        // Insertion marker while dragging a card.
        if let (Some(from), Some(target)) = (self.effect_drag_from, self.effect_drag_target) {
            if from != target && target < effect_count {
                let x = rect.left() + margin + card_lefts[target];
                painter.rect_filled(
                    Rect::from_min_max(
                        egui::pos2(x - 2.0, body_top),
                        egui::pos2(x + 2.0, body_bottom),
                    ),
                    CornerRadius::from(1),
                    theme::ACCENT,
                );
            }
        }

        // 末尾「＋」卡片。
        // Trailing "+" card.
        let plus_x = rect.left()
            + margin
            + card_lefts.last().copied().unwrap_or(0.0)
            + widths.last().copied().unwrap_or(0.0)
            + GAP;
        let plus_rect = Rect::from_min_size(
            egui::pos2(plus_x, body_top),
            Vec2::new(plus_width, body_bottom - body_top),
        );
        if plus_rect.right() > rect.left() && plus_rect.left() < rect.right() {
            painter.rect_filled(
                plus_rect,
                CornerRadius::from(8),
                theme::with_alpha(theme::ACCENT, 10),
            );
            painter.rect_stroke(
                plus_rect,
                CornerRadius::from(8),
                Stroke::new(1.0_f32, theme::with_alpha(theme::ACCENT, 120)),
                StrokeKind::Inside,
            );
            painter.text(
                plus_rect.center() - Vec2::new(0.0, 8.0),
                Align2::CENTER_CENTER,
                "+",
                FontId::proportional(34.0),
                theme::ACCENT,
            );
            painter.text(
                plus_rect.center() + Vec2::new(0.0, 18.0),
                Align2::CENTER_CENTER,
                "添加效果器",
                FontId::proportional(11.0),
                theme::TEXT_DIM,
            );
            let plus_response = ui.interact(
                plus_rect,
                ui.id().with(("plus_effect", track_id)),
                Sense::click(),
            );
            if plus_response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            // 点击末尾卡片直接打开选择窗；内置与 VST 效果器都在同一窗口中选择。
            // The trailing card opens the picker directly; native and VST effects share it.
            if plus_response.clicked() {
                if is_master {
                    self.open_browser(PluginTarget::MasterEffect);
                } else {
                    self.selected_track = track_index;
                    self.open_browser(PluginTarget::Effect);
                }
                return;
            }
        }

        if effect_count == 0 {
            painter.text(
                egui::pos2(rect.center().x, rect.top() + 22.0),
                Align2::CENTER_CENTER,
                if is_master {
                    "主控还没有效果器，点击右侧 + 添加"
                } else {
                    "该轨道还没有效果器，点击右侧 + 添加"
                },
                FontId::proportional(12.0),
                theme::TEXT_DIM,
            );
        }

        // 完成拖拽排序。
        // Finish a drag-reorder.
        if finish_drag && !is_master {
            if let (Some(from), Some(to)) = (self.effect_drag_from, self.effect_drag_target) {
                if from != to && from < effect_count && to < effect_count {
                    self.editors.retain(|((id, slot), _)| {
                        !(*id == track_id && matches!(slot, SlotKind::Effect(_)))
                    });
                    if let Some(track) = self.project.tracks.get_mut(track_index) {
                        let effect = track.effects.remove(from);
                        track.effects.insert(to.min(track.effects.len()), effect);
                        track.bump_revision();
                    }
                    self.effect_widths.retain(|(id, _), _| *id != track_id);
                    self.effect_can_resize.retain(|(id, _), _| *id != track_id);
                    self.inline_failed.retain(|(id, _), _| *id != track_id);
                    self.effect_editor_flags
                        .retain(|(id, _), _| *id != track_id);
                    self.mark_dirty();
                    self.status = "已调整效果器顺序".into();
                }
            }
            self.effect_drag_from = None;
            self.effect_drag_target = None;
        }

        // 绘制结束后应用链编辑。
        // Apply chain edits after drawing.
        if let Some(index) = remove_effect {
            self.selected_effect = None;
            if is_master {
                self.editors
                    .retain(|((id, slot), _)| !(*id == track_id && *slot == SlotKind::Master));
                self.project.master_effect = None;
            } else {
                self.editors.retain(|((id, slot), _)| {
                    !(*id == track_id && matches!(slot, SlotKind::Effect(_)))
                });
                if let Some(track) = self.project.tracks.get_mut(track_index) {
                    if index < track.effects.len() {
                        track.effects.remove(index);
                        track.bump_revision();
                    }
                }
            }
            self.effect_widths.retain(|(id, _), _| *id != track_id);
            self.effect_can_resize.retain(|(id, _), _| *id != track_id);
            self.inline_failed.retain(|(id, _), _| *id != track_id);
            self.effect_editor_flags
                .retain(|(id, _), _| *id != track_id);
            self.mark_dirty();
            self.status = "已移除效果器".into();
            return;
        }

        // 切换启用状态：只改模型与听感版本，插件实例保持加载。
        // Toggle the enable flag: only the model and its audible revision change; the plugin
        // instance stays loaded.
        if let Some(index) = toggle_effect {
            let mut now_enabled = None;
            if is_master {
                if let Some(effect) = &mut self.project.master_effect {
                    let enabled = !effect.enabled();
                    effect.set_enabled(enabled);
                    now_enabled = Some(enabled);
                }
            } else if let Some(track) = self.project.tracks.get_mut(track_index) {
                if let Some(effect) = track.effects.get_mut(index) {
                    let enabled = !effect.enabled();
                    effect.set_enabled(enabled);
                    now_enabled = Some(enabled);
                }
                if now_enabled.is_some() {
                    track.bump_revision();
                }
            }
            if let Some(enabled) = now_enabled {
                self.mark_dirty();
                self.status = if enabled {
                    "已启用效果器".into()
                } else {
                    "已停用效果器（旁路）".into()
                };
            }
        }

        if let Some(index) = open_detached {
            self.open_editor(track_id, slot_of(index));
        }

        painter.rect_stroke(
            rect,
            CornerRadius::from(6),
            Stroke::new(1.0_f32, theme::LINE),
            StrokeKind::Inside,
        );
    }
}

/// 所属模块：ui / 效果器链
/// 功能：绘制内置效果器的参数滑块（检查器与效果器卡片共用）。
/// 输入：
/// - `ui`：绘制上下文。
/// - `effect`：内置效果器引用。
/// 输出/副作用：修改 `effect.settings` 并返回是否发生变化。
/// 复杂度：每帧 O(该效果器参数数)。
///
/// Module: ui / effect chain
/// Purpose: Draw native-effect parameter sliders (shared by the inspector and effect cards).
/// Inputs:
/// - `ui`: paint context.
/// - `effect`: native effect reference.
/// Outputs / side effects: mutates `effect.settings` and returns whether anything changed.
/// Complexity: O(parameters of this effect) per frame.
pub(crate) fn native_effect_parameters_ui(
    ui: &mut egui::Ui,
    effect: &mut crate::core::model::NativeEffect,
) -> bool {
    let mut changed = false;
    for param in effect.kind.params() {
        let value = effect.settings.get(*param);
        let mut edited = value;
        ui.horizontal(|ui| {
            ui.add_sized(
                [48.0, 18.0],
                egui::Label::new(
                    RichText::new(param.label())
                        .size(10.0)
                        .color(theme::TEXT_DIM),
                ),
            );
            if ui
                .add(
                    egui::Slider::new(&mut edited, 0.0..=1.0)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed()
            {
                changed = true;
            }
            ui.label(
                RichText::new(param.display(edited))
                    .size(9.0)
                    .color(theme::TEXT_FAINT),
            );
        });
        if (edited - value).abs() > f32::EPSILON {
            *effect.settings.get_mut(*param) = edited.clamp(0.0, 1.0);
        }
    }
    changed
}
