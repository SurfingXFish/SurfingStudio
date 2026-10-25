# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-21 22:09:10 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 问题 3 未解决：FabFilter GUI 不动态更新；VST 插件扫描卡顿 |
| 关联分支/提交 | 未知 |
| 影响模块 | app / plugin / vendor/vst3-host / docs |
| 关联记录 | `history/ai/20261021-210525-plugin-space-fabfilter-classify.md` |

## 2. 任务目标

- 修复插件编辑器 GUI 在播放时停止刷新（FabFilter Pro-Q 频谱停在最后一帧）。
- 消除扫描 VST 插件时的长时间卡顿，同时保留乐器/效果器分类能力。
- 保留上一轮的独立窗口空格暂停、编辑器尺寸转发与严格类型筛选。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/heartbeat.rs` | 新增 | 编辑器打开期间的 UI 心跳线程：每 16 ms 调用 `egui::Context::request_repaint` 并向 UI 线程 `PostThreadMessageW(WM_NULL)`，唤醒空闲消息循环，让低优先级 `WM_TIMER` 被处理 |
| `src/app/mod.rs` | 修改 | 声明 `heartbeat` 模块；每帧安装/更新心跳，按是否存在编辑器调用 `set_active`；后台识别插件类型时保持 120 ms 重绘 |
| `src/app/plugins.rs` | 修改 | `start_scan_with` 清理旧识别任务；`poll_scan` 合并上次分类结果并调用 `start_plugin_probes`；新增 `start_plugin_probes` / `poll_plugin_probes` |
| `src/ui/browser.rs` | 修改 | 扫描/后台识别期间显示“扫描中…”或“正在识别插件类型…” |
| `src/plugin/backend.rs` | 修改 | `PluginBackend` 增加 `probe`（默认 `None`） |
| `src/plugin/backends/vst3.rs` | 修改 | `Vst3Backend::probe` 调用 `probe_vst3_path` |
| `src/plugin/host.rs` | 修改 | 新增 `PluginHost::probe` / `PluginHost::probe_plugin_metadata` |
| `src/plugin/scan.rs` | 修改 | 快速扫描不再加载无元数据的模块（bundle/单文件 VST3 直接标记未知）；新增 `probe_vst3_path` 供后台分类 |
| `src/plugin/mod.rs` | 修改 | 新增 `probes_single_file_vst3_metadata` 测试；保留 Pro-Q 诊断测试（默认 ignore） |
| `vendor/vst3-host/src/internal/com_implementations.rs` | 修改 | `ConnectionProxy` 不再丢弃处理器线程的 `notify`：复制进有界队列（256），由 UI 线程 `pump` 送达；测试改为验证排队与送达 |
| `vendor/vst3-host/src/internal/plugin_impl.rs` | 修改 | 实现 `service_connection_messages`（每帧送达排队消息） |
| `vendor/vst3-host/src/plugin.rs` | 修改 | `PluginInternal` / `Plugin` 新增 `service_connection_messages` |
| `vendor/vst3-host/src/window.rs` | 修改 | `service_platform_events` 每帧先送达连接点消息再处理 resize |
| `doc/技术文档.md` | 修改 | 2.3 心跳、6.3 快速扫描 + 后台分类、6.7 连接点消息队列、7.1 每帧流程、10 性能表 |
| `doc/关键常量.md` | 修改 | 新增快速扫描策略、连接点队列上限、心跳 16 ms、分类重绘 120 ms |
| `doc/框架工作流.md` | 修改 | 首帧流程、扫描流程、backend/host/scan 索引、`app/heartbeat.rs` 索引、插件扫描入口 |
| `doc/项目简介.md` | 修改 | 扫描不再长时间卡顿、UI 心跳与频谱持续刷新 |
| `doc/代码规范.md` | 修改 | 8.2 扫描规则改为“快速扫描 + 后台工厂元数据分类” |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 插件 GUI 刷新 | 仅依赖 egui 重绘与插件自身 `WM_TIMER`；主窗口空闲/被覆盖时定时器不被处理 | 编辑器打开期间 UI 心跳每 16 ms 请求重绘并投递 `WM_NULL` | Pro-Q / FabFilter 频谱在未聚焦、主窗口被覆盖时仍持续刷新 |
| 插件扫描 | 扫描线程逐个加载模块读取工厂元数据，约 13.5 s | 快速扫描只读 `moduleinfo.json`，无元数据先标记未知（约 0.01 s） | 浏览器立即显示列表，不再卡顿 |
| 类型识别 | 扫描阶段同步完成 | 扫描后后台线程逐项 `probe_plugin_metadata`，结果通过 mpsc 更新缓存 | 类型逐步补齐，UI 始终可交互 |
| 重复扫描 | 每次重新加载所有插件 | 保留上次已识别的类型/名称/厂商/CID | 二次扫描接近瞬时 |
| 连接点消息 | 处理器线程的 `notify` 被丢弃 | 复制进有界队列，UI 线程每帧送达 | 依赖连接点推送电平/波形的插件 GUI 不再冻结 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `HEARTBEAT_INTERVAL` | 无 | `16` ms | `src/app/heartbeat.rs` | UI 心跳间隔 |
| 后台识别重绘 | 无 | `120` ms | `src/app/mod.rs` | 分类进行中轮询插件列表 |
| `MAX_PENDING_CONNECTION_MESSAGES` | 无（消息被丢弃） | `256` | `vendor/vst3-host/src/internal/com_implementations.rs` | 连接点消息队列上限 |
| VST3 扫描策略 | 同步工厂探测 | 快速扫描 + 后台分类 | `src/plugin/scan.rs`、`src/app/plugins.rs` | 扫描响应时间从约 13.5 s 降到约 0.01 s |
| 后台识别间隔 | 无 | `10` ms/插件 | `src/app/plugins.rs` | 降低连续 `LoadLibrary` 对 UI/音频线程的抢占 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 2.3、6.3、6.7、7.1、10 |
| `doc/关键常量.md` | 是 | 第 4 节插件宿主与扫描、第 5 节重绘 |
| `doc/框架工作流.md` | 是 | 1.1、1.7、3.3、3.4、4 |
| `doc/项目简介.md` | 是 | 2.4、3.2 |
| `doc/代码规范.md` | 是 | 8.2 扫描规则 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 已运行 |
| `cargo check --all-targets` | 通过 | 无警告 |
| `cargo test` | 通过 | 24 passed / 0 failed / 9 ignored |
| `probes_single_file_vst3_metadata` | 通过 | Pro-Q 4 识别为效果器且 CID 非空 |
| `scans_installed_vst3_metadata` | 通过 | 快速扫描 0.01 s；FabFilter 条目先显示为未知，等待后台分类 |
| 桌面复现验证 | 通过 | 临时诊断脚本自动加载 Pro-Q 4、循环播放并截图对比：分析器区域像素持续变化；诊断代码已在验证后移除 |

## 8. 风险与后续

- 风险：UI 心跳在编辑器打开期间持续请求重绘，会多消耗少量 CPU；心跳只在编辑器打开时工作。
- 风险：后台分类仍会加载插件模块（不创建实例），个别插件的 `DllMain` / `InitDll` 可能写日志或初始化 COM；但不再阻塞浏览器列表。
- 风险：连接点消息队列满时丢弃最旧消息，极端情况下电平/波形会跳帧。
- 后续：在真实工程中用 FabFilter Pro-Q 4 / Pro-DS / Pro-C 2 验证频谱刷新与边框缩放；若某插件依赖 `IConnectionPoint` 的特定返回码，记录插件名并扩展测试。
