# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-20 21:54:43 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 卡片内原生 GUI 仍然闪烁且无法刷新分析器，并出现无法关闭的残留进程；要求给出稳定方案 |
| 关联分支/提交 | 未知 |
| 影响模块 | ui, app, plugin, vendor/vst3-host, docs |
| 关联记录 | `history/ai/20261020-204905-vst-editor-host-rewrite.md`、`history/ai/20261020-213159-stabilize-vst-editor-host.md` |

## 2. 任务目标

- 彻底消除效果器链卡片内原生 GUI 的闪烁与残留窗口。
- 保留插件界面与实时分析器的可用性。
- 避免反复创建/销毁原生窗口造成的挂起与无法退出。
- 保留上一轮已完成的“实时”文案清理。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/embed.rs` | 删除 | 移除无边框宿主窗口方案（在 FabFilter 等自绘 GPU 界面下无法持续重绘分析器，且存在挂起/崩溃风险） |
| `src/plugin/mod.rs` | 修改 | 移除 `embed` 模块与 `PluginEditorHost` / `EditorPlacement` / `EditorOpenError` 导出 |
| `src/ui/effects.rs` | 重写 | 卡片内改为通用参数面板；新增卡片头部「界面」按钮，双击头部或点击按钮打开厂商独立编辑器；移除卡片内原生窗口生命周期与失败缓存；独立编辑器打开时每帧转发处理器参数反馈以驱动分析器 |
| `src/app/mod.rs` | 修改 | 移除 `embedded_editors`、`editor_failures` 及相关清理；保留编辑器尺寸查询节流 |
| `src/app/history.rs` | 修改 | 移除 `drop_embedded_editors` / `drop_embedded_for_track` 与失败缓存清理 |
| `src/app/edit.rs` / `plugins.rs` / `files.rs` / `rolls.rs` / `ui/inspector.rs` | 修改 | 移除对嵌入编辑器清理方法的调用 |
| `src/plugin/runtime.rs` | 修改 | 新增 `pump_editor_feedback`：排空处理器输出参数反馈并同步参数快照（回写控制器由 vst3-host 完成） |
| `src/audio/engine/plugins.rs` | 修改 | 新增 `service_editor_feedback`，把参数反馈转发给插件控制器 |
| `vendor/vst3-host/src/internal/plugin_impl.rs` | 修改 | ① 加载阶段不再 `createView` 探测 GUI，视图只在 UI 线程创建；② `get_parameter_changes` 把处理器输出参数回写到 `IEditController::setParamNormalized`，修复分析器/电平表静止 |
| `doc/技术文档.md` | 修改 | 1.2 模块表、6.6 效果器卡片界面、6.7 本地 patch、7.1、7.5、7.8、9.x、12.1、12.2 同步更新 |
| `doc/关键常量.md` | 修改 | 移除宿主窗口常量，保留编辑器尺寸查询节流，更新宽度手柄位置 |
| `doc/框架工作流.md` | 修改 | 移除 `plugin/embed.rs` 索引，更新独立编辑器与 `ui_effects` 说明 |
| `doc/项目简介.md` | 修改 | 2.4 插件、7 已知限制、目录结构同步更新 |
| `doc/代码规范.md` | 修改 | 插件规范改为“原生 GUI 统一独立窗口，不嵌入卡片” |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 卡片 GUI | 卡片内嵌 VST3 原生窗口 | 卡片内显示参数滑块 | 无原生子窗口，闪烁/残留窗口消失 |
| 打开插件界面 | 卡片内直接显示或双击 | 双击卡片头部或点击「界面」按钮打开独立窗口 | 走稳定的 `PluginWindow` / `Vst2EditorWindow` 路径 |
| 分析器刷新 | 宿主只轮询自身参数 | 独立窗口打开时每帧排空处理器输出参数反馈，并回写控制器 | 分析器/电平表可持续刷新 |
| 插件 GUI 创建 | 加载阶段可能跨线程 `createView` | 只在 UI 线程创建一次 | 降低闪烁、挂起与崩溃风险 |
| 编辑器尺寸查询 | 缓存缺失时每帧查询 | 失败时节流 1 秒 | 避免反复创建临时视图 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 效果器宿主窗口类 | `SurfingStudioPluginEditorHost` | 删除 | `src/plugin/embed.rs` | 不再使用卡片内宿主窗口 |
| 效果器宿主重定位 / detach 等待 | 有 | 删除 | `src/plugin/embed.rs` | 同上 |
| 编辑器尺寸查询节流 | `1` 秒 | 保留 | `src/ui/effects.rs` | 限制临时视图创建频率 |
| 宽度拖拽手柄 | 卡片外侧 6 点 | 卡片右边缘 `-4 ~ +2` 点 | `src/ui/effects.rs` | 卡片内无原生窗口遮挡 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2、6.6、6.7、7.1、7.5、7.8、9.x、12.1、12.2 |
| `doc/关键常量.md` | 是 | 第 7 节效果器链 |
| `doc/框架工作流.md` | 是 | 模块表、`plugin` 索引、`ui/effects.rs` 索引、撤销流程 |
| `doc/项目简介.md` | 是 | 2.4 插件、7 已知限制、目录结构 |
| `doc/代码规范.md` | 是 | 8.2 生命周期 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | `cargo test --lib`：23 passed / 0 failed / 5 ignored |
| `cargo clippy --all-targets` | 通过 | 无新增警告 |
| 手工验证 | 部分执行 | 用 Pro-Q 4 实测：处理器每块都有非零输入/输出、独立编辑器正常显示；卡片内原生窗口无法持续重绘分析器，故移除卡片内嵌方案 |

## 8. 风险与后续

- 风险：卡片内不再直接显示原生 GUI，需要双击或点击「界面」打开独立窗口；分析器在该独立窗口中查看。
- 风险：`get_parameter_changes` 现在会回写控制器，若发现某些插件对 `setParamNormalized` 有副作用，需要增加按插件白名单/开关。
- 后续：在真实工程中验证独立编辑器的分析器刷新与删除/重排时的窗口关闭；残留进程问题建议用管理员权限任务管理器清理，并记录触发插件。
