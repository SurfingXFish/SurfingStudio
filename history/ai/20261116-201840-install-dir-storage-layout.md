# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-16 20:18:40 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 主代理 |
| 任务来源 | 用户纠正：数据要放在软件安装目录下，但不要都放进 data 文件夹 |
| 关联分支/提交 | feature/midi-storage-installer / 未知 |
| 影响模块 | platform, app, docs |
| 关联记录 | `history/ai/20261116-194929-storage-layout-refactor.md`（本记录修正其系统用户目录方案） |

## 2. 任务目标

- 运行期数据仍保存在软件安装目录下，保持随安装目录迁移的特性。
- 不再统一放入 `data/`，改为按用途拆分为安装目录顶层的 `config/`、`cache/`、`logs/`、`temp/`、`Projects/` 与 `ai/`。
- 保留旧 `<安装目录>/data` 的升级迁移，避免已有设置、AI 资料与插件缓存丢失。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `Cargo.toml` | 修改 | 移除不再需要的 winapi `shlobj`、`knownfolders` 特性 |
| `src/platform/mod.rs` | 修改 | 新增 `CONFIG_DIR_SUFFIX`、`PROJECTS_DIR_SUFFIX`；移除 `documents_dir`；`data_dir` 语义改为安装目录 |
| `src/platform/backend.rs` | 修改 | 移除 `documents_dir` 后端接口 |
| `src/platform/backends/desktop.rs` | 修改 | 配置/缓存/日志/临时目录改为 `<安装目录>/{config,cache,logs,temp}`；删除用户目录与 Known Folder 辅助函数 |
| `src/app/files.rs` | 修改 | 默认工程目录改为 `<安装目录>/Projects` |
| `src/app/settings.rs` | 修改 | 路径文案改为安装目录 `config` 文件夹；保留旧 `data/settings.json` 迁移 |
| `src/app/ai_data.rs` | 修改 | AI 资料根改为 `<安装目录>/ai`；保留旧 `data/ai` 迁移 |
| `src/app/plugins.rs` | 修改 | 缓存默认路径与旧 `data/cache` 回退注释同步 |
| `src/app/mod.rs`、`src/app/shared.rs`、`src/diag/mod.rs` | 修改 | 注释中的配置与日志路径同步为安装目录布局 |
| `assets/i18n/en-US.yml` | 修改 | 同步两条路径相关界面文案的英文翻译键 |
| `crates/surfing-ai-data/src/store.rs` | 修改 | 资料库根目录注释改为 `<安装目录>/ai` |
| `installer/surfing-studio.iss` | 修改 | 恢复 `[Dirs]`，为 `config`、`cache`、`logs`、`temp`、`Projects`、`ai` 授予 users-modify；恢复 `[UninstallDelete]` 清理 `cache` 与 `temp` |
| `README.md`、`doc/项目简介.md`、`doc/技术文档.md`、`doc/关键常量.md`、`doc/框架工作流.md`、`doc/日志规范.md`、`doc/协作规范.md` | 修改 | 全部路径改为安装目录顶层分目录 |

## 4. 工作流变化

| 项目 | 变化前（本分支上一版） | 变化后 | 影响 |
| --- | --- | --- | --- |
| 配置 | `%APPDATA%\SurfingStudio\settings.json` | `<安装目录>\config\settings.json` | 数据随安装目录迁移 |
| AI 资料 | `%APPDATA%\SurfingStudio\ai` | `<安装目录>\ai` | 同上，首次升级从旧 `data/ai` 迁移 |
| 缓存/日志/临时 | `%LOCALAPPDATA%\SurfingStudio\{cache,logs,temp}` | `<安装目录>\{cache,logs,temp}` | 便于直接在安装目录调试 |
| 默认工程 | `文档\SurfingStudio\Projects` | `<安装目录>\Projects` | 与安装目录保持一致 |
| 安装包 | 仅程序文件，数据在用户目录 | 程序文件只读；数据子目录授予 users-modify | 普通用户可在 Program Files 下写数据 |
| 升级迁移 | 无旧 `data` 读取 | 保留 `<安装目录>/data` 读取并迁移 | 1.3.3a 等旧版本数据不丢失 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `CONFIG_DIR_SUFFIX` | 无 | `config` | `src/platform/mod.rs` | 安装目录下的设置目录 |
| `PROJECTS_DIR_SUFFIX` | 无 | `Projects` | `src/platform/mod.rs` | 安装目录下的默认工程目录 |
| `DATA_DIR_SUFFIX` | `data`（旧版迁移） | `data`（仅旧版迁移） | `src/platform/mod.rs` | 不再作为当前数据根 |
| `documents_dir` | 平台 API | 删除 | `src/platform/*` | 默认工程不再使用系统文档目录 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 模块表、平台目录表、1.5 安装目录布局、插件缓存、设置存储、日志路径、AI 资料路径 |
| `doc/关键常量.md` | 是 | 插件缓存、崩溃恢复、设置、日志、平台目录常量、AI 资料根目录、安装包目录 |
| `doc/框架工作流.md` | 是 | 崩溃恢复路径、桌面后端、Agent 资料库、设置加载/保存、平台 API 表 |
| `doc/项目简介.md` | 是 | 插件缓存、崩溃恢复、诊断日志、设置、AI 资料、安装包说明 |
| `doc/日志规范.md` | 是 | Windows / 其他平台日志目录 |
| `doc/协作规范.md` | 是 | Issue 模板中的日志目录 |
| `doc/代码规范.md` | 否 | 规范无变化 |
| `README.md` | 是 | 存储卖点、安装说明、安装目录结构 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | `cargo fmt --all -- --check` |
| `cargo check` | 通过 | `cargo check -p surfing-studio --all-targets` |
| `cargo test` | 通过 | `cargo test -p surfing-studio --lib`：178 passed，23 ignored |
| `tools/package.ps1` | 通过 | Inno Setup 6.7.3 成功编译 1.3.5a 安装包，`[Dirs]` 解析并打包 |
| 安装向导语言 | 通过 | 中文系统不带 `/LANG` 启动显示「安装 - SurfingStudio 版本 1.3.5a」 |
| 运行期目录 | 通过 | 在 `target/package/stage` 运行 release 程序后生成顶层 `ai/`、`cache/`、`logs/`，不存在 `data/`；`ai/` 内含 `agents/`、`skills/` 与 `library.json` |

## 8. 风险与后续

- 风险：安装目录位于 Program Files 时依赖安装包授予的 users-modify；若用户手工移动程序文件或复制到受限目录，写入可能失败。
- 后续：在真实升级场景中验证旧 `<安装目录>/data` 的 settings、ai 与插件缓存迁移；确认卸载只删除 cache/temp。
