//! 所属模块：tools / VST2 测试乐器
//! 文件用途：
//! - 提供一个极简 VST2 乐器，验证宿主加载、音频处理与编辑器窗口。
//! - 不作为应用的一部分；测试前运行 `cargo build -p surf-vst2-fixture`。
//!
//! Module: tools / VST2 fixture instrument
//! Purpose:
//! - Provide a tiny VST2 instrument for host loading, audio and editor-window tests.
//! - Not part of the app; run `cargo build -p surf-vst2-fixture` before the tests.

use std::sync::Arc;
use vst::api;
use vst::buffer::AudioBuffer;
use vst::editor::Editor;
use vst::event::Event;
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin, PluginParameters};
use vst::util::AtomicFloat;

/// 极简编辑器：只报告尺寸并接受父窗口，用于验证宿主的 VST2 窗口路径。
/// Minimal editor that reports a size and accepts a parent window, used to
/// exercise the host's VST2 editor path.
struct FixtureEditor {
    open: bool,
}

impl Editor for FixtureEditor {
    fn size(&self) -> (i32, i32) {
        (320, 200)
    }

    fn position(&self) -> (i32, i32) {
        (0, 0)
    }

    fn open(&mut self, _parent: *mut std::ffi::c_void) -> bool {
        self.open = true;
        true
    }

    fn close(&mut self) {
        self.open = false;
    }

    fn is_open(&mut self) -> bool {
        self.open
    }
}

struct FixtureParams {
    gain: AtomicFloat,
}

impl Default for FixtureParams {
    fn default() -> Self {
        Self {
            gain: AtomicFloat::new(0.5),
        }
    }
}

impl PluginParameters for FixtureParams {
    fn get_parameter(&self, index: i32) -> f32 {
        match index {
            0 => self.gain.get(),
            _ => 0.0,
        }
    }

    fn set_parameter(&self, index: i32, value: f32) {
        if index == 0 {
            self.gain.set(value.clamp(0.0, 1.0));
        }
    }

    fn get_parameter_name(&self, index: i32) -> String {
        if index == 0 {
            "Gain".into()
        } else {
            String::new()
        }
    }

    fn get_parameter_text(&self, index: i32) -> String {
        if index == 0 {
            format!("{:.0}%", self.gain.get() * 100.0)
        } else {
            String::new()
        }
    }

    fn get_parameter_label(&self, index: i32) -> String {
        if index == 0 {
            "%".into()
        } else {
            String::new()
        }
    }
}

struct FixtureSynth {
    params: Arc<FixtureParams>,
    sample_rate: f32,
    phase: f32,
    frequency: f32,
    active: bool,
}

impl Plugin for FixtureSynth {
    fn new(_host: HostCallback) -> Self {
        Self {
            params: Arc::new(FixtureParams::default()),
            sample_rate: 44_100.0,
            phase: 0.0,
            frequency: 440.0,
            active: false,
        }
    }

    fn get_info(&self) -> Info {
        Info {
            name: "Surf VST2 Fixture".into(),
            vendor: "SurfingStudio".into(),
            unique_id: 0x5355_5246,
            inputs: 0,
            outputs: 2,
            parameters: 1,
            category: Category::Synth,
            ..Default::default()
        }
    }

    fn init(&mut self) {}

    fn set_sample_rate(&mut self, rate: f32) {
        self.sample_rate = rate;
    }

    fn can_do(&self, can_do: CanDo) -> api::Supported {
        match can_do {
            CanDo::ReceiveMidiEvent => api::Supported::Yes,
            _ => api::Supported::Maybe,
        }
    }

    fn process_events(&mut self, events: &api::Events) {
        for event in events.events() {
            if let Event::Midi(midi) = event {
                let status = midi.data[0] & 0xF0;
                let note = midi.data[1];
                let velocity = midi.data[2];
                if status == 0x90 && velocity > 0 {
                    self.frequency = 440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0);
                    self.active = true;
                } else if status == 0x80 || (status == 0x90 && velocity == 0) {
                    self.active = false;
                }
            }
        }
    }

    fn process(&mut self, buffer: &mut AudioBuffer<f32>) {
        let mut outputs = buffer.split().1;
        let frames = outputs.get(0).len();
        let gain = self.params.gain.get() * 0.4;
        let increment = self.frequency / self.sample_rate.max(1.0);
        for frame in 0..frames {
            let sample = if self.active {
                self.phase += increment;
                if self.phase >= 1.0 {
                    self.phase -= 1.0;
                }
                (self.phase * std::f32::consts::TAU).sin() * gain
            } else {
                0.0
            };
            for channel in 0..outputs.len() {
                outputs[channel][frame] = sample;
            }
        }
    }

    fn get_parameter_object(&mut self) -> Arc<dyn PluginParameters> {
        Arc::clone(&self.params) as Arc<dyn PluginParameters>
    }

    fn get_editor(&mut self) -> Option<Box<dyn Editor>> {
        Some(Box::new(FixtureEditor { open: false }))
    }
}

vst::plugin_main!(FixtureSynth);
