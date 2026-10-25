# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-19 22:15:00 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 修复 Vocaloid6 加载、效果器子轨卡死/动态 GUI、VST1 空白、SoundBug 插件状态丢失、播放头后方仍从头渲染 |
| 关联分支/提交 | 未知 |
| 影响模块 | app, plugin, render, core, docs |
| 关联记录 | `history/ai/20261019-214500-render-thread-async-load.md` |

## 2. 任务目标

- Vocaloid6 等会在加载时创建窗口的 VST 插件可在后台加载并保持可用。
- 播放中打开效果器子轨不再导致主窗口无法操作；嵌入 GUI 的输入与刷新正常。
- VST1 乐器编辑器不再显示空白。
- SoundBug 工程的 VST 插件状态不再丢失。
- 播放头位于片段后方时，预渲染从播放头所在分块开始，而不是片段开头。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/transport.rs` | 修改 | 新增常驻 `SyncRequest` / `run_sync_service` / `pump_plugin_messages`：STA COM + 消息泵，加载线程不退出；`seek` 调用 `cancel_inflight` |
| `src/app/mod.rs` | 修改 | 新增 `engine_sync_tx`；退出时断开常驻同步线程 |
| `src/plugin/editor.rs` | 修改 | VST1/VST2 编辑器附加到填满客户区的 `STATIC` 子窗口；`WM_SIZE` 同步子窗口尺寸 |
| `vendor/vst3-host/src/embed.rs` | 修改 | 嵌入编辑器创建与尺寸协商使用 `try_lock`；原生容器裁剪到卡片矩形，避免超尺寸子窗口覆盖主窗口并吞掉输入 |
| `src/render/manager.rs` | 修改 | 播放头在片段内部时从播放头分块开始预取；新增 `cancel_inflight`；工作线程/插件线程结果携带任务代次，定位后旧结果全部忽略 |
| `src/render/offline.rs` | 修改 | `render_clip_chunks` 支持取消代次，定位后旧工作线程任务尽快停止 |
| `src/core/project_runtime/soundbug/edit.rs` | 修改 | `Value::Bytes` 保留 tag 3 二进制状态块；新增 `Node::prop_bytes` |
| `src/core/project_runtime/soundbug/convert.rs` | 修改 | 导入 `PLUGIN.state`：二进制块直接使用，文本按 `.` 分隔 Base64 解码；写入 `PluginRef::state`；仅接受 32 位十六进制 uid 作为 VST3 class id |
| `src/core/project_runtime/soundbug/import.rs` | 修改 | 导入测试断言插件状态非空 |
| `src/plugin/mod.rs` | 修改 | 新增 ignored 测试：SoundBug 插件状态可在真实 VST3（TDR Nova）中恢复 |
| `doc/技术文档.md` | 修改 | 常驻同步线程、VST1/VST2 子窗口、嵌入裁剪与 try_lock、SoundBug 状态导入、分块预取 |
| `doc/关键常量.md` | 修改 | 常驻同步线程、VST2 编辑器子窗口 |
| `doc/框架工作流.md` | 修改 | 同步服务、取消渲染、SoundBug 状态解码 |
| `doc/项目简介.md` | 修改 | 插件加载、GUI 裁剪、VST1 子窗口、播放头预渲染、SoundBug 状态 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 后台插件同步 | 每次加载工程新建线程，加载完线程退出 | 常驻 STA 线程 + 消息泵 | Vocaloid 启动窗口、Pro-Q GUI 定时器不会随线程退出失效 |
| 效果器子轨嵌入 | 插件固定尺寸大于卡片时，子 HWND 可覆盖主窗口 | 原生容器裁剪到卡片矩形 | 修复“主窗口卡死但音频正常” |
| 嵌入尺寸协商 | 阻塞等待插件锁 | `try_lock`，忙时下一帧重试 | 播放中打开/拖动效果器 GUI 不再阻塞 UI |
| VST1/VST2 编辑器 | 直接附加到顶层窗口 | 附加到填满客户区的 `STATIC` 子窗口 | 修复 VST1 GUI 空白 |
| SoundBug 插件导入 | 只保留路径/名称，状态丢弃 | 二进制或 Base64 文本状态解码后写入 `PluginRef::state` | VST3 插件设置随工程恢复 |
| 预渲染起点 | 空闲预取从首个缺失分块（通常是 0）开始 | 播放头在片段内时从播放头分块开始；定位作废旧任务 | 不再从片段开头重渲染 |
| 任务取消 | 只能等待任务完成 | 任务代次 + `cancel_inflight` | 定位后旧结果不会安装 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 后台插件同步线程 | 每次任务新建 | `1` 个常驻线程 | `src/app/transport.rs` | STA + 消息泵 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 2.1、2.2、5.3、6.6、6.7、9.2、9.3、11、12.2 |
| `doc/关键常量.md` | 是 | 4、5 节 |
| `doc/框架工作流.md` | 是 | 1.6、3.1、3.3、3.5、4 |
| `doc/项目简介.md` | 是 | 2.4、3.1、7 |
| `doc/代码规范.md` | 否 | 本次未改变编码规范 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | `cargo test --lib`：27 通过，6 ignored |
| 手工验证 | 通过 | `restores_soundbug_plugin_state_in_a_vst3`（ignored）通过：SoundBug 状态可在 TDR Nova 中恢复；启动/关闭应用，进程在 10 秒内正常退出 |
| 手工验证 | 通过 | 常驻 STA 线程可成功加载 `VOCALOID VSTi`（探测结果写入 `VOCALOID VSTi`）；该插件在承载线程退出时会触发进程级异常，因此应用内使用常驻线程并在退出阶段不做插件析构 |

## 8. 风险与后续

- 风险：常驻同步线程在加载插件期间持有引擎锁，音频回调会静音/淡出；加载工程时主动暂停传输。
- 风险：VST2 插件状态已随工程保存，但当前运行时只恢复 VST2 参数快照，尚未应用二进制 chunk。
- 后续：可为 VST1/VST2 增加二进制 chunk 恢复；为 SoundBug 的 VST2 参数块补充解析。
