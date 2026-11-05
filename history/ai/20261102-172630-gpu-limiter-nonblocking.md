# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-02 17:26:30 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 开启 GPU 音频加速后，开启效果器整体发声变晚；VOCALOID6 编辑器卡死 |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | audio, plugin, docs |
| 关联记录 | `history/ai/20261102-171257-plugin-transport-sync.md` |

## 2. 任务目标

- 定位“开启效果器后整体发声变晚”：用户已开启设置 → 渲染中的实时 GPU 软限幅。
- 消除实时 GPU 限幅对音频回调的等待，避免叠加效果器后回调超出缓冲预算。
- 保留 GPU 加速能力：结果就绪时仍由 GPU 处理，未就绪即用 CPU。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/audio/gpu/mod.rs` | 修改 | 移除 `GPU_REALTIME_TIMEOUT`；新增 `GpuLimitOutcome { Applied, Busy, Failed }`；`GpuRealtimeLimiter::process` 改为非阻塞签名 |
| `src/audio/gpu/backends/wgpu_compute.rs` | 修改 | 实时限幅器新增 `pending` 回读通道；提交后只做 `PollType::Poll`，未就绪返回 `Busy`，晚到结果丢弃后解锁缓冲 |
| `src/audio/engine/render.rs` | 修改 | 按结果分支：`Applied` 用 GPU 结果；`Busy` 本块 CPU 软限幅并保留实例；`Failed` 才丢弃实例并提示 |
| `src/audio/engine/mod.rs` | 修改 | 更新 `configure_gpu_realtime` 文档，去掉 1 ms 等待描述 |
| `doc/代码规范.md` | 修改 | 5.2 第 8 条改为“禁止等待”，与实现一致 |
| `doc/关键常量.md` | 修改 | GPU 实时等待改为 `0`，删除 `GPU_REALTIME_TIMEOUT` |
| `doc/技术文档.md` | 修改 | 4.2、9.x GPU 路径说明改为非阻塞 |
| `doc/框架工作流.md` | 修改 | 实时链路、`GpuRealtimeLimiter` 与 `GpuLimitOutcome` 条目 |
| `doc/项目简介.md` | 修改 | GPU 加速相关三处用户可见说明 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 实时 GPU 限幅 | 每块最多等 1 ms，超时丢弃实例 | 提交后只轮询一次；未就绪 → 本块 CPU，保留实例 | 回调不再被显卡拖慢，效果器叠加时不会整体变晚 |
| GPU 未就绪 | 视为失败并禁用 GPU | 视为 `Busy`，下一块继续尝试 | GPU 仍可间歇接管限幅 |
| 读取回读缓冲 | 等待映射完成后立即读取 | `.pending` 存在期间不提交新块，晚到结果丢弃后解除 | 不会覆盖未读完的回读缓冲 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `GPU_REALTIME_TIMEOUT` | `1` ms | 删除 | `src/audio/gpu/mod.rs` | 实时路径不再等待 |
| `GpuLimitOutcome` | 无 | `Applied` / `Busy` / `Failed` | `src/audio/gpu/mod.rs` | 区分“未就绪”和“失败” |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 4.2、9.x GPU 约束 |
| `doc/关键常量.md` | 是 | GPU 常量区 |
| `doc/框架工作流.md` | 是 | 实时链路图、`gpu/mod.rs` 函数表 |
| `doc/项目简介.md` | 是 | 2.6、3.3、7 的 GPU 说明 |
| `doc/代码规范.md` | 是 | 5.2 第 8 条 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 `cargo fmt --all` |
| `cargo test` | 通过 | 107 passed / 20 ignored（GPU 用例需 `--ignored`） |
| 手工验证 | 未执行 | 需用户在开启 GPU 音频与效果器的构建版中试听确认 |

## 8. 风险与后续

- 风险：GPU 间歇接管时，相邻块分别由 CPU / GPU 处理；两者使用同一 `soft_clip` 公式，听感一致。
- 后续：若仍有卡顿或延迟，需按 `%LOCALAPPDATA%\SurfingStudio\logs` 的 `.sflog` 进一步定位（是否回调超时、插件阶段卡住）。
- 后续：VOCALOID6 编辑器卡死还需实机日志确认；本次已补走带同步，未覆盖其在 GPU 开启时的 WPF 渲染问题。
