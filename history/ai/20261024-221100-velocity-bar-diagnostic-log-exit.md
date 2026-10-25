# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-24 22:11:00 (Asia/Shanghai) |
| AI 工具/模型 | Trae / AI 编码助手 |
| 任务来源 | 力度控制器增加等长横线；新增诊断日志系统与日志规范；修复关闭软件后进程残留 |
| 关联分支/提交 | 未知 / 未知 |
| 影响模块 | diag, app, ui, docs |
| 关联记录 | `history/ai/20261024-010229-home-close-exit-hang-fix.md` |

## 2. 任务目标

- 钢琴卷帘力度控制器：在力度圆点后方增加一条与音符等长的横线。
- 新增自定义文件格式（`.sflog`）的诊断日志系统，记录做过什么、各处耗时，并建立 `doc/日志规范.md`。
- 修复关闭软件后进程未完全退出的问题，并提供可诊断的退出路径。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/diag/mod.rs` | 新增 | 进程级诊断日志：`init` 创建 `.sflog`、写头帧、安装 panic 钩子、启动看门狗；`info/warn/error`、`Span`（`start` / `start_logged` / `end` / `end_slow`）、`note_frame` / `note_frame_duration` / `set_activity`；`request_exit` 写尾帧后终止进程；`crash` 供异常过滤器使用；看门狗检测 UI 卡死与退出超时 |
| `src/diag/format.rs` | 新增 | `.sflog` 行格式化：字段转义、本地时间戳、旧日志清理（保留最近 20 个） |
| `src/lib.rs` | 修改 | 声明 `pub mod diag` 并在库文档补充该层 |
| `src/main.rs` | 修改 | 启动最先调用 `diag::init`；未处理异常过滤器写 FATAL 记录；`run_native` 返回后统一走 `diag::request_exit` |
| `src/app/shell.rs` | 修改 | 每帧记录 UI 帧心跳与慢帧；关闭请求记录 `window.close.request`；根是主页且无工程时直接 `diag::request_exit("home_closed")`；`on_exit` 走同一退出入口；记录根工程关闭与工程提升 |
| `src/app/mod.rs` | 修改 | 窗口创建日志；`ui_frame` 写入当前阶段（`ui.frame` / `ui.poll` / `ui.paint`）供卡死定位 |
| `src/app/shared.rs` | 修改 | 启动阶段记录设置读取、插件缓存读取、音频设备打开与共享服务初始化的耗时 |
| `src/app/files.rs` | 修改 | 记录工程加载（后台解析与 UI 应用）、保存（含历史版本备份）、WAV 导出的耗时与结果 |
| `src/app/plugins.rs` | 修改 | 记录插件扫描、插件应用、插件编辑器打开耗时；编辑器服务阶段写入 `plugin.editors` |
| `src/app/transport.rs` | 修改 | 记录后台插件同步（真实耗时与 UI 等待时长）与慢同步（`ENGINE_SYNC_SLOW_MS`） |
| `src/ui/piano_roll/mod.rs` | 修改 | 力度控制器在圆点后方绘制与音符等长的横线（按可见区域裁切） |
| `src/ui/piano_roll/helpers.rs` | 修改 | 新增 `VELOCITY_BAR_H` 线宽常量 |
| `doc/日志规范.md` | 新增 | `.sflog` 文件结构、7 列记录格式、转义规则、级别、事件目录、卡死/崩溃判读流程与 PowerShell 查询示例 |
| `doc/技术文档.md` | 修改 | 模块表与线程表新增 diag；7.1 退出路径改为确定性终止；7.3 力度横线；新增 13 章“诊断日志” |
| `doc/关键常量.md` | 修改 | 新增 `VELOCITY_BAR_H`、`ENGINE_SYNC_SLOW_MS` 与“诊断日志”常量表 |
| `doc/框架工作流.md` | 修改 | 启动/首帧/退出流程加入日志锚点；模块地图、函数索引新增 `diag` |
| `doc/项目简介.md` | 修改 | 力度控制器横线、诊断日志说明、退出行为与目录结构更新 |
| `doc/代码规范.md` | 修改 | 分层结构加入 `diag` 叶子模块及依赖规则；联动表与检查清单加入日志规范；新增两条禁止事项 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 应用退出 | 依赖 eframe 关闭根窗口 → `on_exit` → `TerminateProcess` | `WindowHost::ui` 识别真实退出路径后立即 `diag::request_exit`（写尾帧 + 终止）；`on_exit` 与 `main` 为同一入口的兜底；看门狗在 3 s 后再次强制结束 | 窗口消失即代表进程终止，插件/驱动阻塞事件循环也不会留下后台进程 |
| 启动流程 | 直接创建 eframe 窗口 | `main` 先初始化日志，再安装异常过滤器与创建窗口 | 启动耗时、panic 与退出原因写入同一日志文件 |
| 每帧编排 | 关闭处理 → 绘制 → 重绘 | 增加帧心跳、阶段标记与慢帧记录 | 卡死/慢帧可定位到具体阶段，不改变绘制顺序 |
| 工程/插件/同步 | 无耗时记录 | 加载、保存、扫描、插件应用、编辑器打开、引擎同步写入耗时区间 | 可量化每一步耗时，定位卡顿来源 |
| 力度控制器 | 竖线 + 圆点 | 竖线 + 圆点 + 与音符等长的横线 | 力度视图可直接读出时值 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `VELOCITY_BAR_H` | 无 | `1.6` 点 | `src/ui/piano_roll/helpers.rs` | 力度圆点后方横线线宽 |
| `ENGINE_SYNC_SLOW_MS` | 无 | `30` ms | `src/app/transport.rs` | 慢同步日志阈值 |
| `FORMAT_MAGIC` | 无 | `SFLG1` | `src/diag/mod.rs` | 日志格式版本 |
| `LOG_KEEP_FILES` | 无 | `20` | `src/diag/mod.rs` | 日志文件保留数 |
| `WATCHDOG_TICK_MS` | 无 | `250` ms | `src/diag/mod.rs` | 看门狗轮询间隔 |
| `UI_SLOW_FRAME_MS` | 无 | `120` ms | `src/diag/mod.rs` | 慢帧阈值 |
| `UI_SLOW_FRAME_REPEAT_MS` | 无 | `1000` ms | `src/diag/mod.rs` | 慢帧记录节流 |
| `UI_STALL_MS` | 无 | `3000` ms | `src/diag/mod.rs` | UI 卡死判定 |
| `UI_STALL_REPEAT_MS` | 无 | `15000` ms | `src/diag/mod.rs` | 卡死重复上报间隔 |
| `EXIT_WATCHDOG_MS` | 无 | `3000` ms | `src/diag/mod.rs` | 退出超时看门狗 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 模块职责、2.1 线程、7.1 退出与每帧流程、7.3 力度控制器、新增 13 章 |
| `doc/关键常量.md` | 是 | 5 App 阈值、7 力度控制器、新增 10 诊断日志 |
| `doc/框架工作流.md` | 是 | 1.1 启动/首帧流程、2 模块地图、3.4 函数索引、3.6 `diag`、4 快速入口 |
| `doc/项目简介.md` | 是 | 2.2 力度控制器、2.6 退出与诊断日志、3.5 可维护性、6 目录结构 |
| `doc/代码规范.md` | 是 | 1.1/1.2/1.3 分层与依赖、1.4 判定表、10.6 联动表、14 检查清单、15 禁止事项 |
| `doc/日志规范.md` | 是 | 新增；说明格式、事件目录与判读方法 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | 58 passed、16 ignored；新增 `diag` 记录行格式测试 |
| `cargo clippy --all-targets` | 通过 | 未新增错误；历史警告保持原状 |
| 手工验证（启动/退出） | 通过 | 启动 6 s 后关闭主页窗口：进程立即退出，日志尾部为 `window.close.request` → `window.exit` → `process.exit` → `# end reason=home_closed` |
| 手工验证（日志内容） | 通过 | 日志包含 `settings.load`（117 µs）、`plugin_cache.load`、`audio.open`（297 ms，ASIO4ALL）、`shared.init`、增量扫描等耗时 |
| 手工验证（力度横线） | 未逐项截图 | 代码按 `beat_to_x(note.end())` 绘制，长度等于音符宽度；未做截图比对 |

## 8. 风险与后续

- 风险：日志目录与文件内容包含本机路径、插件路径与设备名，分享前需脱敏（已写入日志规范）。
- 风险：`ui.stall` 依赖 UI 帧心跳；窗口被系统挂起（如睡眠）后恢复可能产生一次误报。
- 风险：`WindowHost::ui` 内直接终止进程会跳过 eframe 的收尾，与既有策略一致，但主页新增需要落盘的后台任务时必须在退出前完成。
- 后续：如需长期分析，可在日志规范基础上增加离线查看器；插件加载耗时目前由 `engine.sync` 汇总，未细分到单个插件。
