//! 所属模块：app / 多窗口外壳
//! 文件用途：
//! - 实现 eframe 根应用 `WindowHost`：根视口始终可见，显示主页或某一个工程；其余工程各自一个视口窗口。
//! - 消费各窗口产生的 `WindowAction`：显示主页、打开/聚焦工程窗口、回到最近工程。
//! - 统一维护窗口标题（Windows 下用于定位原生句柄）、焦点与插件心跳。
//! - 在关闭根窗口与 `on_exit` 时写诊断日志，并确定性地终止进程。
//!
//! 函数清单：
//! - `WindowHost::new`：初始化主题、共享服务与主页窗口。
//! - `WindowHost::ui`：eframe 每帧入口：关闭处理 → 根窗口 → 窗口动作 → 工程视口 → 收尾。
//! - `WindowHost::on_exit`：eframe 退出回调：在析构链运行前终止进程，避免插件卸载死锁。
//! - `WindowHost::apply_action` / `open_project` / `open_new_project` / `spawn_project` / `resume_project`：窗口动作与工程窗口管理。
//! - `WindowHost::show_home` / `hide_home` / `close_root_project` / `promote_project_to_root`：主页与根工程切换。
//! - `WindowHost::draw_home_viewport` / `draw_projects`：主页与工程视口绘制。
//! - `WindowHost::update_window_titles`：为根窗口与每个工程窗口生成唯一标题。
//! - `path_key`：路径小写去重键。
//!
//! Module: app / multi-window shell
//! Purpose:
//! - Implement the eframe root app `WindowHost`: the root viewport is always visible and shows the
//!   home page or one project; other projects get their own viewport windows.
//! - Consume `WindowAction`s from the windows: show home, open/focus a project, resume the last one.
//! - Maintain window titles (used to resolve native handles on Windows), focus and plugin heartbeat.
//!
//! Functions:
//! - `WindowHost::new`: initialize the theme, shared services and the home window.
//! - `WindowHost::ui`: eframe entry: close handling → root window → actions → project viewports → shutdown details.
//! - `WindowHost::on_exit`: eframe exit callback: terminate the process before the teardown chain to
//!   avoid plugin-unload deadlocks.
//! - `WindowHost::apply_action` / `open_project` / `open_new_project` / `spawn_project` / `resume_project`: window actions and project-window management.
//! - `WindowHost::show_home` / `hide_home` / `close_root_project` / `promote_project_to_root`: home/root switching.
//! - `WindowHost::draw_home_viewport` / `draw_projects`: paint the home and project viewports.
//! - `WindowHost::update_window_titles`: unique titles for the root and every project window.
//! - `path_key`: lower-case path dedup key.

use super::shared::SharedServices;
use super::*;
use crate::diag;

/// 工程窗口默认尺寸（逻辑点）。
/// Default size of a project window in logical points.
const PROJECT_WINDOW_SIZE: [f32; 2] = [1440.0, 900.0];
/// 工程窗口最小尺寸（逻辑点）。
/// Minimum size of a project window in logical points.
const PROJECT_MIN_SIZE: [f32; 2] = [1100.0, 700.0];
/// 根窗口（主页）标题。
/// Title of the root (home) window.
const HOME_WINDOW_TITLE: &str = "SurfingStudio";

/// 一个非根视口的工程窗口。
/// A project window that is not the root viewport.
struct ProjectWindow {
    app: SurfingApp,
    viewport_id: egui::ViewportId,
    /// 正在加载的工程路径（小写）；用于避免同一文件被打开两次。
    /// Path being loaded (lower case), so one file is never opened twice.
    pending_path: Option<String>,
    /// 本帧是否收到关闭请求。
    /// Whether a close request arrived this frame.
    close_requested: bool,
    /// 下一次绘制前是否需要置前。
    /// Whether the viewport must be raised before the next paint.
    focus: bool,
}

/// 多窗口外壳。
/// Multi-window shell.
///
/// 根视口永远保持可见：它要么显示主页，要么显示最近打开的工程。这样 egui 的立即视口
/// 始终由可见的根窗口驱动，不会因为根窗口被隐藏而降低刷新率。主页作为独立窗口显示时，
/// 使用立即视口创建真正的 OS 窗口。
/// The root viewport always stays visible: it shows either the home page or the most recently
/// opened project, so egui's immediate viewports are always driven by a visible root window and
/// never throttled. The home page is shown as its own OS window via an immediate viewport.
pub struct WindowHost {
    shared: Arc<SharedServices>,
    home: SurfingApp,
    /// 根视口当前显示的工程；`None` 表示根视口显示主页。
    /// Project currently shown by the root viewport; `None` means the root shows the home page.
    root_project: Option<SurfingApp>,
    /// 根工程正在加载的路径（小写）。
    /// Path being loaded by the root project (lower case).
    root_pending_path: Option<String>,
    /// 下一帧是否需要把根窗口置前。
    /// Whether the root window must be raised next frame.
    root_focus: bool,
    /// 根是工程时，主页是否作为独立窗口显示。
    /// Whether the home page is shown as a separate window while the root is a project.
    home_viewport: bool,
    /// 下一帧是否需要把主页窗口置前。
    /// Whether the home window must be raised next frame.
    home_focus: bool,
    projects: Vec<ProjectWindow>,
    /// 上一次发送给根窗口的标题，避免每帧重复发送命令。
    /// Last title sent to the root window, so the command is not repeated every frame.
    last_root_title: String,
    /// 单窗口模式：平台不支持多视口时（安卓），主页与工程都绘制在根窗口里。
    /// Single-window mode: on platforms without multi-viewport (Android) both home and the project
    /// are painted inside the root window.
    single_window: bool,
}

impl WindowHost {
    /// 所属模块：app / 多窗口外壳
    /// 功能：初始化主题、共享服务与主页窗口，并应用保存的界面缩放。
    /// 输入：`cc`：eframe 创建上下文。
    /// 输出/副作用：打开音频设备与 MIDI 输入；读取设置与插件缓存；创建主页窗口。
    /// 复杂度：时间 O(设置 + 插件缓存 + 设备初始化)，空间 O(插件数量)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Initialize the theme, shared services and the home window, and apply the saved UI
    /// scale.
    /// Inputs: `cc`: eframe creation context.
    /// Outputs / side effects: opens the audio device and MIDI input; reads settings and the plugin
    /// cache; creates the home window.
    /// Complexity: O(settings + plugin cache + device setup) time and O(plugin count) space.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let span = diag::Span::start_logged("app.shell", "window.root.create", &[]);
        theme::setup(&cc.egui_ctx);
        let shared = SharedServices::new();
        let (settings, _) = shared.settings_snapshot();
        cc.egui_ctx.set_zoom_factor(settings.ui_scale);
        let home = SurfingApp::new_home(&cc.egui_ctx, shared.clone());
        let host = Self {
            shared,
            home,
            root_project: None,
            root_pending_path: None,
            root_focus: false,
            home_viewport: false,
            home_focus: false,
            projects: Vec::new(),
            last_root_title: HOME_WINDOW_TITLE.to_string(),
            single_window: !crate::platform::caps().multi_window,
        };
        span.end(&[("status", "ok")]);
        host
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：返回当前打开的工程窗口数量（含根工程）。
    /// 输入：`self`。
    /// 输出/副作用：数量；不修改状态。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Return the number of open project windows (including the root project).
    /// Inputs: `self`.
    /// Outputs / side effects: the count; no state change.
    /// Complexity: O(1) time and space.
    fn project_count(&self) -> usize {
        self.projects.len() + usize::from(self.root_project.is_some())
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：执行一个窗口级动作。
    /// 输入：
    /// - `ctx`：egui 上下文。
    /// - `action`：要执行的动作。
    /// 输出/副作用：可能创建/聚焦工程窗口、显示主页窗口或回到最近的工程。
    /// 复杂度：时间 O(工程窗口数)，空间 O(1)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Execute one window-level action.
    /// Inputs:
    /// - `ctx`: egui context.
    /// - `action`: action to execute.
    /// Outputs / side effects: may create/focus a project window, show the home window or resume a
    /// project.
    /// Complexity: O(project windows) time and O(1) space.
    fn apply_action(&mut self, ctx: &egui::Context, action: WindowAction) {
        match action {
            WindowAction::ShowHome => self.show_home(ctx),
            WindowAction::OpenProject { path } => self.open_project(ctx, path),
            WindowAction::NewProject { template } => self.open_new_project(ctx, template),
            WindowAction::ResumeProject => self.resume_project(ctx),
        }
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：打开工程文件窗口；同一路径已有窗口时只置前，不重复打开。
    /// 输入：
    /// - `ctx`：egui 上下文。
    /// - `path`：工程文件路径。
    /// 输出/副作用：创建 `SurfingApp` 工程窗口（后台加载），关闭主页窗口并请求重绘。
    /// 复杂度：时间 O(工程窗口数)，空间 O(1)（工程加载在后台）。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Open a project-file window; an already open path is only raised, never duplicated.
    /// Inputs:
    /// - `ctx`: egui context.
    /// - `path`: project file path.
    /// Outputs / side effects: creates a `SurfingApp` project window (background load), closes the
    /// home window and requests a repaint.
    /// Complexity: O(project windows) time and O(1) space (the load runs in the background).
    fn open_project(&mut self, ctx: &egui::Context, path: PathBuf) {
        let key = path_key(&path);
        let matches = |project_path: &Option<PathBuf>, pending: &Option<String>| {
            pending.as_deref() == Some(key.as_str())
                || project_path
                    .as_ref()
                    .is_some_and(|path| path_key(path) == key)
        };
        if matches(
            &self
                .root_project
                .as_ref()
                .and_then(|app| app.project_path.clone()),
            &self.root_pending_path,
        ) {
            self.root_focus = true;
            self.dismiss_home_viewport();
            ctx.request_repaint();
            return;
        }
        if let Some(window_id) = self
            .projects
            .iter_mut()
            .find(|window| matches(&window.app.project_path, &window.pending_path))
            .map(|window| {
                window.focus = true;
                window.app.window_id
            })
        {
            self.shared.set_last_focused_window(window_id);
            self.dismiss_home_viewport();
            ctx.request_repaint();
            return;
        }
        self.spawn_project(ctx, ProjectStart::Open(path), Some(key));
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：新建工程窗口；模板/范例由 `template` 指定，`None` 表示空白工程。
    /// 输入：
    /// - `ctx`：egui 上下文。
    /// - `template`：模板或范例标识。
    /// 输出/副作用：创建工程窗口、关闭主页窗口并请求重绘；不做路径去重。
    /// 复杂度：时间 O(工程窗口数)，空间 O(模板音符数)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Create a project window; `template` selects a template/example and `None` creates
    /// an empty project.
    /// Inputs:
    /// - `ctx`: egui context.
    /// - `template`: template or example identifier.
    /// Outputs / side effects: creates a project window, closes the home window and requests a
    /// repaint; there is no path dedup.
    /// Complexity: O(project windows) time and O(template notes) space.
    fn open_new_project(&mut self, ctx: &egui::Context, template: Option<TemplateId>) {
        let start = match template {
            Some(template) => ProjectStart::Template(template),
            None => ProjectStart::New,
        };
        self.spawn_project(ctx, start, None);
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：按初始内容创建一个工程窗口，并决定它接管根窗口还是成为独立视口。
    /// 输入：
    /// - `ctx`：egui 上下文。
    /// - `start`：空白 / 打开路径 / 模板。
    /// - `pending_path`：正在加载的路径去重键。
    /// 输出/副作用：创建 `SurfingApp`、注册焦点、关闭主页并请求重绘。
    /// 复杂度：时间 O(工程窗口数)，空间 O(工程大小)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Create a project window from its initial content and decide whether it takes over
    /// the root viewport or becomes a standalone viewport.
    /// Inputs:
    /// - `ctx`: egui context.
    /// - `start`: empty / open-path / template.
    /// - `pending_path`: dedup key of the path being loaded.
    /// Outputs / side effects: creates a `SurfingApp`, registers focus, closes home and requests a
    /// repaint.
    /// Complexity: O(project windows) time and O(project size) space.
    fn spawn_project(
        &mut self,
        ctx: &egui::Context,
        start: ProjectStart,
        pending_path: Option<String>,
    ) {
        // 单窗口平台先判断再创建：没有第二个窗口时不能白建一个工程（它会启动恢复线程与引擎）。
        // Single-window platforms check first: without a second window there is no point creating a
        // project (it would start a recovery thread and an engine just to be dropped).
        if self.single_window && self.root_project.is_some() {
            // 安卓等单窗口平台：没有第二个窗口可放新工程，提示用户先关闭当前工程。
            // Single-window platforms (Android): there is no second window for a new project, so
            // ask the user to close the current one first.
            self.home.status = "当前系统只支持一个工程窗口，请先关闭当前工程".into();
            self.home_viewport = true;
            ctx.request_repaint();
            return;
        }
        let app = SurfingApp::new_project(ctx, self.shared.clone(), start);
        self.shared.set_last_focused_window(app.window_id);
        self.dismiss_home_viewport();
        if self.root_project.is_none() {
            // 根视口当前显示主页：直接让新工程接管根窗口，主页窗口随之“关闭”。
            // The root currently shows home: the new project takes over the root window, which
            // also closes the home window.
            self.home.forget_native_window();
            self.root_project = Some(app);
            self.root_pending_path = pending_path;
            self.root_focus = true;
            self.last_root_title.clear();
        } else {
            let viewport_id = egui::ViewportId::from_hash_of(("project_window", app.window_id));
            self.projects.push(ProjectWindow {
                app,
                viewport_id,
                pending_path,
                close_requested: false,
                focus: true,
            });
        }
        ctx.request_repaint();
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：回到最近使用的工程窗口；没有工程窗口时显示主页。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：置前根工程或某个工程视口，或显示主页窗口。
    /// 复杂度：时间 O(工程窗口数)，空间 O(1)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Return to the most recently used project window, or show the home page when none.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: raises the root or a project viewport, or shows the home page.
    /// Complexity: O(project windows) time and O(1) space.
    fn resume_project(&mut self, ctx: &egui::Context) {
        if self.root_project.is_some() {
            self.dismiss_home_viewport();
            self.root_focus = true;
            ctx.request_repaint();
            return;
        }
        let last = self.shared.last_focused_window();
        let index = self
            .projects
            .iter()
            .position(|window| window.app.window_id == last)
            .or_else(|| self.projects.len().checked_sub(1));
        let window_id = index.and_then(|index| {
            let window = self.projects.get_mut(index)?;
            window.focus = true;
            Some(window.app.window_id)
        });
        match window_id {
            Some(window_id) => {
                self.shared.set_last_focused_window(window_id);
                self.dismiss_home_viewport();
                ctx.request_repaint();
            }
            None => self.show_home(ctx),
        }
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：显示并置前主页窗口。根窗口是主页时直接置前根窗口；否则显示独立的立即视口。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：可能创建/置前主页视口；不暂停工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Show and raise the home window. When the root is home, raise the root; otherwise
    /// show the standalone immediate viewport.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: may create/raise the home viewport; the project keeps playing.
    /// Complexity: O(1) time and space.
    fn show_home(&mut self, ctx: &egui::Context) {
        if self.single_window {
            // 单窗口模式：主页直接占用根窗口，工程保持打开并继续播放。
            // Single-window mode: the home page takes over the root window and the project keeps
            // playing.
            self.home_viewport = true;
            self.home_focus = true;
            ctx.request_repaint();
            return;
        }
        if self.root_project.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.request_repaint();
            return;
        }
        if !self.home_viewport {
            // 主页从根窗口切换到独立视口（或重新打开）：句柄需要重新解析。
            // The home page moves from the root window to a standalone viewport (or reopens), so its
            // native handle must be resolved again.
            self.home_viewport = true;
            self.home.forget_native_window();
        }
        self.home_focus = true;
        ctx.request_repaint();
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：关闭主页窗口（工程窗口保持打开）。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：停止绘制主页视口；不暂停工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Close the home window (project windows stay open).
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: stops painting the home viewport; the project keeps playing.
    /// Complexity: O(1) time and space.
    fn hide_home(&mut self, ctx: &egui::Context) {
        self.dismiss_home_viewport();
        ctx.request_repaint();
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：停止显示主页视口，并关闭主页持有的设置窗口状态。
    /// 输入：`self`。
    /// 输出/副作用：清空 `home_viewport` 与 `settings_open`；不修改工程。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Stop showing the home viewport and drop the settings-window state owned by home.
    /// Inputs: `self`.
    /// Outputs / side effects: clears `home_viewport` and `settings_open`; the project is untouched.
    /// Complexity: O(1) time and space.
    fn dismiss_home_viewport(&mut self) {
        self.home_viewport = false;
        // 主页窗口关闭后它的设置视口也随之消失，避免下次显示主页时自动弹出旧设置窗口。
        // The settings viewport belongs to the home window, so it must not reappear later.
        self.home.settings_open = false;
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：关闭根视口当前显示的工程；主页已打开时不强行切回项目列表。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：丢弃根工程（编辑器随之关闭）；主页窗口存在且还有其它工程时，由最近使用的
    /// 工程接管根窗口；否则根窗口回到主页并保留主页已有状态（含设置窗口）。
    /// 复杂度：时间 O(工程窗口数)（工程释放另计），空间 O(1)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Close the project shown by the root viewport without forcing a jump back to the
    /// project list when the home window is already open.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: drops the root project (and its editors); when the home window exists
    /// and other projects remain, the most recently used project takes over the root window;
    /// otherwise the root shows home again while keeping the existing home state (incl. settings).
    /// Complexity: O(project windows) time (project teardown excluded) and O(1) space.
    fn close_root_project(&mut self, ctx: &egui::Context) {
        diag::info(
            "app.shell",
            "window.close.project",
            &[
                (
                    "home_viewport",
                    if self.home_viewport { "true" } else { "false" },
                ),
                ("projects", &self.projects.len().to_string()),
            ],
        );
        self.root_project = None;
        self.root_pending_path = None;
        self.last_root_title.clear();
        if self.home_viewport && !self.projects.is_empty() {
            // 主页已经是独立窗口：让最近使用的工程接管根窗口，根窗口不切回项目列表。
            // Home already owns a standalone window: hand the root to the most recently used
            // project instead of switching it to the project list.
            let last = self.shared.last_focused_window();
            let index = self
                .projects
                .iter()
                .position(|window| window.app.window_id == last)
                .unwrap_or(self.projects.len() - 1);
            let mut window = self.projects.remove(index);
            window.app.forget_native_window();
            self.root_project = Some(window.app);
            self.root_pending_path = window.pending_path;
            self.root_focus = true;
            ctx.request_repaint();
            return;
        }
        if self.home_viewport {
            // 主页已在独立窗口显示：把主页内容交回根窗口，但不清空主页状态，
            // 避免设置等子窗口被一起关掉。
            // Home is already displayed in its own window: move it back into the root without
            // clearing its state, so child windows such as settings are not closed.
            self.home_viewport = false;
        }
        self.home.forget_native_window();
        ctx.request_repaint();
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：关闭主页根窗口时，把最近的一个工程窗口提升为根窗口，避免整个应用退出。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：把一个工程从视口列表移到根窗口；其余工程窗口保持不动。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: When the home root window is closed, promote one project window to the root instead
    /// of exiting the whole application.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: moves one project from the viewport list into the root window; other
    /// project windows stay untouched.
    /// Complexity: O(1) time and space.
    fn promote_project_to_root(&mut self, ctx: &egui::Context) {
        let Some(mut window) = self.projects.pop() else {
            return;
        };
        diag::info(
            "app.shell",
            "window.promote_project",
            &[
                ("window_id", &window.app.window_id.to_string()),
                ("remaining", &self.projects.len().to_string()),
            ],
        );
        window.app.forget_native_window();
        self.root_project = Some(window.app);
        self.root_pending_path = window.pending_path;
        self.root_focus = true;
        self.dismiss_home_viewport();
        self.last_root_title.clear();
        ctx.request_repaint();
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：绘制根视口的主页工程或主页内容。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：绘制根窗口；可能修改工程/插件窗口/状态栏。
    /// 复杂度：每帧 O(可见控件数)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Paint the root viewport: the root project or the home page.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: paints the root window; may edit the project, plugin windows or the
    /// status bar.
    /// Complexity: O(visible widgets) per frame.
    fn draw_root(&mut self, ctx: &egui::Context) {
        if self.root_focus {
            self.root_focus = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        if self.single_window && (self.home_viewport || self.root_project.is_none()) {
            self.home.ui_frame(ctx);
            return;
        }
        match &mut self.root_project {
            Some(project) => project.ui_frame(ctx),
            None => self.home.ui_frame(ctx),
        }
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：在根是工程时，把主页绘制成一个独立视口窗口。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：创建/更新主页窗口；关闭请求会停止窗口。
    /// 复杂度：每帧 O(可见控件数)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Paint the home page as a standalone viewport window while the root is a project.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: creates/updates the home window; a close request stops it.
    /// Complexity: O(visible widgets) per frame.
    fn draw_home_viewport(&mut self, ctx: &egui::Context) {
        if self.single_window || self.root_project.is_none() || !self.home_viewport {
            return;
        }
        let viewport_id = home_viewport_id();
        if self.home_focus {
            self.home_focus = false;
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Focus);
        }
        let builder = egui::ViewportBuilder::default()
            .with_title(HOME_WINDOW_TITLE)
            .with_inner_size(PROJECT_WINDOW_SIZE)
            .with_min_inner_size(PROJECT_MIN_SIZE)
            .with_resizable(true)
            .with_decorations(false);
        let mut open = true;
        ctx.show_viewport_immediate(viewport_id, builder, |ui, _class| {
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                open = false;
                return;
            }
            self.home.ui_frame(ui.ctx());
        });
        if !open {
            self.hide_home(ctx);
        }
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：绘制全部工程视口，并记录各窗口的关闭请求。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：绘制工程窗口（含其浮动卷帘）；写入 `close_requested`。
    /// 复杂度：每帧 O(工程窗口数 × 可见控件数)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Draw every project viewport and record each window's close request.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: paints the project windows (including their floating rolls); writes
    /// `close_requested`.
    /// Complexity: O(project windows × visible widgets) per frame.
    fn draw_projects(&mut self, ctx: &egui::Context) {
        if self.single_window {
            // 单窗口模式没有独立工程视口：工程内容由 `draw_root` 绘制。
            // Single-window mode has no standalone project viewport: `draw_root` paints it.
            return;
        }
        for window in &mut self.projects {
            window.close_requested = false;
            let viewport_id = window.viewport_id;
            let title = window.app.window_title.clone();
            if window.focus {
                window.focus = false;
                // 最小化的窗口必须先还原才能被置前。
                // A minimized window must be restored before it can be raised.
                ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Focus);
            }
            let builder = egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size(PROJECT_WINDOW_SIZE)
                .with_min_inner_size(PROJECT_MIN_SIZE)
                .with_resizable(true)
                // 与主页一致：自绘应用栏，不使用系统标题栏。
                // Same as the home window: a hand-drawn app bar, no system title bar.
                .with_decorations(false);
            let mut close_requested = false;
            ctx.show_viewport_immediate(viewport_id, builder, |ui, _class| {
                // 有未保存修改时不移除窗口：应用会在本帧取消关闭并弹出保存确认。
                // With unsaved changes the window is not removed: the app cancels the close and
                // shows the save confirmation in this frame.
                // 加载/插件同步尚未完成时不移除窗口：工程窗口会取消关闭，并在后台工作
                // 结束后自行发送 Close。
                // While loading or syncing the window is not removed: the project window cancels
                // the close and sends Close itself once the background work finishes.
                if ui.ctx().input(|i| i.viewport().close_requested())
                    && (window.app.force_closing()
                        || (!window.app.has_unsaved_changes() && window.app.can_close_now()))
                {
                    close_requested = true;
                }
                window.app.ui_frame(ui.ctx());
            });
            window.close_requested = close_requested;
        }
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：为根工程与所有工程视口生成唯一标题，并把根标题同步到根窗口。
    /// 输入：`ctx`：egui 上下文。
    /// 输出/副作用：写入各窗口 `window_title`；必要时发送 `Title` 命令；清理加载占位。
    /// 复杂度：时间 O(工程窗口数 × 标题长度)，空间 O(工程窗口数)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Give the root project and every project viewport a unique title and sync the root
    /// title to the root window.
    /// Inputs: `ctx`: egui context.
    /// Outputs / side effects: writes each window's `window_title`; may send `Title`; clears load
    /// placeholders.
    /// Complexity: O(project windows × title length) time and O(project windows) space.
    fn update_window_titles(&mut self, ctx: &egui::Context) {
        let mut used: HashSet<String> = HashSet::new();
        let root_title = match &mut self.root_project {
            Some(project) => {
                let title = unique_window_title(&project.project_title(), &mut used);
                project.window_title = title.clone();
                if !project.is_loading_project() {
                    self.root_pending_path = None;
                }
                title
            }
            None => {
                used.insert(HOME_WINDOW_TITLE.to_string());
                HOME_WINDOW_TITLE.to_string()
            }
        };
        for window in &mut self.projects {
            let title = unique_window_title(&window.app.project_title(), &mut used);
            window.app.window_title = title;
            if !window.app.is_loading_project() {
                window.pending_path = None;
            }
        }
        if self.last_root_title != root_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(root_title.clone()));
            self.last_root_title = root_title;
        }
    }
}

impl eframe::App for WindowHost {
    /// 所属模块：app / 多窗口外壳
    /// 功能：每帧编排：关闭处理 → 根窗口 → 窗口动作 → 主页/工程视口 → 插件心跳。
    /// 输入：
    /// - `ui`：eframe 根界面（根窗口内容）。
    /// - `frame`：eframe 帧上下文（原生句柄按窗口各自解析）。
    /// 输出/副作用：绘制全部窗口；创建/移除工程窗口；切换根窗口内容；维护插件心跳开关。
    /// 复杂度：每帧 O(窗口数 × 可见控件数)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: Per frame: close handling → root window → actions → home/project viewports →
    /// plugin heartbeat.
    /// Inputs:
    /// - `ui`: eframe root UI (root window content).
    /// - `frame`: eframe frame context (native handles are resolved per window elsewhere).
    /// Outputs / side effects: paints every window; creates/removes project windows; switches the
    /// root content; maintains the plugin heartbeat flag.
    /// Complexity: O(windows × visible widgets) per frame.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let frame_start = std::time::Instant::now();
        diag::note_frame();
        diag::set_activity("ui.frame");
        hotkeys::install();
        // 关闭根窗口：根是工程时只关闭该工程并回到主页；根是主页且有工程窗口时，把一个工程
        // 提升到根窗口，避免关闭主页导致整个应用退出。根工程有未保存修改时保持窗口打开，
        // 由 `SurfingApp::ui_frame` 弹出保存确认。
        // Closing the root window: when it shows a project, close only that project and return to
        // home; when it shows home and project windows exist, promote one project to the root so
        // closing home does not exit the whole application. A root project with unsaved changes
        // keeps the window open and `SurfingApp::ui_frame` shows the save confirmation.
        if ctx.input(|i| i.viewport().close_requested()) {
            let projects_text = self.project_count().to_string();
            diag::info(
                "app.shell",
                "window.close.request",
                &[
                    (
                        "root",
                        if self.root_project.is_some() {
                            "project"
                        } else {
                            "home"
                        },
                    ),
                    ("projects", &projects_text),
                ],
            );
            if self.root_project.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                let can_close = self.root_project.as_ref().is_some_and(|project| {
                    project.force_closing()
                        || (!project.has_unsaved_changes() && project.can_close_now())
                });
                if can_close {
                    self.close_root_project(&ctx);
                } else if let Some(project) = &mut self.root_project {
                    // 加载/插件同步尚未完成：等后台工作结束后由工程窗口发送关闭。
                    // Loading or plugin sync is still running; the project window closes itself
                    // once the background work finishes.
                    if !project.can_close_now() {
                        project.request_close_when_ready();
                    }
                }
            } else if !self.projects.is_empty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.promote_project_to_root(&ctx);
            } else {
                // 真正退出应用的路径：根窗口是主页且没有任何工程窗口。主动写日志尾帧并终止
                // 进程，不再依赖 eframe 的退出链——插件或驱动让事件循环无法收尾时，
                // 进程会留在后台且无法被 taskkill 正常结束。
                // The only real exit path: the root is home and no project window exists. Write the
                // footer and terminate here instead of relying on eframe's exit chain; a plugin or
                // driver that prevents the event loop from finishing otherwise leaves a background
                // process that taskkill cannot end cleanly.
                diag::info("app.shell", "window.exit", &[("reason", "home_closed")]);
                diag::request_exit("home_closed");
            }
        }
        let project_count = self.project_count();
        self.home.open_project_count = project_count;
        self.home.can_resume_project = project_count > 0;
        self.update_window_titles(&ctx);
        self.draw_root(&ctx);
        // 根窗口产生的动作（新建/打开工程、显示主页）在同一帧执行，新窗口可以立即绘制。
        // Actions from the root frame (new/open project, show home) run in the same frame so the new
        // window can be painted immediately.
        let mut actions = std::mem::take(&mut self.home.pending_window_actions);
        if let Some(project) = &mut self.root_project {
            actions.append(&mut project.pending_window_actions);
        }
        for action in actions {
            self.apply_action(&ctx, action);
        }
        self.draw_home_viewport(&ctx);
        self.draw_projects(&ctx);
        // 工程视口产生的动作在下一帧执行（此时它们已经完成本帧绘制）。
        // Actions from project viewports run on the next frame, after they finish painting.
        let mut actions = Vec::new();
        for window in &mut self.projects {
            actions.append(&mut window.app.pending_window_actions);
        }
        for action in actions {
            self.apply_action(&ctx, action);
        }
        // 关闭工程视口：移除窗口；关闭最后一个工程时停留在主页。
        // Closing a project viewport removes it; closing the last one stays on the home page.
        if self.projects.iter().any(|window| window.close_requested) {
            self.projects.retain(|window| !window.close_requested);
            if self.projects.is_empty() {
                self.shared.set_last_focused_window(0);
            } else if !self
                .projects
                .iter()
                .any(|window| window.app.window_id == self.shared.last_focused_window())
            {
                let fallback = self.projects.last().map(|window| window.app.window_id);
                if let Some(window_id) = fallback {
                    self.shared.set_last_focused_window(window_id);
                }
            }
        }
        // 主页没有作为根窗口或独立视口绘制时，仍需轮询后台扫描/保存结果。
        // When home is neither the root nor a viewport, its background scans/saves still need
        // polling.
        if self.root_project.is_some() && !self.home_viewport {
            self.home.poll_background(&ctx);
        }
        let mut has_editors = self
            .root_project
            .as_ref()
            .is_some_and(|project| !project.editors.is_empty());
        has_editors |= self
            .projects
            .iter()
            .any(|window| !window.app.editors.is_empty());
        heartbeat::set_active(has_editors);
        // 有工程窗口或主页视口时保持 60 FPS：插件 GUI、播放头与电平表都依赖持续重绘。
        // Keep 60 FPS while a project or the home viewport is open: plugin GUIs, playheads and
        // meters need repaints.
        if self.project_count() > 0 || self.home_viewport {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
        diag::note_frame_duration(frame_start.elapsed());
    }

    /// 所属模块：app / 多窗口外壳
    /// 功能：eframe 退出回调：在 `WindowHost` 析构链运行之前直接终止进程。
    /// 输入：
    /// - `_gl`：glow 渲染上下文（未使用，退出时不释放 GL 资源，交给操作系统回收）。
    /// 输出/副作用：立即结束进程；不执行 `WindowHost` / `SurfingApp` / `SharedServices` /
    /// cpal 流 / 插件实例的析构。
    /// 复杂度：时间 O(1)，空间 O(1)。
    ///
    /// Module: app / multi-window shell
    /// Purpose: eframe exit callback: terminate the process before the `WindowHost` teardown chain
    /// runs.
    /// Inputs:
    /// - `_gl`: the glow render context (unused; GL resources are handed to the OS at exit).
    /// Outputs / side effects: ends the process immediately; skips the destructors of
    /// `WindowHost`, `SurfingApp`, `SharedServices`, the cpal stream and plugin instances.
    /// Complexity: O(1) time and space.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // 主页是唯一会真正退出整个应用的路径（关闭工程窗口只回到主页）。eframe 在 `run_native`
        // 内部先调用本回调，再 drop 应用；而应用析构会 drop 共享的 cpal 流并 join 音频线程，
        // 音频回调闭包缓存的工程引擎在此时析构插件实例，插件 DLL 的 DllMain(DETACH) 可能等待
        // 已停止的 UI 消息循环，导致 join 永不返回、`run_native` 永不返回，放在其后的
        // 终止调用永远执行不到。因此必须在这里、析构链运行前写尾帧并直接终止进程。
        // The home page is the only path that really exits the whole app (closing a project window
        // only returns to home). eframe calls this callback inside `run_native` and drops the app
        // afterwards; that teardown drops the shared cpal stream and joins the audio thread, whose
        // callback closure then destroys cached project engines and their plugin instances.
        // A plugin DLL's DllMain(DETACH) may wait for the already-stopped UI message loop, so the
        // join never returns, `run_native` never returns, and any termination placed after it never
        // runs. Write the footer and terminate right here, before the teardown chain starts.
        diag::info("app.shell", "window.exit", &[("reason", "on_exit")]);
        diag::request_exit("on_exit");
    }
}

/// 所属模块：app / 多窗口外壳
/// 功能：返回主页独立视口的 id。
/// 输入：无。
/// 输出/副作用：稳定的视口 id；不修改状态。
/// 复杂度：时间 O(1)，空间 O(1)。
///
/// Module: app / multi-window shell
/// Purpose: Return the viewport id of the standalone home window.
/// Inputs: none.
/// Outputs / side effects: stable viewport id; no state change.
/// Complexity: O(1) time and space.
fn home_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("home_window")
}

/// 所属模块：app / 多窗口外壳
/// 功能：生成形如“名称 - SurfingStudio”的唯一窗口标题；重复时追加序号。
/// 输入：
/// - `base`：工程标题基名。
/// - `used`：已占用的标题集合。
/// 输出/副作用：返回唯一标题并写入 `used`。
/// 复杂度：时间 O(同名窗口数 × 标题长度)，空间 O(标题长度)。
///
/// Module: app / multi-window shell
/// Purpose: Build a unique "name - SurfingStudio" title, appending an index on duplicates.
/// Inputs:
/// - `base`: project base title.
/// - `used`: titles already taken.
/// Outputs / side effects: the unique title, inserted into `used`.
/// Complexity: O(same-name windows × title length) time and O(title length) space.
fn unique_window_title(base: &str, used: &mut HashSet<String>) -> String {
    let mut title = format!("{base} - SurfingStudio");
    let mut index = 2;
    while !used.insert(title.clone()) {
        title = format!("{base} ({index}) - SurfingStudio");
        index += 1;
    }
    title
}

/// 所属模块：app / 多窗口外壳
/// 功能：返回路径的小写去重键。
/// 输入：`path`：工程路径。
/// 输出/副作用：小写路径文本；不修改状态。
/// 复杂度：时间 O(路径长度)，空间 O(路径长度)。
///
/// Module: app / multi-window shell
/// Purpose: Return the lower-case dedup key of a path.
/// Inputs: `path`: project path.
/// Outputs / side effects: lower-case path text; no state change.
/// Complexity: O(path length) time and space.
fn path_key(path: &std::path::Path) -> String {
    path.to_string_lossy().to_lowercase()
}
