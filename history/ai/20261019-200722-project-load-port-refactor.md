# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-19 20:07:22 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 代码智能体 |
| 任务来源 | 重构工程读取链路：建立统一内部接口，将 SoundBug 与 SurfingStudio 读取模块拆分为独立文件夹，由上层加载端口统一分派 |
| 关联分支/提交 | 未知 / 未知 |
| 影响模块 | core, app, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 为工程读取定义规范的内部接口与统一加载端口。
- 将 SurfingStudio `.sfsd` 与 SoundBug `.sndp` 读取实现拆分为两个独立文件夹模块。
- 应用层只调用上层加载端口，不再按扩展名编写格式分支。
- 保留全部现有功能、行为与工程格式兼容性。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/core/project_io/mod.rs` | 新增 | 定义 `ProjectReader` 接口、`ProjectLoadPort` 端口、`LoadedProject` / `WaveformPeak` / `ProjectSource` 与 `load_project` 入口 |
| `src/core/project_io/sfsd/mod.rs` | 新增 | `SfsdReader` 适配器、`read_sfsd` 读取校验、`write_project` / `write_container` 写入 |
| `src/core/project_io/sfsd/container.rs` | 新增 | SFS2 容器读写原语：文件头、TOC、数据块、CRC 与解压 |
| `src/core/project_io/sfsd/manifest.rs` | 新增 | 清单、峰值表序列化结构与插件引用稳定遍历 |
| `src/core/project_io/soundbug/mod.rs` | 新增 | SoundBug 读取模块聚合与 re-export |
| `src/core/project_io/soundbug/reader.rs` | 新增 | `SoundBugReader` 适配器，把导入结果封装为 `LoadedProject` |
| `src/core/project_io/soundbug/import.rs` | 移动/修改 | 从 `core/soundbug/` 迁入；更新模块头与测试引用 |
| `src/core/project_io/soundbug/convert.rs` | 移动/修改 | 从 `core/soundbug/` 迁入；更新模块头 |
| `src/core/project_io/soundbug/edit.rs` | 移动/修改 | 从 `core/soundbug/` 迁入；更新模块头 |
| `src/core/project_io/soundbug/manifest.rs` | 移动/修改 | 从 `core/soundbug/` 迁入；更新模块头 |
| `src/core/persist.rs` | 删除 | 内容拆分到 `core/project_io/sfsd/` |
| `src/core/soundbug/` | 删除 | 内容迁移到 `core/project_io/soundbug/` |
| `src/core/mod.rs` | 修改 | 以 `project_io` 取代 `persist` / `soundbug` 模块声明 |
| `src/app/mod.rs` | 修改 | 后台加载载荷改为 `(PathBuf, LoadedProject)` |
| `src/app/files.rs` | 修改 | `begin_load_project` 调用统一加载端口；应用结果按 `ProjectSource` 判断是否另存 |
| `doc/技术文档.md` | 修改 | 更新模块职责、加载流程、SoundBug 导入路径、文件索引与新增 9.5 端口说明 |
| `doc/框架工作流.md` | 修改 | 更新模块地图、文件函数索引、加载与 SoundBug 导入调用链 |
| `doc/关键常量.md` | 修改 | 更新常量所在路径，新增读取器注册顺序 |
| `doc/项目简介.md` | 修改 | 更新目录结构中的 `core/project_io/` |
| `doc/代码规范.md` | 修改 | 更新工程文件位置规则，新增 13.7 新增工程读取格式手册与检查项 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 工程加载入口 | `app/files.rs` 按扩展名分别调用 `core::persist::read_project` 或 `core::soundbug::import_project` | `app/files.rs` 只调用 `core::project_io::load_project`，由 `ProjectLoadPort` 分派 | 应用层与具体格式解耦 |
| 工程读取结果 | 分支各自返回元组 | 统一返回 `LoadedProject`（工程、峰值、警告、来源） | 应用层统一处理与提示 |
| 导入来源判断 | `is_soundbug_path` 扩展名判断 | `ProjectSource::Imported` 来源标记 | 避免路径与格式耦合 |
| 新增工程格式 | 需要修改应用层分支 | 新增 `ProjectReader` 实现并在端口注册 | 扩展点收敛到 `core/project_io` |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 工程读取器注册顺序 | 无 | `sfsd` → `soundbug` | `src/core/project_io/mod.rs` | `ProjectLoadPort::default`；未知扩展名按此顺序探测 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 模块职责、9.2 加载、9.3 SoundBug 导入、新增 9.5、11.1 测试、附录 B |
| `doc/关键常量.md` | 是 | 第 1 节常量位置与读取器注册顺序 |
| `doc/框架工作流.md` | 是 | 1.5 / 1.6 / 1.9 / 2 / 3.1 / 3.5 / 4 / 5.5 |
| `doc/项目简介.md` | 是 | 第 6 节目录结构 |
| `doc/代码规范.md` | 是 | 1.2、1.4、4.6、13.7、14 检查清单 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 无格式差异 |
| `cargo test` | 通过 | 22 通过，0 失败，4 个需真实插件的测试按预期忽略 |
| 手工验证 | 未执行 | 本次为结构性重构；读写与 SoundBug 示例导入均由单元测试覆盖 |

## 8. 风险与后续

- 风险：内部公开路径 `core::persist` / `core::soundbug` 已移除；仓库内引用已全部迁移，外部若直接依赖该库路径需要同步调整。
- 后续：无。
