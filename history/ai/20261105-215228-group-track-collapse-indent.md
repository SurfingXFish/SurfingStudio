# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-05 21:52:28 (Asia/Shanghai) |
| AI 工具/模型 | TraeCode / Trae 内置编码模型 |
| 任务来源 | 组合轨道不易辨认：增加缩进并支持折叠 |
| 关联分支/提交 | feature/group-track-render-audio / 未知 |
| 影响模块 | core, ui, app, docs |
| 关联记录 | `history/ai/20261105-213117-group-track-render.md` |

## 2. 任务目标

- 增加组合成员的缩进，让嵌套关系更直观。
- 组合轨道可以折叠/展开，折叠后隐藏成员行。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/core/model/track.rs` | 修改 | 新增 `collapsed` 字段（serde 默认 `false`）与 `Track::child_lanes_visible`；折叠组合只保留主轨 |
| `src/core/model/project.rs` | 修改 | 新增 `collapsed_ancestor` / `track_is_hidden` / `subtree_track_count`；补充折叠可见性与序列化测试 |
| `src/ui/arrangement/layout.rs` | 修改 | `GROUP_INDENT` 由 10 提升到 18；`block_height` / `total_height` / `row_at` 使用 `child_lanes_visible` 并跳过折叠成员 |
| `src/ui/arrangement/mod.rs` | 修改 | 所有轨道绘制/背景/子轨/框选循环跳过折叠成员；组合成员绘制逐层竖向引导线；组合副标题显示成员数量；箭头折叠成员与自身子轨 |
| `src/app/edit.rs` | 修改 | `validate_selection` 在折叠后把主选移到组合轨道，并从多选与片段选择中移除隐藏成员 |
| `doc/技术文档.md` | 修改 | 3.2 轨道字段与折叠规则、7.2 编曲区交互 |
| `doc/关键常量.md` | 修改 | `Track.collapsed` 默认值与 `GROUP_INDENT = 18` |
| `doc/框架工作流.md` | 修改 | Project/Track/Layout/Arrangement 函数索引 |
| `doc/项目简介.md` | 修改 | 组合轨道缩进、引导线与折叠说明 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 组合成员缩进 | 每层 10 点，仅名称 | 每层 18 点，名称+副标题缩进并绘制竖向引导线 | 嵌套层级更易辨认 |
| 组合折叠 | 无；箭头只控制子轨 | 箭头折叠成员行与组合自身子轨，`Track::collapsed` 随工程保存 | 可快速收起组合 |
| 行高/命中/框选 | 固定遍历全部轨道 | 跳过折叠组合的隐藏成员 | 隐藏成员不占行高、不可选中 |
| 选择校正 | 只校验越界 | 主选落在折叠组合上，多选移除隐藏项 | 折叠后选择不指向不可见轨道 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `GROUP_INDENT` | `10` | `18` | `src/ui/arrangement/layout.rs` | 每层组合缩进（逻辑点） |
| `Track::collapsed` 默认值 | 无 | `false` | `src/core/model/track.rs` | 组合轨道默认展开成员 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 3.2 轨道、7.2 编曲区 |
| `doc/关键常量.md` | 是 | 1 工程与格式、3 渲染与导出 |
| `doc/框架工作流.md` | 是 | 模块函数索引与编曲区描述 |
| `doc/项目简介.md` | 是 | 2.1 组合轨道说明 |
| `doc/协作规范.md` | 否 | 规范无变化 |
| `doc/代码规范.md` | 否 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行 |
| `cargo test` | 通过 | `cargo test --lib`：131 passed，23 ignored |
| 手工验证 | 未执行 | 需在桌面环境实际查看缩进、引导线与折叠效果 |

## 8. 风险与后续

- 风险：折叠成员仍保留在工程与引擎中，只是不参与编曲区绘制与命中；渲染与混音行为不变。
- 后续：如需更紧凑的层级展示，可继续调整 `GROUP_INDENT` 或为折叠组合增加汇总片段占位。
