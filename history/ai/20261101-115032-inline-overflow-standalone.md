# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-01 11:50:32 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户反馈：所有 VST 插件 GUI 的二级菜单仍无法打开 |
| 关联分支/提交 | fix/ui-plugin-bugfix-batch / 本次提交 |
| 影响模块 | app / plugin / docs |
| 关联记录 | `history/ai/20261101-003148-vst-submenu-thread-and-churn.md` |

## 2. 任务目标

- 定位并修复“二级菜单打不开”的真实原因。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/plugins.rs` | 修改 | 新增 `INLINE_OVERFLOW_TOLERANCE`；`EditorWindow::place_inline` 返回溢出标记；`SurfingApp::place_inline_editor` 在溢出时记入 `inline_failed` 并调用 `open_editor` 改用独立窗口 |
| `src/plugin/mod.rs` | 修改 | 更新 Pro-Q 内嵌诊断测试：记录 `fit_embedded(500×300) → 1484×811`，并断言 Pro-Q 拒绝缩进卡片 |
| `doc/技术文档.md` | 修改 | 6.6 新增“插件最小尺寸大于卡片时改用独立窗口” |
| `doc/关键常量.md` | 修改 | 新增 `INLINE_OVERFLOW_TOLERANCE` |
| `doc/框架工作流.md` | 修改 | `place_inline_editor` 职责 |
| `doc/项目简介.md` | 修改 | 2.4 大界面插件行为 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 无法缩小的插件（Pro-Q 4） | 窗口被强行裁到卡片大小，GUI 只剩左上角 | 自动改用独立窗口（与插件原始尺寸一致） | 预设浏览器与其二级菜单完整可见 |
| 可缩放的插件（Auto-Tune / Serum 2） | 内嵌 | 保持内嵌 | 行为不变 |
| 内嵌失败记录 | 仅创建失败时写入 | 尺寸溢出也写入 `inline_failed` | 不会每帧重复尝试内嵌 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `INLINE_OVERFLOW_TOLERANCE` | 无 | `8`（物理像素） | `src/app/plugins.rs` | 超出卡片的容差，超过即改用独立窗口 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.6 |
| `doc/关键常量.md` | 是 | 插件窗口一节 |
| `doc/框架工作流.md` | 是 | 1.x 索引 |
| `doc/项目简介.md` | 是 | 2.4 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| 根因复现 | 完成 | `fit_embedded(500×300)` 返回 `1484×811`：Pro-Q 4 不接受内容缩放；此前宿主把窗口裁到卡片，GUI 与菜单落在窗口外 |
| `cargo fmt --all` | 通过 | 已运行 |
| `cargo check --all-targets --offline` | 通过 | 无错误、无警告 |
| `pro_q_editor_embeds_in_a_host_child_window`（ignored） | 通过 | 断言 Pro-Q 无法缩进 500×300 |
| `cargo test` | 通过 | 73 passed / 0 failed / 16 ignored |

## 8. 风险与后续

- 风险：Pro-Q 4 这类固定大界面插件不再内嵌，效果器卡片中显示提示文字；这是为保证菜单可用做出的取舍。
- 后续：若希望大界面插件也内嵌，需要实现“卡片内点击后弹出全尺寸悬浮面板”的交互。
