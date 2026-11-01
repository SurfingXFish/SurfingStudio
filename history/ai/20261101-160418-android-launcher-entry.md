# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-01 16:04:18 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户反馈：1.1.10t 的 APK 安装后不出现在桌面 / 应用列表 |
| 关联分支/提交 | test / 本次提交 |
| 影响模块 | platforms/android（清单）、docs |
| 关联记录 | `history/ai/20261101-155118-android-launcher-icon.md` |

## 2. 任务目标

- 让安装后的应用出现在桌面与「所有应用」列表，可以直接点击启动。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `platforms/android/AndroidManifest.xml` | 修改 | 启动意图的 action 由 `android.app.NativeActivity` 改为 `android.intent.action.MAIN`，保留 `LAUNCHER` 分类；同时修正文件头注释（hasCode 早已改为 true） |
| `doc/技术文档.md` | 修改 | 安卓启动路径段落补充桌面入口约定（MAIN + LAUNCHER，`android.app.NativeActivity` 不是启动 action） |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 桌面入口 | action 写成 `android.app.NativeActivity`，系统不认为是可启动应用 | `android.intent.action.MAIN` + `LAUNCHER` | 安装后出现在桌面/应用列表，`aapt2 dump badging` 也会输出 `launchable-activity` |
| 启动方式 | 只能 `adb shell am start -n 包名/Activity` | 桌面点击、`am start` 均可 | 与常规安卓应用一致 |

## 5. 关键常量变化

无。

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 安卓启动路径（3.x） |
| `doc/关键常量.md` | 否 | 无常量变化 |
| `doc/框架工作流.md` | 否 | 桌面入口不涉及模块职责变化 |
| `doc/项目简介.md` | 否 | 行为恢复为常规安卓应用 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `aapt2 dump badging` | 通过 | 输出 `launchable-activity: name='com.surfingstudio.app.MainActivity'` |
| `adb install` | 通过 | MuMu 模拟器（Android 15 / x86_64）安装成功 |
| 启动入口解析 | 通过 | `cmd package resolve-activity -c android.intent.category.LAUNCHER` 能解析到 `MainActivity` |

## 8. 风险与后续

- 风险：无；`am start -n` 显式启动仍然可用。
- 后续：仍未提供自适应图标（`mipmap-anydpi-v26`），主题图标模式下显示普通图标。
