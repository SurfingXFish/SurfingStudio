# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-16 19:49:29 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 子代理 backend-architect |
| 任务来源 | 用户要求修改软件存储格式，不再把所有数据放进安装目录 data 文件夹 |
| 关联分支/提交 | feature/midi-storage-installer / 未知 |
| 影响模块 | platform, app, docs |
| 关联记录 | `history/ai/20261116-195500-midi-async-reconnect.md`、`history/ai/20261116-200000-installer-language-auto.md` |

## 2. 任务目标

- 桌面端配置、缓存、日志、临时文件、AI 资料与默认工程目录按系统规范分目录存放。
- 安装目录保持只读，安装包不再为 `data/` 授予普通用户修改权限。
- 升级时迁移旧 `<安装目录>/data` 中的设置、AI 资料与插件缓存，避免用户数据丢失。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `Cargo.toml` | 修改 | winapi 增加 `shlobj`、`knownfolders` 特性，用于解析系统“文档”目录 |
| `src/platform/backend.rs` | 修改 | 新增 `documents_dir` 后端接口，默认返回 `None` |
| `src/platform/mod.rs` | 修改 | 新增 `documents_dir` 运行时入口与快捷函数、`legacy_data_dir`；更新目录常量注释 |
| `src/platform/backends/desktop.rs` | 修改 | Windows 改为 `%APPDATA%\SurfingStudio`、`%LOCALAPPDATA%\SurfingStudio\{cache,logs,temp}`，新增 Known Folder 文档目录；macOS/Linux 使用 XDG；安装目录不再承载运行期数据 |
| `src/app/settings.rs` | 修改 | 新配置缺失时复制/读取旧 `<install>/data/settings.json` |
| `src/app/ai_data.rs` | 修改 | 新 AI 目录无有效条目时从旧 `<install>/data/ai` 迁移并写回 |
| `src/app/plugins.rs` | 修改 | 插件缓存候选加入旧 `data/cache/plugin-cache.json`；临时回退路径去掉重复的 `SurfingStudio` |
| `src/app/files.rs` | 修改 | 默认工程目录改为“文档/SurfingStudio/Projects”；导出临时目录改为 `temp/export` |
| `src/app/autosave.rs` | 修改 | 未保存工程的恢复文件目录改为 `temp/recovery`，同步更新测试 |
| `crates/surfing-ai-data/src/store.rs` | 修改 | 注释改为资料库根目录，不再声明安装目录 |
| `installer/surfing-studio.iss` | 修改 | 删除 `[Dirs]` 的 data 目录与 `[UninstallDelete]` 的 data 清理项 |
| `README.md`、`doc/项目简介.md`、`doc/技术文档.md`、`doc/关键常量.md`、`doc/框架工作流.md`、`doc/日志规范.md` | 修改 | 同步新的存储路径、升级迁移与安装包行为 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 桌面配置 | `<安装目录>/data/settings.json` | `%APPDATA%\SurfingStudio\settings.json` | 安装目录可只读，配置随用户漫游 |
| 缓存/日志/临时 | `<安装目录>/data/{cache,logs,temp}` | `%LOCALAPPDATA%\SurfingStudio\{cache,logs,temp}` | 便于调试与清理 |
| AI 资料 | `<安装目录>/data/ai` | `%APPDATA%\SurfingStudio\ai` | 用户可编辑，首次升级自动迁移 |
| 默认工程目录 | `<安装目录>/data/Projects` | `<系统文档>/SurfingStudio/Projects` | 符合用户文档习惯 |
| 安装包 | 创建 data 子目录并授予 users-modify | 安装目录只读 | 卸载不再清理运行期数据 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `DATA_DIR_SUFFIX` | 安装目录数据根 | 仅旧版迁移使用 | `src/platform/mod.rs` | 新布局不再使用 |
| `RECOVERY_DIR_NAME` | `SurfingStudio` | `recovery` | `src/app/autosave.rs` | 应用临时目录已含 `SurfingStudio`，避免重复 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 模块表、4.x 平台存储目录、插件缓存、设置存储、13.1 日志路径 |
| `doc/关键常量.md` | 是 | 插件缓存、崩溃恢复、设置、日志、平台目录常量、AI 资料根目录 |
| `doc/框架工作流.md` | 是 | 桌面后端、设置加载/保存、平台目录入口、AI 资料加载 |
| `doc/项目简介.md` | 是 | 自动保存、AI 资料、安装包说明 |
| `doc/代码规范.md` | 否 | 规范无变化 |
| `README.md` | 是 | 功能卖点、安装说明、安装目录结构 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | `cargo fmt --all -- --check` 无差异 |
| `cargo test` | 通过 | `cargo test -p surfing-studio --lib`：178 passed，23 ignored |
| `cargo check` | 通过 | `cargo check -p surfing-studio -p surfing-ai-data --all-targets` |
| 手工验证 | 未执行 | 需在干净用户目录验证首次启动、升级迁移与安装包 |

## 8. 风险与后续

- 风险：旧安装目录中的工程文件不会自动搬移，仍可通过最近项目列表打开；macOS 使用 XDG 而不是原生 Library 目录。
- 后续：发布前在真实安装包上验证配置、日志、AI 资料与工程默认目录，并确认卸载保留用户数据。
