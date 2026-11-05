# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-02 23:09:46 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户要求“继续修复并实现 ARA 归档持久化”，并附日志 `surfing-20261102-230139-p11748.sflog` |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | plugin, audio, core(工程持久化), docs |
| 关联记录 | `history/ai/20261102-225812-ara-role-binding-fix.md`（角色绑定修复） |

## 2. 任务目标

- 实现 ARA 归档（插件侧编辑状态）随工程保存与恢复。
- 确认日志中 VOCALOID 未生效的原因，并保证 ARA 路径可用。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/ara/mod.rs` | 修改 | 归档回调改为真实读写内存（`ArchiveWriter` / `ArchiveReader`）；新增 `refresh_archive` / `archive_snapshot` / `restore_archive`，按内容变更计数节流导出 |
| `src/plugin/mod.rs` | 修改 | 新增 `pack_plugin_state` / `split_plugin_state`：`SFS3PLUG` 头 + u32 状态长度 + VST3 状态 + ARA 归档；无归档时保持原始字节 |
| `src/plugin/runtime.rs` | 修改 | 新增 `refresh_ara_archive` / `ara_archive_snapshot` / `restore_ara_archive` |
| `src/audio/engine/plugins.rs` | 修改 | 保存工程时把 ARA 归档打包进 `PluginRef::state`；新增 `capture_plugin_snapshot` |
| `src/audio/engine/mod.rs` | 修改 | 乐器/效果器/主效果器加载时拆分状态并恢复 ARA 归档；同步时间线后按需刷新归档 |
| `doc/技术文档.md` | 修改 | 6.7 增加归档持久化说明，更新“尚未实现” |
| `doc/关键常量.md` | 修改 | 新增插件状态容器头常量 |
| `doc/框架工作流.md` | 修改 | 补充归档与打包 API |
| `doc/项目简介.md` | 修改 | 更新 ARA 持久化描述 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 工程保存 | 只保存 VST3 状态块 | VST3 状态 + ARA 归档写入同一个 `PLUG` 块 | Bridge 编辑器里的编辑可以随工程保存 |
| 工程加载 | 只 `restore_state` | 拆分后先恢复 VST3 状态，再 `beginEditing → restoreObjectsFromArchive → endEditing` | 重新打开工程可恢复 VOCALOID 序列 |
| ARA 归档导出时机 | 无 | 在文档控制器线程、插件报告内容变化后导出到缓存；保存时读缓存 | 不在保存线程直接调用插件，避免线程违规 |
| 旧工程 | — | 没有 `SFS3PLUG` 头时整体按 VST3 状态处理 | 完全向后兼容 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `PLUGIN_STATE_CONTAINER_MAGIC` | 无 | `SFS3PLUG` | `src/plugin/mod.rs` | 8 字节容器头，后接 u32 VST3 状态长度 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.7 |
| `doc/关键常量.md` | 是 | 第 4 节 |
| `doc/框架工作流.md` | 是 | 3.3 `plugin/ara` |
| `doc/项目简介.md` | 是 | 第 7 节 |
| `doc/代码规范.md` | 否 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | 116 passed / 23 ignored（新增状态容器往返用例） |
| 归档导出/恢复 | 通过 | 真实插件用例完成 `refresh_archive`、`restore_archive` 与销毁；空文档导出 0 字节属正常（尚无编辑内容） |
| 旧状态兼容 | 通过 | 无容器头时 `split_plugin_state` 返回原始 VST3 状态 |

## 8. 风险与后续

- 风险：归档导出依赖插件发送内容变更通知；若插件不通知，缓存可能滞后一次同步周期。当前按“缓存为空或计数变化”触发导出。
- 风险：效果器 ARA 会话在后台同步线程创建；保存发生在 UI 线程时无法刷新归档，只能使用最近一次缓存。已通过每次同步按需刷新降低影响。
- 说明：日志 `surfing-20261102-230139` 中没有 `VOCALOIDBridge.vst3` 的加载记录，`洛天依_V5` 仍只加载了 `VOCALOID VSTi.vst3`（无 ARA）。需要在人声轨效果链中插入 `VOCALOID Bridge` 才会走 ARA 路径。
- 后续：接入 `notifySelection` / `notifyHideRegionSequences`，让 Bridge 编辑器与宿主选择同步。
