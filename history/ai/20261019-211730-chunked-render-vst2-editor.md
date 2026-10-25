# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-19 21:17:30 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 分块渲染 MIDI 片段、改善预渲染卡顿、修复效果器卷帘 GUI、支持 VST1/VST2 乐器 GUI |
| 关联分支/提交 | 未知 |
| 影响模块 | render, audio, plugin, app, ui, docs |
| 关联记录 | 无 |

## 2. 任务目标

- MIDI 片段改为分块渲染；播放头所在分块已渲染时直接播放，到未渲染分块边界再等待。
- 保留预渲染机制并改善预渲染音频的卡顿与启动延迟。
- 修复效果器卷帘中插件 GUI 的渲染位置/尺寸，以及关闭卷帘后 GUI 残留。
- 支持 VST1/VST2 乐器的原生 GUI 打开。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/render/mod.rs` | 修改 | `ClipAudio` 增加 `chunk_index`；新增 `ClipChunkLayout` 分块布局与 `RENDER_CHUNK_SECONDS` / `RENDER_PREROLL_SECONDS`；`WaitRequest` 增加分块序号；测试改为安装多个分块并新增“在首个未渲染分块边界等待”测试 |
| `src/render/offline.rs` | 修改 | 新增 `render_clip_chunks` 流式分块渲染；`render_clip` 收集全部分块 |
| `src/render/manager.rs` | 修改 | 条目改为分块安装位图；工作线程与主线程都逐块安装；等待项按分块入队；播放中为内置乐器轨预取分块；`prepare_for_playback` 只排入播放头附近分块 |
| `src/audio/engine/render.rs` | 修改 | `ClipRenderJob` 支持从指定分块开始、预卷、逐块交付；新增 `begin_clip_render_at` / `take_ready_chunks`；`finish_clip_render` 返回剩余分块 |
| `src/audio/engine/mod.rs` | 修改 | 缓存键改为 `(track, clip, chunk)`；`install_clip_cache` 按分块移除等待项；新增 `playback_ready_at` |
| `src/audio/engine/playback.rs` | 修改 | `missing_caches` 改为 `cache_status`，返回缺失分块与可连续播放边界；缓存读取改为就近采样，消除跨块插值抖动 |
| `src/app/transport.rs` | 修改 | `toggle_play` 只预渲染播放头附近分块，播放头分块就绪即开始 |
| `src/app/mod.rs` | 修改 | 关闭/切换效果器卷帘时销毁嵌入编辑器；`Drop` 先关闭插件编辑器；播放等待改为检查播放头分块就绪 |
| `src/app/plugins.rs` | 修改 | 新增 `EditorWindow` 枚举；`open_editor` 支持 VST1/VST2 独立窗口、隐藏窗口再次打开；`service_editors` 驱动两类窗口 |
| `src/app/input.rs` | 修改 | Esc 只关闭可见的插件编辑器窗口 |
| `src/app/panels.rs` | 修改 | 等待提示由“个片段”改为“个分块” |
| `src/ui/effects.rs` | 修改 | 嵌入编辑器坐标按平台换算为物理像素；接受尺寸换算回卡片点数；轨道缺失或面板过小时释放嵌入编辑器；浮动卷帘不再清理底部卷帘的嵌入编辑器 |
| `src/plugin/editor.rs` | 新增 | VST1/VST2 独立 Win32 编辑器窗口：创建、关闭、再次打开、idle 驱动 |
| `src/plugin/mod.rs` | 修改 | 注册并导出 `Vst2EditorWindow`；新增（ignored）VST2 fixture 编辑器窗口测试 |
| `tools/vst2-fixture/src/lib.rs` | 修改 | 测试乐器实现极简 `Editor`，验证宿主 VST2 窗口与 `open` 路径 |
| `src/plugin/runtime.rs` | 修改 | `has_editor` 支持 VST2；新增 `take_editor` 取出 VST1/VST2 编辑器 |
| `src/audio/engine/plugins.rs` | 修改 | 新增 `take_plugin_editor` |
| `Cargo.toml` | 修改 | Windows 目标增加 `winapi` 直接依赖（编辑器窗口 API） |
| `doc/技术文档.md` | 修改 | 更新分块缓存/等待、分块渲染任务、播放预渲染、VST1/VST2 编辑器窗口与嵌入 GUI 坐标系 |
| `doc/关键常量.md` | 修改 | 新增分块时长、预卷、播放中预取窗口、VST2 编辑器尺寸等常量 |
| `doc/框架工作流.md` | 修改 | 更新渲染/插件/应用函数索引与缓存渲染、播放等待流程 |
| `doc/项目简介.md` | 修改 | 更新分块预渲染、卡顿改善、VST1/VST2 GUI 与已知限制 |
| `doc/代码规范.md` | 修改 | 修正 `EditorRect` 坐标系规则，补充 `plugin/editor.rs` 与编辑器类型 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 片段缓存粒度 | 每个 MIDI 片段一整段音频 | 每个片段按 2 秒分块，逐块安装 | 首块完成即可播放，减少整段渲染等待 |
| 播放等待 | 本块任一 MIDI 片段缺缓存就暂停在块起点 | 先播放到首个缺失分块边界再暂停 | 已渲染内容不被浪费 |
| 主线程插件渲染 | 任务完成时一次性产出整段缓存 | 每帧分片后立即安装已完成分块 | 播放头分块可以更早可用 |
| 播放前预渲染 | 等待所有主线程片段完成或 1.5 秒超时 | 只等待播放头附近分块就绪 | 启动更快 |
| 缓存读取 | 线性插值取样 | 同时间基下就近采样 | 消除跨块浮点插值抖动 |
| VST1/VST2 编辑器 | 只提示“不是 VST3”，无法打开 | 打开独立原生窗口，关闭后保留对象可再次打开 | 乐器 GUI 可用 |
| 效果器卷帘关闭 | 原生子窗口残留在界面上 | 关闭/切换视图时立即销毁 | 消除 GUI 残留 |
| 嵌入编辑器坐标 | 逻辑点直接交给原生子窗口 | Windows/X11 乘 `pixels_per_point`，macOS 保持点数 | 修复高 DPI/缩放下的错位与偏小 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `RENDER_CHUNK_SECONDS` | 无 | `2.0` 秒 | `src/render/mod.rs` | 单块渲染时长 |
| `RENDER_PREROLL_SECONDS` | 无 | `2.0` 秒 | `src/render/mod.rs` | 中间块开始时的状态预卷 |
| `RENDER_PREROLL_MAX_SECONDS` | 无 | `8.0` 秒 | `src/render/mod.rs` | 跨块长音延长预卷的上限 |
| `PLAYBACK_PREFETCH_SECONDS` | 无 | `4.0` 秒 | `src/render/manager.rs` | 播放中预取窗口 |
| VST2 编辑器回退尺寸 | 无 | `860 × 620` 点 | `src/plugin/editor.rs` | 插件未声明尺寸时的客户区 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 4.3、5.1、5.2、5.3、5.4、6.6、7.5、10、12.2 |
| `doc/关键常量.md` | 是 | 3 渲染缓存、4 插件宿主、7 效果器链 |
| `doc/框架工作流.md` | 是 | 1.3、1.4、2、3.2、3.3、3.4、3.5、4、5.2 |
| `doc/项目简介.md` | 是 | 2.4、3.1、7 |
| `doc/代码规范.md` | 是 | 1.2、7.5、8.1 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | `cargo test --lib`：26 通过，5 个需要本机插件/原生窗口的测试被忽略 |
| 手工验证 | 通过 | 构建 `surf-vst2-fixture` 后运行 `cargo test --lib plugin::tests::opens_vst2_fixture_editor_window -- --ignored`：窗口创建、`editor.open` 与可见性断言通过 |

## 8. 风险与后续

- 风险：VST2 编辑器对象每个插件实例只能从宿主取一次；若通过 Esc/加载工程/撤销销毁窗口，需要重新同步插件实例后才能再次打开。
- 风险：含插件片段在播放到达未渲染分块时仍会暂停等待（按需求设计），主线程渲染在播放期间不推进。
- 后续：可在后续版本中为 VST2 编辑器增加效果器卡片内嵌入，或缓存编辑器对象以便完整关闭后仍能重新打开。
