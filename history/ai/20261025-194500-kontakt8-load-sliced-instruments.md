# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-25 19:45:00 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 内置编码模型 |
| 任务来源 | 修复 Kontakt 8 VST3 加载失败（`IPluginCompatibility::getCompatibilityJSON failed: 0x1`）与打开工程时界面长时间“未响应” |
| 关联分支/提交 | 未知 / 未知 |
| 影响模块 | plugin, audio, app, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 让没有 `moduleinfo.json` 的单文件 VST3（Kontakt 8）在兼容 JSON 查询返回 `0x1` 时仍能加载。
- 打开重型插件工程时不再出现一次同步阻塞十余秒、窗口“未响应”随后恢复的问题。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `vendor/vst3-host/src/internal/module_info.rs` | 修改 | `getCompatibilityJSON` 只在返回负错误码时失败；空流按“没有兼容映射”返回空表 |
| `vendor/vst3-host/src/internal/plugin_impl.rs` | 修改 | 兼容映射按需查询：仅当缺少 `moduleinfo.json` 且需要解析类 ID 时才实例化兼容类；查询失败降级为空映射并写 `log::warn!` |
| `src/audio/engine/mod.rs` | 修改 | 新增 `InstrumentLoadBudget` 与 `Engine::sync_project_slice`；`sync_project_with` 内部改为 `sync_project_budgeted`；`ensure_instrument` 按时间片推迟剩余乐器插件；新增时间片同步测试 |
| `src/app/mod.rs` | 修改 | 新增 `instrument_load_pending` / `instrument_load_warnings` / `instrument_load_total` / `instrument_load_started` 字段与初始化；`ui_frame` 每帧推进一个补载时间片 |
| `src/app/transport.rs` | 修改 | 新增 `INSTRUMENT_LOAD_SLICE`（120 ms）、`begin_instrument_load` / `step_instrument_load` / `finish_instrument_load` / `finish_loading_state`；`poll_engine_sync` 不再在 UI 线程一次补载全部乐器；`engine_sync_pending`、`can_close_now`、`request_close_when_ready`、`toggle_play`、`sync_engine` 适配分批状态 |
| `src/app/files.rs` | 修改 | 保存 / 导出 / 另存副本在分批补载期间同样提示“正在加载插件，请稍候…” |
| `src/app/autosave.rs` | 修改 | 分批补载期间跳过自动保存 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 单文件 VST3 加载 | 缺少 `moduleinfo.json` 时无条件查询工厂兼容类，`0x1` 即判失败 | 只在需要解析类 ID 时查询；`0x1`/空流/查询失败都按无兼容映射继续加载 | Kontakt 8 等单文件插件可以进入正常加载流程 |
| 打开工程的乐器补载 | 后台同步结束后由 UI 线程一次 `sync_engine` 补载全部 VST 乐器（实测 24 个插件 12 s，触发 `ui.stall` 与“未响应”） | `begin_instrument_load` → 每帧 `step_instrument_load` → `Engine::sync_project_slice`（≤120 ms，至少一个插件）→ `finish_instrument_load` | 加载提示持续刷新并显示进度，单帧不再长时间阻塞 |
| 加载期间的关闭 | 等解析与后台同步结束 | 分批补载没有进行中的插件初始化，可直接放弃剩余乐器后关闭 | 关闭更快且不会在插件初始化中途析构 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `INSTRUMENT_LOAD_SLICE` | 无 | `120` ms | `src/app/transport.rs` | 乐器插件补载时间片；每帧最多实例化该时长内的插件，至少创建一个 |
| VST3 兼容 JSON 判定 | 非 0 结果即失败 | 仅负数结果失败 | `vendor/vst3-host/src/internal/module_info.rs` | 兼容数据是可选元数据，不能据此拒绝插件 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.7 本地 patch（兼容映射按需查询）、9.2 加载（分帧补载） |
| `doc/关键常量.md` | 是 | 第 4 节（`INSTRUMENT_LOAD_SLICE`、VST3 兼容 JSON 返回码、乐器实例化线程说明）、第 5 节（关闭加载中的工程） |
| `doc/框架工作流.md` | 是 | 1.6 工程加载流程、打开工程流程、`engine/mod.rs` 与 `app/transport.rs` 函数索引 |
| `doc/项目简介.md` | 是 | 2.4 插件：分帧补载与单文件 VST3 兼容说明 |
| `doc/代码规范.md` | 否 | 规范本身无变化 |
| `doc/日志规范.md` | 是 | 5.5 新增 `engine.sync.instruments` 事件；7.1 现象对照 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 无格式差异 |
| `cargo test --lib` | 通过 | 62 通过、16 个需要真实插件/硬件的测试忽略；新增 `sync_project_slice_loads_native_instruments_without_pending_plugins` |
| 手工验证 | 未执行 | 尝试用临时 `#[ignore]` 测试直接加载本机 `Kontakt 8.vst3`，命令沙箱内 Kontakt 读 `install_state` 被拒绝（日志 `install_state.cpp:256`）后触发 `STATUS_ACCESS_VIOLATION`，无法在沙箱中验证；临时测试已删除。需在正常桌面环境启动应用打开含 Kontakt 8 的工程确认 |

## 8. 风险与后续

- 风险：分批补载把单个插件的实例化留给单帧，若某个插件自身初始化超过 5 秒，该帧仍可能短暂“未响应”（宿主无法拆分单次插件调用）；Kontakt 仍需在真实桌面环境确认加载与 GUI。
- 后续：在真实桌面环境用含 Kontakt 8 / Serum 2 / VOCALOID 的工程复核 `engine.sync.instruments` 的 `dur_us` 与 `ui.stall` 记录。
