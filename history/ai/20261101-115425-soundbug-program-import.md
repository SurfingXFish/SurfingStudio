# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-01 11:54:25 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户提供 `SoundBug.exe` / `Assistant.exe`，要求继续解决问题 4（插件内部信息缺失） |
| 关联分支/提交 | fix/ui-plugin-bugfix-batch / 本次提交 |
| 影响模块 | core / audio / plugin / docs |
| 关联记录 | `history/ai/20261101-002719-soundbug-state-and-clip-gain.md` |

## 2. 任务目标

- 参考 SoundBug 二进制，继续尝试恢复导入工程的插件内部信息。
- 在状态块无法还原的前提下，至少恢复插件程序（预设）选择。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/core/model/plugin.rs` | 修改 | `PluginRef` 新增 `program: Option<i32>`（`#[serde(default)]`），`new()` 置 `None` |
| `src/core/project_runtime/soundbug/convert.rs` | 修改 | `PLUGIN.programNum` → `SoundBugPlugin::program` → `plugin_ref` 写入 `PluginRef::program` |
| `src/plugin/runtime.rs` | 修改 | 新增 `PluginRuntime::select_program`：VST3 通过 `IUnitInfo` 选择根单元程序并刷新参数；失败/越界静默忽略 |
| `src/audio/engine/mod.rs` | 修改 | 三处插件加载：`restore_state == false` 且 `program` 存在时调用 `select_program` |
| `src/core/project_runtime/sfsd/mod.rs` | 修改 | 测试用 `PluginRef` 字面量补 `program: None` |
| `src/core/project_runtime/soundbug/import.rs` | 修改 | 示例工程测试新增「导入的程序号必须保留」断言 |
| `doc/技术文档.md` | 修改 | 3.5 `PluginRef` 字段、9.3 `programNum` 与 SoundBug 二进制分析结论 |
| `doc/项目简介.md` | 修改 | 7 已知限制补充程序号恢复 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 导入的插件状态 | 只保留、不恢复 | 仍不恢复，但若带 `programNum` 则选择该程序 | 采样器/合成器可回到原来的音色槽位 |
| `.sfsd` 持久化 | 无程序号字段 | 保存/读取 `program`（旧工程为 `None`） | 手动切换程序后再保存也能记住 |
| SoundBug 二进制分析 | 未分析 | 确认为 Tracktion Engine + JUCE，状态容器仍不可还原 | 文档记录结论，避免重复投入 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无 | - | - | - | 未新增全局常量 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 3.5、9.3 |
| `doc/关键常量.md` | 否 | 无新增常量 |
| `doc/框架工作流.md` | 否 | 加载顺序未变化 |
| `doc/项目简介.md` | 是 | 第 7 节 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| SoundBug.exe 符号分析 | 完成 | 发现 `tracktion::engine::ExternalPlugin`、`VST3PluginState`、JUCE 相关 RTTI；无明文状态 XML、无已知压缩头 |
| `cargo fmt --all` | 通过 | 已运行 |
| `cargo test` | 通过 | 73 passed / 0 failed / 17 ignored（含新增程序号断言） |

## 8. 风险与后续

- 风险：`programNum` 若为 1 基或与插件程序表顺序不一致，可能选到相邻预设；`select_program` 失败时保持默认，不会破坏工程。
- 结论：SoundBug 的插件状态容器仍无法在宿主侧解码，插件参数/音色细节无法完整恢复；程序号是目前可恢复的最大信息量。
- 后续：如需完整恢复，只能由 SoundBug 导出插件预设或提供解码工具。
