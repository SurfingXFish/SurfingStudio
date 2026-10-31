# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-01 00:27:19 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户反馈：SoundBug 工程打开后插件内部信息缺失、音量没有读取到 |
| 关联分支/提交 | fix/ui-plugin-bugfix-batch / 本次提交 |
| 影响模块 | core / audio / ui / docs |
| 关联记录 | `history/ai/20261031-213834-soundbug-crash-ui-polish.md` |

## 2. 任务目标

- 修正 SoundBug `<长度>.<Base64>` 状态解码（`.` 是 Base64 字符而非分隔符），并确认解码结果能否交给插件。
- 导入 SoundBug `AUDIOCLIP.gain`（dB）为片段线性增益，让音频片段音量在混音中生效。
- 复核轨道音量导入（`PLUGIN type="volume"`）与主音量（`MASTERVOLUME`）。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/core/project_runtime/soundbug/convert.rs` | 修改 | `decode_soundbug_state` 把 `.` 映射为 Base64 值 63（标准表的 `/`），返回解码字节；`plugin_ref` 去掉长度校验参数，明确 `restore_state = false`；`import_audio_clip` 读取 `gain`（dB）换算为线性增益（`10^(dB/20)`，clamp 0~8）；更新状态解码单元测试 |
| `src/core/model/clip.rs` | 修改 | `AudioSource` 新增 `gain: f32`，serde 默认 `1.0` |
| `src/audio/engine/render.rs` | 修改 | `mix_audio_clips` 把 `source.gain` 乘进左右声道与峰值统计 |
| `src/ui/arrangement/mod.rs` | 修改 | 拖入音频新建片段时写入 `gain: 1.0` |
| `src/core/project_runtime/sfsd/mod.rs` | 修改 | 测试用 `AudioSource` 补 `gain: 1.0` |
| `src/core/project_runtime/soundbug/import.rs` | 修改 | 示例工程测试新增「片段增益必须被导入」断言 |
| `doc/技术文档.md` | 修改 | 3.3 音频片段字段、9.3 SoundBug 转换规则与状态格式结论 |
| `doc/框架工作流.md` | 修改 | 1.9 `import_audio_clip` 职责 |
| `doc/项目简介.md` | 修改 | 2.3 音频片段 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| SoundBug 状态解码 | `.` 被当作分隔符丢弃 | `.` 按 Base64 值 63 参与解码 | 解码长度与声明长度完全一致（示例：TDR Nova 6281 字节） |
| 插件状态恢复 | 一律不恢复 | 仍不恢复，但保留正确解码的原始容器 | 规避 Ample Sound 等 `setState` 崩溃 |
| 音频片段音量 | `AUDIOCLIP.gain` 忽略 | dB → 线性增益参与混音 | 导入后音频片段响度与 SoundBug 接近 |
| 轨道/主音量 | 已导入 | 复核确认：轨道 `volume` 0.43~1.0、主音量 0.7408182 | 无需改动 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 片段增益范围 | 无 | `0.0 ~ 8.0` | `src/core/project_runtime/soundbug/convert.rs` | SoundBug dB 值换算后的 clamp 上限（约 +18 dB） |
| `gain` 默认值 | 无 | `1.0` | `src/core/model/clip.rs` | `.sfsd` 旧工程与新建片段使用原始音量 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 3.3、9.3 |
| `doc/关键常量.md` | 否 | 增益范围写在函数内，未抽为全局常量 |
| `doc/框架工作流.md` | 是 | 1.9 |
| `doc/项目简介.md` | 是 | 2.3 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| 状态解码逆向 | 完成 | 示例工程全部插件状态解码长度与 `<长度>` 声明一致 |
| 载荷可用性验证 | 不可用 | 解码结果有固定头部（`54 c8 cb …`）、共享固定尾部块、无可读字符串；zlib / raw deflate / lzma / bz2 均无法解压；直接 `load_state` 到 TDR Nova 返回 `Ok` 但参数与状态均未改变 |
| `cargo fmt --all` | 通过 | 已运行 |
| `cargo test --lib soundbug` | 通过 | 6 passed / 0 failed / 1 ignored（含新增增益断言） |

## 8. 风险与后续

- 结论：SoundBug 的插件状态是其私有容器（疑似加密或自定义压缩），**无法**还原为 VST3/VST2 原生状态；导入后插件需在 SurfingStudio 中重新调音色再保存，之后即可正常恢复。
- 风险：`AudioSource.gain` 目前只在导入时写入，界面尚未提供片段增益编辑；如需要可后续在片段检查器中暴露。
- 后续：若 SoundBug 提供导出/解密工具，可再把该私有容器映射为宿主状态。
