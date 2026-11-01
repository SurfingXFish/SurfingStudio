# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-01 12:18:56 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户反馈：Pro-Q 4 / VOCALOID 6 二级菜单仍打不开，按住 Ctrl+Alt 时菜单反而出现 |
| 关联分支/提交 | fix/ui-plugin-bugfix-batch / 本次提交 |
| 影响模块 | plugin / app / docs |
| 关联记录 | `history/ai/20261101-121310-editor-alt-menu-and-foreground.md` |

## 2. 任务目标

- 解释并修复“按住 Ctrl+Alt 二级菜单才出现”的现象。

## 3. 根因分析

`WM_TIMER` 在 Win32 中是**最低优先级**消息：它不会放进消息队列，只在 `GetMessage`/`PeekMessage`
发现队列为空时合成；线程在 `MsgWaitForMultipleObjectsEx` 上空闲等待时**不会因定时器到期而唤醒**。
插件（FabFilter 的悬停展开、VOCALOID 的 WPF 菜单延迟）依赖定时器推进，因此：
- 宿主空闲时定时器一直不派发，悬停再久也不出二级菜单；
- 按下 Ctrl+Alt 产生键盘输入唤醒消息循环，积压的 `WM_TIMER` 被派发，菜单才“跳出来”。

## 4. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/editor.rs` | 修改 | 新增 `pump_editor_timers()`：用 `PeekMessageW(.., WM_TIMER, WM_TIMER, PM_REMOVE)` 只取出本线程已到期的定时器并 `DispatchMessageW`；非 Windows 为空实现 |
| `src/plugin/mod.rs` | 修改 | 导出 `pump_editor_timers` |
| `src/app/plugins.rs` | 修改 | `service_editors` 在存在编辑器时先调用 `pump_editor_timers()` |
| `doc/技术文档.md` | 修改 | 6.6 新增“主动派发 WM_TIMER” |
| `doc/项目简介.md` | 修改 | 2.4 |

## 5. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 插件定时器 | 依赖 winit 被输入唤醒后才派发 | 每帧主动派发本线程到期的 `WM_TIMER` | 悬停计时/菜单延迟不再需要先按键 |
| 其它消息 | 全部由 winit 处理 | 仍由 winit 处理（只取 `WM_TIMER`） | 不改变输入处理顺序 |

## 6. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无 | - | - | - | 使用系统常量 `WM_TIMER` |

## 7. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.6 |
| `doc/关键常量.md` | 否 | 未新增常量 |
| `doc/框架工作流.md` | 否 | `service_editors` 职责已在文档中 |
| `doc/项目简介.md` | 是 | 2.4 |

## 8. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 已运行 |
| `cargo check --all-targets --offline` | 通过 | 无错误、无警告 |
| `cargo test` | 通过 | 见提交前完整测试 |
| 现场复现 | 待用户验证 | 需要真实悬停 Pro-Q 预设条目 / VOCALOID 菜单 |

## 9. 风险与后续

- 风险：在帧内派发定时器会让插件回调在宿主 update 中执行（标准宿主行为）；若插件在定时器里弹出模态菜单，会嵌套在本帧内，退出后继续绘制。
- 若仍不生效，下一步在编辑器窗口挂 `SetWinEventHook` + `WM_TIMER` 计数日志，确认定时器是否创建、是否被派发。
