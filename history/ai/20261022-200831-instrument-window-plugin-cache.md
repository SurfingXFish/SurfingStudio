# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-22 20:08:31 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 代码智能体 |
| 任务来源 | 效果器卷帘 Shift+滚轮横滑、主页欢迎语、启动增量扫描与插件缓存、内置插件资料库、双击卡片顶部栏、轨道末尾加轨、乐器窗口 |
| 关联分支/提交 | 未知 |
| 影响模块 | core, plugin, app, ui, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 效果器卷帘支持 `Shift+滚轮` 左右滑动。
- 主页顶部栏左侧隐藏下拉栏与项目列表，改为「欢迎使用SurfingStudio（用户名）」。
- 每次启动自动增量扫描 VST 插件，并持久化插件记录与文件指纹，避免重复扫描。
- 内置常见 VST 厂商/插件资料库，离线补全扫描结果缺失的厂商、分类与角色。
- 双击效果器卡片顶部栏任意位置都能打开独立 GUI。
- 删除工具栏「+ 轨道」，在轨道列表末尾新增「+ 添加轨道」。
- 单击轨道乐器名称打开乐器窗口：左侧种类，右侧按厂商分组的乐器列表与「选择」按钮。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/catalog.rs` | 新增 | 内置插件资料表（常见合成器、键盘、吉他、弦乐、鼓与效果器）及 `lookup` / `normalize` |
| `src/plugin/mod.rs` | 修改 | `ScannedPlugin` / `PluginKind` 增加 serde；导出 `lookup_catalog`、`list_plugin_files`、`enrich_scan_record` |
| `src/plugin/scan.rs` | 修改 | 新增 `list_plugin_files`、`scan_vst3_paths` 与 VST3/VST2 目录枚举；扫描记录创建时调用 `enrich_from_catalog` |
| `src/core/model/plugin.rs` | 修改 | `PluginFormat` 增加 `Hash`，供扫描结果去重 |
| `src/app/state.rs` | 修改 | `ScanMode::Incremental`；新增 `InstrumentCategory` 与 `InstrumentPicker` |
| `src/app/mod.rs` | 修改 | 启动读取插件缓存并触发增量扫描；新增 `instrument_picker` / 文件指纹字段；每帧绘制乐器窗口 |
| `src/app/plugins.rs` | 修改 | 新增插件缓存读写、文件指纹、`start_startup_scan` 增量扫描、`scan_full` / `scan_incremental`；扫描结果与分类写回缓存文件 |
| `src/app/settings.rs` | 修改 | `AppSettings::favorite_instruments` 与默认值 |
| `src/app/panels.rs` | 修改 | 主页应用栏显示欢迎语；删除工具栏「+ 轨道」；新增系统用户名辅助函数 |
| `src/ui/mod.rs` | 修改 | 注册 `instrument_picker` 模块 |
| `src/ui/effects.rs` | 修改 | 原始滚轮事件解析（含 Shift 横轴）；双击卡片顶部栏任意位置打开独立编辑器（排除开关/关闭按钮） |
| `src/ui/instrument_picker.rs` | 新增 | 乐器窗口：种类栏、收藏、搜索、厂商分组、徽章、选择内置或 VST 乐器 |
| `src/ui/arrangement/mod.rs` | 修改 | 乐器名称单击打开乐器窗口；轨道列表末尾绘制「+ 添加轨道」菜单 |
| `src/ui/arrangement/layout.rs` | 修改 | 新增 `ADD_TRACK_ROW_H` / `add_track_row_h`，`total_height` 计入加轨行 |
| `doc/技术文档.md` | 修改 | 6.3 增量扫描与缓存；7.1 主页应用栏与工具栏；7.2 加轨行与乐器名称；7.5 滚动与双击；7.10 设置字段；新增 7.11 乐器窗口 |
| `doc/关键常量.md` | 修改 | 缓存版本/路径、增量扫描、资料库、加轨行与乐器窗口尺寸 |
| `doc/框架工作流.md` | 修改 | 启动流程、扫描流程、模块地图、函数索引、快速入口 |
| `doc/项目简介.md` | 修改 | 插件扫描与资料库、效果器滚动/双击、主页欢迎语、乐器窗口与加轨行 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 启动流程 | 不扫描插件，插件列表为空 | 读取 `plugin-cache.json` 后后台增量扫描，复用指纹未变化的记录 | 启动即可看到上次插件；只识别新增/变化文件 |
| 插件扫描 | 每次完整扫描目录并覆盖缓存 | 新增 `ScanMode::Incremental`；完整扫描仍保留 | 减少启动耗时与 VST2 重复实例化 |
| 扫描结果处理 | 仅内存 `plugin_cache` | 结果与指纹写盘，分类探测与加载结果也写盘 | 下次启动无需重复识别 |
| 扫描元数据 | 仅 `moduleinfo.json` / VST2 实例化 | 新增离线资料库补全缺失字段 | 未知插件也能显示厂商与角色 |
| 添加轨道 | 工具栏「+ 轨道」菜单 | 轨道列表末尾「+ 添加轨道」行 | 工具栏空间更集中；加轨入口随轨道列表滚动 |
| 乐器切换 | 检查器下拉框 / 插件浏览器 | 新增乐器窗口，按种类浏览内置与 VST 乐器 | 单击轨道乐器名称即可切换 |
| 效果器卷帘交互 | 仅按 `smooth_scroll_delta` 横滑；仅双击标题区 | 原始滚轮事件（含 Shift 横轴）；顶部栏任意位置双击 | 兼容平台差异，降低双击命中要求 |
| 应用栏 | 主页与工作室都显示下拉栏与项目栏 | 主页显示欢迎语，工作室保留原菜单 | 主页更简洁 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `PLUGIN_CACHE_VERSION` | 无 | `1` | `src/app/plugins.rs` | 插件缓存结构版本 |
| 插件缓存路径 | 无 | `%LOCALAPPDATA%/SurfingStudio/plugin-cache.json`（回退 `%APPDATA%`、系统临时目录） | `src/app/plugins.rs` | 记录 + 文件指纹 |
| `ADD_TRACK_ROW_H` | 无 | `34` 点（最小 `26`） | `src/ui/arrangement/layout.rs` | 轨道列表末尾加轨行 |
| `PICKER_W` / `PICKER_H` | 无 | `820 × 560` 点 | `src/ui/instrument_picker.rs` | 乐器窗口默认尺寸 |
| `CATEGORY_W` | 无 | `132` 点 | `src/ui/instrument_picker.rs` | 乐器种类栏宽度 |
| `CATEGORY_ROW_H` | 无 | `30` 点 | `src/ui/instrument_picker.rs` | 种类项高度 |
| `OPTION_ROW_H` | 无 | `46` 点 | `src/ui/instrument_picker.rs` | 乐器行高度 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.3、7.1、7.2、7.5、7.10、新增 7.11 |
| `doc/关键常量.md` | 是 | 第 4、6 节新增缓存、扫描与乐器窗口常量 |
| `doc/框架工作流.md` | 是 | 1.1、1.7、2、3.1、3.3、3.4、3.5、4 |
| `doc/项目简介.md` | 是 | 2.4 插件、2.6 工作流 |
| `doc/代码规范.md` | 否 | 规范本身未变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 无格式差异 |
| `cargo test` | 通过 | 31 通过、12 个需要真实插件的测试忽略；含新增的增量扫描复用测试（默认忽略） |
| 手工验证 | 部分执行 | 启动应用后增量扫描完成并写出 30 条插件记录与指纹（本机 AppData 写入受限时回退到临时目录），内置资料库补全了 Auto-Tune Pro、TDR Nova、Serum 2 等插件的厂商/分类；滚轮、双击、乐器窗口与加轨行的鼠标交互未在可见桌面逐项点击验证 |

## 8. 风险与后续

- 风险：增量扫描以「修改时间秒 + 字节数」判断文件变化；插件原地替换但时间与大小完全相同时不会重新识别，可手动执行完整扫描。新 VST2 文件会扫描其所在目录，该目录内 DLL 会被短暂实例化。
- 后续：在真实插件环境验证启动增量扫描耗时、收藏持久化与乐器窗口在窄窗口下的布局；必要时为资料库补充更多厂商与插件条目。
