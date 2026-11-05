//! Right-hand inspector: track settings, instrument, effect chain and master bus.

use crate::app::{RollKind, SurfingApp};
use crate::audio::SlotKind;
use crate::core::model::{
    ControllerKind, DrumSettings, EffectRef, InstrumentRef, NativeEffectKind, NativeInstrument,
    NativeInstrumentKind, PluginRef, SampleInstrument, SampleSettings, SynthSettings, Waveform,
    bank_instruments,
};
use crate::plugin::{ParamView, PluginKind};
use crate::ui::{BottomView, PluginTarget, theme};
use eframe::egui::{self, RichText, Sense, Slider, Vec2};

#[derive(Clone, Copy, Debug)]
enum EffectAction {
    Remove(usize),
    Move(usize, isize),
}

impl SurfingApp {
    pub fn ui_inspector(&mut self, ui: &mut egui::Ui) {
        let midi_inputs = self.midi_input_ports();
        let Some(mut track) = self.current_track().cloned() else {
            ui.heading("SurfingStudio");
            return;
        };
        let track_index = self.selected_track;
        let track_id = track.id;
        let level = self.ui_levels.get(track_index).copied().unwrap_or(0.0);
        let output_layout = self.project.output_layout;
        let mut layout_choice = output_layout;

        // Parameter lists are served from the app cache (refreshed a few times
        // per second). Cloning them from the engine every frame would hold the
        // engine lock long enough to starve the audio callback.
        let instrument_params = self.cached_plugin_params(track_id, SlotKind::Instrument);
        let effect_params: Vec<Vec<ParamView>> = (0..track.effects.len())
            .map(|index| self.cached_plugin_params(track_id, SlotKind::Effect(index)))
            .collect();
        let master_params = self.cached_plugin_params(0, SlotKind::Master);
        let engine_handle = self.engine.clone();
        // A background plugin sync may hold the engine mutex; fall back to
        // cached/empty data instead of freezing the frame.
        let guard = engine_handle.try_lock();
        let (instrument_kind, effect_kinds, instrument_failure, effect_failures, master_failure) =
            match &guard {
                Ok(engine) => (
                    engine.plugin_kind(track_id, SlotKind::Instrument),
                    (0..track.effects.len())
                        .map(|index| engine.plugin_kind(track_id, SlotKind::Effect(index)))
                        .collect::<Vec<_>>(),
                    engine.is_plugin_failed(track_id, SlotKind::Instrument),
                    (0..track.effects.len())
                        .map(|index| engine.is_plugin_failed(track_id, SlotKind::Effect(index)))
                        .collect::<Vec<_>>(),
                    engine.is_plugin_failed(0, SlotKind::Master),
                ),
                Err(_) => (
                    None,
                    vec![None; track.effects.len()],
                    None,
                    vec![None; track.effects.len()],
                    None,
                ),
            };
        drop(guard);

        let mut track_changed = false;
        let mut open_editor: Option<SlotKind> = None;
        let mut refresh: Option<SlotKind> = None;
        let mut browse: Option<PluginTarget> = None;
        let mut add_native: Option<(u64, NativeEffectKind)> = None;
        let mut remove_effect: Option<usize> = None;
        let mut remove_master_effect = false;
        let mut remove_track = false;
        let mut remove_instrument = false;
        let mut effect_actions: Vec<EffectAction> = Vec::new();
        let mut param_changes: Vec<(SlotKind, usize, f64)> = Vec::new();
        let mut instrument_params = instrument_params;
        let mut effect_params = effect_params;
        let mut master_params = master_params;
        let mut master_volume = self.project.master_volume;
        let mut master_native_changed = false;
        // 选中绑定设备后需要自动启用该端口。
        // Port that must be enabled after the user binds it.
        let mut enable_input_device: Option<String> = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("轨道属性")
                            .color(theme::ACCENT)
                            .strong()
                            .size(14.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("删除轨道").clicked() {
                            remove_track = true;
                        }
                    });
                });
                ui.add_space(2.0);

                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut track.name)
                                .desired_width(150.0)
                                .hint_text("轨道名称"),
                        )
                        .changed()
                    {
                        track_changed = true;
                    }
                    let (swatch, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                    ui.painter().rect_filled(
                        swatch,
                        egui::CornerRadius::from(4),
                        theme::track_color(track.color),
                    );
                });
                // MIDI 输入绑定：显式绑定优先于默认设备。
                // MIDI input binding: an explicit binding takes priority over the default device.
                ui.horizontal(|ui| {
                    ui.label(RichText::new("MIDI 输入").color(theme::TEXT_DIM).size(11.0));
                    let current = track
                        .input_device
                        .clone()
                        .unwrap_or_else(|| "跟随默认设备".to_string());
                    egui::ComboBox::from_id_salt(("track_midi_input", track_id))
                        .width(180.0)
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(&mut track.input_device, None, "跟随默认设备")
                                .changed()
                            {
                                track_changed = true;
                            }
                            for port in &midi_inputs {
                                if ui
                                    .selectable_value(
                                        &mut track.input_device,
                                        Some(port.clone()),
                                        port,
                                    )
                                    .changed()
                                {
                                    track_changed = true;
                                    enable_input_device = Some(port.clone());
                                }
                            }
                        });
                });
                ui.label(
                    RichText::new("绑定后该设备直接控制本轨道；默认设备控制未绑定的选中轨道。")
                        .size(10.0)
                        .color(theme::TEXT_FAINT),
                );
                egui::CollapsingHeader::new("轨道颜色（256 色）")
                    .default_open(false)
                    .id_salt(("track_color", track_id))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::splat(2.0);
                        ui.horizontal_wrapped(|ui| {
                            for index in 0..=255u8 {
                                let color = theme::palette_color(index);
                                let rgb = [color.r(), color.g(), color.b()];
                                let selected = track.color == rgb;
                                let button = egui::Button::new("")
                                    .fill(color)
                                    .min_size(Vec2::splat(12.0))
                                    .stroke(egui::Stroke::new(
                                        if selected { 1.5_f32 } else { 0.0_f32 },
                                        egui::Color32::WHITE,
                                    ));
                                if ui.add(button).clicked() {
                                    track.color = rgb;
                                    track_changed = true;
                                }
                            }
                        });
                    });
                ui.add_space(4.0);

                // 组合轨道没有乐器：主轨作为音量控制器，效果器链处理成员混音。
                // A group has no instrument: its main lane is the volume controller and its effect
                // chain processes the summed member mix.
                if track.is_group() {
                    egui::Frame::NONE
                        .fill(theme::PANEL_2)
                        .corner_radius(8)
                        .inner_margin(egui::Margin::same(9))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("组合轨道")
                                    .color(theme::TEXT)
                                    .size(12.5)
                                    .strong(),
                            );
                            ui.add_space(2.0);
                            ui.label(
                                RichText::new(
                                    "主轨为音量控制器；下方效果器子轨修饰成员混合后的声音，可继续与其它轨道组成外层组合。",
                                )
                                .size(10.5)
                                .color(theme::TEXT_DIM),
                            );
                        });
                } else {
                // Instrument.
                egui::Frame::NONE
                    .fill(theme::PANEL_2)
                    .corner_radius(8)
                    .inner_margin(egui::Margin::same(9))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("乐器").color(theme::TEXT_DIM).size(11.0));
                            if let Some(kind) = instrument_kind {
                                theme::badge(
                                    ui,
                                    kind.label(),
                                    if kind == PluginKind::Instrument {
                                        theme::VIOLET
                                    } else {
                                        theme::TEXT_FAINT
                                    },
                                );
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("浏览插件…").clicked() {
                                        browse = Some(PluginTarget::Instrument);
                                    }
                                },
                            );
                        });
                        ui.add_space(3.0);

                        let old_kind = match &track.instrument {
                            InstrumentRef::Synth(_) => 0,
                            InstrumentRef::Drums(_) => 1,
                            InstrumentRef::Native(_) => 2,
                            InstrumentRef::Sample(_) => 3,
                            InstrumentRef::Plugin(_) => 4,
                        };
                        let mut kind = old_kind;
                        egui::ComboBox::from_id_salt("instrument_kind")
                            .width(ui.available_width())
                            .selected_text(track.instrument.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut kind, 0, "内置合成器");
                                ui.selectable_value(&mut kind, 1, "内置鼓机");
                                // 原生建模乐器已由采样音色库取代；仅当前轨道正在使用时保留，
                                // 避免旧工程无法显示其参数。
                                // Native modelled instruments are superseded by the sampled bank;
                                // keep the option only for tracks that already use one.
                                if old_kind == 2 {
                                    ui.selectable_value(&mut kind, 2, "原生乐器");
                                }
                                ui.selectable_value(&mut kind, 3, "采样乐器");
                                if old_kind == 4 {
                                    ui.selectable_value(&mut kind, 4, "VST 插件");
                                }
                            });
                        if kind != old_kind {
                            track.instrument = match kind {
                                0 => InstrumentRef::Synth(SynthSettings::default()),
                                1 => InstrumentRef::Drums(DrumSettings::default()),
                                3 => bank_instruments()
                                    .first()
                                    .map(|entry| {
                                        InstrumentRef::Sample(SampleInstrument::new(&entry.id))
                                    })
                                    .unwrap_or_else(|| {
                                        InstrumentRef::Synth(SynthSettings::default())
                                    }),
                                _ => track.instrument.clone(),
                            };
                            track_changed = true;
                        }

                        ui.add_space(4.0);
                        match &mut track.instrument {
                            InstrumentRef::Synth(settings) => {
                                if synth_settings_ui(ui, settings) {
                                    track_changed = true;
                                }
                            }
                            InstrumentRef::Drums(settings) => {
                                ui.horizontal(|ui| {
                                    ui.label("音量");
                                    if ui
                                        .add(Slider::new(&mut settings.level, 0.0..=1.0))
                                        .changed()
                                    {
                                        track_changed = true;
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label("音色");
                                    if ui.add(Slider::new(&mut settings.tone, 0.0..=1.0)).changed()
                                    {
                                        track_changed = true;
                                    }
                                });
                            }
                            InstrumentRef::Native(native) => {
                                ui.horizontal(|ui| {
                                    ui.label("音色");
                                    let mut kind = native.kind;
                                    egui::ComboBox::from_id_salt("native_instrument_kind")
                                        .width(150.0)
                                        .selected_text(kind.label())
                                        .show_ui(ui, |ui| {
                                            for candidate in NativeInstrumentKind::ALL {
                                                ui.selectable_value(
                                                    &mut kind,
                                                    candidate,
                                                    candidate.label(),
                                                );
                                            }
                                        });
                                    if kind != native.kind {
                                        *native = NativeInstrument::new(kind);
                                        track_changed = true;
                                    }
                                });
                                if native_settings_ui(ui, native) {
                                    track_changed = true;
                                }
                            }
                            InstrumentRef::Sample(sample) => {
                                ui.horizontal(|ui| {
                                    ui.label("音色");
                                    let mut id = sample.bank.clone();
                                    egui::ComboBox::from_id_salt("sample_instrument_bank")
                                        .width(150.0)
                                        .selected_text(sample.name.clone())
                                        .show_ui(ui, |ui| {
                                            for entry in bank_instruments() {
                                                ui.selectable_value(
                                                    &mut id,
                                                    entry.id.clone(),
                                                    entry.name.clone(),
                                                );
                                            }
                                        });
                                    if id != sample.bank {
                                        *sample = SampleInstrument::new(&id);
                                        track_changed = true;
                                    }
                                });
                                if sample_settings_ui(ui, &mut sample.settings) {
                                    track_changed = true;
                                }
                            }
                            InstrumentRef::Plugin(pref) => {
                                plugin_header(ui, pref, true);
                                if let Some(error) = &instrument_failure {
                                    ui.label(
                                        RichText::new(format!("加载失败：{error}"))
                                            .color(theme::RED),
                                    );
                                }
                                ui.horizontal(|ui| {
                                    if ui.button("打开编辑器").clicked() {
                                        open_editor = Some(SlotKind::Instrument);
                                    }
                                    if ui.button("刷新参数").clicked() {
                                        refresh = Some(SlotKind::Instrument);
                                    }
                                });
                                if !instrument_params.is_empty() {
                                    ui.add_space(2.0);
                                    egui::CollapsingHeader::new(format!(
                                        "参数 ({})",
                                        instrument_params.len()
                                    ))
                                    .default_open(false)
                                    .show(ui, |ui| {
                                        plugin_params_ui(
                                            ui,
                                            &mut instrument_params,
                                            SlotKind::Instrument,
                                            &mut param_changes,
                                        );
                                    });
                                }
                            }
                        }
                        if ui.button("移除乐器并改为内置合成器").clicked() {
                            remove_instrument = true;
                        }
                    });
                }

                ui.add_space(4.0);
                // Effect chain.
                egui::Frame::NONE
                    .fill(theme::PANEL_2)
                    .corner_radius(8)
                    .inner_margin(egui::Margin::same(9))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("效果器链 · {}", track.effects.len()))
                                    .color(theme::TEXT_DIM)
                                    .size(11.0),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.menu_button("添加效果…", |ui| {
                                        for kind in NativeEffectKind::ALL {
                                            if ui.button(kind.label()).clicked() {
                                                add_native = Some((track_id, kind));
                                                ui.close();
                                            }
                                        }
                                        ui.separator();
                                        if ui.button("浏览 VST 插件…").clicked() {
                                            browse = Some(PluginTarget::Effect);
                                            ui.close();
                                        }
                                    });
                                    if ui.button("效果器卷帘").clicked() {
                                        self.bottom_view = BottomView::Effects {
                                            track_id: self.project.tracks[track_index].id,
                                        };
                                        self.bottom_open = true;
                                    }
                                },
                            );
                        });
                        for index in 0..track.effects.len() {
                            let effect_ref = track.effects[index].clone();
                            let is_plugin = effect_ref.plugin().is_some();
                            let kind_label = if is_plugin {
                                effect_kinds
                                    .get(index)
                                    .and_then(|kind| *kind)
                                    .map(|kind| kind.label())
                                    .unwrap_or("未知")
                            } else {
                                "内置"
                            };
                            ui.add_space(3.0);
                            egui::CollapsingHeader::new(
                                RichText::new(format!(
                                    "{}  ·  {}  ·  {}",
                                    index + 1,
                                    effect_ref.label(),
                                    kind_label
                                ))
                                .size(11.0)
                                .color(theme::TEXT),
                            )
                            .id_salt((track_id, index, "effect"))
                            .default_open(index == 0)
                            .show(ui, |ui| {
                                if is_plugin {
                                    if let EffectRef::Plugin(pref) = &effect_ref {
                                        plugin_header(ui, pref, false);
                                    }
                                    if let Some(Some(error)) = effect_failures.get(index) {
                                        ui.label(
                                            RichText::new(format!("加载失败：{error}"))
                                                .color(theme::RED),
                                        );
                                    }
                                } else {
                                    ui.label(
                                        RichText::new("内置效果器")
                                            .size(10.0)
                                            .color(theme::TEXT_FAINT),
                                    );
                                }
                                ui.horizontal(|ui| {
                                    if is_plugin {
                                        if ui.button("GUI").clicked() {
                                            open_editor = Some(SlotKind::Effect(index));
                                        }
                                        if ui.button("刷新").clicked() {
                                            refresh = Some(SlotKind::Effect(index));
                                        }
                                    }
                                    if ui.button("↑").clicked() {
                                        effect_actions.push(EffectAction::Move(index, -1));
                                    }
                                    if ui.button("↓").clicked() {
                                        effect_actions.push(EffectAction::Move(index, 1));
                                    }
                                    if ui.button("移除").clicked() {
                                        effect_actions.push(EffectAction::Remove(index));
                                    }
                                });
                                if is_plugin {
                                    if let Some(views) = effect_params.get_mut(index) {
                                        if !views.is_empty() {
                                            egui::CollapsingHeader::new(format!(
                                                "参数 ({})",
                                                views.len()
                                            ))
                                            .default_open(false)
                                            .show(
                                                ui,
                                                |ui| {
                                                    plugin_params_ui(
                                                        ui,
                                                        views,
                                                        SlotKind::Effect(index),
                                                        &mut param_changes,
                                                    );
                                                },
                                            );
                                        }
                                    }
                                } else if let Some(EffectRef::Native(native)) =
                                    track.effects.get_mut(index)
                                {
                                    if crate::ui::effects::native_effect_parameters_ui(ui, native) {
                                        track_changed = true;
                                    }
                                }
                            });
                        }
                    });

                ui.add_space(4.0);
                // Controller sub-lanes.
                egui::Frame::NONE
                    .fill(theme::PANEL_2)
                    .corner_radius(8)
                    .inner_margin(egui::Margin::same(9))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("子轨 / 控制器")
                                .color(theme::TEXT_DIM)
                                .size(11.0),
                        );
                        ui.horizontal_wrapped(|ui| {
                            let track_color = theme::track_color(track.color);
                            for kind in ControllerKind::ALL {
                                let points = track.controllers.get(kind).len();
                                let supported = track.supports_controller(kind);
                                let mut text =
                                    RichText::new(format!("{} ({})", kind.label(), points)).color(
                                        if supported {
                                            track_color
                                        } else {
                                            theme::TEXT_FAINT
                                        },
                                    );
                                if !supported {
                                    text = text.strikethrough();
                                }
                                if ui.add(egui::Button::new(text)).clicked() {
                                    let track_id = self.project.tracks[track_index].id;
                                    self.request_roll(track_id, RollKind::Controller(kind));
                                }
                            }
                            let text = RichText::new("效果器").color(track_color);
                            if ui.add(egui::Button::new(text)).clicked() {
                                let track_id = self.project.tracks[track_index].id;
                                self.request_roll(track_id, RollKind::Effects);
                            }
                        });
                    });

                ui.add_space(4.0);
                egui::Frame::NONE
                    .fill(theme::PANEL_2)
                    .corner_radius(8)
                    .inner_margin(egui::Margin::same(9))
                    .show(ui, |ui| {
                        ui.label(RichText::new("通道").color(theme::TEXT_DIM).size(11.0));
                        ui.horizontal(|ui| {
                            ui.label("音量");
                            if ui.add(Slider::new(&mut track.volume, 0.0..=1.2)).changed() {
                                track_changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("声像");
                            if ui.add(Slider::new(&mut track.pan, -1.0..=1.0)).changed() {
                                track_changed = true;
                            }
                        });
                        if output_layout.pairs() > 1 {
                            ui.horizontal(|ui| {
                                ui.label("输出通道");
                                let mut pair = (track.output_pair as usize)
                                    .min(output_layout.pairs().saturating_sub(1));
                                egui::ComboBox::from_id_salt(("track_output_pair", track_id))
                                    .width(ui.available_width())
                                    .selected_text(output_layout.pair_label(pair))
                                    .show_ui(ui, |ui| {
                                        for index in 0..output_layout.pairs() {
                                            ui.selectable_value(
                                                &mut pair,
                                                index,
                                                output_layout.pair_label(index),
                                            );
                                        }
                                    });
                                if pair as u8 != track.output_pair {
                                    track.output_pair = pair as u8;
                                    track_changed = true;
                                }
                            });
                        }
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut track.muted, "静音").changed() {
                                track_changed = true;
                            }
                            if ui.checkbox(&mut track.solo, "独奏").changed() {
                                track_changed = true;
                            }
                        });
                        let (meter, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 6.0),
                            Sense::hover(),
                        );
                        theme::level_meter(ui.painter(), meter, level, false);
                    });

                ui.add_space(4.0);
                if let Some(clip_index) = self.selected_clip {
                    if let Some(clip) = track.clips.get_mut(clip_index) {
                        egui::Frame::NONE
                            .fill(theme::PANEL_2)
                            .corner_radius(8)
                            .inner_margin(egui::Margin::same(9))
                            .show(ui, |ui| {
                                let title = if let Some(source) = &clip.audio {
                                    format!(
                                        "音频片段 · {} · {:.1} 秒",
                                        clip.name, source.length_seconds
                                    )
                                } else {
                                    format!("MIDI 片段颜色 · {}", clip.name)
                                };
                                ui.label(RichText::new(title).color(theme::TEXT_DIM).size(11.0));
                                let mut chosen = clip.color;
                                ui.spacing_mut().item_spacing = Vec2::splat(2.0);
                                ui.horizontal_wrapped(|ui| {
                                    for index in 0..=255u8 {
                                        let selected = chosen == Some(index);
                                        let button = egui::Button::new("")
                                            .fill(theme::palette_color(index))
                                            .min_size(Vec2::splat(12.0))
                                            .stroke(egui::Stroke::new(
                                                if selected { 1.5_f32 } else { 0.0_f32 },
                                                egui::Color32::WHITE,
                                            ));
                                        if ui.add(button).clicked() {
                                            chosen = Some(index);
                                        }
                                    }
                                });
                                if ui.button("跟随轨道颜色").clicked() {
                                    chosen = None;
                                }
                                if chosen != clip.color {
                                    clip.color = chosen;
                                    track_changed = true;
                                }
                            });
                    }
                }

                ui.add_space(6.0);
                egui::CollapsingHeader::new("主输出")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("主音量");
                            let _ = ui.add(Slider::new(&mut master_volume, 0.0..=1.2));
                        });
                        ui.horizontal(|ui| {
                            ui.label("输出布局");
                            egui::ComboBox::from_id_salt("master_output_layout")
                                .width(ui.available_width())
                                .selected_text(layout_choice.label())
                                .show_ui(ui, |ui| {
                                    for layout in crate::core::model::ChannelLayout::ALL {
                                        ui.selectable_value(
                                            &mut layout_choice,
                                            layout,
                                            layout.label(),
                                        );
                                    }
                                });
                        });
                        if let Some(effect_ref) = self.project.master_effect.clone() {
                            match &effect_ref {
                                EffectRef::Plugin(pref) => {
                                    plugin_header(ui, pref, false);
                                    if let Some(error) = &master_failure {
                                        ui.label(
                                            RichText::new(format!("加载失败：{error}"))
                                                .color(theme::RED),
                                        );
                                    }
                                    ui.horizontal(|ui| {
                                        if ui.button("打开编辑器").clicked() {
                                            open_editor = Some(SlotKind::Master);
                                        }
                                        if ui.button("刷新参数").clicked() {
                                            refresh = Some(SlotKind::Master);
                                        }
                                        if ui.button("移除").clicked() {
                                            remove_master_effect = true;
                                        }
                                    });
                                    if !master_params.is_empty() {
                                        egui::CollapsingHeader::new(format!(
                                            "参数 ({})",
                                            master_params.len()
                                        ))
                                        .default_open(false)
                                        .show(ui, |ui| {
                                            plugin_params_ui(
                                                ui,
                                                &mut master_params,
                                                SlotKind::Master,
                                                &mut param_changes,
                                            );
                                        });
                                    }
                                }
                                EffectRef::Native(_) => {
                                    ui.label(
                                        RichText::new(format!("{}  ·  内置", effect_ref.label()))
                                            .size(11.0)
                                            .color(theme::TEXT),
                                    );
                                    if let Some(native) = self
                                        .project
                                        .master_effect
                                        .as_mut()
                                        .and_then(|effect| effect.native_mut())
                                    {
                                        if crate::ui::effects::native_effect_parameters_ui(
                                            ui, native,
                                        ) {
                                            master_native_changed = true;
                                        }
                                    }
                                    if ui.button("移除").clicked() {
                                        remove_master_effect = true;
                                    }
                                }
                            }
                        } else {
                            ui.menu_button("添加主效果器", |ui| {
                                for kind in NativeEffectKind::ALL {
                                    if ui.button(kind.label()).clicked() {
                                        add_native =
                                            Some((crate::core::model::MASTER_TRACK_ID, kind));
                                        ui.close();
                                    }
                                }
                                ui.separator();
                                if ui.button("浏览 VST 插件…").clicked() {
                                    browse = Some(PluginTarget::MasterEffect);
                                    ui.close();
                                }
                            });
                        }
                    });

                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "工程：{} · {} 小节 · {} 轨道",
                        self.project.name,
                        (self.project.end_beat() / self.project.beats_per_bar()).ceil() as i64,
                        self.project.tracks.len()
                    ))
                    .size(10.0)
                    .color(theme::TEXT_FAINT),
                );
            });

        // 绑定设备时自动把它加入“启用的设备”，否则该端口不会被打开，绑定将无效。
        // Binding a device also enables it; otherwise the port is never opened and the binding
        // would have no effect.
        if let Some(device) = enable_input_device {
            if !self
                .settings
                .controller_enabled_input_devices
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&device))
            {
                self.settings
                    .controller_enabled_input_devices
                    .push(device.clone());
                self.commit_settings();
                self.midi_status = self.shared.reconnect_midi_devices(
                    self.settings.controller_input_device.as_deref(),
                    &self.settings.controller_enabled_input_devices,
                );
            }
        }

        // ---- apply edits ------------------------------------------------------
        if remove_track {
            // 组合轨道会弹出“整棵删除 / 释放成员”确认，普通轨道立即删除。
            // A group opens the subtree/release confirmation; ordinary tracks are deleted at once.
            self.request_delete_track(track_index);
            return;
        }

        if remove_instrument {
            track.instrument = InstrumentRef::Synth(SynthSettings::default());
            track_changed = true;
            self.close_editor(track_id, SlotKind::Instrument);
        }

        if let Some(action) = effect_actions.first().copied() {
            match action {
                EffectAction::Remove(index) => {
                    if index < track.effects.len() {
                        track.effects.remove(index);
                        track_changed = true;
                        remove_effect = Some(index);
                    }
                }
                EffectAction::Move(index, delta) => {
                    let target = index as isize + delta;
                    if target >= 0 && (target as usize) < track.effects.len() {
                        track.effects.swap(index, target as usize);
                        track_changed = true;
                        remove_effect = Some(usize::MAX);
                    }
                }
            }
        }

        if track_changed {
            let previous = self.project.tracks.get(track_index).cloned();
            if let Some(previous) = previous {
                if previous.instrument.key() != track.instrument.key() {
                    self.close_editor(track_id, SlotKind::Instrument);
                }
                let old_effects: Vec<String> =
                    previous.effects.iter().map(EffectRef::key).collect();
                let new_effects: Vec<String> = track.effects.iter().map(EffectRef::key).collect();
                if old_effects != new_effects {
                    self.editors.retain(|((id, slot), _)| {
                        !(*id == track_id && matches!(slot, SlotKind::Effect(_)))
                    });
                }
                let audio_changed = previous.instrument != track.instrument
                    || previous.effects != track.effects
                    || previous.controllers != track.controllers;
                self.project.tracks[track_index] = track;
                if audio_changed {
                    self.bump_track_revision(track_index);
                } else {
                    self.mark_dirty();
                }
            }
        }

        if layout_choice != self.project.output_layout {
            self.project.output_layout = layout_choice;
            // 布局变化会改变主总线声道数；越界的轨道通道对由引擎夹取到前左/前右。
            // A layout change alters the master channel count; out-of-range track pairs are clamped
            // to front left/right by the engine.
            for track in &mut self.project.tracks {
                let max_pair = layout_choice.pairs().saturating_sub(1) as u8;
                if track.output_pair > max_pair {
                    track.output_pair = max_pair;
                }
            }
            self.mark_dirty();
            self.status = format!("输出布局已切换为 {}", layout_choice.label());
        }
        if (self.project.master_volume - master_volume).abs() > 1e-6 {
            self.project.master_volume = master_volume;
            self.mark_dirty();
        }
        if remove_master_effect {
            self.project.master_effect = None;
            self.close_editor(0, SlotKind::Master);
            self.mark_dirty();
        }
        if master_native_changed {
            self.invalidate_plugin_params(crate::core::model::MASTER_TRACK_ID);
            self.mark_dirty();
            self.status = "已修改主效果器参数".into();
        }
        if let Some((id, kind)) = add_native {
            self.add_native_effect(id, kind);
            return;
        }
        let _ = remove_effect;

        let mut audio_params_changed = false;
        if !param_changes.is_empty() {
            let engine = self.engine.clone();
            let mut changed_slots = Vec::new();
            if let Ok(mut engine) = engine.try_lock() {
                for (slot, index, value) in param_changes {
                    if engine
                        .set_plugin_param(track_id, slot, index, value)
                        .is_ok()
                    {
                        audio_params_changed = true;
                        changed_slots.push(slot);
                    }
                }
            };
            for slot in changed_slots {
                self.invalidate_plugin_param_slot(track_id, slot);
            }
        }
        if audio_params_changed {
            self.bump_track_revision(track_index);
        }

        if let Some(slot) = refresh {
            let engine = self.engine.clone();
            if let Ok(mut engine) = engine.try_lock() {
                engine.refresh_plugin_params(track_id, slot);
            };
            self.invalidate_plugin_param_slot(track_id, slot);
        }
        if let Some(slot) = open_editor {
            self.open_editor(track_id, slot);
        }
        if let Some(target) = browse {
            self.open_browser(target);
        }
    }
}

fn display_plugin_name(pref: &PluginRef) -> String {
    if pref.name.is_empty() {
        pref.path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "插件".into())
    } else {
        pref.name.clone()
    }
}

fn synth_settings_ui(ui: &mut egui::Ui, settings: &mut SynthSettings) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("波形");
        egui::ComboBox::from_id_salt("synth_wave")
            .width(110.0)
            .selected_text(settings.waveform.label())
            .show_ui(ui, |ui| {
                for wave in Waveform::ALL {
                    if ui
                        .selectable_value(&mut settings.waveform, wave, wave.label())
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
        ui.label("八度");
        if ui
            .add(egui::DragValue::new(&mut settings.octave).range(-2..=2))
            .changed()
        {
            changed = true;
        }
    });

    changed |= labeled_slider(ui, "起音", &mut settings.attack, 0.001..=2.0, " s", true);
    changed |= labeled_slider(ui, "衰减", &mut settings.decay, 0.005..=2.0, " s", false);
    changed |= labeled_slider(ui, "延音", &mut settings.sustain, 0.0..=1.0, "", false);
    changed |= labeled_slider(ui, "释放", &mut settings.release, 0.005..=3.0, " s", false);
    changed |= labeled_slider(
        ui,
        "滤波",
        &mut settings.cutoff,
        80.0..=18_000.0,
        " Hz",
        true,
    );
    changed |= labeled_slider(ui, "共振", &mut settings.resonance, 0.0..=0.95, "", false);
    changed |= labeled_slider(ui, "失谐", &mut settings.detune, 0.0..=1.0, "", false);
    changed |= labeled_slider(ui, "音量", &mut settings.level, 0.0..=1.0, "", false);
    changed
}

/// 所属模块：ui / 检查器
/// 功能：绘制原生乐器的公共音色参数滑块。
/// 输入：
/// - `ui`：绘制上下文。
/// - `native`：原生乐器引用。
/// 输出/副作用：修改 `native.settings` 并返回是否发生变化。
/// 复杂度：每帧 O(1)。
///
/// Module: ui / inspector
/// Purpose: Draw the shared tone sliders of a native instrument.
/// Inputs:
/// - `ui`: paint context.
/// - `native`: native instrument reference.
/// Outputs / side effects: mutates `native.settings` and returns whether anything changed.
/// Complexity: O(1) per frame.
fn native_settings_ui(ui: &mut egui::Ui, native: &mut NativeInstrument) -> bool {
    let settings = &mut native.settings;
    let mut changed = false;
    changed |= labeled_slider(ui, "音量", &mut settings.level, 0.0..=1.0, "", false);
    changed |= labeled_slider(ui, "音色", &mut settings.tone, 0.0..=1.0, "", false);
    changed |= labeled_slider(ui, "起音", &mut settings.attack, 0.001..=1.0, " s", true);
    changed |= labeled_slider(ui, "释放", &mut settings.release, 0.02..=3.0, " s", true);
    changed |= labeled_slider(ui, "延音", &mut settings.sustain, 0.0..=1.0, "", false);
    changed |= labeled_slider(ui, "驱动", &mut settings.drive, 0.0..=1.0, "", false);
    changed |= labeled_slider(ui, "颤音", &mut settings.vibrato, 0.0..=1.0, "", false);
    changed
}

/// 所属模块：ui / 检查器 / 采样乐器
/// 功能：绘制采样乐器的音量、音色、起音、释放、力度跟随与神经音色滑杆。
/// 输入：`ui`、`settings` 可调参数。
/// 输出/副作用：可能修改参数；返回是否发生变化。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: ui / inspector / sampled instrument
/// Purpose: Draw level, tone, attack, release, velocity-follow and neural-timbre sliders for a
/// sampled instrument.
/// Inputs: `ui`, `settings` adjustable parameters.
/// Outputs / side effects: may modify the settings; returns whether anything changed.
/// Complexity: O(1) time and space.
fn sample_settings_ui(ui: &mut egui::Ui, settings: &mut SampleSettings) -> bool {
    let mut changed = false;
    changed |= labeled_slider(ui, "音量", &mut settings.level, 0.0..=1.5, "", false);
    changed |= labeled_slider(ui, "音色", &mut settings.tone, 0.0..=1.0, "", false);
    changed |= labeled_slider(ui, "起音", &mut settings.attack, 0.001..=1.0, " s", true);
    changed |= labeled_slider(ui, "释放", &mut settings.release, 0.02..=4.0, " s", true);
    changed |= labeled_slider(ui, "力度", &mut settings.velocity, 0.0..=1.0, "", false);
    // 神经音色：0 = 原始采样，1 = 完全按模型修正；滑杆名与文档保持一致。
    // Neural timbre: 0 = raw sample, 1 = fully model-corrected; the label matches the docs.
    changed |= labeled_slider(ui, "神经音色", &mut settings.neural, 0.0..=1.0, "", false);
    changed
}

fn labeled_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    logarithmic: bool,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized(
            [52.0, 20.0],
            egui::Label::new(RichText::new(label).size(11.0)),
        );
        let mut slider = Slider::new(value, range).show_value(true).suffix(suffix);
        if logarithmic {
            slider = slider.logarithmic(true);
        }
        if ui.add_sized([ui.available_width(), 20.0], slider).changed() {
            changed = true;
        }
    });
    changed
}

fn plugin_header(ui: &mut egui::Ui, pref: &PluginRef, instrument: bool) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(display_plugin_name(pref))
                .strong()
                .size(12.0)
                .color(theme::TEXT),
        );
        theme::badge(
            ui,
            pref.format.label(),
            if pref.format == crate::core::model::PluginFormat::Vst3 {
                theme::ACCENT
            } else {
                theme::AMBER
            },
        );
        if instrument {
            ui.label(RichText::new("乐器").size(9.0).color(theme::TEXT_FAINT));
        }
    });
    ui.label(
        RichText::new(pref.path.to_string_lossy())
            .size(9.0)
            .color(theme::TEXT_FAINT),
    );
}

fn plugin_params_ui(
    ui: &mut egui::Ui,
    params: &mut [ParamView],
    slot: SlotKind,
    changes: &mut Vec<(SlotKind, usize, f64)>,
) {
    egui::ScrollArea::vertical()
        .max_height(230.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (index, param) in params.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&param.name).size(10.0).color(theme::TEXT_DIM));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(&param.display)
                                .size(9.0)
                                .color(theme::TEXT_FAINT),
                        );
                    });
                });
                let mut value = param.value;
                if ui
                    .add(Slider::new(&mut value, 0.0..=1.0).show_value(false))
                    .changed()
                {
                    param.value = value;
                    changes.push((slot, index, value));
                }
            }
        });
}
