# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-01 23:39:39 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 希望在显卡上训练更大的神经音色模型，进一步提升内置乐器听感 |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | audio, app, docs |
| 关联记录 | `history/ai/20261101-230500-neural-timbre-model.md` |

## 2. 任务目标

- 把神经音色模型的训练搬到 GPU，使网络可以比 CPU 回退路径更大。
- 实时推理仍留在 CPU：实测其只占单核约 0.055%，逐块 GPU 提交会因固定开销变慢。
- GPU 不可用时必须无感回退 CPU，且不影响播放。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/audio/gpu/backends/wgpu_timbre.rs` | 新增 | GPU 训练器：显式绑定布局 + 五个计算通道（tick / forward / backward_1 / backward_2 / adam），一次提交跑完全部轮次；状态区与工作区两块存储缓冲 |
| `src/audio/gpu/backends/mod.rs` | 修改 | 注册 `wgpu_timbre` 后端模块 |
| `src/audio/gpu/backends/wgpu_compute.rs` | 修改 | 抽出 `create_compute_device`（实时限幅与训练共用），实时限幅改用它 |
| `src/audio/gpu/mod.rs` | 修改 | 新增 `GpuTimbreTrainer` 包装与 `GpuRuntime::open_timbre_trainer` |
| `src/audio/synth/neural.rs` | 修改 | 隐藏层 10→20；抽出 `normalise` / `fold_output` / `residual_of`；新增 `TinyMlp::flatten` / `from_flat` / `parameter_count`；`train` 支持传入 GPU 训练器并回退 CPU |
| `src/audio/synth/mod.rs` | 修改 | 导出 `HIDDEN_WIDTHS` 供 GPU 训练器建管线 |
| `src/audio/synth/sampled.rs` | 修改 | `SampledInstrument::new` 增加 GPU 训练器参数并透传给 `model_for` |
| `src/audio/engine/mod.rs` | 修改 | 新增 `gpu_timbre` 字段：与实时限幅共用配置打开/关闭，并在 `ensure_instrument` 中传入 |
| `doc/技术文档.md` | 修改 | 1.2 模块表、1.3 模块映射、4.5 训练路径与复杂度 |
| `doc/关键常量.md` | 修改 | 网络结构改为 4→20→20→6，新增 GPU 训练缓冲常量行 |
| `doc/框架工作流.md` | 修改 | GPU 模块地图、`gpu/mod.rs`、`wgpu_compute.rs` 与新增 `wgpu_timbre.rs` 函数表 |
| `doc/项目简介.md` | 修改 | 2.5 说明 GPU 训练与回退 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 神经音色训练 | 仅在 CPU 上用 Adam 训练 | 有 GPU 训练器时在显卡上训练（一次提交），否则 CPU | 网络可加大到 20×20，音色拟合更准 |
| GPU 设备生命周期 | 只为实时限幅创建 | 同一配置下同时创建实时限幅器与训练器，配置变化时一起重建 | 训练器设备只创建一次，按音色复用 |
| 训练失败 | 无此路径 | 设备/着色器失败时静默回退 CPU | 播放与音色不受影响 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `HIDDEN_1` / `HIDDEN_2` | `10` / `10` | `20` / `20` | `src/audio/synth/neural.rs` | 网络容量；参数量 208 → 646 |
| `HIDDEN_WIDTHS` | 无 | `(20, 20)` | `src/audio/synth/neural.rs` | 供 GPU 训练器建管线 |
| `MAX_SAMPLES` | 无 | `256` | `src/audio/gpu/backends/wgpu_timbre.rs` | 训练集上限，决定工作区偏移 |
| `MAX_EPOCHS` | 无 | `20000` | `src/audio/gpu/backends/wgpu_timbre.rs` | 单次训练轮数上限 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2、1.3、4.5 |
| `doc/关键常量.md` | 是 | 内置乐器常量区 |
| `doc/框架工作流.md` | 是 | GPU 模块地图、`gpu/mod.rs`、`wgpu_compute.rs`、`wgpu_timbre.rs` |
| `doc/项目简介.md` | 是 | 2.5 神经音色 |
| `doc/代码规范.md` | 否 | 规范本身无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 `cargo fmt --all` |
| `cargo test` | 通过 | 含 CPU 训练与实时推理测试 |
| `cargo test -- --ignored`（GPU 用例） | 通过 | 本机显卡上 `gpu_training_lowers_the_training_error`：训练集误差 0.238 → 0.081 |
| 手工验证 | 未执行 | 需在构建版中开启 GPU 音频加速后试听 |

## 8. 风险与后续

- 风险：开启 GPU 音频加速后，首次加载每个采样音色会占用一次显卡设备与数百毫秒训练时间；音色模型按 id 缓存，之后不再训练。
- 风险：极少数 Downlevel 设备不支持 20×20 所需的存储缓冲/工作组，训练器会创建失败并回退 CPU。
- 后续：可把同一 GPU 训练器用于原生建模乐器的参数标定。
