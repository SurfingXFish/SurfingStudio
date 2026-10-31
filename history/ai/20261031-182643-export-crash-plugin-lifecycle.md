# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-31 18:26:43 (Asia/Shanghai) |
| AI 工具/模型 | TraeCode / 未标注模型 |
| 任务来源 | 修复导出崩溃、重做导出界面、修复 Kontakt 8 GUI 卡死与无声、关闭工程时终止 VST 拉起的外部应用窗口 |
| 关联分支/提交 | `feature/audio-import-surround-gpu-export` / 待提交 |
| 影响模块 | audio, plugin, app, ui, docs |
| 关联记录 | `20261025-231208-audio-formats-surround-gpu-export.md` |

## 2. 任务目标

- 导出时程序崩溃：不再在后台线程重新实例化 Kontakt / VOCALOID 等插件。
- 导出界面与整体 IDEA New UI 风格统一，信息结构清晰。
- 打开 Kontakt 8 乐器 GUI 时宿主卡死、且乐器没有声音。
- VOCALOID 等 VSTi 拉起的外部应用窗口在关闭工程时不会一起关闭，导致再次打开工程时报插件错误。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/audio/engine/render.rs` | 修改 | 新增 `OfflineRenderSession`、`begin_offline_render` / `render_offline_step` / `finish_offline_render` / `cancel_offline_render` / `restore_after_offline` / `offline_frame_counts` / `offline_render_active`；`process_output` 在会话期间静音，实际渲染路径拆为 `process_output_internal`；新增逐帧导出与取消恢复测试 |
| `src/audio/engine/mod.rs` | 修改 | `Engine` 新增 `offline` 会话字段；新增 `shutdown_plugins`（停止处理、关闭编辑器、终止 VST3 组件、清空乐器/效果器身份键）；导出 `OfflineRenderSession` |
| `src/app/export.rs` | 修改 | 导出改为界面线程逐帧渲染：`start_export` 调用 `begin_offline_render`，`step_export_render` 每帧推进并按耗时自适应 1~64 块，`finish_offline_render` 后把样本交给写盘线程；新增 `cancel_export` / `fail_export` / `spawn_export_writer`；渲染/写盘期间禁止同步覆盖引擎与走带；移除后台线程创建插件的旧路径 |
| `src/app/mod.rs` | 修改 | 新增 `editor_repaint` 字段与 `EDITOR_REPAINT_BACKGROUND_INTERVAL`（200 ms）；窗口析构先取消导出、调用 `engine.shutdown_plugins()` 再销毁编辑器；导出/编辑器/播放期间请求持续重绘 |
| `src/app/plugins.rs` | 修改 | `EditorWindow::service(repaint)`：仅非前台编辑器低频强制重绘；`service_editors` 按槽位限流；关闭编辑器时同步清理重绘记录 |
| `src/app/files.rs` | 修改 | 应用新工程前终止旧工程 VST3 组件（`Engine::shutdown_plugins`），释放 VOCALOID 等拉起的外部应用窗口 |
| `src/app/transport.rs` | 修改 | 导出会话期间 `sync_engine` 只标记 `needs_sync`，不覆盖导出状态；播放按钮在导出期间锁定 |
| `src/plugin/runtime.rs` | 修改 | 新增 `PluginRuntime::shutdown`：终止 VST3 组件后替换为 `Failed` 并泄漏强引用；实时线程 `process` / `panic` / `send_cc` / `send_pitch_bend` 改用 `try_lock`；全量参数比对改为 `HashMap` 索引且参数数超过 `MAX_FULL_SCAN_PARAMS`（512）时跳过；编辑器反馈每帧最多格式化 64 条 |
| `src/ui/export.rs` | 重写 | 卡片式导出界面：摘要卡片、格式与质量、声道布局、渲染范围、渲染选项、导出路径分区；可点击选项卡片、细进度条、固定底部操作栏、最小化/关闭按钮、Esc 关闭；关闭时自动取消渲染 |
| `vendor/vst3-host/src/plugin.rs` | 修改 | `PluginInternal` 默认 `shutdown_component`；公开 `Plugin::shutdown_component` |
| `vendor/vst3-host/src/internal/plugin_impl.rs` | 修改 | 新增 `terminated` 状态；`shutdown_component` 按 Drop 顺序关闭编辑器、停止处理、停用并 `terminate_component`；`Drop` 在已终止时直接返回 |
| `vendor/vst3-host/src/window.rs` | 修改 | 新增 `PluginWindow::is_foreground`（`GetForegroundWindow`） |
| `doc/技术文档.md` | 修改 | 6.6/6.7 编辑器线程与重绘/参数上限、原生 `shutdown_component`；9.2 切换工程终止旧组件；9.4 导出流程与界面 |
| `doc/框架工作流.md` | 修改 | 1.10 关闭/切换工程流程、1.11 逐帧导出流程与模块地图 |
| `doc/关键常量.md` | 修改 | 导出窗口尺寸/标题栏/底栏/选项卡片、每帧块数、参数与反馈上限、后台重绘间隔 |
| `doc/项目简介.md` | 修改 | 导出窗口界面与渲染方式、关闭/切换工程的插件终止策略 |
| `doc/代码规范.md` | 修改 | 关闭工程终止组件、音频线程只用 `try_lock`、导出必须复用实时引擎 |
| `doc/日志规范.md` | 修改 | 新增 `plugin.shutdown` 事件 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 导出渲染 | 后台线程创建新 `Engine` 并重新实例化插件 | 界面线程在实时引擎上保存/覆盖状态，逐帧渲染，结束后恢复 | 不再因跨线程实例化 Kontakt / VOCALOID 崩溃；导出期间界面可刷新 |
| 导出写盘 | 后台线程渲染 + 写盘 | 渲染完成后仅把样本交给后台线程编码写盘 | 写盘不涉及插件，崩溃风险消失 |
| 导出关闭 | 窗口关闭后渲染可能继续 | 渲染阶段关闭会先取消并恢复实时引擎；写盘阶段不可取消 | 不会出现无界面的后台导出 |
| Kontakt GUI | 音频线程等待插件锁；每帧 `RedrawWindow` | 音频线程 `try_lock`（忙则本块静音）；仅非前台编辑器每 200 ms 重绘 | 打开 GUI 不再让输出流停住，减少绘制互相拖拽 |
| 参数扫描 | 停止时全量比对，O(n²)，无上限 | 仅 ≤512 参数时全量比对，`HashMap` O(n)；反馈每条插件每帧 ≤64 条 | 数千参数插件不再卡住 UI 与引擎锁 |
| 关闭/切换工程 | 旧实例在后台同步线程释放 | 先 `IComponent::terminate` 再泄漏实例，并清空身份键 | VOCALOID / ACE Bridge 外部窗口随工程关闭，重开不再报错 |
| 导出界面 | 简单表单 | 摘要 + 分区卡片 + 选项卡片 + 固定底栏进度 | 与 IDEA New UI 风格一致 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 导出窗口默认尺寸 | `580 × 560` 点 | `620 × 690` 点（最小 `520 × 560`） | `src/ui/export.rs` | 卡片布局 |
| `EXPORT_TITLE_H` | `38` 点 | `40` 点 | `src/ui/export.rs` | 标题栏含最小化/关闭 |
| `EXPORT_FOOTER_H` / `EXPORT_CHOICE_H` | 无 | `104` / `54` 点 | `src/ui/export.rs` | 固定底栏与选项卡片 |
| `EXPORT_BLOCKS_PER_FRAME` | 无（后台一次渲染） | 初始 `8`，自适应 `1 ~ 64` | `src/app/export.rs` | 按上一帧耗时调整 |
| `MAX_FULL_SCAN_PARAMS` | 无 | `512` | `src/plugin/runtime.rs` | 超过时只用编辑手势日志 |
| `MAX_EDITOR_FEEDBACK_UPDATES` | 无 | `64` | `src/plugin/runtime.rs` | 编辑器反馈格式化上限 |
| `EDITOR_REPAINT_BACKGROUND_INTERVAL` | 无 | `200` ms | `src/app/mod.rs` | 非前台编辑器强制重绘间隔 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.6 线程/重绘/参数上限、6.7 原生 patch、9.2 切换工程、9.4 导出 |
| `doc/框架工作流.md` | 是 | 1.10 关闭与切换工程、1.11 导出、2 模块地图 |
| `doc/关键常量.md` | 是 | 导出窗口与插件/编辑器常量 |
| `doc/项目简介.md` | 是 | 导出窗口、关闭/切换工程、离线导出说明 |
| `doc/代码规范.md` | 是 | 8.2 生命周期规则 |
| `doc/日志规范.md` | 是 | 5.4 新增 `plugin.shutdown` |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行 |
| `cargo check --all-targets -p surfing-studio` | 通过 | 无警告 |
| `cargo test --lib` | 通过 | 72 通过，0 失败，16 忽略（新增逐帧导出/取消恢复测试） |
| 手工验证 | 部分执行 | 未在本机加载 Kontakt 8 / VOCALOID 实测；需在安装对应插件的环境确认 GUI 交互、发声与关闭工程后外部窗口消失 |

## 8. 风险与后续

- 风险：`shutdown_plugins` 为规避插件析构死锁会泄漏已终止的实例；频繁在同一窗口切换含重型采样器（Kontakt）的工程会累积少量进程内存，建议后续提供“重启工程窗口”入口或受控析构实验。
- 风险：导出时若把采样率改成与设备不同的值，仍会在 UI 线程重载插件；重型工程点击「开始导出」后可能有短暂停顿，但不会再崩溃。
- 后续：把“打开 GUI 前的实例重建”改为不持有引擎锁的槽位级加载，进一步消除重建时的音频欠载；为导出增加后台进度条动画与剩余时间估算。
