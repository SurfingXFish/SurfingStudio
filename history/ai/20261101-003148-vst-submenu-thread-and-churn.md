# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-01 00:31:48 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户反馈：所有 VST 插件 GUI 的二级菜单都无法打开（不仅 Pro-Q 4） |
| 关联分支/提交 | fix/ui-plugin-bugfix-batch / 本次提交 |
| 影响模块 | app / vendor/vst3-host / docs |
| 关联记录 | `history/ai/20261031-213834-soundbug-crash-ui-polish.md` |

## 2. 任务目标

- 修复插件 GUI 的悬停二级菜单无法打开的问题（Pro-Q 4 预设浏览器及其它插件的下拉/子菜单）。
- 消除两条已定位的诱因：内嵌窗口每帧重复 Win32 调用；GUI 可能在后台线程创建的实例上打开视图。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `vendor/vst3-host/src/window.rs` | 修改 | 内嵌窗口新增 `embedded_bounds` / `embedded_clip` / `embedded_visible` 缓存（`Cell`），`set_embedded_bounds` / `set_embedded_clip` / `set_embedded_visible` 在目标值未变化时直接返回；`open` / `open_embedded` / `close` / `destroy_native_window` 重置缓存 |
| `src/app/plugins.rs` | 修改 | `ensure_ui_thread_instance` 改为返回 `bool`：同步被推迟（`engine_sync_rx.is_some()` 或 `needs_sync`）时保留 `background_synced` 标记并返回 `false`；`open_inline_editor` / `open_editor` 收到 `false` 时延后到下一帧（独立窗口提示用户重试），绝不用后台线程实例创建视图；`open_editor` 把重建检查移到“已存在窗口则置前”之后 |
| `doc/技术文档.md` | 修改 | 6.6 线程约束与内嵌窗口步骤、6.7 本地 patch 说明去抖与延后重建 |
| `doc/框架工作流.md` | 修改 | `ensure_ui_thread_instance` 返回值说明 |
| `doc/项目简介.md` | 修改 | 2.4 内嵌 GUI 与实例线程说明 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 内嵌窗口摆放 | 每帧 `SetWindowPos` + `SetWindowRgn` + `ShowWindow` | 仅在目标值变化时调用 | 不再每帧向插件窗口发送移动/裁剪/显示消息，悬停与菜单计时不被打断 |
| GUI 实例线程 | 重建被推迟时仍用后台线程实例打开 GUI | 延后到下一帧，直到实例确实在 UI 线程重建 | 插件的菜单/弹窗创建与消息泵保持同一线程 |
| 打开独立编辑器 | 先重建再检查已有窗口 | 先置前已有窗口，再按需重建 | 已打开的窗口不会因重建被推迟而无法聚焦 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无 | - | - | - | 未新增常量，缓存值运行时计算 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.6、6.7 |
| `doc/关键常量.md` | 否 | 无新增常量 |
| `doc/框架工作流.md` | 是 | 1.x 索引表 |
| `doc/项目简介.md` | 是 | 2.4 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 已运行 |
| `cargo check --all-targets --offline` | 通过 | 无错误、无警告 |
| `cargo test` | 通过 | 73 passed / 0 failed / 16 ignored |
| 交互复现 | 待用户验证 | 需要真实鼠标悬停 Pro-Q 4 预设条目，自动化无法覆盖 |

## 8. 风险与后续

- 风险：本次修复针对两条已确认的诱因，但仍无法在无人值守环境复现真实悬停；若问题依旧，需要用户提供「内嵌窗口还是独立窗口」「哪个插件」以继续缩小范围。
- 后续：可为 `plugin_window_proc` 增加可开关的消息日志（`WM_MOUSEMOVE` / `WM_SHOWWINDOW` / `WM_MOUSEACTIVATE`），在用户机器上抓取一次悬停事件序列。
