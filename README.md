# SurfingStudio

一个用 Rust 编写的、以 MIDI 为核心的音乐工作站（DAW）。内置合成器与鼓机开箱即用，同时支持 VST3 / VST2 插件、音频片段、自动化与实时渲染播放。

- **轻量**：单个原生可执行文件，不依赖 Electron、浏览器或外部音频服务。
- **以 MIDI 为中心**：编曲、钢琴卷帘、控制器自动化是主路径，音频片段作为补充。
- **开箱即用**：内置复音合成器、鼓机与九种原生乐器，即使没有安装任何插件也能发声。
- **专业能力**：VST3 原生编辑器、VST2 支持、插件状态持久化、实时渲染、多通道 / 环绕声路由与独立导出窗口。

当前版本 `1.0.0`，主要面向 Windows 桌面，也可在安卓平板 / 手机上运行（NativeActivity + OpenGL ES，单窗口模式）。

## 主要功能

- **编曲与轨道**：多轨编曲视图，轨道可展开为控制器 / 效果器子轨；主输出固定置顶；工程支持 2.0 / 5.1 / 7.1 输出与每轨通道对路由；循环区间、播放头与自动停止。
- **MIDI 编辑**：钢琴卷帘（音阶高亮、网格吸附、绘制 / 移动 / 缩放 / 框选 / 切割、量化、力度编辑）；控制器卷帘支持音量、声像、延音、弯音自动化。
- **音频片段**：拖入 WAV / MP3 / FLAC / AAC·M4A / OGG / AIFF 等文件即创建片段，波形缩略图、裁剪 / 移动 / 跨轨拖动，支持弯音变调。
- **插件**：VST3 扫描与原生编辑器、VST2 支持、按品牌分组的插件浏览器、增量扫描缓存、参数与状态随工程保存。
- **内置乐器与效果器**：Karplus-Strong 拨弦、钢琴非谐分音、弓弦建模、808/909 风格鼓机；八种原生效果器（过载、失真、法兹、合唱、相位、延时、混响、压缩）。
- **工作流**：多窗口工作区、主页（项目 / 模板 / 插件 / 提示）、19 种风格模板与 6 个范例工程、独立设置窗口、独立导出窗口、自动保存、崩溃恢复、历史版本、可自定义快捷键。
- **工程格式**：`.sfsd`（SFS2 分块二进制容器，MessagePack 清单 + 插件状态块 + 波形峰值缓存），兼容导入 SoundBug `.sndp`。
- **诊断**：每次启动写入二进制 `.sflog` 日志（`--decode-log` 可解码），记录卡顿、资源占用与退出原因。

## 技术栈

| 领域 | 选型 |
| --- | --- |
| 语言 | Rust 2021（`rust-version = 1.85`） |
| GUI | eframe / egui 0.34（安卓为 NativeActivity + OpenGL ES） |
| 音频输出 | cpal 0.18（系统默认 / WASAPI）与原生 ASIO 后端（Windows） |
| MIDI 输入 | midir 0.11 |
| 插件 | vst3-host 0.10（本地 patch）、vst 0.4（VST2） |
| 序列化 | serde / rmp-serde（MessagePack）/ zstd / crc32fast |
| 其他 | hound（WAV）、symphonia（MP3 / FLAC / AAC·M4A / OGG / AIFF / CAF / ALAC）、rfd（文件对话框）、wgpu（GPU 离线渲染与实时限幅） |

## 快速开始

环境要求：Rust stable 工具链（edition 2021）、Windows 10/11 与可用的音频输出设备。

```bash
cargo run              # 启动后进入主页，新建 / 打开工程即可开始
cargo build --release
cargo test
```

打开工程支持 `.sfsd` 与 SoundBug `.sndp`（导入后需另存为 `.sfsd`）。

安卓构建（需要 NDK、安卓目标与 JDK）：

```powershell
$env:JAVA_HOME = 'C:\Program Files\Java\jdk-21'
powershell -ExecutionPolicy Bypass -File platforms\android\build.ps1 -Abi arm64-v8a -Install -Run
```

## 目录结构

```text
src/
├─ main.rs          # 薄二进制入口：创建 eframe 根窗口
├─ lib.rs           # 库根：模块声明
├─ platform/        # 平台运行时（desktop / android 后端、SAF）
├─ app/             # 应用状态机与多窗口外壳（主页、设置、保存、插件、自动保存…）
├─ core/            # 纯数据层：模型、工程运行时（sfsd / soundbug）、MIDI 输入
├─ audio/           # 实时引擎、内置乐器与效果器、音频 I/O、ASIO、GPU 渲染
├─ plugin/          # 插件运行时框架与 VST3 / VST2 后端
├─ diag/            # 进程级诊断日志（.sflog）
└─ ui/              # 立即模式视图：编曲区、钢琴卷帘、控制器、检查器、浏览器…
examples/           # 范例工程
platforms/android/  # 安卓入口 crate 与打包脚本
tools/vst2-fixture/ # 测试用极简 VST2 乐器
vendor/vst3-host/   # 本地 patch 的 VST3 宿主库
doc/                # 项目简介、技术文档、代码规范、关键常量、框架工作流
```

## 文档

- [项目简介](doc/项目简介.md) — 完整功能清单、技术优势与已知限制
- [技术文档](doc/技术文档.md) — 模块职责、数据流与实现细节
- [协作规范](doc/协作规范.md) — 分支、提交信息、Issue 与版本命名规则
- [代码规范](doc/代码规范.md) · [关键常量](doc/关键常量.md) · [日志规范](doc/日志规范.md) · [框架工作流](doc/框架工作流.md)

## 已知限制

- 音频导入支持到 WAV / MP3 / FLAC / AAC·M4A / OGG Vorbis / AIFF / CAF / ALAC 与 MKV/WebM 音频轨；WMA、APE、Opus 需先转码。
- 原生 GUI 内嵌仅支持 Windows + VST3；VST2 使用独立编辑器窗口。
- 主效果器固定 2 in / 2 out，环绕声布局下只处理前左/前右；侧链尚未实现。
- 插件参数自动化写入自动化轨尚未实现；自动化目前面向音量、声像、延音、弯音。
- 安卓为单窗口模式，不加载 VST 插件，界面沿用桌面布局。

完整限制说明见 [项目简介 · 已知限制](doc/项目简介.md#7-已知限制)。
