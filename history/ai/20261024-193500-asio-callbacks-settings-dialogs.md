# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-24 19:35:00 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 未标注模型 |
| 任务来源 | 乐器名称与标签顺序、Realtek ASIO 采样格式 17 与崩溃、设置窗口内文件选择、关闭工程跳主页 |
| 关联分支/提交 | 未知 |
| 影响模块 | audio, app, docs |
| 关联记录 | `history/ai/20261024-164728-home-settings-asio-backups.md`、`history/ai/20261024-170644-home-settings-asio-polish.md`、`history/ai/20261024-183406-settings-plugin-ui-polish.md` |

## 2. 任务目标

- 插件列表改为「名称在前、标签在后」，避免先读标签再读乐器名。
- 支持 Realtek ASIO 的 Int24LSB（17）等采样格式，并修复驱动启动后进程 `0xc0000005` 访问冲突。
- 设置页更改默认存储位置 / 添加 VST 扫描目录时，文件对话框在设置窗口上打开，不把主窗口带到前台。
- 关闭工程时若主页已作为独立窗口打开，不再强行把根窗口切回项目列表。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/home.rs` | 修改 | `home_plugin_row` 把插件名称移到所有徽章之前，并新增固定宽度名称列 `PLUGIN_NAME_W = 210` 保证标签纵向对齐 |
| `src/app/settings.rs` | 修改 | `settings_plugin_row` 同步改为名称在前；新增 `SETTINGS_WINDOW_TITLE` 并复用于视口标题；`open_settings_folder_picker` 优先用 `dialog::parent_by_title(SETTINGS_WINDOW_TITLE)` 作为对话框父窗口，找不到时回退主窗口句柄 |
| `src/app/dialog.rs` | 修改 | 新增 `parent_by_title`：Windows 下按标题 `FindWindowW` 定位本进程顶层窗口并校验进程 id，非 Windows 返回 `None` |
| `src/audio/asio.rs` | 修改 | 新增 Int24LSB(17)、Float64LSB(20)、Int32LSB16/18/20/24(24~27) 常量与写入分支，`write_aligned_int32` 处理 32 位容器左对齐；`AsioCallbacks` 移入堆上的回调上下文（驱动会保存该指针）；补 `buffer_switch_time_info` 真实回调，避免驱动空指针跳转 |
| `src/app/shell.rs` | 修改 | `close_root_project`：主页独立窗口已打开且仍有其它工程时，由最近使用的工程接管根窗口；否则根窗口回主页但保留主页状态（含设置窗口），不再调用 `dismiss_home_viewport` |
| `doc/技术文档.md` | 修改 | 4.9 ASIO 采样格式/回调表生命周期；7.10 插件行名称列、设置对话框父窗口、关闭根工程行为；12.2 限制 |
| `doc/关键常量.md` | 修改 | ASIO 支持格式、`PLUGIN_NAME_W`、`SETTINGS_WINDOW_TITLE` |
| `doc/框架工作流.md` | 修改 | `open_settings_folder_picker` 说明补充父窗口来源 |
| `doc/项目简介.md` | 修改 | ASIO 支持格式限制更新 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 插件行 | 徽章在前、名称在后 | 名称在前（固定 210 点列），徽章在后 | 阅读顺序符合预期，标签列对齐 |
| ASIO 回调表 | 栈上局部 `AsioCallbacks`，`start()` 返回后失效 | 随上下文放在堆上，与流同生命周期 | 修复 Realtek ASIO 启动后访问冲突 |
| ASIO 时间信息回调 | `buffer_switch_time_info = None` | 提供真实函数并复用同一渲染入口 | 避免驱动调用空指针 |
| 设置页文件夹对话框 | 以主窗口为父窗口 | 以设置窗口为父窗口（回退主窗口） | 对话框出现在设置窗口上 |
| 关闭根工程 | 总是 `dismiss_home_viewport` 并把根切回项目列表 | 主页已打开且有其它工程时由工程接管根窗口；否则回主页并保留状态 | 不再强行跳回主页项目列表 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `PLUGIN_NAME_W` | 无 | `210` 点 | `src/app/home.rs` | 插件名称列宽 |
| `SETTINGS_WINDOW_TITLE` | 无（字面量） | `设置 · SurfingStudio` | `src/app/settings.rs` | 视口标题 + 原生窗口定位 |
| `ASIO_ST_INT24_LSB` 等 | 仅 16/18/19 | 新增 17、20、24~27 | `src/audio/asio.rs` | 扩展驱动采样格式支持 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 4.9、7.10、12.2 |
| `doc/关键常量.md` | 是 | 音频常量与主题/窗口常量 |
| `doc/框架工作流.md` | 是 | 3.4 `src/app/settings.rs` 函数索引 |
| `doc/项目简介.md` | 是 | ASIO 限制 |
| `doc/代码规范.md` | 否 | 规范未变化，改动遵循既有注释与文档联动要求 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo build` | 通过 | dev profile |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test --lib` | 通过 | 50 passed，0 failed，14 ignored |
| ASIO 实机诊断 | 通过 | `cargo test --lib audio::asio -- --ignored --nocapture`：ASIO4ALL v2（512 帧）与 Realtek ASIO（544 帧）均成功启动并正常 `drop`，无 `0xc0000005` |

## 8. 风险与后续

- 风险：按标题定位设置窗口依赖标题唯一；若将来允许用户自定义窗口标题，需要改为按视口 id 记录原生句柄。
- 风险：关闭根工程且主页已打开但没有其它工程时，根窗口仍必须显示主页（eframe 根视口始终可见），只是不再销毁主页状态。
