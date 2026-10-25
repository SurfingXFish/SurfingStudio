# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-19 20:21:37 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 代码智能体 |
| 任务来源 | 将工程读取端口升级为类 JVM 的工程运行时框架：上层只调用框架方法，底层 sfsd / soundbug 作为转译后端 |
| 关联分支/提交 | 未知 / 未知 |
| 影响模块 | core, app, docs |
| 关联记录 | `history/ai/20261019-200722-project-load-port-refactor.md`（本记录在其基础上继续演进） |

## 2. 任务目标

- 建立类 JVM 的工程运行时框架：应用层只调用框架 API。
- 把 `sfsd` / `soundbug` 下沉为可插拔的转译后端。
- 框架统一提供打开、保存、格式转译与会话生命周期。
- 保留全部现有功能与工程格式兼容性。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/core/project_io/` | 重命名 | 升级为 `src/core/project_runtime/`，明确“运行时框架”语义 |
| `src/core/project_runtime/mod.rs` | 重写 | 定义 `ProjectRuntime`、进程级入口、打开/保存/转译与后端注册 |
| `src/core/project_runtime/codec.rs` | 新增 | 定义 `ProjectCodec` 转译接口与 `CodecInfo` 能力描述 |
| `src/core/project_runtime/session.rs` | 新增 | 定义 `ProjectSession`、`ProjectSource`、`WaveformPeak` |
| `src/core/project_runtime/sfsd/mod.rs` | 修改 | `SfsdReader` 改为 `SfsdCodec`，实现 `decode` / `encode` |
| `src/core/project_runtime/sfsd/container.rs` | 修改 | 更新后端模块头与所属模块标识 |
| `src/core/project_runtime/sfsd/manifest.rs` | 修改 | 改用 `project_runtime::WaveformPeak`，更新后端模块头 |
| `src/core/project_runtime/soundbug/mod.rs` | 修改 | 声明 `codec` 子模块并导出 `SoundBugCodec` |
| `src/core/project_runtime/soundbug/codec.rs` | 重命名/重写 | 由 `reader.rs` 改为 `codec.rs`，实现只读 `ProjectCodec` |
| `src/core/project_runtime/soundbug/import.rs` | 修改 | 测试改用 `ProjectRuntime` 保存/打开；更新后端模块头 |
| `src/core/project_runtime/soundbug/convert.rs` | 修改 | 更新后端模块头 |
| `src/core/project_runtime/soundbug/edit.rs` | 修改 | 更新后端模块头 |
| `src/core/project_runtime/soundbug/manifest.rs` | 修改 | 更新后端模块头 |
| `src/core/mod.rs` | 修改 | 声明 `project_runtime` 模块 |
| `src/app/mod.rs` | 修改 | 后台加载载荷改为 `ProjectSession` |
| `src/app/files.rs` | 修改 | 加载/保存只调用 `ProjectRuntime::open_project` / `save_project` |
| `doc/技术文档.md` | 修改 | 新增 9.5 工程运行时框架说明并更新路径与类型名 |
| `doc/框架工作流.md` | 修改 | 更新加载/保存/SoundBug 流程、模块地图与函数索引 |
| `doc/关键常量.md` | 修改 | 后端路径与注册顺序说明 |
| `doc/项目简介.md` | 修改 | 目录结构改为 `project_runtime` 框架 + 两个后端 |
| `doc/代码规范.md` | 修改 | 依赖规则新增框架边界；13.7 改为新增格式后端手册 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 应用层加载 | 调用 `ProjectRuntime` 风格的加载端口 | 只调用 `ProjectRuntime::open_project`，返回 `ProjectSession` | 应用层与格式后端彻底解耦 |
| 应用层保存 | 直接调用 SFSD 写入函数 | 只调用 `ProjectRuntime::save_project`，由框架选择可写后端 | 保存路径统一走框架 |
| 格式后端 | `ProjectReader`（只有 read） | `ProjectCodec`（info / decode / encode） | 支持转译与能力声明 |
| 格式转译 | 无统一入口 | `ProjectRuntime::translate_project`：解码 → 规范工程 → 编码 | 新增跨格式转译能力 |
| 新增格式 | 新增读取器并注册 | 新增 `ProjectCodec` 后端并在 `ProjectRuntime::standard` 注册 | 扩展点收敛，应用层零修改 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 格式后端注册顺序 | `sfsd` → `soundbug` | 不变 | `src/core/project_runtime/mod.rs` | 由 `ProjectRuntime::standard` 注册；未知扩展名按此顺序探测 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2、9.2、9.3、9.5、11.1、附录 B |
| `doc/关键常量.md` | 是 | 第 1 节路径与注册顺序 |
| `doc/框架工作流.md` | 是 | 1.5、1.6、1.9、2、3.1、3.5、4、5.5 |
| `doc/项目简介.md` | 是 | 第 6 节目录结构 |
| `doc/代码规范.md` | 是 | 1.3、1.4、13.7、14 检查清单 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 无格式差异 |
| `cargo test` | 通过 | 24 通过，0 失败，4 个真实插件测试按预期忽略 |
| 手工验证 | 未执行 | 本次为架构升级；框架打开/保存/转译与 SoundBug 导入均由单元测试覆盖 |

## 8. 风险与后续

- 风险：内部模块路径由 `core::project_io` 变更为 `core::project_runtime`；仓库内引用已全部迁移。
- 后续：无。
