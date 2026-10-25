//! VST2 后端：加载、宿主回调、MIDI 事件与扫描适配。
//! VST2 backend: loading, host callbacks, MIDI events and scan adapter.

use super::super::backend::{PluginBackend, PluginBackendInfo};
use super::super::scan::scan_legacy_vst;
use super::super::{ParamView, PluginRuntime, ScannedPlugin, Vst2Runtime};
use crate::audio::synth::{GateKind, SynthEvent};
use crate::core::model::PluginFormat;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use vst::host::{Host, PluginLoader};
use vst::plugin::Plugin as VstPluginTrait;

/// VST2 格式后端。
/// VST2 format backend.
pub struct Vst2Backend;

impl PluginBackend for Vst2Backend {
    /// 所属模块：plugin / 后端 / VST2
    /// 功能：返回 VST2 后端能力。
    /// 输入：`self`。
    ///
    /// 输出/副作用：返回能力描述；不修改状态。
    ///
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: plugin / backends / VST2
    /// Purpose: Return the VST2 backend capabilities.
    /// Inputs: `self`.
    ///
    /// Outputs / side effects: returns the descriptor; no state change.
    ///
    /// Complexity: O(1) time and space.
    fn info(&self) -> PluginBackendInfo {
        PluginBackendInfo {
            format: PluginFormat::Vst2,
            label: "VST2",
            can_scan: true,
        }
    }

    /// 所属模块：plugin / 后端 / VST2
    /// 功能：加载 VST2 插件实例。
    /// 输入：同 `load`（`uid` 与 `bpm` 不参与 VST2 加载）。
    ///
    /// 输出/副作用：返回统一插件运行时；失败时返回中文上下文错误。
    ///
    /// 复杂度：时间 O(插件初始化)，空间 O(插件状态)。
    ///
    /// Module: plugin / backends / VST2
    /// Purpose: Load a VST2 plugin instance.
    /// Inputs: same as `load` (`uid` and `bpm` are unused by VST2).
    ///
    /// Outputs / side effects: returns the unified plugin runtime; failures carry
    /// Chinese context.
    ///
    /// Complexity: O(plugin initialization) time and O(plugin state) space.
    fn load(
        &self,
        path: &Path,
        _uid: &str,
        sample_rate: f64,
        block_size: usize,
        _bpm: f64,
        _time_sig: (u8, u8),
    ) -> Result<PluginRuntime, String> {
        load(path, sample_rate, block_size)
    }

    /// 所属模块：plugin / 后端 / VST2
    /// 功能：扫描 VST2 插件。
    /// 输入：`custom_paths` 自定义扫描目录。
    ///
    /// 输出/副作用：返回扫描结果；只读文件系统。
    ///
    /// 复杂度：时间 O(目录项数 × 最大递归深度)。
    ///
    /// Module: plugin / backends / VST2
    /// Purpose: Scan VST2 plugins.
    /// Inputs: `custom_paths` — custom scan directories.
    ///
    /// Outputs / side effects: returns scan results; the filesystem is read-only.
    ///
    /// Complexity: O(directory entries × max recursion depth) time.
    fn scan(&self, custom_paths: &[PathBuf]) -> Result<Vec<ScannedPlugin>, String> {
        scan_legacy_vst(custom_paths)
    }
}

pub(super) fn load(
    path: &Path,
    sample_rate: f64,
    block_size: usize,
) -> Result<PluginRuntime, String> {
    let host = Arc::new(Mutex::new(Vst2Host::default()));
    let mut loader =
        PluginLoader::load(path, host.clone()).map_err(|e| format!("VST2 加载失败: {e}"))?;
    let mut instance = loader
        .instance()
        .map_err(|e| format!("VST2 实例化失败: {e}"))?;

    instance.init();
    instance.set_sample_rate(sample_rate as f32);
    instance.set_block_size(block_size as i64);
    instance.resume();

    let info = instance.get_info();
    let param_count = info.parameters.max(0) as usize;
    let obj = instance.get_parameter_object();
    let mut params = Vec::with_capacity(param_count);
    for i in 0..param_count {
        let value = obj.get_parameter(i as i32) as f64;
        let text = obj.get_parameter_text(i as i32);
        params.push(ParamView {
            id: i as u32,
            name: obj.get_parameter_name(i as i32),
            value,
            display: if text.is_empty() {
                format!("{:.0}%", value * 100.0)
            } else {
                text
            },
        });
    }

    let name = if info.name.is_empty() {
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "VST2 插件".into())
    } else {
        info.name.clone()
    };

    let is_instrument = matches!(
        info.category,
        vst::plugin::Category::Synth | vst::plugin::Category::Generator
    );
    let num_inputs = info.inputs.max(0) as usize;
    let num_outputs = info.outputs.max(1) as usize;

    Ok(PluginRuntime::Vst2(Vst2Runtime {
        instance,
        _host: host,
        name,
        params,
        num_inputs,
        num_outputs,
        is_instrument,
        process_inputs: Vec::new(),
        process_outputs: Vec::new(),
    }))
}

#[derive(Default)]
pub struct Vst2Host {
    // The VST2 host callback is stateless for now. The struct exists so plugins
    // that store the host pointer have a stable object to talk to.
}

impl Host for Vst2Host {
    fn automate(&self, _index: i32, _value: f32) {}
    fn get_plugin_id(&self) -> i32 {
        0
    }
}

/// Build the inline `VstEvents` structure with room for a full block of notes.
#[repr(C)]
struct Vst2EventsBuffer {
    num_events: i32,
    _reserved: isize,
    events: [*mut vst::api::Event; 64],
}

pub(crate) fn send_vst2_events(
    instance: &mut vst::host::PluginInstance,
    events: &[vst::api::MidiEvent],
) {
    if events.is_empty() {
        return;
    }
    let mut buffer = Vst2EventsBuffer {
        num_events: events.len().min(64) as i32,
        _reserved: 0,
        events: [std::ptr::null_mut(); 64],
    };
    let mut raw: Vec<vst::api::MidiEvent> = events.iter().take(64).map(clone_midi).collect();
    for (i, event) in raw.iter_mut().enumerate() {
        buffer.events[i] = event as *mut _ as *mut vst::api::Event;
    }
    let ptr = &buffer as *const Vst2EventsBuffer as *const vst::api::Events;
    // SAFETY: `Vst2EventsBuffer` has the same prefix as `vst::api::Events`
    // (`i32`, `isize`, then an inline pointer array), and the number of events
    // never exceeds the inline capacity.
    let api_events = unsafe { &*ptr };
    instance.process_events(api_events);
}

fn clone_midi(event: &vst::api::MidiEvent) -> vst::api::MidiEvent {
    vst::api::MidiEvent {
        event_type: event.event_type,
        byte_size: event.byte_size,
        delta_frames: event.delta_frames,
        flags: event.flags,
        note_length: event.note_length,
        note_offset: event.note_offset,
        midi_data: event.midi_data,
        _midi_reserved: event._midi_reserved,
        detune: event.detune,
        note_off_velocity: event.note_off_velocity,
        _reserved1: event._reserved1,
        _reserved2: event._reserved2,
    }
}

pub(crate) fn make_vst2_cc(controller: u8, value: u8) -> vst::api::MidiEvent {
    vst::api::MidiEvent {
        event_type: vst::api::EventType::Midi,
        byte_size: std::mem::size_of::<vst::api::MidiEvent>() as i32,
        delta_frames: 0,
        flags: 0,
        note_length: 0,
        note_offset: 0,
        midi_data: [0xB0, controller, value],
        _midi_reserved: 0,
        detune: 0,
        note_off_velocity: 0,
        _reserved1: 0,
        _reserved2: 0,
    }
}

pub(crate) fn send_vst2_gates(instance: &mut vst::host::PluginInstance, gates: &[SynthEvent]) {
    if gates.is_empty() {
        return;
    }
    let mut events = Vec::with_capacity(gates.len());
    for gate in gates {
        let status = if gate.kind == GateKind::On {
            0x90u8
        } else {
            0x80u8
        };
        events.push(vst::api::MidiEvent {
            event_type: vst::api::EventType::Midi,
            byte_size: std::mem::size_of::<vst::api::MidiEvent>() as i32,
            delta_frames: gate.frame as i32,
            flags: 0,
            note_length: 0,
            note_offset: 0,
            midi_data: [
                status,
                gate.pitch & 0x7f,
                if gate.kind == GateKind::On {
                    gate.velocity.max(1) & 0x7f
                } else {
                    0
                },
            ],
            _midi_reserved: 0,
            detune: 0,
            note_off_velocity: 0,
            _reserved1: 0,
            _reserved2: 0,
        });
    }
    send_vst2_events(instance, &events);
}
