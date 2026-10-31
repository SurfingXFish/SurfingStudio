# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-31 19:39:43 (Asia/Shanghai) |
| AI 工具/模型 | TraeCode / 未标注模型 |
| 任务来源 | 1.1.3a 实测 VOCALOID 单实例问题仍未解决，依据用户提供的 `.sflog` 定位 |
| 关联分支/提交 | `feature/audio-import-surround-gpu-export` / 待提交 |
| 影响模块 | app, docs |
| 关联记录 | `20261031-192840-vocaloid-reload-and-export-retry.md` |

## 2. 日志分析结论

用户日志 `surfing-20261031-193153-p9160`（版本 1.1.3a）显示：

- 首次打开工程（无旧插件）时 `phase=plugin_windows_closed windows=7 all_closed=true`——把 winit/egui 的**隐藏辅助窗口**误判为插件窗口。
- 关闭工程时 `windows=14 all_closed=false`，随后 `phase=plugin_windows_still_open`，**跳过了 `terminate`**；VOCALOID 从未终止，模块未卸载，单实例锁未释放。
- 再次打开工程时重复 `windows=1 all_closed=false`，继续跳过终止，最终仍提示 VOCALOID 已在运行。

根因：`close_plugin_owned_windows` 在上一版移除了 `IsWindowVisible` 过滤。winit/egui 的隐藏辅助窗口不响应 `WM_CLOSE`，导致 `all_closed` 永远为 `false`，从而跳过整段插件终止逻辑。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/mod.rs` | 修改 | `close_plugin_owned_windows` 恢复只枚举可见窗口；新增 `phase=plugin_windows_visible` 日志输出被识别窗口的标题，便于后续定位；隐藏辅助窗口不再计入 `all_closed` |
| `doc/技术文档.md` | 修改 | 9.2 明确只枚举可见窗口，并说明隐藏窗口导致跳过 terminate 的后果 |
| `doc/框架工作流.md` | 修改 | 1.10 关闭工程流程同步 |
| `doc/日志规范.md` | 修改 | `window.drop` 新增 `plugin_windows_visible` 与 `titles` |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 插件窗口枚举 | 含隐藏 winit/egui 辅助窗口 | 仅可见窗口 | `all_closed` 能正确反映插件窗口状态 |
| 关闭工程 | 隐藏窗口导致跳过 terminate | VOCALOID6 Editor 关闭后正常 terminate/卸载模块 | 单实例锁释放，可重新打开工程 |
| 诊断 | 只记录窗口数量 | 记录窗口标题 | 下一次可直接确认误判/残留窗口 |

## 5. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 9.2 |
| `doc/框架工作流.md` | 是 | 1.10 |
| `doc/日志规范.md` | 是 | `window.drop` 阶段与字段 |

## 6. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行 |
| `cargo check --all-targets -p surfing-studio` | 通过 | 无警告 |
| `cargo test --lib` | 通过 | 73 通过，0 失败，16 忽略 |
| 实测日志复核 | 通过（定位） | 确认 1.1.3a 因隐藏窗口导致 `all_closed=false` 并跳过 terminate |
| 修复后实测 | 待用户确认 | 关闭含 VOCALOID 的工程后重新打开，应不再提示单实例冲突；本次修复未改动导出逻辑 |

## 7. 风险与后续

- 若插件窗口本身拒绝关闭，仍会按策略跳过 `terminate`（避免 UI 卡死）；此时 `plugin_windows_visible` / `plugin_windows_still_open` 会留下窗口标题，便于判断是哪一个插件。
- 若隐藏窗口仍有干扰，可进一步按窗口类名（winit/egui）过滤；当前可见性过滤已覆盖本次日志中的 7/14 个误判窗口。
