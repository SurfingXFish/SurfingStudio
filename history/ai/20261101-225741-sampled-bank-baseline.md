# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-01 22:57:41 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户要求把工作区既有的 128 件采样乐器改动整理为功能分支基线提交 |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | core, audio, ui, docs, build |
| 关联记录 | `history/ai/20261101-230500-neural-timbre-model.md` |

## 2. 任务目标

- 把 `test` 工作区中尚未提交的采样音色库实现整理成一条独立提交，作为后续神经网络音色功能的基线。
- 保证基线可编译，且与 `doc/` 中已描述的用户可见行为一致。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/core/model/sample_bank.rs` | 新增 | 采样音色库清单、分区、`SampleSettings` 与内嵌音频查询接口 |
| `src/audio/synth/sampled.rs` | 新增 | 采样乐器运行时：分区选择、变速变调、Hermite 插值、循环接缝交叉淡化 |
| `src/audio/synth/mod.rs` | 修改 | 导出 `SampledInstrument` |
| `src/core/model/instrument.rs` | 修改 | `InstrumentRef::Sample` 变体、显示名与身份键 |
| `src/core/model/mod.rs` | 修改 | 导出 `sample_bank` 公共类型 |
| `src/core/model/project.rs` | 修改 | 工程默认轨道乐器保持兼容 |
| `src/core/model/plugin.rs` | 修改 | 乐器窗口分类需要的桥接器/乐器种类判断 |
| `src/audio/engine/mod.rs` | 修改 | `InstrumentRuntime::Sample` 接入与参数更新 |
| `src/audio/engine/render.rs` | 修改 | 采样乐器参与实时渲染的测试覆盖 |
| `src/app/plugins.rs` | 修改 | 乐器列表与内置采样乐器的分类映射 |
| `src/ui/inspector.rs` | 修改 | 检查器新增采样乐器参数区 |
| `src/ui/instrument_picker.rs` | 修改 | 乐器窗口列出 128 件采样乐器 |
| `src/ui/browser.rs` / `src/ui/effect_picker.rs` | 修改 | 复用统一徽章与分类显示 |
| `build.rs` | 修改 | 生成 `soundbank_assets.rs`，把音频内嵌进可执行文件 |
| `assets/soundbank/` | 新增 | 清单 `manifest.json` 与 128 件乐器的 Ogg 采样 |
| `README.md`、`doc/技术文档.md`、`doc/关键常量.md`、`doc/框架工作流.md`、`doc/项目简介.md` | 修改 | 同步采样乐器的架构、常量、函数索引与用户可见说明 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 轨道乐器实例化 | 合成器 / 鼓机 / 原生建模乐器 / 插件 | 增加采样乐器分支，按清单 id 预解码分区 | 工程同步阶段解码并缓存采样 |
| 音频渲染 | 模型合成或插件处理 | 采样乐器按音高/力度选分区、变速变调渲染 | 128 件采样乐器可直接发声 |
| 构建 | 仅编译源码 | `build.rs` 生成内嵌音频表 | 可执行文件自带音色库，无外部依赖 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 采样乐器 voice 数 | 无 | `24` | `src/audio/synth/sampled.rs` | `MAX_VOICES` |
| 采样输出余量 | 无 | `0.55` + `tanh` | `src/audio/synth/sampled.rs` | `OUTPUT_HEADROOM` |
| 采样默认参数 | 无 | 音量 `0.85` / 音色 `1.0` / 起音 `0.002 s` / 释放 `0.28 s` / 力度 `0.7` | `src/core/model/sample_bank.rs` | `SampleSettings::default` |
| 内置采样乐器数 | 无 | `128` | `assets/soundbank/manifest.json` | 附录二全部采样乐器 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 4.5 内置乐器补充采样乐器 |
| `doc/关键常量.md` | 是 | 采样乐器常量行 |
| `doc/框架工作流.md` | 是 | `sample_bank.rs`、`sampled.rs` 函数索引 |
| `doc/项目简介.md` | 是 | 2.5 内置乐器与原生效果器 |
| `doc/代码规范.md` | 否 | 规范本身无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 基线代码已格式化 |
| `cargo test` | 通过 | 采样乐器相关测试包含在本次基线中 |
| 手工验证 | 未执行 | 由用户在构建版中试听采样乐器 |

## 8. 风险与后续

- 风险：`assets/soundbank/` 约 13.7 MB 随仓库分发，仓库体积增加。
- 后续：在基线上增加神经网络音色模型，改善采样乐器的“生硬感”。
