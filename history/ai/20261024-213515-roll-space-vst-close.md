# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-24 21:35:15 (Asia/Shanghai) |
| AI 工具/模型 | Trae / AI 编码助手 |
| 任务来源 | 修复浮动钢琴卷帘空格失效与左侧渲染裁切；独立 VST 乐器 GUI 停帧；标尺点击定位；移除删除音符按钮；关闭工程时 VOCALOID 6 崩溃 |
| 关联分支/提交 | 未知 |
| 影响模块 | app, ui, plugin, vendor, docs |
| 关联记录 | `history/ai/20261024-204951-gpu-render-velocity-lane.md` |

## 2. 任务目标

- 浮动钢琴卷帘窗口获得焦点时，空格可以正常播放/暂停，且不会被重复切换。
- 修复浮动卷帘左侧（工具条标题、键盘、圆角）贴边裁切的渲染问题。
- 主窗口获得焦点时，独立的 VST 乐器 GUI 仍持续动态刷新。
- 钢琴卷帘上方小节标尺支持点击/拖动切换播放位置。
- 移除钢琴卷帘工具条的「删除音符」按钮。
- 关闭含 VOCALOID 6 的工程时不再触发 `NullReferenceException`。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/input.rs` | 修改 | 空格钩子标志每帧只消费一次；浮动卷帘聚焦时父窗口只清空标志不切换，避免一次空格切换两次 |
| `src/app/rolls.rs` | 修改 | 浮动卷帘内容加 `DETACHED_ROLL_MARGIN = 8` 点外边距；按视口焦点记录 `roll_window_focused` 并把本工程设为最近焦点 |
| `src/app/mod.rs` | 修改 | 新增 `roll_window_focused` 字段；编辑器打开时对本视口 `request_repaint_after(16 ms)`；正常关闭窗口改为 `close_all_editors` 走插件 detach 生命周期 |
| `src/app/plugins.rs` | 修改 | `EditorWindow::service` 对独立 VST3 编辑器调用 `PluginWindow::refresh_repaint` |
| `src/ui/piano_roll/mod.rs` | 修改 | 标尺点击/拖动 `seek`；移除「删除音符」按钮 |
| `vendor/vst3-host/src/window.rs` | 修改 | 新增 `PluginWindow::refresh_repaint`（`RedrawWindow` + `RDW_INVALIDATE | RDW_ALLCHILDREN`） |
| `doc/技术文档.md` | 修改 | 空格归属、插件 GUI 刷新、浮动卷帘外边距、关闭生命周期、7.3 标尺与工具条、6.7 vendor patch |
| `doc/关键常量.md` | 修改 | 新增 `DETACHED_ROLL_MARGIN` |
| `doc/框架工作流.md` | 修改 | 关闭流程、每帧编排、浮动卷帘、函数索引（input/rolls/plugins/piano_roll） |
| `doc/项目简介.md` | 修改 | 标尺定位、删除按钮移除、浮动卷帘空格与边距、VST GUI 刷新与 VOCALOID 关闭修复 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 浮动卷帘空格 | 钩子与卷帘视口各处理一次，可能互相抵消 | 钩子标志由父窗口消费但忽略，卷帘视口处理唯一一次切换 | 浮动卷帘聚焦时 Space 稳定播放/暂停 |
| 浮动卷帘绘制 | 内容直接铺满客户区 | 增加 8 点外边距，工具条标题/圆角/键盘不再贴边 | 修复左侧裁切观感 |
| 独立插件 GUI 刷新 | 仅依赖心跳与参数反馈 | 工程未聚焦视口也 16 ms 重绘；独立 VST3 编辑器每帧 `RedrawWindow` | 主窗口聚焦时插件 GUI 继续变化 |
| 关闭工程窗口 | 强制销毁原生窗口并跳过插件清理 | `close_all_editors` 正常 detach + 销毁，仅强制关闭/退出跳过 | 修复 VOCALOID 6 `OnClosing` 崩溃 |
| 钢琴卷帘标尺 | 无交互 | 点击/拖动切换播放位置 | 与编曲区标尺一致 |
| 钢琴卷帘工具条 | 含「删除音符」按钮 | 按钮移除 | 删除统一用 Delete/Backspace |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `DETACHED_ROLL_MARGIN` | 无 | `8` 点 | `src/app/rolls.rs` | 浮动卷帘窗口内容外边距 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.6/6.7 插件窗口与本地 patch、7.3 钢琴卷帘、7.6 浮动卷帘、每帧编排 |
| `doc/关键常量.md` | 是 | 第 5 节新增浮动卷帘外边距 |
| `doc/框架工作流.md` | 是 | 1.5 关闭窗口流程、每帧编排、3.4/3.5 函数索引 |
| `doc/项目简介.md` | 是 | 2.2 MIDI 编辑、2.4 插件、2.6 工作流 |
| `doc/代码规范.md` | 否 | 规范未变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | 54 passed；16 ignored |
| 手工验证（浮动卷帘空格） | 通过 | 新建工程 → 打开钢琴卷帘 → 拖出独立窗口 → 聚焦后按空格：开始播放，再次按空格停止，只切换一次 |
| 手工验证（标尺定位） | 通过 | 点击钢琴卷帘小节标尺，播放头移动到对应小节 |
| 手工验证（工具条） | 通过 | 工具条不再显示「删除音符」 |
| 手工验证（左侧渲染） | 通过 | 独立窗口工具条标题完整显示，四角圆角与键盘不再贴边 |
| 手工验证（VOCALOID 关闭） | 通过 | 打开含 VOCALOID 6 的工程（VOCALOID6 Editor 窗口已出现）后关闭工程窗口，返回主页且进程无异常输出 |
| 手工验证（VST GUI 刷新） | 未逐项测量 | 已加入每帧重绘与 `RedrawWindow`；未用频谱型插件做帧率对比 |

## 8. 风险与后续

- 风险：`RedrawWindow(RDW_ALLCHILDREN)` 每帧执行会增加独立插件窗口的绘制开销；如遇个别插件闪烁，可改为按 30 Hz 节流。
- 风险：VOCALOID 的 WPF 窗口由插件自身创建，若其内部状态仍不完整，关闭时仍可能由其自身代码抛异常；本次只确保宿主不再提前销毁窗口或跳过 detach。
- 后续：在装有完整 VOCALOID 6（VSM 可用）与频谱型 VST 的环境中复测关闭与 GUI 刷新。
