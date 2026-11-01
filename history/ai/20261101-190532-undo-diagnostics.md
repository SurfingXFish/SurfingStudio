# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-01 19:05:32 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户反馈 Ctrl+Z 依旧失效，并提供运行日志 |
| 关联分支/提交 | feature/marquee-context-menu / 未知 |
| 影响模块 | app, docs |
| 关联记录 | `history/ai/20261101-185905-shortcut-event-modifiers.md` |

## 2. 任务目标

- 放宽快捷键的焦点判定：只有真正处于文本编辑时才让出快捷键。
- 增加撤销诊断日志，便于从 `.sflog` 判断是“按键未到达”“被输入框占用”还是“撤销栈为空”。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/input.rs` | 修改 | `typing` 改用 `Context::text_edit_focused()`；计算一次 `undo_pressed`，识别到时记录 `app.input/shortcut.undo` 诊断事件；文本编辑中被按下时在状态栏提示按 Esc |
| `src/app/history.rs` | 修改 | `undo()` 记录 `app.input/history.undo`（`result=applied/empty` 与剩余栈深） |
| `doc/日志规范.md` | 修改 | 新增 5.8 `app.input` 事件目录 |
| `doc/技术文档.md` | 修改 | 7.13 焦点判定与诊断事件说明 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 快捷键焦点判定 | `egui_wants_keyboard_input()`：任何控件有焦点都阻断快捷键 | `Context::text_edit_focused()`：仅文本编辑中阻断 | 普通焦点不再吞掉撤销 |
| 撤销诊断 | 无日志 | Ctrl+Z 识别与撤销结果写入 `app.input` 事件 | 下次日志可直接判断失效环节 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无 | 无 | 无 | 无 | 无 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 7.13 |
| `doc/关键常量.md` | 否 | 未修改常量 |
| `doc/框架工作流.md` | 否 | 函数名与职责未变化 |
| `doc/项目简介.md` | 否 | 用户可见行为不变 |
| `doc/代码规范.md` | 否 | 规范本身无变化 |
| `doc/日志规范.md` | 是 | 新增 5.8 `app.input` |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 已运行 |
| `cargo test --lib` | 通过 | 80 passed / 0 failed / 17 ignored |
| 手工验证 | 未执行 | 需用户在新构建中复现后查看 `app.input` 事件 |

## 8. 风险与后续

- 风险：每次 Ctrl+Z 增加两条 INFO 事件，日志量很小。
- 后续：根据用户新日志中 `shortcut.undo` 是否出现、`typing/focused/stack` 的取值定位剩余问题：
  - 没有 `shortcut.undo`：按键未到达宿主（焦点在插件窗口/输入法拦截）。
  - `typing=1`：文本编辑占用。
  - `stack=0`：撤销栈为空（操作未记录快照）。
