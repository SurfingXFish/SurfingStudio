# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-19 23:20:00 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 修复问题 5（动态 GUI 不刷新/无法拖动）、SoundBug 工程加载崩溃、加载完成显示“就绪”时卡顿 |
| 关联分支/提交 | 未知 |
| 影响模块 | app, audio, render, plugin, ui, vendor/vst3-host, docs |
| 关联记录 | `history/ai/20261019-221500-plugin-sync-gui-soundbug-state.md` |

## 2. 任务目标

- 效果器子轨中的动态 GUI（Pro-Q 4 实时频谱）在播放时持续刷新，且可正常拖动。
- SoundBug 工程加载不再因 egui-wgpu 的 index staging buffer 崩溃。
- 加载提示显示“就绪”时 UI 不再卡住。
- 保证 UI 线程在任何插件渲染/音频处理期间都不会阻塞在引擎锁上。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/main.rs`、`Cargo.toml` | 修改 | 渲染后端改为 `eframe::Renderer::Glow`，规避 wgpu 大网格 staging buffer panic（SoundBug 导入后的崩溃栈） |
| `src/render/manager.rs` | 修改 | 新增 `ui_active`：UI 帧构建期间插件渲染线程让出引擎锁；`update` / `poll_results` 改用 `try_lock`；新增 `pending` 队列，引擎忙时分块安装延后到下一帧 |
| `src/app/mod.rs` | 修改 | 帧开始/结束设置 `render.set_ui_active`；待播放启动遇引擎忙时保持 pending（不再阻塞）；从嵌入编辑器推导 `monitored_effects` 并写回引擎 |
| `src/ui/effects.rs` | 修改 | 引擎锁忙（`engine_ready = false`）时保持现有嵌入编辑器并跳过 prune，避免每帧销毁重建；编辑器尺寸/可缩放查询改为可失败的非阻塞读取 |
| `src/audio/engine/mod.rs` | 修改 | 新增 `TrackEngine::monitored_effects` 与 `Engine::set_monitored_effects`；新增参数轮询游标 |
| `src/audio/engine/playback.rs` | 修改 | 缓存播放 MIDI 轨时把干声复制给打开 GUI 的效果器（输出丢弃），驱动分析器 |
| `src/audio/engine/plugins.rs` | 修改 | `poll_parameter_changes` 每次只轮询一条轨道；`plugin_editor_can_resize` 返回 `Option` |
| `src/plugin/runtime.rs` | 修改 | 新增非阻塞 `lock_plugin`；编辑器尺寸/可缩放/参数轮询使用 `try_lock`；`restore_state` 后立即刷新参数快照 |
| `vendor/vst3-host/src/embed.rs` | 修改 | 原生编辑器矩形未变化时跳过 `SetWindowPos`，避免打断插件内拖动 |
| `doc/技术文档.md` | 修改 | UI 让锁优先级、pending 安装、监控效果器、参数轮询与非阻塞编辑器查询、重定位去重 |
| `doc/关键常量.md` | 修改 | `ui_active`、参数轮询轮转、安装重试、编辑器重定位 |
| `doc/框架工作流.md` | 修改 | 缓存渲染线程模型、监控效果器、UI 帧锁优先级 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 插件渲染线程与 UI | 空闲预渲染几乎持续持有引擎锁，UI `try_lock` 长期失败 | UI 帧期间（`ui_active`）插件线程完全让锁 | 效果器 GUI 能嵌入并持续刷新，不再“显示但拖不动” |
| 嵌入编辑器 prune | 引擎锁忙时读不到插件句柄 → 每帧销毁并重建原生 GUI | 引擎忙时保持现状，跳过 prune | 修复动态 GUI 不刷新、无法拖动、闪断 |
| 分块安装 | UI 线程阻塞等待引擎锁 | 结果留在 `pending`，下一帧重试 | 加载/渲染期间 UI 不卡顿 |
| 参数轮询 | 每 200 ms 扫描全部轨道所有插件 | 每 tick 只轮询一条轨道（游标轮转） | 20+ 插件工程不再出现数百毫秒 UI 卡顿 |
| 缓存轨效果器 | 播放时不运行，打开 GUI 也收不到音频 | 干声送入被监控效果器（输出丢弃） | Pro-Q 等实时频谱随播放刷新 |
| 编辑器重定位 | 每帧 `SetWindowPos` | 仅在矩形变化时重定位 | 插件内鼠标拖动不被中断 |
| 状态恢复 | `load_state` 后参数快照可能滞后 | 恢复后立即刷新快照 | 避免首次全量扫描误判为用户编辑并触发多余同步 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| UI 让锁标记 | 无 | 帧开始置位 / 帧结束清除 | `src/render/manager.rs` | 插件渲染线程让出引擎锁 |
| 参数轮询粒度 | 全部轨道 | 1 条轨道 / tick | `src/audio/engine/plugins.rs` | 限制单帧工作量 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 2.1 线程表、2.2 锁策略、2.3 同步节流、6.4 参数、6.6 嵌入编辑器、6.7 本地 patch、7.1 每帧流程 |
| `doc/关键常量.md` | 是 | 3 渲染调度、5 App 节流、7 钢琴卷帘/效果器链 |
| `doc/框架工作流.md` | 是 | 1.3 播放与预渲染、1.4 缓存渲染 |
| `doc/项目简介.md` | 否 | 本次未改变产品定位与功能概述 |
| `doc/代码规范.md` | 否 | 本次未改变编码规范 |

## 7. 验证

- `cargo fmt`、`cargo check --all-targets`、`cargo test --lib`：27 passed / 6 ignored。
- 运行 SoundBug 示例（后台加载 20+ 插件）：同步完成 `warnings=0`，无 wgpu panic。
- Pro-Q 4 嵌入成功；`set_monitored_effects` 正常写入；UI 帧持续刷新。
- 加载完成瞬间仍有一次约 0.45 s 的首帧绘制耗时（Glow 首帧着色器/纹理上传），之后恢复流畅。

## 8. 已知限制

- Serum 2、MT-PowerDrumKit 在本机后台渲染其片段约 20 秒后会在插件模块内部崩溃（事件日志：`Serum2.vst3 0xc0000409`、`MT-PowerDrumKit.vst3 0xc0000005`）。属第三方插件在离线处理路径中的崩溃，宿主侧无法捕获；后续可考虑限制空闲插件渲染窗口或单独进程隔离。
- TDR Nova 在本机嵌入测试中 `IPlugView::attached()` 长时间不返回（编辑器创建于后台 STA 线程，附加调用在主线程）。Pro-Q 4 / Pro-DS 无此问题。
