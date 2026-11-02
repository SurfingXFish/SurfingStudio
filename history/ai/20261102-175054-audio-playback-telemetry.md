# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-02 17:50:54 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户反馈“开启效果器后声音延迟超过 0.5 s、VOCALOID 6 编辑器冻结，192 kHz 必须保留” |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | audio, plugin, diag, docs |
| 关联记录 | `history/ai/20261102-173634-high-rate-buffer-floor.md`、`history/ai/20261102-173137-plugin-transport-on-seek-only.md` |

## 2. 任务目标

- 补齐**播放期间**的音频证据：此前的日志只覆盖加载与空转，无法判断延迟来自设备缓冲、渲染超预算还是锁竞争。
- 修正两处会直接造成“缺块 / 不跟随走带”的实现缺陷，且不改变 192 kHz 使用方式。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/diag/mod.rs` | 修改 | 新增音频回调遥测：`note_audio_callback` / `note_plugin_lock_miss` / `flush_audio_stats`，全部为原子累加；UI 线程每 5 秒写一条 `audio.stats`（`rate`、`cb_frames`、`cb_ms`、`render_avg_us`、`render_max_us`、`over_budget`、`engine_lock_miss`、`plugin_lock_miss`、`out_latency_ms`、`out_latency_max_ms`）；`note_frame_duration` 负责节流刷新 |
| `src/audio/backend.rs` | 修改 | cpal 回调测量引擎渲染耗时与 `OutputCallbackInfo` 的 `playback - callback`（驱动报告的输出延迟），并登记到遥测；采样率按值捕获进回调 |
| `src/audio/asio.rs` | 修改 | 回调上下文记录驱动采样率并登记同一份遥测（ASIO 无 playback 时间戳，只记帧数与渲染耗时） |
| `src/plugin/runtime.rs` | 修改 | ① VST3 处理缓冲改为按**容量**复用（`clear` + `resize`），块长在 512/512/128 间变化时不再每块重建缓冲；② `set_playing` / `seek_transport` 先写入 `pending_playing` / `pending_transport`，锁被占用时由下一块 `process` 补发，不再静默丢弃走带状态；③ 插件锁被占用时累加 `plugin_lock_miss` |
| `src/plugin/backends/vst3.rs` | 修改 | 新建 `Vst3Runtime` 时初始化待发送字段 |
| `doc/技术文档.md` | 修改 | 2.2 实时缓冲复用按容量、新增音频遥测说明；6.6 补充“走带状态不丢帧”；13.1 说明遥测是日志的例外 |
| `doc/关键常量.md` | 修改 | 新增“音频统计窗口 5000 ms” |
| `doc/框架工作流.md` | 修改 | diag 函数表新增三个遥测函数 |
| `doc/项目简介.md` | 修改 | 新增“播放遥测”说明 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 播放期诊断 | 日志里没有任何播放期数据 | 每 5 秒一条 `audio.stats` | 可区分设备缓冲过长、渲染超预算、锁被抢走三类原因 |
| VST3 处理缓冲 | 块长变化就重建（每块多次堆分配） | 容量不足才扩容，其余时间复用 | 192 kHz 下减少实时线程分配抖动 |
| 插件走带状态发送 | 插件忙时静默丢弃 | 记为待发送，下一块补发 | VOCALOID 等单实例插件能真正收到播放状态与定位 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 音频统计窗口 | 无 | `5000` ms | `src/diag/mod.rs` | `flush_audio_stats` 的节流间隔（硬编码，与资源采样一致） |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 2.2 锁与缓冲；6.6 插件线程协作；13.1 日志模型 |
| `doc/关键常量.md` | 是 | 第 10 节 diag 常量表 |
| `doc/框架工作流.md` | 是 | `src/diag/mod.rs` 函数表 |
| `doc/项目简介.md` | 是 | 第 2 节音频/诊断条目 |
| `doc/代码规范.md` | 否 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo check --all-targets` | 通过 | 仅余既有告警 |
| `cargo test` | 通过 | 107 passed / 20 ignored |
| 手工验证 | 未执行 | 需用户用**当前构建**在 192 kHz 下播放并回传新的 `.sflog` |

## 8. 风险与后续

- 风险：遥测只做原子读写，不引入锁与分配；`AudioBuffers` 容量只增不减，单插件最多占用 `2 × 2 × 最大块长` 个 `f32`。
- 事实核对：用户上一次上传的日志（17:33:23）里设备描述没有帧数后缀，而该后缀是 `d97e0fe`（17:36 提交）才加入的——那份日志来自**修复前的二进制**，因此 6 ms 缓冲下限尚未被真正验证。
- 后续（依据新日志的 `audio.stats` 分支处理）：
  - `out_latency_ms` 本身就接近 500 ms → 问题在设备/驱动缓冲（WASAPI 共享模式或驱动返回的缓冲），需要按实际 `cb_frames` 与 `GetBufferSize` 结果调整打开方式，而不是继续改引擎。
  - `over_budget` 持续偏高且 `render_max_us > cb_ms` → 渲染超预算（192 kHz 下 21 个插件的 DSP 负载），需要降低实时线程负载或并行化。
  - `engine_lock_miss` / `plugin_lock_miss` 偏高 → 锁竞争，需要缩短 UI 侧持锁时间（如 `engine.sync` 的 167 ms 全量同步）。
