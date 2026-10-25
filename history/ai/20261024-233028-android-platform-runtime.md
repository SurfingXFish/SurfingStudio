# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-24 23:30:28 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 主力编码模型 |
| 任务来源 | 严格按 `doc/` 文档改造代码，使软件能在安卓系统上运行；参考 JVM 思路先建立统一的内部 API，再按系统提供兼容该 API 的底层实现 |
| 关联分支/提交 | 未知 |
| 影响模块 | core / audio / plugin / app / ui / platform / docs / build |
| 关联记录 | 无 |

## 2. 任务目标

- 新增「平台运行时」框架：一个对内的统一平台 API（`PlatformRuntime`），桌面与安卓各自提供后端。
- 以最小改动让同一份业务代码在安卓上运行：单窗口模式、无系统文件对话框、无 VST 宿主、应用私有目录与安卓退出方式。
- 提供可复现的安卓构建与打包链路（NDK 交叉编译 + aapt2/zipalign/apksigner），并在真机/模拟器上验证。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `Cargo.toml` | 修改 | `eframe` / `rfd` 改为按目标系统声明：桌面保留默认特性与 rfd，安卓用 `default-features = false` + `glow`/`default_fonts`/`android-native-activity`；workspace 增加 `platforms/android` |
| `src/platform/mod.rs` | 新增 | `PlatformRuntime`（`LazyLock` 单例）、能力/目录/对话框/退出快捷函数、`APP_DIR_NAME`、`LOG_DIR_SUFFIX`、`is_writable_dir` |
| `src/platform/backend.rs` | 新增 | `PlatformBackend` 接口、`PlatformCaps`（`DESKTOP` / `ANDROID`）、`FileRequest`、`PlatformInfo` |
| `src/platform/backends/mod.rs` | 新增 | 按 `cfg(target_os)` 选择后端 |
| `src/platform/backends/desktop.rs` | 新增 | 桌面后端：`%APPDATA%`/`%LOCALAPPDATA%`（或 XDG）、rfd 对话框、`TerminateProcess` 退出 |
| `src/platform/backends/android.rs` | 新增 | 安卓后端：`install_storage` 注入应用私有目录、无对话框、`process::exit` |
| `src/platform/dialog.rs` | 新增 | 从 `app/dialog.rs` 迁入 `DialogParent` 与 `parent_by_title` |
| `src/app/dialog.rs` | 删除 | 句柄封装迁至 `platform/dialog.rs` |
| `src/lib.rs` | 修改 | 声明 `pub mod platform` 并在模块文档中增加该层 |
| `src/app/settings.rs` | 修改 | `settings_path` / `default_cache_dir` 改由平台运行时给目录；文件夹选择改用 `platform::pick_folder`；新增单窗口平台的 `ui_settings_modal`（`egui::Modal`） |
| `src/app/plugins.rs` | 修改 | 插件缓存候选路径改由 `platform::{cache_dir,config_dir,temp_dir}` 生成 |
| `src/app/files.rs` | 修改 | 打开/保存/导出改用 `platform::{pick_file,save_file}`；新增 `dialog_parent()` 与 `default_project_dir()`；无系统对话框时保存/导出写入 `<配置目录>/Projects` 并创建目录 |
| `src/app/shell.rs` | 修改 | 新增 `single_window`：主页占用根窗口、拒绝第二个工程、跳过独立视口绘制 |
| `src/app/rolls.rs` | 修改 | 无多窗口能力时卷帘落到底部编辑区，禁用拖出窗口手势 |
| `src/app/window_chrome.rs` | 修改 | `window_drag = false` 时不发送 `StartDrag` |
| `src/diag/mod.rs` | 修改 | 日志目录改由平台运行时给出；`terminate_now` 走 `platform::exit_process` |
| `src/ui/browser.rs` | 修改 | 插件文件夹/单文件选择改用平台对话框 |
| `src/ui/theme.rs` | 修改 | 增加安卓系统 CJK 字体路径（`/system/fonts/...`），避免中文显示为方块 |
| `src/plugin/scan.rs` | 修改 | 安卓没有标准 VST 目录：对 `standard_vst2_dirs` / `standard_vst3_dirs` 的 `dirs` 加 `cfg_attr(target_os = "android", allow(unused_mut))`，消除安卓构建警告 |
| `src/audio/backend.rs` | 修改 | `audio_systems` 的 `systems` 在非 Windows 平台不需要 `mut`，加 `cfg_attr(not(windows), allow(unused_mut))` |
| `src/main.rs` | 修改 | 桌面入口加 `cfg(not(target_os = "android"))`；新增安卓占位 `main` |
| `platforms/android/Cargo.toml` | 新增 | `surfing-studio-android` cdylib（`libsurfing_studio_android.so`），依赖仅在安卓目标生效 |
| `platforms/android/src/lib.rs` | 新增 | `android_main`：`install_storage` → `diag::init` → `eframe::run_native` → `diag::request_exit` |
| `platforms/android/AndroidManifest.xml` | 新增 | NativeActivity、`android.app.lib_name`、横屏、`hasCode=false`、`extractNativeLibs=true`、minSdk 29 |
| `platforms/android/build.ps1` | 新增 | NDK 交叉编译 + aapt2 链接 + 写入 `.so` + zipalign + apksigner；支持 `-Abi`、`-Install`、`-Run` |
| `doc/技术文档.md` | 修改 | 1.2 模块表、1.3 框架表、新增 1.4 平台运行时 |
| `doc/关键常量.md` | 修改 | 日志目录行；新增第 11 节「平台运行时与安卓打包」 |
| `doc/框架工作流.md` | 修改 | 启动流程补安卓入口与单窗口差异、模块地图、3.7 平台索引、快速入口 |
| `doc/项目简介.md` | 修改 | 定位、2.7 安卓支持、技术栈、目录结构、5.5 安卓构建、已知限制 |
| `doc/代码规范.md` | 修改 | 分层图/职责表/依赖规则 11/判定表/模块映射/13.10 新增平台后端 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 存储目录 | 各模块各自读 `APPDATA`/`LOCALAPPDATA`/XDG | 统一由 `platform::PlatformRuntime` 给出 | 桌面路径不变；安卓落到应用沙箱 |
| 文件对话框 | 业务代码直接调用 rfd | 业务代码调用 `platform::{pick_file,pick_folder,save_file}` | 桌面行为不变；安卓返回 `None`，保存改走默认工程目录 |
| 退出 | `diag::terminate_now` 内直接调 Win32 | 改为 `platform::exit_process` | Windows 仍走 `TerminateProcess`，安卓走 `process::exit` |
| 多窗口 | 所有平台都创建 egui 视口 | `PlatformCaps::multi_window` 为假时进入单窗口模式 | 安卓单工程、主页/工程同窗口、设置与卷帘内联 |
| 启动入口 | 仅 `main.rs` | 桌面 `main.rs` + 安卓 `android_main`（独立 crate） | 同一库、两个薄入口 |
| 字体 | 只查 Windows/macOS/Linux 路径 | 追加安卓 `/system/fonts` 路径 | 安卓中文正常显示 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `APP_DIR_NAME` | 无（散落字面量 `"SurfingStudio"`） | `"SurfingStudio"` | `src/platform/mod.rs` | 目录名与插件缓存回退目录 |
| `LOG_DIR_SUFFIX` | `diag` 内部私有 | `"logs"`（公开） | `src/platform/mod.rs` | 日志子目录 |
| `LOG_DIR_NAME` / `LOG_DIR_SUFFIX`（`diag`） | 存在 | 删除 | `src/diag/mod.rs` | 迁至平台运行时 |
| 安卓最低 API | 无 | `29` | `platforms/android/build.ps1`、清单 | AMidi 需要 `libamidi` |
| 安卓包名 / 库名 | 无 | `com.surfingstudio.app` / `surfing_studio_android` | 清单、`platforms/android/Cargo.toml` | NativeActivity 加载 |
| `PlatformCaps::ANDROID` | 无 | 单窗口 / 无对话框 / 无插件 / 固定音频 | `src/platform/backend.rs` | 应用层降级依据 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 模块职责、1.3 统一框架、新增 1.4 平台运行时 |
| `doc/关键常量.md` | 是 | 第 10 节日志目录行；新增第 11 节 |
| `doc/框架工作流.md` | 是 | 1.1 启动流程、2 模块地图、3.7 平台索引、4 快速入口 |
| `doc/项目简介.md` | 是 | 1 定位、2.7 安卓、4 技术栈、5.5 构建、6 目录、7 限制 |
| `doc/代码规范.md` | 是 | 1.1/1.2/1.3/1.4/1.5、13.10 新增平台后端 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | `cargo fmt --check` 无差异 |
| `cargo test` | 通过 | 61 passed, 0 failed, 16 ignored |
| `cargo check --all-targets`（Windows） | 通过 | 桌面特性、rfd 与平台后端均编译通过 |
| `cargo check -p surfing-studio-android --target aarch64-linux-android` | 通过 | 仅 `AndroidApp` 非 FFI-safe 警告（android-activity 官方签名即如此） |
| 安卓打包 | 通过 | `target/android/SurfingStudio-arm64-v8a.apk`、`SurfingStudio-x86_64.apk` |
| 安卓运行（x86_64 模拟器，Android 16） | 通过 | 启动进入主页；中文正常；音频 `Default Device · 48000 Hz · 2 ch`；新建工程后走带从 0 播放到 62 小节；「文件 → 保存」状态栏显示 `已保存 /data/user/0/com.surfingstudio.app/files/Projects/新工程.sfsd`；重启应用后主页项目列表仍显示该工程（9 分钟前），点击可加载回工程窗口（`已加载 ...新工程.sfsd`）；主页与工程可来回切换且播放不中断 |
| 安卓真机（小米平板 25099RP13C，HyperOS / Android 15，arm64） | 通过 | 打开「USB 安装 + USB 调试（安全设置）」后 `adb install` 成功；启动进入主页；点「新建项目」进入工程并渲染 2560×1600 界面；播放/暂停正常（走带 0 → 48.1.737，CPU 0.3%~2.2%）；「文件 → 保存」状态栏显示 `已保存 /data/user/0/com.surfingstudio.app/files/Projects/新工程.sfsd`；点 logo 回主页可见该项目（刚刚）且工程继续后台播放，「继续当前工程」可返回；logcat 无 FATAL / ANR |

## 8. 风险与后续

- 风险：单窗口模式下同时只允许一个工程，打开第二个会被拒绝；安卓无系统文件对话框，「打开项目…」不可用，只能从主页列表打开已保存工程。
- 风险：`vst3-host` / `vst` 仍参与安卓编译（运行期不加载插件），后续可考虑按目标系统裁剪依赖以减小体积。
- 风险：触控交互复用桌面布局，手机小屏体验未优化。
- 后续：接入安卓 Storage Access Framework（JNI）以支持导入/导出到公共目录；按需隐藏安卓上无意义的插件设置项；补充触屏手势（缩放/长按）优化。
