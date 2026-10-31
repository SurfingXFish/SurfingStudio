# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-31 19:51:08 (Asia/Shanghai) |
| AI 工具/模型 | TraeCode / 未标注模型 |
| 任务来源 | 1.1.4a 实测：VOCALOID 窗口能关闭，但再次打开工程时进程崩溃（`0xE06D7363`） |
| 关联分支/提交 | `feature/audio-import-surround-gpu-export` / 待提交 |
| 影响模块 | audio, plugin, app, docs |
| 关联记录 | `20261031-193943-visible-plugin-windows.md` |

## 2. 日志分析（1.1.4a / pid 21812）

- 关闭工程：
  - `phase=plugin_windows_visible titles=VOCALOID6 Editor`
  - `phase=plugin_windows_closed windows=1 all_closed=true`
  - `phase=plugins_terminated`（约 1.1 s 后）
  - 内存从 3.6 GB 降到 1.2 GB
- 再次打开同一工程：
  - 乐器加载开始后约 4 s，`app.crash/unhandled_exception exception_code=0xE06D7363`（MSVC C++ 异常），进程终止。
- 插件字符串包含：`You can't use a VOCALOID VSTi/AU since another VOCALOID Editor/VSTi/AU is already running.`，并存在 `C:\Program Files\VOCALOID6\Editor\VOCALOID6.exe`。

结论：

1. 单实例检测的来源是 VOCALOID 拉起的外部编辑应用（VOCALOID6.exe），关闭进程内窗口不会结束该进程。
2. 上一版把所有已终止插件的模块交给后台线程释放（`FreeLibrary`）。部分插件不支持“卸载后在同一进程重新加载”，重开工程时抛出未处理 C++ 异常。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/runtime.rs` | 修改 | 回退“后台线程释放/卸载模块”：终止组件后仍泄漏实例、保持模块加载，避免插件析构死锁与卸载/重载崩溃 |
| `src/app/mod.rs` | 修改 | 新增 `terminate_plugin_child_processes`：枚举本进程的进程树，只结束由本软件拉起的编辑器应用（`VOCALOID6.exe` / `VOCALOID5.exe` / `VOCALOID4.exe` / `ACE Studio.exe` / `XStudio.exe` / `Synthesizer V.exe`）；在 `terminate_engine_plugins` 终止组件后调用，并写 `phase=plugin_editor_processes_terminated` |
| `src/audio/engine/mod.rs` | 修改 | 每个 VST 乐器加载前后写 `engine.instrument.load`（name/path/track/status）；若进程在加载中崩溃，最后一条 begin 即故障插件 |
| `doc/技术文档.md` | 修改 | 9.2 终止后不析构、保持模块加载；随后结束外部编辑进程；12.3 说明 |
| `doc/框架工作流.md` | 修改 | 1.10 / `SurfingApp::drop` |
| `doc/项目简介.md` | 修改 | 关闭工程窗口的插件处理 |
| `doc/代码规范.md` | 修改 | 终止后不得卸载模块；必须清理本进程树中的外部编辑器 |
| `doc/日志规范.md` | 修改 | 新增 `engine.instrument.load` 与 `plugin_editor_processes_terminated` |
| `doc/关键常量.md` | 修改 | 新增外部编辑进程清理清单 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 已终止插件实例 | 后台线程释放并卸载模块 | 保持泄漏、模块保持加载 | 避免卸载/重载型 C++ 异常 |
| VOCALOID 单实例来源 | 只关闭进程内窗口 | 同时结束本进程拉起的 VOCALOID6.exe 等编辑器 | 再次打开工程不再被判定为“已在运行” |
| 多实例安全 | 无 | 仅结束本进程树中的编辑器，不动用户单独打开的应用 | 避免误杀 |
| 乐器加载诊断 | 只有总数 | 每个插件 begin/end | 崩溃时可直接定位故障插件 |

## 5. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 9.2、12.3 |
| `doc/框架工作流.md` | 是 | 1.10、`SurfingApp::drop` |
| `doc/项目简介.md` | 是 | 2.6 关闭工程窗口 |
| `doc/代码规范.md` | 是 | 8.2 生命周期 |
| `doc/日志规范.md` | 是 | 5.2 / 5.5 |
| `doc/关键常量.md` | 是 | 外部编辑进程清单 |

## 6. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行 |
| `cargo check --all-targets --offline` | 通过 | 无警告 |
| `cargo test --lib --offline` | 通过 | 73 通过，0 失败，16 忽略 |
| 实测 | 待用户确认 | 关闭工程后检查日志应出现 `plugin_editor_processes_terminated names=VOCALOID6.exe`；再次打开不应崩溃 |

## 7. 风险与后续

- 若 VOCALOID6.exe 不是本进程的直接/间接子进程（例如由 COM 启动器代持），进程树匹配不会命中；下一次日志将不出现 `plugin_editor_processes_terminated`，届时可改为按“启动时间 + 安装路径”匹配。
- 已终止实例与模块继续保留，进程内存不会因关闭工程完全下降；这是为了稳定性，进程退出时由系统回收。
- 若仍崩溃，新的 `engine.instrument.load` 最后一条 begin 会指出具体插件。
