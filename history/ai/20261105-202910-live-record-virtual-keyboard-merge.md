# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-05 20:29:10 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 代码助手模型 |
| 任务来源 | 录音边录边显示、内置虚拟 MIDI 设备（电脑键盘）、Ctrl 多选片段与同轨片段合并 |
| 关联分支/提交 | feature/live-record-virtual-keyboard-merge / 未知 |
| 影响模块 | core, app, ui |
| 关联记录 | 无 |

## 2. 任务目标

- 录音时立即在编曲区与钢琴卷帘显示正在录制的 MIDI 片段，而不是停止后才出现。
- 新增自带虚拟 MIDI 设备，把电脑键盘映射为可绑定、可录音的 MIDI 输入。
- 支持按住 Ctrl 单击多选片段；同一轨道多选时右键菜单提供「合并片段」。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/record.rs` | 修改 | `RecordState` 增加预览片段映射与撤销标记；`TrackTake::preview_clip` 与 `clip_from_notes` 生成实时预览；`record_note_on/off` 即时刷新；新增 `service_recording_preview` / `refresh_recording_preview` / `ensure_recording_preview_id`；`finish_recording` 按预览 id 确认最终片段；新增 `cancel_recording`（删除预览并恢复自动停止）；虚拟键盘作为默认输入回退；补预览片段单测 |
| `src/app/input.rs` | 修改 | 电脑键盘 Note On/Off 改为内置虚拟 MIDI 设备事件：按设备绑定路由、试听并写入录音缓冲；新增 `virtual_keyboard_note_on` / `virtual_keyboard_note_off` |
| `src/app/mod.rs` | 修改 | `keyboard_held` 记录音高与命中轨道；`ui_frame` 在 `poll_midi` 后调用 `service_recording_preview` |
| `src/app/history.rs` | 修改 | 撤销替换工程后调用 `cancel_recording`，避免录音预览状态残留 |
| `src/app/files.rs` | 修改 | 新建/模板/加载工程前放弃录音，避免预览片段写入新工程 |
| `src/core/midi_input.rs` | 修改 | 新增 `VIRTUAL_MIDI_DEVICE`；输入端口列表固定包含虚拟键盘；默认设备解析与连接允许虚拟键盘、无物理端口时不再报错 |
| `src/app/edit.rs` | 修改 | 新增 `selected_same_track_clips` / `merge_selected_clips`（范围并集、音符重定位、音频片段拒绝） |
| `src/ui/arrangement/mod.rs` | 修改 | 片段点击与拖动起点支持 Ctrl/Command 切换多选；右键菜单在同轨多选时显示「合并片段」 |
| `doc/技术文档.md` | 修改 | 4.10 设备路由与录音、7.2 编曲区交互、7.10 全局设置、模块表同步 |
| `doc/关键常量.md` | 修改 | 新增虚拟设备名与录音预览片段名；键盘试听力度说明改为虚拟设备事件 |
| `doc/框架工作流.md` | 修改 | 每帧编排、模块函数索引（midi_input / record / edit / input / ui_arrangement）与右键菜单流程同步 |
| `doc/项目简介.md` | 修改 | 录音实时显示、内置虚拟 MIDI 设备、Ctrl 多选与合并菜单 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| MIDI 录音显示 | 音符仅写入 `RecordState` 缓冲，停止时才创建片段 | 首个音符到达即插入预览片段并按帧延长，停止时按同一 id 替换为最终片段 | 编曲区与钢琴卷帘可在录制中看到内容 |
| 电脑键盘输入 | `audition_on` 直接试听当前选中轨道，不参与录音 | 以虚拟 MIDI 设备事件走 `midi_target_tracks` 路由，同时试听与录音 | 键盘可绑定轨道、参与录音 |
| 片段选择 | 普通单击重置为单选，框选可跨轨多选 | 增加 Ctrl/Command 单击切换多选，且不触发拖动 | 同轨多选可进入合并 |
| 右键菜单 | 剪切/复制/粘贴 | 同轨多选追加「合并片段」 | 片段整理更快 |
| 撤销/工程切换 | 录音状态不受影响 | 撤销或切换工程时放弃录音并清理预览 | 避免预览片段污染新工程 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `VIRTUAL_MIDI_DEVICE` | 无 | `SurfingStudio 虚拟键盘` | `src/core/midi_input.rs` | 内置虚拟设备名，固定排在输入端口列表首位 |
| `RECORDING_CLIP_NAME` | 无 | `录音片段` | `src/app/record.rs` | 录音预览与最终片段名称 |
| 键盘试听力度 | `100`（`audition_on`） | `100`（虚拟设备 Note On 力度） | `src/app/input.rs` | 数值未变，改由虚拟设备事件发送 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 模块表、4.10 MIDI 输入/录音、7.2 编曲区交互、7.12 设置页 |
| `doc/关键常量.md` | 是 | 第 5 节 MIDI 与录音常量 |
| `doc/框架工作流.md` | 是 | 3.1 midi_input、4 每帧编排、5 编辑命令与输入、5.7 右键菜单 |
| `doc/项目简介.md` | 是 | 2.1 编曲区、2.2 MIDI 编辑 |
| `doc/代码规范.md` | 否 | 规范本身无变化 |
| `doc/日志规范.md` | 否 | 未新增日志事件或格式 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | `cargo fmt --check` 无差异 |
| `cargo test` | 通过 | 125 passed，23 ignored；含新增预览片段测试 |
| 手工验证 | 未执行 | 需要图形界面与 MIDI 输入设备，未在本机手动操作 |

## 8. 风险与后续

- 风险：录音预览片段在工程中提前存在但不标记脏、不触发同步；若录制期间发生其他编辑导致引擎同步，已过去的录音音符不会被重新触发，但极端情况下同一 block 内的音符可能重复发声。
- 风险：同一轨道多选限制由 `selected_clips` 判定；跨轨多选不会显示合并选项。
- 后续：如需合并音频片段，应另行设计采样拼接与源偏移规则。
