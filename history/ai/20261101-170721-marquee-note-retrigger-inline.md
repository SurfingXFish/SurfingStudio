# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-01 17:07:21 (Asia/Shanghai) |
| AI 工具/模型 | Trae Code / 当前会话模型 |
| 任务来源 | 用户反馈：框选无法跟随上下移动、前后完全一致的同音高音符第二个不发声、VST 效果器界面超规时自动弹出独立窗口 |
| 关联分支/提交 | fix/marquee-note-retrigger-inline-gui / 本次提交 |
| 影响模块 | audio, app, ui, docs |
| 关联记录 | 无 |

## 2. 任务目标

- 钢琴卷帘框选锚点跟随纵向滚动（此前只跟随横向滚动/缩放）。
- 同音高前后重叠的音符，后一个不再被前一个 Note Off 掐断而无声。
- 效果器插件界面大于卡片时不再自动跳出独立窗口，改为卡片提示、由用户双击标题手动打开。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/ui/piano_roll/mod.rs` | 修改 | 框选起点改存绝对音高行 `top_pitch - (pos.y - grid_top) / note_h`，绘制时用当前 `top_pitch` 还原屏幕 y，纵向滚动后选择框跟随内容 |
| `src/ui/mod.rs` | 修改 | `PianoDrag::Marquee.start_row` 注释改为“内容坐标：起拍 + 绝对音高行” |
| `src/audio/engine/render.rs` | 修改 | `refresh_events` 先展开音符绝对时间跨度并按音高/起点排序，同音高重叠时把较早音符的结束截断到较晚音符起点；补充两条事件表测试 |
| `src/audio/engine/mod.rs` | 修改 | 新增实时渲染测试：0.01 beat 重叠的同音高音符必须重新起音（RMS 阈值） |
| `src/app/plugins.rs` | 修改 | `place_inline_editor` 溢出时只隐藏内嵌并记入 `inline_failed`，不再调用 `open_editor`；同步 `place_inline` 函数头 |
| `doc/技术文档.md` | 修改 | 3.6 同音高重叠截断规则、6.6 超规不自动弹窗、7.3 钢琴卷帘框选锚点 |
| `doc/项目简介.md` | 修改 | 2.2 框选跟随纵向滚动、2.4 超规提示行为、3.1 同音高重叠不掐音 |
| `doc/关键常量.md` | 修改 | `INLINE_OVERFLOW_TOLERANCE` 说明改为隐藏内嵌并提示，不自动弹窗 |
| `doc/框架工作流.md` | 修改 | `refresh_events` 与 `place_inline_editor` 职责说明 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| 钢琴卷帘框选 | 锚点只存纵向屏幕偏移，纵向滚动后框体起点不动 | 锚点存绝对音高行，纵向滚动/缩放跟随 | 框选结果与视觉一致 |
| 同音高重叠音符 | 事件表按原时长输出，前一个 Note Off 落在后一个 Note On 之后，把后一个音符立即掐断 | 较早音符在后一个音符起点结束，同一 beat 先关后开 | 前后相同音符都能发声；首尾相接与完全重合的复制音符行为不变 |
| 效果器界面大于卡片 | 自动调用 `open_editor` 弹出独立窗口 | 只记 `inline_failed` 并在卡片提示，双击标题才打开 | 效果器链不再被多个独立窗口铺满 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| 无 | - | - | - | 未新增或修改常量；`INLINE_OVERFLOW_TOLERANCE` 数值与用途不变 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/技术文档.md` | 是 | 3.6、6.6、7.3 |
| `doc/关键常量.md` | 是 | 插件窗口一节 `INLINE_OVERFLOW_TOLERANCE` 说明 |
| `doc/框架工作流.md` | 是 | 3.2 `refresh_events` 索引、编辑器生命周期 `place_inline_editor` |
| `doc/项目简介.md` | 是 | 2.2、2.4、3.1 |
| `doc/代码规范.md` | 否 | 规范本身无变化 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 已运行 |
| `cargo test --lib` | 通过 | 77 passed / 0 failed / 17 ignored |
| 新增测试 | 通过 | `same_pitch_overlap_trims_the_earlier_note`、`same_pitch_adjacent_notes_keep_both_note_offs`、`overlapping_same_pitch_notes_retrigger` |
| 手工验证 | 未执行 | 未启动桌面应用做框选与插件 GUI 的手工验证 |

## 8. 风险与后续

- 风险：同音高重叠截断改变了音符实际发声长度；若用户刻意让同音高长音叠加（如双音色叠加），较早的音符会在较晚音符起点收声。这与常见宿主“同音重触发”行为一致。
- 风险：界面超规的插件不再自动打开窗口，用户需要双击卡片标题才能看到完整 GUI。
- 后续：可在卡片提示中补充插件名称与所需尺寸，便于判断是否需要手动打开。
