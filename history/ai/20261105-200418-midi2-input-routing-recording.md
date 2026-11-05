# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-05 20:04:18 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 内置编码模型 |
| 任务来源 | 添加 MIDI 2.0 支持、控制器默认/启用设备、轨道 MIDI 设备绑定与 MIDI 录制 |
| 关联分支/提交 | feature/midi2-input-recording / 未知 |
| 影响模块 | core, app, ui, audio, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 增加 MIDI 2.0 输入支持。
- 设置 → 控制器改为“默认设备（下拉单选）+ 启用的设备（列表多选）”。
- 默认 MIDI 设备控制当前选中轨道（低优先级），其他轨道可绑定 MIDI 设备。
- 增加录音按钮，停止后自动退出录音；录制内容为新建 MIDI 片段，无输入不创建片段。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/core/midi_input.rs` | 修改 | 重写输入层：支持 MIDI 1.0 字节流、MIDI 1.0 UMP 与 MIDI 2.0 UMP 解码；事件带来源设备与协议；默认 + 启用多端口连接；新增解码与归一化单元测试 |
| `src/core/model/track.rs` | 修改 | 新增 `input_device: Option<String>`（serde 默认），用于轨道绑定 MIDI 输入端口 |
| `src/app/settings.rs` | 修改 | 新增 `controller_enabled_input_devices`；控制器页改为默认设备下拉与启用设备多选；新增 `midi_input_ports` 枚举入口；更新旧配置默认值测试 |
| `src/app/shared.rs` | 修改 | 按默认/启用列表创建与重建 MIDI 连接；暴露解析后的默认端口；事件类型改为 `MidiInputEvent` |
| `src/app/mod.rs` | 修改 | 引入 `record` 模块；试听挂起音改为按轨道分组 `held_notes`；新增录音状态字段；自动停止时防御性收尾 |
| `src/app/record.rs` | 新增 | 录音状态、按轨缓冲、录音片段生成、设备到轨道路由、听感位置读取与单元测试 |
| `src/app/input.rs` | 修改 | MIDI 事件按设备绑定路由到轨道，触发按轨试听并写入录音缓冲 |
| `src/app/transport.rs` | 修改 | 试听 API 按轨道拆分（`audition_on_for` / `audition_off_for` / `release_track_notes`）；暂停时结束录音 |
| `src/app/panels.rs` | 修改 | 工具栏走带区新增录音按钮（录音中红色选中态） |
| `src/audio/engine/mod.rs` | 修改 | 新增 `Engine::set_auto_stop(enabled, project)`：录音期间关闭自动停止，收尾时按新工程重算停止点 |
| `src/ui/inspector.rs` | 修改 | 轨道属性新增 MIDI 输入设备绑定下拉；选择端口时自动加入启用列表并重连 |
| `doc/项目简介.md` | 修改 | 补充 MIDI 录音、MIDI 2.0、多设备设置与已知限制 |
| `doc/技术文档.md` | 修改 | 更新模块表、Track 模型、自动停止、控制器设置、新增 4.10 节与已知限制、测试清单 |
| `doc/关键常量.md` | 修改 | 新增录音最短音符/片段长度与 MIDI 默认/启用设备默认值 |
| `doc/框架工作流.md` | 修改 | 更新 MIDI 输入、共享服务、设置、输入、工具栏、检查器与引擎索引，新增 `app/record.rs` 索引 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| MIDI 输入连接 | 单个输入端口，Note On/Off 直接进队列 | 默认端口 + 启用端口同时连接；事件带来源设备与协议 | 支持多设备输入与按轨道路由 |
| MIDI 事件消费 | 当前选中轨道试听 | 先匹配 `Track::input_device`，未命中时默认设备回退到未绑定的选中轨道 | 显式绑定优先，多轨可同时试听/录音 |
| 录音流程 | 无 | 工具栏录音按钮 → 关闭自动停止 → 按轨缓冲 → 停止播放/再次点击时生成新 MIDI 片段 | 无输入不建片段；录音期间不会被自动停止打断 |
| 设置控制器页 | 单个启用输入端口 | 默认设备下拉 + 启用设备多选；变更立即重建连接 | 与轨道绑定联动 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `MIN_RECORDED_NOTE_BEATS` | 无 | `1/32` beat | `src/app/record.rs` | 极短点击的最短音符长度 |
| `MIN_RECORDED_CLIP_BEATS` | 无 | `0.25` beat | `src/app/record.rs` | 录音片段最短长度 |
| `controller_enabled_input_devices` | 无 | 空列表 | `src/app/settings.rs` | 附加启用的 MIDI 输入设备 |
| `controller_input_device` | 单个启用输入端口 | 默认输入设备（`None` 自动） | `src/app/settings.rs` | 始终连接并用于选中轨道回退 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 模块表、3.2 Track、4.4 自动停止、4.9 启用设备、4.10 MIDI 2.0/路由/录音、7.10 设置、11.1 测试、12.2 已知限制 |
| `doc/关键常量.md` | 是 | 第 5 节新增录音与 MIDI 设备默认值 |
| `doc/框架工作流.md` | 是 | MIDI 输入、共享服务、设置、输入、工具栏、检查器、引擎与 `app/record.rs` 索引 |
| `doc/项目简介.md` | 是 | 2.2 MIDI 录音与 MIDI 2.0、2.6 设置与工具栏、技术栈、已知限制 |
| `doc/代码规范.md` | 否 | 规范本身无变化 |
| `doc/日志规范.md` | 否 | 未新增日志事件或修改日志格式 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | `cargo fmt --all` 无额外文件改动 |
| `cargo test` | 通过 | 124 passed，0 failed，23 ignored |
| 手工验证 | 未执行 | 需要真实 MIDI 设备与 MIDI 2.0 数据源；已通过纯函数单元测试覆盖解码与录音片段生成 |

## 8. 风险与后续

- 风险：midir 仍通过系统 MIDI 1.0 端口连接，尚未枚举 Windows MIDI Services 的 MIDI 2.0 原生端点；MIDI 2.0 UMP 需由驱动以原始 UMP 数据送达才能解码。
- 风险：工程模型只保存 7 位力度，MIDI 2.0 的 16 位力度写入片段时量化；试听与实时演奏使用量化后的力度。
- 后续：接入 Windows MIDI Services 原生 MIDI 2.0 端点；把 MIDI CC / 弯音输入映射到控制器自动化轨。
