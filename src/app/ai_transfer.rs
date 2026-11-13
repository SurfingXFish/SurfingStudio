//! 所属模块：app / AI 资料导入导出
//! 文件用途：
//! - 在后台线程打开原生文件对话框，导入或导出智能体、基础技能与编曲技巧。
//! - 导入结果回到 UI 线程后再写入资料库并保存，避免后台线程直接修改应用状态。
//! - 单个条目导出为 `.json` / `.md`；“导出全部”按分类写入所选目录。
//!
//! 函数清单：
//! - `SurfingApp::ai_transfer_active`：是否有导入/导出任务在运行。
//! - `SurfingApp::start_ai_import`：选择并导入一个文件。
//! - `SurfingApp::start_ai_export_entry`：导出单条资料。
//! - `SurfingApp::start_ai_export_all`：导出某一类的全部资料。
//! - `SurfingApp::spawn_ai_transfer`：启动后台任务并登记接收通道。
//! - `SurfingApp::poll_ai_transfer`：轮询后台结果并应用。
//! - `SurfingApp::apply_ai_transfer`：把导入条目写入资料库或展示结果。
//! - `ai_kind_id` / `ai_kind_label` / `export_file_name`：类型与文件名辅助函数。
//!
//! Module: app / AI library import and export
//! Purpose:
//! - Open native file dialogs on a worker thread to import or export agents, theory skills and
//!   techniques.
//! - Apply the import on the UI thread only after the worker reports back, so background threads never
//!   mutate app state.
//! - Export one entry as `.json` / `.md`; "export all" writes the category tree into a chosen folder.
//!
//! Functions:
//! - `SurfingApp::ai_transfer_active`: whether an import/export task is running.
//! - `SurfingApp::start_ai_import`: pick and import one file.
//! - `SurfingApp::start_ai_export_entry`: export one entry.
//! - `SurfingApp::start_ai_export_all`: export every entry of one kind.
//! - `SurfingApp::spawn_ai_transfer`: start the worker and register the channel.
//! - `SurfingApp::poll_ai_transfer`: poll the worker result and apply it.
//! - `SurfingApp::apply_ai_transfer`: insert the imported entry or show the result.
//! - `ai_kind_id` / `ai_kind_label` / `export_file_name`: kind and file-name helpers.

use super::*;
use crate::ai::{AiAgentProfile, AiSkillDoc, make_id, unique_id};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

/// 智能体导入过滤。
/// Agent import filters.
const AGENT_IMPORT_FILTER: &[(&str, &[&str])] = &[("智能体资料", &["json"])];
/// 技能导入过滤。
/// Skill import filters.
const SKILL_IMPORT_FILTER: &[(&str, &[&str])] = &[("技能文档", &["md", "markdown"])];
/// 智能体导出过滤。
/// Agent export filters.
const AGENT_EXPORT_FILTER: &[(&str, &[&str])] = &[("智能体资料", &["json"])];
/// 技能导出过滤。
/// Skill export filters.
const SKILL_EXPORT_FILTER: &[(&str, &[&str])] = &[("技能文档", &["md"])];

/// 后台导入/导出的结果。
/// Result of a background import/export.
pub(super) enum AiTransferOutcome {
    /// 已解析出一个条目：`kind` 为目标分类，`entry` 为 JSON 文本。
    /// One parsed entry: `kind` is the target category and `entry` is JSON text.
    Imported { kind: String, entry: String },
    /// 操作完成但不需要修改资料库。
    /// The operation finished without changing the library.
    Done(String),
    /// 操作失败。
    /// The operation failed.
    Failed(String),
}

/// 一次后台导入/导出任务。
/// One background import/export task.
pub(super) struct AiTransferTask {
    receiver: Receiver<AiTransferOutcome>,
}

impl SurfingApp {
    /// 所属模块：app / AI 资料导入导出
    /// 功能：返回是否有导入/导出任务在运行。
    /// 输入：`self`。
    /// 输出/副作用：布尔值；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / AI library import and export
    /// Purpose: Whether an import/export task is running.
    /// Inputs: `self`.
    /// Outputs / side effects: a boolean; no state change.
    /// Complexity: O(1) time and space.
    pub(super) fn ai_transfer_active(&self) -> bool {
        self.ai_transfer.is_some()
    }

    /// 所属模块：app / AI 资料导入导出
    /// 功能：在后台选择并导入一个智能体或技能文件。
    /// 输入：`kind` 当前页面类型（决定过滤与默认分类）。
    /// 输出/副作用：启动后台任务并登记通道；不直接修改资料库。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / AI library import and export
    /// Purpose: Pick and import one agent or skill file on a worker thread.
    /// Inputs: `kind`: current page kind (drives filters and the default category).
    /// Outputs / side effects: starts a worker and registers the channel; the library is unchanged.
    /// Complexity: O(1) time and space.
    pub(super) fn start_ai_import(&mut self, kind: AiLibraryKind) {
        if !crate::platform::caps().native_file_dialog {
            self.agent_library.notice = Some("当前平台不支持文件导入".into());
            return;
        }
        let kind_id = ai_kind_id(kind).to_string();
        let hint = kind_id.clone();
        let filters = match kind {
            AiLibraryKind::Agent => AGENT_IMPORT_FILTER,
            _ => SKILL_IMPORT_FILTER,
        };
        let title = format!("导入{}", ai_kind_label(kind));
        self.spawn_ai_transfer("请在对话框中选择导入文件…", move || {
            let request = crate::platform::FileRequest {
                title: &title,
                filters,
                start_dir: None,
                default_name: None,
                parent: None,
            };
            let Some(path) = crate::platform::pick_file(&request) else {
                return AiTransferOutcome::Done("已取消导入".into());
            };
            match super::ai_data::import_file(&path, &hint) {
                Ok((kind, entry)) => AiTransferOutcome::Imported { kind, entry },
                Err(error) => AiTransferOutcome::Failed(error),
            }
        });
        self.agent_library.notice = Some(format!("正在导入{}…", ai_kind_label(kind)));
    }

    /// 所属模块：app / AI 资料导入导出
    /// 功能：在后台把一条资料导出到用户选择的位置。
    /// 输入：`kind` 类型；`id` 条目 id；`title` 条目名称（用于默认文件名）。
    /// 输出/副作用：启动后台任务；不修改资料库。
    /// 复杂度：时间 O(条目大小)，空间 O(资料库大小)。
    ///
    /// Module: app / AI library import and export
    /// Purpose: Export one entry to a user-chosen location on a worker thread.
    /// Inputs: `kind`; `id`; `title` (used for the default file name).
    /// Outputs / side effects: starts a worker; the library is unchanged.
    /// Complexity: O(entry size) time and O(library size) space.
    pub(super) fn start_ai_export_entry(&mut self, kind: AiLibraryKind, id: &str, title: &str) {
        if !crate::platform::caps().native_file_dialog {
            self.agent_library.notice = Some("当前平台不支持文件导出".into());
            return;
        }
        let kind_id = ai_kind_id(kind).to_string();
        let id = id.to_string();
        let library = self.settings.ai.library.clone();
        let filters = if kind == AiLibraryKind::Agent {
            AGENT_EXPORT_FILTER
        } else {
            SKILL_EXPORT_FILTER
        };
        let default_name = export_file_name(title, &id, kind);
        let dialog_title = format!("导出{}", ai_kind_label(kind));
        self.spawn_ai_transfer("请选择导出位置…", move || {
            let request = crate::platform::FileRequest {
                title: &dialog_title,
                filters,
                start_dir: None,
                default_name: Some(&default_name),
                parent: None,
            };
            let Some(target) = crate::platform::save_file(&request) else {
                return AiTransferOutcome::Done("已取消导出".into());
            };
            match super::ai_data::export_entry(&library, &kind_id, &id, &target) {
                Ok(()) => AiTransferOutcome::Done(format!("已导出到 {}", target.display())),
                Err(error) => AiTransferOutcome::Failed(error),
            }
        });
        self.agent_library.notice = Some(format!("正在导出{}…", ai_kind_label(kind)));
    }

    /// 所属模块：app / AI 资料导入导出
    /// 功能：在后台把某一类的全部资料导出到所选目录。
    /// 输入：`kind` 类型。
    /// 输出/副作用：启动后台任务；不修改资料库。
    /// 复杂度：时间 O(条目数 × 条目大小)，空间 O(资料库大小)。
    ///
    /// Module: app / AI library import and export
    /// Purpose: Export every entry of one kind into a chosen folder on a worker thread.
    /// Inputs: `kind`.
    /// Outputs / side effects: starts a worker; the library is unchanged.
    /// Complexity: O(entries × entry size) time and O(library size) space.
    pub(super) fn start_ai_export_all(&mut self, kind: AiLibraryKind) {
        if !crate::platform::caps().native_file_dialog {
            self.agent_library.notice = Some("当前平台不支持文件导出".into());
            return;
        }
        let kind_id = ai_kind_id(kind).to_string();
        let library = self.settings.ai.library.clone();
        let title = format!("选择导出{}的目录", ai_kind_label(kind));
        self.spawn_ai_transfer("请选择导出目录…", move || {
            let request = crate::platform::FileRequest::titled(&title);
            let Some(directory) = crate::platform::pick_folder(&request) else {
                return AiTransferOutcome::Done("已取消导出".into());
            };
            match super::ai_data::export_all(&library, &kind_id, &directory) {
                Ok(count) => {
                    AiTransferOutcome::Done(format!("已导出 {count} 条到 {}", directory.display()))
                }
                Err(error) => AiTransferOutcome::Failed(error),
            }
        });
        self.agent_library.notice = Some(format!("正在导出全部{}…", ai_kind_label(kind)));
    }

    /// 所属模块：app / AI 资料导入导出
    /// 功能：启动后台任务并登记接收通道；已有任务时直接提示。
    /// 输入：`progress` 进度提示；`work` 后台工作闭包。
    /// 输出/副作用：设置 `ai_transfer` 与提示；创建线程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / AI library import and export
    /// Purpose: Start a worker and register its channel; report when one is already running.
    /// Inputs: `progress`; `work`.
    /// Outputs / side effects: sets `ai_transfer` and the notice; spawns a thread.
    /// Complexity: O(1) time and space.
    fn spawn_ai_transfer(
        &mut self,
        progress: &str,
        work: impl FnOnce() -> AiTransferOutcome + Send + 'static,
    ) {
        if self.ai_transfer.is_some() {
            self.agent_library.notice = Some("已有导入/导出任务在进行中".into());
            return;
        }
        let (sender, receiver) = channel();
        self.ai_transfer = Some(AiTransferTask { receiver });
        self.agent_library.notice = Some(progress.to_string());
        std::thread::spawn(move || {
            let outcome = work();
            let _ = sender.send(outcome);
        });
    }

    /// 所属模块：app / AI 资料导入导出
    /// 功能：非阻塞轮询后台任务结果；完成后清理通道。
    /// 输入：`self`。
    /// 输出/副作用：可能修改资料库、提示与状态栏；不阻塞。
    /// 复杂度：时间 O(1)（应用导入时 O(条目数)），空间 O(1)。
    ///
    /// Module: app / AI library import and export
    /// Purpose: Non-blocking poll of the worker result and channel cleanup.
    /// Inputs: `self`.
    /// Outputs / side effects: may update the library, notice and status bar; never blocks.
    /// Complexity: O(1) time (O(entries) when applying an import) and O(1) space.
    pub(super) fn poll_ai_transfer(&mut self) {
        let Some(task) = self.ai_transfer.as_ref() else {
            return;
        };
        match task.receiver.try_recv() {
            Ok(outcome) => {
                self.ai_transfer = None;
                self.apply_ai_transfer(outcome);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.ai_transfer = None;
                self.agent_library.notice = Some("导入/导出任务意外结束".into());
            }
        }
    }

    /// 所属模块：app / AI 资料导入导出
    /// 功能：把后台结果写入资料库或展示提示。
    /// 输入：`outcome` 后台结果。
    /// 输出/副作用：导入时修改资料库、选中项并写盘；其他结果只更新提示。
    /// 复杂度：时间 O(条目数)，空间 O(条目数)。
    ///
    /// Module: app / AI library import and export
    /// Purpose: Apply a worker result to the library or show its message.
    /// Inputs: `outcome`.
    /// Outputs / side effects: an import mutates the library, selection and writes files; other
    /// results only update the notice.
    /// Complexity: O(entries) time and space.
    fn apply_ai_transfer(&mut self, outcome: AiTransferOutcome) {
        match outcome {
            AiTransferOutcome::Imported { kind, entry } => {
                let mut library = self.settings.ai.library.clone();
                let imported: Result<(AiLibraryKind, String), String> = match kind.as_str() {
                    "agent" => serde_json::from_str::<AiAgentProfile>(&entry)
                        .map_err(|error| format!("智能体 JSON 无效：{error}"))
                        .map(|mut agent| {
                            let base = if agent.id.trim().is_empty() {
                                make_id(&agent.name, "agent")
                            } else {
                                agent.id.clone()
                            };
                            agent.id = unique_id(&base, |candidate| {
                                library
                                    .agents
                                    .iter()
                                    .any(|existing| existing.id == candidate)
                            });
                            let id = agent.id.clone();
                            library.agents.push(agent);
                            (AiLibraryKind::Agent, id)
                        }),
                    "basic" | "technique" => serde_json::from_str::<AiSkillDoc>(&entry)
                        .map_err(|error| format!("技能 JSON 无效：{error}"))
                        .map(|mut skill| {
                            let base = if skill.id.trim().is_empty() {
                                make_id(&skill.title, kind.as_str())
                            } else {
                                skill.id.clone()
                            };
                            let id = unique_id(&base, |candidate| {
                                library
                                    .basics
                                    .iter()
                                    .chain(library.techniques.iter())
                                    .any(|existing| existing.id == candidate)
                            });
                            skill.id = id.clone();
                            if kind == "technique" {
                                library.techniques.push(skill);
                                (AiLibraryKind::Technique, id)
                            } else {
                                library.basics.push(skill);
                                (AiLibraryKind::Basic, id)
                            }
                        }),
                    other => Err(format!("不支持的资料类型：{other}")),
                };
                match imported {
                    Ok((imported_kind, id)) => {
                        library.normalise();
                        self.settings.ai.library = library;
                        self.commit_settings();
                        self.agent_library.selected = Some(id);
                        self.agent_library.draft = None;
                        let message = format!("已导入到{}", ai_kind_label(imported_kind));
                        self.agent_library.notice = Some(message.clone());
                        self.status = message;
                    }
                    Err(error) => {
                        let message = format!("导入失败：{error}");
                        self.agent_library.notice = Some(message.clone());
                        self.status = message;
                    }
                }
            }
            AiTransferOutcome::Done(message) => {
                self.agent_library.notice = Some(message.clone());
                self.status = message;
            }
            AiTransferOutcome::Failed(error) => {
                let message = format!("导入/导出失败：{error}");
                self.agent_library.notice = Some(message.clone());
                self.status = message;
            }
        }
    }
}

/// 所属模块：app / AI 资料导入导出
/// 功能：返回条目类型对应的持久化 id 片段。
/// 输入：`kind` 条目类型。
/// 输出/副作用：静态字符串；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / AI library import and export
/// Purpose: Return the persisted id fragment of an entry kind.
/// Inputs: `kind`.
/// Outputs / side effects: static text; no state change.
/// Complexity: O(1) time and space.
fn ai_kind_id(kind: AiLibraryKind) -> &'static str {
    match kind {
        AiLibraryKind::Agent => "agent",
        AiLibraryKind::Basic => "basic",
        AiLibraryKind::Technique => "technique",
        AiLibraryKind::Instrument => "instrument",
    }
}

/// 所属模块：app / AI 资料导入导出
/// 功能：返回条目类型的中文名称。
/// 输入：`kind` 条目类型。
/// 输出/副作用：静态文本；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / AI library import and export
/// Purpose: Return the Chinese label of an entry kind.
/// Inputs: `kind`.
/// Outputs / side effects: static text; no state change.
/// Complexity: O(1) time and space.
fn ai_kind_label(kind: AiLibraryKind) -> &'static str {
    match kind {
        AiLibraryKind::Agent => "智能体",
        AiLibraryKind::Basic => "基础技能",
        AiLibraryKind::Technique => "编曲技巧",
        AiLibraryKind::Instrument => "乐器资料",
    }
}

/// 所属模块：app / AI 资料导入导出
/// 功能：由标题生成安全的默认导出文件名。
/// 输入：`title` 标题；`id` 条目 id；`kind` 类型。
/// 输出/副作用：文件名；标题为空或全为非法字符时退回 id。
/// 复杂度：时间 O(标题长度)，空间 O(标题长度)。
///
/// Module: app / AI library import and export
/// Purpose: Build a safe default export file name from the title.
/// Inputs: `title`; `id`; `kind`.
/// Outputs / side effects: file name; empty or fully invalid titles fall back to the id.
/// Complexity: O(title length) time and space.
fn export_file_name(title: &str, id: &str, kind: AiLibraryKind) -> String {
    let cleaned: String = title
        .chars()
        .filter(|character| {
            !matches!(
                character,
                '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
        })
        .collect();
    let base = cleaned.trim();
    let base = if base.is_empty() { id } else { base };
    let extension = if kind == AiLibraryKind::Agent {
        "json"
    } else {
        "md"
    };
    format!("{base}.{extension}")
}
