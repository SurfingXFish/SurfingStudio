//! 所属模块：core / MIDI 输入
//! 文件用途：
//! - 连接一个 MIDI 输入端口，把 Note On / Note Off 转换为键盘事件队列。
//! - 提供输入端口列表与手动切换端口的能力（设置 → 控制器）。
//!
//! 函数清单：
//! - `MidiInputManager::new` / `set_device` / `connect_default_with`：选择并连接输入端口。
//! - `MidiInputManager::port_names` / `output_port_names`：枚举 MIDI 输入/输出端口。
//! - `MidiInputManager::drain`：取走待处理事件。
//!
//! Module: core / MIDI input
//! Purpose:
//! - Connect one MIDI input port and turn Note On/Off into a key-event queue.
//! - Expose the port list and manual port switching (settings → controller).
//!
//! Functions:
//! - `MidiInputManager::new` / `set_device` / `connect_default_with`: select and connect an input port.
//! - `MidiInputManager::port_names` / `output_port_names`: enumerate MIDI input/output ports.
//! - `MidiInputManager::drain`: take the pending events.

use midir::{MidiInput, MidiInputConnection, MidiOutput};
use std::sync::mpsc::{channel, Receiver};

pub struct MidiKeyEvent {
    pub on: bool,
    pub pitch: u8,
    pub velocity: u8,
}

pub struct MidiInputManager {
    _connection: Option<MidiInputConnection<()>>,
    receiver: Receiver<MidiKeyEvent>,
    pub status: String,
}

impl MidiInputManager {
    /// 所属模块：core / MIDI 输入
    /// 功能：创建管理器并连接首选端口（缺失时连接第一个可用端口）。
    /// 输入：`preferred` 设置中启用的端口名。
    /// 输出/副作用：打开 MIDI 输入端口；状态写入 `status`。
    /// 复杂度：时间 O(端口数)，空间 O(1)。
    ///
    /// Module: core / MIDI input
    /// Purpose: Create the manager and connect the preferred port (falling back to the first one).
    /// Inputs: `preferred` — port name enabled in the settings.
    /// Outputs / side effects: opens a MIDI input port; the result is stored in `status`.
    /// Complexity: O(ports) time and O(1) space.
    pub fn new(preferred: Option<&str>) -> Self {
        let (tx, receiver) = channel();
        let mut manager = Self {
            _connection: None,
            receiver,
            status: "未连接 MIDI 输入".into(),
        };
        if let Err(error) = manager.connect_default_with(tx, preferred) {
            manager.status = error;
        }
        manager
    }

    /// 所属模块：core / MIDI 输入
    /// 功能：枚举当前可用的 MIDI 输入端口名称。
    /// 输入：无。
    /// 输出/副作用：端口名列表；失败时返回空列表。
    /// 复杂度：时间 O(端口数)，空间 O(端口数)。
    ///
    /// Module: core / MIDI input
    /// Purpose: Enumerate the available MIDI input port names.
    /// Inputs: none.
    /// Outputs / side effects: the port names; an empty list when enumeration fails.
    /// Complexity: O(ports) time and space.
    pub fn port_names() -> Vec<String> {
        let Ok(input) = MidiInput::new("SurfingStudio MIDI 输入") else {
            return Vec::new();
        };
        input
            .ports()
            .iter()
            .filter_map(|port| input.port_name(port).ok())
            .collect()
    }

    /// 所属模块：core / MIDI 输入
    /// 功能：枚举当前可用的 MIDI 输出端口名称（设置页记录启用状态）。
    /// 输入：无。
    /// 输出/副作用：端口名列表；失败时返回空列表。
    /// 复杂度：时间 O(端口数)，空间 O(端口数)。
    ///
    /// Module: core / MIDI input
    /// Purpose: Enumerate MIDI output port names (the settings page records which are enabled).
    /// Inputs: none.
    /// Outputs / side effects: the port names; an empty list when enumeration fails.
    /// Complexity: O(ports) time and space.
    pub fn output_port_names() -> Vec<String> {
        let Ok(output) = MidiOutput::new("SurfingStudio MIDI 输出") else {
            return Vec::new();
        };
        output
            .ports()
            .iter()
            .filter_map(|port| output.port_name(port).ok())
            .collect()
    }

    /// 所属模块：core / MIDI 输入
    /// 功能：切换输入端口：先断开旧连接，再连接指定端口（`None` 连接第一个可用端口）。
    /// 输入：`device` 端口名；`None` 表示自动选择。
    /// 输出/副作用：替换内部连接并更新 `status`；返回状态文本。
    /// 复杂度：时间 O(端口数)，空间 O(1)。
    ///
    /// Module: core / MIDI input
    /// Purpose: Switch the input port: disconnect first, then connect the requested port.
    /// Inputs: `device` — port name; `None` selects the first available port.
    /// Outputs / side effects: replaces the connection, updates `status` and returns it.
    /// Complexity: O(ports) time and O(1) space.
    pub fn set_device(&mut self, device: Option<&str>) -> String {
        self._connection = None;
        let (tx, receiver) = channel();
        self.receiver = receiver;
        if let Err(error) = self.connect_default_with(tx, device) {
            self.status = error;
        }
        self.status.clone()
    }

    /// 所属模块：core / MIDI 输入
    /// 功能：连接指定端口，把 Note On / Note Off 写入事件队列。
    /// 输入：
    /// - `tx`：事件发送端。
    /// - `preferred`：优先端口名；缺失时使用第一个端口。
    /// 输出/副作用：保存连接句柄；更新 `status` 与 `connected_port`。
    /// 复杂度：时间 O(端口数)，空间 O(端口名长度)。
    ///
    /// Module: core / MIDI input
    /// Purpose: Connect the requested port and push Note On/Off into the event queue.
    /// Inputs:
    /// - `tx`: event sender.
    /// - `preferred`: preferred port name; the first port is used when missing.
    /// Outputs / side effects: stores the connection handle; updates `status` and `connected_port`.
    /// Complexity: O(ports) time and O(port name length) space.
    fn connect_default_with(
        &mut self,
        tx: std::sync::mpsc::Sender<MidiKeyEvent>,
        preferred: Option<&str>,
    ) -> Result<(), String> {
        let input = MidiInput::new("SurfingStudio MIDI 输入").map_err(|e| e.to_string())?;
        let ports = input.ports();
        if ports.is_empty() {
            return Err("未发现 MIDI 输入设备".into());
        }
        let preferred_key = preferred.unwrap_or("").trim().to_lowercase();
        let selected = ports
            .iter()
            .find(|port| {
                !preferred_key.is_empty()
                    && input
                        .port_name(port)
                        .map(|name| name.to_lowercase() == preferred_key)
                        .unwrap_or(false)
            })
            .or_else(|| {
                if preferred_key.is_empty() {
                    ports.first()
                } else {
                    None
                }
            });
        let Some(port) = selected else {
            return Err(format!(
                "未找到 MIDI 输入端口「{}」",
                preferred.unwrap_or("")
            ));
        };
        let port_name = input
            .port_name(port)
            .unwrap_or_else(|_| "MIDI 输入".to_string());
        let connection = input
            .connect(
                port,
                "surfing-studio-in",
                move |_stamp, message, _| {
                    if message.is_empty() {
                        return;
                    }
                    let status = message[0] & 0xF0;
                    match status {
                        0x90 => {
                            let pitch = message[1] & 0x7F;
                            let velocity = message[2] & 0x7F;
                            if velocity == 0 {
                                let _ = tx.send(MidiKeyEvent {
                                    on: false,
                                    pitch,
                                    velocity: 0,
                                });
                            } else {
                                let _ = tx.send(MidiKeyEvent {
                                    on: true,
                                    pitch,
                                    velocity,
                                });
                            }
                        }
                        0x80 => {
                            let pitch = message[1] & 0x7F;
                            let _ = tx.send(MidiKeyEvent {
                                on: false,
                                pitch,
                                velocity: 0,
                            });
                        }
                        _ => {}
                    }
                },
                (),
            )
            .map_err(|e| format!("连接 MIDI 设备失败: {e}"))?;
        self._connection = Some(connection);
        self.status = format!("MIDI: {port_name}");
        Ok(())
    }

    pub fn drain(&self) -> Vec<MidiKeyEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.receiver.try_recv() {
            events.push(event);
        }
        events
    }
}
