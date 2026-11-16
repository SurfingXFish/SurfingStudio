//! 所属模块：app / AI 编曲
//! 文件用途：
//! - 管理 AI 对话状态、流式后台线程与附件读取线程。
//! - 把模型流式返回的编曲操作逐个写入工程，并为整个任务记录一步撤销。
//! - 与全局设置协作：模型预设、API Key 与采样参数。
//!
//! 函数清单：
//! - `AiChatState::new` / `AiChatState::clear`：对话状态构造与清空。
//! - `SurfingApp::toggle_ai_panel` / `open_ai_settings` / `close_side_panel`：右侧面板切换。
//! - `SurfingApp::send_ai_message`：收集上下文、插件清单与检索配置，启动后台流式线程。
//! - `run_ai_worker`：流式补全 → 联网检索/技巧加载（如模型请求）→ 继续流式补全。
//! - `SurfingApp::ai_plugin_catalog` / `ai_plugin_param_catalog`：已安装 VST 清单与参数快照。
//! - `SurfingApp::poll_ai_chat` / `apply_ai_operation` / `finish_ai_task`：接收流式事件并逐步写入工程。
//! - `SurfingApp::apply_ai_operation_runtime` / `flush_pending_plugin_params`：把 VST 参数写入运行时并在实例就绪后重试。
//! - `SurfingApp::apply_ai_plan`：流式事件缺失时的一次性写入兜底。
//! - `SurfingApp::attach_track_to_ai` / `attach_clip_to_ai` / `attach_clips_to_ai` / `attach_clipboard_to_ai`：把轨道或片段复制为 AI 附件。
//! - `SurfingApp::cancel_ai_message`：停止等待当前请求。
//! - `SurfingApp::open_ai_attachment_dialog` / `poll_ai_attachment`：后台选择并读取附件。
//! - `read_attachment`：把文本或 MIDI 文件转成附件文本。
//! - `ai_track_reference` / `ai_clip_reference` / `ai_note_text`：轨道/片段文本快照。
//!
//! Module: app / AI composition
//! Purpose:
//! - Own AI chat state, the streaming completion worker and the attachment loader.
//! - Apply streamed operations step by step and push one undo step per task.
//! - Cooperate with global settings: model preset, API key and sampling parameters.
//!
//! Functions:
//! - `AiChatState::new` / `AiChatState::clear`: construct and clear chat state.
//! - `SurfingApp::toggle_ai_panel` / `open_ai_settings` / `close_side_panel`: right-panel switching.
//! - `SurfingApp::send_ai_message`: collect context and start the streaming worker.
//! - `run_ai_worker`: stream completions → research/technique loading → stream again.
//! - `SurfingApp::ai_plugin_catalog` / `ai_plugin_param_catalog`: installed VST catalogue and parameters.
//! - `SurfingApp::poll_ai_chat` / `apply_ai_operation` / `finish_ai_task`: consume events and write step by step.
//! - `SurfingApp::apply_ai_operation_runtime` / `flush_pending_plugin_params`: push VST parameters into
//!   runtimes and retry once the instance exists.
//! - `SurfingApp::apply_ai_plan`: one-shot fallback when streaming events are missing.
//! - `SurfingApp::attach_track_to_ai` / `attach_clip_to_ai` / `attach_clips_to_ai`: copy tracks/clips
//!   into AI attachments.
//! - `SurfingApp::cancel_ai_message`: stop waiting for the current request.
//! - `SurfingApp::open_ai_attachment_dialog` / `poll_ai_attachment`: pick and read attachments.
//! - `read_attachment`: turn a text or MIDI file into attachment text.
//! - `ai_track_reference` / `ai_clip_reference` / `ai_note_text`: text snapshots of tracks/clips.

use super::*;
use crate::ai::midi_file;
use crate::ai::prompt::{
    self, AiAttachment, AiPluginParamEntry, AiPluginParamInfo, AiPromptSelection,
};
use crate::ai::search::{self, AiSearchConfig};
use crate::ai::{
    AiApplySummary, AiLibrary, AiMessage, AiOperation, AiPlan, AiPluginInfo, AiPluginKind,
    AiRequest, AiRole, AiRuntime, AiStreamEvent, PlanStream,
};

/// 对话历史最多发送给模型的条数，避免上下文无限增长。
/// Maximum chat messages sent to the model so the context never grows without bound.
const AI_HISTORY_MESSAGES: usize = 12;
/// 文本附件最大读取字节数。
/// Maximum bytes read from a text attachment.
const AI_TEXT_ATTACHMENT_LIMIT: usize = 512 * 1024;
/// MIDI 附件最大读取字节数。
/// Maximum bytes read from a MIDI attachment.
const AI_MIDI_ATTACHMENT_LIMIT: usize = 4 * 1024 * 1024;
/// 流式展示时思考文本的字节上限（超出后停止追加，避免界面被超长推理拖慢）。
/// Byte cap for live reasoning text (stop appending beyond it so the UI stays responsive).
const AI_LIVE_REASONING_LIMIT: usize = 96 * 1024;
/// 提示词中最多列出的插件参数槽位数。
/// Maximum plugin-parameter slots listed in the prompt.
const AI_PLUGIN_PARAM_SLOTS: usize = 12;
/// 每个插件槽位最多列出的参数数。
/// Maximum parameters listed per plugin slot.
const AI_PLUGIN_PARAMS_PER_SLOT: usize = 32;

/// 一条对话消息（仅界面展示，不持久化）。
/// One chat message (display only, never persisted).
#[derive(Clone, Debug)]
pub struct AiChatMessage {
    pub role: AiRole,
    pub content: String,
    /// 思考过程（服务商返回的 reasoning_content）。
    /// Reasoning trace returned by the provider.
    pub reasoning: Option<String>,
    /// 本轮任务的耗时、检索次数与加载的技巧。
    /// Elapsed time, search count and loaded techniques of this task.
    pub meta: Option<AiMessageMeta>,
    /// 助手消息写入工程后的结构化改动，用于绘制“工程改动”卡片。
    /// Structured project changes of an assistant message, used by the change card.
    pub changes: Option<AiApplySummary>,
}

/// 已有 AI 任务运行时触发的冲突提示。
/// Conflict prompt shown when an AI task is already running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiTaskConflict {
    /// 用户想发送第二个任务。
    /// The user tried to send a second task.
    Send,
    /// 用户开始编辑编曲区/卷帘。
    /// The user started editing the arrangement or a roll.
    Edit,
}

/// 一个项目窗口内的历史对话。
/// One historical conversation inside a project window.
pub struct AiConversation {
    pub id: u64,
    pub title: String,
    pub messages: Vec<AiChatMessage>,
    pub updated_at: f64,
    /// 该对话自己的后台任务状态；切换对话时随历史一起保存。
    /// Background task state owned by this conversation; saved with the history when switching.
    pub pending: Option<AiPending>,
    pub live: Option<AiLiveMessage>,
    receiver: Option<Receiver<AiWorkerEvent>>,
    pub error: Option<String>,
}

/// 一条助手消息的任务元信息。
/// Task metadata of one assistant message.
#[derive(Clone, Debug, Default)]
pub struct AiMessageMeta {
    pub elapsed_seconds: f32,
    pub searches: usize,
    pub skills: Vec<String>,
    /// 生成本条消息的模型名。
    /// Model name that produced this message.
    pub model: String,
    /// 过程步骤（思考/写入等）。
    /// Process steps (reasoning, writes, ...).
    pub steps: Vec<String>,
}

/// 后台补全的待处理请求。
/// A completion request that is pending in the background.
#[derive(Clone, Copy, Debug)]
pub struct AiPending {
    pub request_id: u64,
    pub started: Instant,
}

/// 一个尚未写入插件运行时的参数（插件实例可能下一帧才创建）。
/// A parameter not yet written into a plugin runtime (the instance may appear next frame).
pub(super) struct PendingPluginParam {
    pub track_id: TrackId,
    pub slot: SlotKind,
    pub index: usize,
    pub value: f64,
}

/// 后台线程回传的事件：流式增量、逐个操作、最终结果或错误。
/// Event sent by the worker: streaming increments, individual operations, the final result or an error.
pub(super) enum AiWorkerEvent {
    /// 新增的思考内容。
    /// Newly received reasoning text.
    Reasoning { request_id: u64, text: String },
    /// 新增的回复内容。
    /// Newly received reply text.
    Content { request_id: u64, text: String },
    /// 当前阶段说明（检索、加载技巧等）。
    /// Description of the current stage (research, loading techniques, ...).
    Stage { request_id: u64, label: String },
    /// 一个已完整的编曲操作，UI 线程立即写入工程。
    /// One complete operation; the UI thread writes it into the project immediately.
    Operation {
        request_id: u64,
        operation: AiOperation,
    },
    /// 整个任务完成。
    /// The whole task finished.
    Done {
        request_id: u64,
        outcome: AiWorkerOutcome,
    },
    /// 任务失败。
    /// The task failed.
    Failed { request_id: u64, message: String },
}

/// 后台线程一次完整任务的结果（可能包含联网检索与技巧加载）。
/// Result of one complete worker task (possibly including web research and technique loading).
#[derive(Clone, Debug, Default)]
pub(super) struct AiWorkerOutcome {
    pub content: String,
    /// 各轮思考过程的合并文本。
    /// Merged reasoning traces from every round.
    pub reasoning: Option<String>,
    pub truncated: bool,
    /// 是否执行过联网检索。
    /// Whether web research was performed.
    pub used_search: bool,
    /// 检索次数。
    /// Number of searches.
    pub searches: usize,
    /// 已加载的技巧 id。
    /// Loaded technique ids.
    pub loaded_skills: Vec<String>,
    /// 整个任务耗时（秒）。
    /// Total task duration in seconds.
    pub elapsed_seconds: f32,
}

/// 正在流式生成的助手消息（实时思考、流式文本与已写入步骤）。
/// Assistant message currently streaming (live reasoning, streamed text and applied steps).
#[derive(Clone, Debug)]
pub struct AiLiveMessage {
    /// 已累计的思考内容。
    /// Accumulated reasoning text.
    pub reasoning: String,
    /// 已累计的回复内容（`reply` 字段的当前文本，流式展示）。
    /// Accumulated reply text (the current `reply` value, shown while streaming).
    pub content: String,
    /// 已写入的步骤说明。
    /// Descriptions of applied steps.
    pub steps: Vec<String>,
    /// 当前阶段说明。
    /// Current stage description.
    pub stage: String,
    /// 已成功写入的改动摘要。
    /// Summary of changes written so far.
    pub summary: AiApplySummary,
    /// 任务开始时间。
    /// Task start time.
    pub started: Instant,
    /// 是否已经为本次任务压入撤销快照。
    /// Whether an undo snapshot was pushed for this task.
    pub undo_pushed: bool,
    /// 第一个失败操作的原因；有值时停止继续写入。
    /// Reason of the first failed operation; further writes stop once set.
    pub failed: Option<String>,
    /// 生成本次任务的模型名。
    /// Model name of this task.
    pub model: String,
}

impl Default for AiLiveMessage {
    fn default() -> Self {
        Self {
            reasoning: String::new(),
            content: String::new(),
            steps: Vec::new(),
            stage: "正在准备…".into(),
            summary: AiApplySummary::default(),
            started: Instant::now(),
            undo_pushed: false,
            failed: None,
            model: String::new(),
        }
    }
}

/// AI 编曲面板的界面状态。
/// UI state of the AI composition panel.
pub struct AiChatState {
    pub messages: Vec<AiChatMessage>,
    pub input: String,
    /// 当前智能体 id；空或未知时按资料库第一个智能体处理。
    /// Current agent id; empty/unknown ids resolve to the first library agent.
    pub agent: String,
    /// 输入框显式选择加载的基础技能 id（空表示跟随智能体默认）。
    /// Explicitly loaded theory-skill ids (empty means the agent defaults).
    pub selected_skills: Vec<String>,
    /// 输入框显式选择加载的编曲技巧 id。
    /// Explicitly loaded technique ids.
    pub selected_techniques: Vec<String>,
    /// 用户是否手动改过技能选择；为假时按智能体默认技能显示与发送。
    /// Whether the user edited skill picks; when false the agent defaults apply.
    pub skills_customized: bool,
    /// 只允许使用的乐器（内置 id / 名称 / VST 名称）；空表示不限制。
    /// Instruments the model may use; empty means unrestricted.
    pub usable_instruments: Vec<String>,
    /// 必须使用的乐器。
    /// Instruments that must be used.
    pub required_instruments: Vec<String>,
    /// 「可用乐器」下拉的搜索文本（仅 UI）。
    /// Search text of the usable-instrument menu (UI only).
    pub usable_filter: String,
    /// 「必须乐器」下拉的搜索文本（仅 UI）。
    /// Search text of the required-instrument menu (UI only).
    pub required_filter: String,
    /// 「技能」下拉的搜索文本（仅 UI）。
    /// Search text of the skill menu (UI only).
    pub skill_filter: String,
    pub attachments: Vec<AiAttachment>,
    pub pending: Option<AiPending>,
    /// 正在流式生成的助手消息。
    /// Assistant message currently streaming.
    pub live: Option<AiLiveMessage>,
    pub error: Option<String>,
    /// 当前项目窗口内的历史对话；只属于本窗口，不写入工程。
    /// Historical conversations of this project window; window-local and never persisted.
    pub conversations: Vec<AiConversation>,
    /// 当前查看的历史对话 id；`None` 表示新任务。
    /// Id of the conversation being viewed; `None` means a new task.
    pub active_conversation: Option<u64>,
    /// 已有任务运行时的冲突提示。
    /// Conflict prompt while another task is running.
    pub conflict_prompt: Option<AiTaskConflict>,
    /// 用户选择「后台继续」后，本次任务不再重复提示编辑冲突。
    /// After choosing "keep running", stop prompting for edit conflicts in this task.
    pub edit_guard_dismissed: bool,
    receiver: Option<Receiver<AiWorkerEvent>>,
    next_request_id: u64,
    next_conversation_id: u64,
}

impl Default for AiChatState {
    /// 所属模块：app / AI 编曲 / 状态
    /// 功能：构造空白对话：无消息、默认智能体与空附件。
    /// 输入：无。
    /// 输出/副作用：默认状态；不访问网络。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / AI composition / state
    /// Purpose: Build an empty conversation: no messages, default agent and no attachments.
    /// Inputs: none.
    /// Outputs / side effects: defaults; no network access.
    /// Complexity: O(1) time and space.
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            agent: "light-music".into(),
            selected_skills: Vec::new(),
            selected_techniques: Vec::new(),
            skills_customized: false,
            usable_instruments: Vec::new(),
            required_instruments: Vec::new(),
            usable_filter: String::new(),
            required_filter: String::new(),
            skill_filter: String::new(),
            attachments: Vec::new(),
            pending: None,
            live: None,
            error: None,
            conversations: Vec::new(),
            active_conversation: None,
            conflict_prompt: None,
            edit_guard_dismissed: false,
            receiver: None,
            next_request_id: 1,
            next_conversation_id: 1,
        }
    }
}

impl AiChatState {
    /// 所属模块：app / AI 编曲 / 状态
    /// 功能：清空对话、附件、错误与等待中的请求标记。
    /// 输入：`self`。
    /// 输出/副作用：重置全部界面状态；后台线程的结果会被丢弃。
    /// 复杂度：时间 O(消息数)，空间 O(1)。
    ///
    /// Module: app / AI composition / state
    /// Purpose: Clear messages, attachments, errors and the pending marker.
    /// Inputs: `self`.
    /// Outputs / side effects: resets UI state; a late worker result is discarded.
    /// Complexity: O(messages) time and O(1) space.
    pub fn clear(&mut self) {
        self.stash_current();
        self.active_conversation = None;
        self.messages.clear();
        self.input.clear();
        self.selected_skills.clear();
        self.selected_techniques.clear();
        self.skills_customized = false;
        self.usable_instruments.clear();
        self.required_instruments.clear();
        self.usable_filter.clear();
        self.required_filter.clear();
        self.skill_filter.clear();
        self.attachments.clear();
        self.pending = None;
        self.live = None;
        self.error = None;
        self.receiver = None;
        self.conflict_prompt = None;
        self.edit_guard_dismissed = false;
    }

    /// 所属模块：app / AI 编曲 / 历史对话
    /// 功能：打开指定历史对话，并把当前消息与其后台任务一起存回历史；不取消后台任务。
    /// 输入：`id` 历史对话 id。
    /// 输出/副作用：切换 `messages`/任务状态/`active_conversation`；后台任务继续运行。
    /// 复杂度：时间 O(消息数)，空间 O(消息数)。
    ///
    /// Module: app / AI composition / history
    /// Purpose: Open a historical conversation and stash the current messages and task state; the
    /// background task is not cancelled.
    /// Inputs: `id`: conversation id.
    /// Outputs / side effects: switches messages/task state/active id; the background task keeps running.
    /// Complexity: O(messages) time and space.
    pub fn open_conversation(&mut self, id: u64) {
        if self.active_conversation == Some(id) {
            return;
        }
        self.stash_current();
        self.messages.clear();
        self.pending = None;
        self.live = None;
        self.error = None;
        self.receiver = None;
        if let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == id)
        {
            self.messages = std::mem::take(&mut conversation.messages);
            self.pending = conversation.pending.take();
            self.live = conversation.live.take();
            self.receiver = conversation.receiver.take();
            self.error = conversation.error.take();
            self.active_conversation = Some(id);
        }
        self.attachments.clear();
        self.input.clear();
        self.selected_skills.clear();
        self.selected_techniques.clear();
        self.skills_customized = false;
        self.usable_instruments.clear();
        self.required_instruments.clear();
        self.usable_filter.clear();
        self.required_filter.clear();
        self.skill_filter.clear();
        self.conflict_prompt = None;
        self.edit_guard_dismissed = false;
    }

    /// 所属模块：app / AI 编曲 / 历史对话
    /// 功能：返回当前查看的对话 id；`None` 表示新任务。
    /// 输入：`self`。
    /// 输出/副作用：id；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / AI composition / history
    /// Purpose: Return the active conversation id; `None` means a new task.
    /// Inputs: `self`.
    /// Outputs / side effects: the id; no state change.
    /// Complexity: O(1) time and space.
    pub fn active_conversation(&self) -> Option<u64> {
        self.active_conversation
    }

    /// 所属模块：app / AI 编曲 / 后台任务
    /// 功能：项目中是否有任意对话任务在运行（含已切走的历史对话）。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(历史对话数)，空间 O(1)。
    ///
    /// Module: app / AI composition / background tasks
    /// Purpose: Whether any conversation in this project has a running task, including background ones.
    /// Inputs: `self`.
    /// Outputs / side effects: a boolean; no state change.
    /// Complexity: O(conversations) time and O(1) space.
    pub fn any_task_running(&self) -> bool {
        self.pending.is_some()
            || self
                .conversations
                .iter()
                .any(|conversation| conversation.pending.is_some())
    }

    /// 所属模块：app / AI 编曲 / 后台任务
    /// 功能：项目编辑是否应当被锁住；用户选择后台继续后解锁。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(历史对话数)，空间 O(1)。
    ///
    /// Module: app / AI composition / background tasks
    /// Purpose: Whether project editing should be locked; choosing "keep running" unlocks it.
    /// Inputs: `self`.
    /// Outputs / side effects: a boolean; no state change.
    /// Complexity: O(conversations) time and O(1) space.
    pub fn project_editing_locked(&self) -> bool {
        self.any_task_running() && !self.edit_guard_dismissed
    }

    /// 所属模块：app / AI 编曲 / 历史对话
    /// 功能：当前对话已有消息但尚未登记为历史时，自动登记并设为当前任务。
    /// 输入：`self`。
    /// 输出/副作用：可能追加历史并设置 `active_conversation`；不修改消息内容。
    /// 复杂度：时间 O(消息数)，空间 O(消息数)。
    ///
    /// Module: app / AI composition / history
    /// Purpose: Register the current conversation once it has messages, and make it active.
    /// Inputs: `self`.
    /// Outputs / side effects: may append history and set `active_conversation`; messages unchanged.
    /// Complexity: O(messages) time and space.
    pub fn ensure_active_conversation(&mut self) {
        if self.active_conversation.is_none() && !self.messages.is_empty() {
            self.stash_current();
        }
    }

    /// 所属模块：app / AI 编曲 / 历史对话
    /// 功能：从第一条用户消息生成任务标题。
    /// 输入：`messages` 消息列表。
    /// 输出/副作用：标题文本；不修改输入。
    /// 复杂度：时间 O(消息数 + 文本长度)，空间 O(1)。
    ///
    /// Module: app / AI composition / history
    /// Purpose: Build a task title from the first user message.
    /// Inputs: `messages`.
    /// Outputs / side effects: the title; the input is unchanged.
    /// Complexity: O(messages + text length) time and O(1) space.
    pub fn conversation_title(messages: &[AiChatMessage]) -> String {
        for message in messages {
            if message.role == AiRole::User {
                let text = message.content.replace('\n', " ");
                let text = text.trim();
                if !text.is_empty() {
                    return truncate_title(text, 22);
                }
            }
        }
        "新任务".into()
    }

    fn stash_current(&mut self) {
        if self.messages.is_empty() {
            return;
        }
        let title = Self::conversation_title(&self.messages);
        let updated_at = now_seconds();
        if let Some(id) = self.active_conversation {
            if let Some(conversation) = self
                .conversations
                .iter_mut()
                .find(|conversation| conversation.id == id)
            {
                conversation.title = title;
                conversation.messages = std::mem::take(&mut self.messages);
                conversation.updated_at = updated_at;
                conversation.pending = self.pending.take();
                conversation.live = self.live.take();
                conversation.receiver = self.receiver.take();
                conversation.error = self.error.take();
                return;
            }
        }
        let id = self.next_conversation_id;
        self.next_conversation_id = self.next_conversation_id.saturating_add(1);
        // 新任务首次发送后登记为历史；此时任务仍留在活动区，历史项不保存任务状态。
        // Register a freshly sent task as history; the task state stays in the active fields.
        self.conversations.push(AiConversation {
            id,
            title,
            messages: self.messages.clone(),
            updated_at,
            pending: None,
            live: None,
            receiver: None,
            error: None,
        });
        self.active_conversation = Some(id);
    }

    /// 所属模块：app / AI 编曲 / 智能体
    /// 功能：切换智能体并清空显式技能选择，让提示词回到该智能体的默认技能。
    /// 输入：`agent_id`：资料库中的智能体 id。
    /// 输出/副作用：修改 `agent` 与技能选择；不修改工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / AI composition / agent
    /// Purpose: Switch the agent and clear explicit skill picks so the prompt uses its defaults.
    /// Inputs: `agent_id`: library agent id.
    /// Outputs / side effects: changes the agent and skill selections; no project change.
    /// Complexity: O(1) time and space.
    pub fn select_agent(&mut self, agent_id: &str) {
        self.agent = agent_id.to_string();
        self.selected_skills.clear();
        self.selected_techniques.clear();
        self.skills_customized = false;
    }
}

/// 所属模块：app / AI 编曲 / 历史对话
/// 功能：按字符数截断任务标题。
/// 输入：`text` 原文本、`max` 最大字符数。
/// 输出/副作用：截断文本；不修改输入。
/// 复杂度：时间 O(字符数)，空间 O(1)。
///
/// Module: app / AI composition / history
/// Purpose: Truncate a task title by character count.
/// Inputs: `text`, `max`.
/// Outputs / side effects: the truncated text; the input is unchanged.
/// Complexity: O(characters) time and O(1) space.
fn truncate_title(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut result: String = text.chars().take(max.saturating_sub(1)).collect();
    result.push('…');
    result
}

/// 所属模块：app / AI 编曲 / 历史对话
/// 功能：返回当前 Unix 秒数，用于对话更新时间排序。
/// 输入：无。
/// 输出/副作用：秒级时间戳；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / AI composition / history
/// Purpose: Return Unix seconds for conversation update ordering.
/// Inputs: none.
/// Outputs / side effects: a second timestamp; no state change.
/// Complexity: O(1) time and space.
fn now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

impl SurfingApp {
    /// 所属模块：app / AI 编曲 / 面板
    /// 功能：切换 AI 编曲面板：未打开时打开，已打开时收起。
    /// 输入：`self`。
    /// 输出/副作用：修改 `side_panel`；不修改工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / AI composition / panel
    /// Purpose: Toggle the AI panel: open it when hidden, close it when already open.
    /// Inputs: `self`.
    /// Outputs / side effects: changes `side_panel`; no project change.
    /// Complexity: O(1) time and space.
    pub fn toggle_ai_panel(&mut self) {
        self.side_panel = match self.side_panel {
            Some(SidePanel::Ai) => None,
            _ => Some(SidePanel::Ai),
        };
    }

    /// 所属模块：app / AI 编曲 / 面板
    /// 功能：收起右侧面板（轨道属性与 AI 对话共用一个位置）。
    /// 输入：`self`。
    /// 输出/副作用：`side_panel = None`；不修改工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / AI composition / panel
    /// Purpose: Close the right-hand panel (track properties and AI chat share one slot).
    /// Inputs: `self`.
    /// Outputs / side effects: `side_panel = None`; no project change.
    /// Complexity: O(1) time and space.
    pub fn close_side_panel(&mut self) {
        self.side_panel = None;
    }

    /// 所属模块：app / AI 编曲 / 面板
    /// 功能：打开设置窗口并定位到 AI 页。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：打开设置窗口并切换分类。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / AI composition / panel
    /// Purpose: Open the settings window on the AI page.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: opens settings and switches the category.
    /// Complexity: O(1) time and space.
    pub fn open_ai_settings(&mut self, ctx: &egui::Context) {
        self.open_settings(ctx);
        self.settings_page = settings::SettingsPage::Models;
    }

    /// 所属模块：app / AI 编曲 / 插件清单
    /// 功能：把已扫描插件转换为 AI 清单；当前平台不支持 VST 时返回空列表。
    /// 输入：`self`。
    /// 输出/副作用：插件条目列表；不包含本机路径之外的额外信息，不修改状态。
    /// 复杂度：时间 O(插件数 log 插件数)，空间 O(插件数)。
    ///
    /// Module: app / AI composition / plugin catalogue
    /// Purpose: Convert scanned plugins into the AI catalogue; an empty list when the platform has no
    /// VST hosting.
    /// Inputs: `self`.
    /// Outputs / side effects: plugin entries; no state change.
    /// Complexity: O(plugins log plugins) time and O(plugins) space.
    fn ai_plugin_catalog(&self) -> Vec<AiPluginInfo> {
        if !crate::platform::caps().plugin_hosting {
            return Vec::new();
        }
        let mut plugins: Vec<AiPluginInfo> = self
            .plugin_cache
            .iter()
            .map(|plugin| {
                let name = if plugin.name.is_empty() {
                    plugin
                        .path
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().to_string())
                        .unwrap_or_else(|| "插件".into())
                } else {
                    plugin.name.clone()
                };
                let kind = match plugin.kind {
                    crate::plugin::PluginKind::Instrument => AiPluginKind::Instrument,
                    crate::plugin::PluginKind::Effect => AiPluginKind::Effect,
                    crate::plugin::PluginKind::Unknown => AiPluginKind::Unknown,
                };
                // 扫描结果可能缺少厂商/分类；用离线资料库补全，并附上中文用途说明，
                // 让模型能判断“Serum 是波表合成器”“Pro-Q 4 是均衡器”从而主动选用。
                // Scan results may lack vendor/category; fill them from the offline catalog and add a
                // Chinese description so the model knows what each plugin is good for.
                let catalog = crate::plugin::lookup_catalog(&name);
                AiPluginInfo {
                    description: catalog
                        .map(|entry| entry.description.to_string())
                        .unwrap_or_default(),
                    vendor: if plugin.vendor.is_empty() {
                        catalog
                            .map(|entry| entry.vendor.to_string())
                            .unwrap_or_default()
                    } else {
                        plugin.vendor.clone()
                    },
                    category: if plugin.category.is_empty() {
                        catalog
                            .map(|entry| entry.category.to_string())
                            .unwrap_or_default()
                    } else {
                        plugin.category.clone()
                    },
                    name,
                    format: plugin.format,
                    path: plugin.path.clone(),
                    uid: plugin.uid.clone(),
                    kind,
                    has_midi_input: plugin.has_midi_input,
                }
            })
            .collect();
        plugins.sort_by(|a, b| {
            let rank = |kind: AiPluginKind| match kind {
                AiPluginKind::Instrument => 0,
                AiPluginKind::Effect => 1,
                AiPluginKind::Unknown => 2,
            };
            rank(a.kind)
                .cmp(&rank(b.kind))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        plugins
    }

    /// 所属模块：app / AI 编曲 / 插件参数
    /// 功能：收集已连接插件的参数快照（归一化值），供提示词给出可调 index。
    /// 输入：`self`。
    /// 输出/副作用：参数快照列表；不修改工程与运行时。
    /// 复杂度：时间 O(轨道数 × 参数数)，空间 O(参数数)。
    ///
    /// Module: app / AI composition / plugin parameters
    /// Purpose: Collect normalized parameter snapshots of connected plugins so the prompt can expose
    /// adjustable indices.
    /// Inputs: `self`.
    /// Outputs / side effects: parameter snapshots; the project and runtimes are unchanged.
    /// Complexity: O(tracks × parameters) time and O(parameters) space.
    fn ai_plugin_param_catalog(&mut self) -> Vec<AiPluginParamInfo> {
        if !crate::platform::caps().plugin_hosting {
            return Vec::new();
        }
        let mut slots: Vec<(TrackId, String, String, Option<usize>)> = Vec::new();
        for track in &self.project.tracks {
            if matches!(track.instrument, InstrumentRef::Plugin(_)) {
                slots.push((track.id, track.name.clone(), track.instrument.label(), None));
            }
            for (index, effect) in track.effects.iter().enumerate() {
                if let Some(plugin) = effect.plugin() {
                    slots.push((
                        track.id,
                        track.name.clone(),
                        plugin.name.clone(),
                        Some(index),
                    ));
                }
            }
        }
        let mut catalog = Vec::new();
        for (track_id, track_name, plugin_name, effect_index) in
            slots.into_iter().take(AI_PLUGIN_PARAM_SLOTS)
        {
            let slot = effect_index
                .map(SlotKind::Effect)
                .unwrap_or(SlotKind::Instrument);
            let params = self.cached_plugin_params(track_id, slot);
            if params.is_empty() {
                continue;
            }
            catalog.push(AiPluginParamInfo {
                track: track_name,
                slot: if effect_index.is_some() {
                    "effect".into()
                } else {
                    "instrument".into()
                },
                effect_index,
                plugin: plugin_name,
                params: params
                    .into_iter()
                    .take(AI_PLUGIN_PARAMS_PER_SLOT)
                    .enumerate()
                    .map(|(index, param)| AiPluginParamEntry {
                        index,
                        name: param.name,
                        value: param.value as f32,
                        display: param.display,
                    })
                    .collect(),
            });
        }
        catalog
    }

    /// 所属模块：app / AI 编曲 / 发送
    /// 功能：收集系统提示词与历史消息，在后台线程请求一次补全；缺少密钥或自定义地址时只提示。
    /// 输入：`self`。
    /// 输出/副作用：追加用户消息、清空输入、设置 `pending` 与接收通道；不修改工程。
    /// 复杂度：时间 O(工程大小 + 技能长度)，空间 O(同)。
    ///
    /// Module: app / AI composition / send
    /// Purpose: Collect the system prompt and history and request one completion on a worker thread;
    /// a missing key or custom URL only shows an error.
    /// Inputs: `self`.
    /// Outputs / side effects: appends the user message, clears the input, sets `pending` and the
    /// receiver; no project change.
    /// Complexity: O(project + skills) time and space.
    pub fn send_ai_message(&mut self) {
        if self.ai_chat.pending.is_some() {
            return;
        }
        let text = self.ai_chat.input.trim().to_string();
        if text.is_empty() && self.ai_chat.attachments.is_empty() {
            return;
        }
        let text = if text.is_empty() {
            "请参考附件。".to_string()
        } else {
            text
        };
        let Some(active_model) = self.settings.ai.active_model().cloned() else {
            self.ai_chat.error = Some("尚未添加模型，请在 设置 → 模型中添加".into());
            return;
        };
        if !active_model.enabled {
            self.ai_chat.error = Some("当前模型已停用，请在 设置 → 模型中启用或切换模型".into());
            return;
        }
        if active_model.api_key.trim().is_empty() {
            self.ai_chat.error = Some(format!(
                "尚未配置 {} 的 API Key，请在 设置 → 模型中填写",
                active_model.provider_label()
            ));
            return;
        }
        if active_model.model.trim().is_empty()
            || active_model.resolved_base_url().trim().is_empty()
        {
            self.ai_chat.error = Some("当前模型的接口地址或模型 ID 为空".into());
            return;
        }
        self.ai_chat.error = None;
        self.ai_chat.messages.push(AiChatMessage {
            role: AiRole::User,
            content: text,
            reasoning: None,
            meta: None,
            changes: None,
        });
        self.ai_chat.input.clear();
        let plugins = self.ai_plugin_catalog();
        let plugin_params = self.ai_plugin_param_catalog();
        let library = self.settings.ai.library.clone();
        let selection = AiPromptSelection {
            skill_ids: self.ai_chat.selected_skills.clone(),
            technique_ids: self.ai_chat.selected_techniques.clone(),
            skills_customized: self.ai_chat.skills_customized,
            usable_instruments: self.ai_chat.usable_instruments.clone(),
            required_instruments: self.ai_chat.required_instruments.clone(),
        };
        let system = prompt::system_prompt(
            &library,
            &self.ai_chat.agent,
            &selection,
            &self.project,
            &self.ai_chat.attachments,
            &plugins,
            &plugin_params,
        );
        let mut messages = vec![AiMessage {
            role: AiRole::System,
            content: system,
        }];
        let start = self
            .ai_chat
            .messages
            .len()
            .saturating_sub(AI_HISTORY_MESSAGES);
        for message in &self.ai_chat.messages[start..] {
            messages.push(AiMessage {
                role: message.role,
                content: message.content.clone(),
            });
        }
        let model_name = active_model.display_label().to_string();
        let request = active_model.build_request(
            messages,
            Duration::from_secs(self.settings.ai.timeout_seconds.max(30)),
            active_model.context_output,
            true,
        );
        let request_id = self.ai_chat.next_request_id;
        self.ai_chat.next_request_id = self.ai_chat.next_request_id.wrapping_add(1).max(1);
        // 联网搜索用 DeepSeek 的 Search 接口，因此需要 DeepSeek Key；没有 Key 时搜索关闭，
        // 模型仍可基于已有知识完成编曲。
        // Web research uses DeepSeek's Search API and therefore a DeepSeek key; without one the model
        // still composes from its own knowledge.
        let search_key = self.settings.ai.provider_api_key("deepseek").to_string();
        let search = AiSearchConfig {
            enabled: self.settings.ai.search_enabled && !search_key.is_empty(),
            base_url: "https://api.deepseek.com".into(),
            api_key: search_key,
            timeout: Duration::from_secs(30),
        };
        let (tx, rx) = channel();
        // 网络请求只在工作线程执行；UI 线程每帧非阻塞轮询事件并逐条写入工程。
        // Network calls run on a worker only; the UI thread polls events without blocking and writes
        // each completed operation into the project.
        let spawned = std::thread::Builder::new()
            .name("ai-compose".into())
            .spawn(move || {
                let events = tx.clone();
                match run_ai_worker(request, search, library, request_id, tx) {
                    Ok(outcome) => {
                        let _ = events.send(AiWorkerEvent::Done {
                            request_id,
                            outcome,
                        });
                    }
                    Err(message) => {
                        let _ = events.send(AiWorkerEvent::Failed {
                            request_id,
                            message,
                        });
                    }
                }
            });
        if let Err(error) = spawned {
            self.ai_chat.error = Some(format!("无法启动 AI 后台线程：{error}"));
            return;
        }
        self.ai_chat.receiver = Some(rx);
        self.ai_chat.live = Some(AiLiveMessage {
            stage: "正在思考…".into(),
            model: model_name,
            ..AiLiveMessage::default()
        });
        self.ai_chat.pending = Some(AiPending {
            request_id,
            started: Instant::now(),
        });
        self.ai_chat.conflict_prompt = None;
        self.ai_chat.edit_guard_dismissed = false;
    }

    /// 所属模块：app / AI 编曲 / 冲突
    /// 功能：AI 任务后台运行时，若用户在任意工程编辑区按下鼠标，则请求冲突提示。
    /// 输入：`ctx` 渲染上下文、`rect` 编辑区矩形。
    /// 输出/副作用：可能设置 `conflict_prompt`；不修改工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / AI composition / conflict
    /// Purpose: Request a conflict prompt when the user presses the mouse in any project edit surface
    /// while an AI task runs in the background.
    /// Inputs: `ctx`, `rect`.
    /// Outputs / side effects: may set `conflict_prompt`; no project change.
    /// Complexity: O(1) time and space.
    pub fn note_edit_surface_interaction(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        if !self.ai_chat.any_task_running()
            || self.ai_chat.conflict_prompt.is_some()
            || self.ai_chat.edit_guard_dismissed
        {
            return;
        }
        let pressed = ctx.input(|input| input.pointer.primary_pressed());
        if !pressed {
            return;
        }
        if let Some(position) = ctx.input(|input| input.pointer.interact_pos()) {
            if rect.contains(position) {
                self.ai_chat.conflict_prompt = Some(AiTaskConflict::Edit);
            }
        }
    }

    /// 所属模块：app / AI 编曲 / 轮询
    /// 功能：非阻塞接收后台事件：实时思考/回复增量、逐个操作写入工程、最终结果或错误。
    /// 输入：`self`。
    /// 输出/副作用：更新 `live`、追加助手消息、逐步修改工程并压入一次撤销、更新状态栏与错误。
    /// 复杂度：时间 O(事件数 + 写入操作数 × 轨道数)，空间 O(流式文本)。
    ///
    /// Module: app / AI composition / poll
    /// Purpose: Receive worker events without blocking: live reasoning/reply increments, individual
    /// operations written into the project, the final result or an error.
    /// Inputs: `self`.
    /// Outputs / side effects: updates `live`, appends an assistant message, mutates the project step
    /// by step with one undo step, updates the status bar and error.
    /// Complexity: O(events + applied operations × tracks) time and O(streaming text) space.
    pub(super) fn poll_ai_chat(&mut self) {
        self.poll_background_ai_tasks();
        self.poll_active_ai_chat();
    }

    /// 所属模块：app / AI 编曲 / 后台任务
    /// 功能：依次轮询已切走对话的后台任务；临时借用活动对话字段，处理完再还原。
    /// 输入：`self`。
    /// 输出/副作用：把后台事件应用到工程与对应对话消息；不切换当前查看的对话。
    /// 复杂度：时间 O(后台任务事件数 + 写入操作数 × 轨道数)，空间 O(消息数)。
    ///
    /// Module: app / AI composition / background tasks
    /// Purpose: Poll background tasks of conversations that are not currently viewed, temporarily
    /// borrowing the active fields and restoring them afterwards.
    /// Inputs: `self`.
    /// Outputs / side effects: applies background events to the project and their conversation; the
    /// visible conversation does not change.
    /// Complexity: O(background events + operations × tracks) time and O(messages) space.
    fn poll_background_ai_tasks(&mut self) {
        let active = self.ai_chat.active_conversation;
        let indices: Vec<usize> = self
            .ai_chat
            .conversations
            .iter()
            .enumerate()
            .filter(|(_, conversation)| {
                conversation.receiver.is_some() && Some(conversation.id) != active
            })
            .map(|(index, _)| index)
            .collect();
        for index in indices {
            let (
                background_messages,
                background_pending,
                background_live,
                background_receiver,
                background_error,
            ) = {
                let conversation = &mut self.ai_chat.conversations[index];
                (
                    std::mem::take(&mut conversation.messages),
                    conversation.pending.take(),
                    conversation.live.take(),
                    conversation.receiver.take(),
                    conversation.error.take(),
                )
            };
            let active_messages = std::mem::take(&mut self.ai_chat.messages);
            let active_pending = self.ai_chat.pending.take();
            let active_live = self.ai_chat.live.take();
            let active_receiver = self.ai_chat.receiver.take();
            let active_error = self.ai_chat.error.take();
            let saved_conflict = self.ai_chat.conflict_prompt.take();
            let saved_guard = self.ai_chat.edit_guard_dismissed;

            self.ai_chat.messages = background_messages;
            self.ai_chat.pending = background_pending;
            self.ai_chat.live = background_live;
            self.ai_chat.receiver = background_receiver;
            self.ai_chat.error = background_error;
            self.poll_active_ai_chat();

            let background_messages = std::mem::take(&mut self.ai_chat.messages);
            let background_pending = self.ai_chat.pending.take();
            let background_live = self.ai_chat.live.take();
            let background_receiver = self.ai_chat.receiver.take();
            let background_error = self.ai_chat.error.take();

            self.ai_chat.messages = active_messages;
            self.ai_chat.pending = active_pending;
            self.ai_chat.live = active_live;
            self.ai_chat.receiver = active_receiver;
            self.ai_chat.error = active_error;
            self.ai_chat.conflict_prompt = saved_conflict;
            self.ai_chat.edit_guard_dismissed = saved_guard;

            let conversation = &mut self.ai_chat.conversations[index];
            conversation.messages = background_messages;
            conversation.pending = background_pending;
            conversation.live = background_live;
            conversation.receiver = background_receiver;
            conversation.error = background_error;
        }
    }

    /// 所属模块：app / AI 编曲 / 轮询
    /// 功能：非阻塞接收当前对话的后台事件。
    /// 输入：`self`。
    /// 输出/副作用：更新 `live`、追加助手消息、逐步修改工程并压入一次撤销、更新状态栏与错误。
    /// 复杂度：时间 O(事件数 + 写入操作数 × 轨道数)，空间 O(流式文本)。
    ///
    /// Module: app / AI composition / poll
    /// Purpose: Receive events of the active conversation without blocking.
    /// Inputs: `self`.
    /// Outputs / side effects: updates `live`, appends an assistant message, mutates the project step
    /// by step with one undo step, updates the status bar and error.
    /// Complexity: O(events + applied operations × tracks) time and O(streaming text) space.
    fn poll_active_ai_chat(&mut self) {
        let Some(receiver) = &self.ai_chat.receiver else {
            return;
        };
        let mut events = Vec::new();
        let mut disconnected = false;
        loop {
            match receiver.try_recv() {
                Ok(event) => events.push(event),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if events.is_empty() && !disconnected {
            return;
        }
        let Some(pending) = self.ai_chat.pending else {
            self.ai_chat.receiver = None;
            return;
        };
        for event in events {
            match event {
                AiWorkerEvent::Reasoning { request_id, text } => {
                    if request_id != pending.request_id {
                        continue;
                    }
                    if let Some(live) = self.ai_chat.live.as_mut() {
                        if live.reasoning.len() < AI_LIVE_REASONING_LIMIT {
                            live.reasoning.push_str(&text);
                        }
                        live.stage = "正在思考…".into();
                    }
                }
                AiWorkerEvent::Content { request_id, text } => {
                    if request_id != pending.request_id {
                        continue;
                    }
                    if let Some(live) = self.ai_chat.live.as_mut() {
                        live.content = text;
                    }
                }
                AiWorkerEvent::Stage { request_id, label } => {
                    if request_id != pending.request_id {
                        continue;
                    }
                    if let Some(live) = self.ai_chat.live.as_mut() {
                        live.stage = label;
                    }
                }
                AiWorkerEvent::Operation {
                    request_id,
                    operation,
                } => {
                    if request_id != pending.request_id {
                        continue;
                    }
                    self.apply_ai_operation(operation);
                }
                AiWorkerEvent::Done {
                    request_id,
                    outcome,
                } => {
                    if request_id != pending.request_id {
                        continue;
                    }
                    self.ai_chat.receiver = None;
                    self.ai_chat.pending = None;
                    self.ai_chat.conflict_prompt = None;
                    self.ai_chat.edit_guard_dismissed = false;
                    let live = self.ai_chat.live.take();
                    self.finish_ai_task(outcome, live);
                    return;
                }
                AiWorkerEvent::Failed {
                    request_id,
                    message,
                } => {
                    if request_id != pending.request_id {
                        continue;
                    }
                    self.ai_chat.receiver = None;
                    self.ai_chat.pending = None;
                    self.ai_chat.conflict_prompt = None;
                    self.ai_chat.edit_guard_dismissed = false;
                    let live = self.ai_chat.live.take();
                    self.fail_ai_task(message, live);
                    return;
                }
            }
        }
        if disconnected {
            self.ai_chat.receiver = None;
            self.ai_chat.pending = None;
            self.ai_chat.conflict_prompt = None;
            self.ai_chat.edit_guard_dismissed = false;
            let live = self.ai_chat.live.take();
            self.fail_ai_task("AI 后台线程已退出".into(), live);
        }
    }

    /// 所属模块：app / AI 编曲 / 应用
    /// 功能：把一个完整的编曲操作立即写入工程；本次任务的第一个操作压入一次撤销。
    /// 输入：`operation`：流式解析出的单个操作。
    /// 输出/副作用：可能修改 `self.project`、`live` 摘要与状态栏；失败时记录原因并停止后续写入。
    /// 复杂度：时间 O(轨道数 + 本操作音符数 + 插件数)，空间 O(1)。
    ///
    /// Module: app / AI composition / apply
    /// Purpose: Write one complete operation into the project immediately; the first operation of a
    /// task pushes a single undo snapshot.
    /// Inputs: `operation`: one operation parsed from the stream.
    /// Outputs / side effects: may mutate `self.project`, the `live` summary and the status bar; a
    /// failure records the reason and stops later writes.
    /// Complexity: O(tracks + notes in the operation + plugins) time and O(1) space.
    fn apply_ai_operation(&mut self, operation: AiOperation) {
        if self
            .ai_chat
            .live
            .as_ref()
            .is_some_and(|live| live.failed.is_some())
        {
            return;
        }
        let label = operation.describe();
        let needs_undo = self
            .ai_chat
            .live
            .as_ref()
            .is_some_and(|live| !live.undo_pushed);
        if needs_undo {
            self.push_undo_now();
            if let Some(live) = self.ai_chat.live.as_mut() {
                live.undo_pushed = true;
            }
        }
        let plugins = self.ai_plugin_catalog();
        let mut summary = self
            .ai_chat
            .live
            .as_ref()
            .map(|live| live.summary)
            .unwrap_or_default();
        match operation.apply(&mut self.project, &plugins, &mut summary) {
            Ok(()) => {
                self.apply_ai_operation_runtime(&operation);
                self.mark_dirty();
                if let Some(live) = self.ai_chat.live.as_mut() {
                    live.summary = summary;
                    live.steps.push(label.clone());
                    live.stage = "正在写入…".into();
                }
                self.status = format!("AI 正在写入：{label}");
            }
            Err(error) => {
                self.ai_chat.error = Some(error.clone());
                if let Some(live) = self.ai_chat.live.as_mut() {
                    live.failed = Some(error);
                    live.stage = "写入中断".into();
                }
            }
        }
    }

    /// 所属模块：app / AI 编曲 / 插件参数
    /// 功能：把 VST 参数操作立即推送到插件运行时；实例尚未就绪时排队到后续帧。
    /// 输入：`operation`：刚写入工程的 AI 操作。
    /// 输出/副作用：可能修改插件运行时、参数缓存或待处理队列；不修改工程内容。
    /// 复杂度：时间 O(参数数)，空间 O(参数数)。
    ///
    /// Module: app / AI composition / plugin parameters
    /// Purpose: Push a VST parameter operation into plugin runtimes; queue it for later frames when
    /// the instance is not ready yet.
    /// Inputs: `operation`: the AI operation just written into the project.
    /// Outputs / side effects: may modify plugin runtimes, the parameter cache or the pending queue;
    /// project content is unchanged.
    /// Complexity: O(parameters) time and space.
    fn apply_ai_operation_runtime(&mut self, operation: &AiOperation) {
        let AiOperation::SetPluginParams {
            track,
            slot,
            effect,
            params,
        } = operation
        else {
            return;
        };
        let Ok(track_index) = crate::ai::plan::find_track(&self.project, track) else {
            return;
        };
        let track_id = self.project.tracks[track_index].id;
        let slot_kind = match slot.trim().to_lowercase().as_str() {
            "instrument" => SlotKind::Instrument,
            "effect" => {
                let Some(index) = effect else {
                    return;
                };
                SlotKind::Effect(*index)
            }
            _ => return,
        };
        for param in params {
            let pending = PendingPluginParam {
                track_id,
                slot: slot_kind,
                index: param.index,
                value: param.value.clamp(0.0, 1.0),
            };
            if !self.write_plugin_param_now(&pending) {
                self.pending_plugin_params.push(pending);
            }
        }
        self.invalidate_plugin_param_slot(track_id, slot_kind);
    }

    /// 所属模块：app / AI 编曲 / 插件参数
    /// 功能：尝试把一个参数写入插件运行时；引擎锁忙或插件不存在时返回 `false`。
    /// 输入：`pending`：待写入参数。
    /// 输出/副作用：成功时调用插件 `setParamNormalized`；不修改工程。
    /// 复杂度：时间 O(轨道数 + 效果器数)，空间 O(1)。
    ///
    /// Module: app / AI composition / plugin parameters
    /// Purpose: Try to write one parameter into a plugin runtime; returns `false` while the engine is
    /// busy or the plugin is missing.
    /// Inputs: `pending`.
    /// Outputs / side effects: calls the plugin's `setParamNormalized` on success; no project change.
    /// Complexity: O(tracks + effects) time and O(1) space.
    fn write_plugin_param_now(&self, pending: &PendingPluginParam) -> bool {
        let Ok(mut engine) = self.engine.try_lock() else {
            return false;
        };
        engine
            .set_plugin_param(pending.track_id, pending.slot, pending.index, pending.value)
            .is_ok()
    }

    /// 所属模块：app / AI 编曲 / 插件参数
    /// 功能：每帧重试待写入的插件参数；轨道已被删除的参数直接丢弃。
    /// 输入：`self`。
    /// 输出/副作用：可能写入插件运行时并清空队列项；不修改工程。
    /// 复杂度：时间 O(待处理数 × 轨道数)，空间 O(待处理数)。
    ///
    /// Module: app / AI composition / plugin parameters
    /// Purpose: Retry pending plugin parameters every frame and drop entries whose track is gone.
    /// Inputs: `self`.
    /// Outputs / side effects: may write plugin runtimes and drain queue entries; no project change.
    /// Complexity: O(pending × tracks) time and O(pending) space.
    pub(super) fn flush_pending_plugin_params(&mut self) {
        if self.pending_plugin_params.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending_plugin_params);
        let mut remaining = Vec::new();
        for param in pending {
            if !self
                .project
                .tracks
                .iter()
                .any(|track| track.id == param.track_id)
            {
                continue;
            }
            if !self.write_plugin_param_now(&param) {
                remaining.push(param);
            }
        }
        self.pending_plugin_params = remaining;
    }

    /// 所属模块：app / AI 编曲 / 收尾
    /// 功能：把流式任务结果转成正式助手消息；流式阶段未写入完整计划时补写。
    /// 输入：`outcome` 后台结果；`live` 流式状态（可能为空）。
    /// 输出/副作用：追加助手消息、清理插件缓存与选择、更新状态栏与错误。
    /// 复杂度：时间 O(内容长度 + 计划大小 × 轨道数)，空间 O(工程副本)。
    ///
    /// Module: app / AI composition / finalise
    /// Purpose: Turn a streaming task into a regular assistant message; apply the plan if the stream
    /// never wrote it.
    /// Inputs: `outcome` worker result; `live` streaming state (may be empty).
    /// Outputs / side effects: appends an assistant message, clears plugin caches and selection,
    /// updates the status bar and error.
    /// Complexity: O(content + plan size × tracks) time and O(project copy) space.
    fn finish_ai_task(&mut self, outcome: AiWorkerOutcome, live: Option<AiLiveMessage>) {
        let applied_summary = live.as_ref().map(|live| live.summary).unwrap_or_default();
        let applied_steps = live.as_ref().map(|live| live.steps.len()).unwrap_or(0);
        let failed = live.as_ref().and_then(|live| live.failed.clone());
        let live_model = live
            .as_ref()
            .map(|live| live.model.clone())
            .filter(|model| !model.is_empty())
            .unwrap_or_else(|| {
                self.settings
                    .ai
                    .active_model()
                    .map(|model| model.display_label().to_string())
                    .unwrap_or_default()
            });
        let live_steps = live
            .as_ref()
            .map(|live| live.steps.clone())
            .unwrap_or_default();
        let live_reply = live
            .as_ref()
            .map(|live| live.content.trim().to_string())
            .filter(|text| !text.is_empty());
        let mut changes = None;
        let mut text = match AiPlan::parse(&outcome.content) {
            Ok(plan) => {
                let reply = if plan.reply.trim().is_empty() {
                    "已生成编曲计划。".to_string()
                } else {
                    plan.reply.trim().to_string()
                };
                if applied_steps > 0 {
                    if summary_has_changes(&applied_summary) {
                        changes = Some(applied_summary);
                        format!("{reply}\n\n已写入：{}", applied_summary.summary_text())
                    } else {
                        reply
                    }
                } else if !plan.operations.is_empty() {
                    // 流式事件缺失时的兜底：一次性写入，保持旧行为。
                    // Fallback when streaming events were missing: write the plan in one go.
                    match self.apply_ai_plan(plan) {
                        Ok(summary) => {
                            changes = Some(summary);
                            format!("{reply}\n\n已写入：{}", summary.summary_text())
                        }
                        Err(error) => {
                            self.ai_chat.error = Some(error.clone());
                            format!("{reply}\n\n未能写入工程：{error}")
                        }
                    }
                } else {
                    reply
                }
            }
            Err(_) => live_reply.unwrap_or_else(|| outcome.content.trim().to_string()),
        };
        if let Some(error) = failed {
            text.push_str(&format!("\n\n部分操作未能写入：{error}"));
        }
        if outcome.used_search {
            text = format!("已联网检索资料。\n\n{text}");
        }
        if outcome.truncated {
            text.push_str("\n\n（回复因最大输出上限被截断；可在 设置 → 模型 调高“输出”上限。）");
        }
        if text.is_empty() {
            text = "（模型没有返回内容）".into();
        }
        if let Some(summary) = changes {
            self.cleanup_after_ai_write();
            self.status = format!("AI 编曲：{}", summary.summary_text());
        }
        let reasoning = outcome.reasoning.or_else(|| {
            live.and_then(|live| (!live.reasoning.trim().is_empty()).then_some(live.reasoning))
        });
        let model = live_model;
        let steps = live_steps;
        self.ai_chat.messages.push(AiChatMessage {
            role: AiRole::Assistant,
            content: text,
            reasoning,
            meta: Some(AiMessageMeta {
                elapsed_seconds: outcome.elapsed_seconds,
                searches: outcome.searches,
                skills: outcome.loaded_skills,
                model,
                steps,
            }),
            changes,
        });
    }

    /// 所属模块：app / AI 编曲 / 失败
    /// 功能：把后台错误转成助手消息，并提示已写入部分可撤销。
    /// 输入：`message` 错误文本；`live` 流式状态。
    /// 输出/副作用：追加助手消息与错误提示；不修改工程。
    /// 复杂度：时间 O(思考文本长度)，空间 O(同)。
    ///
    /// Module: app / AI composition / failure
    /// Purpose: Turn a worker error into an assistant message and note that partial writes are undoable.
    /// Inputs: `message` error text; `live` streaming state.
    /// Outputs / side effects: appends an assistant message and an error; no project change.
    /// Complexity: O(reasoning length) time and space.
    fn fail_ai_task(&mut self, message: String, live: Option<AiLiveMessage>) {
        self.ai_chat.error = Some(message.clone());
        let applied = live.as_ref().map(|live| live.steps.len()).unwrap_or(0);
        let mut text = format!("请求失败：{message}");
        if applied > 0 {
            text.push_str(&format!(
                "\n\n失败前已写入 {applied} 项，可按 Ctrl+Z 撤销。"
            ));
        }
        let reasoning = live
            .as_ref()
            .and_then(|live| (!live.reasoning.trim().is_empty()).then(|| live.reasoning.clone()));
        let meta = live.as_ref().map(|live| AiMessageMeta {
            elapsed_seconds: live.started.elapsed().as_secs_f32(),
            model: live.model.clone(),
            steps: live.steps.clone(),
            ..Default::default()
        });
        self.ai_chat.messages.push(AiChatMessage {
            role: AiRole::Assistant,
            content: text,
            reasoning,
            meta,
            changes: None,
        });
    }

    /// 所属模块：app / AI 编曲 / 应用
    /// 功能：在工程副本上执行完整计划；全部成功后才替换工程并记录一次撤销（流式兜底）。
    /// 输入：`plan`：模型返回的编曲计划。
    /// 输出/副作用：替换 `self.project`，清空选择与底部编辑器，标记脏；返回结构化摘要。
    /// 复杂度：时间 O(计划大小 × 轨道数)，空间 O(工程副本)。
    ///
    /// Module: app / AI composition / apply
    /// Purpose: Apply a complete plan to a project copy; only replace the project after every
    /// operation succeeds and push one undo step (streaming fallback).
    /// Inputs: `plan`: composition plan returned by the model.
    /// Outputs / side effects: replaces `self.project`, clears selection and the bottom editor, marks
    /// the project dirty; returns the structured summary.
    /// Complexity: O(plan size × tracks) time and O(project copy) space.
    fn apply_ai_plan(&mut self, plan: AiPlan) -> Result<AiApplySummary, String> {
        let plugins = self.ai_plugin_catalog();
        let mut project = self.project.clone();
        let summary = plan.apply(&mut project, &plugins)?;
        self.push_undo_now();
        self.project = project;
        // 工程被整体替换：取消录音、关闭插件编辑器并清理失效选择。
        // The project was replaced wholesale: cancel recording, close plugin editors and clear stale
        // selections.
        self.after_history_apply();
        self.cleanup_after_ai_write();
        let text = summary.summary_text();
        self.status = if summary.plugins_added > 0 {
            format!("AI 编曲：{text}；正在加载 VST 插件…")
        } else {
            format!("AI 编曲：{text}")
        };
        Ok(summary)
    }

    /// 所属模块：app / AI 编曲 / 收尾
    /// 功能：写入后清理插件参数缓存、关闭底部编辑器并校正选择。
    /// 输入：`self`。
    /// 输出/副作用：清空缓存与选择状态；不修改工程内容。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / AI composition / finalise
    /// Purpose: Clear plugin-parameter caches, close the bottom editor and fix the selection after a
    /// write.
    /// Inputs: `self`.
    /// Outputs / side effects: clears caches and selection state; project content unchanged.
    /// Complexity: O(1) time and space.
    pub(super) fn cleanup_after_ai_write(&mut self) {
        // 插件槽位可能变化：参数缓存与内嵌失败记录必须失效，否则下一帧会读到旧插件参数。
        // Plugin slots may have changed: parameter caches and inline failures must be invalidated or
        // the next frame reads the old plugin's parameters.
        self.plugin_param_cache.clear();
        self.plugin_param_cache_at.clear();
        self.effect_editor_flags.clear();
        self.inline_failed.clear();
        self.bottom_open = false;
        self.validate_selection();
    }

    /// 所属模块：app / AI 编曲 / 取消
    /// 功能：停止等待当前请求；后台线程的结果随后被丢弃。
    /// 输入：`self`。
    /// 输出/副作用：清空接收通道与 `pending`，状态栏提示。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / AI composition / cancel
    /// Purpose: Stop waiting for the current request; a later worker result is discarded.
    /// Inputs: `self`.
    /// Outputs / side effects: drops the receiver and `pending`, updates the status bar.
    /// Complexity: O(1) time and space.
    pub fn cancel_ai_message(&mut self) {
        if self.ai_chat.pending.is_some() {
            self.ai_chat.receiver = None;
            self.ai_chat.pending = None;
            self.ai_chat.conflict_prompt = None;
            self.ai_chat.edit_guard_dismissed = false;
            if let Some(live) = self.ai_chat.live.take() {
                let applied = live.steps.len();
                let mut text = "已停止生成。".to_string();
                if applied > 0 {
                    text.push_str(&format!(
                        "已写入 {applied} 项：{}。\n可按 Ctrl+Z 撤销。",
                        live.summary.summary_text()
                    ));
                    self.cleanup_after_ai_write();
                }
                let changes =
                    (applied > 0 && summary_has_changes(&live.summary)).then_some(live.summary);
                let reasoning = (!live.reasoning.trim().is_empty()).then_some(live.reasoning);
                self.ai_chat.messages.push(AiChatMessage {
                    role: AiRole::Assistant,
                    content: text,
                    reasoning,
                    meta: Some(AiMessageMeta {
                        elapsed_seconds: live.started.elapsed().as_secs_f32(),
                        model: live.model.clone(),
                        steps: live.steps.clone(),
                        ..Default::default()
                    }),
                    changes,
                });
            }
            self.status = "已停止等待 AI 回复（后台请求可能仍在完成）".into();
        }
    }

    /// 所属模块：app / AI 编曲 / 后台任务
    /// 功能：停止当前项目内所有对话的 AI 任务（活动对话与已切走的后台对话）。
    /// 输入：`self`。
    /// 输出/副作用：丢弃接收器，并在每个对话末尾追加停止说明；不修改工程。
    /// 复杂度：时间 O(历史对话数 + 消息数)，空间 O(1)。
    ///
    /// Module: app / AI composition / background tasks
    /// Purpose: Stop every AI task in this project, including background conversations.
    /// Inputs: `self`.
    /// Outputs / side effects: drops receivers and appends a stop note to each conversation; no
    /// project change.
    /// Complexity: O(conversations + messages) time and O(1) space.
    pub fn cancel_all_ai_tasks(&mut self) {
        self.cancel_ai_message();
        for conversation in &mut self.ai_chat.conversations {
            if conversation.pending.is_none() {
                continue;
            }
            conversation.pending = None;
            conversation.receiver = None;
            conversation.error = None;
            let applied = conversation
                .live
                .as_ref()
                .map(|live| live.steps.len())
                .unwrap_or(0);
            let text = if applied > 0 {
                format!("已停止生成。已写入 {applied} 项，可按 Ctrl+Z 撤销。")
            } else {
                "已停止生成。".to_string()
            };
            conversation.live = None;
            conversation.messages.push(AiChatMessage {
                role: AiRole::Assistant,
                content: text,
                reasoning: None,
                meta: None,
                changes: None,
            });
        }
        self.ai_chat.conflict_prompt = None;
    }

    /// 所属模块：app / AI 编曲 / 轨道引用
    /// 功能：把一条轨道（乐器、效果器、控制器与片段摘要）复制为 AI 输入附件并打开面板。
    /// 输入：`track_index`：轨道下标。
    /// 输出/副作用：追加附件、打开 AI 面板并更新状态栏；不修改工程。
    /// 复杂度：时间 O(轨道音符数)，空间 O(附件文本)。
    ///
    /// Module: app / AI composition / track reference
    /// Purpose: Copy one track (instrument, effects, controllers and clip summary) into an AI
    /// attachment and open the panel.
    /// Inputs: `track_index`.
    /// Outputs / side effects: appends an attachment, opens the AI panel and updates the status bar;
    /// no project change.
    /// Complexity: O(track notes) time and O(attachment text) space.
    pub fn attach_track_to_ai(&mut self, track_index: usize) {
        let Some(track) = self.project.tracks.get(track_index) else {
            return;
        };
        let name = format!("轨道：{}", track.name);
        let text = ai_track_reference(track);
        self.ai_chat.attachments.push(AiAttachment { name, text });
        self.side_panel = Some(SidePanel::Ai);
        self.status = "已复制轨道到 AI 输入".into();
    }

    /// 所属模块：app / AI 编曲 / 片段引用
    /// 功能：把一个 MIDI 片段复制为 AI 输入附件并打开面板。
    /// 输入：`track_index` / `clip_index`：片段位置。
    /// 输出/副作用：追加附件、打开 AI 面板并更新状态栏；不修改工程。
    /// 复杂度：时间 O(片段音符数)，空间 O(附件文本)。
    ///
    /// Module: app / AI composition / clip reference
    /// Purpose: Copy one MIDI clip into an AI attachment and open the panel.
    /// Inputs: `track_index` / `clip_index`.
    /// Outputs / side effects: appends an attachment, opens the AI panel and updates the status bar;
    /// no project change.
    /// Complexity: O(clip notes) time and O(attachment text) space.
    pub fn attach_clip_to_ai(&mut self, track_index: usize, clip_index: usize) {
        let Some(track) = self.project.tracks.get(track_index) else {
            return;
        };
        let Some(clip) = track.clips.get(clip_index) else {
            return;
        };
        if clip.is_audio() {
            self.ai_chat.error = Some("音频片段暂不支持复制到 AI 输入".into());
            return;
        }
        let name = format!("片段：{} / {}", track.name, clip.name);
        let text = ai_clip_reference(track, clip);
        self.ai_chat.attachments.push(AiAttachment { name, text });
        self.side_panel = Some(SidePanel::Ai);
        self.status = "已复制 MIDI 片段到 AI 输入".into();
    }

    /// 所属模块：app / AI 编曲 / 片段引用
    /// 功能：把多选片段逐个复制为 AI 输入附件（最多 4 个）。
    /// 输入：`clips`：`(轨道下标, 片段下标)` 列表。
    /// 输出/副作用：追加附件、打开 AI 面板并更新状态栏；不修改工程。
    /// 复杂度：时间 O(选中片段音符数)，空间 O(附件文本)。
    ///
    /// Module: app / AI composition / clip reference
    /// Purpose: Copy several selected clips into AI attachments (at most four).
    /// Inputs: `clips`: `(track index, clip index)` pairs.
    /// Outputs / side effects: appends attachments, opens the AI panel and updates the status bar;
    /// no project change.
    /// Complexity: O(selected clip notes) time and O(attachment text) space.
    pub fn attach_clips_to_ai(&mut self, clips: &[(usize, usize)]) {
        let mut added = 0usize;
        for (track_index, clip_index) in clips.iter().take(4) {
            let Some(track) = self.project.tracks.get(*track_index) else {
                continue;
            };
            let Some(clip) = track.clips.get(*clip_index) else {
                continue;
            };
            if clip.is_audio() {
                continue;
            }
            let name = format!("片段：{} / {}", track.name, clip.name);
            let text = ai_clip_reference(track, clip);
            self.ai_chat.attachments.push(AiAttachment { name, text });
            added += 1;
        }
        if added > 0 {
            self.side_panel = Some(SidePanel::Ai);
            self.status = format!("已复制 {added} 个 MIDI 片段到 AI 输入");
        }
    }

    /// 所属模块：app / AI 编曲 / 剪贴板引用
    /// 功能：把应用内剪贴板中的整轨、片段或音符粘贴成 AI 输入附件。
    /// 输入：`self`。
    /// 输出/副作用：可能追加附件、打开 AI 面板并更新状态栏；不修改工程。
    /// 复杂度：时间 O(剪贴板音符数)，空间 O(附件文本)。
    ///
    /// Module: app / AI composition / clipboard reference
    /// Purpose: Paste a whole track, clips or notes from the in-app clipboard into AI attachments.
    /// Inputs: `self`.
    /// Outputs / side effects: may append attachments, open the AI panel and update the status bar;
    /// no project change.
    /// Complexity: O(clipboard notes) time and O(attachment text) space.
    pub fn attach_clipboard_to_ai(&mut self) -> bool {
        let mut generated: Vec<(String, String)> = Vec::new();
        match &self.clipboard {
            Clipboard::Track(track) => {
                generated.push((format!("轨道：{}", track.name), ai_track_reference(track)));
            }
            Clipboard::Clips(clips) => {
                for (track_index, clip) in clips.iter().take(4) {
                    let track = self.project.tracks.get(*track_index);
                    let name = track
                        .map(|track| format!("片段：{} / {}", track.name, clip.name))
                        .unwrap_or_else(|| format!("片段：{}", clip.name));
                    let text = track
                        .map(|track| ai_clip_reference(track, clip))
                        .unwrap_or_else(|| ai_clip_only_reference(clip));
                    generated.push((name, text));
                }
            }
            Clipboard::Notes(notes) => {
                if notes.is_empty() {
                    return false;
                }
                let mut text = String::from("音符引用（相对片段开头）：\n");
                text.push_str("音高:相对拍:时值:力度 = ");
                for note in notes.iter().take(AI_REFERENCE_NOTES) {
                    text.push_str(&ai_note_text(note));
                    text.push(' ');
                }
                generated.push(("音符".into(), text));
            }
            _ => return false,
        }
        if generated.is_empty() {
            return false;
        }
        let count = generated.len();
        for (name, text) in generated {
            self.ai_chat.attachments.push(AiAttachment { name, text });
        }
        self.side_panel = Some(SidePanel::Ai);
        self.status = format!("已从剪贴板添加 {count} 个 AI 引用");
        true
    }

    /// 所属模块：app / AI 编曲 / 附件
    /// 功能：在后台打开文件选择器并读取附件（文本 / Markdown / JSON / CSV / MIDI）。
    /// 输入：`self`。
    /// 输出/副作用：启动后台线程并保存接收通道；不修改工程。
    /// 复杂度：时间 O(1)，空间 O(1)（读取在后台）。
    ///
    /// Module: app / AI composition / attachments
    /// Purpose: Open a file picker on a worker and read the attachment (text / Markdown / JSON / CSV / MIDI).
    /// Inputs: `self`.
    /// Outputs / side effects: starts a worker and stores the receiver; no project change.
    /// Complexity: O(1) time and O(1) space (reading happens in the background).
    pub fn open_ai_attachment_dialog(&mut self) {
        if self.ai_attach_rx.is_some() {
            return;
        }
        let parent = self.dialog_parent();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let filters: &[(&str, &[&str])] = &[
                (
                    "附件（文本 / MIDI）",
                    &["txt", "md", "json", "csv", "mid", "midi"],
                ),
                ("所有文件", &["*"]),
            ];
            let request = crate::platform::FileRequest {
                title: "添加 AI 附件",
                filters,
                start_dir: None,
                default_name: None,
                parent,
            };
            let result = match crate::platform::import_file(&request) {
                Ok(Some(path)) => read_attachment(&path),
                Ok(None) => Err(String::new()),
                Err(error) => Err(error),
            };
            let _ = tx.send(result);
        });
        self.ai_attach_rx = Some(rx);
    }

    /// 所属模块：app / AI 编曲 / 附件
    /// 功能：非阻塞接收附件读取结果；取消时静默，失败时写入面板错误。
    /// 输入：`self`。
    /// 输出/副作用：追加附件、更新状态栏或错误；不修改工程。
    /// 复杂度：时间 O(附件文本长度)，空间 O(同)。
    ///
    /// Module: app / AI composition / attachments
    /// Purpose: Receive the attachment result without blocking; cancellation is silent and failures
    /// surface in the panel error.
    /// Inputs: `self`.
    /// Outputs / side effects: appends an attachment, updates the status bar or error; no project change.
    /// Complexity: O(attachment length) time and space.
    pub(super) fn poll_ai_attachment(&mut self) {
        let Some(receiver) = &self.ai_attach_rx else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(attachment)) => {
                self.ai_attach_rx = None;
                self.status = format!("已添加附件：{}", attachment.name);
                self.ai_chat.attachments.push(attachment);
            }
            Ok(Err(error)) => {
                self.ai_attach_rx = None;
                if !error.is_empty() {
                    self.ai_chat.error = Some(error);
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.ai_attach_rx = None;
            }
        }
    }
}

/// 所属模块：app / AI 编曲 / 后台任务
/// 功能：执行一次完整的 AI 任务：首轮补全 → 如模型请求检索则联网搜索 → 再次补全；最多 3 轮。
/// 输入：
/// - `request`：模型、密钥、消息与采样参数；
/// - `search_config`：联网搜索配置；
/// - `library`：可编辑资料库（按需加载技巧正文）。
/// 输出/副作用：最终模型内容、截断标记与是否检索过；阻塞调用，必须在后台线程执行。
/// 复杂度：时间 O(轮次 × 网络延迟)，空间 O(消息 + 搜索结果长度)。
///
/// Module: app / AI composition / worker
/// Purpose: Run one complete AI task: first completion → web research when requested → completion
/// again, at most three rounds.
/// Inputs:
/// - `request`: model, key, messages and sampling parameters;
/// - `search_config`: web-research configuration;
/// - `library`: editable library (loads requested technique bodies).
/// Outputs / side effects: final model content, truncation flag and whether research ran; blocking, so
/// it must run on a worker thread.
/// Complexity: O(rounds × network latency) time and O(messages + search results) space.
fn run_ai_worker(
    mut request: AiRequest,
    search_config: AiSearchConfig,
    library: AiLibrary,
    request_id: u64,
    events: Sender<AiWorkerEvent>,
) -> Result<AiWorkerOutcome, String> {
    let started = Instant::now();
    let runtime = AiRuntime::standard();
    let mut reasoning_parts: Vec<String> = Vec::new();
    let mut searches = 0usize;
    let mut loaded_skills: Vec<String> = Vec::new();
    for round in 0..3 {
        let mut stream = PlanStream::new();
        // 流式补全：思考与已解析的操作立即推送给 UI，UI 边接收边写入工程。
        // Streaming completion: reasoning and parsed operations go to the UI immediately, which
        // writes them into the project as they arrive.
        let response = runtime.stream(&request, &mut |event| match event {
            AiStreamEvent::Reasoning(text) => {
                let _ = events.send(AiWorkerEvent::Reasoning { request_id, text });
            }
            AiStreamEvent::Content(text) => {
                for operation in stream.push(&text) {
                    let _ = events.send(AiWorkerEvent::Operation {
                        request_id,
                        operation,
                    });
                }
                if let Some(reply) = stream.partial_reply() {
                    let _ = events.send(AiWorkerEvent::Content {
                        request_id,
                        text: reply,
                    });
                }
            }
        })?;
        // 后端若忽略流式回调（自定义实现），用完整内容补一次同样的流程。
        // If a backend ignored the streaming callback (a custom implementation), run the same flow
        // with the complete content.
        if stream.content().is_empty() && !response.content.is_empty() {
            for operation in stream.push(&response.content) {
                let _ = events.send(AiWorkerEvent::Operation {
                    request_id,
                    operation,
                });
            }
            if let Some(reply) = stream.partial_reply() {
                let _ = events.send(AiWorkerEvent::Content {
                    request_id,
                    text: reply,
                });
            }
        }
        if let Some(reasoning) = response
            .reasoning
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            reasoning_parts.push(reasoning.to_string());
        }
        let outcome = AiWorkerOutcome {
            content: response.content.clone(),
            reasoning: merge_reasoning(&reasoning_parts),
            truncated: response.truncated,
            used_search: searches > 0,
            searches,
            loaded_skills: loaded_skills.clone(),
            elapsed_seconds: started.elapsed().as_secs_f32(),
        };
        let Ok(plan) = AiPlan::parse(&response.content) else {
            return Ok(outcome);
        };
        let queries = plan
            .research
            .iter()
            .map(|query| query.trim().to_string())
            .filter(|query| !query.is_empty())
            .collect::<Vec<_>>();
        let requested_skills = plan
            .skills
            .iter()
            .map(|id| id.trim().to_lowercase())
            .filter(|id| !id.is_empty())
            .collect::<Vec<_>>();
        if queries.is_empty() && requested_skills.is_empty() {
            // 流式阶段未放行的尾部操作（字段顺序异常或事件缺失）在这里补齐。
            // Emit the tail of operations the stream did not release (unexpected field order or a
            // missing event).
            let released = stream.emitted();
            if released < plan.operations.len() {
                for operation in plan.operations[released..].iter().cloned() {
                    let _ = events.send(AiWorkerEvent::Operation {
                        request_id,
                        operation,
                    });
                }
            }
            return Ok(outcome);
        }
        if round == 2 {
            return Ok(outcome);
        }
        let new_skills: Vec<String> = requested_skills
            .iter()
            .filter(|id| !loaded_skills.contains(id))
            .cloned()
            .collect();
        let mut user_text = String::new();
        if !new_skills.is_empty() {
            let _ = events.send(AiWorkerEvent::Stage {
                request_id,
                label: "正在加载编曲技巧…".into(),
            });
            let (skill_text, newly_loaded) = library.load_techniques(&new_skills);
            if newly_loaded.is_empty() {
                user_text.push_str("请求的技巧未找到；请参考技巧索引或直接基于已有知识继续。\n");
            } else {
                user_text.push_str("已加载以下编曲技巧：\n");
                user_text.push_str(&skill_text);
                user_text.push('\n');
                loaded_skills.extend(newly_loaded);
            }
        } else if !requested_skills.is_empty() {
            user_text.push_str("请求的技巧已经加载，请直接使用；不要再次请求相同技巧。\n");
        }
        if !queries.is_empty() {
            if search_config.enabled {
                let _ = events.send(AiWorkerEvent::Stage {
                    request_id,
                    label: "正在联网检索资料…".into(),
                });
                match search::search(&search_config, &queries) {
                    Ok(results) => {
                        searches += queries.len().min(3);
                        user_text.push_str("以下是联网检索结果：\n");
                        user_text.push_str(&results);
                        user_text.push('\n');
                    }
                    Err(error) => {
                        user_text.push_str(&format!("联网检索失败：{error}\n"));
                    }
                }
            } else {
                user_text.push_str(
                    "联网检索不可用（未启用或未配置 DeepSeek API Key）。请基于已有知识继续。\n",
                );
            }
        }
        user_text.push_str(
            "请基于以上资料给出最终 JSON 编曲计划；`research` 与 `skills` 必须是空数组，不要再请求检索或技巧。",
        );
        let _ = events.send(AiWorkerEvent::Stage {
            request_id,
            label: "正在生成最终编曲…".into(),
        });
        request.messages.push(AiMessage {
            role: AiRole::Assistant,
            content: response.content,
        });
        request.messages.push(AiMessage {
            role: AiRole::User,
            content: user_text,
        });
    }
    Err("AI 研究轮次过多".into())
}

/// 所属模块：app / AI 编曲 / 摘要
/// 功能：判断写入摘要是否包含真实工程改动。
/// 输入：`summary`：累计摘要。
/// 输出/副作用：是否有改动；不修改输入。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / AI composition / summary
/// Purpose: Whether a write summary contains real project changes.
/// Inputs: `summary`: accumulated summary.
/// Outputs / side effects: whether anything changed; the input is unchanged.
/// Complexity: O(1) time and space.
fn summary_has_changes(summary: &AiApplySummary) -> bool {
    summary.tracks_added > 0
        || summary.tracks_updated > 0
        || summary.clips_added > 0
        || summary.notes_added > 0
        || summary.effects_added > 0
        || summary.plugins_added > 0
        || summary.params_changed > 0
        || summary.controllers_added > 0
        || summary.cleared
        || summary.tempo.is_some()
        || summary.time_signature.is_some()
}

/// 所属模块：app / AI 编曲 / 思考
/// 功能：合并多轮思考过程，限制总长度，避免 UI 与上下文被超长推理占满。
/// 输入：`parts`：各轮思考文本。
/// 输出/副作用：合并后的文本或 `None`；不修改输入。
/// 复杂度：时间 O(总长度)，空间 O(总长度)。
///
/// Module: app / AI composition / reasoning
/// Purpose: Merge reasoning traces from several rounds and cap their total length.
/// Inputs: `parts`: reasoning text of each round.
/// Outputs / side effects: merged text or `None`; the input is unchanged.
/// Complexity: O(total length) time and space.
fn merge_reasoning(parts: &[String]) -> Option<String> {
    const MAX_CHARS: usize = 16_000;
    if parts.is_empty() {
        return None;
    }
    let mut text = parts.join("\n\n");
    if text.chars().count() > MAX_CHARS {
        text = text.chars().take(MAX_CHARS).collect();
        text.push_str("\n…（思考过程过长，已截断）");
    }
    Some(text)
}

/// 复制轨道/片段为 AI 附件时最多包含的音符数。
/// Maximum notes included when copying a track/clip into an AI attachment.
const AI_REFERENCE_NOTES: usize = 256;

/// 所属模块：app / AI 编曲 / 轨道引用
/// 功能：生成一条轨道的文本快照：乐器、音量/声像、效果器、控制器与片段音符。
/// 输入：`track`：轨道。
/// 输出/副作用：附件文本；不修改轨道。
/// 复杂度：时间 O(轨道音符数)，空间 O(文本长度)。
///
/// Module: app / AI composition / track reference
/// Purpose: Build a text snapshot of one track: instrument, volume/pan, effects, controllers and
/// clip notes.
/// Inputs: `track`.
/// Outputs / side effects: attachment text; the track is unchanged.
/// Complexity: O(track notes) time and O(text length) space.
fn ai_track_reference(track: &Track) -> String {
    let mut text = format!(
        "轨道引用：{}（id={}）\n乐器：{}\n音量={:.2} 声像={:.2}{}{}\n",
        track.name,
        track.id,
        track.instrument.label(),
        track.volume,
        track.pan,
        if track.muted { " 静音" } else { "" },
        if track.solo { " 独奏" } else { "" }
    );
    if !track.effects.is_empty() {
        text.push_str("效果器链：\n");
        for (index, effect) in track.effects.iter().enumerate() {
            text.push_str(&format!(
                "  [{index}] {}（{}）\n",
                effect.label(),
                if effect.enabled() { "启用" } else { "旁路" }
            ));
        }
    }
    let mut controller_lines = Vec::new();
    for kind in ControllerKind::ALL {
        let lane = track.controllers.get(kind);
        if lane.is_empty() {
            continue;
        }
        let points = lane
            .iter()
            .take(32)
            .map(|point| format!("{:.2}={:.3}", point.beat, point.value))
            .collect::<Vec<_>>()
            .join(" ");
        controller_lines.push(format!("{}（{} 点）：{points}", kind.label(), lane.len()));
    }
    if !controller_lines.is_empty() {
        text.push_str("控制器子轨：\n");
        for line in controller_lines {
            text.push_str(&format!("  {line}\n"));
        }
    }
    if track.clips.is_empty() {
        text.push_str("片段：无\n");
        return text;
    }
    text.push_str("片段：\n");
    let mut written = 0usize;
    for (index, clip) in track.clips.iter().enumerate() {
        text.push_str(&format!(
            "  [{}]「{}」{:.2}~{:.2} 拍\n",
            index,
            clip.name,
            clip.start,
            clip.end()
        ));
        if clip.is_audio() {
            text.push_str("    音频片段（无 MIDI 音符）\n");
            continue;
        }
        if written >= AI_REFERENCE_NOTES {
            text.push_str("    …（已达音符上限）\n");
            continue;
        }
        text.push_str("    音高:相对拍:时值:力度 = ");
        for note in &clip.notes {
            if written >= AI_REFERENCE_NOTES {
                text.push_str("…");
                break;
            }
            text.push_str(&ai_note_text(note));
            text.push(' ');
            written += 1;
        }
        text.push('\n');
    }
    text
}

/// 所属模块：app / AI 编曲 / 片段引用
/// 功能：生成一个 MIDI 片段的文本快照（所属轨道、音色与音符）。
/// 输入：`track` 所属轨道；`clip` 片段。
/// 输出/副作用：附件文本；不修改输入。
/// 复杂度：时间 O(片段音符数)，空间 O(文本长度)。
///
/// Module: app / AI composition / clip reference
/// Purpose: Build a text snapshot of one MIDI clip (owner track, timbre and notes).
/// Inputs: `track`; `clip`.
/// Outputs / side effects: attachment text; the inputs are unchanged.
/// Complexity: O(clip notes) time and O(text length) space.
fn ai_clip_reference(track: &Track, clip: &Clip) -> String {
    let mut text = format!(
        "片段引用：{} / {}（id={}）\n所属轨道：{}（id={}）\n乐器：{}\n范围：{:.2}~{:.2} 拍（长度 {:.2} 拍）\n音符：\n",
        track.name,
        clip.name,
        clip.id,
        track.name,
        track.id,
        track.instrument.label(),
        clip.start,
        clip.end(),
        clip.length
    );
    if clip.notes.is_empty() {
        text.push_str("  （空片段）\n");
        return text;
    }
    text.push_str("  音高:相对拍:时值:力度 = ");
    for note in clip.notes.iter().take(AI_REFERENCE_NOTES) {
        text.push_str(&ai_note_text(note));
        text.push(' ');
    }
    if clip.notes.len() > AI_REFERENCE_NOTES {
        text.push_str("…（其余省略）");
    }
    text.push('\n');
    text
}

/// 所属模块：app / AI 编曲 / 片段引用
/// 功能：在没有所属轨道时生成一个片段的文本快照。
/// 输入：`clip`：片段。
/// 输出/副作用：附件文本；不修改片段。
/// 复杂度：时间 O(片段音符数)，空间 O(文本长度)。
///
/// Module: app / AI composition / clip reference
/// Purpose: Build a clip-only snapshot when the owner track is unavailable.
/// Inputs: `clip`.
/// Outputs / side effects: attachment text; the clip is unchanged.
/// Complexity: O(clip notes) time and O(text length) space.
fn ai_clip_only_reference(clip: &Clip) -> String {
    let mut text = format!(
        "片段引用：{}（id={}）\n范围：{:.2}~{:.2} 拍（长度 {:.2} 拍）\n音符：\n",
        clip.name,
        clip.id,
        clip.start,
        clip.end(),
        clip.length
    );
    text.push_str("  音高:相对拍:时值:力度 = ");
    for note in clip.notes.iter().take(AI_REFERENCE_NOTES) {
        text.push_str(&ai_note_text(note));
        text.push(' ');
    }
    text.push('\n');
    text
}

/// 所属模块：app / AI 编曲 / 片段引用
/// 功能：把音符格式化为紧凑文本。
/// 输入：`note`：音符。
/// 输出/副作用：`C4:0.00:1.00:90` 形式文本；不修改音符。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / AI composition / clip reference
/// Purpose: Format one note into compact text.
/// Inputs: `note`.
/// Outputs / side effects: text in `C4:0.00:1.00:90` form; the note is unchanged.
/// Complexity: O(1) time and space.
fn ai_note_text(note: &Note) -> String {
    format!(
        "{}:{:.2}:{:.2}:{}",
        crate::ai::plan::pitch_name(note.pitch),
        note.start,
        note.length,
        note.velocity
    )
}

/// 所属模块：app / AI 编曲 / 附件读取
/// 功能：读取附件文件：MIDI 解析为音符文本，其它按 UTF-8 文本读取并限制大小。
/// 输入：`path`：用户选择的文件路径（安卓为沙箱副本）。
/// 输出/副作用：附件文本或中文错误；不修改文件。
/// 复杂度：时间 O(文件大小)，空间 O(文件大小)。
///
/// Module: app / AI composition / attachment reading
/// Purpose: Read an attachment: MIDI becomes note text; everything else is read as UTF-8 text with a
/// size cap.
/// Inputs: `path`: user-picked file path (a sandbox copy on Android).
/// Outputs / side effects: attachment text or a Chinese error; the file is unchanged.
/// Complexity: O(file size) time and space.
fn read_attachment(path: &std::path::Path) -> Result<AiAttachment, String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "附件".into());
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let is_midi = matches!(extension.as_str(), "mid" | "midi");
    let limit = if is_midi {
        AI_MIDI_ATTACHMENT_LIMIT
    } else {
        AI_TEXT_ATTACHMENT_LIMIT
    };
    let metadata = std::fs::metadata(path).map_err(|error| format!("读取附件信息失败：{error}"))?;
    if metadata.len() as usize > limit {
        return Err(format!("附件「{name}」过大（上限 {} KB）", limit / 1024));
    }
    let bytes = std::fs::read(path).map_err(|error| format!("读取附件失败：{error}"))?;
    let text = if is_midi {
        midi_file::attachment_text(&name, &bytes)?
    } else {
        String::from_utf8_lossy(&bytes).to_string()
    };
    Ok(AiAttachment { name, text })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖：清空对话会重置消息、附件与错误。
    /// Covers: clearing the chat resets messages, attachments and error.
    #[test]
    fn clearing_chat_resets_state() {
        let mut state = AiChatState::default();
        state.messages.push(AiChatMessage {
            role: AiRole::User,
            content: "你好".into(),
            reasoning: None,
            meta: None,
            changes: None,
        });
        state.error = Some("错误".into());
        state.attachments.push(AiAttachment {
            name: "a.txt".into(),
            text: "x".into(),
        });
        state.clear();
        assert!(state.messages.is_empty());
        assert!(state.attachments.is_empty());
        assert!(state.error.is_none());
        assert_eq!(state.conversations.len(), 1);
    }

    /// 覆盖：新任务自动登记为历史，切换历史对话后消息恢复且当前任务正确高亮。
    /// Covers: a new task is registered automatically and switching restores its messages and active id.
    #[test]
    fn conversation_history_switches_and_highlights() {
        let mut state = AiChatState::default();
        state.messages.push(AiChatMessage {
            role: AiRole::User,
            content: "第一段旋律".into(),
            reasoning: None,
            meta: None,
            changes: None,
        });
        state.ensure_active_conversation();
        let first_id = state.active_conversation().expect("first active");
        assert_eq!(state.conversations.len(), 1);
        assert_eq!(state.conversations[0].title, "第一段旋律");

        state.clear();
        assert_eq!(state.active_conversation(), None);
        state.messages.push(AiChatMessage {
            role: AiRole::User,
            content: "第二段旋律".into(),
            reasoning: None,
            meta: None,
            changes: None,
        });
        state.ensure_active_conversation();
        assert_eq!(state.conversations.len(), 2);
        assert_ne!(state.active_conversation(), Some(first_id));

        state.open_conversation(first_id);
        assert_eq!(state.active_conversation(), Some(first_id));
        assert_eq!(state.messages[0].content, "第一段旋律");
    }

    /// 覆盖：已切走对话的后台任务也会被视为运行中，并锁住项目编辑。
    /// Covers: a background task in another conversation counts as running and locks project editing.
    #[test]
    fn background_task_locks_editing() {
        let mut state = AiChatState::default();
        state.conversations.push(AiConversation {
            id: 9,
            title: "后台任务".into(),
            messages: Vec::new(),
            updated_at: 0.0,
            pending: Some(AiPending {
                request_id: 7,
                started: std::time::Instant::now(),
            }),
            live: None,
            receiver: None,
            error: None,
        });
        assert!(state.any_task_running());
        assert!(state.project_editing_locked());
        state.edit_guard_dismissed = true;
        assert!(state.any_task_running());
        assert!(!state.project_editing_locked());
    }
}
