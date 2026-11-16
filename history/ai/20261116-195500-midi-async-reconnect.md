# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-16 19:55:00 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 子代理 backend-architect |
| 任务来源 | 用户反馈设置界面启用 MIDI 设备时卡死 |
| 关联分支/提交 | feature/midi-storage-installer / 未知 |
| 影响模块 | core, app, docs |
| 关联记录 | `history/ai/20261116-194929-storage-layout-refactor.md`、`history/ai/20261116-200000-installer-language-auto.md` |

## 2. 任务目标

- 消除设置界面勾选/启用 MIDI 设备时的界面卡死。
- 端口枚举与 midir 连接不在 UI 线程执行，UI 只发命令、读状态与非阻塞取事件。
- 保留默认设备 + 启用设备、多窗口路由、虚拟键盘与状态文案行为。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/core/midi_input.rs` | 修改 | 新增 `MidiInputService` 与 `surfing-midi` 工作线程；命令合并、状态/默认端口发布、非阻塞事件队列；`MidiInputManager` 改为工作线程私有连接器；新增 `enumerate_ports` |
| `src/app/shared.rs` | 修改 | `SharedServices.midi` 改为直接持有服务；启动与重连不再阻塞；状态/事件读取保持原签名 |
| `src/app/settings.rs` | 修改 | MIDI 端口改为 `midi-ports` 后台枚举 + 每帧 `try_recv`；控制器页增加“刷新设备”，枚举中定时重绘 |
| `src/app/mod.rs` | 修改 | 新增 `settings_midi_ports_rx` 字段并初始化 |
| `doc/技术文档.md` | 修改 | 4.10 新增端口枚举与连接的线程模型 |
| `doc/框架工作流.md` | 修改 | 启动流程、MIDI 输入模块、SharedServices 与设置枚举函数表 |
| `doc/项目简介.md` | 修改 | 用户可见行为：后台扫描、刷新设备、启用不卡界面 |
| `doc/关键常量.md` | 修改 | 登记 `MIDI_WORKER_THREAD`、`MIDI_PORTS_THREAD`、`MIDI_PORTS_REPAINT_MS`、`MIDI_CONNECTING_STATUS` |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 设置页启用 MIDI 设备 | UI 线程同步枚举并重建连接 | 只发送命令；工作线程合并积压后重连 | 界面不再因驱动阻塞而卡死 |
| 设置页首次枚举端口 | UI 线程调用 midir | `midi-ports` 线程枚举，UI 每帧 `try_recv` | 首次进入控制器页不阻塞 |
| 应用启动 | 同步连接 MIDI | 启动工作线程后立即返回 | 驱动异常不再拖慢启动 |
| 状态与事件 | 与设备 I/O 共用一把锁 | 状态/端口短锁发布，事件通道非阻塞 | 重连期间 UI 仍可响应 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `MIDI_WORKER_THREAD` | 无 | `surfing-midi` | `src/core/midi_input.rs` | midir 连接专用线程名 |
| `MIDI_CONNECTING_STATUS` | 无 | `正在连接 MIDI 输入…` | `src/core/midi_input.rs` | 重连进行中的状态 |
| `MIDI_PORTS_THREAD` | 无 | `midi-ports` | `src/app/settings.rs` | 端口枚举线程名 |
| `MIDI_PORTS_REPAINT_MS` | 无 | `100` ms | `src/app/settings.rs` | 枚举期间重绘间隔 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 4.10 线程模型与模块表 |
| `doc/关键常量.md` | 是 | MIDI 设备与控制器一节 |
| `doc/框架工作流.md` | 是 | 启动流程、MIDI 输入与共享服务函数表 |
| `doc/项目简介.md` | 是 | 2.2 MIDI 2.0 与多设备 |
| `doc/代码规范.md` | 否 | 规范无变化 |
| `doc/日志规范.md` | 否 | 未新增日志事件 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | `cargo fmt --all -- --check` |
| `cargo check` | 通过 | `cargo check -p surfing-studio --all-targets` |
| `cargo test` | 通过 | `cargo test -p surfing-studio --lib`：178 passed，0 failed |
| 手工验证 | 未执行 | 需接真实 MIDI 设备验证勾选、刷新与热插拔 |

## 8. 风险与后续

- 风险：驱动在后台线程内仍然阻塞时，该线程会停留在连接中，但 UI 不再冻结；状态会显示“正在连接 MIDI 输入…”。
- 后续：建议在真实 MIDI 键盘与多设备场景下验证快速连续勾选、设备占用与拔插。
