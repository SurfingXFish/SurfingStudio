# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-05 22:23:52 (Asia/Shanghai) |
| AI 工具/模型 | TraeCode / Trae 内置编码模型 |
| 任务来源 | 撤销不再回退展开/折叠；组合层级改为等差数列缩进 |
| 关联分支/提交 | feature/group-track-render-audio / 未知 |
| 影响模块 | app, ui, docs |
| 关联记录 | `history/ai/20261105-221619-group-members-flat-inset.md` |

## 2. 任务目标

- Ctrl+Z 不再撤销轨道展开/折叠等纯显示状态。
- 组合层级逐级稍微内缩，缩进量按等差数列排布；下一级子轨不与上一级重合。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/history.rs` | 修改 | 新增 `mark_view_dirty`：只置脏不递增 `edit_seq`、不触发同步 |
| `src/ui/arrangement/mod.rs` | 修改 | 展开/折叠改用 `mark_view_dirty`；子轨与组合脊线使用 `nested_inset` 的层级缩进 |
| `src/ui/arrangement/layout.rs` | 修改 | 新增 `NESTED_INDENT_STEP = 6`、`NESTED_INDENT_MAX_LEVELS = 4` 与 `nested_inset`；卡片内缩为 `4 + 层级 × 6` |
| `doc/技术文档.md` | 修改 | 7.2 组合缩进、7.8 撤销行为 |
| `doc/关键常量.md` | 修改 | 内缩基础值、步长与最大层数 |
| `doc/框架工作流.md` | 修改 | `mark_view_dirty`、`nested_inset` 索引 |
| `doc/项目简介.md` | 修改 | 组合缩进与撤销说明 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 展开/折叠撤销 | 调用 `mark_dirty` 递增 edit_seq，产生撤销步骤 | 调用 `mark_view_dirty`，只置脏保存 | Ctrl+Z 不再回退展开/折叠，也不会连带重排界面 |
| 组合缩进 | 所有层级统一 4 点 | 基础 4 点 + 每层 6 点（4、10、16、22……，最多四层） | 层级有轻微等差缩进，子轨不再与上级重合 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `NESTED_CARD_INSET` | `4` | `4`（保留为基础值） | `src/ui/arrangement/layout.rs` | 顶层卡片/子轨起始内缩 |
| `NESTED_INDENT_STEP` | 无 | `6` 点/层 | `src/ui/arrangement/layout.rs` | 等差步长 |
| `NESTED_INDENT_MAX_LEVELS` | 无 | `4` | `src/ui/arrangement/layout.rs` | 最大累计层数 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 7.2 编曲区、7.8 撤销/重做 |
| `doc/关键常量.md` | 是 | 3 渲染与导出 |
| `doc/框架工作流.md` | 是 | History/Layout 函数索引 |
| `doc/项目简介.md` | 是 | 2.1 组合轨道、2.6 撤销 |
| `doc/协作规范.md` | 否 | 规范无变化 |
| `doc/代码规范.md` | 否 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行 |
| `cargo test` | 通过 | `cargo test --lib`：132 passed，23 ignored |
| 手工验证 | 未执行 | 需在桌面环境确认 Ctrl+Z 行为与层级缩进 |

## 8. 风险与后续

- 风险：展开/折叠仍会置脏，关闭未保存工程时会提示保存；这是为了让 `expanded` / `collapsed` 随工程持久化。
- 后续：若希望展开/折叠完全不置脏，可把它们移出 `Project` 改为 UI 状态，但这会改变工程保存格式的语义。
