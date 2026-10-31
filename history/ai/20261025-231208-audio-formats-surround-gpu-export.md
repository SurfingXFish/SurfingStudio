# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-25 23:12:08 (Asia/Shanghai) |
| AI 工具/模型 | TraeCode / 未标注模型 |
| 任务来源 | 增加主流音频导入格式、多通道/环绕声路由、实时播放 GPU 加速，并把导出改为独立窗口（格式/参数/路径） |
| 关联分支/提交 | 未知 / 未知 |
| 影响模块 | core, audio, app, ui, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 增加 MP3 / FLAC / AAC·M4A / OGG Vorbis / AIFF / CAF / ALAC 等主流音频格式导入。
- 提供工程级 2.0 / 5.1 / 7.1 输出布局与每轨通道对路由，并在设备声道不足时自动下混。
- 为实时播放增加可选的 GPU 主总线软限幅，超时/失败自动回退 CPU。
- 把导出改为独立窗口，可选择 WAV/AIFF、位深、采样率、声道布局、范围、尾长、GPU 与导出路径，并在后台渲染写盘。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `Cargo.toml` | 修改 | 增加 `symphonia 0.6`（纯 Rust 解码；aac/aiff/alac/caf/flac/isomp4/mkv/mp3/ogg/pcm/vorbis/wav） |
| `Cargo.lock` | 修改 | 依赖解析结果 |
| `src/audio/io/mod.rs` | 修改 | 注册 wav/symphonia/aiff 后端；新增 `AudioBitDepth`、`AudioEncodeOptions`、`export_with_options`、`export_project_with_options`、`supports_import`、`accepts_audio_file`、`build_peaks`、`split_stereo` |
| `src/audio/io/wav/mod.rs` | 修改 | 解码支持 8/16/24/32-bit 与 Float，多声道下混；导出支持 16/24-bit PCM、32-bit 浮点与任意声道数 |
| `src/audio/io/aiff/mod.rs` | 新增 | AIFF 只写后端：FORM/AIFF、COMM、SSND、80 位采样率、16/24-bit PCM |
| `src/audio/io/symphonia/mod.rs` | 新增 | 主流压缩/容器格式解码（含 MKV/WebM 音频轨）、时长读取、单包错误跳过与多声道下混 |
| `src/core/model/channel.rs` | 新增 | `ChannelLayout`（2.0 / 5.1 / 7.1）、声道数/通道对/名称与持久化标识 |
| `src/core/model/mod.rs` | 修改 | 声明并导出 `channel` 模块 |
| `src/core/model/project.rs` | 修改 | 新增 `output_layout`（追加在结构末尾，serde 默认立体声） |
| `src/core/model/track.rs` | 修改 | 新增 `output_pair`（追加在结构末尾，serde 默认 0） |
| `src/core/project_runtime/soundbug/import.rs` | 修改 | SoundBug 导入工程补齐 `output_layout` |
| `src/audio/engine/mod.rs` | 修改 | 主总线多声道缓冲、布局同步、`configure_gpu_realtime` / `gpu_realtime_active` / `take_gpu_realtime_warning` |
| `src/audio/engine/render.rs` | 修改 | `process_output` 多声道入口、按 `output_pair` 路由、设备声道映射、`OfflineRenderOptions` / `RenderRange`、多声道离线渲染、实时 GPU 限幅与 CPU 回退、相关测试 |
| `src/audio/gpu/mod.rs` | 修改 | `GPU_REALTIME_TIMEOUT`、`GpuRealtimeLimiter` 运行时封装与 `open_realtime_limiter` |
| `src/audio/gpu/backends/wgpu_compute.rs` | 修改 | 新增常驻 `WgpuRealtimeLimiter`（设备/管线/缓冲复用，限时提交与回读） |
| `src/audio/backend.rs` | 修改 | `AudioOptions.output_channels`、cpal 声道协商、多声道断流淡入淡出与 `process_output` 调用 |
| `src/app/export.rs` | 新增 | 导出状态、格式/布局选项、默认路径、后台路径选择、后台渲染写盘与结果轮询 |
| `src/ui/export.rs` | 新增 | 独立导出视口/模态面板、参数表单、自绘标题栏与路径选择 |
| `src/app/mod.rs` | 修改 | 注册导出模块、字段与初始化、每帧轮询 `poll_export_dialog` / `poll_export`、绘制导出窗口 |
| `src/app/files.rs` | 修改 | `export_wav_dialog` 改为打开导出窗口；`default_project_dir` 提升为 `pub(super)` |
| `src/app/transport.rs` | 修改 | 同步/播放/后台同步结束时应用实时 GPU 配置；状态栏显示 GPU 回退原因 |
| `src/app/settings.rs` | 修改 | 新增 `audio_output_channels`、`gpu_audio_realtime_enabled`、`gpu_realtime_config`、加载校验、音频页输出声道与渲染页实时 GPU 开关 |
| `src/app/panels.rs` | 修改 | 文件菜单改为「导出音频…」 |
| `src/app/shortcuts.rs` | 修改 | 导出动作名称改为「导出音频」 |
| `src/ui/mod.rs` | 修改 | 声明 `export` 视图模块 |
| `src/ui/inspector.rs` | 修改 | 轨道「输出通道」下拉与主输出「输出布局」下拉（布局变化时夹取通道对并标记脏） |
| `src/ui/arrangement/mod.rs` | 修改 | 拖放音频改用 `AudioRuntime::accepts_audio_file`，提示支持的格式 |
| `doc/项目简介.md` | 修改 | 导入格式、环绕声路由、GPU 实时、独立导出窗口、技术栈与限制 |
| `doc/技术文档.md` | 修改 | 模块表、数据模型、每块流程、离线渲染/GPU、音频文件、导出窗口与已知限制 |
| `doc/关键常量.md` | 修改 | 新增布局/通道对、音频后端、导入下混、GPU 实时、导出窗口与声道设置常量 |
| `doc/框架工作流.md` | 修改 | 模块地图、文件索引、实时管线、导出工作流与快速入口 |
| `doc/日志规范.md` | 修改 | 新增 `project.export_window` 事件 |
| `doc/代码规范.md` | 修改 | 实时 GPU 有界等待规则与更新后的混音链路 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 音频导入 | 仅 WAV；多声道取前两声道 | 按扩展名分派 wav → symphonia → aiff；多声道按 ITU 系数下混 | 主流格式可直接拖入 |
| 实时渲染输出 | `Engine::process(out_l, out_r)` 固定立体声 | `Engine::process_output(&mut [Vec<f32>])` 按工程布局渲染并映射到设备声道 | 支持 2.0 / 5.1 / 7.1 与设备下混 |
| 轨道混音目标 | 直接累加到 master L/R | 按 `Track::output_pair` 累加到主总线通道对 | 轨道可路由到环绕通道 |
| 主效果器 | 处理整个主总线 | 固定处理前 L/R，其余通道直通 | 避免 2 in/2 out 插件逐对复用串扰 |
| GPU 音频 | 仅离线导出 | 离线导出 + 可选实时主总线限幅（1 ms 有界等待，CPU 回退） | 实时播放可利用显卡，且不阻塞音频线程 |
| 导出 | 文件菜单选择路径后同步导出 48k/16-bit/立体声 | 独立导出窗口 + 后台渲染写盘，可选格式/位深/采样率/声道/范围/尾长/GPU/路径 | 导出参数完整、UI 不冻结 |
| 工程格式 | 无输出布局字段 | `Project::output_layout` / `Track::output_pair` 追加在结构末尾 | 旧 `.sfsd` 按默认立体声/通道对 0 解码 |
| 设置 | 采样率/缓冲/GPU 离线开关 | 新增输出声道与实时 GPU 开关 | 设备声道协商与实时加速可配置 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `PEAK_BINS` | `src/audio/io/wav/mod.rs` 私有 | `src/audio/io/mod.rs` 公开 `512` | `src/audio/io/mod.rs` | WAV / Symphonia 共用 |
| 音频后端注册顺序 | `wav` | `wav` → `symphonia` → `aiff` | `src/audio/io/mod.rs` | AiffCodec 只写 |
| `GPU_REALTIME_TIMEOUT` | 无 | `1` ms | `src/audio/gpu/mod.rs` | 实时 GPU 等待上限 |
| `REALTIME_CAPACITY_SAMPLES` | 无 | `32_768`（8 声道 × 4096 帧） | `src/audio/gpu/backends/wgpu_compute.rs` | 实时限幅缓冲容量 |
| `EXPORT_WINDOW_TITLE` / 尺寸 / `EXPORT_TITLE_H` | 无 | `导出音频 · SurfingStudio` / `580×560`（最小 `480×440`）/ `38` 点 | `src/ui/export.rs` | 导出窗口 |
| 导出采样率选项 | 无（固定 48 kHz） | `44_100 / 48_000 / 88_200 / 96_000 / 192_000` | `src/ui/export.rs` | 导出窗口 |
| 输出声道 `audio_output_channels` | 无（固定设备默认） | 默认 `0`；可选 `2/6/8` | `src/app/settings.rs` | 设备声道协商 |
| GPU 实时开关 `gpu_audio_realtime_enabled` | 无 | 默认关闭 | `src/app/settings.rs` | 实时 GPU |
| `ChannelLayout` 默认 / `output_pair` 默认 | 无 | 立体声 / `0` | `src/core/model/` | 工程与轨道新字段 |
| WAV/AIFF 导出 | 48 kHz / 2 ch / 16-bit | 16/24-bit PCM 或 32-bit 浮点；AIFF 16/24-bit | `src/audio/io/` | 按请求写头 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 模块职责、1.3 运行时映射、3.1/3.2 数据模型、4.2 每块流程、4.8 离线渲染与 GPU、4.9 音频设备、8 音频文件、9.4 导出、12.2 已知限制 |
| `doc/关键常量.md` | 是 | 第 1、2、3、8 节新增/更新布局、后端、GPU 实时、导出窗口与声道常量 |
| `doc/框架工作流.md` | 是 | 1.3/1.4 实时管线、1.11 导出工作流、2 模块地图、3 文件索引、4 快速入口 |
| `doc/项目简介.md` | 是 | 2.1 路由、2.3 导入格式、2.6 设置/GPU/导出、3.1 渲染与 GPU、4 技术栈、6 目录、7 已知限制 |
| `doc/代码规范.md` | 是 | 5.2 新增实时 GPU 有界等待规则，5.4 更新混音链路 |
| `doc/日志规范.md` | 是 | 5.3 新增 `project.export_window` |
| `README.md` | 是 | 专业能力、音频片段格式、环绕声路由、独立导出窗口、技术栈与已知限制 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已执行，无残留格式差异 |
| `cargo test` | 通过 | `cargo test --lib`：69 通过，0 失败，16 忽略 |
| 手工验证 | 部分执行 | 单元测试覆盖 AIFF 写入→Symphonia 解码、5.1 WAV 24-bit 写头、5.1 轨道路由到环绕对与离线多声道渲染；另用 ffmpeg 生成 1 秒正弦 MP3 / FLAC / M4A(AAC) / ADTS AAC / OGG Vorbis / AIFF，逐个通过 `SURFING_TEST_AUDIO` 环境变量驱动解码测试并全部通过；本仓库 `examples/…/*.mp3` 实为 MKV/WebM + AAC，启用 `mkv` 特性后也通过解码测试；未在真实多声道声卡上试听环绕输出，未验证实时 GPU 在实际显卡上的限幅时延 |

## 8. 风险与后续

- 风险：实时 GPU 限幅在性能不足的显卡上会频繁超时并回退 CPU（功能正确但无加速收益）；每个工程窗口在启用实时 GPU 时各自创建设备，多个重型工程可能增加显存占用。环绕声实际听感依赖设备是否声明 6/8 声道；ASIO 路径仍按左右声道写出。
- 后续：为实时 GPU 增加“上次耗时/命中率”遥测与自动关闭；支持 Opus / WMA 解码；为 AIFF 增加浮点编码或改用可写后端；补充真实 MP3/AAC 素材的集成测试。
