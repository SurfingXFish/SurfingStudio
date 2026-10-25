# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-25 20:27:37 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 内置编码模型 |
| 任务来源 | Kontakt 8 打开 GUI 仍表现为不可操作，控制台出现 `External WM_DESTROY received for ni::qt::QuickWindow` 与 `CXBrowserUI.qml` 报错 |
| 关联分支/提交 | 未知 / 未知 |
| 影响模块 | plugin, app, docs |
| 关联记录 | `history/ai/20261025-195710-kontakt-gui-open-and-editor-close.md` |

## 2. 任务目标

- 复核最新一次运行日志，确认 Kontakt 8 编辑器是否已能打开。
- 采纳插件在 attach 时请求的窗口尺寸，避免 Qt 插件（Kontakt 8）使用错误的占位尺寸。
- 打开失败时先让插件清理部分视图再销毁窗口，减少 Qt 的“外部 WM_DESTROY”报错。
- 在 `plugin.editor.open` 日志中记录编辑器尺寸，便于后续区分尺寸与渲染问题。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `vendor/vst3-host/src/window.rs` | 修改 | attach 后取出的 `IPlugFrame::resizeView` 请求改为按优先级采纳为窗口尺寸（仅 Serum 2 归一化生效时丢弃）；失败分支先 `close_editor()` 再 `DestroyWindow` |
| `src/app/plugins.rs` | 修改 | `plugin.editor.open` 成功时记录 `size`（编辑器物理像素尺寸） |
| `doc/技术文档.md` | 修改 | 6.7 本地 patch：采纳 attach 请求尺寸、失败清理、日志 `size` |
| `doc/日志规范.md` | 修改 | 5.4 `plugin.editor.open` 增加 `size` 字段 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 打开编辑器后的窗口尺寸 | 归一化未生效时也会丢弃插件 attach 期间的 resize 请求，退回 attach 尺寸或占位尺寸 | 归一化未生效时优先采用插件请求尺寸 | Qt 插件（Kontakt 8）拿到它自己声明的窗口尺寸 |
| 打开失败清理 | `DestroyWindow` 后直接丢弃视图 | 先 `close_editor()` 再销毁窗口 | 减少 Qt “External WM_DESTROY” 警告与残留窗口 |
| 诊断日志 | 只记录耗时/状态 | 额外记录编辑器尺寸 | 复盘 GUI 问题时能判断是否尺寸异常 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无 | 无 | 无 | 无 | 本次未改动常量 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.7 本地 patch |
| `doc/关键常量.md` | 否 | 未改动常量 |
| `doc/框架工作流.md` | 否 | 函数职责未变化 |
| `doc/项目简介.md` | 否 | 用户可见行为无新增说明之外的改变 |
| `doc/代码规范.md` | 否 | 规范无变化 |
| `doc/日志规范.md` | 是 | 5.4 新增 `size` 字段 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 无格式差异 |
| `cargo test --lib` | 通过 | 62 通过、16 ignored |
| 手工验证 | 未执行 | 需真实 Kontakt 8 与桌面交互；命令沙箱内 Kontakt 无法读取 `install_state` |

## 8. 风险与后续

- 风险：插件请求的尺寸可能大于屏幕；窗口仍可拖动缩放，未做上限裁剪。
- 后续：需要用户确认 GUI 是“白屏/空白”还是“有画面但点不动”、主程序是否同步卡死、Qt 行出现在打开还是关闭时，以及系统显示缩放比例，再决定是否需要“先显示窗口再 attach”或渲染后端相关处理。
