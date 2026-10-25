# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-21 19:39:58 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 重构顶部菜单栏：去掉 Windows 原生标题栏，参考 IDEA 改为「应用栏 + 工具栏」两层 |
| 关联分支/提交 | 未知 |
| 影响模块 | ui, app, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 关闭系统标题栏，使用应用内自绘的两层顶部栏。
- 上层应用栏：左侧软件图标、隐藏下拉栏、项目栏（工程名 + 打开/新建工程）；右侧关闭、最大化、最小化、设置。
- 点击隐藏下拉栏后，在左上角显示文件、编辑、VST 插件（管理 VST 插件）、帮助。
- 下层工具栏集中放置节拍、循环起止、鼠标模式等编辑控件。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/main.rs` | 修改 | `ViewportBuilder` 增加 `with_decorations(false)`，由应用自绘窗口边框与按钮 |
| `src/app/panels.rs` | 重写 | 将原 `top_bar` 拆分为 `app_bar` 与 `main_toolbar`；新增 `ui_main_menu`、`ui_settings_menu`、`window_resize_edges` 及自绘控件 `app_logo` / `hamburger_button` / `bar_text_button` / `project_chip` / `window_control`；新增图标绘制函数 `paint_minimize` / `paint_maximize` / `paint_close` / `paint_chevron`；应用栏整条作为拖动热区并支持双击最大化 |
| `src/app/mod.rs` | 修改 | 新增 `menu_bar_visible`、`window_maximized`、`window_restore_rect`（仅 UI 状态）；每帧改为调用 `app_bar` + `main_toolbar`，并在中央面板之后调用 `window_resize_edges` |
| `src/app/window_chrome.rs` | 新增 | 无边框窗口最大化/还原：Windows 下读取显示器工作区并手动设置位置与尺寸，避免 DWM 外扩 8 像素裁切应用栏；其他平台沿用 `ViewportCommand::Maximized` |
| `src/ui/theme.rs` | 修改 | 新增应用栏配色 `APP_BAR` / `APP_BAR_HOVER` / `CLOSE_HOVER` |
| `src/ui/help.rs` | 修改 | 帮助窗口补充应用栏、窗口拖动与缩放的说明 |
| `doc/技术文档.md` | 修改 | 1.2 模块表与 7.1 每帧布局更新，补充无边框窗口与 `window_resize_edges` 说明 |
| `doc/关键常量.md` | 修改 | 顶部高度常量更新为应用栏 36 点 / 工具栏 46 点，新增按钮宽度、项目名截断、缩放热区与新主题色 |
| `doc/框架工作流.md` | 修改 | 每帧流程图与 `app/panels.rs` 函数索引更新 |
| `doc/项目简介.md` | 修改 | 2.5 工作流补充自绘应用栏与工具栏的用户可见行为 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 窗口标题栏 | Windows 原生标题栏 | 自绘应用栏（图标、菜单、项目栏、设置、窗口按钮） | 外观统一为 IDEA 风格；窗口控制通过 `ViewportCommand` 驱动 |
| 顶部布局 | 单条 62 点工具条，文件/轨道/插件/走带混排 | 上层应用栏 + 下层工具栏 | 菜单与高频编辑控件分离，工具栏专注走带/节拍/循环/鼠标模式 |
| 主菜单入口 | 顶栏「文件 / 轨道 / 插件 / 帮助」 | 隐藏下拉栏展开后显示「文件 / 编辑 / VST 插件 / 帮助」 | 编辑命令（撤销/重做/复制/粘贴/删除）可在菜单中执行 |
| 添加轨道 | 顶栏「轨道」菜单 | 工具栏「+ 轨道」菜单 | 保留原有三种轨道创建命令 |
| 窗口拖动/缩放 | 由系统标题栏提供 | 应用栏拖动 + 双击最大化；`window_resize_edges` 提供边缘/角落缩放 | 无系统边框后仍可移动与缩放窗口 |
| 窗口最大化 | winit `ViewportCommand::Maximized` | Windows 下按显示器工作区手动设置尺寸/位置（`window_chrome.rs`），其他平台仍用 winit | 避免 DWM 把无边框窗口外扩 8 像素导致应用栏与窗口按钮被裁切 |
| 工程入口 | 文件菜单中的新建/打开 | 文件菜单 + 应用栏项目栏下拉 | 工程名与未保存状态常驻可见 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 顶部工具条最小高度 | `62` 点 | 删除 | `src/app/panels.rs` | 拆分为应用栏与工具栏 |
| `APP_BAR_H` | 无 | `36` 点 | `src/app/panels.rs` | 应用栏高度，窗口按钮与拖动热区 |
| `TOOLBAR_MIN_H` | 无 | `46` 点 | `src/app/panels.rs` | 工具栏最小高度，窄窗口自动换行 |
| `WINDOW_BTN_W` | 无 | `46` 点 | `src/app/panels.rs` | 窗口按钮宽度 |
| `PROJECT_NAME_MAX_W` | 无 | `210` 点 | `src/app/panels.rs` | 项目名截断宽度 |
| `RESIZE_EDGE_W` / `RESIZE_CORNER_W` | 无 | `4` / `10` 点 | `src/app/panels.rs` | 无边框窗口缩放热区 |
| `APP_BAR` / `APP_BAR_HOVER` / `CLOSE_HOVER` | 无 | `(18,24,31)` / `(33,43,53)` / `(196,62,62)` | `src/ui/theme.rs` | 应用栏底色与悬停色 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 模块表、7.1 每帧布局与无边框窗口说明 |
| `doc/关键常量.md` | 是 | 第 5 节窗口与顶部栏常量、第 8 节主题色 |
| `doc/框架工作流.md` | 是 | 1.1 每帧流程、3.4 `src/app/panels.rs` 函数索引 |
| `doc/项目简介.md` | 是 | 2.5 工作流 |
| `doc/代码规范.md` | 否 | 规范本身无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | `cargo test --lib`：23 passed / 0 failed / 5 ignored |
| 手工验证 | 通过 | `cargo run` 启动：应用栏、工具栏、项目栏、设置菜单、文件/编辑/VST 插件/帮助菜单均正常渲染；拖动应用栏窗口由 (338,338) 移到 (408,366)；拖右边缘宽度 1440→1494；最大化铺满工作区 (0,0,2560×1392) 且应用栏未被裁切，还原回 1440×900 |
| `cargo clippy --all-targets` | 通过 | 仓库既有的文档缩进类警告仍存在，本次新增代码无新增警告类别；已消除冗余闭包与多余 `return` |

## 8. 风险与后续

- 风险：无系统边框后，Windows 的 Aero Snap（悬停最大化按钮弹出贴靠布局）不可用；应用接管的“最大化”是铺满工作区，不是系统最大化状态。
- 风险：Windows 的 Win+↑ 等原生最大化仍由 winit 处理，DWM 会外扩 8 像素；若需要完全一致，需要额外拦截系统最大化命令。
- 风险：最大化时拖动应用栏不会自动还原窗口（避免与视口命令竞争），需要先点击还原。
- 风险：窗口缩放依赖 `ViewportCommand::BeginResize`，不同平台的窗口管理器支持程度需在 macOS / Linux 上验证。
- 后续：设置菜单目前只展示音频设备与 MIDI 状态，真正的可编辑设置项（主题、缩放、缓冲）可在后续任务中扩展。
