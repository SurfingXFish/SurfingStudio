# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-01 23:05:00 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 内置采样乐器听感生硬，要求引入机器学习 / 神经网络改善 |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | core, audio, ui, docs |
| 关联记录 | `history/ai/20261101-225741-sampled-bank-baseline.md` |

## 2. 任务目标

- 用机器学习方法改善内置采样乐器的“假乐器”听感，而不是继续堆叠手工 DSP 规则。
- 不引入外部推理依赖、不联网、不在实时回调中分配内存。
- 保持根音上的原始采样音色不被改动，并提供可关闭的强度开关。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/audio/synth/neural.rs` | 新增 | 神经音色模型：4→10→10→6 MLP、基 2 FFT 特征提取、Adam 训练、按音色 id 缓存、逐 voice 三段 RBJ 整形器与确定性抖动 |
| `src/audio/synth/sampled.rs` | 修改 | voice 增加神经音色状态；`note_on` 与每个 block 推理一次并更新整形器；逐采样串接整形器与逐音符音量抖动；`load_sample` 提升为 `pub(super)` 供训练复用 |
| `src/audio/synth/mod.rs` | 修改 | 注册 `neural` 子模块并更新文件头 |
| `src/core/model/sample_bank.rs` | 修改 | `SampleSettings` 新增 `neural` 字段（`serde(default)`）与 `default_neural_amount`；更新文件头函数清单 |
| `src/ui/inspector.rs` | 修改 | 采样乐器检查器新增「神经音色」滑杆 |
| `doc/技术文档.md` | 修改 | 1.2 模块表补充 `neural.rs`；4.5 新增「SampledInstrument 与神经音色模型」小节；修正内置采样库的描述 |
| `doc/关键常量.md` | 修改 | 新增网络结构、分析窗、频段边界、训练超参、参考点、增益换算、整形器、抖动与最少训练帧常量 |
| `doc/框架工作流.md` | 修改 | 模块地图、`sampled.rs` 函数表与新增 `neural.rs` 函数表 |
| `doc/项目简介.md` | 修改 | 2.5 新增「神经音色」用户可见说明，并补充检查器滑杆 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 采样乐器实例化 | 只解码分区 | 解码分区后按音色 id 取神经模型（首次训练并缓存） | 首次加载多一次训练开销，之后命中缓存 |
| 音符开启 | 只分配 voice 与步进 | 额外推理一次音色增益并写入整形器，同时生成逐音符抖动 | 起音第一个 block 就有修正 |
| 每个音频块 | 逐采样读取采样 | 块首对每个发声 voice 推理一次，块边界重算滤波器系数 | 实时开销按块计，不随采样数放大 |
| 逐采样渲染 | 低通后直接混音 | 低通 → 神经整形器 → 包络/力度/抖动增益 | 关闭神经音色（0）时整形器为直通 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `SampleSettings::neural` | 无 | `0.7` | `src/core/model/sample_bank.rs` | 默认强度；旧工程缺字段时取默认值 |
| `FEATURE_COUNT` / `HIDDEN_1` / `HIDDEN_2` / `BAND_COUNT` | 无 | `4` / `10` / `10` / `6` | `src/audio/synth/neural.rs` | 网络结构 |
| `FFT_SIZE` / `ANALYSIS_POSITIONS` | 无 | `256` / `8%·25%·45%·70%` | `src/audio/synth/neural.rs` | 训练特征分析窗 |
| `BAND_EDGES` | 无 | `1/3/6/12/24/48/96` | `src/audio/synth/neural.rs` | 6 段对数频段边界 |
| `TRAIN_EPOCHS` / `ADAM_*` / `TRAIN_DECAY` | 无 | `700` / `0.012`·`0.9`·`0.999`·`1e-8` / `1e-4` | `src/audio/synth/neural.rs` | Adam 训练超参数 |
| `REFERENCE_PITCH` / `REFERENCE_VELOCITY` / `REFERENCE_TIME` | 无 | `60` / `0.62` / `0.35` s | `src/audio/synth/neural.rs` | 该点修正量为 0 |
| `TIME_SCALE` / `BAND_TO_DB` / `MAX_GAIN_DB` | 无 | `0.8` s / `4.0` / `9.0` dB | `src/audio/synth/neural.rs` | 时间特征与增益换算 |
| `LOW_SHELF_HZ` / `PEAK_HZ` / `HIGH_SHELF_HZ` / `PEAK_Q` / `SHELF_Q` / `GAIN_SMOOTHING` | 无 | `220` / `1400` / `5200` Hz / `0.9` / `0.7071` / `0.35` | `src/audio/synth/neural.rs` | 三段整形器 |
| `JITTER_DB` / `JITTER_LEVEL_DB` / `MIN_TRAINING_FRAMES` | 无 | `0.9` dB / `0.3` dB / `8` | `src/audio/synth/neural.rs` | 抖动与训练下限 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 模块表、4.5 神经音色模型小节 |
| `doc/关键常量.md` | 是 | 内置乐器常量区新增神经音色行 |
| `doc/框架工作流.md` | 是 | 1.x 模块地图、`sampled.rs` 与 `neural.rs` 函数表 |
| `doc/项目简介.md` | 是 | 2.5 神经音色说明与检查器参数 |
| `doc/代码规范.md` | 否 | 规范本身无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 `cargo fmt --all` |
| `cargo test` | 通过 | 含新增神经网络与采样乐器测试 |
| 手工验证 | 未执行 | 需在构建版中试听并调整「神经音色」滑杆 |

## 8. 风险与后续

- 风险：模型在首次加载某音色时训练并缓存（单次数百毫秒），重型工程首次加载该音色会有一次额外耗时。
- 风险：频谱修正依赖采样库自身的多音高分区，分区过少或音量过低的音色可学习信号有限，抖动与修正幅度会随训练残差自动减小。
- 后续：可把同一模型推广到原生建模乐器，并为力度层更完整的音色补充力度维度训练数据。
