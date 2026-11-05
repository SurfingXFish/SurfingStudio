# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-05 16:47:32 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | “闪退”，附日志 `surfing-20261105-164221-p22272.sflog` |
| 关联分支/提交 | feature/neural-timbre-ml / 未知 |
| 影响模块 | main, audio, diag, docs |
| 关联记录 | `history/ai/20261105-164136-effect-editor-ui-thread-deadlock.md` |

## 2. 任务目标

- 定位 `0xC0000005` 闪退：日志只有 `exception_code` 与 `phase=native`，没有崩溃地址、模块与调用栈。
- 同时为效果器加载补上日志，下一次崩溃/卡死能直接看出是哪一个插件。

## 3. 日志证据

- 进程运行到 32.8 s，帧数 3585，无播放、无新的 ARA 日志。
- 最后一条 FATAL：`app.crash/unhandled_exception exception_code=0xC0000005 phase=native`，崩溃线程 `tid=576`（后台同步线程，而不是界面线程 6344）。
- 之前一次同类崩溃同样缺少定位信息。

## 4. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/main.rs` | 修改 | 新增 `describe_address`：用 `GetModuleHandleExW(FROM_ADDRESS)` + `GetModuleFileNameW` 把地址转成“模块名+0xRVA”；未处理异常记录新增 `fault` 与 `stack`（`RtlCaptureStackBackTrace` 16 帧） |
| `src/audio/engine/mod.rs` | 修改 | 效果器加载新增 `engine.effect.load` span（名称、路径、轨道、线程 id），成功/失败都结束该 span |
| `Cargo.toml` | 修改 | winapi 增加 `winbase` feature（调用栈相关常量） |
| `doc/框架工作流.md` | 修改 | 入口模块说明补充崩溃定位字段 |

## 5. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 未处理异常记录 | 只有异常码 | 异常码 + 崩溃模块/RVA + 16 帧调用栈（同样是模块/RVA） | 无需调试器即可判断是哪个插件或哪段宿主代码 |
| 效果器加载 | 无日志 | `engine.effect.load` 记录名称/路径/轨道/线程 | 崩溃或卡死时能看出最后一个正在加载的效果器 |

## 6. 关键常量变化

无。

## 7. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/框架工作流.md` | 是 | 2. 模块地图入口行 |
| `doc/技术文档.md` | 否 | 本次只增加诊断字段 |
| `doc/关键常量.md` | 否 | 无常量变化 |
| `doc/项目简介.md` | 否 | 无用户可见功能变化 |

## 8. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt` | 通过 | 已运行 |
| `cargo test --lib` | 通过 | 116 passed / 23 ignored |
| 实机验证 | 待执行 | 若再次闪退，日志会带 `fault=` 与 `stack=`；据此定位到具体插件/代码 |

## 9. 风险与后续

- 风险：崩溃发生在后台同步线程（`tid=576`），与上一版新增的“界面线程重建效果器”可能有关联；若新日志显示崩溃位于某个效果器的 `engine.effect.load` 期间，下一步应禁止该插件在后台上/重建或改为延迟创建。
- 后续：根据 `fault` 的模块名决定是插件内部问题还是宿主代码问题。
