# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-21 23:10:44 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 效果器卷帘只显示 GUI；大界面插件打开卡顿；GUI 默认缩放；双击标题打开独立窗口 |
| 关联分支/提交 | 未知 |
| 影响模块 | ui / app / vendor/vst3-host / docs |
| 关联记录 | `history/ai/20261021-224958-inline-popup-async-plugin-apply.md` |

## 2. 任务目标

- 删除效果器卷帘卡片中的参数列表，卡片主体只显示插件原生 GUI。
- 修复大界面插件（Auto-Tune Pro）打开 GUI 时卡顿。
- 内嵌 GUI 默认等比缩放以契合卡片/卷帘大小。
- 双击卡片标题以独立窗口打开效果器 GUI。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/ui/effects.rs` | 修改 | 删除参数缓存/参数滑块与参数写回；卡片主体自动内嵌原生 GUI；每帧最多创建一个内嵌编辑器；「界面/收起」改为「独立」按钮；双击标题走独立窗口；卡片最小宽度不再跟随插件尺寸 |
| `src/app/plugins.rs` | 修改 | 新增 `has_editor_window` / `open_inline_editor`（失败记入 `inline_failed`，引擎锁忙时下一帧重试）；删除 `toggle_inline_editor`；`place_inline` 先 `fit_embedded` 等比缩放再居中摆放；`close_all_editors` 清除失败记录 |
| `src/app/mod.rs` | 修改 | 新增 `inline_failed`；删除 `editor_sizes` / `editor_size_query_at`；底部面板高度不再按插件高度撑开 |
| `src/app/input.rs` | 修改 | Esc 只关闭独立窗口；没有独立窗口时关闭底部卷帘（内嵌 GUI 随卷帘存在） |
| `src/app/history.rs` | 修改 | 删除 `effects_panel_min_height` 与编辑尺寸缓存清理 |
| `src/app/edit.rs` | 修改 | 删除效果器/粘贴效果器链时同步清理 `inline_failed` |
| `vendor/vst3-host/src/window.rs` | 修改 | 新增 `fit_embedded`（内容缩放等比适配、居中、阈值 0.01、失败沿用上一帧尺寸）；`open_embedded` 用 400×300 占位替代 `get_editor_size()`；记录 `embedded_host_scale/scale/base_size/size/scalable`；支持内容缩放时不再回灌 `onSize` |
| `src/plugin/mod.rs` | 修改 | 更新 Pro-Q 内嵌诊断测试（先 `fit_embedded`）；新增 Auto-Tune Pro 打开耗时/缩放诊断测试（默认 ignore） |
| `doc/技术文档.md` | 修改 | 6.6 卡片只显示 GUI、自动内嵌、缩放策略；6.7 本地 patch；7.5 / 7.6 |
| `doc/框架工作流.md` | 修改 | `app/plugins.rs` / `ui/effects.rs` / `app/history.rs` 索引 |
| `doc/项目简介.md` | 修改 | 2.4 插件说明 |
| `doc/代码规范.md` | 修改 | 8.2 内嵌 GUI 规范 |
| `doc/关键常量.md` | 修改 | 内嵌缩放阈值/上下限、卡片内缩；删除尺寸查询节流 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 卡片主体 | 参数滑块列表，点击「界面」才内嵌 | 自动内嵌插件原生 GUI，无参数列表 | 打开卷帘即看到插件界面 |
| 打开方式 | 双击/「界面」切换内嵌 | 双击标题/「独立」按钮打开独立窗口 | 与内嵌模式解耦，互斥 |
| 缩放 | 窗口固定为卡片大小，靠 `onSize` 强制插件布局 | 支持内容缩放时按 `min(可用/逻辑, DPI)` 等比缩放并居中；不支持时退回 `onSize` | Auto-Tune 1140×680 → 403×240（430×240 卡片），耗时 < 1 ms |
| 大界面插件打开 | 先 `get_editor_size`（额外临时视图，约 87 ms）再附加 | 占位窗口直接附加，附加后读取尺寸 | Auto-Tune 打开总耗时 248 ms → 206 ms，减少一次视图创建 |
| 创建节奏 | 每次点击创建 | 每帧最多一个，失败的槽位不再重试 | 避免多张大界面同时初始化造成长时间无响应 |
| Esc | 关闭全部编辑器（内嵌会被立刻重新打开） | 只关闭独立窗口；无独立窗口时关闭卷帘 | 行为稳定 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 内嵌缩放阈值 | 无 | `0.01` | `vendor/vst3-host/src/window.rs` | 避免每帧重复 `setContentScaleFactor` |
| 内嵌缩放范围 | 无 | `0.1 ~ 宿主 DPI 缩放` | `vendor/vst3-host/src/window.rs` | 只缩小，不放大超过 100% |
| 内嵌占位窗口尺寸 | 无 | `400×300` | `vendor/vst3-host/src/window.rs` | 附加前的临时尺寸 |
| 卡片最小宽度来源 | 插件编辑器宽 / ppp + 12 | `MIN_CARD_W`（300） | `src/ui/effects.rs` | 宽度不再被大界面插件撑大 |
| 底部面板最小高度 | 插件编辑器高 + 64（≤720） | `190` | `src/app/mod.rs` | 面板不再被大界面插件撑高 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.6、6.7、7.5、7.6、11 模块表 |
| `doc/关键常量.md` | 是 | 第 7 节效果器链 |
| `doc/框架工作流.md` | 是 | `app/history.rs`、`app/plugins.rs`、`ui/effects.rs` 索引 |
| `doc/项目简介.md` | 是 | 2.4、目录树 |
| `doc/代码规范.md` | 是 | 8.2 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 已运行 |
| `cargo check --all-targets` | 通过 | 无错误 |
| `cargo test` | 通过 | 24 passed / 0 failed / 11 ignored |
| `pro_q_editor_embeds_in_a_host_child_window` | 通过 | `fit_embedded(500×300)` → 500×300，位置与显隐正确 |
| `auto_tune_editor_open_timing` | 通过 | 插件自然尺寸 1140×680；`fit_embedded(430×240)` → 403×240（< 1 ms）；`fit_embedded(860×480)` → 807×481；逐帧服务最坏 17 µs |
| 真实 eframe 窗口截屏 | 通过 | 顶层附属窗口可显示（此前 `WS_CHILD` 版本截图为纯色） |

## 8. 风险与后续

- 风险：内嵌窗口是顶层附属窗口，会盖住与其重叠的 egui 对话框；后续可在打开模态窗口时临时隐藏。
- 风险：不支持内容缩放的插件若卡片小于其最小尺寸，视图会被窗口裁切；可提示用户拉宽卡片。
- 风险：每帧最多创建一个内嵌编辑器，打开多效果器工程时界面会在若干帧内逐个出现。
- 后续：用 Auto-Tune Pro / Pro-Q 4 验证缩放、频谱刷新、滚动裁剪与独立窗口切换。
