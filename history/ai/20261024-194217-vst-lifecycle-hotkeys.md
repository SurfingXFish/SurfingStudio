# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-24 19:42:17 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 修复进程残留、打开工程时关闭崩溃、播放中打开 VST GUI 后音频颗粒化、VOCALOID 6 GUI 崩溃；补全滑动与画笔快捷键并支持自定义 |
| 关联分支/提交 | 未知 |
| 影响模块 | app, audio, plugin, ui, docs, build |
| 关联记录 | 无 |

## 2. 任务目标

- 消除插件未处理异常导致进程停留在错误报告对话框、外部 `taskkill` 返回“拒绝访问”的问题。
- 打开工程过程中关闭窗口时不再触发 VOCALOID 的 `NullReferenceException`，也不卡死。
- VST 乐器实例、GUI 与析构保持在同一线程，修复 VOCALOID 6 GUI 无法打开或点击后崩溃。
- 播放中打开 VST GUI 不再导致音频欠载与持续颗粒化输出。
- 补全滑动（视图平移）与控制器画笔切换快捷键，并支持在设置中自定义全部快捷键。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/shortcuts.rs` | 新增 | `ShortcutAction` / `ShortcutGroup` / `Binding`；默认绑定、中文显示文本与英文持久化文本、解析、精确修饰键匹配、冲突检测；`scroll_view` 实现视图滑动；3 个单元测试 |
| `src/app/settings.rs` | 修改 | `AppSettings` 新增 `shortcuts` 字段；快捷键页改为可编辑（录制、清除、单条重置、全部恢复默认）；新增 `capture_hotkey_input` 与冲突提示 |
| `src/app/mod.rs` | 修改 | 新增 `close_after_ready` / `hotkey_recording` / `hotkey_hint` 与 `ENGINE_RELEASE_WAIT`；关闭请求在加载/同步期间改为延迟关闭；`Drop` 先取消音频输出目标并等待引擎引用释放，再销毁编辑器与后台线程 |
| `src/app/input.rs` | 修改 | `handle_keyboard` 全部改为读取 `AppSettings` 绑定；新增 `handle_controller_tool_shortcuts`（4 循环、Ctrl+1~4 直选）与 `cancel_or_close`；新增滑动动作 |
| `src/app/shell.rs` | 修改 | 根窗口与工程视口在 `can_close_now()` 为假时取消关闭并调用 `request_close_when_ready` |
| `src/app/transport.rs` | 修改 | `SyncRequest` 新增 `defer_instruments`；后台同步线程与音频解码线程改为只持引擎 `Weak`；新增 `project_has_plugin_instruments` / `can_close_now` / `request_close_when_ready`；`poll_engine_sync` 在 UI 线程补载 VST 乐器；`toggle_play` 先执行 `prepare_playback_plugins` |
| `src/app/plugins.rs` | 修改 | 新增 `prepare_playback_plugins`；`ensure_ui_thread_instance` 改为 `invalidate_plugin_slot` 只重建目标槽位；`begin_engine_load` 清空后台实例标记 |
| `src/audio/engine/mod.rs` | 修改 | 新增 `sync_project_with(project, defer_plugin_instruments)`；后台同步可延后 VST 乐器实例化 |
| `src/audio/engine/plugins.rs` | 修改 | 新增 `invalidate_plugin_slot`：只丢弃指定乐器/效果器/主效果器运行时 |
| `src/audio/backend.rs` | 修改 | 新增 `AudioRouter::deactivate`，窗口关闭时取消输出目标 |
| `src/plugin/runtime.rs` | 修改 | VST3 复用 `AudioBuffers`、VST2 复用输入/输出通道缓冲，实时回调不再每块分配 |
| `src/plugin/backends/vst3.rs` / `vst2.rs` | 修改 | 运行时初始化新增缓冲字段 |
| `src/app/home.rs` | 修改 | 主页提示页与新建/打开按钮的快捷键文本改为读取当前绑定 |
| `src/ui/help.rs` | 修改 | 快捷键列表改为读取当前绑定；复制/粘贴、工具、删除、滑动与控制器画笔提示均使用当前绑定；快捷键列表放入限高滚动区 |
| `src/ui/controller.rs` | 修改 | `Tool::label` 改为公开，供快捷键状态栏使用 |
| `src/ui/mod.rs` | 修改 | 删除不再使用的 `MouseMode::shortcut` |
| `src/main.rs` | 修改 | 安装 `SetUnhandledExceptionFilter`：未处理异常直接 `TerminateProcess` |
| `Cargo.toml` | 修改 | winapi 增加 `errhandlingapi` feature |
| `doc/技术文档.md` | 修改 | 2.1/2.2 线程与引擎所有权、实时缓冲复用；6.6 插件线程约束；7.10 设置字段与快捷键页；7.13 快捷键；9.1 关闭流程；9.2 加载流程 |
| `doc/关键常量.md` | 修改 | 插件缓冲复用、乐器实例化线程、`ENGINE_RELEASE_WAIT`、延迟关闭、可配置快捷键、滑动步长 |
| `doc/框架工作流.md` | 修改 | 首帧/关闭/加载/插件流程、模块地图、函数索引（shortcuts、settings、transport、plugins、input、engine） |
| `doc/项目简介.md` | 修改 | 插件线程与崩溃修复、关闭中加载、播放中打开 GUI、可自定义快捷键、滑动与画笔快捷键 |
| `doc/代码规范.md` | 修改 | 7.4 与 13.5 要求快捷键统一注册在 `app/shortcuts.rs` 并读取 `shortcut_label` |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 打开工程的插件加载 | 后台线程创建全部插件实例，VST 乐器也在后台创建 | 后台线程只创建效果器；VST 乐器在后台同步结束后由 UI 线程补载 | 乐器实例、GUI、析构同线程，VOCALOID 不再跨线程崩溃 |
| 后台同步线程/音频解码 | 持有引擎 `Arc`，窗口关闭后仍可能最后析构引擎与插件 | 只持 `Weak`；关闭时先取消输出目标并等待引用释放 | 插件实例最终在 UI 线程析构，避免后台线程销毁插件窗口 |
| 关闭加载中的工程 | 直接销毁窗口与应用状态，插件可能在初始化中途被析构 | 取消关闭并延迟到后台同步结束；未开始的加载直接放弃 | 不再出现 `NullReferenceException` 与卡死进程 |
| 插件同步长时间不返回 | 关闭只能无限等待 | 5 秒后强制关闭窗口并设置 `abandon_engine`，泄漏一次引擎引用 | 加载卡死时窗口仍可关闭 |
| 播放前插件准备 | 后台创建的效果器实例保留到首次打开 GUI 时才重建 | 开始播放前 `prepare_playback_plugins` 逐槽位重建 | 播放中打开 GUI 不触发长时间引擎锁占用，避免音频欠载 |
| 打开 GUI 的实例重建 | `invalidate_track_plugins` 重建整条轨道插件链 | `invalidate_plugin_slot` 只重建目标槽位 | 重建耗时显著降低 |
| 插件音频处理缓冲 | VST3/VST2 每个 block 分配缓冲 | 块大小/声道数不变时复用 | 降低实时线程分配与掉音风险 |
| 快捷键 | 写死在 `input.rs`、帮助、菜单 | `AppSettings::shortcuts` 统一存储，全部 UI 读取同一配置 | 用户可录制、清除、重置，滑动与画笔动作可绑定 |
| 未处理异常 | Windows 错误报告可能长期占用进程 | `SetUnhandledExceptionFilter` 直接终止进程 | 不再残留无法 `taskkill` 的进程 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `ENGINE_RELEASE_WAIT` | 无 | `200` ms | `src/app/mod.rs` | 关闭时等待音频/后台线程释放引擎引用 |
| `CLOSE_SYNC_TIMEOUT` | 无 | `5` 秒 | `src/app/transport.rs` | 加载中关闭时等待后台插件同步的上限；超时强制关闭并放弃引擎 |
| `SCROLL_ROW_STEP` | 无 | `60` 点 | `src/app/shortcuts.rs` | 方向键垂直滑动编曲区 |
| `SCROLL_PITCH_STEP` | 无 | `2` 半音 | `src/app/shortcuts.rs` | 方向键垂直滑动钢琴卷帘 |
| 默认快捷键 | 固定列表 | 29 个可配置动作 | `src/app/shortcuts.rs` | 新增 Home/End、滑动方向键、控制器画笔 Ctrl+1~4 与 4 循环 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 2.1、2.2、6.6、7.10、7.13、9.1、9.2 |
| `doc/关键常量.md` | 是 | 第 4、5 节新增行，快捷键行重写 |
| `doc/框架工作流.md` | 是 | 1.1、1.5、1.6、1.7、模块地图与函数索引 |
| `doc/项目简介.md` | 是 | 2.4 插件、2.6 工作流与快捷键 |
| `doc/代码规范.md` | 是 | 7.4、13.5 快捷键注册规范 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 `cargo fmt --all` |
| `cargo check --all-targets` | 通过 | 无编译错误 |
| `cargo test` | 通过 | `cargo test --lib`：54 passed / 0 failed / 14 ignored（新增 4 个快捷键测试） |
| 手工验证（启动/退出） | 通过 | `cargo run` 正常启动，无异常输出；通过 `PostMessage(WM_CLOSE)` 关闭主页窗口后 PID 8352 完整退出，进程列表中没有新增僵尸进程 |
| 手工验证（插件） | 未执行 | 需要本机 VOCALOID 6、VST 乐器与 ASIO 驱动，未在真实插件上复测关闭/打开 GUI |
| 残留进程排查 | 完成 | 现存 5 个 `surfing-studio.exe` 均为 0 线程 / 0 句柄的已终止僵尸进程（最早 2026-10-23），无法被 `taskkill` / `Stop-Process` 结束；新版本安装异常过滤器后不再让崩溃实例进入 Windows 错误报告 |

## 8. 风险与后续

- 风险：加载含 VST 乐器的工程时，乐器实例在 UI 线程补载会让加载末尾出现一次短暂阻塞；该阶段仍在“正在加载插件…”提示内，且音频未开始，不会造成播放欠载。
- 风险：加载/插件同步期间关闭工程会等待后台同步结束（显示“正在停止加载…”），最长 5 秒；超时强制关闭会泄漏一次插件实例，但不会卡死窗口。
- 风险：`SurfingApp::drop` 最多等待 200 ms 让引擎引用释放；若音频回调恰好在处理大 block，引擎仍可能最后由回调线程释放。
- 风险：本次排查时机器上已有的 5 个僵尸进程（0 线程 / 0 句柄）是更早版本留下的已终止进程，句柄由其他进程/系统持有，代码无法结束；注销或重启后消失。
- 后续：在装有 VOCALOID 6 与 ASIO 的真实环境中复测打开/关闭工程与播放中打开 GUI；如仍有个别插件在 UI 线程析构挂起，可再评估按格式区分析构线程。
