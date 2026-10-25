# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-21 22:49:58 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 内嵌 GUI 实际不可见；后台识别插件类型期间选择插件卡死 |
| 关联分支/提交 | 未知 |
| 影响模块 | app / ui / vendor/vst3-host / docs |
| 关联记录 | `history/ai/20261021-223500-plugin-switch-close-inline-editor.md` |

## 2. 任务目标

- 修复效果器卷帘内嵌 GUI 实际不可见的问题（用户反馈“未实现”）。
- 修复插件类型后台识别进行时，选择 VST 插件导致界面卡死的问题。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `vendor/vst3-host/src/window.rs` | 修改 | 内嵌编辑器由 `WS_CHILD` 子窗口改为 `WS_POPUP | WS_EX_TOOLWINDOW` 无边框顶层窗口（owner = 主窗口）；新增 `embedded_parent` 字段；`set_embedded_bounds` 先把宿主客户区坐标 `ClientToScreen` 再 `SetWindowPos`；`close` 重置 owner |
| `src/app/plugins.rs` | 修改 | `apply_plugin` 不再同步 `sync_engine`：写入槽位后置 `needs_sync + sync_in_background`，由后台线程创建实例；新增 `PendingPluginApply` 与 `finish_plugin_apply` 在同步完成后读取名称/类型、写回实例状态；新增 `cancel_plugin_probes`，`apply_plugin` / `start_scan_with` 会先取消后台识别 |
| `src/app/mod.rs` | 修改 | 新增 `pending_plugin_apply` / `plugin_probe_cancel` 字段与初始化 |
| `src/app/transport.rs` | 修改 | `poll_engine_sync` 在同步成功后调用 `finish_plugin_apply`，失败时清空待处理项 |
| `src/app/files.rs` | 修改 | 新建/切换/应用工程时清空 `pending_plugin_apply` |
| `src/plugin/mod.rs` | 修改 | 更新内嵌诊断测试（按窗口类查找附属窗口）；新增 `pro_q_editor_stays_visible_inside_an_eframe_window`：在真实 eframe/OpenGL 窗口中内嵌 Pro-Q 并截屏统计颜色数 |
| `doc/技术文档.md` | 修改 | 6.6 内嵌窗口改为顶层附属窗口并记录 OpenGL 覆盖原因；6.7 本地 patch |
| `doc/框架工作流.md` | 修改 | 1.7 插件加载改为后台同步 + `finish_plugin_apply`；索引补充取消识别 |
| `doc/项目简介.md` | 修改 | 2.4 内嵌窗口与后台加载说明 |
| `doc/代码规范.md` | 修改 | 8.2 明确不得改回 `WS_CHILD` |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 内嵌 GUI 窗口类型 | `WS_CHILD` 子窗口 | 主窗口拥有的 `WS_POPUP` 无边框顶层窗口 | 不再被 OpenGL 父窗口整块擦除；实测插件区域颜色数 1 → 1644 |
| 内嵌定位 | 父窗口客户区坐标 | 客户区坐标 → `ClientToScreen` → 屏幕坐标 | 顶层窗口可以正确贴在卡片上 |
| 选择插件 | UI 线程同步 `sync_engine`，加载期间界面冻结 | 提交后台同步，UI 显示“正在加载插件…”；完成后 `finish_plugin_apply` 收尾 | 加载慢插件/扫描期间不再卡死 |
| 后台类型识别 | 一直跑到队列结束 | 插件加载或重新扫描时置取消标志并退出 | 及时让出 Windows 加载器锁 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无 | - | - | - | 本次未新增全局常量 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.6、6.7 |
| `doc/关键常量.md` | 否 | 无新增常量 |
| `doc/框架工作流.md` | 是 | 1.7、3.x 索引 |
| `doc/项目简介.md` | 是 | 2.4 |
| `doc/代码规范.md` | 是 | 8.2 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 已运行 |
| `cargo check --all-targets` | 通过 | 无错误 |
| `cargo test` | 通过 | 24 passed / 0 failed / 11 ignored |
| `pro_q_editor_embeds_in_a_host_child_window` | 通过 | 附属窗口位置 (40,40)、尺寸 500×300、显隐正确 |
| `pro_q_editor_stays_visible_inside_an_eframe_window` | 通过 | 屏幕截图中插件区域 1644 种颜色（`WS_CHILD` 版本仅 1 种） |

## 8. 风险与后续

- 风险：内嵌 GUI 现在是顶层附属窗口，会盖在主窗口之上；egui 弹出的对话框若与其重叠，会被插件窗口遮住。后续可在打开模态窗口时临时隐藏内嵌 GUI。
- 风险：主窗口移动/缩放时依赖每帧重新定位，拖动过程中可能出现一帧延迟。
- 风险：取消后台识别后，未识别的插件会保持“未知”，需要重新扫描才能补齐。
- 后续：在真实工程里验证内嵌 Pro-Q 频谱刷新、横向滚动裁剪、切换工程与底部面板收起/展开。
