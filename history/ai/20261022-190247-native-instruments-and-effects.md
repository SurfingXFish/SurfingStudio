# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-22 19:02:47 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 未标注模型 |
| 任务来源 | 扩充内部原生乐器（钢琴、架子鼓、木吉他、电吉他、失真电吉他、贝斯、小提琴、弦乐断奏、弦乐拨奏、弦乐联奏）并支持全部内部控制器；扩充内部原生效果器（过载、失真、法兹、合唱、相位、延时、混响、压缩） |
| 关联分支/提交 | 未知 / 未知 |
| 影响模块 | core, audio, app, ui, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 新增九类原生乐器（架子鼓沿用现有 DrumKit），并让原生乐器与鼓机都响应音量、声像、力度、延音、弯音五类内部控制器。
- 新增八种内置原生效果器，可插入轨道与主输出的效果链。
- 保持旧 `.sfsd` 工程兼容：旧工程的效果链仍是 `PluginRef` 数组，加载后自动转为 `EffectRef::Plugin`。
- 在检查器与效果器卷帘中提供乐器类型切换、原生效果添加与参数编辑。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/core/model/effect.rs` | 新增 | `NativeEffectKind`、`NativeEffectParam`、`NativeEffectSettings`、`NativeEffect`、`EffectRef`（untagged，兼容旧插件数组）及单元测试 |
| `src/core/model/instrument.rs` | 修改 | 新增 `NativeInstrumentKind`、`NativeInstrumentSettings`、`NativeInstrument` 与 `InstrumentRef::Native` |
| `src/core/model/mod.rs` | 修改 | 声明并导出 `effect` 模块与原生乐器/效果类型 |
| `src/core/model/track.rs` | 修改 | `effects: Vec<PluginRef>` → `Vec<EffectRef>` |
| `src/core/model/project.rs` | 修改 | `master_effect: Option<PluginRef>` → `Option<EffectRef>`；新增乐器轨道命名 |
| `src/audio/synth/modeled.rs` | 新增 | 钢琴非谐分音、拨弦 Karplus-Strong、弓弦多振荡器 + ADSR；延音踏板、弯音与单元测试 |
| `src/audio/synth/mod.rs` | 修改 | 导出 `ModeledInstrument` |
| `src/audio/synth/poly.rs` | 修改 | `poly_blep` 提升为 `pub(super)` 供弓弦模型复用 |
| `src/audio/synth/drums.rs` | 修改 | 新增 `set_sustain` / `set_pitch_bend`，弯音作用于音调类鼓件，延音延长闭镲衰减 |
| `src/audio/fx/mod.rs` | 新增 | 内置效果器模块聚合与 `NativeEffectProcessor` 导出 |
| `src/audio/fx/native.rs` | 新增 | 过载/失真/法兹/合唱/相位/延时/混响/压缩 DSP、延迟线预分配与测试 |
| `src/audio/mod.rs` | 修改 | 声明 `fx` 模块 |
| `src/audio/engine/mod.rs` | 修改 | `InstrumentRuntime::Native`、`EffectRuntime`（Plugin/Native）、按 `EffectRef::key` 复用/同步参数、主效果器支持内置效果 |
| `src/audio/engine/render.rs` | 修改 | 原生乐器渲染；插入/主效果链通过 `EffectRuntime::process`；延音/弯音分发到原生乐器与鼓机 |
| `src/audio/engine/plugins.rs` | 修改 | 插件 API 只访问 `EffectRuntime::Plugin`，状态捕获跳过内置效果 |
| `src/app/edit.rs` | 修改 | 新增 `add_native_effect`，支持轨道与主输出；剪贴板类型随 `EffectRef` |
| `src/app/state.rs` | 修改 | 剪贴板 `Effect` / `EffectChain` 改为 `EffectRef` |
| `src/app/plugins.rs` | 修改 | 插件写入效果链包为 `EffectRef::Plugin`；名称/路径/状态访问适配 `EffectRef` |
| `src/app/files.rs` | 修改 | 加载提示只在存在真实插件时显示“正在加载插件” |
| `src/app/transport.rs` | 修改 | 后台插件槽位标记跳过内置效果 |
| `src/core/project_runtime/sfsd/manifest.rs` | 修改 | 插件状态块遍历只收集 `EffectRef::Plugin` |
| `src/core/project_runtime/sfsd/mod.rs` | 修改 | 往返测试改用 `EffectRef::Plugin` |
| `src/core/project_runtime/soundbug/convert.rs` | 修改 | 导入效果器包为 `EffectRef::Plugin` |
| `src/core/project_runtime/soundbug/import.rs` | 修改 | 导入测试只收集插件效果 |
| `src/plugin/mod.rs` | 修改 | 真实插件测试只遍历插件效果 |
| `src/ui/effects.rs` | 修改 | 卡片支持内置效果参数编辑；添加菜单含八种内置效果与 VST 浏览；开关使用 `EffectRef` |
| `src/ui/inspector.rs` | 修改 | 乐器选择新增“原生乐器”与类型下拉；效果链/主输出支持添加内置效果并编辑参数 |
| `src/ui/arrangement/mod.rs` | 修改 | 主输出与轨道效果链标签使用 `EffectRef::label` / `badge` |
| `doc/技术文档.md` | 修改 | 模块职责、数据模型、4.5/4.6 内置乐器与效果器、测试与文件索引 |
| `doc/关键常量.md` | 修改 | 原生乐器 voice/延迟线/分音数，内置效果延迟线、混响与压缩参数范围 |
| `doc/框架工作流.md` | 修改 | 模块地图、文件/函数索引与常见任务入口 |
| `doc/项目简介.md` | 修改 | 新增 2.5 内置乐器与原生效果器；目录结构更新 |
| `doc/代码规范.md` | 不涉及 | 规范未变化 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 效果链模型 | `Vec<PluginRef>` / `Option<PluginRef>` | `Vec<EffectRef>` / `Option<EffectRef>`（插件或内置效果） | 效果链可混排，旧工程自动兼容 |
| 引擎效果运行时 | `Vec<PluginRuntime>` | `Vec<EffectRuntime>`（Plugin / Native） | 渲染按类型分派，插件 API 只访问 Plugin |
| 效果同步 | 按插件键复用，参数直接作用实例 | 按 `EffectRef::key` 复用；内置效果每次同步 `set_settings` | 参数实时生效且不重建延迟线 |
| 新增效果入口 | 仅插件浏览器 | 效果器卷帘/检查器添加菜单可选内置效果，`add_native_effect` 写入模型 | 无需插件即可添加效果 |
| 内置乐器 | 仅 Synth / Drums | 增加 `ModeledInstrument` 与 `InstrumentRef::Native` | 检查器切换类型后实时生效 |
| 控制器分发 | 鼓机忽略延音/弯音 | 原生乐器与鼓机均接收延音/弯音 | 五类控制器对全部内置乐器有效 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `MAX_VOICES` | 无 | `24` | `src/audio/synth/modeled.rs` | 原生乐器复音数 |
| `DELAY_LEN` | 无 | `8192` 采样 | `src/audio/synth/modeled.rs` | 原生乐器延迟线长度 |
| 钢琴分音数 | 无 | `6` | `src/audio/synth/modeled.rs` | 非谐分音数量 |
| 弓弦振荡器数 | 无 | `3` | `src/audio/synth/modeled.rs` | 失谐锯齿数量 |
| `CHORUS_MAX_SECONDS` | 无 | `0.06` 秒 | `src/audio/fx/native.rs` | 合唱延迟线容量 |
| `DELAY_MAX_SECONDS` | 无 | `1.0` 秒 | `src/audio/fx/native.rs` | 延时线容量 |
| 内置延时时间范围 | 无 | `40 ~ 900` ms | `src/audio/fx/native.rs` | `time` 参数映射 |
| 内置压缩范围 | 无 | 阈值 `-48 ~ 0` dB、比率 `1 ~ 20`:1、起音 `1 ~ 100` ms、释放 `20 ~ 2000` ms | `src/audio/fx/native.rs` | 压缩参数映射 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 模块职责、1.3 运行时表、3.2/3.5 数据模型、4.5 内置乐器、4.6 内置原生效果器、11.1 测试、12.1 扩展点、附录 B |
| `doc/关键常量.md` | 是 | 第 2 节音频引擎与内置乐器常量表 |
| `doc/框架工作流.md` | 是 | 2 模块地图、3.1/3.2 文件与函数索引、4 常见任务入口 |
| `doc/项目简介.md` | 是 | 新增 2.5 内置乐器与原生效果器；目录结构 |
| `doc/代码规范.md` | 否 | 规范未变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 无格式差异 |
| `cargo test` | 通过 | 31 passed / 0 failed / 11 ignored（含原生乐器发声、延音、内置效果与引擎整链渲染） |
| 手工验证 | 未执行 | 未启动 GUI 试听；已由自动化测试覆盖原生乐器发声、延音踏板、内置效果处理与工程序列化 |

## 8. 风险与后续

- 风险：原生乐器为纯 DSP 建模，音色接近但不等于真实采样；高复音时延迟线占用约 `24 × 8192 × 4 B ≈ 768 KiB/轨`。
- 风险：旧工程不含 `EffectRef::Native`，保存后会写入新的 untagged 结构；已验证旧插件数组仍可反序列化，但第三方工具不应依赖旧内联结构。
- 后续：可为原生乐器增加更细的奏法参数、采样层与效果器预设；如需 flanger（镶边）可新增 `NativeEffectKind` 并在 `audio/fx/native.rs` 增加 DSP。
