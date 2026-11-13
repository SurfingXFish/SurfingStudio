# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-14 01:31:31 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 瘦 exe + DLL 拆分、安装包、数据目录固定到安装目录、AI 智能体/技能文件化与导入导出 |
| 关联分支/提交 | feature/thin-exe-installer-ai-data / 未知 |
| 影响模块 | app, core, platform, ai, ui, build, release |
| 关联记录 | 无 |

## 2. 任务目标

- 将可执行文件拆成“车钥匙”：瘦启动器只加载运行时 DLL，运行时再按需加载 AI 资料 DLL。
- 提供 Inno Setup 安装包与一键打包脚本，默认安装到 Program Files 并保证普通用户可写数据目录。
- 把设置、日志、缓存、临时文件与工程默认目录强制绑定到安装目录 `data/`。
- 智能体以 JSON、技能以 Markdown 分类存储在安装目录，首次运行自动释放内置资料。
- 主页 Agent 资料库新增导入、导出全部与单条导出。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `Cargo.toml` | 修改 | 新增 `crates/surfing-ai-data` 与 `launcher` 工作区成员；根库改为 `cdylib + rlib`；移除 `[[bin]]`；桌面依赖增加 `libloading` |
| `Cargo.lock` | 修改 | 记录新工作区 crate 与 `libloading` 依赖 |
| `launcher/Cargo.toml`、`launcher/src/main.rs`、`launcher/build.rs` | 新增 | 瘦启动器：加载同目录 `surfing_studio.dll`、调用 `surfing_studio_run`、缺失时中文弹窗；图标资源链接到启动器 |
| `crates/surfing-ai-data/Cargo.toml`、`src/lib.rs`、`src/store.rs`、`src/markdown.rs` | 新增 | AI 资料 DLL：C ABI、分类文件读写、Markdown 头部编解码、单条/整类导入导出与单元测试 |
| `src/main.rs` → `src/desktop.rs` | 重命名/修改 | 桌面运行时代码移入库内，入口改为 `run_desktop()`，内部引用改 `crate::` |
| `src/lib.rs` | 修改 | 声明 `desktop` 模块并导出 `surfing_studio_run`（`catch_unwind` 收敛 panic） |
| `src/platform/mod.rs`、`backend.rs`、`backends/desktop.rs` | 修改 | 新增 `install_dir`；桌面目录改为 `<安装目录>/data`、`/cache`、`/logs`、`/temp`；新增目录后缀常量 |
| `src/app/ai_data.rs` | 新增 | 资料库文件加载/保存、首次释放、`libloading` 加载 AI 资料 DLL 的 C ABI 桥 |
| `src/app/ai_transfer.rs` | 新增 | 后台导入导出任务、结果轮询、唯一 id 分配与资料库落库 |
| `src/app/settings.rs` | 修改 | 启动时从 `data/ai` 加载资料库并在保存设置时同步写回分类文件 |
| `src/app/agent_library.rs` | 修改 | 页头新增「导入 / 导出全部」，编辑器新增「导出」，任务进行中禁用 |
| `src/app/mod.rs` | 修改 | 注册 `ai_data`、`ai_transfer` 模块；新增 `ai_transfer` 字段与每帧轮询 |
| `src/app/autosave.rs` | 修改 | 未保存工程的恢复文件改写到应用临时目录（安装目录 `data/temp`） |
| `build.rs` | 修改 | 移除图标链接（图标改由启动器嵌入），保留采样音色库内嵌 |
| `installer/surfing-studio.iss` | 新增 | Inno Setup：安装到 `{autopf}\SurfingStudio`，`data/` 授予 `users-modify`，卸载保留用户数据 |
| `tools/package.ps1` | 新增 | 一键构建三个产物、暂存、调用 ISCC 并复制到 `dist/` |
| `.gitignore` | 修改 | 忽略 `/dist/` |
| `README.md`、`doc/*.md` | 修改 | 同步运行布局、数据目录、安装包、导入导出、版本号与文件索引 |
| `Cargo.toml`、`platforms/android/Cargo.toml`、`platforms/android/AndroidManifest.xml` | 修改 | 版本提升到 `1.3.3-a` / `1.3.3a` |
| `history/ai/20261114-013131-thin-exe-installer-ai-data.md` | 新增 | 本记录 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 启动 | exe 内直接创建 eframe 窗口 | exe 加载 `surfing_studio.dll` → `surfing_studio_run` → `desktop::run_desktop` | exe 仅 180 KB 左右，业务逻辑可独立更新 |
| AI 资料存储 | `settings.json` 的 `ai.library` | `<安装目录>/data/ai` 的分类 JSON / Markdown，首次运行自动释放 | 手动编辑、版本管理与导入导出更方便 |
| 资料库读写 | `AppSettings` 直接序列化 | `AppSettings::load/save` 调用 `app/ai_data`，由 `surfing_ai_data.dll` 完成文件格式 | 文件格式与运行时解耦 |
| 数据目录 | `%APPDATA%` / `%LOCALAPPDATA%` / 系统临时目录 | 全部位于 `<安装目录>/data`（配置、缓存、日志、临时、AI、Projects） | 软件数据随安装目录迁移 |
| 自动保存恢复 | 系统临时目录 `SurfingStudio/` | `<安装目录>/data/temp/SurfingStudio/` | 与“数据只在安装目录”一致 |
| Agent 资料库 | 仅界面内增删改 | 增加导入、导出全部与单条导出，后台线程执行文件对话框 | 资料可在机器间迁移 |
| 发布 | 直接分发 exe | `tools/package.ps1` 生成 Inno Setup 安装包 | 安装、卸载与快捷方式统一 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `DATA_DIR_SUFFIX` | 无 | `data` | `src/platform/mod.rs` | 安装目录下的数据根 |
| `CACHE_DIR_SUFFIX` | 无 | `cache` | `src/platform/mod.rs` | 缓存子目录 |
| `TEMP_DIR_SUFFIX` | 无 | `temp` | `src/platform/mod.rs` | 临时子目录 |
| `LOG_DIR_SUFFIX` | `logs`（位于用户目录） | `logs`（位于安装目录 `data/`） | `src/platform/mod.rs` | 路径语义变化 |
| 桌面配置目录 | `%APPDATA%\SurfingStudio` | `<安装目录>/data` | `src/platform/backends/desktop.rs` | 强制绑定安装目录 |
| 桌面缓存目录 | `%LOCALAPPDATA%\SurfingStudio` | `<安装目录>/data/cache` | `src/platform/backends/desktop.rs` | — |
| 桌面日志目录 | `%LOCALAPPDATA%\SurfingStudio\logs` | `<安装目录>/data/logs` | `src/platform/backends/desktop.rs` | — |
| 桌面临时目录 | 系统临时目录 | `<安装目录>/data/temp` | `src/platform/backends/desktop.rs` | — |
| AI 资料目录 | 无 | `<安装目录>/data/ai` | `src/app/ai_data.rs` | 分类：`agents/`、`skills/basics`、`skills/techniques`、`skills/instruments`、`library.json` |
| 运行时 DLL | 无 | `surfing_studio.dll` | 根包 `[lib]` | 导出 `surfing_studio_run` |
| AI 资料 DLL | 无 | `surfing_ai_data.dll` | `crates/surfing-ai-data` | 六个 C ABI 函数 |
| 安装包 | 无 | `dist/SurfingStudio-1.3.3a-setup.exe` | `installer/`、`tools/package.ps1` | 默认目录 `{autopf}\SurfingStudio`；实测静默安装 19,511,695 字节 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 模块职责、1.4 平台目录表、新增 1.5 运行布局/安装包/数据目录、6.3 插件缓存、7.10 设置与 AI 资料库、13 日志位置、附录 B 文件索引 |
| `doc/关键常量.md` | 是 | 插件缓存/恢复/设置/日志路径；第 11 节平台目录常量；第 12 节 AI 资料与 DLL；新增第 13 节运行布局与安装包 |
| `doc/框架工作流.md` | 是 | 模块地图新增启动器、运行时入口、AI 资料 DLL；Agent 资料库入口与函数；设置读写；新增 `ai_data` / `ai_transfer` 函数表；日志解码与退出路径 |
| `doc/项目简介.md` | 是 | 版本号、运行方式、安装包、数据目录、AI 智能体文件位置与导入导出、目录结构 |
| `doc/代码规范.md` | 是 | 1.1 分层结构、1.3 第 7/13 条（启动器/FFI 约束）、13.10 平台后端方法 |
| `README.md` | 是 | 轻量描述、快速开始、安装包、目录结构、安装目录数据布局 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | `cargo fmt --all` 无格式差异 |
| `cargo test` | 通过 | `surfing-studio` 172 passed / 23 ignored；`surfing-ai-data` 4 passed |
| 手工验证 | 通过 | ① 瘦启动器 169,984 字节，`LoadLibrary` 可取得 `surfing_studio_run`；② AI 资料 DLL 六个导出均存在；③ UTF-8 参数调用 `sfs_ai_save/load/export`，生成 `library.json`、`agents/*.json`、`skills/basics/*.md` 并正确回读；④ 将 exe + 两个 DLL 复制到独立目录启动，自动生成 `data/ai` 149 个文件；⑤ release 产物同样通过；⑥ 静默安装到临时目录成功，`data` 权限为 `BUILTIN\Users:(OI)(CI)(M)`，安装后可生成 149 个 AI 文件，卸载保留 `data/ai` 与 `logs`、只清理 `cache/temp` |

## 8. 风险与后续

- 风险：安装到 `Program Files` 时依赖安装器为 `data/` 授予 `users-modify`；若用户手工拷贝文件而非安装，普通用户可能无法写入，需要以管理员身份运行或手动调整权限。
- 风险：AI 资料 DLL 缺失或被杀毒软件隔离时，程序会保留 `settings.json` 中的资料库镜像并继续启动，但文件形式的资料不会更新。
- 后续：如需支持“整库打包导入/导出（zip）”，可在现有 C ABI 上新增 `sfs_ai_*` 函数，不需要改动运行时目录结构。
