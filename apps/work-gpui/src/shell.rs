//! Native workbench layout. The shared view model owns transcript ordering, tool
//! labels, code tokens and metrics.

#[path = "attachments.rs"]
pub(crate) mod attachments;
#[path = "browser.rs"]
mod browser;
#[path = "collaboration.rs"]
mod collaboration;
#[path = "composer_config.rs"]
pub(crate) mod composer_config;
#[path = "composer.rs"]
pub(crate) mod editor;
#[path = "message_queue.rs"]
mod message_queue;
#[path = "navigation.rs"]
mod navigation;
#[path = "node_connection.rs"]
mod node_connection;
#[path = "node_share.rs"]
mod node_share;
#[path = "project_files.rs"]
pub(crate) mod project_files;
#[path = "project_management.rs"]
pub(crate) mod project_management;
#[path = "provider_settings.rs"]
mod provider_settings;
#[path = "question.rs"]
mod question;
#[path = "resource_settings.rs"]
mod resource_settings;
#[path = "review.rs"]
mod review;
#[path = "rich_text.rs"]
mod rich_text;
#[path = "session_action.rs"]
pub(crate) mod session_action;
#[path = "session_context.rs"]
mod session_context;
#[path = "session_filter.rs"]
mod session_filter;
#[path = "session_fork.rs"]
pub(crate) mod session_fork;
#[path = "session_management.rs"]
pub(crate) mod session_management;
#[path = "settings.rs"]
mod settings;
#[path = "search.rs"]
mod workbench_search;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, Context, CursorStyle, Div, FocusHandle, FontWeight, Hsla, MouseButton,
    MouseMoveEvent, ScrollHandle, SharedString, Task, Window, WindowControlArea, div, point,
    prelude::*, px, relative,
};
use serde_json::Value;
use work_client::ClientError;
use work_core::{MessageBlock, RunPhase, RunProjection, SettlementOutcome};
use work_viewmodel::conversation::transcript_view::{self, ActivityItem, Part, ToolView};
use work_viewmodel::conversation::{code_tokens, markdown, tool_display};
use work_viewmodel::observability::metrics::{percent, tokens};
use work_viewmodel::session_controls::{
    PendingRequests, SessionControls, SessionObservation, controls,
};
use work_viewmodel::session_receipts::{command_admission, interpret_receipt};
use work_viewmodel::transcript::TranscriptOrder;

use crate::data::{Creation, Node, SessionFrame, Snapshot};
use crate::icons::icon;
use crate::prompt::Draft;
use crate::theme::Palette;

const PRIMARY_MODIFIER_LABEL: &str = if cfg!(target_os = "macos") {
    "⌘"
} else {
    "Ctrl"
};

#[derive(Clone, Copy)]
enum WorkbenchShortcut {
    NewConversation,
    Navigation,
    Browser,
    Settings,
    Search,
}

impl WorkbenchShortcut {
    const BINDINGS: [(Self, &str, bool, &str); 5] = [
        (Self::Search, "K", false, "搜索与操作"),
        (Self::NewConversation, "N", false, "新对话"),
        (Self::Navigation, "B", false, "切换侧栏"),
        (Self::Browser, "B", true, "切换浏览器面板"),
        (Self::Settings, ",", false, "打开设置"),
    ];

    fn from_keystroke(stroke: &gpui::Keystroke) -> Option<Self> {
        if !primary_modifier(stroke.modifiers) || stroke.modifiers.alt || stroke.modifiers.function
        {
            return None;
        }
        Self::BINDINGS.iter().find_map(|&(action, key, shift, _)| {
            (stroke.key.eq_ignore_ascii_case(key) && stroke.modifiers.shift == shift)
                .then_some(action)
        })
    }
}

fn primary_modifier(modifiers: gpui::Modifiers) -> bool {
    if cfg!(target_os = "macos") {
        modifiers.platform && !modifiers.control
    } else {
        modifiers.control && !modifiers.platform
    }
}

const READING_WIDTH: f32 = 768.0;
const READING_INSET: f32 = 24.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    Navigation,
    Inspector,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum DraftKey {
    Session(usize),
    NewConversation(Option<usize>),
}

struct WelcomeSend {
    revision: u64,
    message: String,
}

impl WelcomeSend {
    fn matches(&self, draft: &Draft) -> bool {
        draft.can_send() && draft.input.revision == self.revision && draft.message() == self.message
    }
}

enum SessionCreation {
    New,
    Send(WelcomeSend),
    Review,
}

enum Row {
    User(String),
    Assistant {
        key: String,
        source: String,
        view: Rc<markdown::MarkdownView>,
        fork_entry_id: Option<String>,
    },
    Tool(ToolView),
}

pub struct Shell {
    snapshot: Snapshot,
    node: Option<Node>,
    watcher: Option<(Task<()>, tokio::task::AbortHandle)>,
    frame: Option<SessionFrame>,
    actions: HashMap<usize, session_action::Request>,
    next_action: u64,
    checking_receipts: HashSet<usize>,
    load_error: Option<String>,
    observation: Option<&'static str>,
    palette: Palette,
    preferences: settings::Preferences,
    settings: settings::Panel,
    collaboration: collaboration::Panel,
    node_connection: node_connection::Form,
    node_share: node_share::Share,
    resource_page: Option<resource_settings::ResourceTab>,
    resource_inspect_focus: FocusHandle,
    resource_settings: resource_settings::Panel,
    provider_settings: provider_settings::Panel,
    search: editor::SearchInput,
    workbench_search: workbench_search::Panel,
    session_context: session_context::Panel,
    finish_notice: Option<(&'static str, Task<()>)>,
    queue: message_queue::Panel,
    dark: bool,
    navigation_open: bool,
    overview_open: bool,
    inspector_full: bool,
    inspector_width: Option<f32>,
    resizing_inspector: bool,
    overlay: Option<Overlay>,
    session_history: Vec<(Option<usize>, Option<usize>)>,
    history_position: usize,
    files: project_files::Panel,
    comment: Option<review::Comment>,
    project_form: project_management::Form,
    project_menu: Option<usize>,
    welcome_project_picker: bool,
    question: Option<question::Question>,
    session_management: session_management::Panel,
    session_filter: session_filter::Panel,
    session_fork: session_fork::Panel,
    session_catalog_revision: u64,
    browser: browser::Tabs,
    composer_config: composer_config::Panel,
    creating_session: Option<Task<()>>,
    review_after_bind: Option<usize>,
    send_after_bind: Option<(usize, WelcomeSend)>,
    creation_error: Option<(bool, String)>,
    creation_readback: Option<(usize, String)>,
    navigation_revision: u64,
    rows: Vec<Row>,
    project: Option<usize>,
    session: Option<usize>,
    transcript_session: Option<usize>,
    transcript_scroll: ScrollHandle,
    expanded: HashSet<String>,
    drafts: HashMap<DraftKey, Draft>,
    selecting_attachments: HashSet<usize>,
    focus: FocusHandle,
    input_layout: Option<editor::Layout>,
    input_dragging: bool,
    order: TranscriptOrder,
    /// One-shot diagnostic so a headless launch can prove the first frame ran.
    traced: bool,
}

impl Shell {
    pub fn new(
        snapshot: Snapshot,
        node: Option<Node>,
        appearance: gpui::WindowAppearance,
        cx: &mut Context<Self>,
    ) -> Self {
        let fixture = node.is_none();
        let session = if fixture {
            snapshot
                .sessions
                .iter()
                .position(|session| session.id == "dark")
        } else {
            snapshot
                .sessions
                .iter()
                .enumerate()
                .filter(|(_, session)| !session.archived)
                .max_by_key(|(_, session)| session.updated_at)
                .map(|(index, _)| index)
        };
        let project = match session {
            Some(index) => snapshot
                .projects
                .iter()
                .position(|project| project.id == snapshot.sessions[index].project_id),
            None => (!snapshot.projects.is_empty()).then_some(0),
        };
        let mut order = TranscriptOrder::default();
        let (rows, _) = present_rows(
            &mut order,
            "fixture",
            &snapshot.history,
            &snapshot.blocks,
            None,
            &[],
        );
        let mut shell = Self {
            session: session.filter(|_| fixture),
            transcript_session: session.filter(|_| fixture),
            transcript_scroll: ScrollHandle::new(),
            project,
            snapshot,
            node,
            watcher: None,
            frame: None,
            actions: HashMap::new(),
            next_action: 0,
            checking_receipts: HashSet::new(),
            load_error: None,
            observation: None,
            palette: Palette::from_preset("codex", false, false),
            preferences: settings::Preferences::load(),
            settings: settings::Panel::new(cx),
            collaboration: collaboration::Panel::default(),
            node_connection: node_connection::Form::default(),
            node_share: node_share::Share::default(),
            resource_page: None,
            resource_inspect_focus: cx.focus_handle().tab_stop(true),
            resource_settings: resource_settings::Panel::default(),
            provider_settings: provider_settings::Panel::default(),
            search: editor::SearchInput::new(cx),
            workbench_search: workbench_search::Panel::new(cx),
            session_context: session_context::Panel::new(cx),
            finish_notice: None,
            queue: message_queue::Panel::default(),
            dark: false,
            navigation_open: true,
            overview_open: true,
            inspector_full: false,
            inspector_width: None,
            resizing_inspector: false,
            overlay: None,
            session_history: vec![(project, session)],
            history_position: 0,
            files: project_files::Panel::default(),
            comment: None,
            project_form: project_management::Form::default(),
            project_menu: None,
            welcome_project_picker: false,
            question: None,
            session_management: session_management::Panel::default(),
            session_filter: session_filter::Panel::new(cx),
            session_fork: session_fork::Panel::default(),
            session_catalog_revision: 0,
            browser: browser::Tabs::default(),
            composer_config: composer_config::Panel::default(),
            creating_session: None,
            review_after_bind: None,
            send_after_bind: None,
            creation_error: None,
            creation_readback: None,
            navigation_revision: 0,
            rows,
            expanded: HashSet::new(),
            drafts: HashMap::new(),
            selecting_attachments: HashSet::new(),
            focus: cx.focus_handle(),
            input_layout: None,
            input_dragging: false,
            order,
            traced: false,
        };
        shell.apply_appearance(appearance, true);
        if !fixture {
            if session.is_some() {
                shell.change_session(session, cx);
            }
            shell.show_project_tab(project_files::Tab::Changes, cx);
        }
        shell
    }

    pub fn focus_composer(&self, window: &mut Window) {
        self.browser.restore_composer_focus();
        self.focus.focus(window);
    }

    fn draft_key(&self) -> DraftKey {
        self.session
            .map_or(DraftKey::NewConversation(self.project), DraftKey::Session)
    }

    fn select_project(&mut self, index: usize, cx: &mut Context<Self>) {
        self.welcome_project_picker = false;
        self.set_project(Some(index), cx);
        self.navigate_session(None, cx);
    }

    fn set_project(&mut self, project: Option<usize>, cx: &mut Context<Self>) {
        if self.project != project {
            self.cancel_comment();
            if self.session.is_none() {
                if let Some(draft) = self.drafts.get_mut(&self.draft_key()) {
                    draft.input.finish_composition();
                }
                self.input_layout = None;
                self.input_dragging = false;
            }
            self.close_session_filter_menu();
            self.project = project;
            self.reset_resource_catalog();
            self.navigation_revision += 1;
            self.reset_project_files(cx);
        }
    }

    fn select_session(&mut self, index: usize, cx: &mut Context<Self>) {
        self.project_menu = None;
        self.close_session_filter_menu();
        self.welcome_project_picker = false;
        let project = self
            .snapshot
            .projects
            .iter()
            .position(|project| project.id == self.snapshot.sessions[index].project_id);
        self.set_project(project, cx);
        self.navigate_session(Some(index), cx);
        if self.overlay == Some(Overlay::Navigation) {
            self.overlay = None;
        }
    }

    fn navigate_session(&mut self, session: Option<usize>, cx: &mut Context<Self>) {
        self.close_resource_page();
        let target = (self.project, session);
        if self.session_history[self.history_position] != target {
            self.session_history.truncate(self.history_position + 1);
            self.session_history.push(target);
            self.history_position += 1;
        }
        self.change_session(session, cx);
    }

    fn history_target(&self, forward: bool) -> Option<usize> {
        let available = |&index: &usize| {
            self.session_history[index]
                .1
                .is_none_or(|session| !self.snapshot.sessions[session].archived)
        };
        if forward {
            (self.history_position + 1..self.session_history.len()).find(available)
        } else {
            (0..self.history_position).rev().find(available)
        }
    }

    fn step_history(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(position) = self.history_target(forward) else {
            return;
        };
        self.return_from_resources(window, cx);
        self.history_position = position;
        let (project, session) = self.session_history[self.history_position];
        self.set_project(project, cx);
        self.change_session(session, cx);
    }

    fn can_create_session(&self) -> bool {
        self.node.is_some()
            && self.project.is_some()
            && self.creating_session.is_none()
            && !self.session_fork.in_flight()
            && !self
                .creation_error
                .as_ref()
                .is_some_and(|(unknown, _)| *unknown)
    }

    fn create_session(&mut self, source: SessionCreation, cx: &mut Context<Self>) {
        let detached_review = matches!(source, SessionCreation::Review);
        if !self.can_create_session() {
            return;
        }
        let (Some(node), Some(project)) = (self.node.as_ref(), self.project) else {
            return;
        };
        if detached_review && self.creation_readback.is_some() {
            return;
        }
        let retry = self.creation_readback.take();
        let (project, id) = retry.map_or((project, None), |(project, id)| (project, Some(id)));
        let work = node.create_session(
            self.snapshot.projects[project].id.clone(),
            id,
            detached_review,
        );
        let revision = self.navigation_revision;
        let welcome = self.session.is_none();
        self.creation_error = None;
        self.creating_session = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |shell, cx| {
                shell.creating_session = None;
                match result {
                    Ok(Creation::Created(session)) => {
                        let existing = shell
                            .snapshot
                            .sessions
                            .iter()
                            .position(|item| item.id == session.id);
                        if detached_review && (existing.is_some() || session.archived) {
                            shell
                                .files
                                .set_review_notice("节点未确认一个新的可用审阅会话。");
                            cx.notify();
                            return;
                        }
                        shell.session_catalog_revision += 1;
                        let index = match existing {
                            Some(index) => {
                                shell.snapshot.sessions[index] = session;
                                index
                            }
                            None => {
                                shell.snapshot.sessions.push(session);
                                shell.snapshot.sessions.len() - 1
                            }
                        };
                        if shell.project == Some(project) && shell.navigation_revision == revision {
                            let welcome_key = DraftKey::NewConversation(Some(project));
                            if welcome
                                && !detached_review
                                && existing.is_none()
                                && !shell.snapshot.sessions[index].archived
                            {
                                if let Some(mut draft) = shell.drafts.remove(&welcome_key) {
                                    draft.input.finish_composition();
                                    shell.drafts.insert(DraftKey::Session(index), draft);
                                }
                            }
                            shell.select_session(index, cx);
                            if existing.is_none()
                                && !shell.snapshot.sessions[index].archived
                                && let SessionCreation::Send(send) = source
                            {
                                shell.send_after_bind = Some((index, send));
                            }
                            if detached_review {
                                shell.review_after_bind = Some(index);
                                shell.files.set_review_notice("正在绑定独立审阅会话…");
                            }
                        }
                    }
                    Ok(Creation::ReadFailed { id, error }) => {
                        shell.creation_readback = Some((project, id));
                        shell.creation_error =
                            Some((false, format!("节点已创建会话，读取详情失败：{error}")));
                        if detached_review && shell.navigation_revision == revision {
                            shell
                                .files
                                .set_review_notice("独立审阅未提交。可从左栏核对已创建的会话。");
                        }
                    }
                    Ok(Creation::WriteFailed(error)) if rejected(&error) => {
                        shell.creation_error = Some((false, format!("新建对话失败：{error}")));
                        if detached_review && shell.navigation_revision == revision {
                            shell
                                .files
                                .set_review_notice(format!("创建独立审阅会话失败：{error}"));
                        }
                    }
                    _ => {
                        shell.creation_error =
                            Some((true, "新建结果未知。重新连接后可查看节点会话列表。".into()));
                        if detached_review && shell.navigation_revision == revision {
                            shell
                                .files
                                .set_review_notice("独立审阅未提交。请核对节点会话列表。");
                        }
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn change_session(&mut self, session: Option<usize>, cx: &mut Context<Self>) {
        let changed = self.session != session;
        if !changed && self.load_error.is_none() {
            cx.notify();
            return;
        }
        if changed {
            self.session_context.reset();
            self.session_fork.invalidate_follow();
            self.close_question();
            self.cancel_comment();
            self.close_session_management();
            self.review_after_bind = None;
            self.send_after_bind = None;
            self.welcome_project_picker = false;
            self.files.cancel_review_check();
            self.navigation_revision += 1;
            if let Some(draft) = self.drafts.get_mut(&self.draft_key()) {
                draft.input.finish_composition();
            }
            self.input_layout = None;
            self.input_dragging = false;
            self.transcript_scroll.set_offset(point(px(0.0), px(0.0)));
            self.expanded.clear();
            if self.node.is_some() {
                self.transcript_session = None;
                self.rows.clear();
            }
        }
        self.session = session;
        if self.session.is_some() {
            self.read_session_management_capabilities(cx);
        }
        self.queue.select(session, self.preferences.follow_up);
        self.composer_config.reset_observation();
        self.frame = None;
        self.observation = None;
        self.load_error = None;
        if let Some((_, request)) = self.watcher.take() {
            request.abort();
        }
        if let Some((node, index)) = self.node.as_ref().zip(session) {
            let session_id = self.snapshot.sessions[index].id.clone();
            let (mut updates, cancel) = node.watch(session_id);
            let display = cx.spawn(async move |this, cx| {
                while let Some(result) = updates.recv().await {
                    let stop = this
                        .update(cx, |shell, cx| {
                            if shell.session != Some(index) {
                                return true;
                            }
                            match result {
                                Ok(frame) => shell.apply_frame(index, frame, cx),
                                Err(error) => {
                                    shell.frame = None;
                                    shell.composer_config.reset_observation();
                                    shell.load_error = Some(error.to_string());
                                    shell.session_fork.invalidate_follow();
                                    shell.send_after_bind = None;
                                    if shell.review_after_bind.take().is_some() {
                                        shell.files.set_review_notice(
                                            "独立审阅会话绑定失败，审阅尚未提交。",
                                        );
                                    }
                                }
                            }
                            cx.notify();
                            shell.load_error.is_some()
                        })
                        .unwrap_or(true);
                    if stop {
                        break;
                    }
                }
            });
            self.watcher = Some((display, cancel));
        }
        cx.notify();
    }

    fn show_finish_notice(&mut self, outcome: &SettlementOutcome, cx: &mut Context<Self>) {
        let message = match outcome {
            SettlementOutcome::Completed => "任务已完成",
            SettlementOutcome::Failed => "任务执行失败",
            SettlementOutcome::Interrupted => "任务已停止",
            SettlementOutcome::Suspended => "任务已暂停",
            SettlementOutcome::Declined => "任务已拒绝",
            SettlementOutcome::Compacted | SettlementOutcome::Unknown(_) => return,
        };
        let timer = cx.background_executor().timer(Duration::from_secs(3));
        let expiry = cx.spawn(async move |shell, cx| {
            timer.await;
            let _ = shell.update(cx, |shell, cx| {
                shell.finish_notice = None;
                cx.notify();
            });
        });
        self.finish_notice = Some((message, expiry));
    }

    fn apply_frame(&mut self, index: usize, frame: SessionFrame, cx: &mut Context<Self>) {
        if self.preferences.notify_finish
            && frame.event == "agent_settled"
            && !self
                .frame
                .as_ref()
                .is_some_and(|previous| matches!(previous.projection.phase, RunPhase::Settled(_)))
            && let RunPhase::Settled(outcome) = &frame.projection.phase
        {
            self.show_finish_notice(outcome, cx);
        }
        if frame.projection.connection != work_core::ConnectionState::Online {
            self.session_fork.invalidate_follow();
        }
        if frame.projection.connection == work_core::ConnectionState::Online
            && self
                .question
                .as_ref()
                .is_some_and(|question| !question.current(index, &frame.dialogs))
        {
            self.close_question();
        }
        if self
            .actions
            .get_mut(&index)
            .is_some_and(|request| request.observe(&frame))
        {
            self.actions.remove(&index);
        }
        self.queue.observe(index, &frame);
        let draft = self.drafts.entry(DraftKey::Session(index)).or_default();
        draft.observe(
            &frame.event,
            &frame.projection.phase,
            &frame.projection.connection,
        );
        let follow = self.transcript_scroll.max_offset().height + self.transcript_scroll.offset().y
            <= px(24.0);
        let (rows, observation) = present_rows(
            &mut self.order,
            &frame.session_id,
            &frame.history,
            &frame.projection.blocks,
            Some(&frame.projection),
            &self.rows,
        );
        self.rows = rows;
        self.observation = Some(observation);
        if follow {
            self.transcript_scroll.scroll_to_bottom();
        }
        self.transcript_session = Some(index);
        if std::env::var_os("RPI_GPUI_TRACE").is_some() {
            if frame.event.is_empty() {
                eprintln!("session loaded: {} transcript rows", self.rows.len());
            } else if matches!(
                frame.event.as_str(),
                "message_update"
                    | "agent_settled"
                    | "agent_start"
                    | "ui_dialog_request"
                    | "ui_dialog_closed"
                    | "connection_reconnecting"
                    | "connection_online"
            ) {
                eprintln!("session event: {}", frame.event);
            }
        }
        let final_access = frame.event.is_empty() || frame.event == "access";
        let refresh_queue =
            final_access || matches!(frame.event.as_str(), "agent_start" | "turn_end");
        let refresh_config = final_access;
        let clear_config = frame.access.is_none();
        self.frame = Some(frame);
        if refresh_queue {
            self.refresh_queue(cx);
        }
        if self.review_after_bind == Some(index) {
            if self.session_controls().send {
                self.review_after_bind = None;
                self.files.clear_review_notice();
                self.send_prompt(true, cx);
            } else if final_access {
                self.review_after_bind = None;
                self.files
                    .set_review_notice("独立会话未确认可发送，审阅尚未提交。");
            }
        }
        if self
            .send_after_bind
            .as_ref()
            .is_some_and(|(pending, _)| *pending == index)
            && (final_access || self.session_controls().send)
        {
            let send = self.send_after_bind.take();
            if self.session_controls().send
                && send.is_some_and(|(_, send)| {
                    self.drafts
                        .get(&DraftKey::Session(index))
                        .is_some_and(|draft| send.matches(draft))
                })
            {
                self.send_prompt(false, cx);
            }
        }
        if refresh_config {
            self.observe_config(cx);
        } else if clear_config {
            self.composer_config.reset_observation();
        } else {
            self.composer_config
                .close_if_locked(&self.session_controls());
        }
    }

    fn pending_session_requests(&self) -> PendingRequests {
        let draft = self.drafts.get(&self.draft_key());
        let mut pending = draft.map(Draft::pending).unwrap_or_default();
        pending.submitting |= self.files.checking_review()
            || self.session_management.blocks_session(self.session)
            || self.session_fork.blocks_session(self.session);
        if let Some(request) = self.session.and_then(|index| self.actions.get(&index)) {
            request.apply_pending(&mut pending);
        }
        if let Some(index) = self.session {
            self.composer_config.pending(index, &mut pending);
            self.queue.pending(index, &mut pending);
        }
        pending
    }

    fn session_controls(&self) -> SessionControls {
        let config = self.composer_config.observed.as_ref();
        controls(&SessionObservation {
            projection: self.frame.as_ref().map(|frame| &frame.projection),
            access: self.frame.as_ref().and_then(|frame| frame.access.as_ref()),
            configuration: config.map(|config| &config.schema),
            runtime: config.map(|config| &config.runtime),
            has_dialogs: self
                .frame
                .as_ref()
                .is_some_and(|frame| !frame.dialogs.is_empty()),
            pending_resume: self
                .frame
                .as_ref()
                .is_some_and(|frame| frame.pending_resume),
            pending: self.pending_session_requests(),
        })
    }

    fn welcome_can_send(&self) -> bool {
        self.session.is_none()
            && self.can_create_session()
            && self
                .creation_readback
                .as_ref()
                .is_none_or(|(project, _)| self.project == Some(*project))
            && self
                .drafts
                .get(&self.draft_key())
                .is_some_and(Draft::can_send)
    }

    fn send(&mut self, cx: &mut Context<Self>) {
        if self.welcome_can_send() {
            let draft = &self.drafts[&self.draft_key()];
            self.create_session(
                SessionCreation::Send(WelcomeSend {
                    revision: draft.input.revision,
                    message: draft.message(),
                }),
                cx,
            );
        } else if self.can_enqueue(&self.session_controls()) {
            self.enqueue(cx);
        } else {
            self.send_prompt(false, cx);
        }
    }

    fn send_prompt(&mut self, review: bool, cx: &mut Context<Self>) {
        let Some((node, index)) = self.node.as_ref().zip(self.session) else {
            return;
        };
        let Some(frame) = self.frame.as_ref().filter(|_| self.session_controls().send) else {
            return;
        };
        let draft = self.drafts.entry(DraftKey::Session(index)).or_default();
        if !review && !draft.can_send() {
            return;
        }
        let message = if review {
            work_viewmodel::diff_view::WORKTREE_REVIEW_PROMPT.into()
        } else {
            draft.message()
        };
        let id = frame.session_id.clone();
        let request = if !review && !draft.attachments.is_empty() {
            if !frame
                .access
                .as_ref()
                .is_some_and(|access| access.supports_prompt_content())
            {
                draft.attachment_notice = Some("当前会话尚未确认支持附件。".into());
                cx.notify();
                return;
            }
            let blocks = match attachments::content_blocks(message, &draft.attachments) {
                Ok(blocks) => blocks,
                Err(error) => {
                    draft.attachment_notice = Some(error);
                    cx.notify();
                    return;
                }
            };
            draft.attachment_notice = None;
            node.prompt_content(id, blocks)
        } else {
            node.prompt(id, message)
        };
        if self
            .actions
            .get(&index)
            .is_some_and(|request| !request.locked())
        {
            self.actions.remove(&index);
        }
        self.queue.clear_notice(index);
        draft.begin(!review);
        cx.spawn(async move |this, cx| {
            let result = request.await;
            this.update(cx, |shell, cx| {
                let draft = shell.drafts.entry(DraftKey::Session(index)).or_default();
                match result {
                    Ok(Ok(reply)) => match command_admission(reply, Some("prompt"), false) {
                        Some(Ok(())) => draft.accept(),
                        Some(Err(reason)) => draft.reject(reason),
                        None => draft.unknown(None),
                    },
                    Ok(Err(ClientError::OperationOutcomeUnknown { operation })) => {
                        draft.unknown(Some(operation))
                    }
                    Ok(Err(error)) if rejected(&error) => draft.reject(error.to_string()),
                    _ => draft.unknown(None),
                }
                if std::env::var_os("RPI_GPUI_TRACE").is_some() {
                    eprintln!(
                        "prompt acknowledgement: {}",
                        if draft.pending().uncertain {
                            "unknown"
                        } else if draft.notice.is_some() {
                            "rejected"
                        } else {
                            "accepted"
                        }
                    );
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn check_receipt(&mut self, cx: &mut Context<Self>) {
        let Some((node, index)) = self.node.as_ref().zip(self.session) else {
            return;
        };
        if !self.session_controls().online || self.checking_receipts.contains(&index) {
            return;
        }
        let Some(operation) = self
            .drafts
            .get(&DraftKey::Session(index))
            .and_then(Draft::receipt)
            .cloned()
        else {
            return;
        };
        self.checking_receipts.insert(index);
        let task = node.receipt(operation);
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |shell, cx| {
                shell.checking_receipts.remove(&index);
                let draft = shell.drafts.entry(DraftKey::Session(index)).or_default();
                match result
                    .ok()
                    .and_then(Result::ok)
                    .and_then(|status| interpret_receipt(status, Some("prompt"), false))
                {
                    Some(Ok(())) => draft.accept(),
                    Some(Err(reason)) => draft.reject(reason),
                    None => draft.notice = Some("暂无可验证的发送回执，草稿已保留".into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn toggle_tool(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(id) {
            self.expanded.insert(id.to_string());
        }
        cx.notify();
    }

    // ---- render helpers -------------------------------------------------

    fn text(&self, value: impl Into<SharedString>, size: f32, color: Hsla) -> Div {
        div()
            .text_size(px(size))
            .text_color(color)
            .child(value.into())
    }

    fn title(&self) -> &str {
        self.session
            .and_then(|index| self.snapshot.sessions.get(index))
            .map(|session| session.title.as_str())
            .filter(|title| !title.is_empty())
            .or_else(|| {
                self.project
                    .map(|index| self.snapshot.projects[index].name.as_str())
            })
            .unwrap_or("工作台")
    }

    fn button(&self, id: impl Into<gpui::ElementId>, label: &str) -> gpui::Stateful<Div> {
        let palette = self.palette;
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(px(28.0))
            .px(px(8.0))
            .rounded(px(6.0))
            .cursor_pointer()
            .hover(move |style| style.bg(palette.hover))
            .when(!label.is_empty(), |button| {
                button.child(self.text(label.to_owned(), 12.0, palette.muted))
            })
    }

    fn icon_button(
        &self,
        id: &'static str,
        shape: &'static str,
        width: f32,
    ) -> gpui::Stateful<Div> {
        self.button(id, "")
            .w(px(width))
            .child(icon(shape, 16.0, self.palette.muted))
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(275.0))
            .h_full()
            .min_h(px(0.0))
            .px(px(9.0))
            .bg(palette.side)
            .border_r_1()
            .border_color(palette.line)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(46.0))
                    .pl(px(7.0))
                    .flex_none()
                    .child(
                        self.text("RPI Work", 16.0, palette.text)
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .child(
                        div()
                            .relative()
                            .flex()
                            .items_center()
                            .child(
                                self.icon_button("workbench-search-button", "search", 25.0)
                                    .on_click(cx.listener(|shell, _, window, cx| {
                                        shell.open_workbench_search(window, cx)
                                    })),
                            )
                            .when(self.node.is_some(), |tools| {
                                tools.child(self.session_filter_button(cx))
                            })
                            .child(
                                self.icon_button("fold-navigation", "panel-left", 25.0)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.close_navigation(cx)),
                                    ),
                            )
                            .children(
                                self.session_filter_controls(cx)
                                    .map(|menu| gpui::deferred(menu).with_priority(1)),
                            ),
                    ),
            )
            .when(self.node.is_some() && self.project.is_some(), |sidebar| {
                let enabled = self.can_create_session();
                let label = if self.creating_session.is_some() {
                    "正在读取…"
                } else if self.creation_readback.is_some() {
                    "核对新会话"
                } else {
                    "新对话"
                };
                sidebar
                    .child(
                        self.button("new-session", "")
                            .justify_start()
                            .gap(px(8.0))
                            .h(px(30.0))
                            .child(icon("compose", 15.0, palette.text))
                            .child(
                                self.text(label, 14.0, palette.text)
                                    .font_weight(FontWeight::MEDIUM),
                            )
                            .when(enabled, |button| {
                                button.on_click(cx.listener(|shell, _, window, cx| {
                                    shell.close_resource_page();
                                    shell.focus_composer(window);
                                    shell.create_session(SessionCreation::New, cx);
                                }))
                            })
                            .when(!enabled, |button| button.opacity(0.6).cursor_default()),
                    )
                    .when(self.session.is_some(), |sidebar| {
                        sidebar.when_some(self.creation_error.as_ref(), |sidebar, (_, error)| {
                            sidebar.child(self.text(error.clone(), 12.0, palette.danger).p_2())
                        })
                    })
            })
            .child(
                div()
                    .id("navigation-scroll")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .pt(px(16.0))
                    .child(self.navigation_lists(cx)),
            )
            .when(self.node.is_some(), |sidebar| {
                sidebar.child(self.archives_button(cx))
            })
            .when(self.overlay == Some(Overlay::Navigation), |sidebar| {
                sidebar
                    .child(
                        self.button("mobile-resources", "")
                            .h(px(44.0))
                            .gap_2()
                            .justify_start()
                            .tab_index(0)
                            .tab_stop(!self.settings.open && self.navigation_keyboard_enabled())
                            .focus(|button| button.bg(palette.subtle))
                            .child(icon("blocks", 16.0, palette.muted))
                            .child(self.text("技能与插件", 13.0, palette.text))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.open_resource_page(window, cx);
                            })),
                    )
                    .child(
                        self.button("mobile-settings", "设置")
                            .gap_2()
                            .justify_start()
                            .child(icon("settings", 16.0, palette.muted))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.overlay = None;
                                shell.open_settings(window, cx);
                            })),
                    )
            })
            .when(self.node.is_none(), |sidebar| {
                sidebar.child(
                    self.text(self.snapshot.mode.clone(), 11.0, palette.muted)
                        .px(px(10.0))
                        .py(px(12.0)),
                )
            })
    }

    fn session_row(
        &self,
        index: usize,
        session: &crate::data::SessionItem,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let palette = self.palette;
        let selected = self.session == Some(index);
        let title = session.display_title();
        let label = self
            .snapshot
            .projects
            .iter()
            .find(|project| project.id == session.project_id)
            .map_or_else(
                || title.to_owned(),
                |project| format!("{} · {title}", project.name),
            );
        self.navigation_tooltip(
            div()
                .id(("session", index))
                .tab_index(0)
                .tab_stop(self.navigation_keyboard_enabled())
                .focus(|row| row.bg(palette.hover))
                .flex()
                .items_center()
                .h(px(30.0))
                .pl(px(11.0))
                .pr(px(10.0))
                .rounded(px(10.0))
                .cursor_pointer()
                .hover(move |style| style.bg(palette.hover))
                .when(selected, |row| {
                    row.bg(palette.hover).font_weight(FontWeight::MEDIUM)
                })
                .child(
                    self.text(
                        title.to_owned(),
                        14.0,
                        if selected {
                            palette.text
                        } else {
                            palette.muted
                        },
                    )
                    .flex_1()
                    .truncate(),
                )
                .when(session.pinned, |row| {
                    row.child(icon("pin", 13.0, palette.muted))
                }),
            label,
        )
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.select_session(index, cx);
            shell.focus_composer(window);
        }))
    }

    fn close_navigation(&mut self, cx: &mut Context<Self>) {
        self.project_menu = None;
        self.close_session_filter_menu();
        if self.overlay == Some(Overlay::Navigation) {
            self.overlay = None;
        } else {
            self.navigation_open = false;
        }
        cx.notify();
    }

    fn toggle_navigation(&mut self, narrow: bool, cx: &mut Context<Self>) {
        self.close_session_filter_menu();
        if narrow {
            self.overlay =
                (self.overlay != Some(Overlay::Navigation)).then_some(Overlay::Navigation);
        } else {
            self.navigation_open = !self.navigation_open;
        }
        cx.notify();
    }

    fn browser_panel_tab(&self) -> project_files::Tab {
        if self.files.tab.browser_id().is_some() {
            self.files.tab
        } else {
            project_files::Tab::Overview
        }
    }

    fn handle_workbench_shortcut(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(shortcut) = WorkbenchShortcut::from_keystroke(&event.keystroke) else {
            return false;
        };
        if !self.navigation_keyboard_enabled() || self.composer_is_marked() {
            return false;
        }
        if event.is_held {
            return true;
        }
        let width = window.viewport_size().width;
        match shortcut {
            WorkbenchShortcut::NewConversation => self.open_new_conversation(window, cx),
            WorkbenchShortcut::Search => self.open_workbench_search(window, cx),
            WorkbenchShortcut::Navigation => {
                self.toggle_navigation(width <= px(700.0), cx);
            }
            WorkbenchShortcut::Browser => {
                self.toggle_inspector(self.browser_panel_tab(), width <= px(900.0), window, cx);
            }
            WorkbenchShortcut::Settings => self.open_settings(window, cx),
        }
        true
    }

    fn open_new_conversation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.return_from_resources(window, cx);
        self.dismiss_drawer(window);
        self.inspector_full = false;
        if let Some(project) = self.project {
            self.open_project_welcome(project, window, cx);
        } else {
            self.focus_composer(window);
            cx.notify();
        }
    }

    fn dismiss_drawer(&mut self, window: &mut Window) -> bool {
        let width = window.viewport_size().width;
        let visible = match self.overlay {
            Some(Overlay::Navigation) => width <= px(700.0),
            Some(Overlay::Inspector) => width <= px(900.0),
            None => false,
        };
        if visible {
            self.overlay = None;
            if self.resource_page_open() {
                self.search.focus.focus(window);
            } else {
                self.focus_composer(window);
            }
        }
        visible
    }

    fn toggle_inspector(
        &mut self,
        tab: project_files::Tab,
        narrow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let returning = self.return_from_resources(window, cx);
        let same = self.files.tab == tab;
        if narrow {
            self.overlay =
                (!(same && self.overlay == Some(Overlay::Inspector))).then_some(Overlay::Inspector);
        } else {
            self.overview_open = returning || !same || !self.overview_open;
        }
        self.show_project_tab(tab, cx);
        cx.notify();
    }

    fn rail(&self, inspector_overlay: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let dock_button = |tab, id, shape, cx: &mut Context<Self>| {
            self.icon_button(id, shape, 34.0)
                .h(px(34.0))
                .rounded(px(9.0))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.toggle_inspector(tab, inspector_overlay, window, cx)
                }))
        };
        div()
            .flex()
            .flex_col()
            .items_center()
            .flex_none()
            .w(px(52.0))
            .py(px(8.0))
            .gap(px(5.0))
            .bg(palette.titlebar)
            .child(
                self.icon_button("rail-conversation", "terminal", 34.0)
                    .h(px(34.0))
                    .when(!self.resource_page_open(), |button| {
                        button.bg(palette.hover)
                    })
                    .on_click(cx.listener(|shell, _, window, cx| {
                        if !shell.return_from_resources(window, cx) {
                            shell.focus_composer(window);
                        }
                    })),
            )
            .child(
                self.icon_button("rail-navigation", "folder", 34.0)
                    .h(px(34.0))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_navigation(false, cx))),
            )
            .child(
                self.navigation_tooltip(
                    self.icon_button("rail-resources", "blocks", 34.0)
                        .h(px(34.0))
                        .when(self.resource_page_open(), |button| button.bg(palette.hover))
                        .tab_index(0)
                        .tab_stop(!self.settings.open && self.navigation_keyboard_enabled())
                        .focus(|button| button.bg(palette.subtle))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.open_resource_page(window, cx);
                        })),
                    "技能与插件",
                ),
            )
            .child(div().w(px(22.0)).h(px(1.0)).my(px(5.0)).bg(palette.line))
            .child(dock_button(
                project_files::Tab::Overview,
                "rail-project",
                "globe",
                cx,
            ))
            .child(dock_button(
                project_files::Tab::Files,
                "rail-files",
                "code-xml",
                cx,
            ))
            .child(div().flex_1())
            .child(
                self.icon_button("settings", "settings", 34.0)
                    .h(px(34.0))
                    .on_click(cx.listener(|this, _, window, cx| this.open_settings(window, cx))),
            )
    }

    fn header(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let compact = width <= 700.0;
        let overlay = width <= 900.0;
        div()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(if compact { 44.0 } else { 46.0 }))
            .bg(palette.titlebar)
            .pr(px(if compact { 5.0 } else { 12.0 }))
            .pl(px(if cfg!(target_os = "macos") {
                75.0
            } else {
                12.0
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .children(
                        [
                            (
                                false,
                                "session-back",
                                "history-back",
                                self.history_target(false).is_some(),
                            ),
                            (
                                true,
                                "session-forward",
                                "history-forward",
                                self.history_target(true).is_some(),
                            ),
                        ]
                        .into_iter()
                        .filter(|_| !compact)
                        .map(|(forward, id, shape, enabled)| {
                            self.icon_button(id, shape, 24.0)
                                .when(!enabled, |button| button.opacity(0.35).cursor_default())
                                .when(enabled, |button| {
                                    button.on_click(cx.listener(move |this, _, window, cx| {
                                        this.step_history(forward, window, cx)
                                    }))
                                })
                        }),
                    )
                    .child(self.icon_button("navigation", "panel-left", 28.0).on_click(
                        cx.listener(move |this, _, _, cx| this.toggle_navigation(compact, cx)),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .left(px(if compact { 112.0 } else { width * 0.22 }))
                    .right(px(if compact { 100.0 } else { width * 0.22 }))
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .min_w(px(0.0))
                    .gap_1()
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .window_control_area(WindowControlArea::Drag),
                    )
                    .child(
                        self.text(
                            if self.resource_page_open() {
                                "技能与插件".to_owned()
                            } else {
                                self.title().to_owned()
                            },
                            14.0,
                            palette.text,
                        )
                        .font_weight(FontWeight::MEDIUM)
                        .truncate(),
                    )
                    .when(
                        !self.resource_page_open() && self.node.is_some() && self.session.is_some(),
                        |header| header.child(self.session_actions_button(cx)),
                    ),
            )
            .when(self.resource_page_open(), |header| {
                header.child(
                    self.button("resource-return", "返回对话")
                        .tab_index(0)
                        .focus(|button| button.bg(palette.hover))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.return_from_resources(window, cx);
                        })),
                )
            })
            .when(!self.resource_page_open(), |header| {
                header.child(
                    div().flex().items_center().gap(px(4.0)).children(
                        [
                            (
                                project_files::Tab::Overview,
                                "header-overview",
                                "globe",
                                "浏览器",
                            ),
                            (
                                project_files::Tab::Files,
                                "header-files",
                                "code-xml",
                                "代码",
                            ),
                            (
                                project_files::Tab::Changes,
                                "header-changes",
                                "file-diff",
                                "变更",
                            ),
                        ]
                        .into_iter()
                        .map(|(tab, id, shape, label)| {
                            let tab = if tab == project_files::Tab::Overview {
                                self.browser_panel_tab()
                            } else {
                                tab
                            };
                            let active = self.files.tab == tab
                                && if overlay {
                                    self.overlay == Some(Overlay::Inspector)
                                } else {
                                    self.overview_open
                                };
                            self.button(id, "")
                                .gap(px(6.0))
                                .child(icon(shape, 15.0, palette.muted))
                                .when(!compact, |button| {
                                    button.child(self.text(label, 12.0, palette.muted))
                                })
                                .when_some(
                                    (tab == project_files::Tab::Changes && width >= 1200.0)
                                        .then(|| self.files.line_totals())
                                        .flatten(),
                                    |button, counts| button.child(self.git_line_counts(counts)),
                                )
                                .when(active, |button| button.bg(palette.subtle))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.toggle_inspector(tab, overlay, window, cx)
                                }))
                        }),
                    ),
                )
            })
    }

    fn inspector_divider(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("inspector-divider")
            .absolute()
            .left(px(-4.0))
            .top_0()
            .bottom_0()
            .w(px(8.0))
            .cursor(CursorStyle::ResizeLeftRight)
            .flex()
            .justify_center()
            .hover(|style| style.bg(self.palette.line.opacity(0.5)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                    if event.click_count == 2 {
                        this.inspector_width = None;
                        this.resizing_inspector = false;
                        cx.notify();
                    } else {
                        this.resizing_inspector = true;
                    }
                }),
            )
    }

    fn overview(&self, width: f32, overlay: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let project = self
            .project
            .and_then(|index| self.snapshot.projects.get(index));
        let name = project.map_or("未选择项目", |project| project.name.as_str());
        let session_count = self
            .snapshot
            .sessions
            .iter()
            .filter(|session| project.is_some_and(|project| project.id == session.project_id))
            .count();
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(width))
            .h_full()
            .bg(palette.bg)
            .border_l_1()
            .border_color(palette.line)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(46.0))
                    .px(px(8.0))
                    .border_b_1()
                    .border_color(palette.line)
                    .child(
                        div()
                            .id("inspector-tab-scroll")
                            .flex()
                            .items_center()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_x_scroll()
                            .child(self.project_tabs(cx))
                            .child(self.browser_tabs(cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .when(!overlay, |row| {
                                row.child(
                                    self.icon_button("inspector-full", "panel-right", 26.0)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.inspector_full = !this.inspector_full;
                                            this.focus_composer(window);
                                            cx.notify();
                                        })),
                                )
                            })
                            .child(self.icon_button("close-overview", "x", 26.0).on_click(
                                cx.listener(move |this, _, _, cx| {
                                    if overlay {
                                        this.overlay = None;
                                    } else {
                                        this.overview_open = false;
                                    }
                                    cx.notify();
                                }),
                            )),
                    ),
            )
            .child(if let Some(id) = self.files.tab.browser_id() {
                self.browser_contents(id)
            } else if self.files.is_overview() {
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p(px(28.0))
                    .child(
                        self.text(self.snapshot.mode.clone(), 11.0, palette.muted)
                            .truncate(),
                    )
                    .child(
                        self.text(name.to_owned(), 26.0, palette.text)
                            .font_weight(FontWeight::BOLD)
                            .truncate(),
                    )
                    .when(project.is_some(), |panel| {
                        panel.child(self.text(
                            format!("{session_count} 个会话"),
                            13.0,
                            palette.muted,
                        ))
                    })
                    .when_some(self.session, |panel, _| {
                        panel
                            .child(div().mt(px(13.0)).border_t_1().border_color(palette.line))
                            .child(self.text("当前会话", 12.0, palette.muted))
                            .child(
                                self.text(self.title().to_owned(), 14.0, palette.text)
                                    .truncate(),
                            )
                    })
                    .into_any_element()
            } else {
                self.project_contents(cx)
            })
    }

    fn transcript(
        &self,
        inset: f32,
        reading_width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.palette;
        if self.session.is_none()
            && let Some(project) = self.project
        {
            return self.project_welcome(project, cx).into_any_element();
        }
        let font_size = self.preferences.font_size();
        let classic = self.preferences.classic;
        let prompt_width = (reading_width * 0.9).min((reading_width * 0.77).max(320.0));
        let show_rows = self.session.is_some() && self.session == self.transcript_session;
        let fork_replies = show_rows && self.can_fork() && self.supports_reply_fork();
        let empty_text = match (self.watcher.is_some() && self.frame.is_none(), self.session) {
            (true, _) => "正在读取会话…",
            (false, Some(_)) => "此会话暂无消息",
            (false, None) => "从侧栏选择会话",
        };
        let rows =
            self.rows
                .iter()
                .enumerate()
                .filter(|_| show_rows)
                .map(|(index, row)| match row {
                    Row::User(text) => div()
                        .flex()
                        .flex_col()
                        .when(!classic, |row| row.items_end())
                        .child(
                            self.text(text.clone(), font_size, palette.text)
                                .line_height(px(if self.preferences.compact {
                                    font_size * 1.4
                                } else {
                                    font_size * 1.55
                                }))
                                .when(!classic, |text| {
                                    text.max_w(px(prompt_width))
                                        .px(px(15.0))
                                        .py(px(10.0))
                                        .rounded(px(19.0))
                                        .bg(palette.subtle)
                                })
                                .when(classic, |text| text.font_weight(FontWeight::MEDIUM)),
                        )
                        .into_any_element(),
                    Row::Assistant {
                        key,
                        view,
                        fork_entry_id,
                        ..
                    } => div()
                        .child(self.rich_answer(key, view))
                        .children(
                            fork_entry_id
                                .as_ref()
                                .filter(|_| fork_replies)
                                .map(|entry| self.fork_reply_button(key, entry, cx)),
                        )
                        .into_any_element(),
                    Row::Tool(block) => self.tool_card(index, block, cx),
                });
        div()
            .id("transcript")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .track_scroll(&self.transcript_scroll)
            .px(px(inset))
            .pt(px(28.0))
            .pb(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(if self.preferences.compact { 14.0 } else { 22.0 }))
                    .w(px(reading_width))
                    .mx_auto()
                    .children(rows)
                    .when_some(self.load_error.as_ref(), |column, error| {
                        column
                            .child(self.text(
                                format!("读取会话失败：{error}"),
                                14.0,
                                palette.danger,
                            ))
                            .child(self.button("retry", "重新读取").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.change_session(this.session, cx);
                                },
                            )))
                    })
                    .when(
                        self.load_error.is_none() && (!show_rows || self.rows.is_empty()),
                        |column| column.child(self.text(empty_text, 14.0, palette.muted)),
                    ),
            )
            .into_any_element()
    }

    fn tool_card(&self, index: usize, block: &ToolView, cx: &mut Context<Self>) -> AnyElement {
        let palette = &self.palette;
        let ToolView {
            id,
            name,
            arguments,
            output,
            status,
            ..
        } = block;
        let open = self.expanded.contains(id);
        let target = tool_display::target(name, arguments);
        let glyph = match tool_display::icon(name) {
            "read" => "▤",
            "shell" => "❯",
            "edit" | "write" => "✎",
            "search" => "⌕",
            _ => "•",
        };
        let status_color = match *status {
            "completed" => palette.success,
            "failed" => palette.danger,
            _ => palette.muted,
        };
        let detail = if open {
            let mut detail = div()
                .flex()
                .flex_col()
                .gap_2()
                .pt(px(8.0))
                .border_1()
                .border_color(palette.line)
                .child(self.text(tool_detail_label(name), 11.0, palette.muted))
                .child(self.text(arguments.clone(), 12.0, palette.plain));
            if tool_display::is_read(name) {
                detail = detail
                    .child(self.text("工具结果", 11.0, palette.muted))
                    .child(self.code_block(output, &target));
            } else {
                detail = detail
                    .child(self.text("工具结果", 11.0, palette.muted))
                    .child(self.text(output.clone(), 12.0, palette.plain));
            }
            Some(detail)
        } else {
            None
        };
        div()
            .id(("tool", index))
            .flex()
            .flex_col()
            .rounded(px(6.0))
            .cursor_pointer()
            .hover({
                let background = palette.subtle;
                move |style| style.bg(background)
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(28.0))
                    .px(px(6.0))
                    .gap_2()
                    .child(self.text(if open { "⌄" } else { "›" }, 13.0, palette.muted))
                    .child(self.text(glyph, 12.0, palette.muted))
                    .child(self.text(name.clone(), 12.0, palette.muted))
                    .child(
                        self.text(target, 12.0, palette.muted)
                            .min_w(px(0.0))
                            .truncate(),
                    )
                    .child(
                        div()
                            .size(px(5.0))
                            .flex_none()
                            .rounded(px(3.0))
                            .bg(status_color),
                    ),
            )
            .children(detail)
            .on_click(cx.listener({
                let id = id.clone();
                move |this, _, _, cx| this.toggle_tool(&id, cx)
            }))
            .into_any_element()
    }

    /// Code lines with lexer tokens from `work-viewmodel::conversation::code_tokens`.
    fn code_block(&self, source: &str, path: &str) -> impl IntoElement {
        let palette = &self.palette;
        let lines: Vec<_> = code_tokens::tokenize_file(source, path)
            .into_iter()
            .map(|line| {
                let spans: Vec<_> = line
                    .into_iter()
                    .map(|token| {
                        self.text(
                            token.text.to_string(),
                            12.5,
                            palette.code_token(token.class),
                        )
                    })
                    .collect();
                div().flex().flex_row().gap_1().children(spans)
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .p(px(8.0))
            .rounded(px(6.0))
            .bg(palette.bg)
            .border_1()
            .border_color(palette.line)
            .font_family("Menlo")
            .children(lines)
    }

    fn composer(&self, inset: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let draft = self.drafts.get(&self.draft_key());
        let loaded_sample = self.node.is_none()
            && self.session.is_some()
            && self.session == self.transcript_session;
        let runtime = self
            .composer_config
            .observed
            .as_ref()
            .map(|observed| &observed.runtime);
        let live_metrics = work_viewmodel::observability::metrics::Metrics::session(
            runtime.and_then(|runtime| runtime.stats.as_ref()),
        );
        let live_context = work_viewmodel::observability::metrics::Context::reported(
            runtime.and_then(|runtime| runtime.context.as_ref()),
        );
        let (metrics, context) = if loaded_sample {
            (&self.snapshot.metrics, &self.snapshot.context)
        } else {
            (&live_metrics, &live_context)
        };
        let controls = self.session_controls();
        let welcome = self.node.is_some() && self.project.is_some() && self.session.is_none();
        let can_send = self.welcome_can_send()
            || controls.send
                && draft.is_some_and(|draft| {
                    draft.can_send()
                        && (draft.attachments.is_empty() || self.supports_attachments())
                });
        let checking = self
            .session
            .is_some_and(|index| self.checking_receipts.contains(&index));
        let can_check = controls.online && !checking && draft.and_then(Draft::receipt).is_some();
        let action = self.session.and_then(|index| self.actions.get(&index));
        let notice = action
            .map(session_action::Request::status)
            .or_else(|| draft.and_then(|draft| draft.notice.as_deref()))
            .or_else(|| self.session.and_then(|index| self.queue.notice(index)))
            .or_else(|| {
                draft
                    .filter(|draft| draft.feedback_over_limit())
                    .map(|_| "消息和行内反馈合计超过长度上限")
            })
            .or_else(|| draft.and_then(|draft| draft.attachment_notice.as_deref()))
            .or_else(|| match welcome {
                true if self.creating_session.is_some() => Some("正在创建并核对新会话…"),
                true => self
                    .creation_error
                    .as_ref()
                    .map(|(_, error)| error.as_str()),
                false => self.config_notice(),
            })
            .or_else(|| {
                (self.queue_active() && draft.is_some_and(|draft| !draft.attachments.is_empty()))
                    .then_some("带附件的消息可在当前运行结算后发送。")
            });
        let status = if checking {
            Some("正在核对发送回执…")
        } else {
            notice.or_else(|| {
                if self.node.is_none() {
                    Some("示例 · 草稿保存在当前窗口")
                } else if welcome {
                    None
                } else if controls.online
                    && self
                        .frame
                        .as_ref()
                        .is_some_and(|frame| !frame.dialogs.is_empty())
                {
                    Some("等待审批回复")
                } else {
                    (!controls.reason.is_empty()).then_some(controls.reason.as_str())
                }
            })
        };
        div()
            .flex_none()
            .bg(palette.bg)
            .px(px(inset))
            .pt(px(4.0))
            .pb(px(14.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .max_w(px(READING_WIDTH))
                    .mx_auto()
                    .children(self.fork_status(cx))
                    .child(self.approvals(&controls, cx))
                    .child(self.queue_panel(&controls, cx))
                    .child(
                        div()
                            .id("composer")
                            .flex()
                            .flex_col()
                            .bg(palette.bg)
                            .rounded(px(22.0))
                            .border_1()
                            .border_color(palette.strong_line)
                            .px(px(10.0))
                            .pt(px(12.0))
                            .pb(px(9.0))
                            .when_some(draft, |composer, draft| {
                                composer
                                    .child(self.attachment_chips(&draft.attachments, cx))
                                    .child(self.feedback_chips(&draft.feedback, cx))
                            })
                            .when(
                                self.comment.is_none()
                                    && !self.project_form.open
                                    && self.question.is_none()
                                    && !self.session_management.open(),
                                |composer| composer.child(self.input_element(cx)),
                            )
                            .when(
                                self.comment.is_some()
                                    || self.project_form.open
                                    || self.question.is_some()
                                    || self.session_management.open(),
                                |composer| composer.child(div().h(px(47.0))),
                            )
                            .when_some(status, |composer, status| {
                                composer.child(
                                    self.text(status.to_owned(), 12.0, palette.muted)
                                        .px(px(4.0))
                                        .py_1(),
                                )
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .flex_wrap()
                                    .min_h(px(28.0))
                                    .pl(px(4.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(self.add_attachments_button(cx))
                                            .child(self.config_toolbar(&controls, cx)),
                                    )
                                    .when(
                                        welcome
                                            && self.creation_readback.is_some()
                                            && self.can_create_session(),
                                        |toolbar| {
                                            toolbar.child(
                                                self.button("welcome-check-session", "核对新会话")
                                                    .on_click(cx.listener(|shell, _, _, cx| {
                                                        shell.create_session(
                                                            SessionCreation::New,
                                                            cx,
                                                        )
                                                    })),
                                            )
                                        },
                                    )
                                    .when(can_check, |toolbar| {
                                        toolbar.child(
                                            self.button("check-receipt", "核对回执").on_click(
                                                cx.listener(|this, _, _, cx| {
                                                    this.check_receipt(cx)
                                                }),
                                            ),
                                        )
                                    })
                                    .when(
                                        action.is_some_and(session_action::Request::can_check)
                                            && controls.online,
                                        |bar| {
                                            bar.child(
                                                self.button("check-control", "核对操作回执")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.check_control(cx)
                                                    })),
                                            )
                                        },
                                    )
                                    .child(self.queue_toolbar(&controls, cx))
                                    .child(self.run_button(can_send, &controls, cx)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .h(px(32.0))
                            .px(px(6.0))
                            .gap_2()
                            .child(
                                self.text(self.snapshot.node.clone(), 11.0, palette.muted)
                                    .truncate(),
                            )
                            .when_some(self.observation, |bar, observation| {
                                bar.child(self.text(observation, 11.0, palette.muted))
                            })
                            .when(
                                metrics.input.is_some()
                                    || metrics.output.is_some()
                                    || self.preferences.show_context && self.session.is_some(),
                                |bar| {
                                    let usage = div().flex().items_center().gap_2().min_w(px(0.0));
                                    bar.child(
                                        usage
                                            .when(
                                                metrics.input.is_some() || metrics.output.is_some(),
                                                |usage| {
                                                    usage.child(
                                                        self.text(
                                                            format!(
                                                                "↑ {}   ↓ {}",
                                                                tokens(metrics.input),
                                                                tokens(metrics.output)
                                                            ),
                                                            11.0,
                                                            palette.muted,
                                                        )
                                                        .truncate(),
                                                    )
                                                },
                                            )
                                            .when(
                                                self.preferences.show_context
                                                    && self.session.is_some(),
                                                |usage| {
                                                    let label = context.percent.map_or_else(
                                                        || "上下文".into(),
                                                        |value| {
                                                            format!(
                                                                "上下文 {}",
                                                                percent(Some(value))
                                                            )
                                                        },
                                                    );
                                                    usage.child(
                                                        self.navigation_tooltip(
                                                            self.button(
                                                                "open-session-context",
                                                                &label,
                                                            ),
                                                            "查看上下文",
                                                        )
                                                        .h(px(24.0))
                                                        .tab_index(0)
                                                        .focus(|button| button.bg(palette.hover))
                                                        .on_click(cx.listener(
                                                            |shell, _, window, cx| {
                                                                shell.open_session_context(
                                                                    window, cx,
                                                                )
                                                            },
                                                        )),
                                                    )
                                                },
                                            ),
                                    )
                                },
                            ),
                    ),
            )
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.traced {
            self.traced = true;
            if std::env::var_os("RPI_GPUI_TRACE").is_some() {
                let tools = self
                    .rows
                    .iter()
                    .filter(|row| matches!(row, Row::Tool(_)))
                    .count();
                eprintln!(
                    "first frame: {} rows ({} transcript + {} tools), {} projects, {} sessions, metrics input={:?} context={:?}",
                    self.rows.len(),
                    self.rows.len() - tools,
                    tools,
                    self.snapshot.projects.len(),
                    self.snapshot.sessions.len(),
                    self.snapshot.metrics.input,
                    self.snapshot.context.percent,
                );
                for row in self.rows.iter().filter(|_| self.node.is_none()) {
                    eprintln!(
                        "  row {}",
                        match row {
                            Row::User(text) => format!("user {:?}", short(text)),
                            Row::Assistant { source, .. } =>
                                format!("assistant {:?}", short(source)),
                            Row::Tool(block) => format!(
                                "tool {} {}",
                                block.name,
                                tool_display::target(&block.name, &block.arguments)
                            ),
                        }
                    );
                }
            }
        }
        self.apply_appearance(window.appearance(), false);
        let width = f32::from(window.viewport_size().width);
        let height = f32::from(window.viewport_size().height);
        let compact = width <= 700.0;
        let overlay = width <= 900.0;
        let rail = if compact { 0.0 } else { 52.0 };
        let navigation = if !compact && self.navigation_open {
            275.0
        } else {
            0.0
        };
        let remaining = width - rail - navigation;
        let dock_width = self
            .inspector_width
            .unwrap_or((width * 0.38).round())
            .clamp(320.0, (remaining - 350.0).max(320.0));
        let docked = self.overview_open && !overlay && !self.resource_page_open();
        let full = docked && self.inspector_full;
        if full
            || self.search_input_open()
            || self.session_context.open
            || self.comment.is_some()
            || self.project_form.open
            || self.question.is_some()
            || self.session_management.open()
            || overlay && self.overlay.is_some()
        {
            self.composer_config.close_menu();
        }
        let browser_visible = docked || overlay && self.overlay == Some(Overlay::Inspector);
        self.browser.hide_inactive(
            self.files.tab.browser_id().filter(|_| {
                browser_visible
                    && !self.search_input_open()
                    && !self.session_context.open
                    && self.comment.is_none()
                    && !self.project_form.open
                    && self.question.is_none()
                    && !self.session_management.open()
                    && !self.composer_config.menu_open()
            }),
            window,
            cx,
        );
        let inset = if compact { 14.0 } else { READING_INSET };
        let reading_width =
            (remaining - if docked { dock_width } else { 0.0 } - inset * 2.0).min(READING_WIDTH);
        let palette = self.palette;
        div()
            .id("workbench")
            .when(full && self.navigation_keyboard_enabled(), |body| {
                body.track_focus(&self.focus)
            })
            .relative()
            .on_key_down(
                cx.listener(|shell, event: &gpui::KeyDownEvent, window, cx| {
                    let key = &event.keystroke.key;
                    let modifiers = event.keystroke.modifiers;
                    if shell.handle_workbench_shortcut(event, window, cx) {
                        window.prevent_default();
                        cx.stop_propagation();
                        return;
                    }
                    if key == "tab"
                        && shell.navigation_keyboard_enabled()
                        && !modifiers.platform
                        && !modifiers.alt
                        && !modifiers.control
                    {
                        if modifiers.shift {
                            window.focus_prev();
                        } else {
                            window.focus_next();
                        }
                        shell.reveal_resource_focus(window);
                        cx.stop_propagation();
                    }
                    if key == "escape"
                        && !modifiers.modified()
                        && !shell.composer_is_marked()
                        && (shell.dismiss_project_form()
                            || shell.close_session_management()
                            || shell.close_question()
                            || shell.cancel_comment()
                            || shell.composer_config.close_menu()
                            || shell.dismiss_session_filter_menu(window)
                            || shell.project_menu.take().is_some()
                            || std::mem::take(&mut shell.welcome_project_picker)
                            || shell.dismiss_drawer(window)
                            || shell.return_from_resources(window, cx))
                    {
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
            .flex()
            .flex_col()
            .size(relative(1.0))
            .bg(palette.bg)
            .child(self.header(width, cx))
            .child(
                div()
                    .id("workbench-body")
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .when(!compact, |body| body.child(self.rail(overlay, cx)))
                    .when(navigation > 0.0, |body| body.child(self.sidebar(cx)))
                    .when_some(self.resource_page, |body, tab| {
                        body.child(self.resource_page(tab, remaining, window, cx))
                    })
                    .when(!full && !self.resource_page_open(), |body| {
                        body.child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w(px(0.0))
                                .child(self.transcript(inset, reading_width, cx))
                                .child(self.composer(inset, cx)),
                        )
                    })
                    .when(docked, |body| {
                        body.child(
                            div()
                                .relative()
                                .flex_none()
                                .h_full()
                                .child(self.overview(
                                    if full { remaining } else { dock_width },
                                    false,
                                    cx,
                                ))
                                .when(!full, |dock| dock.child(self.inspector_divider(cx))),
                        )
                    })
                    .when(
                        (compact && self.overlay == Some(Overlay::Navigation))
                            || (overlay && self.overlay == Some(Overlay::Inspector)),
                        |body| {
                            body.child(
                                div()
                                    .id("workbench-scrim")
                                    .absolute()
                                    .inset_0()
                                    .bg(gpui::rgba(0x00000028))
                                    .occlude()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_session_filter_menu();
                                        this.overlay = None;
                                        cx.notify();
                                    })),
                            )
                            .when(self.overlay == Some(Overlay::Navigation), |body| {
                                body.child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .top_0()
                                        .bottom_0()
                                        .occlude()
                                        .child(self.sidebar(cx)),
                                )
                            })
                            .when(
                                self.overlay == Some(Overlay::Inspector),
                                |body| {
                                    body.child(div().absolute().right_0().top_0().bottom_0().child(
                                        self.overview(
                                            if self.files.is_overview() {
                                                (width - 20.0).min(560.0)
                                            } else {
                                                (width - 20.0)
                                                    .min((width * 0.38).clamp(360.0, 560.0))
                                            },
                                            true,
                                            cx,
                                        ),
                                    ))
                                },
                            )
                        },
                    )
                    .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                        if this.resizing_inspector && event.dragging() {
                            this.inspector_width =
                                Some((width - f32::from(event.position.x)).round());
                            cx.notify();
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.resizing_inspector = false),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.resizing_inspector = false),
                    ),
            )
            .when_some(self.session_management_dialog(cx), |body, dialog| {
                body.child(gpui::deferred(dialog).with_priority(100))
            })
            .when_some(self.comment.as_ref(), |body, comment| {
                body.child(gpui::deferred(self.comment_dialog(comment, cx)).with_priority(100))
            })
            .when(self.project_form.open, |body| {
                body.child(gpui::deferred(self.render_project_form(cx)).with_priority(100))
            })
            .when_some(
                self.question
                    .as_ref()
                    .and_then(|question| self.question_dialog(question, cx)),
                |body, dialog| body.child(gpui::deferred(dialog).with_priority(100)),
            )
            .when(self.settings.open, |body| {
                body.child(
                    gpui::deferred(self.settings_dialog(width, height, window, cx))
                        .with_priority(110),
                )
            })
            .when(self.workbench_search.open, |body| {
                body.child(
                    gpui::deferred(self.workbench_search_dialog(window, cx)).with_priority(110),
                )
            })
            .when(self.session_context.open, |body| {
                body.child(
                    gpui::deferred(self.session_context_dialog(window, cx)).with_priority(110),
                )
            })
            .when_some(self.finish_notice.as_ref(), |body, (message, _)| {
                body.child(
                    gpui::deferred(
                        div()
                            .absolute()
                            .bottom(px(25.0))
                            .left(px(14.0))
                            .right(px(14.0))
                            .flex()
                            .justify_center()
                            .child(
                                self.text(*message, 13.0, self.palette.text)
                                    .border_1()
                                    .border_color(self.palette.strong_line)
                                    .bg(self.palette.bg)
                                    .rounded(px(8.0))
                                    .shadow_md()
                                    .px(px(14.0))
                                    .py(px(9.0)),
                            ),
                    )
                    .with_priority(120),
                )
            })
    }
}

fn rejected(error: &ClientError) -> bool {
    matches!(error, ClientError::Unsupported { .. })
        || matches!(error, ClientError::Api { status, .. }
            if matches!(status.as_u16(), 400 | 401 | 403 | 404 | 405 | 409 | 422 | 429))
}

fn present_rows(
    order: &mut TranscriptOrder,
    scope: &str,
    history: &[Value],
    blocks: &[MessageBlock],
    projection: Option<&RunProjection>,
    previous: &[Row],
) -> (Vec<Row>, &'static str) {
    let ordered = order.update(scope, history, blocks);
    let conversation = transcript_view::present(
        &ordered,
        history,
        blocks,
        projection.map(|run| &run.phase),
        projection.map(|run| &run.connection),
        Default::default(),
    );
    let previous: HashMap<_, _> = previous
        .iter()
        .filter_map(|row| match row {
            Row::Assistant {
                key, source, view, ..
            } => Some((key.as_str(), (source, view))),
            _ => None,
        })
        .collect();
    let mut rows = Vec::new();
    let mut fork_entries = HashSet::new();
    for part in conversation.parts {
        match part {
            Part::Message {
                role: "user", text, ..
            } => rows.push(Row::User(text)),
            Part::Message {
                key,
                reading_key,
                fork_entry_id,
                text,
                ..
            } => {
                let key = reading_key.unwrap_or(key);
                let view = previous
                    .get(key.as_str())
                    .filter(|(source, _)| *source == &text)
                    .map_or_else(
                        || Rc::new(markdown::present(&text)),
                        |(_, view)| Rc::clone(view),
                    );
                rows.push(Row::Assistant {
                    key,
                    source: text,
                    view,
                    fork_entry_id: fork_entry_id.filter(|entry| fork_entries.insert(entry.clone())),
                });
            }
            Part::Activity { items, .. } => {
                rows.extend(items.into_iter().filter_map(|item| match item {
                    ActivityItem::Tool { block, .. } => Some(Row::Tool(block)),
                    ActivityItem::Note { .. } => None,
                }))
            }
        }
    }
    (rows, conversation.observation)
}

fn tool_detail_label(name: &str) -> &'static str {
    if tool_display::is_read(name) {
        "调用参数 · 只读文件"
    } else if tool_display::is_shell(name) {
        "命令"
    } else {
        "调用参数"
    }
}

fn short(text: &str) -> String {
    let mut value: String = text.chars().take(24).collect();
    if text.chars().count() > 24 {
        value.push('…');
    }
    value
}
