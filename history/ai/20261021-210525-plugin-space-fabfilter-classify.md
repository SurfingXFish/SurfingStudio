# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-21 21:05:25 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 修复独立窗口空格暂停、FabFilter GUI 卡死与不跟随缩放、VST GUI 未聚焦不刷新、VST 乐器/效果器无法区分 |
| 关联分支/提交 | 未知 |
| 影响模块 | app, plugin, ui, vendor/vst3-host, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 焦点在插件编辑器、浮动卷帘等独立窗口时，空格仍能播放/暂停。
- 修复 FabFilter 系列 VST3 编辑器 GUI 无动态显示、拖动窗口边框无法缩放的问题。
- 插件 GUI 打开但未获得焦点时仍持续刷新（分析器、频谱、电平）。
- 扫描阶段正确区分 VST 乐器与效果器，尤其是 Windows 单文件 `.vst3`。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/hotkeys.rs` | 新增 | UI 线程级 `WH_GETMESSAGE` 钩子：记录非主窗口的空格按下（过滤长按重复），`take_space_toggle` 交给键盘处理；消息只观察不拦截 |
| `src/app/mod.rs` | 修改 | 声明 `hotkeys` 模块；每帧传入主窗口句柄安装钩子；插件编辑器打开期间保持 `EDITOR_REPAINT_INTERVAL`（16 ms）重绘 |
| `src/app/input.rs` | 修改 | 空格来源合并为主窗口 egui 输入 + 独立窗口消息钩子 |
| `src/app/plugins.rs` | 修改 | `service_editors` 在编辑器打开期间每帧调用 `service_editor_feedback`，不再依赖效果器卷帘是否绘制 |
| `src/ui/effects.rs` | 修改 | 移除卡片绘制中的反馈转发与随之失效的 `engine_ready` / `slot` 局部变量 |
| `src/plugin/editor.rs` | 修改 | VST1/VST2 编辑器窗口增加 `WS_CLIPSIBLINGS`；打开与重新置前时激活窗口并聚焦子窗口 |
| `src/plugin/scan.rs` | 修改 | 新增 `Vst3ClassInfo`；moduleinfo 同时探测 `Contents` 与 `Contents/Resources`，兼容 `Sub Categories` 键名；单文件 `.vst3` 通过 `read_factory_classes` 补齐名称/CID/分类（不创建实例）；新增分类单元测试 |
| `src/ui/browser.rs` | 修改 | 类型筛选严格匹配，未知类型不再同时出现在乐器与效果器列表 |
| `vendor/vst3-host/src/discovery.rs` | 修改 | 新增 `FactoryClassInfo` 与 `read_factory_classes`：只加载模块并枚举 `IPluginFactory` / `Factory2` / `Factory3` 元数据，不调用 `createInstance` |
| `vendor/vst3-host/src/lib.rs` | 修改 | 导出 `FactoryClassInfo` / `read_factory_classes` |
| `vendor/vst3-host/src/window.rs` | 修改 | 编辑器窗口样式增加 `WS_CLIPCHILDREN` / `WS_CLIPSIBLINGS`；打开后 `SetForegroundWindow` / `SetFocus`；`WM_SIZE` 记录客户区尺寸，`service_platform_events` 调用 `resize_editor`（`checkSizeConstraint` + `onSize`）并跟随插件约束尺寸 |
| `doc/技术文档.md` | 修改 | 2.3 重绘节奏、6.3 扫描与分类、6.6 编辑器窗口与反馈、6.7 本地 patch、7.1 每帧流程与空格钩子、10 性能表、11.3 手工验证 |
| `doc/关键常量.md` | 修改 | 新增 VST3 元数据路径、单文件探测、编辑器窗口样式、编辑器打开时重绘间隔；更新快捷键行 |
| `doc/框架工作流.md` | 修改 | 首帧流程、插件扫描流程、`app/hotkeys.rs` 索引、`scan.rs` 索引、反馈调用链与快速入口 |
| `doc/项目简介.md` | 修改 | 插件扫描分类、独立窗口激活与缩放、GUI 持续刷新、Space 适用范围、兼容性说明 |
| `doc/代码规范.md` | 修改 | 8.2 扫描规则改为“缺元数据时只读工厂类元数据，不创建实例” |
| `history/ai/20261021-210525-plugin-space-fabfilter-classify.md` | 新增 | 本记录 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 独立窗口空格 | 仅主窗口 egui 输入可见，插件/浮动窗口按空格无效 | UI 线程消息钩子观察非主窗口空格，主窗口仍走 egui | 插件编辑器与浮动卷帘内均可播放/暂停 |
| 编辑器反馈服务 | 仅在效果器卷帘绘制卡片时转发处理器反馈 | `service_editors` 每帧对全部打开编辑器转发 | 从检查器打开或卷帘关闭时分析器仍刷新 |
| 编辑器重绘节奏 | 空闲 80 ms，仅播放中 16 ms | 编辑器打开期间 16 ms | 插件定时器、重绘与 VST2 `idle` 持续得到服务 |
| VST3 窗口缩放 | 窗口边框可拖但未通知插件，GUI 尺寸不变 | `WM_SIZE` 记录 → `resize_editor` → `onSize`，固定尺寸插件忽略 | FabFilter 等自适应 GUI 可正常缩放 |
| VST3 窗口激活 | 打开后不主动激活 | 打开/置前时激活并聚焦 | 减少“未选中窗口不动画”的现象 |
| VST3 单文件扫描 | 直接按文件名加入，类型为未知、CID 为空 | 无元数据时读取工厂类元数据（不创建实例），补齐名称/CID/分类 | 乐器与效果器可区分，加载可按 CID 选择类，且不触发插件启动窗口 |
| 浏览器类型筛选 | 未知类型同时归入乐器与效果器 | 严格匹配，未知单独归入“未知” | 列表不再混淆两类插件 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `EDITOR_REPAINT_INTERVAL` | 无（编辑器打开时沿用 80 ms） | `16` ms | `src/app/mod.rs` | 编辑器打开期间的重绘间隔 |
| VST3 元数据搜索路径 | `Contents/moduleinfo.json` | `Contents/moduleinfo.json` → `Contents/Resources/moduleinfo.json` | `src/plugin/scan.rs` | 兼容 Serum 2 等厂商布局 |
| VST3 无元数据探测 | 无 | 只读工厂类元数据，不创建实例 | `vendor/vst3-host/src/discovery.rs` | 扫描单文件 `.vst3` 时补齐名称/CID/分类 |
| VST3 编辑器窗口样式 | `WS_OVERLAPPEDWINDOW` | `WS_OVERLAPPEDWINDOW \| WS_CLIPCHILDREN \| WS_CLIPSIBLINGS` | `vendor/vst3-host/src/window.rs` | 防止父窗口覆盖插件子窗口 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 2.3、6.3、6.6、6.7、7.1、10、11.3 |
| `doc/关键常量.md` | 是 | 第 4 节插件宿主与扫描、第 5 节重绘与快捷键 |
| `doc/框架工作流.md` | 是 | 1.1、1.7、3.3、3.4、4、5.3 |
| `doc/项目简介.md` | 是 | 2.4、2.5、3.2 |
| `doc/代码规范.md` | 是 | 8.2 扫描规则 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | 24 passed / 0 failed / 5 ignored；新增 `moduleinfo_reads_spaced_sub_categories_and_classifies_roles` |
| `cargo check --all-targets` | 通过 | 无警告 |
| 真实插件扫描测试 | 通过 | `scans_installed_vst3_metadata`（ignored）：13.51s；FabFilter One → 乐器，Pro-Q 4 / Pro-C 2 / Pro-DS / Pro-R 2 等 → 效果器，Serum 2 / VOCALOID VSTi → 乐器，TDR Nova → 效果器 |
| 手工验证 | 未执行 | 未在真实桌面会话中打开 FabFilter 编辑器与插件窗口，无法人工确认动画、边框缩放与空格行为 |

## 8. 风险与后续

- 风险：线程消息钩子会观察所有非主窗口的空格；若插件 GUI 内存在文本输入框，按空格会同时切换播放。
- 风险：单文件 `.vst3` 扫描仍会 `LoadLibrary` 并调用模块工厂，插件 `DllMain` / `InitDll` 中的副作用（写日志、初始化 COM）无法避免；但不再创建组件/控制器，已避免 VOCALOID 等启动窗口。
- 风险：拖动窗口边框期间 Windows 进入模态缩放循环，插件 `onSize` 在松开鼠标后的下一次服务才生效，缩放不是逐帧实时。
- 后续：在真实机器上验证 FabFilter Pro-Q 4 / Pro-DS 的频谱刷新、边框缩放与空格暂停；若某插件对 `onSize` 有特殊约束，记录插件名并考虑加入白名单。
