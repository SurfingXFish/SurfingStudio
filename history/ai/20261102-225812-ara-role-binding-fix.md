# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-02 22:58:12 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户反馈“Vocaloid 问题依旧”，并附运行日志 `surfing-20261102-223744-p28176.sflog` |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | plugin, audio, docs |
| 关联记录 | `history/ai/20261102-223340-ara2-host-integration.md`（ARA 宿主端骨架） |

## 2. 任务目标

- 让 ARA 角色绑定真正成功，而不是在插件内部触发访问冲突。
- 让 VOCALOID Bridge 的编辑器跟随工程走带、在播放时输出声音。
- 修正 ARA 文档与插件实例的销毁顺序。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/plugin/ara/mod.rs` | 修改 | `bind_plugin` 通过 `VirtualQuery` 判断 `IPlugInEntryPoint2` 的 vtable 布局：VOCALOID Bridge 为“无继承”草稿布局，角色绑定在槽 3；标准布局在槽 5。新增 `IPlugInEntryPoint2StandaloneVtbl` 与 `is_executable` |
| `src/plugin/ara/mod.rs` | 修改 | `host_enabled` 改为 `host_enabled_for(path)`：默认只为 `VOCALOIDBridge.vst3` 启用绑定，`SURFING_ARA_HOST=1` 启用全部 |
| `src/plugin/runtime.rs` | 修改 | `Vst3Runtime` 的 `ara` 字段移到 `plugin` 之前：文档控制器必须先于插件模块卸载销毁 |
| `src/audio/engine/mod.rs` | 修改 | 插件效果器（含 VOCALOID Bridge）每轮同步 ARA 时间线与速度；新增 `sync_ara_timeline` |
| `src/plugin/backends/vst3.rs` | 修改 | 更新注释与启用条件；保留未启用时的工厂探测日志 |
| `Cargo.toml` | 修改 | winapi 增加 `memoryapi`（`VirtualQuery` 判断内存可执行性） |
| `doc/技术文档.md` | 修改 | 6.7 补充入口接口两种布局、启用范围、Bridge 使用方式、析构顺序 |
| `doc/关键常量.md` | 修改 | ARA 启用范围常量更新 |
| `doc/框架工作流.md` | 修改 | `host_enabled_for` 与 ARA 绑定说明 |
| `doc/项目简介.md` | 修改 | 2.4 与第 7 节更新 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| ARA 角色绑定 | 按标准头文件调用槽 5，VOCALOID Bridge 内部 `0xC0000005` | 检查槽 4 是否可执行，草稿布局改调槽 3 | 绑定成功，`bound roles=7`，编辑器可收到 ARA 文档 |
| ARA 启用 | 全部默认关闭 | 仅 `VOCALOIDBridge.vst3` 默认启用 | 用户把 Bridge 插入人声轨效果链即可使用；其它 ARA 插件仍按需开启 |
| 文档销毁 | `ara` 在 `plugin` 之后析构，销毁回调访问已卸载插件 | `ara` 在 `plugin` 之前析构 | 关闭工程/卸载插件不再崩溃 |
| 效果器时间线 | 只有乐器同步 ARA | 插件效果器也按轨道片段同步 ARA 播放区域 | Bridge 的播放区域与工程时间线一致 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| ARA 启用范围 | `SURFING_ARA_HOST=1` 才启用 | 默认仅 `VOCALOIDBridge.vst3`；环境变量启用全部 | `src/plugin/ara/mod.rs` | `host_enabled_for` |
| `memoryapi` | 未启用 | 已启用 | `Cargo.toml` | `VirtualQuery` 判断 vtable 槽位是否指向代码 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 6.7 ARA2 宿主端 |
| `doc/关键常量.md` | 是 | 第 4 节 |
| `doc/框架工作流.md` | 是 | 3.3 `plugin` 索引 |
| `doc/项目简介.md` | 是 | 2.4、第 7 节 |
| `doc/代码规范.md` | 否 | 规范无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | 115 passed / 23 ignored |
| 真实插件绑定 | 通过 | 忽略用例 `binds_real_vocaloid_ara_document`：`bound roles=7`、factory=`com.yamaha.vocaloidbridge.arafactory`，设置时间线后先销毁会话再销毁插件，无异常 |
| 崩溃定位 | 已确认 | 修复前异常位于 `VOCALIDBridge.vst3 +0x59eb8`（`queryInterface` 被当成角色绑定调用，`rdi=7`）；修复后不再触发 |
| 析构顺序回归 | 已修复 | 修复前插件先卸载、会话后销毁时在宿主内崩溃；字段顺序调整后通过 |

## 8. 风险与后续

- 风险：`is_executable` 依赖 Windows `VirtualQuery`；其它平台按标准布局处理，尚未实测。
- 风险：ARA 归档持久化未实现，Bridge 编辑器里的编辑不会随工程保存；关闭工程后需要重新编辑/导入。
- 风险：Bridge 作为效果器在后台同步线程创建；若同一实例在其它线程被同步，ARA 调用会返回线程错误并被忽略（不会崩溃，但时间线可能滞后到下一次同线程同步）。
- 后续：实现 ARA `storeObjectsToArchive` / `restoreObjectsFromArchive` 并接入 `.sfsd`，让 VOCALOID 编辑内容可持久化；接入 `notifySelection` 以同步轨道/区域选择。
