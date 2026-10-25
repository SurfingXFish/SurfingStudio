# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-25 00:49:30 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 主力编码模型 |
| 任务来源 | 继续按既定方案完成安卓 SAF 文件选择（导入/导出到公共目录）、触屏缩放手势，以及只在桌面显示的插件设置项 |
| 关联分支/提交 | 未知 |
| 影响模块 | platform / app / ui / platforms/android / docs |
| 关联记录 | [20261024-233028-android-platform-runtime.md](./20261024-233028-android-platform-runtime.md) |

## 2. 任务目标

- 在平台运行时上补齐「打开外部文件」与「另存到用户位置」两个能力，安卓用 Storage Access Framework 实现，桌面保持 rfd 行为。
- 编曲区与钢琴卷帘支持触屏双指捏合缩放，且捏合不误触发单指拖动。
- 无插件宿主能力（`plugin_hosting = false`）时，应用栏「VST 插件」菜单、主页插件页与设置插件分类整体隐藏。
- 修复 SAF 在真机上的进程崩溃，并在真机上验证导入/导出全流程。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/platform/backend.rs` | 修改 | `PlatformBackend` 新增 `import_file` / `export_copy`，默认实现分别是「复用 `pick_file`」与「`save_file` + `fs::copy`」 |
| `src/platform/mod.rs` | 修改 | `PlatformRuntime::import_file` / `export_copy` 及同名模块级快捷函数 |
| `src/platform/backends/mod.rs` | 修改 | 安卓目标下声明 `mod android_saf`；导出 `install_activity` / `install_storage` |
| `src/platform/backends/android.rs` | 修改 | `install_activity` 转交 SAF 桥；`import_via_saf`（复制到 `<数据>/Imported/`）、`export_via_saf`（按扩展名推断 MIME）、`sandbox_file_name`、`mime_for_name` |
| `src/platform/backends/android_saf.rs` | 新增 | JNI SAF 桥：`install`、`pick_document`、`create_document`、`copy_from_uri`、`copy_to_uri`、`open_fd`/`open_fd_inner`、`nativeOnActivityResult`、`trace`、`clear_exception` |
| `src/app/files.rs` | 修改 | 打开工程改走 `platform::import_file`；新增 `export_scratch_dir`、`export_project_copy_to_public`、`export_wav_copy_to_public`、`poll_public_copy`；`dialog_rx` 改为 `Result<Option<PathBuf>, String>` |
| `src/app/mod.rs` | 修改 | 新增 `public_copy_rx` 字段与每帧 `poll_public_copy` |
| `src/app/panels.rs` | 修改 | 无原生对话框时文件菜单追加「工程副本保存到公共目录…」「WAV 导出到公共目录…」；「VST 插件」菜单按 `plugin_hosting` 整体隐藏 |
| `src/app/home.rs` | 修改 | 侧栏跳过 `HomePage::Plugins`，内容页回落「项目」 |
| `src/app/settings.rs` | 修改 | 设置分类跳过 `SettingsPage::Plugins` 并在已选中插件时回落「界面」；单窗口平台用 `egui::Modal` |
| `src/ui/arrangement/mod.rs` | 修改 | 双指捏合以中心为锚点缩放时间轴；捏合期间 `arr_drag = None`，`drag_started`/`dragged`/音量条拖动都加 `!pinching` 守卫 |
| `src/ui/piano_roll/mod.rs` | 修改 | 同上，缩放 `piano_zoom`（1~16），捏合期间 `piano_drag = None` 并阻止标尺定位 |
| `platforms/android/java/com/surfingstudio/app/MainActivity.java` | 新增 | `NativeActivity` 子类：静态 `System.loadLibrary("surfing_studio_android")`、`onActivityResult` → `nativeOnActivityResult`、`queryDisplayName` 读 `OpenableColumns.DISPLAY_NAME` |
| `platforms/android/AndroidManifest.xml` | 修改 | activity 指向 `com.surfingstudio.app.MainActivity`、`hasCode="true"`、`debuggable`、`extractNativeLibs`、minSdk 29 |
| `platforms/android/build.ps1` | 修改 | 新增 `javac` + `d8` 步骤与 `classes.dex` 写入；`-Run` 启动 `MainActivity` |
| `doc/技术文档.md` | 修改 | 1.2 模块表、1.4 平台运行时（新 API、SAF 约定、能力位影响）、7.2/7.3 捏合手势 |
| `doc/关键常量.md` | 修改 | 第 11 节补导入落点、导出临时目录、SAF 请求码/超时/日志 tag、Java 类，新增 11.1 触屏手势 |
| `doc/框架工作流.md` | 修改 | 1.1 安卓启动与 SAF 时序、1.6 加载入口、模块地图、3.7 平台索引 |
| `doc/项目简介.md` | 修改 | 2.7 SAF 与触屏手势、5.5 六步打包、目录结构、7 安卓限制 |
| `doc/代码规范.md` | 修改 | 13.10 十个后端方法 + JNI 桥七条约束（工作线程等待、全局引用、异常清理、fd 复制、loadLibrary 等） |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 打开工程 | 业务层直接调用 rfd | 业务层调用 `platform::import_file` | 桌面不变；安卓走 SAF 并先复制到沙箱 |
| 导出副本 | 无此能力 | `platform::export_copy` | 桌面走「另存为 + 复制」；安卓走 SAF 新建文档 |
| 安卓文件结果 | 无 | Java `onActivityResult` → `nativeOnActivityResult` → 进程内队列 | 等待发生在工作线程，UI 不阻塞 |
| 触屏缩放 | 仅滚轮/`Ctrl` 组合 | 追加 `i.multi_touch()` 捏合 | 桌面无多点触控，不受影响 |
| 捏合期间拖动 | — | 取消 `arr_drag` / `piano_drag` 并跳过拖动分支 | 避免误移动片段/音符 |
| 插件入口 | 所有平台都显示 | 按 `plugin_hosting` 隐藏菜单、主页页与设置分类 | 安卓不再出现空插件页 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `REQUEST_OPEN_DOCUMENT` | 无 | `1001` | `android_saf.rs`、`MainActivity.java` | 两处必须一致 |
| `REQUEST_CREATE_DOCUMENT` | 无 | `1002` | 同上 | 两处必须一致 |
| `PICK_TIMEOUT` | 无 | `600 s` | `android_saf.rs` | 用户可能长时间停留在选择器 |
| SAF 日志 tag | 无 | `SurfingStudioSaf` | `android_saf.rs` | `adb logcat -s` 排障 |
| 导入落点 | 无 | `<数据目录>/Imported/` | `android.rs` | 显示名清洗后写入 |
| 导出临时目录 | 无 | `<临时目录>/SurfingStudio/export` | `app/files.rs` | SAF 导出前写快照/渲染 WAV |
| 捏合判定阈值 | 无 | `|zoom_delta - 1| > 1e-3` | 编曲区 / 钢琴卷帘 | 过滤零位移噪声 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 模块职责、1.4 平台运行时（API 表 + SAF 约定）、7.2 编曲区、7.3 钢琴卷帘 |
| `doc/关键常量.md` | 是 | 第 11 节平台运行时与安卓打包、新增 11.1 触屏手势 |
| `doc/框架工作流.md` | 是 | 1.1 安卓启动与 SAF 时序、1.6 加载工程、2 模块地图、3.7 平台索引 |
| `doc/项目简介.md` | 是 | 2.7 安卓、5.5 构建、6 目录、7 限制 |
| `doc/代码规范.md` | 是 | 13.10 新增平台后端（方法数与 JNI 桥约束） |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 通过 | 无差异 |
| `cargo test` | 通过 | 61 passed, 0 failed, 16 ignored |
| `cargo check --all-targets`（Windows） | 通过 | 桌面后端与 `import_file` / `export_copy` 默认实现编译通过 |
| 安卓交叉编译 | 通过 | `arm64-v8a` 与 `x86_64` 均构建成功，仅保留 `AndroidApp` 非 FFI-safe 的既有警告 |
| APK 打包 | 通过 | `target/android/SurfingStudio-arm64-v8a.apk`、`SurfingStudio-x86_64.apk`（含 `classes.dex`） |
| 真机崩溃修复（小米平板 25099RP13C / HyperOS / Android 15） | 通过 | `logcat -b crash` 显示 `java.lang.UnsatisfiedLinkError: No implementation found for nativeOnActivityResult`；在 `MainActivity` 静态块 `System.loadLibrary("surfing_studio_android")` 后不再崩溃 |
| 真机导出工程副本 | 通过 | 文件 → 工程副本保存到公共目录… → 下载内容 → 保存；`/sdcard/Download/新工程 (1).sfsd` 235 字节；状态栏「已导出副本 新工程 (1).sfsd」 |
| 真机导入工程 | 通过 | 主页「打开项目」→ SAF → 下载内容/午后咖啡.sfsd → 确定；`files/Imported/午后咖啡.sfsd` 1796 字节，工程以「午后咖啡」打开并渲染 鼓组/贝斯/和弦/主旋律 四条轨道 |
| 真机取消分支 | 通过 | 关闭选择器返回 `已取消加载工程`，进程存活 |
| 单窗口导入限制 | 通过 | 已有工程时导入仍完成复制，但提示「当前系统只支持一个工程窗口」 |
| 插件入口隐藏 | 通过 | 应用栏只有 文件/编辑/帮助；主页侧栏 项目/模板/提示；设置仅 界面/存储/音频/渲染/控制器/快捷键/软件信息 |
| SAF 日志埋点 | 通过 | `adb logcat -s SurfingStudioSaf` 输出 `request 1002 started=true` → `callback queued` → `result 1002 uri=true` → `fd ok mode=w` → `copy_to done` |
| 触屏捏合缩放 | 未验证 | 真机未 root 且 `/dev/input` 不可写，adb 无法注入多点触控；代码路径与桌面共用，需人工双指操作确认 |
| 单指拖动回归 | 通过 | 拖动「贝斯」片段右移成功，工程标记未保存；播放/定位与保存路径显示正常 |

## 8. 风险与后续

- 风险：`startActivityForResult` 由工作线程发起（Activity 方法通常要求主线程），当前真机可用；若后续机型报错，应改为 `runOnUiThread` 里发起。
- 风险：导入按显示名清洗后写入 `files/Imported/`，同名文件会被覆盖；如需保留多份可加时间戳后缀。
- 风险：SAF 等待上限 600 s，超时按取消处理，用户不会收到明确提示（只在状态栏显示取消）。
- 后续：为捏合手势补人工验证；考虑在导入成功后把源 URI 持久化（`takePersistableUriPermission`），支持直接引用公共目录文件而不复制。
- 后续：安卓仍加载 `vst3-host` 依赖（运行期不使用），可考虑按目标系统裁剪以减小 APK 体积。
