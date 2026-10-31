# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-10-31 19:11:40 (Asia/Shanghai) |
| AI 工具/模型 | TraeCode / 未标注模型 |
| 任务来源 | 验证 Kontakt 8 GUI 与 VOCALOID 外部窗口问题；修复关闭工程时 VOCALOID 窗口残留与 UI 卡死 |
| 关联分支/提交 | `feature/audio-import-surround-gpu-export` / 待提交 |
| 影响模块 | app, docs |
| 关联记录 | `20261031-182643-export-crash-plugin-lifecycle.md` |

## 2. 任务目标

- 在装有 Kontakt 8 / VOCALOID VSTi 的本机验证上一轮修复。
- 复现并解决：关闭工程后 VOCALOID 拉起的 `VOCALOID6 Editor` 窗口不关闭，且关闭流程卡死。

## 3. 验证过程与结果

- 使用 `--decode-log` 解码真实运行日志（`surfing-20261031-183314-p18908`、`surfing-20261031-185325-p24476`）：
  - Kontakt 8 编辑器打开耗时 632 ms（尺寸 1010×647），其后 57 秒 UI 帧耗时稳定在 0.4–0.7 ms，说明上一轮的“非阻塞插件锁 + 编辑器重绘限流”已消除 GUI 卡死。
  - 关闭工程时日志停在 `window.drop phase=workers_stopped`，随后 UI 线程 3 秒无新帧；窗口枚举显示 `VOCALOID6 Editor` 与工程窗口仍在，并出现 `UnhandledException` 对话框（属于本进程的 WPF 异常窗口）。
  - 结论：上一轮在关闭工程时直接调用 `IComponent::terminate`，VOCALOID 的 WPF 编辑器在 `OnClosing` 抛异常并弹出模态对话框，导致 UI 线程永久阻塞、外部窗口残留。
- 临时诊断（编译后已删除）：在主线程加载插件、打开 GUI、并发 try_lock 处理音频；VOCALOID 加载成功但直接调用 `terminate` 会触发上述问题；Kontakt 8 在独立诊断进程中偶发在 `load_plugin` 内原生崩溃，应用内多次加载可成功，说明该崩溃为插件初始化侧的非确定性行为，非宿主代码路径。

## 4. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/app/mod.rs` | 修改 | 新增 `close_plugin_owned_windows`：枚举本进程内插件创建、无 owner、标题不含 `SurfingStudio` 的可见顶层窗口，发送 `WM_CLOSE` 并泵消息最多 3 秒；`terminate_engine_plugins` 先执行该步骤，窗口全部关闭后才调用 `Engine::shutdown_plugins`，若窗口拒绝关闭则跳过终止并写 `phase=plugin_windows_still_open`，避免 UI 卡死 |
| `doc/技术文档.md` | 修改 | 9.2 应用工程时先关闭插件自建窗口、再终止组件；拒绝关闭时跳过终止 |
| `doc/框架工作流.md` | 修改 | 1.10 关闭工程流程加入 `plugin_windows_closed` / `plugin_windows_still_open` |
| `doc/项目简介.md` | 修改 | 关闭工程窗口的插件窗口处理说明 |
| `doc/日志规范.md` | 修改 | `window.drop` 新增 `plugin_windows_closed` / `plugin_windows_still_open` 阶段 |
| `doc/关键常量.md` | 修改 | 新增插件窗口关闭等待 3 s |

## 5. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 关闭工程时的插件窗口 | 直接 `IComponent::terminate`，VOCALOID 弹出 UnhandledException 并卡住 UI | 先 `WM_CLOSE` + 泵消息，窗口关闭后再 terminate | VOCALOID6 Editor 走正常关闭路径，UI 不再卡死 |
| 插件窗口拒绝关闭 | 无处理 | 跳过 terminate，保留实例并告警 | 避免模态对话框导致永久卡死；实例在进程退出前保留 |
| 多窗口安全 | 无差别枚举 | 跳过标题含 `SurfingStudio` 的宿主窗口与有 owner 的宿主插件编辑器 | 关闭一个工程不会误关其它工程的插件 GUI |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 9.2 加载/应用工程 |
| `doc/框架工作流.md` | 是 | 1.10 关闭工程 |
| `doc/项目简介.md` | 是 | 2.6 关闭工程窗口 |
| `doc/日志规范.md` | 是 | 5.2 `window.drop` 阶段 |
| `doc/关键常量.md` | 是 | 插件窗口关闭等待 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| 真实日志验证（Kontakt GUI） | 通过 | 编辑器 632 ms 打开，之后 57 s 帧耗时 0.4–0.7 ms，无卡死 |
| 真实日志验证（VOCALOID 关闭） | 失败（修复前） | 旧实现卡在 terminate，出现 `UnhandledException`，窗口残留 |
| `cargo fmt` | 通过 | 已执行 |
| `cargo check --all-targets -p surfing-studio` | 通过 | 无警告 |
| `cargo test --lib` | 通过 | 72 通过，0 失败，16 忽略 |
| 修复后端到端验证 | 未完成 | 本机诊断环境的 GUI 自动化与子进程日志不稳定；需在真实桌面手动确认：关闭工程后 `VOCALOID6 Editor` 消失、无 `UnhandledException`、可再次打开同一工程 |

## 8. 风险与后续

- 风险：若 VOCALOID 弹出“保存修改？”等模态对话框，`WM_CLOSE` 无法在 3 秒内关闭窗口，按新策略将跳过 `terminate` 并保留实例（进程退出前不释放），但不会卡死 UI。
- 风险：Kontakt 8 初始化存在非确定性的原生崩溃（`install_state` 读取被拒绝后偶发 `0xC0000005`），宿主无法在进程内完全隔离；后续可考虑插件进程隔离或加载失败重试。
- 后续：补充“插件窗口关闭”日志窗口标题，便于用户反馈时定位具体插件。
