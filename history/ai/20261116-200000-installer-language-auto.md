# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-16 20:00:00 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 主代理 |
| 任务来源 | 用户反馈安装界面只有英文，要求按系统语言自动选择 |
| 关联分支/提交 | feature/midi-storage-installer / 未知 |
| 影响模块 | build, docs |
| 关联记录 | `history/ai/20261116-194929-storage-layout-refactor.md`、`history/ai/20261116-195500-midi-async-reconnect.md` |

## 2. 任务目标

- 安装向导支持简体中文与英文，并根据 Windows 界面语言自动选择。
- 不依赖 Inno Setup 安装目录中是否带有中文语言文件。
- 桌面快捷方式与完成页启动项跟随当前安装语言显示。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `installer/surfing-studio.iss` | 修改 | 增加 `LanguageDetectionMethod=uilanguage`、`ShowLanguageDialog=auto`、`UsePreviousLanguage=no`；`[Languages]` 增加简体中文；任务与完成页改用内置 `{cm:CreateDesktopIcon}`、`{cm:AdditionalIcons}`、`{cm:LaunchProgram,...}` |
| `installer/languages/ChineseSimplified.isl` | 新增 | 随仓库提供完整 Inno Setup 简体中文语言文件（UTF-8 BOM，含 `[Messages]` 与 `[CustomMessages]`） |
| `README.md` | 修改 | 安装说明补充自动语言选择，目录结构注明中文语言文件 |
| `doc/项目简介.md` | 修改 | 5.3 安装包说明补充自动语言选择 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 安装语言 | 固定英文 | 按 Windows 界面语言自动选择中文或英文 | 中文系统不再显示纯英文安装界面 |
| 语言文件来源 | 仅 `compiler:Default.isl` | 英文用自带，中文用仓库内文件 | 兼容中文语言文件缺失的 Inno Setup 6 |
| 任务/完成页文案 | 中英双语硬编码 | 使用当前语言的内置消息 | 文案跟随语言，不再中英混排 |
| 语言选择框 | 未启用 | 仅在没有匹配语言时显示 | 常见系统无需手动选择 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无 | 无 | 无 | 无 | 仅安装脚本配置，未新增 Rust 常量 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 否 | 安装包脚本不在运行时技术文档范围 |
| `doc/关键常量.md` | 否 | 无 Rust 常量变化 |
| `doc/框架工作流.md` | 否 | 运行时流程无变化 |
| `doc/项目简介.md` | 是 | 5.3 构建、安装包与测试 |
| `doc/代码规范.md` | 否 | 规范无变化 |
| `README.md` | 是 | 发布安装包与目录结构 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 未运行 | 未修改 Rust 代码 |
| `cargo test` | 未运行 | 未修改 Rust 代码 |
| Inno Setup 编译 | 未运行 | 当前机器未安装 Inno Setup；脚本语法按官方文档核对 |
| 手工验证 | 未执行 | 需在中文与英文 Windows 上分别运行 setup.exe 检查向导语言 |

## 8. 风险与后续

- 风险：若构建机使用 Inno Setup 6.4 或更早版本，6.5.0+ 语言文件中的新增消息可能产生“未识别消息”警告，但不影响安装包生成；建议使用 Inno Setup 6.5+ 或 7。
- 后续：发布前分别在简体中文与英文系统上验证安装向导、桌面快捷方式任务与完成页启动文案。
