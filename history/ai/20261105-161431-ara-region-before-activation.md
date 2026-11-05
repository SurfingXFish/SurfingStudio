# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-05 16:14:31 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | “现在是点击 Vocaloid 内音符有声音，但轨道中播放时完全没声音，Vocaloid 进度条也没跟着走”，附日志 `surfing-20261105-160004-p3156.sflog` |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | plugin, audio, docs |
| 关联记录 | `history/ai/20261102-233638-ara-effect-thread-consistency.md`；后续修正见 `history/ai/20261105-162207-ara-bridge-sync-only.md` |

## 2. 任务目标

- 让 VOCALOID Bridge 在走带播放时真正渲染声音，并让编辑器播放头跟随宿主。
- 现象证据：日志中 `洛天依_V5` 轨道电平全程 0.0000，其他轨道正常；`ara.session_created` / `ara.bound roles=7` 成功，说明绑定本身没问题。

## 3. 根因

ARA 规范要求 `addPlaybackRegion` 只在插件不处于渲染状态（VST3 未激活）时调用。此前的顺序是：

1. `component_ready_hook` 里建立 ARA 文档（未激活，正确）；
2. 加载器立即 `start_processing()` 激活插件；
3. 之后 `sync_ara_timeline` 才通过 `set_timeline` 创建播放区域并 `addPlaybackRegion`。

第 3 步发生在激活之后，播放渲染器不接受此时加入的区域，于是编辑器预览（编辑器渲染器）正常、走带播放没有可渲染区域，编辑器也拿不到正确的时间线映射，表现为“试听有声、播放无声、进度条不跟随”。

另外，`ara_span_seconds` 在轨道没有片段时只给 0..4 拍，而 VOCALOID 轨道的人声数据在编辑器里、轨道上可能没有片段，区域过短也会让播放很快越过区域末尾而静音。

## 4. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/ara/mod.rs` | 修改 | 新增 `DEFAULT_REGION_SECONDS`、线程局部 `INITIAL_TIMELINE` 与 `set_initial_timeline_hint`；`AraSession::create` 在绑定完成后、插件激活前立即建立播放区域并登记到播放/编辑器渲染器；建立成功时缓存 `timeline`；`set_timeline` 改为以 `playback_region` 是否为空决定重建还是更新；新增 `ara.playback_region` 诊断（起止、时长、两个渲染器是否可用）与 `ara.timeline_create_failed` |
| `src/audio/engine/mod.rs` | 修改 | `ara_span_seconds` 增加 `project_end_beat` 参数，空轨道区域覆盖整首工程（至少 4 拍）；`sync_ara_timeline` 增加同参数并把 `set_ara_timeline` 失败记录为 `engine.ara/ara.timeline_failed`（前 5 次）；`ensure_instrument` / `ensure_effects` 传入 `project.end_beat()`；`ensure_effects` 在加载插件前写入 `set_initial_timeline_hint`，加载后清除 |
| `src/plugin/runtime.rs` | 修改 | `process_audio` 失败不再静默：前 5 次记录 `plugin.runtime/plugin.process_failed`（插件名 + 错误），便于区分“区域没建好”和“插件拒绝处理” |
| `doc/技术文档.md` | 修改 | 6.7 增加“播放区域必须在激活前建立”“空轨道区域覆盖整首工程”“时间线同步诊断” |
| `doc/框架工作流.md` | 修改 | 3.2 `ensure_effects`、ARA 函数表同步更新 |
| `doc/关键常量.md` | 修改 | 新增 `DEFAULT_REGION_SECONDS = 60` |

## 5. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| ARA 播放区域创建时机 | 插件激活后（`set_timeline` 首次调用） | 绑定完成、插件激活前（`AraSession::create` 内），范围由引擎提示提供 | 播放渲染器能接受区域，走带播放才有声音 |
| 空轨道 ARA 区域范围 | 0..4 拍 | 0..`project.end_beat()`（至少 4 拍） | 没有片段的 VOCALOID 轨道整首歌都保持在区域内 |
| 时间线同步错误 | `let _ =` 静默丢弃 | 前 5 次写 `engine.ara/ara.timeline_failed` | 线程不符/插件拒绝可定位 |
| VST3 处理失败 | 只输出静音 | 前 5 次写 `plugin.runtime/plugin.process_failed` | 可区分 ARA 区域问题与插件处理失败 |

## 6. 关键常量变化

| 常量 | 变化 | 位置 | 说明 |
| --- | --- | --- | --- |
| `DEFAULT_REGION_SECONDS` | 新增 `60.0` | `src/plugin/ara/mod.rs` | 没有引擎提示（如界面线程应用插件）时，激活前先建一个可更新的区域 |

## 7. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.7 |
| `doc/框架工作流.md` | 是 | 3.2 与 `src/plugin/ara/` 函数表 |
| `doc/关键常量.md` | 是 | ARA 常量段 |
| `doc/项目简介.md` | 否 | 无用户可见功能变化 |
| `doc/代码规范.md` | 否 | 无变化 |

## 8. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test --lib` | 通过 | 116 passed / 23 ignored |
| 日志分析 | 已执行 | 日志显示 Bridge 绑定成功、轨道电平 0；确认播放区域创建晚于激活 |
| 实机验证 | 待执行 | 需要重新构建后播放工程，检查 `ara.playback_region` 的 `playback_renderer=true` 与 `洛天依_V5` 电平 |

## 9. 风险与后续

- 风险：若 VOCALOID6 Editor 未勾选“允许与 VOCALOID Bridge 插件连接”（或勾选后未重载 VSTi），Bridge 无法与编辑器通信，仍会没有声音/不跟随；这是插件侧设置，不是宿主能代做的。
- 风险：Bridge 作为轨道插入效果使用，而官方流程是把 Bridge 作为音频事件 FX；当前 ARA 模型用静音音频源 + 播放区域模拟，若后续仍异常，需要进一步对照事件 FX 语义。
- 后续：查看新日志中的 `ara.playback_region`、`engine.ara/ara.timeline_failed`、`plugin.runtime/plugin.process_failed` 定位下一步。
