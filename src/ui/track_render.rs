//! 所属模块：ui / 轨道渲染弹窗
//! 文件用途：
//! - 绘制“渲染为音频”的确认提醒与进度弹窗。
//! - 只负责显示与收集意图，渲染任务由 `app/render.rs` 执行。
//!
//! 函数清单：
//! - `SurfingApp::ui_track_render_modal`：按渲染阶段绘制确认或进度弹窗。
//!
//! Module: ui / track render modal
//! Purpose:
//! - Paint the "render as audio" confirmation reminder and the progress modal.
//! - Only present state and collect intents; `app/render.rs` runs the render jobs.
//!
//! Functions:
//! - `SurfingApp::ui_track_render_modal`: paint the confirmation or progress modal per phase.

use crate::app::{SurfingApp, TrackRenderPhase};
use crate::ui::theme;
use eframe::egui::{self, RichText};

impl SurfingApp {
    /// 所属模块：ui / 轨道渲染弹窗
    /// 功能：按当前渲染阶段绘制确认提醒或进度弹窗。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：可能开始或取消渲染；不直接修改工程内容。
    /// 复杂度：时间 O(轨道数)，空间 O(轨道数)。
    ///
    /// Module: ui / track render modal
    /// Purpose: Paint the confirmation reminder or the progress modal for the current phase.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: may start or cancel rendering; project content is not edited here.
    /// Complexity: O(tracks) time and space.
    pub fn ui_track_render_modal(&mut self, ctx: &egui::Context) {
        match self.track_render.phase {
            TrackRenderPhase::Idle => {}
            TrackRenderPhase::Confirm => {
                let names: Vec<String> = self
                    .track_render
                    .requests
                    .iter()
                    .map(|request| {
                        self.project
                            .tracks
                            .iter()
                            .find(|track| track.id == request.target)
                            .map(|track| track.name.clone())
                            .unwrap_or_else(|| "已删除的轨道".into())
                    })
                    .collect();
                let mut start = false;
                let mut cancel = false;
                egui::Modal::new(egui::Id::new("track_render_confirm")).show(ctx, |ui| {
                    ui.set_min_width(380.0);
                    ui.label(
                        RichText::new("渲染为音频")
                            .size(15.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!("将离线渲染 {} 条轨道：", names.len()))
                            .size(12.5)
                            .color(theme::TEXT_DIM),
                    );
                    for name in &names {
                        ui.label(RichText::new(format!("· {name}")).size(12.0));
                    }
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "渲染期间播放会被禁用；完成后会在原轨道下方创建音频轨道，并把原轨道静音。",
                        )
                        .size(11.5)
                        .color(theme::TEXT_DIM),
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("开始渲染").clicked() {
                            start = true;
                        }
                        if ui.button("取消").clicked() {
                            cancel = true;
                        }
                    });
                });
                if start {
                    self.confirm_track_render();
                } else if cancel {
                    self.cancel_track_render();
                }
            }
            TrackRenderPhase::Rendering => {
                let mut cancel = false;
                let caption = self.track_render.caption();
                let status = self.track_render.status.clone();
                let progress = self.track_render.progress;
                egui::Modal::new(egui::Id::new("track_render_progress")).show(ctx, |ui| {
                    ui.set_min_width(380.0);
                    ui.label(
                        RichText::new(caption)
                            .size(15.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.add_space(6.0);
                    ui.label(RichText::new(status).size(12.0).color(theme::TEXT_DIM));
                    ui.add_space(6.0);
                    ui.add(
                        egui::ProgressBar::new(progress.clamp(0.0, 1.0))
                            .desired_height(8.0)
                            .show_percentage(),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("渲染期间播放已禁用，请稍候…")
                            .size(11.0)
                            .color(theme::TEXT_FAINT),
                    );
                    ui.add_space(8.0);
                    if ui.button("取消渲染").clicked() {
                        cancel = true;
                    }
                });
                if cancel {
                    self.cancel_track_render();
                }
            }
        }
    }
}
