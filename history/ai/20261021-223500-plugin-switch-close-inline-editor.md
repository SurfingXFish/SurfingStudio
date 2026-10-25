# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-21 22:35:00 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 切换工程时独立插件 GUI 不关闭并闪退；希望 VST 效果器 GUI 渲染在效果器卷帘卡片中 |
| 关联分支/提交 | 未知 |
| 影响模块 | app / ui / vendor/vst3-host / docs |
| 关联记录 | `history/ai/20261021-220910-plugin-gui-heartbeat-fast-scan.md` |

## 2. 任务目标

- 切换工程时自动关闭仍打开的独立插件编辑器，并消除随之而来的闪退。
- 新增能力：把 VST3 效果器原生 GUI 内嵌渲染到效果器卷帘卡片主体中。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `vendor/vst3-host/src/window.rs` | 修改 | 新增 `PluginWindow::open_embedded(parent)`：创建 `WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS` 子窗口（不带 `WS_VISIBLE`）并附加插件视图；新增 `set_embedded_bounds` / `set_embedded_clip` / `set_embedded_visible`；`embedded` 字段；`resize_native_window` 在内嵌模式下不再响应插件 resize；`close` 重置内嵌标记 |
| `vendor/vst3-host/Cargo.toml` | 修改 | winapi features 增加 `wingdi`（`CreateRectRgn`） |
| `src/app/plugins.rs` | 修改 | `EditorWindow` 增加 `Vst3Inline`；新增 `hide_inline` / `place_inline` / `main_window_handle` / `has_inline_editor` / `toggle_inline_editor` / `place_inline_editor` / `finish_inline_layout`；`open_editor` 与内嵌窗口互斥；`service_editors` 不再每帧强制隐藏 |
| `src/app/mod.rs` | 修改 | 新增 `inline_placed` 帧内记录；每帧末尾调用 `finish_inline_layout` 隐藏未摆放的内嵌窗口 |
| `src/app/files.rs` | 修改 | `new_project` / `begin_load_project` / `apply_loaded_project` 统一为「暂停音频 → 重置播放 → 关闭全部编辑器 → 替换工程」 |
| `src/app/history.rs` | 修改 | `after_history_apply` 按同样顺序重置播放并关闭编辑器 |
| `src/ui/effects.rs` | 修改 | 「界面 / 收起」按钮与卡片头部双击切换内嵌编辑器；内嵌时用卡片主体矩形调用 `place_inline_editor`；浮动卷帘与 VST2 回退独立窗口 |
| `src/plugin/mod.rs` | 修改 | 新增诊断测试 `pro_q_editor_embeds_in_a_host_child_window`（默认 ignore）：验证内嵌子窗口的位置、尺寸与显隐 |
| `doc/技术文档.md` | 修改 | 6.6 内嵌窗口、6.7 本地 patch、7.1 帧流程、7.5 卷帘、7.6 浮动卷帘 |
| `doc/框架工作流.md` | 修改 | `app/plugins.rs` / `ui/effects.rs` 索引、加载工程顺序 |
| `doc/项目简介.md` | 修改 | 2.4 插件、7 已知限制、目录注释 |
| `doc/代码规范.md` | 修改 | 8.2 编辑器窗口生命周期与内嵌几何约束 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 打开插件 GUI | 只能打开厂商独立窗口 | 底部效果器卷帘内嵌到卡片主体；浮动卷帘 / VST2 / 失败时回退独立窗口 | GUI 与效果器链在同一视图，无需再切换到独立窗口 |
| 内嵌窗口几何 | 无 | 每帧由 `ui_effects` 给出卡片主体矩形；`SetWindowRgn` 裁剪到卷帘可见区；帧末隐藏未摆放窗口 | 滚动、关闭卷帘、切换视图时不会残留窗口 |
| 切换工程 | 旧编辑器继续存在，插件实例替换后闪退 | 先暂停音频、重置播放、关闭全部编辑器，再替换工程与同步实例 | 编辑器不会晚于插件实例释放 |
| 编辑器尺寸 | 独立窗口跟随插件 resize | 内嵌模式忽略插件 resize，卡片尺寸为准；宿主改尺寸时仍 `onSize` 转发 | 避免窗口在宿主与插件之间来回跳动 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 内嵌卡片内缩 | 无 | `6` 逻辑点 | `src/ui/effects.rs` | 插件子窗口与卡片圆角之间留白 |
| `EDITOR_REPAINT_INTERVAL`、`HEARTBEAT_INTERVAL` | 16 ms | 不变 | `src/app/mod.rs`、`src/app/heartbeat.rs` | 内嵌窗口同样依赖 UI 心跳刷新 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.6、6.7、7.1、7.5、7.6 |
| `doc/关键常量.md` | 否 | 未新增全局常量 |
| `doc/框架工作流.md` | 是 | 1.6 加载顺序、`app/plugins.rs`、`ui/effects.rs` 索引 |
| `doc/项目简介.md` | 是 | 2.4、7、目录树 |
| `doc/代码规范.md` | 是 | 8.2 生命周期 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 已运行 |
| `cargo check --all-targets` | 通过 | 无错误 |
| `cargo test` | 通过 | 24 passed / 0 failed / 10 ignored |
| `pro_q_editor_embeds_in_a_host_child_window` | 通过 | 内嵌子窗口位置 (40,40)、尺寸 500×300、可见性切换正确 |
| 应用启动 | 通过 | `target/debug/surfing-studio.exe` 启动后 10 s 内保持运行 |

## 8. 风险与后续

- 风险：主窗口使用 OpenGL 交换缓冲，插件子窗口与父窗口共用同一客户区；若出现闪烁或分析器停帧，需要在每帧交换后强制子窗口重绘（`InvalidateRect` / `RedrawWindow`），或在内嵌模式下降低父窗口重绘频率。
- 风险：固定尺寸插件（`IPlugView::canResize == false`）不会接受卡片尺寸，GUI 会在卡片内留白。
- 风险：内嵌窗口与独立窗口互斥，切换模式会重建插件视图，个别插件需要重新打开分析器。
- 后续：用 FabFilter Pro-Q 4 / Pro-DS 验证内嵌模式频谱持续刷新、横向滚动裁剪、底部面板收起/展开以及切换工程不闪退。
