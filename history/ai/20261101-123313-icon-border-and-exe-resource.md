# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-01 12:33:13 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户反馈：① exe 文件图标的颜色没有跟着 SVG 改；② 主页图标与深色背景融为一体，需要加边框 |
| 关联分支/提交 | fix/ui-plugin-bugfix-batch / 本次提交 |
| 影响模块 | assets / build / ui / app / docs |
| 关联记录 | `history/ai/20261101-114916-logo-dark-grey-and-in-app-icon.md` |

## 2. 任务目标

- 让 `surfing-studio.exe` 的文件图标与仓库内 `assets/icon.svg` 一致：此前后端只设置了窗口/任务栏图标（`ViewportBuilder::with_icon`），可执行文件本身没有图标资源，资源管理器一直显示旧/默认图标。
- 给 SVG 图标加深色面板上可见的描边，避免深灰底 `#2E2E33` 与深色界面背景融为一体；应用内矢量图标同步加描边。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `assets/icon.svg` | 修改 | 圆角底内缩为 `x=5 y=5 w=h=502 rx=107`，增加 `stroke="#5C5C66" stroke-width="10"` 描边 |
| `assets/icon.png` / `assets/icon-256.rgba` | 修改 | 由新 SVG 重新渲染 256×256，并把圆角外区域改为透明（此前为不透明白底） |
| `assets/icon.ico` | 新增 | 16/32/48 位图条目（32bpp + AND 掩码）与 256 PNG 条目共四种尺寸，供 Windows 资源管理器/任务栏使用 |
| `build.rs` | 新增 | 用 Windows SDK 的 `rc.exe` 把 `icon.ico` 编译为资源并经 `cargo:rustc-link-arg-bins` 链接进 exe；找不到 `rc.exe` 时回退 `windres`；都不可用时只打印 `cargo:warning`，不阻断构建 |
| `src/ui/theme.rs` | 修改 | 新增 `LOGO_BORDER`（`#5C5C66`） |
| `src/app/panels.rs` | 修改 | `paint_studio_logo` 在填充底色后按 `(10 × scale).max(1)` 绘制 `StrokeKind::Inside` 描边 |
| `doc/技术文档.md` | 修改 | 桌面窗口图标段落补充描边、透明背景、`.ico` 与 `build.rs` 内嵌说明 |
| `doc/关键常量.md` | 修改 | `LOGO_BG` / `LOGO_BLUE` 一行扩展为包含 `LOGO_BORDER` |
| `doc/项目简介.md` | 修改 | 2.6 应用图标说明补充 exe 文件图标与描边 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| Windows 文件图标 | 仅窗口图标，exe 无图标资源 | `build.rs` 把 `assets/icon.ico` 链接进 exe | 资源管理器与任务栏显示同一套图标 |
| 图标渲染 | SVG 圆角外为不透明白底 | 渲染时使用透明背景 | 图标在浅色/深色背景下都不出现白框 |
| 应用内图标 | 仅深灰底，无轮廓 | 深灰底 + `#5C5C66` 描边 | 贴在深色面板上仍能分辨轮廓 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `LOGO_BORDER` | 无 | `#5C5C66` | `src/ui/theme.rs` | 矢量图标描边颜色，与 SVG `stroke` 一致 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 桌面窗口图标（3.x） |
| `doc/关键常量.md` | 是 | 插件窗口/资源表 |
| `doc/框架工作流.md` | 否 | `paint_studio_logo` 职责未变，仅多一条描边绘制命令 |
| `doc/项目简介.md` | 是 | 2.6 应用图标 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 已运行 |
| `cargo build --offline` | 通过 | 无错误、无 `cargo:warning`，`rc.exe` 找到并链接成功 |
| `cargo test --offline` | 通过 | 74 passed，0 failed |
| exe 内嵌图标 | 通过 | `ExtractAssociatedIcon` 提取：四角透明、底色 `46,46,51`、中心 `0,120,212` |
| PNG 像素抽查 | 通过 | 角点 alpha 0；描边 `92,92,102`；底色 `46,46,51` |

## 8. 风险与后续

- 风险：Windows 会缓存文件图标，已存在的旧 exe 可能需要刷新图标缓存（或复制到新路径）才能看到新图标。
- 后续：安卓启动器图标仍未接入；若需要，可从同一 SVG 渲染各密度 `mipmap`。
