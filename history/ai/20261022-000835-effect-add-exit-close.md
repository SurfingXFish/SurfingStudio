# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-22 00:08:35 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 播放中新增 VST 仍卡死；关闭软件时独立 GUI 不关闭并卡死；删除独立窗口按钮 |
| 关联分支/提交 | 未知 |
| 影响模块 | app / ui / audio/engine / plugin / vendor/vst3-host / docs |
| 关联记录 | `history/ai/20261021-235132-plugin-ui-thread-instance-gui-only.md` |

## 2. 任务目标

- 消除播放中新增 VST 效果器的卡顿。
- 关闭软件时立即关闭独立/内嵌 GUI，并让进程正常退出。
- 删除卡片头部的「独立」按钮，保留双击标题打开独立窗口。

## 3. 根因

1. `TrackEngine::ensure_effects` 在插件键列表变化时**清空并重载整条效果链**；新增一个效果器会把该轨道所有插件都重新创建，界面冻结数秒。
2. 关闭应用时 `Drop` 只是 `mem::forget` 编辑器窗口：GUI 留在屏幕上，且进程退出时插件 DLL 的 `DllMain(DETACH)` 等待已停止的 UI 线程，`run_native` 已返回但进程永不结束（实测 25 s 未退出）。

## 4. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/audio/engine/mod.rs` | 修改 | `ensure_effects` 改为按插件键增量复用运行时：只加载新增的效果器，复用其余实例（实测新增 Pro-Q 45 ms、新增 Auto-Tune 55 ms） |
| `src/ui/effects.rs` | 修改 | 删除卡片头部「独立」按钮；标题悬停提示“双击在独立窗口打开”；徽章让出空间给关闭按钮 |
| `src/main.rs` | 修改 | `run_native` 返回后调用 `TerminateProcess`，跳过插件 DLL 的正常卸载，避免退出卡死 |
| `src/app/mod.rs` | 修改 | 收到关闭请求时立即销毁编辑器原生窗口；`Drop` 再兜底销毁并 `mem::forget`，避免 `removed()` 阻塞 |
| `src/app/plugins.rs` | 修改 | 新增 `EditorWindow::destroy_for_exit` |
| `src/plugin/editor.rs` | 修改 | 新增 `Vst2EditorWindow::destroy_for_exit`（隐藏 + DestroyWindow，不调用编辑器 close） |
| `vendor/vst3-host/src/window.rs` | 修改 | 新增 `PluginWindow::destroy_native_window`（隐藏 + DestroyWindow，不 detach 视图） |
| `Cargo.toml` | 修改 | winapi features 增加 `processthreadsapi` |
| `doc/技术文档.md` | 修改 | 5.1 增量复用；6.6 双击说明；7.5/7.6 移除「独立」按钮 |
| `doc/框架工作流.md` | 修改 | `ensure_effects` 增量复用、`Drop`/`TerminateProcess` 退出流程 |
| `doc/项目简介.md` | 修改 | 2.4 双击打开 |
| `doc/代码规范.md` | 修改 | 退出流程、增量复用、独立窗口入口 |

## 5. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 新增效果器 | 重载该轨道全部效果器（数秒） | 只加载新增的插件（约 45–55 ms） | 播放中新增不再卡顿 |
| 关闭应用（有 GUI） | 窗口留在屏幕，进程不退出 | Drop 先销毁编辑器窗口；`main` 直接 `TerminateProcess` | GUI 立即消失，进程退出 |
| 卡片头部 | 「独立」按钮 + 双击 | 仅双击标题；悬停提示 | 头部更简洁 |

## 6. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无 | - | - | - | 本次未新增全局常量 |

## 7. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 5.1、6.6、7.5、7.6 |
| `doc/关键常量.md` | 否 | 无新增常量 |
| `doc/框架工作流.md` | 是 | `app/plugins.rs`、引擎与退出流程 |
| `doc/项目简介.md` | 是 | 2.4 |
| `doc/代码规范.md` | 是 | 8.2 |

## 8. 验证（真实应用流程，临时诊断代码已删除）

| 场景 | 结果 |
| --- | --- |
| 播放中新增 Pro-Q 4 | `apply_plugin` 45 ms，播放继续，卷帘自动内嵌 |
| 播放中在已有 Pro-Q 的轨道新增 Auto-Tune Pro | 55 ms，播放继续，两个 GUI 均自动内嵌（editors=2） |
| 打开独立 Auto-Tune GUI 后发送 `ViewportCommand::Close` | 修复前 25 s 不退出；修复后进程立即退出，GUI 随窗口销毁 |
| 关闭流程日志 | `drop start → destroy editor → editors destroyed → drop end → run_native returned` 后进程结束 |

| 项目 | 结果 |
| --- | --- |
| `cargo fmt --all` | 通过 |
| `cargo check --all-targets` | 通过 |
| `cargo test` | 24 passed / 0 failed / 11 ignored |

## 9. 风险与后续

- 风险：`TerminateProcess` 跳过静态对象析构；工程保存已在 `Drop` 中等待最多 3 秒，音频流与撤销栈本就故意泄漏，符合现有退出策略。
- 风险：`destroy_native_window` 不调用插件 `removed()`，仅适用于进程退出阶段。
- 后续：继续用大工程验证多次新增/删除效果器、主输出效果器与退出。
