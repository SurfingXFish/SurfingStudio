# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-24 22:43:00 (Asia/Shanghai) |
| AI 工具/模型 | Trae / AI 编码助手 |
| 任务来源 | 调整日志内容格式：放弃文本可读性，追求信息量与存储/写入效率 |
| 关联分支/提交 | 未知 / 未知 |
| 影响模块 | diag, main, docs, build |
| 关联记录 | `history/ai/20261024-223145-project-close-plugin-teardown-fix.md` |

## 2. 任务目标

- 把 `.sflog` 从 7 列文本改为紧凑的二进制格式，提高单条记录的信息密度与写入效率。
- 扩大记录内容：线程 id、UI 帧号、进程资源（内存/CPU/句柄/线程）、帧耗时聚合。
- 提供离线解码器，保证二进制日志仍可分析。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/diag/mod.rs` | 修改 | 改为 `.sflog` v2 二进制：24 字节文件头前缀 + 头文本；每条记录 32 字节头（kind/level/seq/thread_id/t_us/frame/payload_len/crc32）+ 变长负载；事件负载使用 varint 与长度前缀字符串；新增 RESOURCE 记录（工作集/峰值/私有字节/CPU/句柄/线程，每 5 秒与卡死时采样）、`ui.frames` 每 600 帧聚合；`crash`/`finish` 写 FOOTER 记录 |
| `src/diag/format.rs` | 修改 | 删除文本转义；新增 LEB128 varint 与长度前缀字符串编解码；保留本地时间与日志清理；补 varint/字符串往返测试 |
| `src/diag/decode.rs` | 新增 | 二进制日志解码器：校验魔数/版本/CRC32，输出可读文本；`export_text` 写出同名 `.txt`；补解码测试 |
| `src/main.rs` | 修改 | 新增 `--decode-log <文件>` 命令行模式（先于日志初始化执行），解码后退出 |
| `Cargo.toml` | 修改 | 为 `winapi` 增加 `psapi` / `handleapi` / `tlhelp32` 特性，用于资源采样（无新增第三方 crate） |
| `doc/日志规范.md` | 重写 | 改为二进制格式说明：文件头/记录头/编码原语/负载布局/事件目录/解码器用法/判读流程 |
| `doc/技术文档.md` | 修改 | 1.2 模块职责（format/decode）；13.1~13.3 改为二进制写入模型、资源采样与帧聚合 |
| `doc/关键常量.md` | 修改 | 诊断日志表更新为 v2 常量（FORMAT_VERSION、头部长度、资源采样间隔、帧聚合窗口等） |
| `doc/框架工作流.md` | 修改 | 启动流程加入 `--decode-log`；模块地图与 3.6 函数索引加入解码器与资源采样 |
| `doc/项目简介.md` | 修改 | 诊断日志改为二进制 + 解码器说明；目录结构补充 `decode.rs` |
| `doc/代码规范.md` | 修改 | 1.4 判定表要求格式变更同步 `decode.rs` 与 `FORMAT_VERSION` |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 日志写入 | 每条记录拼成 7 列文本行后写入 | 32 字节二进制头 + 变长负载，单次 `write_all`；字符串用 varint 长度前缀 | 单条记录更小，写入更快，无格式化开销 |
| 记录内容 | seq/t 相对时间/级别/scope/event/dur/字段 | 额外携带线程 id、UI 帧号、负载 CRC32；新增资源采样与帧耗时聚合 | 可分析线程归属、内存/句柄/CPU 走势与帧率 |
| 日志读取 | 任意文本编辑器直接打开 | `surfing-studio.exe --decode-log <文件>` 生成 `.txt`；`decode.rs` 为参考实现 | 阅读需解码，但格式可被工具直接解析 |
| 启动入口 | 直接初始化日志并创建窗口 | 先处理 `--decode-log`（命中则解码后退出），再初始化日志 | 解码模式不产生新日志、不创建窗口 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `FORMAT_MAGIC` | `"SFLG1"`（文本行） | `0x474C4653`（小端 `SFLG`） | `src/diag/mod.rs` | 二进制魔数 |
| `FORMAT_VERSION` | 无（由文本首行表示） | `2` | `src/diag/mod.rs` | 显式格式版本 |
| `HEADER_PREFIX_LEN` | 无 | `24` 字节 | `src/diag/mod.rs` | 文件头固定前缀 |
| `RECORD_HEADER_LEN` | 无 | `32` 字节 | `src/diag/mod.rs` | 每条记录固定头 |
| `RESOURCE_SAMPLE_INTERVAL_MS` | 无 | `5000` ms | `src/diag/mod.rs` | 进程资源采样间隔 |
| `UI_FRAME_STATS_FRAMES` | 无 | `600` 帧 | `src/diag/mod.rs` | 帧耗时聚合窗口 |
| 日志级常量 | `UI_SLOW_FRAME_MS` / `UI_SLOW_FRAME_REPEAT_MS` / `UI_STALL_MS` / `UI_STALL_REPEAT_MS` / `EXIT_WATCHDOG_MS` / `WATCHDOG_TICK_MS` / `LOG_KEEP_FILES` | 不变 | `src/diag/mod.rs` | 行为阈值保持不变 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 1.2 模块职责、13.1~13.3 二进制写入模型与资源采样 |
| `doc/关键常量.md` | 是 | 10 诊断日志表 |
| `doc/框架工作流.md` | 是 | 1.1 启动流程、2 模块地图、3.6 `diag` 索引、4 快速入口 |
| `doc/项目简介.md` | 是 | 2.6 诊断日志、3.5 可维护性、6 目录结构 |
| `doc/代码规范.md` | 是 | 1.4 日志格式变更规则 |
| `doc/日志规范.md` | 是 | 全文重写为 v2 二进制规范 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test` | 通过 | 61 passed、16 ignored；新增 varint/字符串往返、事件负载往返、解码器测试 |
| `cargo clippy --all-targets` | 通过 | 无错误 |
| 手工验证（写入） | 通过 | 启动约 8 秒后关闭：日志 2012 字节 / 19 条记录；含首条与 5 秒资源采样，`resource ws=200.5MiB ... handles=783 threads=44` |
| 手工验证（解码） | 通过 | `surfing-studio.exe --decode-log <日志>` 生成同名 `.txt`，含文件头、事件、`dur=`、资源行与 `# end reason=home_closed` |

## 8. 风险与后续

- 风险：日志不再能用文本编辑器直接查看，必须使用解码器或按规范解析。
- 风险：资源采样使用 Toolhelp 快照枚举系统线程，频率为 5 秒一次，开销可忽略；非 Windows 平台不写资源记录。
- 风险：格式为 v2，旧版文本 `.sflog` 不再可读；解码器会以“魔数不匹配”拒绝。
- 后续：如需二进制日志的可视化时间线，可基于 `decode.rs` 输出结构化 JSON。
