# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-20 20:49:05 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 重写 VST 效果器模块：修复效果器链 GUI 闪烁与插件分析器无法显示；删除界面中的“实时渲染 / 实时”等文案 |
| 关联分支/提交 | 未知 |
| 影响模块 | plugin, ui, app, docs |
| 关联记录 | `history/ai/20261019-232000-ui-lock-priority-embedded-gui.md` |

## 2. 任务目标

- 修复效果器链卡片中原生 VST3 GUI 持续闪烁、无法正常渲染的问题。
- 让 Pro-Q 等插件在卡片 GUI 中的实时频谱/音高分析持续刷新，而不是被主窗口重绘清空。
- 删除界面中“实时渲染 / 实时处理 / 实时”等无意义文字与轨道状态徽章。
- 保持效果器排序、删除、宽度拖拽、双击弹出独立窗口与参数回退面板等既有能力。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/embed.rs` | 新增 | 新增 `PluginEditorHost` / `EditorPlacement` / `EDITOR_CAN_HIDE`：Windows 使用无边框 `WS_POPUP` 宿主窗口承载 VST3 编辑器；主窗口安装 `WNDPROC` 跟随钩子（`WM_MOVE` / `WM_SIZE` / `WM_WINDOWPOSCHANGED` / `WM_SHOWWINDOW`）；按客户区物理矩形 → 屏幕坐标定位；矩形未变化时跳过 `SetWindowPos`；支持仅隐藏不销毁；Drop 先 `close_editor()` 再销毁窗口。非 Windows 平台继续封装 `vst3_host::EmbeddedEditor` |
| `src/plugin/mod.rs` | 修改 | 声明并导出 `embed` 模块的 `PluginEditorHost` / `EditorPlacement` / `EDITOR_CAN_HIDE` |
| `src/app/mod.rs` | 修改 | `embedded_editors` 类型改为 `PluginEditorHost`；移除 `EmbeddedEditor` 依赖；参数变化提示改为“插件参数已更新”；存在编辑器宿主时以 33 ms 间隔保持重绘，供宿主窗口跟随卡片 |
| `src/ui/effects.rs` | 重写 | 编辑器生命周期改为“按槽位持久维护”：卡片完整可见时创建/显示；滚出视野时 Windows 仅隐藏；引擎锁忙时保持现有宿主；不再每帧按可见性销毁重建。宽度拖拽手柄移到卡片外侧，避免被原生编辑器窗口吞掉鼠标事件。删除“信号从左到右处理”等无意义副标题 |
| `src/app/panels.rs` | 修改 | 移除状态栏“● 实时处理”文字与分隔线 |
| `src/ui/arrangement/mod.rs` | 修改 | 移除轨道“无片段 / 音频就绪 / 实时”状态徽章 |
| `doc/技术文档.md` | 修改 | 1.2 模块表新增 `plugin/embed.rs`；6.6 重写效果器卡片编辑器承载方案与闪烁根因；6.7 补充 Windows 不再依赖 `EmbeddedEditor`；7.1 状态栏去除“实时处理”；7.5 更新效果器链交互说明 |
| `doc/关键常量.md` | 修改 | 第 7 节更新宿主坐标换算、宿主窗口类、跟随消息、重定位去重、33 ms 重绘间隔与外侧宽度手柄 |
| `doc/框架工作流.md` | 修改 | 模块表、`plugin` 索引新增 `embed.rs`，更新 `ui_effects` 职责 |
| `doc/项目简介.md` | 修改 | 2.4 更新卡片内 VST3 GUI 的宿主方式与闪烁/分析器修复；7 更新卡片 GUI 支持范围表述 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 卡片原生 GUI 承载 | 插件编辑器作为 OpenGL 主窗口的 `WS_CHILD`，主窗口每帧重绘覆盖子 HWND | Windows 创建独立无边框 owner 宿主窗口，主窗口重绘不再覆盖 | 消除 GUI 闪烁；分析器可连续刷新 |
| 编辑器位置跟随 | 依赖 egui 每帧 `SetWindowPos`，窗口拖动时 GUI 脱节 | 主窗口 `WNDPROC` 钩子在移动/缩放/显示时重新定位全部宿主窗口 | 拖动、缩放、最小化恢复时 GUI 贴合卡片 |
| 卡片滚出视野 | 立即销毁编辑器，滚回时重新附加 | Windows 仅 `ShowWindow(SW_HIDE)`，滚回直接显示 | 不再重建 GUI、不丢失插件内部状态 |
| 引擎锁忙 | 读不到插件时可能误判“无编辑器” | 保持现有宿主并跳过释放 | 避免瞬时锁竞争导致 GUI 重建 |
| 宽度拖拽手柄 | 位于卡片右边缘内侧，被原生窗口覆盖 | 完全位于卡片外侧 6 点 | 原生 GUI 显示时仍可拖拽卡片宽度 |
| 界面文案 | 状态栏/轨道徽章显示“实时处理 / 实时” | 删除 | 界面不再出现无意义状态文字 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 效果器宿主窗口类 | 无 | `SurfingStudioPluginEditorHost` | `src/plugin/embed.rs` | 无边框 `WS_POPUP` + `WS_EX_TOOLWINDOW` |
| 效果器宿主跟随消息 | 无 | `WM_MOVE` / `WM_SIZE` / `WM_WINDOWPOSCHANGED` / `WM_SHOWWINDOW` | `src/plugin/embed.rs` | 主窗口几何变化时重新定位宿主 |
| 编辑器打开时重绘间隔 | 80 ms（空闲） | 33 ms | `src/app/mod.rs` | 宿主窗口跟随卡片布局 |
| 宽度拖拽手柄位置 | 卡片右侧 `-4 ~ +2` 点 | 卡片外侧 `0 ~ +6` 点 | `src/ui/effects.rs` | 避免被原生编辑器窗口吞掉鼠标事件 |
| 效果器宿主坐标换算 | Windows/Linux `× ppp` 嵌入子窗口 | 逻辑点 `× ppp` → 客户区物理像素 → `ClientToScreen` | `src/plugin/embed.rs` | 独立宿主窗口使用屏幕坐标 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 模块表、6.6 效果器卡片编辑器、6.7 本地 patch、7.1 每帧流程、7.5 效果器链 |
| `doc/关键常量.md` | 是 | 第 7 节效果器链常量 |
| `doc/框架工作流.md` | 是 | 2 模块地图、3.3 `plugin` 函数索引（新增 `embed.rs`）、`ui/effects.rs` 索引 |
| `doc/项目简介.md` | 是 | 2.4 插件、7 已知限制 |
| `doc/代码规范.md` | 否 | 本次未改变编码规范 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行，无格式差异 |
| `cargo test` | 通过 | `cargo test --lib`：23 passed / 0 failed / 5 ignored |
| `cargo clippy --all-targets` | 通过 | 无新增警告 |
| 手工验证 | 未执行 | 未启动桌面应用，未在真实插件（Pro-Q 等）上验证 GUI 刷新与窗口跟随 |

## 8. 风险与后续

- 风险：Windows 宿主窗口为独立顶层窗口，需依赖主窗口 owner 关系保持置顶；若未来主窗口改为多顶层窗口并存，需要重新评估 z-order。
- 风险：主窗口 `WNDPROC` 跟随钩子在应用生命周期内只安装一次；若运行期重建主窗口句柄，需要重新安装。
- 后续：在真实插件上手工验证 Pro-Q 频谱刷新、卡片滚动、窗口拖动/缩放、独立编辑器切换与退出清理。
