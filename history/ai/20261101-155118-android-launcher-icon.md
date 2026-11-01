# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-01 15:51:18 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户反馈：1.1.9t 的 APK 没有图标且无法安装（在 MuMu x86_64 模拟器上点击无反应） |
| 关联分支/提交 | test（修复直接在 test 上提交）/ 本次提交 |
| 影响模块 | platforms/android（清单、res、打包脚本）、docs |
| 关联记录 | `history/ai/20260101-…`（无），首次处理安卓图标 |

## 2. 任务目标

- 让 APK 有启动器图标：此前清单没有 `android:icon`，打包时也完全没有编译 `res/`，安装后系统只能显示默认安卓图标。
- 排除“无法安装”的原因：改为同时写入 v1 / v2 / v3 三种签名方案，并在真实模拟器上验证安装与启动。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `platforms/android/res/mipmap-{m,h,xh,xxh,xxxh}dpi/ic_launcher.png` | 新增 | 由 `assets/icon.png` 缩放出 48/72/96/144/192 像素的启动器图标 |
| `platforms/android/AndroidManifest.xml` | 修改 | `<application>` 增加 `android:icon` / `android:roundIcon`（`@mipmap/ic_launcher`） |
| `platforms/android/build.ps1` | 修改 | 新增 `aapt2 compile` 步骤编译 `res/`，`aapt2 link` 带上 `.flat` 资源；签名显式启用 v1/v2/v3 并补 `apksigner verify`；步骤号改为 7 步 |
| `doc/项目简介.md` | 修改 | 安卓构建说明与打包步骤（6 步 → 7 步） |
| `doc/关键常量.md` | 修改 | APK 产物（签名方案）、新增启动器图标行、打包步骤 |
| `doc/技术文档.md` | 修改 | `platforms/android/*` 模块职责补充图标资源与 aapt2 compile |
| `doc/框架工作流.md` | 修改 | 安卓入口与打包一行的产物列补充 `res/mipmap-*/ic_launcher.png` |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 安卓资源 | 只打包清单，没有 `res/` 与图标 | `aapt2 compile res/` 后用 `.flat` 参与 link | 启动器、设置页、最近任务都能显示 SF STUDIO 图标 |
| APK 签名 | 只写 v3 | v1 + v2 + v3 同时写入，并在打包末尾 `verify` | 更老的 ROM / 第三方安装器也能识别 |
| 打包步骤 | 6 步（link → javac/d8 → 写入 → align → sign） | 7 步（多一步 `aapt2 compile`） | 文档与脚本编号一致 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 启动器图标尺寸 | 无 | mdpi 48 / hdpi 72 / xhdpi 96 / xxhdpi 144 / xxxhdpi 192 | `platforms/android/res` | 与 Android 密度规范一致 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | module 表 `platforms/android/*` |
| `doc/关键常量.md` | 是 | 11 平台运行时与安卓打包 |
| `doc/框架工作流.md` | 是 | 平台与打包表 |
| `doc/项目简介.md` | 是 | 2.x 安卓说明、5.x 打包步骤 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `aapt2 dump badging` | 通过 | `application: icon='res/mipmap-mdpi-v4/ic_launcher.png'`，五种密度齐全 |
| `zipalign -c -v 4` | 通过 | `resources.arsc` 未压缩且 4 字节对齐，PNG 资源对齐 |
| `apksigner verify` | 通过 | 存在 `META-INF/ANDROIDD.SF` 等 v1 文件，v3 校验通过 |
| MuMu 模拟器（Android 15 / x86_64）安装 | 通过 | `adb install` 返回 `Success`，`primaryCpuAbi=arm64-v8a`（模拟器带 ARM 转译） |
| 启动运行 | 通过 | `am start` 后进程存活，截图显示主页界面与版本号 `1.1.9t` |

## 8. 风险与后续

- 风险：模拟器安装成功说明包本身可安装；用户此前的“点了没反应”更可能来自第三方安装器或未完成的下载，仍然建议用 `adb install` 或系统安装器安装。
- 后续：尚未提供自适应图标（`mipmap-anydpi-v26`）与 themed icon，主题图标模式下会显示普通图标；如需可在 `res/` 增加前景/背景层。
