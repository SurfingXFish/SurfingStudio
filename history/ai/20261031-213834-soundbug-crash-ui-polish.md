# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-31 21:38:34 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 修复 SoundBug 工程崩溃、钢琴卷帘缩放/力度、自动翻页、Ctrl+A、乐器窗口留白、红色边框闪烁、卷帘误拖出与 Pro-Q 预设二级菜单 |
| 关联分支/提交 | test / 未知 |
| 影响模块 | core, plugin, app, ui, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 打开 SoundBug 工程（Ample Guitar SC 乐器）时不再因插件状态恢复崩溃。
- 钢琴卷帘首次打开的水平缩放与编曲区可见拍数一致。
- 工具栏增加自动翻页按钮与快捷键，播放时追踪播放头。
- 修正乐器选择窗口顶栏上下留白不对称。
- 增加 Ctrl+A 全选快捷键。
- 力度控制器圆点后方的横线可直接上下拖动调整力度。
- 关闭 debug 构建中 egui 的红色边框调试叠加。
- 拖出独立卷帘只允许从顶部工具条触发，画布框选不再误触。
- 修复 Pro-Q 4 VST3 预设浏览器二级菜单无法打开的问题。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/core/project_runtime/soundbug/convert.rs` | 修改 | 导入的 SoundBug 插件状态只保留、不自动恢复（`restore_state = false`），移除 `state_is_binary` 字段与参数；避免 Ample Guitar SC 在 `load_state` 内崩溃 |
| `src/plugin/runtime.rs` | 修改 | VST3 `restore_state` 只接受带 `VST3HOST_STATE\0\0` 宿主头的状态块；补齐函数头 |
| `src/plugin/mod.rs` | 修改 | 忽略测试改名为 `skips_unverified_soundbug_state_in_a_vst3`，覆盖导入状态被安全跳过 |
| `src/app/plugins.rs` | 修改 | VST3 独立编辑器改用 `PluginWindow::open()`（无所有者），避免 FabFilter 预设二级菜单被同级 owned 窗口盖住；生命周期仍由 `SurfingApp::drop` 显式销毁 |
| `src/app/edit.rs` | 修改 | 新增 `select_all`：按上下文全选控制器点位、卷帘音符、效果器链或编曲区片段；补齐双语文件头 |
| `src/app/shortcuts.rs` | 修改 | 新增 `SelectAll`（Ctrl+A）与 `ToggleAutoPage`（F10）动作、id、名称、分组与默认绑定 |
| `src/app/input.rs` | 修改 | 处理 Ctrl+A 与自动翻页快捷键 |
| `src/app/mod.rs` | 修改 | 新增 `auto_page` UI 状态（默认关闭，不持久化） |
| `src/app/panels.rs` | 修改 | 工具栏加入「自动翻页」开关按钮与提示；更新文件头 |
| `src/app/rolls.rs` | 修改 | 删除 `try_detach_roll` 长按拖出实现，只保留工具条拖出 |
| `src/ui/arrangement/mod.rs` | 修改 | 自动翻页：播放头越过右侧 90% 或回到视口左侧时翻页 |
| `src/ui/controller.rs` / `src/ui/effects.rs` | 修改 | 移除画布长按拖出，保留工具条拖出 |
| `src/ui/piano_roll/mod.rs` | 修改 | 首次打开按编曲区 `visible_beats` 计算缩放；力度横线可上下拖动（`drag_started` 用按下点命中，避免越过拖动阈值后错过 4 点容差）；自动翻页；移除画布拖出；`PianoDrag::Velocity` 锁定拖动目标 |
| `src/ui/piano_roll/helpers.rs` | 修改 | 新增 `VELOCITY_BAR_HIT_W = 4.0` |
| `src/ui/mod.rs` | 修改 | `PianoDrag` 新增 `Velocity { clip, note }` |
| `src/ui/instrument_picker.rs` | 修改 | 去掉顶栏与分隔线之间多余的 6 点空白，使上下留白对称 |
| `src/ui/theme.rs` | 修改 | debug 构建关闭 `warn_if_rect_changes_id` 与 `show_unaligned`，消除页面/按钮刷新时的红色边框 |
| `src/ui/help.rs` | 修改 | 帮助窗口说明更新：只用工具条拖出、Ctrl+A 全选、F10 自动翻页 |
| `doc/项目简介.md` | 修改 | 同步钢琴卷帘缩放/力度横线、自动翻页、快捷键、卷帘拖出、插件窗口所有者与 SoundBug 状态限制 |
| `doc/技术文档.md` | 修改 | 同步状态恢复白名单、插件窗口无所有者、力度横线、自动翻页、Ctrl+A、debug 叠加、拖动入口与 SoundBug 导入行为 |
| `doc/关键常量.md` | 修改 | 新增 `VELOCITY_BAR_HIT_W`、`VST3_STATE_MAGIC`、自动翻页阈值；更新 VST3 窗口所有者、快捷键与钢琴卷帘缩放说明 |
| `doc/框架工作流.md` | 修改 | 更新 `open_editor`、`select_all`、`main_toolbar`、卷帘拖出、`ui_piano_roll_for` 职责 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| SoundBug 插件状态 | 二进制状态标记为可恢复并调用 `load_state` | 所有导入状态只保留、不自动恢复；`restore_state` 只接受宿主头状态块 | 打开 Ample Sound 等 SoundBug 工程不再崩溃；导入后插件回到默认音色 |
| VST3 独立编辑器窗口 | `PluginWindow::open_with_owner(主窗口)` | `PluginWindow::open()`（无所有者） | FabFilter 预设二级菜单可显示；窗口仍由 `SurfingApp::drop` 显式销毁 |
| 钢琴卷帘首次打开 | 固定 `zoom = 2.0` | 按编曲区 `visible_beats` 计算 `zoom` | 两个页面时间跨度一致 |
| 卷帘拖出 | 工具条拖拽 + 画布长按拖拽 | 仅工具条拖拽 | 大范围框选不再跳出独立窗口 |
| 力度编辑 | 仅竖线命中 | 竖线与横线都可拖动，按下后锁定音符 | 横线上也能直接上下调整力度 |
| 全选 | 无 | Ctrl+A 按上下文全选 | 编辑更快 |
| 播放跟随 | 无自动跟随 | 自动翻页（F10/按钮）在 90% 处翻页 | 长工程播放时视图持续跟随 |
| debug 红色边框 | egui 默认绘制 id 变化警告 | 主题初始化关闭调试叠加 | 页面/按钮刷新不再闪红框 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `VELOCITY_BAR_HIT_W` | 无 | `4.0` 点 | `src/ui/piano_roll/helpers.rs` | 力度横线纵向命中容差 |
| `VST3_STATE_MAGIC` | 隐式（vendor 内部） | `VST3HOST_STATE\0\0` | `src/plugin/runtime.rs` | 只恢复本宿主写出的状态块 |
| 自动翻页阈值 | 无 | 视口右侧 `90%` | `src/ui/arrangement/mod.rs`、`src/ui/piano_roll/mod.rs` | 越过阈值翻页，左侧保留 10% 上下文 |
| 默认快捷键 | 无 Ctrl+A / F10 | `Ctrl+A` / `F10` | `src/app/shortcuts.rs` | 全选与自动翻页 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.4/6.6 状态恢复与编辑窗口；7.2 自动翻页；7.3 力度横线/缩放；7.6 拖出；7.9 debug 叠加；7.11 乐器窗口留白；7.13 Ctrl+A/F10；9.3 SoundBug 导入；11/12 测试与限制 |
| `doc/关键常量.md` | 是 | 5 快捷键、6 插件窗口与状态、7 钢琴卷帘常量与缩放、自动翻页阈值 |
| `doc/框架工作流.md` | 是 | `app/edit.rs` 的 `select_all`；`app/plugins.rs` 的 `open_editor`/`main_window_handle`；`app/rolls.rs` 拖出；`main_toolbar`；`ui_piano_roll_for` |
| `doc/项目简介.md` | 是 | 2.2 力度与缩放；2.4 插件窗口所有者；2.6 快捷键/自动翻页/卷帘拖出；7 已知限制 |
| `doc/代码规范.md` | 否 | 未改变编码、分层或命名规则 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行 |
| `cargo test` | 通过 | 73 passed，16 ignored |
| 手工验证 | 通过 | ① 复现 SoundBug 工程 `NewProject33` 崩溃：Ample Guitar SC 在 `restore_state` 内 0xC0000005；修复后以同一状态块调用 `restore_state` 正常返回。② 应用内逐项截图：钢琴卷帘首次打开与编曲区标尺同为 0–7 拍；力度横线自 100 拖到 20 后再次拖动横线 20→52；`Ctrl+A` 空工程提示「没有可全选的内容」、有片段时「已全选 1 个片段」；`F10` 状态栏「自动翻页: 开」且播放头越过 90% 后编曲区与卷帘标尺一起从第 6 拍翻页；大跨度框选后窗口列表仍只有主窗口（未误拖出）；连续切换按钮 24 帧截图中无纯红像素（无红色边框）；乐器选择窗口顶栏上下留白对称。③ Pro-Q 4 在完整应用内以无所有者窗口打开，预设浏览器可展开类别二级菜单（自动化鼠标悬停对 FabFilter 无效，改用方向键 → 选中 Drums 后按右键，右侧滑出预设列表，说明二级菜单链路正常）。 |

## 8. 风险与后续

- 风险：SoundBug 导入的插件状态不再自动恢复，导入后需要重新选择音色；这是避免第三方插件在非标准状态上崩溃的取舍。
- 风险：VST3 独立编辑器不再由 Windows 所有者关系兜底销毁，依赖 `SurfingApp::drop` / `close_all_editors` 的显式清理路径。
- 后续：如需把 FabFilter 菜单的鼠标悬停行为纳入自动化回归，需要真实鼠标或更低层的注入；自动化已验证菜单键盘导航与二级菜单展开链路可用。
