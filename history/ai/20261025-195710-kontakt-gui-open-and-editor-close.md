# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-25 19:57:10 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 内置编码模型 |
| 任务来源 | Kontakt 8 打开 GUI 卡死后失败；关闭工程后插件独立 GUI 未同步关闭 |
| 关联分支/提交 | 未知 / 未知 |
| 影响模块 | plugin, app, docs |
| 关联记录 | `history/ai/20261025-194500-kontakt8-load-sliced-instruments.md` |

## 2. 任务目标

- 打开 Kontakt 8 的 GUI 不再卡住约 26 秒后报 `Failed to get view size`，而是正常显示界面。
- 关闭工程窗口时，该工程打开的独立插件 GUI 一并关闭，不残留独立窗口。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `vendor/vst3-host/src/internal/plugin_impl.rs` | 修改 | `open_editor` 改为「创建视图 → 记录 attach 前尺寸（失败继续）→ setFrame → attached」，不再因 attach 前 `getSize` 失败中止；新增 `declared_view_size` 字段、`declared_editor_size()` 查询，`close_editor` 清空该字段 |
| `vendor/vst3-host/src/plugin.rs` | 修改 | `PluginInterface` 增加带默认实现的 `declared_editor_size`；`Plugin` 增加同名公开方法 |
| `vendor/vst3-host/src/window.rs` | 修改 | `PluginWindow::open` 不再预创建临时视图取尺寸，改为 `800 × 600` 占位后按 attach 后尺寸调整；新增 `open_with_owner`，独立窗口以宿主主窗口为所有者创建；macOS 同样在 attach 后取尺寸 |
| `src/plugin/editor.rs` | 修改 | `Vst2EditorWindow` 新增 `new_with_owner`，主窗口作为所有者创建窗口；文件头函数清单同步 |
| `src/app/plugins.rs` | 修改 | `open_editor` 用 `PluginWindow::open_with_owner`（owner 取 `main_window_handle`）；VST2 分支改用 `new_with_owner` |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 打开 VST3 编辑器 | 先 `get_editor_size()` 创建临时视图，再创建窗口、创建真实视图并 **attach 前** `getSize`，失败即报错 | 占位尺寸创建窗口 → 创建视图 →（记录 attach 前尺寸，失败继续）→ setFrame → attached → 按插件尺寸调整窗口 | Kontakt 8 可以打开 GUI；重型插件少创建一次视图，卡顿时间约减半 |
| 独立编辑器窗口归属 | 无所有者的顶层窗口 | 以宿主主窗口为 owner（`open_with_owner` / `new_with_owner`） | 主窗口销毁时系统一并销毁插件 GUI，关闭工程不再残留独立窗口 |
| Serum 2 归一化 | 以 `get_editor_size()` 的预探测结果为基准 | 以 attach 前在真实视图上取得的 `declared_editor_size()` 为基准，取不到时跳过归一化 | 行为保持一致，且不再多创建视图 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 编辑器窗口占位尺寸 | 无（使用预探测尺寸） | `800 × 600` 点 | `vendor/vst3-host/src/window.rs` | 显示前会按插件尺寸调整，占位大小不可见 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.7 本地 patch：编辑器打开顺序、占位尺寸、owner、`declared_editor_size` |
| `doc/关键常量.md` | 是 | 第 4 节：编辑器窗口样式补充 owner、新增占位尺寸行 |
| `doc/框架工作流.md` | 是 | `app/plugins.rs` 函数表：`open_editor` / `main_window_handle` |
| `doc/项目简介.md` | 是 | 2.4 插件：Kontakt GUI 打开方式、独立 GUI 随工程关闭 |
| `doc/代码规范.md` | 否 | 规范无变化 |
| `doc/日志规范.md` | 否 | 无新增/修改日志事件 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 无格式差异 |
| `cargo test --lib` | 通过 | 62 通过、16 个需要真实插件/硬件的测试忽略 |
| 手工验证 | 未执行 | 需要真实 Kontakt 8 GUI 与桌面交互；命令沙箱内 Kontakt 无法读取 `install_state`，不能代表桌面环境 |

## 8. 风险与后续

- 风险：以主窗口为 owner 后独立插件 GUI 会始终位于工程窗口之上（DAW 常见行为）；若 eframe 先销毁主窗口再执行 `SurfingApp::drop`，系统会先销毁编辑器窗口，此时插件视图不会再收到 `removed()`（与现有强制关闭路径一致）。
- 风险：Kontakt 8 首次创建编辑器视图仍需其自身初始化时间，打开 GUI 期间 UI 线程可能短暂停顿；宿主无法把单次 `createView` 拆开。
- 后续：在真实桌面环境复核 Kontakt 8 打开 GUI 的成功率、耗时，以及关闭工程后 GUI 是否立即消失。
