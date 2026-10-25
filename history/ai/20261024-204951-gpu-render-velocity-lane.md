# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-24 20:49:51 (Asia/Shanghai) |
| AI 工具/模型 | Trae / AI 编码助手 |
| 任务来源 | 删除力度自动化子轨并在钢琴卷帘增加力度控制器开关；增加界面 GPU 渲染加速设置；增加 GPU 音频渲染设置与实现 |
| 关联分支/提交 | 未知 |
| 影响模块 | core, audio, app, ui, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 删除力度控制器（自动化子轨），改为钢琴卷帘右上角开关控制的底部力度控制器，排列与参考图一致。
- 在设置 → 界面中增加 GPU 渲染加速开关与图形 API 下拉列表。
- 在设置中新增渲染分类，可选择是否使用 GPU 加速音频渲染、使用的 GPU、驱动与 API。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/core/model/controller.rs` | 修改 | `ControllerKind::ALL` 移除 `Velocity`，只保留音量/声像/延音/弯音；变体保留用于旧工程兼容 |
| `src/ui/controller.rs` | 修改 | 文件头说明同步为四类控制器 |
| `src/ui/piano_roll/mod.rs` | 修改 | 新增底部力度控制器绘制、悬停读数、命中与拖动改写力度；工具条右上角增加「力度」开关；主网格/键盘/命中区域收缩到控制器上方；补齐双语文件头与函数头 |
| `src/ui/piano_roll/helpers.rs` | 修改 | 新增 `VELOCITY_LANE_H` / `VELOCITY_LANE_PAD` / `VELOCITY_KNOB_R` / `VELOCITY_HIT_W` |
| `src/ui/theme.rs` | 修改 | 新增可复用的 `toggle_switch`（28×14 开关），补双语文件头 |
| `src/app/mod.rs` | 修改 | 新增 `piano_velocity_open` 与 `settings_gpu_devices` 字段及初始化；re-export GPU 渲染启动类型 |
| `src/app/settings.rs` | 修改 | 新增 GPU 界面渲染与 GPU 音频渲染字段、默认值与归一化；新增 `GpuRenderApi` / `GpuRenderStartup` / `gpu_render_startup` / `gpu_audio_config`；新增「渲染」页与界面页 GPU 分区；新增 GPU 设备枚举缓存 |
| `src/app/files.rs` | 修改 | 载入工程时清理旧工程的力度子轨；WAV 导出改用 `export_project_with_gpu` 并显示 GPU 回退提示 |
| `src/main.rs` | 修改 | 按启动设置选择 glow 或 wgpu（DX12/Vulkan），先探测 API 是否有可用 GPU，失败回退 glow |
| `Cargo.toml` | 修改 | 新增 `wgpu = "29"`、`pollster = "0.4"`（与 eframe 的 wgpu 版本一致） |
| `src/audio/mod.rs` | 修改 | 声明并 re-export `gpu` 模块 |
| `src/audio/gpu/mod.rs` | 新增 | GPU 渲染运行时：`GpuApi`、`GpuDeviceInfo`、`GpuRenderConfig`、`GpuBackend`、`GpuRuntime` |
| `src/audio/gpu/backends/mod.rs` | 新增 | 后端注册 |
| `src/audio/gpu/backends/wgpu_compute.rs` | 新增 | wgpu 计算后端：设备/驱动/API 枚举、WGSL 软限幅、分块上传与回读、忽略测试 |
| `src/audio/engine/mod.rs` | 修改 | `Engine` 新增 `defer_master_clip`；导出 `OfflineRender` |
| `src/audio/engine/render.rs` | 修改 | 主总线在 GPU 导出时只乘主音量；新增 `render_offline_with` 与 `OfflineRender`，失败回退 CPU 软限幅 |
| `src/audio/io/mod.rs` | 修改 | 新增 `export_project_with_gpu`，保留 `export_project` 兼容包装 |
| `doc/技术文档.md` | 修改 | 架构、控制器、离线渲染、钢琴卷帘、控制器卷帘、设置、限制与文件索引 |
| `doc/关键常量.md` | 修改 | 力度控制器尺寸、GPU 分块/拐点/工作组、GPU 设置默认值、设置分类 |
| `doc/框架工作流.md` | 修改 | 启动流程、模块地图、函数索引、离线导出流程与快速入口 |
| `doc/项目简介.md` | 修改 | 力度控制器、控制器类别、设置八类、GPU 加速、限制与目录结构 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 力度编辑 | 轨道控制器子轨（力度自动化） | 钢琴卷帘底部力度控制器，按音符编辑 | 力度自动化子轨从菜单/检查器/卷帘消失；旧工程载入时清理残留力度子轨 |
| 界面渲染启动 | 固定 `Renderer::Glow` | 按设置选择 glow 或 wgpu（DX12/Vulkan），启动前探测可用 GPU | 修改设置后需重启；探测失败保持 OpenGL |
| WAV 导出 | `render_offline` 全 CPU | `render_offline_with`：主总线增益后由 GPU 执行软限幅，失败回退 CPU | 导出听感不变；状态栏可显示回退原因 |
| 设置页面 | 七类 | 八类（新增渲染） | 新增 GPU 设备/驱动/API 选择与状态 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `VELOCITY_LANE_H` | 无 | `84.0` 点 | `src/ui/piano_roll/helpers.rs` | 力度控制器高度 |
| `VELOCITY_LANE_PAD` | 无 | `8.0` 点 | `src/ui/piano_roll/helpers.rs` | 控制器上下留白 |
| `VELOCITY_KNOB_R` | 无 | `3.0` 点 | `src/ui/piano_roll/helpers.rs` | 力度圆点半径 |
| `VELOCITY_HIT_W` | 无 | `8.0` 点 | `src/ui/piano_roll/helpers.rs` | 命中容差 |
| `CHUNK_SAMPLES` | 无 | `1 << 20` 样本 | `src/audio/gpu/backends/wgpu_compute.rs` | GPU 单次提交上限 |
| `DEFAULT_GPU_RENDER_API` | 无 | `auto` | `src/app/settings.rs` | GPU 界面渲染默认 API |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 模块职责、1.3 运行时表、3.4 控制器、4.2/4.8 渲染与导出、7.3/7.4/7.10 UI 与设置、12.2 限制、附录 B |
| `doc/关键常量.md` | 是 | 3 渲染与导出、7 钢琴卷帘、8 主题与窗口（设置/GPU 默认值） |
| `doc/框架工作流.md` | 是 | 1.1 启动、1.4 离线导出、2 模块地图、3.2/3.4/3.5 函数索引、4 快速入口 |
| `doc/项目简介.md` | 是 | 2.2 MIDI 编辑、2.5 原生乐器、2.6 工作流、3.1/3.5、6 目录结构、7 已知限制 |
| `doc/代码规范.md` | 否 | 规范本身未变化；新增代码按其运行时+后端、双语注释与常量同步要求实现 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | 54 passed；16 ignored（依赖真实插件/GPU 的测试） |
| 手工验证 | 部分通过 | `wgpu_backend_limits_samples_on_a_real_gpu` 与 `gpu_offline_render_limits_the_master_bus` 在本机（NVIDIA RTX 4070 Ti SUPER，DX12/Vulkan）通过；设置窗口、力度控制器显示/隐藏与力度杆已在运行中的界面确认；本机 `%APPDATA%\SurfingStudio\settings.json` 为只读，无法验证 GPU 界面渲染开关的持久化与重启生效 |

## 8. 风险与后续

- 风险：wgpu 界面渲染路径为实验性（历史上大网格可能触发 staging buffer 问题），已限制为手动开启、启动前探测 GPU 并在失败时回退 glow。
- 风险：GPU 音频加速只覆盖离线导出的主总线软限幅，实时播放仍为 CPU；极端长工程会分块提交，吞吐受 GPU 回读带宽限制。
- 后续：如需实时 GPU 合成，需要设计无回读的驻留缓冲与低延迟同步策略，不能直接在当前音频回调内阻塞提交。
