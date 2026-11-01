# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-01 12:12:45 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户反馈：SurfingStudio 工程读取失败 |
| 关联分支/提交 | fix/ui-plugin-bugfix-batch / 本次提交 |
| 影响模块 | core / docs |
| 关联记录 | `history/ai/20261101-115425-soundbug-program-import.md` |

## 2. 任务目标

- 修复上一提交引入的 `.sfsd` 读取失败。

## 3. 实际问题

`.sfsd` 用 `rmp_serde::to_vec` 的**紧凑数组编码**（按字段声明顺序），上一条提交把 `program` 插在 `params` 与 `enabled` 之间：
- 旧工程没有该字段，读取时 `program` 会消费原本属于 `enabled` 的 bool → 类型错误 → “工程读取失败”。
- 新工程被旧代码读取也会错位。

## 4. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/core/model/plugin.rs` | 修改 | `program` 移到 `PluginRef` 末尾（`enabled` 之后），保留 `#[serde(default)]`；新增回归测试 `legacy_compact_plugin_ref_without_program_still_loads`（用旧字段顺序编码后反序列化）；旧 JSON 测试补充 `program` 为空断言 |
| `doc/技术文档.md` | 修改 | 3.5 字段顺序注释与“字段顺序约束”说明 |

## 5. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 新增持久化字段 | 可插在任意位置 | 必须追加在末尾并带 serde 默认值 | 旧工程保持可读 |

## 6. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无 | - | - | - | 本次未新增常量 |

## 7. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 3.5 |
| `doc/关键常量.md` | 否 | 无常量变化 |
| `doc/框架工作流.md` | 否 | 流程未变化 |
| `doc/项目简介.md` | 否 | 用户可见行为无变化 |

## 8. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 已运行 |
| `cargo test --lib plugin` | 通过 | 含新增的旧格式反序列化测试 |
| `cargo check --all-targets --offline` | 通过 | 无错误、无警告 |

## 9. 风险与后续

- 风险：其他结构体新增字段时仍需遵守“末尾追加 + 默认值”，已在文档中明确。
- 后续：可考虑把工程元数据改为 `rmp_serde::to_vec_named`（map 编码）以彻底消除顺序敏感，但会让旧文件无法读取，需要迁移方案。
