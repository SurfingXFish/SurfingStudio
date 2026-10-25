# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-24 16:47:28 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 未标注模型 |
| 任务来源 | 调整主页项目与插件布局、重构设置分类、增加 ASIO 支持、项目列表按保存时间排序并展示工程信息与历史版本 |
| 关联分支/提交 | 未知 |
| 影响模块 | audio, app, core, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 主页插件页按品牌分组并默认展开，乐器在上、效果器在下，行内增加类型标签与详情下拉。
- 设置窗口改为界面、存储、音频、控制器、插件、快捷键、软件信息七类，并补齐各项可配置能力。
- 音频增加 ASIO 支持，可选择音频系统、驱动/设备、采样率、缓冲与启用的输入设备。
- 主页项目页显示全部打开过的项目，删除不存在条目，并按最近保存时间排序。
- 项目行新增下拉，可查看工程信息与历史版本。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `Cargo.toml` | 修改 | winapi 增加 `winreg`、`winnt`、`minwinbase`、`fileapi`、`timezoneapi` 特性，用于 ASIO 驱动枚举与本地时间换算 |
| `src/audio/asio.rs` | 新增 | Windows ASIO 后端：注册表驱动枚举、`IASIO` COM 接口、采样率/缓冲协商、双缓冲实时回调、三种采样格式转换、诊断测试 |
| `src/audio/backend.rs` | 修改 | 新增 `AudioSystem`、`AudioOptions`、音频系统/设备枚举、`start_output_with`、`start_cpal` / `start_asio`；`lock_engine` 提升为 `pub(super)` |
| `src/audio/mod.rs` | 修改 | 声明 `asio` 模块并导出音频系统/选项类型 |
| `src/core/midi_input.rs` | 修改 | 支持按设置连接指定 MIDI 输入端口、端口枚举（输入/输出）、`set_device` 重连与 `output_port_names` |
| `src/app/shared.rs` | 修改 | 音频输出改为可替换槽位；新增 `start_audio` / `restart_audio` / `audio_status()` / `reconnect_midi`；插件缓存写入设置中的缓存目录 |
| `src/app/plugins.rs` | 修改 | 插件缓存路径支持自定义缓存目录，加载/保存函数增加 `cache_dir` 参数 |
| `src/app/settings.rs` | 重写 | 七类设置模型与界面；存储、音频、控制器、插件、快捷键、软件信息页；设备枚举缓存；后台文件夹选择支持三种目标 |
| `src/app/home.rs` | 重写 | 项目页展示全部项目、按 mtime 排序、清理失效路径、工程信息与历史版本下拉；插件页按品牌分组、乐器/效果器/未知分区、标签与详情下拉 |
| `src/app/backup.rs` | 新增 | 历史版本目录、创建（最小间隔 300 秒）、保留数清理、版本列表与历史路径判定 |
| `src/app/time_fmt.rs` | 新增 | 本地时间分量（Windows FILETIME）、绝对/相对时间与版本文件名时间戳 |
| `src/app/files.rs` | 修改 | 保存/打开对话框使用默认存储地址；保存前在写盘线程创建历史版本；历史版本不写入项目列表 |
| `src/app/autosave.rs` | 修改 | 自动保存间隔改为读取设置 `autosave_seconds`（0 关闭），移除固定 60 秒常量 |
| `src/app/mod.rs` | 修改 | 新增主页项目展开/摘要缓存、插件折叠/详情状态、设置设备缓存与文件夹目标字段；音频状态改走 `SharedServices::audio_status()` |
| `doc/技术文档.md` | 修改 | 2.1 线程表、4.9 音频系统/ASIO、7.10 主页与设置、12.2 限制、附录 B 文件索引 |
| `doc/关键常量.md` | 修改 | 采样率/缓冲/ASIO、自动保存与历史版本、主页与设置布局、缓存位置等常量 |
| `doc/框架工作流.md` | 修改 | 模块地图、函数索引、保存/自动保存流程、共享服务与主页调用链 |
| `doc/项目简介.md` | 修改 | 插件页、主页项目/设置/ASIO/历史版本、技术栈、目录结构与已知限制 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 音频启动 | `SharedServices::new` 直接 `start_output`，设备在启动时固定 | 按设置的音频系统/设备/采样率/缓冲启动；失败回退系统默认设备；设置变更时 `restart_audio` 重开 | 支持 WASAPI 与 ASIO，切换设备无需重启软件 |
| ASIO | 不支持 | 枚举 `HKLM\SOFTWARE\ASIO`，`CoCreateInstance` 创建 `IASIO`，`bufferSwitch` 驱动渲染 | Windows 下可用低延迟 ASIO 驱动 |
| MIDI 输入 | 固定连接第一个端口 | 按设置端口连接，设置页可切换并立即重连 | 控制器页输入设备可选择 |
| 项目列表 | 最近 12 条、按打开/保存顺序 | 全部项目、按文件 mtime 排序、每 2 秒清理失效路径 | 项目页内容更完整 |
| 项目行 | 单击打开 | 单击打开，右侧箭头展开工程信息与历史版本 | 可查看摘要并打开历史版本 |
| 保存 | 直接写盘 | 写盘线程先创建 `Backups` 历史版本，再写工程 | 可回看历史版本，受保留数限制 |
| 自动保存 | 固定 60 秒 | 读取设置 `autosave_seconds`，0 关闭 | 用户可配置间隔 |
| 设置窗口 | 界面 / 音频 / MIDI / VST / 最近项目 | 界面 / 存储 / 音频 / 控制器 / 插件 / 快捷键 / 软件信息 | 设置能力与分类重组 |
| 插件页 | 平铺列表 | 品牌折叠分组，乐器在上、效果器在下，标签 + 详情下拉 | 浏览更清晰 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `AUTOSAVE_INTERVAL` | `60` 秒 | 删除，改为设置 `autosave_seconds` | `src/app/autosave.rs`、`src/app/settings.rs` | 可选关闭 / 15 / 30 / 60 / 120 / 300 / 600 秒 |
| `MAX_RECENT_PROJECTS` | `12` | 删除 | `src/app/settings.rs` | 项目列表不再截断 |
| `PROJECT_ROW_H` | `48` 点 | `52` 点 | `src/app/home.rs` | 项目行含展开箭头与保存时间 |
| `PLUGIN_ROW_H` | `42` 点 | 删除，新增 `PLUGIN_BRAND_H = 34` 点 | `src/app/home.rs` | 插件改为品牌折叠 + 卡片 |
| `SETTINGS_WINDOW_W/H` | `640 × 560` | `760 × 620` | `src/app/settings.rs` | 七类设置内容更宽 |
| `SETTINGS_MIN_W/H` | `520 × 420` | `620 × 460` | `src/app/settings.rs` | 保证两栏可用 |
| `SETTINGS_NAV_W` | `148` 点 | `152` 点 | `src/app/settings.rs` | 容纳“软件信息”等分类 |
| 新增 `PROJECT_SWEEP_INTERVAL` | 无 | `2` 秒 | `src/app/home.rs` | 失效项目清理间隔 |
| 新增 `PROJECT_VERSION_LIMIT` | 无 | `20` | `src/app/home.rs` | 项目下拉最多列出的历史版本 |
| 新增 `BACKUP_MIN_INTERVAL` | 无 | `300` 秒 | `src/app/backup.rs` | 两次历史版本的最小间隔 |
| 新增 `backup_retention` | 无 | 默认 `10`，可选 3/5/10/20/50 | `src/app/settings.rs` | 历史版本保留数 |
| 新增 ASIO 常量 | 无 | 注册表 `HKLM\SOFTWARE\ASIO`、最低有效采样率 `32 kHz`、缓冲 `32 ~ 4096`、支持 Float32LSB/Int32LSB/Int16LSB | `src/audio/asio.rs` | ASIO 驱动发现与协商 |
| 采样率/缓冲 | 固定 48 kHz / 512 帧 | 设置默认 48 kHz / 512 帧，可选 44.1 k~192 k / 128~4096 帧 | `src/app/settings.rs`、`src/audio/backend.rs` | 用户可配置 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 2.1 线程、4.9 音频系统/ASIO、7.10 主页与设置、12.2 限制、附录 B |
| `doc/关键常量.md` | 是 | 第 2、4、5、8 节相关常量 |
| `doc/框架工作流.md` | 是 | 第 2 节模块地图、第 3 节函数索引、1.5 保存流程、4 节快速入口、5.6 主页调用链 |
| `doc/项目简介.md` | 是 | 2.4 插件、2.6 工作流、技术栈、目录结构与已知限制 |
| `doc/代码规范.md` | 否 | 代码规范未变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | `cargo test --lib`：48 passed，0 failed，13 ignored |
| 手工验证 | 部分执行 | ASIO 诊断测试实际打开 ASIO4ALL v2 输出流（48 kHz / 2 ch / 512 frames）；GUI 交互未执行，环境中已有正在运行的 SurfingStudio 实例，未启动第二个界面实例 |

## 8. 风险与后续

- 风险：ASIO 后端依赖驱动自身的 `bufferSwitch` 行为与采样格式；当前仅在本机 ASIO4ALL 上验证，Realtek ASIO 等驱动需实机复测。音频输入设备与 MIDI 输出端口只记录启用状态。
- 后续：如需录音，可把 ASIO 输入通道与音频输入设备接入引擎；控制器延迟偏移可在录音补偿中实际生效。
