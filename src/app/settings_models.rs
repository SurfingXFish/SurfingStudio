//! 所属模块：app / 设置窗口 / 模型管理
//! 文件用途：
//! - 绘制“设置 → 模型”页：用户添加的模型列表、启用开关、编辑与删除，以及空状态。
//! - 绘制“添加模型”两阶段弹窗：服务商网格 → 通过服务商添加/自定义模型表单。
//! - 在后台线程执行连通性测试，成功后写入模型列表并同步设置。
//!
//! 函数清单：
//! - `SurfingApp::ui_settings_models`：模型管理页入口。
//! - `SurfingApp::ui_model_modal`：添加/编辑/删除弹窗。
//! - `SurfingApp::poll_model_probe`：轮询后台连通性测试。
//! - `SurfingApp::start_model_probe` / `apply_model_draft`：启动测试与落库。
//! - `model_row` / `provider_card` / `ui_model_form`：页面与表单绘制。
//! - `model_icon_button` / `paint_provider_mark`：细线图标与模型标识。
//!
//! Module: app / settings / model management
//! Purpose:
//! - Draw the “Settings → Models” page: user-added models, toggles, edit/delete and the empty state.
//! - Draw the two-stage add dialog: provider grid → provider/custom form.
//! - Run the connectivity probe on a worker and commit the model after it succeeds.
//!
//! Functions:
//! - `SurfingApp::ui_settings_models`: entry of the model page.
//! - `SurfingApp::ui_model_modal`: add/edit/delete dialogs.
//! - `SurfingApp::poll_model_probe`: poll the background connectivity test.
//! - `SurfingApp::start_model_probe` / `apply_model_draft`: start the probe and commit the draft.
//! - `model_row` / `provider_card` / `ui_model_form`: page and form painting.
//! - `model_icon_button` / `paint_provider_mark`: thin icons and the model mark.

use super::i18n::t;
use super::settings::{settings_combo, settings_hint, settings_section, settings_switch};
use super::*;
use crate::ai::{
    AiMessage, AiModelConfig, AiReasoning, AiRole, AiRuntime, CUSTOM_PROVIDER_ID,
    DEFAULT_MODEL_CONTEXT_INPUT, DEFAULT_MODEL_CONTEXT_OUTPUT, DEFAULT_MODEL_TEMPERATURE,
    DEFAULT_MODEL_TOOL_ROUNDS, MAX_MODEL_OUTPUT_TOKENS, PROVIDERS, model_provider,
};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// 模型列表行高（逻辑点）。
/// Model-list row height in logical points.
const MODELS_ROW_H: f32 = 56.0;
/// 模型列表图标尺寸（逻辑点）。
/// Model-list mark size in logical points.
const MODELS_ICON: f32 = 34.0;
/// 操作列宽（逻辑点）；含当前标记、启用开关与两个图标按钮。
/// Action column width in logical points; holds the active chip, toggle and two icon buttons.
const MODELS_OPS_W: f32 = 112.0;
/// 添加/编辑弹窗宽度（逻辑点）。
/// Add/edit dialog width in logical points.
const MODELS_MODAL_W: f32 = 680.0;
/// 服务商选择弹窗宽度（逻辑点）。
/// Provider-picker dialog width in logical points.
const MODELS_GRID_W: f32 = 600.0;
/// 服务商网格卡片高度（逻辑点）。
/// Provider-card height in logical points.
const MODELS_CARD_H: f32 = 52.0;
/// 表单滚动区最大高度（逻辑点）；保证 620 点高的默认设置窗口上下仍留出边距。
/// Maximum height of the form scroll area in logical points; keeps margins above and below in the
/// default 620-point settings window.
const MODELS_FORM_H: f32 = 330.0;
/// 表单左侧字段标签列宽（逻辑点）。
/// Width of the form's left label column in logical points.
const MODELS_FORM_LABEL_W: f32 = 92.0;
/// 左侧标签与控件之间的间距（逻辑点）。
/// Gap between the left label column and the controls in logical points.
const MODELS_FORM_GAP: f32 = 20.0;
/// 表单行之间的垂直间距（逻辑点）。
/// Vertical gap between form rows in logical points.
const MODELS_FORM_ROW_GAP: f32 = 10.0;
/// 连通性测试超时（秒）；超过后 UI 主动放弃等待。
/// Connectivity-test timeout in seconds; the UI stops waiting afterwards.
const MODELS_PROBE_TIMEOUT: u64 = 40;
/// 连通性测试期间的重绘间隔（毫秒）。
/// Repaint interval in milliseconds while the connectivity test runs.
const MODELS_PROBE_REPAINT_MS: u64 = 100;

/// 模型页的界面状态；只存在于当前窗口，不持久化。
/// UI state of the model page; window-local and never persisted.
pub(super) struct ModelsUi {
    /// 当前弹窗；`None` 表示未打开。
    /// Current dialog; `None` means closed.
    pub modal: Option<ModelModal>,
    /// 表单草稿。
    /// Form draft.
    pub draft: ModelDraft,
    /// 正在编辑的模型 id；`None` 表示新增。
    /// Id of the model being edited; `None` means adding.
    pub editing_id: Option<String>,
    /// 后台连通性测试。
    /// Background connectivity test.
    pub probe: Option<ModelProbe>,
    /// 正在测试、等待落库的模型；测试成功后写入它，避免用户在等待期间修改表单造成未测试配置被保存。
    /// Model waiting for the probe result; committing it avoids saving untested edits made while the
    /// test runs.
    pub pending_model: Option<AiModelConfig>,
    /// 表单错误或测试失败原因。
    /// Form error or probe failure reason.
    pub error: Option<String>,
    /// 待确认删除的模型 id。
    /// Model id awaiting delete confirmation.
    pub confirm_delete: Option<String>,
    /// API 密钥是否以明文显示。
    /// Whether the API key is shown in plain text.
    pub show_key: bool,
}

impl Default for ModelsUi {
    fn default() -> Self {
        Self {
            modal: None,
            draft: ModelDraft::default(),
            editing_id: None,
            probe: None,
            pending_model: None,
            error: None,
            confirm_delete: None,
            show_key: false,
        }
    }
}

/// 弹窗阶段。
/// Dialog stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ModelModal {
    /// 服务商网格。
    /// Provider grid.
    ProviderGrid,
    /// 通过服务商添加 / 自定义模型表单。
    /// Provider or custom model form.
    Form,
}

/// 后台连通性测试。
/// Background connectivity test.
pub(super) struct ModelProbe {
    rx: Receiver<Result<(), String>>,
    started: Instant,
}

/// 模型表单草稿；采样参数用文本保存，允许留空表示使用服务商最佳配置。
/// Model form draft; sampling values are text so an empty field can mean “use the provider default”.
#[derive(Clone, Debug)]
pub(super) struct ModelDraft {
    pub provider_id: String,
    pub model: String,
    pub model_custom: bool,
    pub display_name: String,
    pub api_key: String,
    pub base_url: String,
    pub context_input: u32,
    pub context_output: u32,
    pub tool_rounds: u32,
    /// 推理强度不提供界面，仅原样保留已有条目的值。
    /// Reasoning has no UI and only carries over the existing entry's value.
    pub reasoning: String,
    pub temperature: String,
    pub top_p: String,
    pub top_k: String,
    pub enabled: bool,
}

impl Default for ModelDraft {
    fn default() -> Self {
        Self::for_provider(CUSTOM_PROVIDER_ID)
    }
}

impl ModelDraft {
    /// 所属模块：app / 设置 / 模型
    /// 功能：按服务商创建空草稿，自动填入默认地址与第一个常用模型。
    /// 输入：`provider_id`：服务商 id。
    /// 输出/副作用：草稿；不读盘。
    /// 复杂度：时间 O(模型数)，空间 O(1)。
    ///
    /// Module: app / settings / models
    /// Purpose: Build an empty draft for a provider with the default URL and first common model.
    /// Inputs: `provider_id`: provider id.
    /// Outputs / side effects: the draft; no disk access.
    /// Complexity: O(models) time and O(1) space.
    pub fn for_provider(provider_id: &str) -> Self {
        let spec = model_provider(provider_id);
        let model = spec.models.first().map(|model| model.id).unwrap_or("");
        Self {
            provider_id: spec.id.to_string(),
            model: model.to_string(),
            model_custom: model.is_empty(),
            display_name: String::new(),
            api_key: String::new(),
            base_url: if spec.id == CUSTOM_PROVIDER_ID {
                String::new()
            } else {
                spec.base_url.to_string()
            },
            context_input: DEFAULT_MODEL_CONTEXT_INPUT,
            context_output: DEFAULT_MODEL_CONTEXT_OUTPUT,
            tool_rounds: DEFAULT_MODEL_TOOL_ROUNDS,
            reasoning: AiReasoning::Medium.id().to_string(),
            temperature: format!("{DEFAULT_MODEL_TEMPERATURE:.2}"),
            top_p: String::new(),
            top_k: String::new(),
            enabled: true,
        }
    }

    /// 所属模块：app / 设置 / 模型
    /// 功能：从已有模型条目创建编辑草稿。
    /// 输入：`model`：模型条目。
    /// 输出/副作用：草稿；不读盘。
    /// 复杂度：时间 O(模型数)，空间 O(文本长度)。
    ///
    /// Module: app / settings / models
    /// Purpose: Build an edit draft from an existing model entry.
    /// Inputs: `model`: model entry.
    /// Outputs / side effects: the draft; no disk access.
    /// Complexity: O(models) time and O(text length) space.
    pub fn from_model(model: &AiModelConfig) -> Self {
        let spec = model_provider(&model.provider_id);
        Self {
            provider_id: model.provider_id.clone(),
            model: model.model.clone(),
            model_custom: !spec.models.iter().any(|entry| entry.id == model.model),
            display_name: model.display_name.clone(),
            api_key: model.api_key.clone(),
            base_url: model.resolved_base_url(),
            context_input: model.context_input,
            context_output: model.context_output,
            tool_rounds: model.tool_rounds,
            reasoning: model.reasoning.clone(),
            temperature: format!("{:.2}", model.temperature),
            top_p: model
                .top_p
                .map(|value| format!("{value:.2}"))
                .unwrap_or_default(),
            top_k: model
                .top_k
                .map(|value| value.to_string())
                .unwrap_or_default(),
            enabled: model.enabled,
        }
    }

    /// 所属模块：app / 设置 / 模型
    /// 功能：校验草稿并返回中文错误；用于连通性测试前的本地检查。
    /// 输入：`models`：现有模型列表；`editing_id`：正在编辑的 id。
    /// 输出/副作用：`Ok(())` 或错误文本；不修改状态。
    /// 复杂度：时间 O(模型数)，空间 O(1)。
    ///
    /// Module: app / settings / models
    /// Purpose: Validate the draft and return a Chinese error; used before the connectivity test.
    /// Inputs: `models`: existing models; `editing_id`: id being edited.
    /// Outputs / side effects: `Ok(())` or error text; no state change.
    /// Complexity: O(models) time and O(1) space.
    pub fn validate(
        &self,
        models: &[AiModelConfig],
        editing_id: Option<&str>,
    ) -> Result<(), String> {
        if self.model.trim().is_empty() {
            return Err(t("请选择或输入模型 ID").to_string());
        }
        if self.base_url.trim().is_empty() {
            return Err(t("请填写服务商接口地址").to_string());
        }
        if !self.base_url.trim().starts_with("http://")
            && !self.base_url.trim().starts_with("https://")
        {
            return Err(t("接口地址必须以 http:// 或 https:// 开头").to_string());
        }
        if self.api_key.trim().is_empty() {
            return Err(t("请填写 API 密钥").to_string());
        }
        if self.display_name.chars().count() > 64 {
            return Err(t("模型展示名称不能超过 64 个字符").to_string());
        }
        if let Some(value) = parse_optional_f32(&self.temperature)? {
            if !(0.0..=2.0).contains(&value) {
                return Err(t("Temperature 必须在 0 ~ 2 之间").to_string());
            }
        }
        if let Some(value) = parse_optional_f32(&self.top_p)? {
            if !(0.0..=1.0).contains(&value) {
                return Err(t("Top P 必须在 0 ~ 1 之间").to_string());
            }
        }
        if let Some(value) = parse_optional_u32(&self.top_k)? {
            if !(1..=100).contains(&value) {
                return Err(t("Top K 必须在 1 ~ 100 之间").to_string());
            }
        }
        let duplicate = models.iter().any(|existing| {
            existing.provider_id == self.provider_id
                && existing.model == self.model.trim()
                && Some(existing.id.as_str()) != editing_id
        });
        if duplicate {
            return Err(t("该服务商下已存在相同模型").to_string());
        }
        Ok(())
    }

    /// 所属模块：app / 设置 / 模型
    /// 功能：把草稿转换为持久化模型条目。
    /// 输入：`id` 条目 id。
    /// 输出/副作用：模型条目；不读盘。
    /// 复杂度：时间 O(文本长度)，空间 O(文本长度)。
    ///
    /// Module: app / settings / models
    /// Purpose: Convert the draft into a persistable model entry.
    /// Inputs: `id` entry id.
    /// Outputs / side effects: the model entry; no disk access.
    /// Complexity: O(text length) time and space.
    pub fn to_model(&self, id: &str) -> AiModelConfig {
        let temperature = parse_optional_f32(&self.temperature)
            .ok()
            .flatten()
            .unwrap_or(DEFAULT_MODEL_TEMPERATURE);
        let top_p = parse_optional_f32(&self.top_p).ok().flatten();
        let top_k = parse_optional_u32(&self.top_k).ok().flatten();
        let spec = model_provider(&self.provider_id);
        let base = self.base_url.trim();
        // 与目录默认地址一致时留空，后续服务商调整地址可以自动跟随。
        // Keep it empty when it matches the catalogue default so later catalogue updates apply.
        let base_url = if base == spec.base_url {
            String::new()
        } else {
            base.to_string()
        };
        let mut model = AiModelConfig {
            id: id.to_string(),
            provider_id: self.provider_id.clone(),
            display_name: self.display_name.trim().to_string(),
            model: self.model.trim().to_string(),
            base_url,
            api_key: self.api_key.trim().to_string(),
            context_input: self.context_input,
            context_output: self.context_output,
            tool_rounds: self.tool_rounds,
            reasoning: self.reasoning.clone(),
            temperature,
            top_p,
            top_k,
            enabled: self.enabled,
        };
        model.sanitize();
        model
    }

    /// 所属模块：app / 设置 / 模型
    /// 功能：切换服务商并重置模型、接口地址与自定义标记。
    /// 输入：`provider_id`：目标服务商 id。
    /// 输出/副作用：原地修改草稿；不读盘。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / settings / models
    /// Purpose: Switch the provider and reset the model, base URL and manual flag.
    /// Inputs: `provider_id`: target provider id.
    /// Outputs / side effects: mutates the draft; no disk access.
    /// Complexity: O(1) time and space.
    pub fn switch_provider(&mut self, provider_id: &str) {
        let spec = model_provider(provider_id);
        self.provider_id = spec.id.to_string();
        self.model = spec
            .models
            .first()
            .map(|model| model.id)
            .unwrap_or("")
            .to_string();
        self.model_custom = self.model.is_empty();
        // 不同服务商的密钥通常不通用，切换时清空，避免把旧密钥发给新服务商。
        // Keys are usually provider-specific, so clear the key when switching providers.
        self.api_key.clear();
        self.base_url = if spec.id == CUSTOM_PROVIDER_ID {
            String::new()
        } else {
            spec.base_url.to_string()
        };
    }
}

/// 所属模块：app / 设置 / 模型
/// 功能：解析可留空的浮点数；空文本返回 `None`，非法文本返回中文错误。
/// 输入：`text`：用户输入。
/// 输出/副作用：数值或错误；不修改输入。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: app / settings / models
/// Purpose: Parse an optional float; empty text returns `None`, invalid text returns a Chinese error.
/// Inputs: `text`: user input.
/// Outputs / side effects: value or error; the input is unchanged.
/// Complexity: O(text length) time and O(1) space.
fn parse_optional_f32(text: &str) -> Result<Option<f32>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<f32>()
        .map(Some)
        .map_err(|_| t("请输入有效的数字").to_string())
}

/// 所属模块：app / 设置 / 模型
/// 功能：解析可留空的非负整数；空文本返回 `None`，非法文本返回中文错误。
/// 输入：`text`：用户输入。
/// 输出/副作用：数值或错误；不修改输入。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: app / settings / models
/// Purpose: Parse an optional non-negative integer; empty text returns `None`, invalid text returns
/// a Chinese error.
/// Inputs: `text`: user input.
/// Outputs / side effects: value or error; the input is unchanged.
/// Complexity: O(text length) time and O(1) space.
fn parse_optional_u32(text: &str) -> Result<Option<u32>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<u32>()
        .map(Some)
        .map_err(|_| t("请输入有效的整数").to_string())
}

impl SurfingApp {
    /// 所属模块：app / 设置 / 模型
    /// 功能：绘制模型管理页：标题与添加按钮、用户模型列表（或空状态）与联网检索开关。
    /// 输入：`ui`：设置内容区。
    /// 输出/副作用：增删改模型、启停、写盘；打开添加/编辑弹窗。
    /// 复杂度：每帧 O(模型数)。
    ///
    /// Module: app / settings / models
    /// Purpose: Draw the model page: title and add button, the user model list (or empty state) and
    /// the web-search switch.
    /// Inputs: `ui`: settings content area.
    /// Outputs / side effects: adds/edits/deletes models, toggles them and saves; opens the dialogs.
    /// Complexity: O(models) per frame.
    pub(super) fn ui_settings_models(&mut self, ui: &mut egui::Ui) {
        self.poll_model_probe(ui.ctx());
        let mut add = false;
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(t("模型"))
                    .size(theme::FONT_PAGE)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let button = egui::Button::new(
                    egui::RichText::new(format!("+ {}", t("添加模型")))
                        .size(theme::FONT_LABEL)
                        .color(egui::Color32::WHITE),
                )
                .fill(theme::ACCENT)
                .stroke(egui::Stroke::NONE)
                .min_size(egui::vec2(0.0, 28.0));
                if ui
                    .add(button)
                    .on_hover_text(t("配置服务商与 API Key，添加新的可用模型"))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    add = true;
                }
            });
        });
        ui.add_space(theme::SPACE_XS);
        settings_hint(
            ui,
            t("对话与编曲使用这里选择的模型；API 密钥只保存在本机配置文件中。"),
        );
        ui.add_space(theme::SPACE_LG);

        let models = self.settings.ai.models.clone();
        let active_id = self.settings.ai.active_model.clone();

        if models.is_empty() {
            egui::Frame::NONE
                .fill(theme::PANEL)
                .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                .corner_radius(theme::RADIUS_CARD)
                .inner_margin(egui::Margin::symmetric(16, 30))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
                        ui.painter().rect_filled(
                            rect,
                            egui::CornerRadius::from(11),
                            theme::PANEL_2,
                        );
                        ui.painter().rect_stroke(
                            rect,
                            egui::CornerRadius::from(11),
                            egui::Stroke::new(1.0_f32, theme::LINE),
                            egui::StrokeKind::Inside,
                        );
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "+",
                            egui::FontId::proportional(22.0),
                            theme::TEXT_DIM,
                        );
                        ui.add_space(theme::SPACE_MD);
                        ui.label(
                            egui::RichText::new(t("尚未添加模型"))
                                .size(theme::FONT_SECTION)
                                .strong()
                                .color(theme::TEXT),
                        );
                        ui.add_space(theme::SPACE_XS);
                        ui.label(
                            egui::RichText::new(t(
                                "从服务商目录选择模型并填写 API 密钥，即可开始使用。",
                            ))
                            .size(theme::FONT_LABEL)
                            .color(theme::TEXT_DIM),
                        );
                        ui.add_space(theme::SPACE_MD);
                        let button = egui::Button::new(
                            egui::RichText::new(t("添加第一个模型"))
                                .size(theme::FONT_LABEL)
                                .color(egui::Color32::WHITE),
                        )
                        .fill(theme::ACCENT)
                        .stroke(egui::Stroke::NONE)
                        .min_size(egui::vec2(0.0, 28.0));
                        if ui.add(button).clicked() {
                            add = true;
                        }
                    });
                });
        } else {
            egui::Frame::NONE
                .fill(theme::PANEL)
                .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                .corner_radius(theme::RADIUS_CARD)
                .inner_margin(egui::Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let mut actions: Vec<(String, ModelRowAction)> = Vec::new();
                    for (index, model) in models.iter().enumerate() {
                        if index > 0 {
                            let y = ui.cursor().top();
                            ui.painter().hline(
                                ui.max_rect().x_range(),
                                y,
                                egui::Stroke::new(1.0_f32, theme::LINE),
                            );
                        }
                        if let Some(action) = model_row(ui, model, model.id == active_id) {
                            actions.push((model.id.clone(), action));
                        }
                    }
                    for (id, action) in actions {
                        match action {
                            ModelRowAction::Edit => self.open_edit_model(&id),
                            ModelRowAction::Delete => {
                                self.settings_models.confirm_delete = Some(id);
                            }
                            ModelRowAction::Toggle(enabled) => {
                                self.settings.ai.set_model_enabled(&id, enabled);
                                changed = true;
                            }
                        }
                    }
                });
        }

        ui.add_space(theme::SPACE_LG);
        settings_section(ui, t("联网搜索"), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(t("插件检索"))
                        .size(theme::FONT_LABEL)
                        .color(theme::TEXT),
                );
                let mut enabled = self.settings.ai.search_enabled;
                if settings_switch(ui, &mut enabled) {
                    self.settings.ai.search_enabled = enabled;
                    changed = true;
                }
                ui.label(
                    egui::RichText::new(t("允许 AI 在不确定插件特点时联网检索"))
                        .size(theme::FONT_MICRO)
                        .color(theme::TEXT_DIM),
                );
            });
            settings_hint(
                ui,
                t("使用内置的 Bing 网页检索，无需额外 API Key，也不会把密钥发送给搜索引擎。"),
            );
            settings_hint(
                ui,
                t("检索词只包含插件名称/厂商，不会发送工程内容或本机路径。"),
            );
        });

        if add {
            self.settings_models.modal = Some(ModelModal::ProviderGrid);
            self.settings_models.draft = ModelDraft::default();
            self.settings_models.editing_id = None;
            self.settings_models.error = None;
            self.settings_models.probe = None;
            self.settings_models.pending_model = None;
        }
        if changed {
            self.commit_settings();
        }
        self.ui_model_modal(ui.ctx());
    }

    /// 所属模块：app / 设置 / 模型
    /// 功能：打开编辑弹窗并填入已有配置。
    /// 输入：`id`：模型条目 id。
    /// 输出/副作用：设置弹窗阶段、草稿与编辑 id；不写盘。
    /// 复杂度：时间 O(模型数)，空间 O(文本长度)。
    ///
    /// Module: app / settings / models
    /// Purpose: Open the edit dialog prefilled with an existing model.
    /// Inputs: `id`: model-entry id.
    /// Outputs / side effects: sets the dialog stage, draft and editing id; no disk write.
    /// Complexity: O(models) time and O(text length) space.
    fn open_edit_model(&mut self, id: &str) {
        let Some(model) = self.settings.ai.model_by_id(id).cloned() else {
            return;
        };
        self.settings_models.draft = ModelDraft::from_model(&model);
        self.settings_models.editing_id = Some(id.to_string());
        self.settings_models.modal = Some(ModelModal::Form);
        self.settings_models.error = None;
        self.settings_models.pending_model = None;
        self.settings_models.show_key = false;
    }

    /// 所属模块：app / 设置 / 模型
    /// 功能：轮询后台连通性测试；成功时落库并关闭弹窗，失败时保留表单并显示原因。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：修改弹窗、错误与模型列表；成功时写盘。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / settings / models
    /// Purpose: Poll the background connectivity test; success commits and closes the dialog, failure
    /// keeps the form and shows the reason.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: updates the dialog, error and model list; writes settings on success.
    /// Complexity: O(1) time and space.
    fn poll_model_probe(&mut self, ctx: &egui::Context) {
        let Some(probe) = &self.settings_models.probe else {
            return;
        };
        let result = match probe.rx.try_recv() {
            Ok(result) => Some(result),
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                if probe.started.elapsed() >= Duration::from_secs(MODELS_PROBE_TIMEOUT) {
                    Some(Err(t("连通性测试超时，请检查网络与接口地址").to_string()))
                } else {
                    ctx.request_repaint_after(Duration::from_millis(MODELS_PROBE_REPAINT_MS));
                    None
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Some(Err(t("连通性测试异常结束，请重试").to_string()))
            }
        };
        let Some(result) = result else {
            return;
        };
        self.settings_models.probe = None;
        match result {
            Ok(()) => {
                self.apply_model_draft();
                self.commit_settings();
            }
            Err(message) => {
                self.settings_models.pending_model = None;
                self.settings_models.error = Some(message);
            }
        }
    }

    /// 所属模块：app / 设置 / 模型
    /// 功能：在后台线程发起一次最小对话请求，验证地址、密钥与模型是否可用。
    /// 输入：`model`：待测试模型条目。
    /// 输出/副作用：写入 `settings_models.probe`；网络请求在后台线程执行。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / settings / models
    /// Purpose: Start a minimal chat request on a worker to verify the URL, key and model.
    /// Inputs: `model`: the entry to test.
    /// Outputs / side effects: fills `settings_models.probe`; the network call runs on a worker.
    /// Complexity: O(1) time and space.
    fn start_model_probe(&mut self, model: AiModelConfig) {
        if self.settings_models.probe.is_some() {
            return;
        }
        self.settings_models.pending_model = Some(model.clone());
        // 连通性测试关闭思考，避免一次探活消耗大量推理 token。
        // Disable thinking for the connectivity test so a probe does not burn reasoning tokens.
        let mut probe_model = model;
        probe_model.reasoning = AiReasoning::Off.id().to_string();
        let request = probe_model.build_request(
            vec![AiMessage {
                role: AiRole::User,
                content: "Reply with OK only.".into(),
            }],
            Duration::from_secs(20),
            32,
            false,
        );
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("ai-model-probe".into())
            .spawn(move || {
                let result = AiRuntime::standard().complete(&request).map(|_| ());
                let _ = tx.send(result);
            });
        if spawned.is_err() {
            self.settings_models.error = Some(t("无法启动连通性测试线程").to_string());
            return;
        }
        self.settings_models.probe = Some(ModelProbe {
            rx,
            started: Instant::now(),
        });
        self.settings_models.error = None;
    }

    /// 所属模块：app / 设置 / 模型
    /// 功能：把当前草稿写入模型列表；新增时生成唯一 id，编辑时保留原 id。
    /// 输入：`self` 可变引用。
    /// 输出/副作用：修改 `settings.ai.models` 与 `active_model`，关闭弹窗；不写盘。
    /// 复杂度：时间 O(模型数)，空间 O(文本长度)。
    ///
    /// Module: app / settings / models
    /// Purpose: Write the current draft into the model list; new entries get a unique id while edits
    /// keep the original id.
    /// Inputs: `self`: mutable reference.
    /// Outputs / side effects: mutates `settings.ai.models` and `active_model`, closes the dialog; no
    /// disk write.
    /// Complexity: O(models) time and O(text length) space.
    fn apply_model_draft(&mut self) {
        let Some(model) = self.settings_models.pending_model.take() else {
            return;
        };
        self.settings.ai.upsert_model(model);
        self.settings_models.modal = None;
        self.settings_models.editing_id = None;
        self.settings_models.error = None;
        self.settings_models.show_key = false;
    }

    /// 所属模块：app / 设置 / 模型
    /// 功能：绘制添加/编辑/删除弹窗。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：修改弹窗阶段与草稿；提交时启动连通性测试或删除模型。
    /// 复杂度：每帧 O(服务商数 + 模型数)。
    ///
    /// Module: app / settings / models
    /// Purpose: Draw the add/edit/delete dialogs.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: updates the dialog stage and draft; submission starts the probe or
    /// deletes a model.
    /// Complexity: O(providers + models) per frame.
    fn ui_model_modal(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.settings_models.confirm_delete.clone() {
            let mut confirm = false;
            let mut cancel = false;
            let response =
                egui::Modal::new(egui::Id::new("settings_model_delete")).show(ctx, |ui| {
                    ui.set_width(360.0);
                    ui.label(
                        egui::RichText::new(t("删除模型"))
                            .size(theme::FONT_SECTION)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.add_space(theme::SPACE_SM);
                    ui.label(
                        egui::RichText::new(t("删除后需要重新填写 API Key，确定继续吗？"))
                            .size(theme::FONT_BODY)
                            .color(theme::TEXT_DIM),
                    );
                    ui.add_space(theme::SPACE_LG);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let delete = egui::Button::new(
                                egui::RichText::new(t("删除"))
                                    .size(theme::FONT_LABEL)
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(theme::RED)
                            .min_size(egui::vec2(72.0, 28.0));
                            if ui.add(delete).clicked() {
                                confirm = true;
                            }
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(t("取消"))
                                            .size(theme::FONT_LABEL)
                                            .color(theme::TEXT),
                                    )
                                    .fill(theme::PANEL_2)
                                    .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
                                    .min_size(egui::vec2(72.0, 28.0)),
                                )
                                .clicked()
                            {
                                cancel = true;
                            }
                        });
                    });
                });
            if response.should_close() {
                cancel = true;
            }
            if confirm {
                self.settings.ai.remove_model(&id);
                self.settings_models.confirm_delete = None;
                self.commit_settings();
            } else if cancel {
                self.settings_models.confirm_delete = None;
            }
            return;
        }

        let Some(modal) = self.settings_models.modal else {
            return;
        };
        match modal {
            ModelModal::ProviderGrid => {
                let mut chosen: Option<String> = None;
                let mut close = false;
                let response =
                    egui::Modal::new(egui::Id::new("settings_model_picker")).show(ctx, |ui| {
                        ui.set_width(MODELS_GRID_W);
                        if model_modal_title(ui, t("添加模型"), false, &mut close) {
                            return;
                        }
                        ui.add_space(theme::SPACE_XS);
                        settings_hint(ui, t("选择服务商，下一步填写 API 密钥并测试连接。"));
                        ui.add_space(theme::SPACE_MD);
                        egui::ScrollArea::vertical()
                            .max_height(500.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                if provider_card(
                                    ui,
                                    ui.available_width(),
                                    t("自定义模型"),
                                    CUSTOM_PROVIDER_ID,
                                ) {
                                    chosen = Some(CUSTOM_PROVIDER_ID.to_string());
                                }
                                ui.add_space(theme::SPACE_SM);
                                let width = ui.available_width();
                                let card_w = ((width - theme::SPACE_MD) / 2.0).max(160.0);
                                for chunk in PROVIDERS.chunks(2) {
                                    ui.horizontal(|ui| {
                                        for spec in chunk {
                                            if provider_card(ui, card_w, spec.label, spec.id) {
                                                chosen = Some(spec.id.to_string());
                                            }
                                        }
                                    });
                                    ui.add_space(theme::SPACE_SM);
                                }
                            });
                    });
                if response.should_close() {
                    close = true;
                }
                if let Some(provider_id) = chosen {
                    self.settings_models.draft = ModelDraft::for_provider(&provider_id);
                    self.settings_models.editing_id = None;
                    self.settings_models.modal = Some(ModelModal::Form);
                    self.settings_models.error = None;
                } else if close {
                    self.settings_models.modal = None;
                    self.settings_models.probe = None;
                    self.settings_models.pending_model = None;
                }
            }
            ModelModal::Form => {
                let editing = self
                    .settings_models
                    .editing_id
                    .as_deref()
                    .and_then(|id| self.settings.ai.model_by_id(id))
                    .cloned();
                let mut draft = self.settings_models.draft.clone();
                let mut show_key = self.settings_models.show_key;
                let mut action = FormAction::None;
                let mut close = false;
                let mut go_back = false;
                let response =
                    egui::Modal::new(egui::Id::new("settings_model_form")).show(ctx, |ui| {
                        ui.set_width(MODELS_MODAL_W);
                        let title = if editing.is_some() {
                            t("编辑模型")
                        } else {
                            t("通过服务商添加")
                        };
                        if model_modal_title(ui, title, editing.is_none(), &mut close) {
                            go_back = true;
                            return;
                        }
                        ui.add_space(theme::SPACE_XS);
                        ui.separator();
                        ui.add_space(theme::SPACE_MD);
                        action = ui_model_form(
                            ui,
                            &mut draft,
                            editing.is_some(),
                            self.settings_models.probe.is_some(),
                            self.settings_models.error.as_deref(),
                            &mut show_key,
                        );
                    });
                if response.should_close() {
                    close = true;
                }
                if go_back {
                    action = FormAction::Back;
                }
                self.settings_models.draft = draft;
                self.settings_models.show_key = show_key;
                match action {
                    FormAction::Back => {
                        self.settings_models.modal = Some(ModelModal::ProviderGrid);
                        self.settings_models.editing_id = None;
                        self.settings_models.error = None;
                        self.settings_models.probe = None;
                        self.settings_models.pending_model = None;
                    }
                    FormAction::Reset => {
                        self.settings_models.draft = if let Some(model) = &editing {
                            ModelDraft::from_model(model)
                        } else {
                            ModelDraft::for_provider(&self.settings_models.draft.provider_id)
                        };
                        self.settings_models.error = None;
                        self.settings_models.probe = None;
                        self.settings_models.pending_model = None;
                    }
                    FormAction::Submit => {
                        let models = self.settings.ai.models.clone();
                        let editing_id = self.settings_models.editing_id.clone();
                        if let Err(message) = self
                            .settings_models
                            .draft
                            .validate(&models, editing_id.as_deref())
                        {
                            self.settings_models.error = Some(message);
                        } else {
                            let id = editing
                                .as_ref()
                                .map(|model| model.id.clone())
                                .unwrap_or_else(|| {
                                    self.settings.ai.unique_model_id(&format!(
                                        "{}-{}",
                                        self.settings_models.draft.provider_id,
                                        self.settings_models.draft.model
                                    ))
                                });
                            let model = self.settings_models.draft.to_model(&id);
                            self.start_model_probe(model);
                        }
                    }
                    FormAction::None => {}
                }
                if close {
                    self.settings_models.modal = None;
                    self.settings_models.editing_id = None;
                    self.settings_models.error = None;
                    self.settings_models.probe = None;
                    self.settings_models.pending_model = None;
                }
            }
        }
    }
}

/// 模型行操作。
/// Action produced by a model row.
enum ModelRowAction {
    Edit,
    Delete,
    Toggle(bool),
}

/// 表单操作。
/// Action produced by the model form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormAction {
    None,
    Back,
    Reset,
    Submit,
}

/// 所属模块：app / 设置 / 模型
/// 功能：绘制一条模型行：服务商标识、展示名、服务商与模型、当前标记、启用与编辑/删除。
/// 输入：`ui` 列表；`model` 模型；`active` 是否当前模型。
/// 输出/副作用：返回行操作；不直接修改状态。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: app / settings / models
/// Purpose: Draw one model row: provider mark, display name, provider/model, active chip, toggle and
/// edit/delete.
/// Inputs: `ui`; `model`; `active`.
/// Outputs / side effects: the row action; no direct state change.
/// Complexity: O(text length) time and O(1) space.
fn model_row(ui: &mut egui::Ui, model: &AiModelConfig, active: bool) -> Option<ModelRowAction> {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, MODELS_ROW_H), egui::Sense::hover());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::from(8), theme::PANEL_2);
    }
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 14.0 + MODELS_ICON / 2.0, rect.center().y),
        egui::vec2(MODELS_ICON, MODELS_ICON),
    );
    paint_model_icon(ui, icon_rect, model, active);

    let ops_left = rect.right() - MODELS_OPS_W;
    let chip_w = if active { 44.0 } else { 0.0 };
    let text_left = icon_rect.right() + 12.0;
    let text_right = (ops_left - 10.0 - chip_w).max(text_left + 40.0);
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(text_left, rect.top()),
        egui::pos2(text_right, rect.bottom()),
    );
    let painter = ui.painter();
    painter.with_clip_rect(text_rect).text(
        egui::pos2(text_left, rect.center().y - 8.0),
        egui::Align2::LEFT_CENTER,
        model.display_label(),
        egui::FontId::proportional(theme::FONT_BODY),
        theme::TEXT,
    );
    painter.with_clip_rect(text_rect).text(
        egui::pos2(text_left, rect.center().y + 9.0),
        egui::Align2::LEFT_CENTER,
        format!("{}  ·  {}", model.provider_label(), model.model),
        egui::FontId::proportional(theme::FONT_MICRO),
        theme::TEXT_FAINT,
    );
    if active {
        let chip_rect = egui::Rect::from_center_size(
            egui::pos2(ops_left - 6.0 - chip_w / 2.0, rect.center().y),
            egui::vec2(chip_w, 18.0),
        );
        painter.rect_filled(chip_rect, egui::CornerRadius::from(9), theme::ACCENT_DARK);
        painter.text(
            chip_rect.center(),
            egui::Align2::CENTER_CENTER,
            t("当前"),
            egui::FontId::proportional(theme::FONT_MICRO),
            theme::TEXT,
        );
    }

    let ops_rect = egui::Rect::from_min_max(
        egui::pos2(ops_left, rect.top()),
        egui::pos2(rect.right() - 6.0, rect.bottom()),
    );
    let mut action = None;
    ui.scope_builder(egui::UiBuilder::new().max_rect(ops_rect), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut enabled = model.enabled;
            if theme::toggle_switch(ui, &mut enabled)
                .on_hover_text(if enabled {
                    t("点击停用该模型")
                } else {
                    t("点击启用该模型")
                })
                .clicked()
            {
                action = Some(ModelRowAction::Toggle(enabled));
            }
            if model_icon_button(ui, ModelIcon::Delete, t("删除模型")) {
                action = Some(ModelRowAction::Delete);
            }
            if model_icon_button(ui, ModelIcon::Edit, t("编辑模型")) {
                action = Some(ModelRowAction::Edit);
            }
        });
    });
    response.on_hover_text(format!("{} · {}", model.display_label(), model.model));
    action
}

/// 模型标识用的细线图标。
/// Thin icons used by the model page.
enum ModelIcon {
    Edit,
    Delete,
    Back,
    Close,
    Eye(bool),
}

/// 所属模块：app / 设置 / 模型
/// 功能：绘制 24×24 的细线图标按钮，并处理悬停底色与提示。
/// 输入：`ui`；`icon` 图标类型；`tooltip` 悬停提示。
/// 输出/副作用：返回是否点击；不修改其他状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / settings / models
/// Purpose: Draw a 24×24 thin icon button with hover fill and tooltip.
/// Inputs: `ui`; `icon`; `tooltip`.
/// Outputs / side effects: whether it was clicked; no other state change.
/// Complexity: O(1) time and space.
fn model_icon_button(ui: &mut egui::Ui, icon: ModelIcon, tooltip: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::from(4), theme::PANEL_2);
    }
    let color = if response.hovered() {
        theme::TEXT
    } else {
        theme::TEXT_DIM
    };
    let stroke = egui::Stroke::new(1.2_f32, color);
    let painter = ui.painter();
    let c = rect.center();
    match icon {
        ModelIcon::Edit => {
            let start = egui::pos2(c.x - 5.0, c.y + 5.0);
            let end = egui::pos2(c.x + 4.0, c.y - 4.0);
            let direction = (end - start).normalized();
            let normal = egui::vec2(-direction.y, direction.x) * 2.4;
            let start_a = start + normal;
            let start_b = start - normal;
            let end_a = end + normal;
            let end_b = end - normal;
            let tip = end + direction * 3.0;
            painter.line_segment([start_a, end_a], stroke);
            painter.line_segment([start_b, end_b], stroke);
            painter.line_segment([start_a, start_b], stroke);
            painter.line_segment([end_a, tip], stroke);
            painter.line_segment([end_b, tip], stroke);
        }
        ModelIcon::Delete => {
            painter.line_segment(
                [
                    egui::pos2(c.x - 6.0, c.y - 4.0),
                    egui::pos2(c.x + 6.0, c.y - 4.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 2.0, c.y - 6.0),
                    egui::pos2(c.x + 2.0, c.y - 6.0),
                ],
                stroke,
            );
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 4.5, c.y - 4.0),
                    egui::pos2(c.x + 4.5, c.y + 6.0),
                ),
                egui::CornerRadius::from(1),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 1.5, c.y - 1.5),
                    egui::pos2(c.x - 1.5, c.y + 3.5),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 1.5, c.y - 1.5),
                    egui::pos2(c.x + 1.5, c.y + 3.5),
                ],
                stroke,
            );
        }
        ModelIcon::Back => {
            painter.line_segment(
                [egui::pos2(c.x + 3.0, c.y - 5.0), egui::pos2(c.x - 2.0, c.y)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x - 2.0, c.y), egui::pos2(c.x + 3.0, c.y + 5.0)],
                stroke,
            );
        }
        ModelIcon::Close => {
            painter.line_segment(
                [
                    egui::pos2(c.x - 4.0, c.y - 4.0),
                    egui::pos2(c.x + 4.0, c.y + 4.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 4.0, c.y - 4.0),
                    egui::pos2(c.x - 4.0, c.y + 4.0),
                ],
                stroke,
            );
        }
        ModelIcon::Eye(shown) => {
            painter.circle_stroke(c, 6.5, stroke);
            painter.circle_filled(c, 2.2, color);
            if !shown {
                painter.line_segment(
                    [
                        egui::pos2(c.x - 7.0, c.y + 7.0),
                        egui::pos2(c.x + 7.0, c.y - 7.0),
                    ],
                    stroke,
                );
            }
        }
    }
    response
        .on_hover_text(tooltip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// 所属模块：app / 设置 / 模型
/// 功能：绘制模型标识：圆角方块 + 服务商首字；当前模型使用强调色描边。
/// 输入：`ui`；`rect` 图标区域；`model` 模型；`active` 是否当前模型。
/// 输出/副作用：只绘制；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / settings / models
/// Purpose: Draw the model mark: rounded square with the provider initial; the active model uses the
/// accent outline.
/// Inputs: `ui`; `rect`; `model`; `active`.
/// Outputs / side effects: paint only; no state change.
/// Complexity: O(1) time and space.
fn paint_model_icon(ui: &egui::Ui, rect: egui::Rect, model: &AiModelConfig, active: bool) {
    paint_provider_mark(ui, rect, model.provider_label(), active);
}

/// 所属模块：app / 设置 / 模型
/// 功能：绘制服务商标识：圆角方块 + 名称首字；可切换强调色描边与更大的字号。
/// 输入：`ui`；`rect` 图标区域；`label` 服务商名称；`active` 是否强调。
/// 输出/副作用：只绘制；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / settings / models
/// Purpose: Draw a provider mark: rounded square with the name initial, with an optional accent
/// outline and a larger glyph.
/// Inputs: `ui`; `rect`; `label`; `active`.
/// Outputs / side effects: paint only; no state change.
/// Complexity: O(1) time and space.
fn paint_provider_mark(ui: &egui::Ui, rect: egui::Rect, label: &str, active: bool) {
    let painter = ui.painter();
    let radius = egui::CornerRadius::from(if rect.width() >= 40.0 { 9 } else { 7 });
    painter.rect_filled(
        rect,
        radius,
        if active {
            theme::ACCENT_DARK
        } else {
            theme::PANEL_2
        },
    );
    painter.rect_stroke(
        rect,
        radius,
        egui::Stroke::new(1.0_f32, if active { theme::ACCENT } else { theme::LINE }),
        egui::StrokeKind::Inside,
    );
    let initial = label.chars().next().unwrap_or('?').to_string();
    let size = if rect.width() >= 40.0 {
        theme::FONT_SECTION
    } else {
        theme::FONT_BODY
    };
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        egui::FontId::proportional(size),
        if active {
            egui::Color32::WHITE
        } else {
            theme::TEXT_DIM
        },
    );
}

/// 所属模块：app / 设置 / 模型
/// 功能：绘制一张服务商卡片（图标 + 名称 + 右箭头），用于添加模型的服务商网格。
/// 输入：`ui`；`width` 卡片宽度；`label` 名称；`id` 服务商 id。
/// 输出/副作用：返回是否点击；不修改状态。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: app / settings / models
/// Purpose: Draw one provider card (mark + name + right chevron) for the add-model grid.
/// Inputs: `ui`; `width`; `label`; `id`.
/// Outputs / side effects: whether it was clicked; no state change.
/// Complexity: O(text length) time and O(1) space.
fn provider_card(ui: &mut egui::Ui, width: f32, label: &str, id: &str) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, MODELS_CARD_H), egui::Sense::click());
    let hovered = response.hovered();
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::from(8),
        if hovered {
            theme::PANEL_3
        } else {
            theme::PANEL
        },
    );
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::from(8),
        egui::Stroke::new(1.0_f32, if hovered { theme::ACCENT } else { theme::LINE }),
        egui::StrokeKind::Inside,
    );
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 14.0 + 13.0, rect.center().y),
        egui::vec2(26.0, 26.0),
    );
    let spec = model_provider(id);
    paint_provider_mark(ui, icon_rect, spec.label, false);
    ui.painter().text(
        egui::pos2(icon_rect.right() + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(theme::FONT_BODY),
        theme::TEXT,
    );
    theme::paint_dropdown_angle(
        ui.painter(),
        egui::pos2(rect.right() - 14.0, rect.center().y),
        -std::f32::consts::FRAC_PI_2,
        theme::TEXT_FAINT,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// 所属模块：app / 设置 / 模型
/// 功能：绘制弹窗标题栏：可选返回箭头、标题与关闭按钮。
/// 输入：`ui`；`title` 标题；`back` 是否显示返回；`close` 关闭标记。
/// 输出/副作用：点击关闭时写 `close`；返回是否点击返回。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / settings / models
/// Purpose: Draw a dialog title bar with an optional back arrow, title and close button.
/// Inputs: `ui`; `title`; `back`; `close`.
/// Outputs / side effects: sets `close` when clicked; returns whether back was clicked.
/// Complexity: O(1) time and space.
fn model_modal_title(ui: &mut egui::Ui, title: &str, back: bool, close: &mut bool) -> bool {
    let mut go_back = false;
    ui.horizontal(|ui| {
        if back && model_icon_button(ui, ModelIcon::Back, t("返回服务商列表")) {
            go_back = true;
        }
        ui.label(
            egui::RichText::new(title)
                .size(theme::FONT_SECTION)
                .strong()
                .color(theme::TEXT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if model_icon_button(ui, ModelIcon::Close, t("关闭")) {
                *close = true;
            }
        });
    });
    go_back
}

/// 所属模块：app / 设置 / 模型
/// 功能：绘制模型表单：身份行、连接、凭据、上下文、采样与底部操作。
/// 输入：
/// - `ui`：弹窗内容；
/// - `draft`：表单草稿；
/// - `editing`：是否正在编辑已有模型；
/// - `probing`：是否正在测试；
/// - `error`：错误文本；
/// - `show_key`：是否明文显示密钥。
/// 输出/副作用：修改草稿与显示状态；返回表单操作。
/// 复杂度：时间 O(服务商模型数)。
///
/// Module: app / settings / models
/// Purpose: Draw the model form: identity row, connection, credentials, context, sampling and footer.
/// Inputs:
/// - `ui`: dialog content;
/// - `draft`: form draft;
/// - `editing`: whether an existing model is being edited;
/// - `probing`: whether a test is running;
/// - `error`: error text;
/// - `show_key`: whether the key is visible.
/// Outputs / side effects: mutates the draft and visibility; returns the form action.
/// Complexity: O(provider models) time.
fn ui_model_form(
    ui: &mut egui::Ui,
    draft: &mut ModelDraft,
    editing: bool,
    probing: bool,
    error: Option<&str>,
    show_key: &mut bool,
) -> FormAction {
    let mut action = FormAction::None;
    let spec = model_provider(&draft.provider_id);
    let mut chosen_provider: Option<&'static str> = None;

    // 身份行不加卡片：一个标识、一个名称、一行来源，让弹窗保持单一平面。
    // The identity block stays card-free: one mark, one name and one source line on a single plane.
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
        paint_provider_mark(ui, rect, spec.label, true);
        ui.add_space(theme::SPACE_LG);
        ui.vertical(|ui| {
            ui.add_space(1.0);
            let title = if !draft.display_name.trim().is_empty() {
                draft.display_name.trim().to_string()
            } else if !draft.model.trim().is_empty() {
                draft.model.trim().to_string()
            } else {
                t("新模型").to_string()
            };
            ui.label(
                egui::RichText::new(title)
                    .size(theme::FONT_TITLE)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.add_space(3.0);
            let subtitle = if draft.model.trim().is_empty() {
                spec.label.to_string()
            } else {
                format!("{}  ·  {}", spec.label, draft.model.trim())
            };
            ui.label(
                egui::RichText::new(subtitle)
                    .size(theme::FONT_LABEL)
                    .color(theme::TEXT_FAINT),
            );
        });
    });
    ui.add_space(theme::SPACE_LG);
    ui.separator();
    ui.add_space(theme::SPACE_MD);

    // 输入控件使用比弹窗略亮的表面与细描边，避免默认近黑输入框压过内容。
    // Inputs use a surface slightly lighter than the sheet plus a hairline border, so the default
    // near-black fields do not overpower the content.
    let visuals = ui.visuals_mut();
    visuals.extreme_bg_color = theme::PANEL_2;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, theme::LINE);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, theme::TEXT_FAINT);

    egui::ScrollArea::vertical()
        .max_height(MODELS_FORM_H)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            form_row(ui, t("服务商"), |ui| {
                settings_combo(ui, "settings_model_provider", spec.label, |ui| {
                    for provider in PROVIDERS.iter() {
                        if ui
                            .selectable_label(draft.provider_id == provider.id, provider.label)
                            .clicked()
                        {
                            chosen_provider = Some(provider.id);
                        }
                    }
                    if ui
                        .selectable_label(draft.provider_id == CUSTOM_PROVIDER_ID, t("自定义"))
                        .clicked()
                    {
                        chosen_provider = Some(CUSTOM_PROVIDER_ID);
                    }
                });
            });

            form_row(ui, t("模型"), |ui| {
                let selected = if draft.model_custom {
                    t("手动输入 Model ID").to_string()
                } else {
                    spec.models
                        .iter()
                        .find(|model| model.id == draft.model)
                        .map(|model| model.label.to_string())
                        .unwrap_or_else(|| draft.model.clone())
                };
                let mut choose_manual = false;
                settings_combo(ui, "settings_model_model", selected, |ui| {
                    for model in spec.models {
                        if ui
                            .selectable_label(
                                !draft.model_custom && draft.model == model.id,
                                format!("{}  ({})", model.label, model.id),
                            )
                            .clicked()
                        {
                            draft.model = model.id.to_string();
                            draft.model_custom = false;
                        }
                    }
                    if ui
                        .selectable_label(draft.model_custom, t("手动输入 Model ID"))
                        .clicked()
                    {
                        choose_manual = true;
                    }
                });
                if choose_manual {
                    draft.model_custom = true;
                }
                if draft.model_custom {
                    ui.add_space(theme::SPACE_XS);
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.model)
                            .hint_text("model-id")
                            .desired_width(f32::INFINITY),
                    );
                }
            });

            form_row(ui, t("展示名称"), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut draft.display_name)
                        .char_limit(64)
                        .hint_text(t("留空时显示 Model ID"))
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "{}/64",
                                draft.display_name.chars().count()
                            ))
                            .size(theme::FONT_MICRO)
                            .color(theme::TEXT_FAINT),
                        );
                    });
                });
            });

            form_divider(ui);

            form_row(ui, t("API 密钥"), |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.api_key)
                            .password(!*show_key)
                            .hint_text("sk-…")
                            .desired_width(f32::INFINITY),
                    );
                    if model_icon_button(ui, ModelIcon::Eye(*show_key), t("显示或隐藏密钥"))
                    {
                        *show_key = !*show_key;
                    }
                });
                if !spec.key_url.is_empty() {
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(t("获取 API 密钥"))
                                            .size(theme::FONT_MICRO)
                                            .color(theme::ACCENT),
                                    )
                                    .frame(false),
                                )
                                .on_hover_text(spec.key_url)
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(spec.key_url));
                            }
                        });
                    });
                }
            });

            form_divider(ui);

            form_row(ui, t("接口地址"), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut draft.base_url)
                        .hint_text("https://example.com/v1")
                        .desired_width(f32::INFINITY),
                );
            });

            form_row(ui, t("上下文窗口"), |ui| {
                ui.columns(2, |columns| {
                    form_context_field(
                        &mut columns[0],
                        t("输入"),
                        &mut draft.context_input,
                        1_024,
                        4_000_000,
                        &[
                            ("128k", 128_000),
                            ("256k", 256_000),
                            ("512k", 512_000),
                            ("1M", 1_000_000),
                        ],
                    );
                    form_context_field(
                        &mut columns[1],
                        t("输出"),
                        &mut draft.context_output,
                        256,
                        MAX_MODEL_OUTPUT_TOKENS,
                        &[
                            ("4k", 4_096),
                            ("16k", 16_384),
                            ("32k", 32_768),
                            ("128k", 128_000),
                        ],
                    );
                });
            });

            form_row(ui, t("工具调用轮数"), |ui| {
                ui.add(egui::DragValue::new(&mut draft.tool_rounds).range(1..=1_000));
            });

            form_divider(ui);

            form_row(ui, t("采样参数"), |ui| {
                ui.columns(3, |columns| {
                    form_text_field(
                        &mut columns[0],
                        "Temperature",
                        &mut draft.temperature,
                        "0 ~ 2",
                    );
                    form_text_field(&mut columns[1], "Top P", &mut draft.top_p, "0 ~ 1");
                    form_text_field(&mut columns[2], "Top K", &mut draft.top_k, "1 ~ 100");
                });
                ui.add_space(theme::SPACE_XS);
                settings_hint(ui, t("留空使用服务商默认值，不会随请求发送。"));
            });
        });

    if let Some(provider_id) = chosen_provider {
        draft.switch_provider(provider_id);
    }

    if let Some(message) = error {
        ui.add_space(theme::SPACE_XS);
        ui.label(
            egui::RichText::new(message)
                .size(theme::FONT_LABEL)
                .color(theme::RED),
        );
    }

    ui.add_space(theme::SPACE_SM);
    ui.separator();
    ui.add_space(theme::SPACE_MD);
    ui.horizontal(|ui| {
        let (icon, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
        ui.painter().circle_stroke(
            icon.center(),
            5.8,
            egui::Stroke::new(1.1_f32, theme::TEXT_FAINT),
        );
        ui.painter().text(
            icon.center(),
            egui::Align2::CENTER_CENTER,
            "i",
            egui::FontId::proportional(8.0),
            theme::TEXT_FAINT,
        );
        ui.label(
            egui::RichText::new(t("连通性测试会发起一次真实请求，会消耗少量Token"))
                .size(theme::FONT_MICRO)
                .color(theme::TEXT_FAINT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let submit_label = if editing {
                t("保存")
            } else {
                t("添加模型")
            };
            let submit = egui::Button::new(
                egui::RichText::new(submit_label)
                    .size(theme::FONT_LABEL)
                    .color(egui::Color32::WHITE),
            )
            .fill(theme::ACCENT)
            .stroke(egui::Stroke::NONE)
            .corner_radius(6)
            .min_size(egui::vec2(96.0, 30.0));
            if ui
                .add_enabled(!probing, submit)
                .on_hover_text(t("先测试地址、密钥与模型，再保存"))
                .clicked()
            {
                action = FormAction::Submit;
            }
            if probing {
                ui.add(egui::Spinner::new().size(14.0));
                ui.label(
                    egui::RichText::new(t("正在测试连通性…"))
                        .size(theme::FONT_MICRO)
                        .color(theme::TEXT_DIM),
                );
            }
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(t("重置"))
                            .size(theme::FONT_LABEL)
                            .color(theme::TEXT_DIM),
                    )
                    .frame(false),
                )
                .on_hover_text(t("恢复该模型的默认表单值"))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                action = FormAction::Reset;
            }
        });
    });
    action
}

/// 所属模块：app / 设置 / 模型
/// 功能：绘制一行表单：左侧字段标签、右侧控件，并在行末保留固定垂直间距。
/// 输入：`ui`；`label` 字段名；`add` 控件闭包。
/// 输出/副作用：只绘制与布局；控件闭包可修改草稿。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / settings / models
/// Purpose: Draw one form row with a left field label and right-hand controls, followed by a fixed
/// vertical gap.
/// Inputs: `ui`; `label`; `add`.
/// Outputs / side effects: layout and paint only; the closure may mutate the draft.
/// Complexity: O(1) time and space.
fn form_row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    let width = ui.available_width();
    let content_w = (width - MODELS_FORM_LABEL_W - MODELS_FORM_GAP).max(160.0);
    let content_h = ui.available_height().max(96.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.allocate_ui_with_layout(
            egui::vec2(MODELS_FORM_LABEL_W, content_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(MODELS_FORM_LABEL_W);
                ui.add_space(3.0);
                ui.label(
                    egui::RichText::new(label)
                        .size(theme::FONT_LABEL)
                        .color(theme::TEXT_DIM),
                );
            },
        );
        ui.add_space(MODELS_FORM_GAP);
        ui.allocate_ui_with_layout(
            egui::vec2(content_w, content_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(content_w);
                ui.spacing_mut().item_spacing.x = theme::SPACE_SM;
                add(ui);
            },
        );
    });
    ui.add_space(MODELS_FORM_ROW_GAP);
}

/// 所属模块：app / 设置 / 模型
/// 功能：在表单分组之间绘制一条细线与上下留白，替代卡片边框。
/// 输入：`ui`：表单滚动区。
/// 输出/副作用：只绘制；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / settings / models
/// Purpose: Draw a hairline with vertical breathing room between form groups instead of card borders.
/// Inputs: `ui`: form scroll area.
/// Outputs / side effects: paint only; no state change.
/// Complexity: O(1) time and space.
fn form_divider(ui: &mut egui::Ui) {
    ui.add_space(theme::SPACE_SM);
    ui.separator();
    ui.add_space(theme::SPACE_MD);
}

/// 所属模块：app / 设置 / 模型
/// 功能：绘制控件上方的小号字段标签。
/// 输入：`ui`；`text` 标签文本。
/// 输出/副作用：只绘制；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / settings / models
/// Purpose: Draw a small field label above a control.
/// Inputs: `ui`; `text`.
/// Outputs / side effects: paint only; no state change.
/// Complexity: O(1) time and space.
fn form_sub_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme::FONT_MICRO)
            .color(theme::TEXT_FAINT),
    );
}

/// 所属模块：app / 设置 / 模型
/// 功能：绘制上下文数值字段：小标签、可拖拽数值与常用值快捷按钮。
/// 输入：`ui`；`label`；`value`；`min`/`max` 范围；`chips` 快捷值。
/// 输出/副作用：点击快捷值或拖动时修改 `value`；不读盘。
/// 复杂度：时间 O(快捷值数)，空间 O(1)。
///
/// Module: app / settings / models
/// Purpose: Draw a context numeric field: small label, drag value and quick-pick chips.
/// Inputs: `ui`; `label`; `value`; `min`/`max`; `chips`.
/// Outputs / side effects: mutates `value` on drag or chip click; no disk access.
/// Complexity: O(chips) time and O(1) space.
fn form_context_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u32,
    min: u32,
    max: u32,
    chips: &[(&str, u32)],
) {
    form_sub_label(ui, label);
    ui.add(
        egui::DragValue::new(value)
            .range(min..=max)
            .speed(1_024.0)
            .suffix(" tokens"),
    );
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for (text, chip_value) in chips {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(*text)
                            .size(theme::FONT_MICRO)
                            .color(theme::TEXT_FAINT),
                    )
                    .frame(false),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                *value = *chip_value;
            }
        }
    });
}

/// 所属模块：app / 设置 / 模型
/// 功能：绘制一个文本字段：小标签与输入框。
/// 输入：`ui`；`label`；`value`；`hint` 占位文本。
/// 输出/副作用：修改 `value`；不读盘。
/// 复杂度：时间 O(文本长度)，空间 O(1)。
///
/// Module: app / settings / models
/// Purpose: Draw one text field: small label plus input.
/// Inputs: `ui`; `label`; `value`; `hint`.
/// Outputs / side effects: mutates `value`; no disk access.
/// Complexity: O(text length) time and O(1) space.
fn form_text_field(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    form_sub_label(ui, label);
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    );
}
