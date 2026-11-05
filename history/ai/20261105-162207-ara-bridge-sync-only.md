# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-05 16:22:07 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | “未修复，且现在点击 Vocaloid 内音符也没有声音”，附日志 `surfing-20261105-161603-p27856.sflog` |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | plugin, audio, docs |
| 关联记录 | `history/ai/20261105-161431-ara-region-before-activation.md` |

## 2. 任务目标

- 修复上一版“激活前建立播放区域”带来的回归：编辑器内音符试听也变成无声。
- 让 VOCALOID 的播放链路符合 Bridge 的真实职责（同步走带，而不是发声）。

## 3. 日志证据

- `ara.playback_region start=0.000 duration=257.419 playback_renderer=true editor_renderer=true`：播放区域已在激活前建立并被两个渲染器接受。
- `ara.session_created` / `ara.bound roles=7`：绑定正常。
- `洛天依_V5` 轨道电平全程 0.0000；其他轨道正常。
- `engine.ara/ara.timeline_failed ... 必须在创建文档的线程调用`：界面线程的同步调用（值未变）被误记为失败。
- 无 `plugin.runtime/plugin.process_failed`：VST3 处理本身没有报错。

## 4. 根因

VOCALOID Bridge 的官方定位是“走带/速度/循环同步桥”：Studio One 流程把它放在一条静音音频轨的事件 FX 上，人声由 VOCALOID VSTi 发声。上一版让 Bridge 的播放区域真正生效后，播放渲染器按 ARA 语义用它的（静音）渲染结果替换轨道音频，所以连 VSTi 的编辑器试听也被吞掉。Bridge 本身不产生人声。

## 5. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/ara/mod.rs` | 修改 | 新增 `sync_only_passthrough`（VOCALOID Bridge 判定）；新增 `notify_selection` 并在建立播放区域后调用；`set_timeline` / `set_tempo` 先比较值再检查线程，值未变直接返回，消除界面线程的误报 |
| `src/plugin/ara/ffi.rs` | 修改 | 新增 `ARAViewSelection`（48 字节）与 `ARAEditorViewInterface`（24 字节），`editor_view_interface` 改为具体类型；尺寸用例补充断言 |
| `src/plugin/runtime.rs` | 修改 | `Vst3Runtime` 新增 `sync_only`；`process` 对同步桥保持输入直通（仍调用 `process_audio` 送达走带），并首次处理时写 `ara.bridge_processed` |
| `src/plugin/backends/vst3.rs` | 修改 | 加载时按插件名设置 `sync_only` |
| `src/audio/engine/mod.rs` | 修改 | `restore_ara_archive` 失败改为写 `engine.ara/ara.archive_restore_failed`，不再静默；新增 `EffectRuntime::set_playing` |
| `src/audio/engine/render.rs` | 修改 | 停止时把 `kPlaying=false` 也发给效果器/主效果器，播放开始时恢复 `kPlaying=true` |
| `doc/技术文档.md` | 修改 | 6.7 修正 Bridge 的职责、输入直通、`notifySelection`；更新“尚未实现” |
| `doc/框架工作流.md` | 修改 | ARA 函数表补充 `notify_selection`、`sync_only_passthrough`、先比较后检查线程 |

## 6. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| Bridge 音频输出 | 用 ARA 渲染结果替换轨道音频 | 输入直通，只接收走带 | 恢复编辑器试听，VSTi 人声不被静音渲染替换 |
| 编辑器视图选择 | 从不通知 | 建立播放区域后 `notifySelection` | 让 Bridge 能把 DAW 时间轴映射到编辑器时间轴 |
| 值未变的时间线同步 | 界面线程调用返回线程错误并写日志 | 先比较缓存值，未变直接返回 | 消除 `ara.timeline_failed` 噪声 |
| ARA 归档恢复失败 | 静默忽略 | `engine.ara/ara.archive_restore_failed` | 可定位编辑器内容为空的来源 |
| 效果器的播放状态 | 从不发送（默认恒为 playing） | 停止发 `false`、新一轮播放发 `true` | ARA 同步桥不会让编辑器停在“播放中”，也不会缺失起播通知 |

## 7. 关键常量变化

无。

## 8. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.7 |
| `doc/框架工作流.md` | 是 | `src/plugin/ara/` 函数表 |
| `doc/关键常量.md` | 否 | 无常量变化 |
| `doc/项目简介.md` | 否 | 无用户可见功能变化 |
| `doc/代码规范.md` | 否 | 无变化 |

## 9. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test --lib` | 通过 | 116 passed / 23 ignored |
| 新增结构体尺寸 | 通过 | `AraEditorViewInterface` 24 / `AraViewSelection` 48 |
| 实机验证 | 待执行 | 需检查编辑器试听恢复、`ara.selection` 是否出现、`ara.bridge_processed` 的走带位置 |

## 10. 风险与后续

- 风险：Bridge 与 VOCALOID6 Editor 的 IPC 连接依赖编辑器设置「允许与 VOCALOID Bridge 插件连接」，勾选后必须重载 VOCALOID VSTi；未满足时编辑器播放头仍不会跟随。
- 后续：若 `ara.bridge_processed` 显示 Bridge 已在走带中被处理、但编辑器仍不跟随，下一层排查 Bridge 对 ARA 内容（tempo/bar）与选择的具体要求。
