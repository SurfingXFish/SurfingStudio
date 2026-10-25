# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-19 20:33:53 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 代码智能体 |
| 任务来源 | 将所有多实现模块统一为「运行时 + 后端」框架，并把规范写入 doc |
| 关联分支/提交 | 未知 / 未知 |
| 影响模块 | core, audio, plugin, app, docs |
| 关联记录 | `history/ai/20261019-202137-project-runtime-framework.md`（本记录继续推广该模式） |

## 2. 任务目标

- 把「运行时 + 后端」从工程格式推广到插件格式与音频文件格式。
- 应用层与引擎只调用运行时 API，不直接依赖格式后端。
- 为实时 DSP、渲染、输出等模块明确框架边界与例外规则。
- 将统一模块框架规范写入代码规范、技术文档与框架工作流。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/backend.rs` | 新增 | 定义 `PluginBackend` 与 `PluginBackendInfo` |
| `src/plugin/host.rs` | 新增 | 定义 `PluginHost`：后端注册、加载与扫描分派、进程级入口 |
| `src/plugin/backends/mod.rs` | 新增 | 聚合并导出 VST3 / VST2 后端 |
| `src/plugin/backends/vst3.rs` | 移动/修改 | 从 `plugin/vst3.rs` 迁入，实现 `Vst3Backend` |
| `src/plugin/backends/vst2.rs` | 移动/修改 | 从 `plugin/vst2.rs` 迁入，实现 `Vst2Backend` |
| `src/plugin/mod.rs` | 修改 | 声明框架模块并移除 `load_plugin` / 扫描直出 |
| `src/plugin/scan.rs` | 修改 | 改为共享扫描工具，经 `backends` 暴露 VST2 宿主类型 |
| `src/plugin/runtime.rs` | 修改 | 调整 VST2 后端符号导入路径 |
| `src/audio/io/mod.rs` | 新增 | 定义 `AudioCodec`、`AudioCodecInfo`、`AudioRuntime`、`AudioFile` 与离线渲染导出 |
| `src/audio/io/wav/mod.rs` | 新增 | WAV 后端：解码、时长、16-bit 编码 |
| `src/audio/file.rs` | 删除 | 内容迁入 `audio/io/wav/mod.rs` |
| `src/core/export.rs` | 删除 | 导出逻辑迁入 `audio/io/mod.rs`，修复 `core` 依赖音频的问题 |
| `src/audio/mod.rs` | 修改 | 声明 `audio::io` 并导出 `AudioRuntime` / `AudioFile` |
| `src/core/mod.rs` | 修改 | 移除 `export` 模块 |
| `src/audio/engine/mod.rs` | 修改 | 使用 `PluginHost::load_plugin` 与 `AudioRuntime::import_audio` |
| `src/app/transport.rs` | 修改 | 使用 `AudioRuntime::import_audio` |
| `src/app/plugins.rs` | 修改 | 使用 `PluginHost::scan_plugin_format` |
| `src/app/files.rs` | 修改 | 使用 `AudioRuntime::export_project` |
| `src/app/mod.rs` | 修改 | 引入 `PluginHost` |
| `src/ui/arrangement/mod.rs` | 修改 | 使用 `AudioRuntime::audio_duration_seconds` |
| `src/lib.rs` | 修改 | 更新分层说明并补齐双语模块文档 |
| `doc/代码规范.md` | 修改 | 新增 1.5 统一模块框架规范、8.1 插件框架、13.8/13.9 扩展手册与检查项 |
| `doc/技术文档.md` | 修改 | 新增 1.3 统一模块框架，更新模块职责、插件、音频 I/O、导出与附录 |
| `doc/框架工作流.md` | 修改 | 更新插件/音频 I/O 流程、模块地图、函数索引与快速入口 |
| `doc/关键常量.md` | 修改 | 更新后端路径与注册顺序 |
| `doc/项目简介.md` | 修改 | 更新目录结构与可维护性说明 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 插件加载 | 引擎直接调用 `load_plugin` 并按格式分支 | `PluginHost::load_plugin` 统一分派到 `PluginBackend` | 应用层不感知 VST3/VST2 |
| 插件扫描 | 应用层分别调用 `scan_vst3` / `scan_legacy_vst` | `PluginHost::scan_plugin_format` / `scan_plugins` | 扫描入口与错误处理统一 |
| 音频解码 | 引擎/应用直接调用 `load_wav` | `AudioRuntime::import_audio` 分派到 `AudioCodec` | 新增格式不改调用方 |
| 音频导出 | `core::export::export_wav` 直接渲染 + 写 WAV | `AudioRuntime::export_project` 渲染后按扩展名选择后端 | 导出与音频格式解耦，`core` 不再依赖音频 |
| 新增格式 | 修改多个调用点 | 实现后端并在运行时注册 | 扩展点统一 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 音频后端注册顺序 | 无 | `wav` | `src/audio/io/mod.rs` | `AudioRuntime::standard` |
| 插件后端注册顺序 | 无 | `vst3` → `vst2` | `src/plugin/host.rs` | `PluginHost::standard` |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 新增 1.3；更新 1.2、6、8、9.4、12.1、附录 B |
| `doc/关键常量.md` | 是 | 第 2、4 节路径与注册顺序 |
| `doc/框架工作流.md` | 是 | 1.7、2、3.2、3.4、4 |
| `doc/项目简介.md` | 是 | 3.5 与第 6 节目录结构 |
| `doc/代码规范.md` | 是 | 1.2、1.3、1.4、新增 1.5、8.1、12、13.8、13.9、14 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 无格式差异 |
| `cargo test` | 通过 | 25 通过，0 失败，4 个真实插件测试按预期忽略 |
| 手工验证 | 未执行 | 本次为架构重构；工程/插件分派、WAV 导出与框架入口由单元测试覆盖 |

## 8. 风险与后续

- 风险：`plugin/backends/*` 与 `audio/io/*` 成为内部路径；仓库内引用已全部迁移。
- 后续：若未来新增 FLAC/MP3 音频后端或 AU/CLAP 插件后端，按 1.5 规范新增后端并注册即可。
