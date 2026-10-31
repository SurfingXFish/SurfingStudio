# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-25 23:29:35 (Asia/Shanghai) |
| AI 工具/模型 | TraeCode / 未标注模型 |
| 任务来源 | 按协作规范补齐版本号更新与分支流程；解决 Cargo SemVer 与发布号 `1.1.0a` 的表示差异 |
| 关联分支/提交 | feature/audio-import-surround-gpu-export / 未知 |
| 影响模块 | app, docs |
| 关联记录 | `history/ai/20261025-231208-audio-formats-surround-gpu-export.md` |

## 2. 任务目标

- 将版本号从 `1.0.0` 提升到 `1.1.0a`（新周期首个构建）。
- 让 Cargo 使用合法的 SemVer 版本号，同时保证界面、日志与安卓清单显示 `1.1.0a`。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/lib.rs` | 修改 | 新增 `version_label()` 与私有 `format_version()`：把 `CARGO_PKG_VERSION` 的 `-a` / `-t` 后缀转换为发布号 `a` / `t`，并补单元测试 |
| `src/main.rs` | 修改 | 诊断日志初始化改用 `surfing_studio::version_label()` |
| `src/app/home.rs` | 修改 | 主页版本文字改用 `crate::version_label()` |
| `Cargo.toml` | 修改 | `[package] version` 改为 `1.1.0-a` |
| `platforms/android/Cargo.toml` | 修改 | 安卓入口 crate 版本改为 `1.1.0-a` |
| `platforms/android/AndroidManifest.xml` | 修改 | `android:versionName` 改为 `1.1.0a`；`versionCode` 保持 `1`（未对外发布） |
| `README.md` | 修改 | 当前版本改为 `1.1.0a` |
| `doc/项目简介.md` | 修改 | 当前版本改为 `1.1.0a` |
| `doc/协作规范.md` | 修改 | 4.5/4.6 记录 Cargo SemVer 写法与显示号转换规则 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 版本号显示 | 界面与日志直接使用 `CARGO_PKG_VERSION` | 统一通过 `version_label()` 转换 | Cargo 可用合法 SemVer，用户/日志/清单显示发布号 |
| 发布检查 | 要求 Cargo version 与发布号完全一致（不可实现） | 明确 `1.1.0-a` → `1.1.0a` 的映射 | 规范可执行 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 软件版本 | `1.0.0` | `1.1.0a`（Cargo `1.1.0-a`） | `Cargo.toml`、`platforms/android/AndroidManifest.xml` | 1.1 周期首个构建 |
| `version_label()` | 无 | 新增 | `src/lib.rs` | SemVer 后缀转为发布号 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 `lib.rs` 职责补充 `version_label` |
| `doc/关键常量.md` | 否 | 未新增影响行为的常量 |
| `doc/框架工作流.md` | 是 | 2 模块地图入口行补充 `version_label` |
| `doc/项目简介.md` | 是 | 当前版本改为 `1.1.0a` |
| `doc/代码规范.md` | 否 | 规范无变化 |
| `doc/协作规范.md` | 是 | 4.5/4.6 版本号落地与 SemVer 写法 |
| `README.md` | 是 | 当前版本改为 `1.1.0a` |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行 |
| `cargo test` | 通过 | `cargo test --lib`：70 通过，0 失败，16 忽略（含版本后缀转换测试） |
| 手工验证 | 通过 | `cargo metadata` 可解析 `1.1.0-a`；`version_label()` 结果为 `1.1.0a`（由单元测试覆盖） |

## 8. 风险与后续

- 风险：`version_label()` 只识别 `-a` / `-t` 后缀；后续若引入其它 SemVer 预发布后缀，需要同步扩展映射。
- 后续：正式版发布时移除后缀（Cargo `1.1.2`、清单 `1.1.2`），并递增安卓 `versionCode`。
