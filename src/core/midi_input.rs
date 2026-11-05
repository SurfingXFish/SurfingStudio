//! 所属模块：core / MIDI 输入
//! 文件用途：
//! - 连接默认 MIDI 输入端口与设置中启用的附加端口，把 MIDI 1.0 字节流与 MIDI 2.0 UMP
//!   （Universal MIDI Packet）统一转换为事件队列。
//! - 提供输入端口列表、默认/启用设备选择与事件队列读取。
//!
//! 函数清单：
//! - `MidiInputManager::new` / `set_devices` / `connect_with`：选择并连接默认与启用端口。
//! - `MidiInputManager::port_names` / `output_port_names`：枚举 MIDI 输入/输出端口。
//! - `MidiInputManager::drain` / `default_port`：读取事件与当前默认端口。
//! - `decode_packet` / `decode_ump` / `decode_midi1`：解码 MIDI 1.0 与 MIDI 2.0 数据包。
//! - `scale7_velocity` / `scale7_u32` / `scale14_u32`：把低分辨率数值映射到统一位宽。
//!
//! Module: core / MIDI input
//! Purpose:
//! - Connect the default MIDI input port plus additional enabled ports, and normalise MIDI 1.0
//!   byte streams and MIDI 2.0 UMP (Universal MIDI Packet) into one event queue.
//! - Expose the port list, default/enabled device selection and queue draining.
//!
//! Functions:
//! - `MidiInputManager::new` / `set_devices` / `connect_with`: select and connect ports.
//! - `MidiInputManager::port_names` / `output_port_names`: enumerate MIDI input/output ports.
//! - `MidiInputManager::drain` / `default_port`: read events and the resolved default port.
//! - `decode_packet` / `decode_ump` / `decode_midi1`: decode MIDI 1.0 and MIDI 2.0 packets.
//! - `scale7_velocity` / `scale7_u32` / `scale14_u32`: widen low-resolution values.

use midir::{MidiInput, MidiInputConnection, MidiOutput};
use std::sync::mpsc::{Receiver, channel};

/// 一条输入事件使用的 MIDI 协议。
/// MIDI protocol carried by one input event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidiProtocol {
    /// MIDI 1.0 字节流或 MIDI 1.0 UMP（消息类型 0x2）。
    /// MIDI 1.0 byte stream or a MIDI 1.0 UMP packet (message type 0x2).
    Midi1,
    /// MIDI 2.0 UMP 通道语音消息（消息类型 0x4），力度为 16 位。
    /// MIDI 2.0 UMP channel-voice message (message type 0x4) with 16-bit velocity.
    Midi2,
}

/// 统一后的 MIDI 输入消息；数值按协议位宽归一化。
/// Normalised MIDI input message; values use their protocol-native width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidiMessage {
    NoteOn {
        channel: u8,
        pitch: u8,
        /// MIDI 1.0 为 `7 位值 << 8`，MIDI 2.0 为原始 16 位力度。
        /// MIDI 1.0 stores `7-bit value << 8`; MIDI 2.0 keeps the original 16-bit velocity.
        velocity: u16,
    },
    NoteOff {
        channel: u8,
        pitch: u8,
        velocity: u16,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        /// 统一为 32 位：MIDI 1.0 的 7 位值按比例放大，MIDI 2.0 保持原始 32 位。
        /// Normalised to 32 bits: 7-bit MIDI 1.0 values are scaled; MIDI 2.0 keeps 32 bits.
        value: u32,
    },
    PitchBend {
        channel: u8,
        /// 统一为 32 位：MIDI 1.0 的 14 位值左移 18 位，MIDI 2.0 保持原始 32 位。
        /// Normalised to 32 bits: 14-bit MIDI 1.0 values shift by 18; MIDI 2.0 keeps 32 bits.
        value: u32,
    },
}

impl MidiMessage {
    /// 所属模块：core / MIDI 输入 / 消息
    /// 功能：把消息转换为音符形式；Note On 力度 0 按 Note Off 处理。
    /// 输入：`self`。
    /// 输出/副作用：`Some((是否按下, 音高, 16 位力度))`；非音符消息为 `None`。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / MIDI input / message
    /// Purpose: Convert the message into a note form; Note On with velocity 0 acts as Note Off.
    /// Inputs: `self`.
    /// Outputs / side effects: `Some((pressed, pitch, 16-bit velocity))`; `None` for non-notes.
    /// Complexity: O(1) time and space.
    pub fn note(&self) -> Option<(bool, u8, u16)> {
        match *self {
            MidiMessage::NoteOn {
                pitch, velocity, ..
            } if velocity > 0 => Some((true, pitch, velocity)),
            MidiMessage::NoteOn { pitch, .. } | MidiMessage::NoteOff { pitch, .. } => {
                Some((false, pitch, 0))
            }
            _ => None,
        }
    }

    /// 所属模块：core / MIDI 输入 / 消息
    /// 功能：把 16 位力度换算为工程使用的 7 位力度。
    /// 输入：`velocity`：16 位力度。
    /// 输出/副作用：`1 ~ 127` 的力度；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: core / MIDI input / message
    /// Purpose: Convert a 16-bit velocity to the 7-bit velocity stored by the project.
    /// Inputs: `velocity`: 16-bit velocity.
    /// Outputs / side effects: a velocity in `1 ..= 127`.
    /// Complexity: O(1) time and space.
    pub fn velocity7(velocity: u16) -> u8 {
        // MIDI 1.0 在解码时已左移 8 位，因此取高字节即可还原原始力度。
        // MIDI 1.0 is shifted left by 8 while decoding, so the high byte restores the value.
        ((velocity >> 8) as u8).clamp(1, 127)
    }
}

/// 带来源设备的一条 MIDI 输入事件。
/// One MIDI input event tagged with its source device.
#[derive(Clone, Debug)]
pub struct MidiInputEvent {
    pub device: String,
    pub protocol: MidiProtocol,
    pub message: MidiMessage,
}

pub struct MidiInputManager {
    _connections: Vec<MidiInputConnection<()>>,
    receiver: Receiver<MidiInputEvent>,
    pub status: String,
    /// 当前用于“选中轨道”回退控制的默认端口（未配置时自动取第一个可用端口）。
    /// Default port used as the selected-track fallback (the first available port when unset).
    default_port: Option<String>,
}

impl MidiInputManager {
    /// 所属模块：core / MIDI 输入
    /// 功能：创建管理器并连接默认端口与设置中启用的附加端口。
    /// 输入：
    /// - `default`：默认端口名；`None` 自动选择第一个可用端口。
    /// - `enabled`：额外启用的端口名列表。
    /// 输出/副作用：打开 MIDI 输入端口；状态写入 `status`。
    /// 复杂度：时间 O(端口数)，空间 O(端口名长度)。
    ///
    /// Module: core / MIDI input
    /// Purpose: Create the manager and connect the default port plus the enabled extra ports.
    /// Inputs:
    /// - `default`: default port name; `None` selects the first available port.
    /// - `enabled`: additional enabled port names.
    /// Outputs / side effects: opens MIDI input ports; the result is stored in `status`.
    /// Complexity: O(ports) time and O(port name length) space.
    pub fn new(default: Option<&str>, enabled: &[String]) -> Self {
        let (tx, receiver) = channel();
        let mut manager = Self {
            _connections: Vec::new(),
            receiver,
            status: "未连接 MIDI 输入".into(),
            default_port: None,
        };
        manager.default_port = match manager.connect_with(tx, default, enabled) {
            Ok(resolved) => resolved,
            Err(error) => {
                manager.status = error;
                None
            }
        };
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
    /// Purpose: Enumerate the available MIDI output port names (the settings page records them).
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
    /// 功能：按新的默认设备与启用列表重建全部输入连接。
    /// 输入：
    /// - `default`：默认端口名；`None` 自动选择。
    /// - `enabled`：额外启用的端口名列表。
    /// 输出/副作用：替换内部连接并更新 `status`；返回状态文本。
    /// 复杂度：时间 O(端口数)，空间 O(端口数)。
    ///
    /// Module: core / MIDI input
    /// Purpose: Rebuild every input connection from the new default and enabled selection.
    /// Inputs:
    /// - `default`: default port name; `None` selects automatically.
    /// - `enabled`: additional enabled port names.
    /// Outputs / side effects: replaces the connections, updates `status` and returns it.
    /// Complexity: O(ports) time and space.
    pub fn set_devices(&mut self, default: Option<&str>, enabled: &[String]) -> String {
        self._connections.clear();
        self.default_port = None;
        let (tx, receiver) = channel();
        self.receiver = receiver;
        match self.connect_with(tx, default, enabled) {
            Ok(resolved) => self.default_port = resolved,
            Err(error) => self.status = error,
        }
        self.status.clone()
    }

    /// 所属模块：core / MIDI 输入
    /// 功能：返回当前默认端口名；没有可用端口时为 `None`。
    /// 输入：`self`。
    /// 输出/副作用：端口名；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(端口名长度)。
    ///
    /// Module: core / MIDI input
    /// Purpose: Return the resolved default port name; `None` when no port is available.
    /// Inputs: `self`.
    /// Outputs / side effects: the port name; no state change.
    /// Complexity: O(1) time and O(port name length) space.
    pub fn default_port(&self) -> Option<String> {
        self.default_port.clone()
    }

    /// 所属模块：core / MIDI 输入
    /// 功能：连接默认端口与启用端口，把解析后的事件写入队列。
    /// 输入：
    /// - `tx`：事件发送端。
    /// - `default`：默认端口名；缺失时使用第一个端口。
    /// - `enabled`：额外启用的端口名。
    /// 输出/副作用：保存连接句柄；更新 `status`；返回解析出的默认端口名。
    /// 复杂度：时间 O(端口数 × 启用数)，空间 O(端口数)。
    ///
    /// Module: core / MIDI input
    /// Purpose: Connect the default and enabled ports and push decoded events into the queue.
    /// Inputs:
    /// - `tx`: event sender.
    /// - `default`: default port name; the first port is used when missing.
    /// - `enabled`: additional enabled port names.
    /// Outputs / side effects: stores connection handles, updates `status` and returns the resolved
    /// default port.
    /// Complexity: O(ports × enabled) time and O(ports) space.
    fn connect_with(
        &mut self,
        tx: std::sync::mpsc::Sender<MidiInputEvent>,
        default: Option<&str>,
        enabled: &[String],
    ) -> Result<Option<String>, String> {
        let input = MidiInput::new("SurfingStudio MIDI 输入").map_err(|e| e.to_string())?;
        let ports = input.ports();
        if ports.is_empty() {
            return Err("未发现 MIDI 输入设备".into());
        }
        let names: Vec<String> = ports
            .iter()
            .filter_map(|port| input.port_name(port).ok())
            .collect();
        let default_key = default.unwrap_or("").trim().to_lowercase();
        let mut targets: Vec<String> = Vec::new();
        let mut resolved_default: Option<String> = None;

        // 默认端口优先按名称匹配；未配置或找不到时退回第一个可用端口。
        // Match the default by name first; fall back to the first port when unset or missing.
        if !default_key.is_empty() {
            if let Some(name) = names.iter().find(|name| name.to_lowercase() == default_key) {
                resolved_default = Some(name.clone());
                targets.push(name.clone());
            }
        }
        if resolved_default.is_none() {
            if let Some(name) = names.first() {
                resolved_default = Some(name.clone());
                targets.push(name.clone());
            }
        }

        for name in enabled {
            let key = name.trim().to_lowercase();
            if key.is_empty() || targets.iter().any(|target| target.to_lowercase() == key) {
                continue;
            }
            if let Some(found) = names
                .iter()
                .find(|candidate| candidate.to_lowercase() == key)
            {
                targets.push(found.clone());
            }
        }

        let mut connections = Vec::new();
        let mut connected_names = Vec::new();
        let mut failures = Vec::new();
        for name in targets {
            // midir 的 `connect` 会消耗 `MidiInput`，因此每个端口单独创建一次实例。
            // midir's `connect` consumes `MidiInput`, so each port gets its own instance.
            let input = MidiInput::new("SurfingStudio MIDI 输入").map_err(|e| e.to_string())?;
            let port = input.ports().into_iter().find(|port| {
                input
                    .port_name(port)
                    .map(|candidate| candidate.eq_ignore_ascii_case(&name))
                    .unwrap_or(false)
            });
            let Some(port) = port else {
                continue;
            };
            let device = name.clone();
            let tx = tx.clone();
            let connection = input.connect(
                &port,
                "surfing-studio-in",
                move |_stamp, message, _| {
                    let Some((protocol, message)) = decode_packet(message) else {
                        return;
                    };
                    let _ = tx.send(MidiInputEvent {
                        device: device.clone(),
                        protocol,
                        message,
                    });
                },
                (),
            );
            match connection {
                Ok(connection) => {
                    connections.push(connection);
                    connected_names.push(name);
                }
                // 单个端口被占用时继续连接其余设备，避免一个设备失败导致全部不可用。
                // Keep connecting the other devices when one port is busy, so one failure does
                // not take the whole input selection down.
                Err(error) => failures.push(format!("连接 MIDI 设备「{name}」失败: {error}")),
            }
        }
        if connections.is_empty() {
            return Err(if failures.is_empty() {
                "未找到可用的 MIDI 输入端口".into()
            } else {
                failures.join("；")
            });
        }
        self._connections = connections;
        self.status = format!("MIDI: {}", connected_names.join(" · "));
        if !failures.is_empty() {
            self.status = format!("{}；{}", self.status, failures.join("；"));
        }
        if default.is_some_and(|requested| {
            !requested.trim().is_empty()
                && !connected_names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(requested.trim()))
        }) {
            self.status = format!(
                "未找到默认 MIDI「{}」；{}",
                default.unwrap_or(""),
                self.status
            );
        }
        Ok(resolved_default)
    }

    /// 所属模块：core / MIDI 输入
    /// 功能：取走当前队列中的全部事件。
    /// 输入：`self`。
    /// 输出/副作用：返回事件列表并清空队列。
    /// 复杂度：时间 O(事件数)，空间 O(事件数)。
    ///
    /// Module: core / MIDI input
    /// Purpose: Take every queued event.
    /// Inputs: `self`.
    /// Outputs / side effects: returns the events and clears the queue.
    /// Complexity: O(events) time and space.
    pub fn drain(&self) -> Vec<MidiInputEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.receiver.try_recv() {
            events.push(event);
        }
        events
    }
}

/// 所属模块：core / MIDI 输入 / 解码
/// 功能：识别一个回调数据包是 MIDI 1.0 字节流还是 UMP，并解码为统一消息。
/// 输入：`message`：midir 回调收到的原始字节。
/// 输出/副作用：`(协议, 消息)`；无法识别时为 `None`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / MIDI input / decoding
/// Purpose: Detect whether a callback packet is a MIDI 1.0 byte stream or UMP and decode it.
/// Inputs: `message`: raw bytes from the midir callback.
/// Outputs / side effects: `(protocol, message)`; `None` when unsupported.
/// Complexity: O(1) time and space.
fn decode_packet(message: &[u8]) -> Option<(MidiProtocol, MidiMessage)> {
    if message.is_empty() {
        return None;
    }
    // UMP 首字节的高 4 位是消息类型，取值 0x0~0x7 时不再是 MIDI 1.0 状态字节；
    // UMP 包长总是 4 字节的整数倍。
    // The high nibble of a UMP word is the message type (0x0~0x7), which is never a MIDI 1.0
    // status byte, and UMP packets are always a multiple of four bytes.
    if message.len() % 4 == 0 && message[0] < 0x80 {
        return decode_ump(message);
    }
    decode_midi1(message).map(|message| (MidiProtocol::Midi1, message))
}

/// 所属模块：core / MIDI 输入 / 解码
/// 功能：解码 MIDI 1.0 UMP（类型 0x2）与 MIDI 2.0 通道语音 UMP（类型 0x4）。
/// 输入：`message`：UMP 字节。
/// 输出/副作用：`(协议, 消息)`；不支持的消息类型或长度不足时为 `None`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / MIDI input / decoding
/// Purpose: Decode MIDI 1.0 UMP (type 0x2) and MIDI 2.0 channel-voice UMP (type 0x4).
/// Inputs: `message`: UMP bytes.
/// Outputs / side effects: `(protocol, message)`; `None` for unsupported types or short packets.
/// Complexity: O(1) time and space.
fn decode_ump(message: &[u8]) -> Option<(MidiProtocol, MidiMessage)> {
    let message_type = message[0] >> 4;
    if message_type == 0x2 && message.len() >= 4 {
        let status = message[1] & 0xF0;
        let channel = message[1] & 0x0F;
        let data1 = message[2];
        let data2 = message[3];
        let decoded = match status {
            0x90 if data2 > 0 => MidiMessage::NoteOn {
                channel,
                pitch: data1,
                velocity: scale7_velocity(data2),
            },
            0x90 | 0x80 => MidiMessage::NoteOff {
                channel,
                pitch: data1,
                velocity: scale7_velocity(data2),
            },
            0xB0 => MidiMessage::ControlChange {
                channel,
                controller: data1,
                value: scale7_u32(data2),
            },
            0xE0 => MidiMessage::PitchBend {
                channel,
                value: scale14_u32((u16::from(data2) << 7) | u16::from(data1)),
            },
            _ => return None,
        };
        return Some((MidiProtocol::Midi1, decoded));
    }
    if message_type == 0x4 && message.len() >= 8 {
        let status = message[1] & 0xF0;
        let channel = message[1] & 0x0F;
        let pitch = message[2];
        // MIDI 2.0 的力度 / 控制器值 / 弯音值都是大端 16 或 32 位。
        // MIDI 2.0 velocities, controller values and pitch-bend values are big-endian.
        let velocity = u16::from_be_bytes([message[4], message[5]]);
        let value = u32::from_be_bytes([message[4], message[5], message[6], message[7]]);
        let decoded = match status {
            0x90 if velocity > 0 => MidiMessage::NoteOn {
                channel,
                pitch,
                velocity,
            },
            0x90 | 0x80 => MidiMessage::NoteOff {
                channel,
                pitch,
                velocity,
            },
            0xB0 => MidiMessage::ControlChange {
                channel,
                controller: pitch,
                value,
            },
            0xE0 => MidiMessage::PitchBend { channel, value },
            _ => return None,
        };
        return Some((MidiProtocol::Midi2, decoded));
    }
    None
}

/// 所属模块：core / MIDI 输入 / 解码
/// 功能：解码 MIDI 1.0 字节流的 Note On / Note Off / CC / 弯音。
/// 输入：`message`：MIDI 1.0 消息字节。
/// 输出/副作用：统一消息；长度不足或不支持的状态返回 `None`。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / MIDI input / decoding
/// Purpose: Decode Note On/Off, CC and pitch bend from a MIDI 1.0 byte stream.
/// Inputs: `message`: MIDI 1.0 message bytes.
/// Outputs / side effects: the normalised message; `None` for short or unsupported messages.
/// Complexity: O(1) time and space.
fn decode_midi1(message: &[u8]) -> Option<MidiMessage> {
    let status = message[0] & 0xF0;
    let channel = message[0] & 0x0F;
    match status {
        0x90 if message.len() >= 3 => {
            let pitch = message[1] & 0x7F;
            let velocity = message[2] & 0x7F;
            if velocity == 0 {
                Some(MidiMessage::NoteOff {
                    channel,
                    pitch,
                    velocity: 0,
                })
            } else {
                Some(MidiMessage::NoteOn {
                    channel,
                    pitch,
                    velocity: scale7_velocity(velocity),
                })
            }
        }
        0x80 if message.len() >= 3 => Some(MidiMessage::NoteOff {
            channel,
            pitch: message[1] & 0x7F,
            velocity: scale7_velocity(message[2] & 0x7F),
        }),
        0xB0 if message.len() >= 3 => Some(MidiMessage::ControlChange {
            channel,
            controller: message[1] & 0x7F,
            value: scale7_u32(message[2] & 0x7F),
        }),
        0xE0 if message.len() >= 3 => Some(MidiMessage::PitchBend {
            channel,
            value: scale14_u32((u16::from(message[2] & 0x7F) << 7) | u16::from(message[1] & 0x7F)),
        }),
        _ => None,
    }
}

/// 所属模块：core / MIDI 输入 / 数值归一化
/// 功能：把 7 位力度左移到 16 位区间，便于与 MIDI 2.0 力度统一处理。
/// 输入：`value`：0~127。
/// 输出/副作用：`value << 8`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / MIDI input / scaling
/// Purpose: Shift a 7-bit velocity into the 16-bit range to match MIDI 2.0.
/// Inputs: `value`: 0~127.
/// Outputs / side effects: `value << 8`.
/// Complexity: O(1) time and space.
fn scale7_velocity(value: u8) -> u16 {
    u16::from(value) << 8
}

/// 所属模块：core / MIDI 输入 / 数值归一化
/// 功能：把 7 位控制器值按比例放大到 32 位。
/// 输入：`value`：0~127。
/// 输出/副作用：0~`u32::MAX`；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / MIDI input / scaling
/// Purpose: Scale a 7-bit controller value up to 32 bits.
/// Inputs: `value`: 0~127.
/// Outputs / side effects: 0~`u32::MAX`.
/// Complexity: O(1) time and space.
fn scale7_u32(value: u8) -> u32 {
    (u64::from(value) * u64::from(u32::MAX) / 127) as u32
}

/// 所属模块：core / MIDI 输入 / 数值归一化
/// 功能：把 14 位弯音值左移 18 位，得到 32 位表示。
/// 输入：`value`：0~16383。
/// 输出/副作用：0~`u32::MAX`（中心 8192 → 0x8000_0000）；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: core / MIDI input / scaling
/// Purpose: Shift a 14-bit pitch-bend value by 18 bits to get its 32-bit form.
/// Inputs: `value`: 0~16383.
/// Outputs / side effects: 0~`u32::MAX` (centre 8192 → 0x8000_0000).
/// Complexity: O(1) time and space.
fn scale14_u32(value: u16) -> u32 {
    u32::from(value) << 18
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：MIDI 1.0 字节流的 Note On / 零力度 Note Off 与 16 位力度换算。
    /// Covers: MIDI 1.0 Note On, zero-velocity Note Off and 16-bit velocity scaling.
    #[test]
    fn midi1_byte_stream_decodes_notes() {
        let (protocol, message) = decode_packet(&[0x90, 60, 100]).expect("note on");
        assert_eq!(protocol, MidiProtocol::Midi1);
        assert_eq!(MidiMessage::velocity7(100 << 8), 100);
        assert_eq!(
            message,
            MidiMessage::NoteOn {
                channel: 0,
                pitch: 60,
                velocity: 100 << 8
            }
        );

        let (_, release) = decode_packet(&[0x90, 60, 0]).expect("zero velocity note off");
        assert_eq!(
            release,
            MidiMessage::NoteOff {
                channel: 0,
                pitch: 60,
                velocity: 0
            }
        );
    }

    /// 覆盖：MIDI 1.0 UMP（类型 0x2）与裸 MIDI 1.0 字节流产生相同音符。
    /// Covers: MIDI 1.0 UMP (type 0x2) and a raw MIDI 1.0 stream produce the same note.
    #[test]
    fn midi1_ump_matches_the_byte_stream() {
        let (protocol, message) = decode_packet(&[0x20, 0x90, 64, 127]).expect("ump note on");
        assert_eq!(protocol, MidiProtocol::Midi1);
        assert_eq!(
            message,
            MidiMessage::NoteOn {
                channel: 0,
                pitch: 64,
                velocity: 127 << 8
            }
        );
    }

    /// 覆盖：MIDI 2.0 UMP 音符保留 16 位力度，并能换算为 7 位工程力度。
    /// Covers: MIDI 2.0 UMP notes keep 16-bit velocity and convert to the 7-bit project value.
    #[test]
    fn midi2_ump_keeps_sixteen_bit_velocity() {
        let (protocol, message) =
            decode_packet(&[0x40, 0x91, 72, 0x00, 0x80, 0x00, 0x00, 0x00]).expect("ump note on");
        assert_eq!(protocol, MidiProtocol::Midi2);
        assert_eq!(
            message,
            MidiMessage::NoteOn {
                channel: 1,
                pitch: 72,
                velocity: 0x8000
            }
        );
        assert_eq!(MidiMessage::velocity7(0x8000), 127);
    }

    /// 覆盖：MIDI 2.0 控制器与弯音按 32 位读取。
    /// Covers: MIDI 2.0 controller and pitch-bend values are read as 32-bit.
    #[test]
    fn midi2_ump_decodes_controllers_and_bend() {
        let (_, cc) =
            decode_packet(&[0x40, 0xB2, 74, 0x00, 0x12, 0x34, 0x56, 0x78]).expect("ump cc");
        assert_eq!(
            cc,
            MidiMessage::ControlChange {
                channel: 2,
                controller: 74,
                value: 0x1234_5678
            }
        );

        let (_, bend) =
            decode_packet(&[0x40, 0xE0, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00]).expect("ump bend");
        assert_eq!(
            bend,
            MidiMessage::PitchBend {
                channel: 0,
                value: 0x8000_0000
            }
        );
    }

    /// 覆盖：7 位与 14 位数值的归一化边界。
    /// Covers: normalisation boundaries for 7-bit and 14-bit values.
    #[test]
    fn low_resolution_values_scale_to_the_wide_range() {
        assert_eq!(scale7_u32(0), 0);
        assert_eq!(scale7_u32(127), u32::MAX);
        assert_eq!(scale14_u32(0), 0);
        assert_eq!(scale14_u32(8192), 0x8000_0000);
        assert_eq!(scale14_u32(16383), 0xFFFC_0000);
    }
}
