//! Help window.

use crate::app::{ShortcutAction, SurfingApp};
use crate::ui::theme;
use eframe::egui::{self, RichText};

impl SurfingApp {
    pub fn ui_help(&mut self, ctx: &egui::Context) {
        if !self.show_help {
            return;
        }
        let mut open = true;
        egui::Window::new("SurfingStudio 帮助")
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .default_width(520.0)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("以 MIDI 为核心的 Rust 音乐工作站")
                        .size(15.0)
                        .color(theme::ACCENT)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.label("• 顶部应用栏：点击左上角的菜单按钮显示「文件 / 编辑 / VST 插件 / 帮助」，此时左上角只保留菜单；点击其他区域或按 Esc 收起");
                ui.label("• 项目栏显示工程名与未保存圆点，下拉可打开或新建工程；拖动应用栏移动窗口，双击应用栏最大化");
                ui.label("• 双击 MIDI 片段 / 子轨 = 打开编辑卷帘；双击空白轨道 = 创建 MIDI 片段");
                ui.label("• 单击主轨或子轨只会选中（用于复制/粘贴）；双击才会进入编辑");
                ui.label("• 拖动卷帘顶部工具条的空白处（按钮和滑块以外，或长按卷帘空白处拖动）可把卷帘拖出为独立窗口");
                ui.label("• 独立卷帘支持多个、最小化/最大化、跨屏移动；再次双击同一轨道只会将其置顶");
                ui.label(format!(
                    "• 选中子轨 / 效果器后按 {}，选中目标轨道后按 {} 即可复制（已有同名子轨时询问覆盖）",
                    self.settings.shortcut_label(ShortcutAction::Copy),
                    self.settings.shortcut_label(ShortcutAction::Paste)
                ));
                ui.label("• 将 WAV 拖到主轨即可创建音频片段；轨道 → 添加音频轨道；音频支持音量/声像/弯音动态变调");
                ui.label("• 同一轨道不能混放 MIDI 与音频片段；子轨不支持时显示灰色并划去");
                ui.label(format!(
                    "• 顶部鼠标模式：选择 / 绘制 / 切割（默认快捷键 {} / {} / {}，按住 Shift 临时绘制）",
                    self.settings.shortcut_label(ShortcutAction::ToolSelect),
                    self.settings.shortcut_label(ShortcutAction::ToolDraw),
                    self.settings.shortcut_label(ShortcutAction::ToolCut)
                ));
                ui.label("• 选择：拖动片段或音符；在空白处拖拽可框选（靠近两端自动滚动）");
                ui.label("• 绘制：轨道内拖出新片段，钢琴卷帘内新建音符；切割：拆分片段/音符");
                ui.label(format!(
                    "• 控制器卷帘画笔：画笔 / 直线 / 平滑 / 擦除（{} 循环切换，{} / {} / {} / {} 直接选择）",
                    self.settings
                        .shortcut_label(ShortcutAction::ControllerToolNext),
                    self.settings
                        .shortcut_label(ShortcutAction::ControllerBrush),
                    self.settings
                        .shortcut_label(ShortcutAction::ControllerLine),
                    self.settings
                        .shortcut_label(ShortcutAction::ControllerSmooth),
                    self.settings
                        .shortcut_label(ShortcutAction::ControllerErase)
                ));
                ui.label(format!(
                    "• 视图滑动：{} / {} 水平滑动，{} / {} 垂直滑动；效果器链支持滚轮 / Shift+滚轮左右滑动",
                    self.settings.shortcut_label(ShortcutAction::ScrollLeft),
                    self.settings.shortcut_label(ShortcutAction::ScrollRight),
                    self.settings.shortcut_label(ShortcutAction::ScrollUp),
                    self.settings.shortcut_label(ShortcutAction::ScrollDown)
                ));
                ui.label(format!(
                    "• 右键音符 / {} = 删除选中项（片段/音符/效果器/控制器/节点）；顶部「量化」按网格对齐",
                    self.settings.shortcut_label(ShortcutAction::Delete)
                ));
                ui.add_space(6.0);
                ui.label(
                    RichText::new("快捷键")
                        .size(12.0)
                        .color(theme::TEXT_DIM)
                        .strong(),
                );
                egui::ScrollArea::vertical()
                    .id_salt("help_shortcut_list")
                    .max_height(240.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for action in ShortcutAction::ALL {
                            ui.label(format!(
                                "{}：{}",
                                action.label(),
                                self.settings.shortcut_label(action)
                            ));
                        }
                    });
                ui.label("Ctrl+滚轮 纵向缩放 · Ctrl+Shift+滚轮 横向缩放（编曲区 / 钢琴卷帘）");
                ui.label("拖动标尺上的「起 / 止」小旗可修改循环范围");
                ui.label("Z–M / Q–U 键 = 试听当前轨道（C3–C5）");
                ui.label("所有快捷键都可以在 设置 → 快捷键 中修改");
                ui.add_space(6.0);
                ui.label(
                    RichText::new("插件")
                        .size(12.0)
                        .color(theme::TEXT_DIM)
                        .strong(),
                );
                ui.label("• VST3：自动扫描标准目录，支持原生编辑器与参数");
                ui.label("• VST2：选择 64 位 .dll，支持音频与 MIDI 处理及通用参数面板");
                ui.label("• 插件状态会随工程一起保存（VST3 状态块 / VST2 参数快照）");
                ui.add_space(6.0);
                ui.label(
                    RichText::new("内置乐器")
                        .size(12.0)
                        .color(theme::TEXT_DIM)
                        .strong(),
                );
                ui.label("• 内置复音合成器（锯齿/方波/三角/正弦 + 滤波 + ADSR）");
                ui.label("• 内置鼓机（底鼓/军鼓/闭镲/开镲/拍手/通鼓）");
            });
        if !open {
            self.show_help = false;
        }
    }
}
