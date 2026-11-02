# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-02 23:18:18 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | “点击 Vocaloid 内音符有声音，但轨道播放没声音、进度条不走”，附日志 `surfing-20261102-231242-p24812.sflog` |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | audio, plugin, app, docs |
| 关联记录 | `history/ai/20261102-230946-ara-archive-persistence.md` |

## 2. 任务目标

- 让 `VOCALOID Bridge` 能在效果器选择窗中找到并插入。
- 让插件效果器（ARA 渲染器）拿到宿主走带位置，从而跟随播放并输出声音。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/scan.rs` | 修改 | `kind_from_category` 识别 `onlyara` 并归为效果器；VOCALOID Bridge 不再落入“未知” |
| `src/app/plugins.rs` | 修改 | `PLUGIN_CACHE_VERSION` 升到 `2`，旧缓存分类被丢弃并重新识别 |
| `src/audio/engine/mod.rs` | 修改 | `EffectRuntime::process` 增加 `TransportPosition` 参数；新增 `warn_vocaloid_bridge_missing` 一次性提示 |
| `src/audio/engine/render.rs` | 修改 | 轨道效果器与主效果器都按块传入 `TransportPosition`（`projectTimeSamples` + `projectTimeMusic`） |
| `doc/技术文档.md` | 修改 | 6.3 增加 OnlyARA 分类；6.7 增加效果器走带与使用方式 |
| `doc/关键常量.md` | 修改 | 插件缓存版本 2 |
| `doc/框架工作流.md` | 修改 | 补充 `EffectRuntime::process` 的走带参数 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| ARA 插件分类 | `OnlyARA` 归为未知，效果器选择窗不可见 | 归为效果器 | 用户可以插入 VOCALOID Bridge |
| 效果器走带 | `EffectRuntime::process` 传 `None`，插件 ProcessContext 位置不更新 | 每块传入宿主位置 | ARA 播放渲染器可跟随走带并渲染区域 |
| 主效果器 | 同样不传位置 | 传入位置 | ARA 类别的主效果器也能工作 |
| 诊断 | 无提示 | VSTi 轨道缺少 Bridge 时记录一次 `ara.vocaloid_vsti_without_bridge` | 便于从日志确认使用路径 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `PLUGIN_CACHE_VERSION` | `1` | `2` | `src/app/plugins.rs` | 使 `OnlyARA` 的新分类在生产环境生效 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.3、6.7 |
| `doc/关键常量.md` | 是 | 第 4 节 |
| `doc/框架工作流.md` | 是 | 3.2/3.3 效果器处理 |
| `doc/项目简介.md` | 否 | 已有 ARA 说明，无需重复 |
| `doc/代码规范.md` | 否 | 无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | 116 passed / 23 ignored |
| 插件分类 | 通过（静态） | `onlyara` 命中 EFFECT 关键字，扫描记录写入效果器列表 |
| 效果器走带 | 通过（编译/单测） | 所有 `EffectRuntime::process` 调用点均传入位置 |

## 8. 风险与后续

- 风险：日志中仍未出现 `VOCALOIDBridge.vst3`，说明用户尚未把 Bridge 插入效果链。需要按说明在 `洛天依_V5` 的效果器链添加 `VOCALOID Bridge`。
- 风险：Bridge 的 ARA 文档是新建的，若 VOCALOID 工程内容仍保存在 VSTi 状态里，需要在 Bridge 编辑器中导入/重建序列后才会渲染声音。
- 后续：若 Bridge 已插入仍无声音，检查日志中的 `ara.bound`、`ara.bind_failed` 与 `engine.track.levels`，并确认 ARA 归档在保存后非空。
