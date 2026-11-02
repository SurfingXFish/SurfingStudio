# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-02 22:33:40 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户要求“尝试接入 ARA2 宿主端支持” |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | plugin, vendor, audio, docs |
| 关联记录 | `history/ai/20261102-213338-plugin-latency-change-and-transport.md`（走带写回） |

## 2. 任务目标

- 引入 Celemony ARA SDK，并确认许可证与本项目 GPL-3.0 兼容。
- 在宿主端识别 VST3 插件的 ARA 工厂，建立最小 ARA 文档模型（文档、音乐上下文、区域序列、音频源、音频修改、播放区域）。
- 在 VST3 规定的时机（首次 `setActive` / `setState` / `getProcessContextRequirements` / GUI 之前）把插件实例绑定到文档控制器。
- 让 ARA 文档的时间线与工程轨道、速度、拍号同步；失败时必须安全降级，不能拖垮宿主。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `vendor/ARA_SDK/` | 新增 | `ARA_SDK` 顶层（Apache-2.0，含 NOTICE）与 `ARA_API` 头文件（`ARAInterface.h` / `ARAVST3.h`）；供 FFI 对照 |
| `src/plugin/ara/ffi.rs` | 新增 | 手写 ARA C API 的 `#[repr(C, packed(1))]` 结构体：ARAFactory、ARADocumentControllerInterface（54 个函数指针）、宿主控制器、属性与内容事件；用例断言每个结构体尺寸 |
| `src/plugin/ara/mod.rs` | 新增 | ARA 工厂发现（`IPlugInEntryPoint` → `IMainFactory`）、`AraSession`（文档 / 音乐上下文 / 区域序列 / 静音音频源 / 修改 / 播放区域）、宿主回调（音频读取、速度与拍号内容读取、模型更新、归档、播放请求）、绑定与线程约束保护 |
| `src/plugin/backends/vst3.rs` | 修改 | 检测二进制是否声明 ARA；启用时通过 `component_ready_hook` 在激活前建立并绑定文档；默认只探测并记录 `ara.detected` |
| `src/plugin/runtime.rs` | 修改 | `Vst3Runtime` 增加 `ara: Option<AraSession>`；新增 `PluginRuntime::set_ara_timeline` / `set_ara_tempo`；ARA 插件不再进入跨工程实例复用缓存 |
| `src/audio/engine/mod.rs` | 修改 | 乐器插件加载后按轨道片段计算 ARA 时间线并同步速度/拍号；新增 `ara_span_seconds` |
| `vendor/vst3-host/src/plugin.rs` | 修改 | 新增公开 `RawPluginHandles` 与 `ComponentReadyHook` 类型、`Plugin::raw_handles` |
| `vendor/vst3-host/src/host.rs` | 修改 | 新增 `defer_activation` 与 `component_ready_hook` 构建选项，并按选项选择加载入口 |
| `vendor/vst3-host/src/internal/plugin_impl.rs` | 修改 | 新增 `load_with_hook`；把 `getProcessContextRequirements` / `prefetchable` 查询移到钩子之后，确保 ARA 绑定早于这些调用 |
| `vendor/vst3-host/src/lib.rs` | 修改 | 导出 `ComponentReadyHook` / `RawPluginHandles` |
| `Cargo.toml` | 修改 | 增加直接依赖 `vst3 = "0.3"`（复用 vst3-host 已有的依赖，无需新下载） |
| `doc/技术文档.md` | 修改 | 新增 6.7 ARA2 宿主端（实验）；6.7 本地 patch 顺延为 6.8，并补 `raw_handles` / `ComponentReadyHook` |
| `doc/关键常量.md` | 修改 | 第 4 节补 ARA 扫描阈值、启用开关、API 世代、角色位、静音音频源通道 |
| `doc/框架工作流.md` | 修改 | 3.3 插件索引补 `src/plugin/ara/` 与新增运行时方法；VST3 `load` 行说明绑定时机 |
| `doc/项目简介.md` | 修改 | 2.4 增加 ARA2（实验）；第 7 节新增默认关闭的已知限制 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| ARA 插件加载 | 只作为普通 VST3 实例加载 | 二进制含 ARA 标记时，通过组件就绪钩子建立文档并尝试绑定（默认仅探测） | 绑定时机符合 VST3/ARA 要求，不再在 `setActive` 之后调用 |
| 组件激活 | 加载结束时直接 `setActive(true)` | 保留同一路径；ARA 绑定在钩子中完成，仍早于激活 | 不改变普通插件的加载顺序 |
| 工程时间线 | 只写 VST3 `ProcessContext` | 额外把轨道片段范围换算成秒并更新 ARA 播放区域；速度/拍号变化时通知音乐上下文 | ARA 插件能获得与工程一致的播放时间与速度 |
| 单实例复用 | VOCALOID / ACE Bridge 跨工程保留实例 | ARA 会话存在时跳过复用，避免实例与旧文档控制器绑定错位 | 关闭再打开工程会重新加载 ARA 插件 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `SURFING_ARA_HOST` | 无 | `1` / `true` 启用 | `src/plugin/ara/mod.rs` | 默认关闭，防止插件在角色绑定时崩溃 |
| `ARA_MARKER_SCAN_LIMIT` | 无 | `64` MiB | `src/plugin/ara/mod.rs` | 单文件超过 256 MiB 跳过标记扫描 |
| `ARA_API_GENERATION_2_0_FINAL` | 无 | `4` | `src/plugin/ara/mod.rs` | 插件支持范围内优先使用 2.0 Final |
| `ARA_ROLE_ALL` | 无 | `7` | `src/plugin/ara/mod.rs` | 播放渲染 + 编辑器渲染 + 编辑器视图 |
| `SILENT_SOURCE_CHANNELS` | 无 | `2` | `src/plugin/ara/mod.rs` | 宿主提供的静音音频源通道数 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.7 / 6.8 |
| `doc/关键常量.md` | 是 | 第 4 节插件宿主与扫描 |
| `doc/框架工作流.md` | 是 | 3.3 `plugin` 文件与函数索引 |
| `doc/项目简介.md` | 是 | 2.4、第 7 节 |
| `doc/代码规范.md` | 否 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | 115 passed / 21 ignored（新增 3 个 ARA 用例、1 个真实插件忽略用例） |
| ARA 结构体尺寸 | 通过 | `ffi::tests::struct_sizes_match_the_packed_c_layout` 覆盖 20 个跨边界结构体 |
| 真实插件探测 | 通过 | 忽略用例在 `VOCALOIDBridge.vst3` 上读到 `com.yamaha.vocaloidbridge.arafactory`、Yamaha 6.5.0、API 世代 3..5、归档 ID `...aradocumentarchive.version1` |
| ARA 文档创建 | 通过（单测环境） | 工厂 → `createDocumentControllerWithDocument` → 音乐上下文 → 区域序列 → `notifyModelUpdates` 均成功返回 |
| ARA 角色绑定 | 失败（插件侧） | 调用 `IPlugInEntryPoint2::bindToDocumentControllerWithRoles` 时在 VOCALOID Bridge 内部触发 `0xC0000005`；ARA 1 绑定被插件拒绝（返回空）。因此默认关闭绑定 |
| 普通插件回归 | 通过 | 非 ARA 插件的加载/扫描/处理路径未改变 |

## 8. 风险与后续

- 风险：ARA 绑定在 VOCALOID Bridge 6.5 内部崩溃，原因尚未定位（已排除工厂不一致、API 世代、绑定时机、角色位与 ARA 1 回退）。宿主无法捕获插件线程内的访问冲突，因此默认关闭；只有设置 `SURFING_ARA_HOST=1` 才会执行绑定。
- 风险：ARA C API 结构体为 `packed(1)`，手写绑定存在版本漂移风险；当前以 `vendor/ARA_SDK/ARA_API` 为准，并用尺寸用例固定。
- 风险：ARA 文档控制器只能在创建线程操作；如果将来把乐器加载移出界面线程，`set_timeline` / `set_tempo` 会返回错误而不是跨线程调用。
- 后续：补齐 ARA 归档（`storeObjectsToArchive` / `restoreObjectsFromArchive`）与 `notifySelection`，让插件侧编辑可随工程保存；在可调试环境下抓取 VOCALOID Bridge 的崩溃栈，确认是草稿接口差异还是插件对宿主环境的要求。
