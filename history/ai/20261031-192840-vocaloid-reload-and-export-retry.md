# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-31 19:28:40 (Asia/Shanghai) |
| AI 工具/模型 | TraeCode / 未标注模型 |
| 任务来源 | VOCALOID 窗口已能关闭，但重新打开工程提示单实例冲突；导出失败后再次导出被“已有导出任务”阻塞 |
| 关联分支/提交 | `feature/audio-import-surround-gpu-export` / 待提交 |
| 影响模块 | audio, plugin, docs |
| 关联记录 | `20261031-191140-vocaloid-window-close.md` |

## 2. 任务目标

- 关闭工程后能再次加载 VOCALOID VSTi（释放进程级单实例锁）。
- 修复导出失败后会话残留，导致后续导出被“已有导出任务正在进行”阻塞。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/runtime.rs` | 修改 | `PluginRuntime::shutdown` 终止组件后不再永久泄漏实例：把最后一份 `Arc<Plugin>` 交给后台线程释放；`PluginImpl::Drop` 在 `terminated` 时跳过 teardown，只释放 COM 引用并 `FreeLibrary` 卸载 VST3 模块，从而释放 VOCALOID / ACE Bridge 的进程级单实例锁；对这两个插件最多等待 800 ms 卸载完成 |
| `src/audio/engine/render.rs` | 修改 | `OfflineRenderSession::progress` 改用 `f64` 计算并在未完成时封顶 `0.9999`；新增 `Engine::offline_render_finished()`；新增长工程进度回归测试，并在逐帧导出测试中断言完成标志 |
| `src/app/export.rs` | 修改 | `step_export_render` 以 `offline_render_finished()` 判断完成，不再用 `progress >= 0.999`；`finish_offline_render` 失败时调用 `cancel_offline_render` 清理会话，状态栏进度封顶 99% |
| `doc/技术文档.md` | 修改 | 9.2 已终止实例由后台线程释放/卸载模块；9.4 导出完成判定与失败清理；12.3 关闭窗口后的引擎外壳说明 |
| `doc/框架工作流.md` | 修改 | 1.10/1.11 与 `SurfingApp::drop` 说明 |
| `doc/项目简介.md` | 修改 | 关闭工程后的插件释放与重新打开说明 |
| `doc/代码规范.md` | 修改 | 已终止实例必须释放并卸载模块，不能永久泄漏 |
| `doc/日志规范.md` | 修改 | `engine_retained` 含义更新为“引擎外壳保留” |
| `doc/关键常量.md` | 修改 | 新增 VOCALOID/ACE 卸载等待 800 ms 与导出完成判定说明 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 关闭工程后的插件实例 | 终止后永久泄漏，VST3 模块不卸载 | 后台线程释放，`FreeLibrary` 卸载模块 | VOCALOID 单实例锁释放，可重新打开工程 |
| 导出完成判定 | 进度 ≥ 0.999 | 会话 `finished` 标志 | 长工程不会提前判完成 |
| 导出失败清理 | `finish` 失败只清 job/busy | 同时 `cancel_offline_render` | 再次导出不再报“已有导出任务正在进行” |
| 导出进度 | 可能显示 100% 但仍在渲染 | 未完成封顶 99%（内部 0.9999） | 进度与状态一致 |

## 5. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 9.2、9.4、12.3 |
| `doc/框架工作流.md` | 是 | 1.10、1.11、`SurfingApp::drop` |
| `doc/项目简介.md` | 是 | 2.6 关闭工程窗口 |
| `doc/代码规范.md` | 是 | 8.2 生命周期 |
| `doc/日志规范.md` | 是 | `window.drop` / 排查表 |
| `doc/关键常量.md` | 是 | 插件卸载等待、导出完成判定 |

## 6. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行 |
| `cargo check --all-targets -p surfing-studio` | 通过 | 无警告 |
| `cargo test --lib` | 通过 | 73 通过，0 失败，16 忽略（新增 `progress_stays_below_one_until_finished`） |
| 手工验证 | 待用户确认 | 关闭含 VOCALOID 的工程后重新打开应可再次拉起 VOCALOID6 Editor；长工程导出失败后可直接重试 |

## 7. 风险与后续

- 风险：已终止实例在后台线程释放；若插件模块的 `DllMain(DETACH)` 卡住，只影响该后台线程（UI 不阻塞），但模块会保持加载；VOCALOID / ACE Bridge 最多等待 800 ms。
- 风险：若仍有宿主编辑器窗口持有 `Arc<Plugin>`，后台线程不是最后一个引用，模块不会立即卸载；正常关闭流程已先清空编辑器。
- 后续：为插件卸载增加“卸载完成/超时”诊断日志；若仍出现单实例冲突，可在重新加载前显式等待卸载完成或重试一次。
