# AI 代码变更记录

## 1. 基本信息

| 字段 | 内容 |
| --- | --- |
| 记录时间 | 2026-11-14 23:04:20 (Asia/Shanghai) |
| AI 工具/模型 | Trae / 当前会话模型 |
| 任务来源 | 修正 AI 工作台比例；统一下拉箭头方向与动画；主页插件列表改为动画下拉 |
| 关联分支/提交 | feature/ui-visual-consistency / 本次提交 |
| 影响模块 | ui, app |
| 关联记录 | 20261114-224659-ai-overlay-dropdowns.md |

## 2. 任务目标

- 修正 AI 工作台“侧栏过窄、主区过宽、发送按钮换行、快捷指令被裁切”的比例问题。
- 统一下拉/折叠箭头规则：展开向下；收起时最左侧向右、最右侧向左；选择菜单固定向下。
- 为折叠展开与箭头旋转加入 0.18 秒补间动画。
- 主页插件列表改为带动画的下拉折叠，详情在行内展开。

## 3. 实际改动

| 文件 | 类型 | 改动说明 |
| --- | --- | --- |
| `src/ui/theme.rs` | 修改 | 新增 `DropdownDir` / `dropdown_angle` / `paint_dropdown_angle` / `paint_dropdown_dir`；`DROPDOWN_ANIM_SECS = 0.18`；ComboBox 图标统一向下；删除旧的 `paint_dropdown` |
| `src/ui/ai_panel.rs` | 修改 | 侧栏 `ui.set_min_width(AI_SIDEBAR_W)` 强制 240 点；快捷指令改为短标签 + `horizontal_wrapped`；工具栏改为 `horizontal` + `right_to_left` 放置发送按钮；任务耗时折叠箭头按 `openness` 从向左补间到向下 |
| `src/app/home.rs` | 修改 | 插件品牌标题与插件行改用 `CollapsingState` 动画折叠；品牌箭头在左缘从右补间到向下，插件行箭头在右缘从左补间到向下；删除右侧抽屉与相关常量/状态；详情改为行内 `PANEL` 卡片（左路径/标识，右厂商/分类/能力）；Agent 侧栏箭头改为 0.18 秒补间 |
| `src/app/mod.rs` | 修改 | 删除 `home_plugin_collapsed` / `home_plugin_selected` 状态 |
| `src/ui/effect_picker.rs` / `src/ui/instrument_picker.rs` | 修改 | 厂商分组箭头改用方向枚举（左缘向右） |
| `doc/代码规范.md` | 修改 | 明确下拉方向规则与动画要求、检查项 |
| `doc/关键常量.md` | 修改 | 新增 `DROPDOWN_ANIM_SECS`、插件详情卡片说明；删除抽屉宽度常量 |
| `doc/技术文档.md` | 修改 | 7.10 插件页改为动画折叠 + 行内详情；7.14 AI 侧栏强制 240 点、胶囊换行、发送右对齐 |
| `doc/框架工作流.md` | 修改 | 主页与 AI 面板函数表、主题下拉函数表 |
| `doc/项目简介.md` | 修改 | 插件页动画下拉、AI 面板比例与箭头动画 |
| `history/ai/20261114-230420-dropdown-motion-plugin-inline.md` | 新增 | 本记录 |

## 4. 工作流变化

| 项目 | 变化前 | 变化后 | 影响 |
| --- | --- | --- | --- |
| AI 侧栏宽度 | 只设子控件宽度，父布局按内容收缩到约 171 点 | `set_min_width(240)` 强制占满 | 侧栏/主区比例与设计一致，新任务按钮恢复整宽 |
| 快捷指令 | 横向滚动，首个胶囊被裁切 | 短标签 + 自动换行 | 所有指令完整可见 |
| 发送按钮 | `horizontal_wrapped` + 手工 spacer，掉到第二行 | `horizontal` + `right_to_left` 子布局 | 固定在同一行最右侧 |
| 箭头规则 | 统一向下或随展开翻转 | 展开向下；收起左缘向右、右缘向左；选择菜单向下 | 方向与容器位置一致 |
| 折叠动画 | 立即出现/消失 | `CollapsingState` 高度补间 + 箭头按 `openness` 旋转（0.18 秒） | 展开/收起更连贯 |
| 插件详情 | 右侧抽屉，挤压列表 | 行内展开卡片，品牌与行均可折叠 | 不再遮挡列表，详情与行直接对应 |

## 5. 关键常量变化

| 常量 | 变化前 | 变化后 | 位置 | 说明 |
| --- | --- | --- | --- | --- |
| `DROPDOWN_ANIM_SECS` | 无 | `0.18` 秒 | `src/ui/theme.rs` | 箭头与折叠补间 |
| `AI_SIDEBAR_W` 使用方式 | 仅作为期望宽度 | 同时 `set_min_width` | `src/ui/ai_panel.rs` | 防止父布局收缩 |
| `PLUGIN_DRAWER_MIN_W` / `PLUGIN_DRAWER_MAX_W` | `280 / 340` | 删除 | `src/app/home.rs` | 不再使用右侧抽屉 |
| `PLUGIN_LIST_MIN_W` | `320` | 删除 | `src/app/home.rs` | 列表占满宽度 |

## 6. 文档联动

| 文档 | 是否更新 | 具体位置/说明 |
| --- | --- | --- |
| `doc/代码规范.md` | 是 | 7.6.3 下拉方向与动画；7.6.7 检查项 |
| `doc/关键常量.md` | 是 | 第 8 / 12 节 |
| `doc/技术文档.md` | 是 | 7.10 / 7.14 |
| `doc/框架工作流.md` | 是 | 模块表、主题表、AI 面板表 |
| `doc/项目简介.md` | 是 | 2.4 插件、2.7 AI 编曲 |

## 7. 验证

| 项目 | 结果 | 说明 |
| --- | --- | --- |
| `cargo fmt --all` | 通过 | 无格式差异 |
| `cargo check --all-targets` | 通过 | 无编译错误 |
| `cargo test` | 通过 | 172 passed / 23 ignored |
| 手工验证 | 待用户确认 | 需启动确认侧栏 240 点比例、发送按钮右对齐、插件行展开动画与箭头方向 |

## 8. 风险与后续

- 风险：`CollapsingState` 的展开高度动画依赖 egui 内部默认时长；箭头使用同一 `openness`，两者同步。若后续统一动画时长，可改为自定义高度补间。
- 风险：选择菜单（ComboBox/菜单按钮）按“向下打开”的语义固定向下，不套用左右位置规则；如果希望右缘菜单关闭时指向左，需要单独传入方向。
- 后续：可把品牌/插件折叠状态写入设置，跨会话保留；目前折叠状态存在 egui 内存中。
