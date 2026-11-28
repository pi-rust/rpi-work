//! Native workbench layout. The shared view model owns transcript ordering, tool
//! labels, code tokens and metrics.

#[path = "attachments.rs"]
pub(crate) mod attachments;
#[path = "browser.rs"]
mod browser;
#[path = "collaboration.rs"]
mod collaboration;
#[path = "composer_commands.rs"]
mod composer_commands;
#[path = "composer_config.rs"]
pub(crate) mod composer_config;
#[path = "desktop_plugins.rs"]
mod desktop_plugins;
#[path = "draft_storage.rs"]
mod draft_storage;
#[path = "chat_integrations.rs"]
mod chat_integrations;
#[path = "rpi_hosts.rs"]
mod rpi_hosts;
#[path = "composer.rs"]
pub(crate) mod editor;
#[path = "feature_entries.rs"]
mod feature_entries;
#[path = "file_format.rs"]
mod file_format;
#[path = "file_reference.rs"]
mod file_reference;
#[path = "message_attachments.rs"]
mod message_attachments;
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
#[path = "reply_ratings.rs"]
mod reply_ratings;
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
#[path = "session_storage.rs"]
mod session_storage;
#[path = "settings.rs"]
mod settings;
#[path = "startup.rs"]
pub(crate) mod startup;
#[path = "transcript.rs"]
mod transcript;
#[path = "search.rs"]
mod workbench_search;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, ClipboardItem, Context, CursorStyle, Div, FocusHandle, FontWeight, Hsla,
    MouseButton, MouseMoveEvent, ScrollHandle, SharedString, Task, Window, WindowControlArea, div,
    prelude::*, px, relative,
};
use rpi_extcore::reply::ReplyKind;
use serde_json::Value;
use work_client::{ClientError, LocalError};
use work_core::{MessageBlock, RunPhase, RunProjection, SettlementOutcome};
use work_viewmodel::conversation::transcript_view::{
    self, ActivityItem, ContentRef, Part, ToolView,
};
use work_viewmodel::conversation::{markdown, tool_display};
use work_viewmodel::observability::metrics::{percent, tokens};
use work_viewmodel::session_controls::{
    PendingRequests, SessionControls, SessionObservation, controls,
};
use work_viewmodel::session_receipts::{command_admission, interpret_receipt};
use work_viewmodel::transcript::TranscriptOrder;

use crate::data::{Creation, Node, Prompt, SessionFrame, Snapshot};
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

const READING_WIDTH: f32 = 720.0;
const READING_INSET: f32 = 24.0;
const PROGRESS_REPLY_GROUP: &str = "progress-reply";
const COMPACT_NAVIGATION_WIDTH: f32 = 700.0;
const EXPANDED_NAVIGATION_WIDTH: f32 = 1200.0;
const NAVIGATION_WIDTH: f32 = 275.0;
const DONATION_URL: &str = "https://rpi.laofu.online/support.html";

#[derive(Clone, Copy, PartialEq, Eq)]
enum NavigationLayout {
    Drawer,
    Collapsed,
    Expanded,
}

impl NavigationLayout {
    fn for_width(width: f32) -> Self {
        if width <= COMPACT_NAVIGATION_WIDTH {
            Self::Drawer
        } else if width < EXPANDED_NAVIGATION_WIDTH {
            Self::Collapsed
        } else {
            Self::Expanded
        }
    }
}

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
    last_attachment: Option<u64>,
    attachment_count: usize,
}

impl WelcomeSend {
    fn matches(&self, draft: &Draft) -> bool {
        draft.can_send()
            && draft.input.revision == self.revision
            && draft.message() == self.message
            && draft.last_attachment() == self.last_attachment
            && draft.attachments.len() == self.attachment_count
    }
}

enum SessionCreation {
    New,
    Send(WelcomeSend),
    Review,
}

enum CreationError {
    HostUnavailable,
    Failed(String),
    Unknown,
}

impl CreationError {
    fn message(&self) -> &str {
        match self {
            Self::HostUnavailable => "rpi 未能启动新会话。请检查默认模型及供应商凭据，然后重试。",
            Self::Failed(message) => message,
            Self::Unknown => "新建结果未知。重新连接后可查看节点会话列表。",
        }
    }
}

enum Row {
    User {
        key: String,
        durable_key: bool,
        text: String,
        attachments: Vec<ContentRef>,
    },
    Assistant {
        key: String,
        durable_key: bool,
        source: String,
        view: Rc<markdown::MarkdownView>,
        attachments: Vec<ContentRef>,
        reply: Option<ReplyActions>,
    },
    Activity {
        key: String,
        label: &'static str,
        collapsed: bool,
        attention: bool,
        items: Vec<ActivityItem>,
    },
}

struct ReplyActions {
    entry_id: Option<String>,
    parts: Vec<Rc<markdown::MarkdownView>>,
}

impl Row {
    fn key(&self) -> &str {
        match self {
            Self::User { key, .. } | Self::Assistant { key, .. } | Self::Activity { key, .. } => {
                key
            }
        }
    }

    fn has_durable_key(&self) -> bool {
        match self {
            Self::User { durable_key, .. } | Self::Assistant { durable_key, .. } => *durable_key,
            Self::Activity { items, .. } => {
                matches!(items.first(), Some(ActivityItem::Tool { block, .. }) if !block.id.is_empty())
            }
        }
    }

    fn spacing_before(&self, previous: Option<&Self>, compact: bool) -> f32 {
        let (comfortable, compact_spacing) = match (previous, self) {
            (None, _) => (0.0, 0.0),
            (Some(Self::User { .. }), Self::User { .. }) => (14.0, 10.0),
            (Some(_), Self::User { .. }) => (48.0, 28.0),
            (Some(Self::User { .. }), _) => (30.0, 20.0),
            (Some(Self::Activity { .. }), Self::Activity { .. }) => (8.0, 6.0),
            (Some(Self::Activity { .. }), Self::Assistant { .. }) => (16.0, 12.0),
            _ => (24.0, 14.0),
        };
        if compact {
            compact_spacing
        } else {
            comfortable
        }
    }
}

struct RunObservation {
    label: &'static str,
    tone: &'static str,
}

pub struct Shell {
    snapshot: Snapshot,
    node: Option<Node>,
    startup: Option<startup::Setup>,
    watcher: Option<(Task<()>, tokio::task::AbortHandle)>,
    frame: Option<SessionFrame>,
    actions: HashMap<usize, session_action::Request>,
    next_action: u64,
    checking_receipts: HashSet<usize>,
    load_error: Option<String>,
    observation: Option<RunObservation>,
    run_timing: session_context::RunTiming,
    palette: Palette,
    preferences: settings::Preferences,
    settings: settings::Panel,
    collaboration: collaboration::Panel,
    node_connection: node_connection::Form,
    node_share: node_share::Share,
    resource_page: Option<resource_settings::ResourceTab>,
    resource_inspect_focus: FocusHandle,
    resource_use_skill_focus: FocusHandle,
    resource_settings: resource_settings::Panel,
    provider_settings: provider_settings::Panel,
    chat_integrations: chat_integrations::Panel,
    rpi_hosts: rpi_hosts::Panel,
    search: editor::SearchInput,
    workbench_search: workbench_search::Panel,
    session_context: session_context::Panel,
    finish_notice: Option<(&'static str, Task<()>)>,
    queue: message_queue::Panel,
    dark: bool,
    navigation_layout: NavigationLayout,
    navigation_open: Option<bool>,
    navigation: navigation::Panel,
    overview_open: bool,
    auto_hide_empty_changes: bool,
    inspector_full: bool,
    inspector_focus: FocusHandle,
    inspector_width: Option<f32>,
    resizing_inspector: bool,
    overlay: Option<Overlay>,
    session_history: Vec<(Option<usize>, Option<usize>)>,
    history_position: usize,
    files: project_files::Panel,
    comment: Option<review::Comment>,
    project_form: project_management::Form,
    project_menu: Option<navigation::ProjectMenu>,
    welcome_project_picker: bool,
    question: Option<question::Question>,
    session_management: session_management::Panel,
    session_filter: session_filter::Panel,
    session_fork: session_fork::Panel,
    session_catalog_revision: u64,
    browser: browser::Tabs,
    composer_config: composer_config::Panel,
    composer_commands: composer_commands::Menu,
    desktop_plugins: desktop_plugins::Catalog,
    file_references: file_reference::Panel,
    creating_session: Option<Task<()>>,
    review_after_bind: Option<usize>,
    send_after_bind: Option<(usize, WelcomeSend)>,
    creation_error: Option<CreationError>,
    creation_readback: Option<(usize, String)>,
    navigation_revision: u64,
    rows: Vec<Row>,
    transcript_history: Arc<Vec<Value>>,
    project: Option<usize>,
    session: Option<usize>,
    transcript_session: Option<usize>,
    transcript_scroll: ScrollHandle,
    transcript_focus: FocusHandle,
    reading: transcript::Reading,
    _ratings_observer: gpui::Subscription,
    drafts: HashMap<DraftKey, Draft>,
    draft_storage: Option<draft_storage::Storage>,
    _draft_observer: gpui::Subscription,
    _save_on_release: gpui::Subscription,
    _save_on_quit: gpui::Subscription,
    selecting_attachments: HashSet<DraftKey>,
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
        reply_ratings::Ratings::initialize(cx);
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
            PendingRequests::default(),
            &[],
        );
        let (settings, preferences) = settings::Panel::new(cx);
        let mut shell = Self {
            session: session.filter(|_| fixture),
            transcript_session: session.filter(|_| fixture),
            transcript_scroll: ScrollHandle::new(),
            transcript_focus: cx.focus_handle(),
            project,
            transcript_history: snapshot.history.clone(),
            snapshot,
            node,
            startup: None,
            watcher: None,
            frame: None,
            actions: HashMap::new(),
            next_action: 0,
            checking_receipts: HashSet::new(),
            load_error: None,
            observation: None,
            run_timing: session_context::RunTiming::default(),
            palette: Palette::from_preset("codex", false, false),
            preferences,
            settings,
            collaboration: collaboration::Panel::default(),
            node_connection: node_connection::Form::default(),
            node_share: node_share::Share::default(),
            resource_page: None,
            resource_inspect_focus: cx.focus_handle().tab_stop(true),
            resource_use_skill_focus: cx.focus_handle().tab_stop(true),
            resource_settings: resource_settings::Panel::default(),
            provider_settings: provider_settings::Panel::default(),
            chat_integrations: chat_integrations::Panel::default(),
            rpi_hosts: rpi_hosts::Panel::default(),
            search: editor::SearchInput::new(cx),
            workbench_search: workbench_search::Panel::new(cx),
            session_context: session_context::Panel::new(cx),
            finish_notice: None,
            queue: message_queue::Panel::default(),
            dark: false,
            navigation_layout: NavigationLayout::Expanded,
            navigation_open: None,
            navigation: navigation::Panel::new(cx),
            overview_open: false,
            auto_hide_empty_changes: true,
            inspector_full: false,
            inspector_focus: cx.focus_handle(),
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
            composer_commands: composer_commands::Menu::default(),
            desktop_plugins: if fixture {
                desktop_plugins::Catalog::default()
            } else {
                desktop_plugins::Catalog::load_local()
            },
            file_references: file_reference::Panel::default(),
            creating_session: None,
            review_after_bind: None,
            send_after_bind: None,
            creation_error: None,
            creation_readback: None,
            navigation_revision: 0,
            rows,
            reading: transcript::Reading::default(),
            _ratings_observer: cx.observe_global::<reply_ratings::Ratings>(|_, cx| cx.notify()),
            drafts: HashMap::new(),
            draft_storage: (!fixture).then(draft_storage::Storage::new).flatten(),
            _draft_observer: cx.observe_self(|shell, _| shell.persist_drafts()),
            _save_on_release: cx.on_release(|shell, _| {
                shell.persist_drafts();
                shell.save_reading_state();
            }),
            _save_on_quit: cx.on_app_quit(|shell, _| {
                shell.save_reading_state();
                std::future::ready(())
            }),
            selecting_attachments: HashSet::new(),
            focus: cx.focus_handle(),
            input_layout: None,
            input_dragging: false,
            order,
            traced: false,
        };
        shell.apply_appearance(appearance, true);
        if !fixture {
            shell.change_session(session, cx);
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
            self.auto_hide_empty_changes = true;
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
            && !self.selecting_attachments.contains(&self.draft_key())
            && !self.session_fork.in_flight()
            && !matches!(self.creation_error, Some(CreationError::Unknown))
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
                                shell.move_draft(welcome_key, DraftKey::Session(index));
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
                        shell.creation_error = Some(CreationError::Failed(format!(
                            "节点已创建会话，读取详情失败：{error}"
                        )));
                        if detached_review && shell.navigation_revision == revision {
                            shell
                                .files
                                .set_review_notice("独立审阅未提交。可从左栏核对已创建的会话。");
                        }
                    }
                    Ok(Creation::WriteFailed(ClientError::Local(LocalError::Service {
                        kind: ReplyKind::HostUnavailable,
                        ..
                    }))) => {
                        shell.creation_error = Some(CreationError::HostUnavailable);
                        if detached_review && shell.navigation_revision == revision {
                            shell
                                .files
                                .set_review_notice(CreationError::HostUnavailable.message());
                        }
                    }
                    Ok(Creation::WriteFailed(error)) if rejected(&error) => {
                        shell.creation_error =
                            Some(CreationError::Failed(format!("新建对话失败：{error}")));
                        if detached_review && shell.navigation_revision == revision {
                            shell
                                .files
                                .set_review_notice(format!("创建独立审阅会话失败：{error}"));
                        }
                    }
                    _ => {
                        shell.creation_error = Some(CreationError::Unknown);
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
            self.restore_draft(self.draft_key());
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
            if let Some(index) = session {
                self.load_reading_state(index);
            }
            self.reading.switch(
                self.session,
                session,
                &self.transcript_scroll,
                self.node.is_some(),
            );
            self.transcript_scroll = ScrollHandle::new();
            if self.node.is_some() {
                self.transcript_session = None;
                self.rows.clear();
                self.transcript_history = Arc::default();
            }
        }
        self.session = session;
        self.restore_draft(self.draft_key());
        if changed && session.is_none() {
            self.auto_hide_empty_changes = true;
            if self.files.tab == project_files::Tab::Changes && self.files.changes_empty().is_some()
            {
                self.reset_project_files(cx);
            }
        }
        if self.session.is_some() {
            self.read_session_management_capabilities(cx);
        }
        self.queue.select(session, self.preferences.follow_up);
        self.composer_config.reset_observation();
        self.frame = None;
        self.observation = None;
        self.run_timing = session_context::RunTiming::default();
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
                                    shell.run_timing = session_context::RunTiming::default();
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
        self.run_timing.observe(&frame.event, Instant::now());
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
        let pending = self.pending_session_requests();
        let (rows, observation) = present_rows(
            &mut self.order,
            &frame.session_id,
            &frame.history,
            &frame.projection.blocks,
            Some(&frame.projection),
            pending,
            &self.rows,
        );
        self.rows = rows;
        self.transcript_history = frame.history.clone();
        self.observation = Some(observation);
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
                    last_attachment: draft.last_attachment(),
                    attachment_count: draft.attachments.len(),
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
        let key = DraftKey::Session(index);
        let draft = self.drafts.entry(key).or_default();
        if !review && (!draft.can_send() || self.selecting_attachments.contains(&key)) {
            return;
        }
        let message = if review {
            work_viewmodel::diff_view::WORKTREE_REVIEW_PROMPT.into()
        } else {
            draft.message()
        };
        let id = frame.session_id.clone();
        let input = if !review && !draft.attachments.is_empty() {
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
            let project_files = if draft
                .attachments
                .iter()
                .any(|file| file.project_bytes() > 0)
            {
                let Some(project) = self
                    .snapshot
                    .sessions
                    .get(index)
                    .filter(|_| node.is_local())
                    .and_then(|session| {
                        self.snapshot
                            .projects
                            .iter()
                            .find(|project| project.id == session.project_id)
                    })
                else {
                    draft.attachment_notice = Some("二进制附件需要发送到它所属的本机项目。".into());
                    cx.notify();
                    return;
                };
                Some((project.path.clone().into(), draft.attachments.clone()))
            } else {
                None
            };
            Prompt::Attachments {
                content: blocks,
                project_files,
            }
        } else {
            Prompt::Text(message)
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
        if !review && let Some(storage) = &mut self.draft_storage {
            storage.persist(key, &self.drafts, &self.queue);
        }
        let request = node.prompt(id, input);
        cx.spawn(async move |this, cx| {
            let result = request.await;
            this.update(cx, |shell, cx| {
                let draft = shell.drafts.entry(DraftKey::Session(index)).or_default();
                match result {
                    Ok(Ok(Ok(reply))) => match command_admission(reply, Some("prompt"), false) {
                        Some(Ok(())) => draft.accept(),
                        Some(Err(reason)) => draft.reject(reason),
                        None => draft.unknown(None),
                    },
                    Ok(Ok(Err(ClientError::OperationOutcomeUnknown { operation }))) => {
                        draft.unknown(Some(operation))
                    }
                    Ok(Ok(Err(error))) if rejected(&error) => draft.reject(error.to_string()),
                    Ok(Err(error)) => draft.reject(error),
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

    fn button_frame(&self, id: impl Into<gpui::ElementId>) -> gpui::Stateful<Div> {
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
    }

    fn button(&self, id: impl Into<gpui::ElementId>, label: &str) -> gpui::Stateful<Div> {
        let palette = self.palette;
        self.button_frame(id)
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
            .px_0()
            .child(icon(shape, 16.0, self.palette.muted))
    }

    fn dialog_button(
        &self,
        id: &'static str,
        label: &str,
        enabled: bool,
        primary: bool,
    ) -> gpui::Stateful<Div> {
        let palette = self.palette;
        self.button_frame(id)
            .gap(px(6.0))
            .border_1()
            .border_color(if primary { palette.text } else { palette.line })
            .bg(if primary { palette.text } else { palette.bg })
            .hover(move |button| {
                if !enabled {
                    button
                } else if primary {
                    button.opacity(0.85)
                } else {
                    button.bg(palette.subtle).border_color(palette.strong_line)
                }
            })
            .focus(|button| button.border_color(palette.muted))
            .when(!enabled, |button| button.opacity(0.5).cursor_default())
            .text_size(px(12.0))
            .text_color(if primary { palette.bg } else { palette.text })
            .child(label.to_owned())
    }

    fn sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let lists = self.navigation_lists(window, cx);
        div()
            .flex()
            .flex_col()
            .flex_none()
            .id("navigation-sidebar")
            .track_focus(&self.navigation.scope)
            .tab_group()
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                if shell.close_session_filter_menu() {
                    cx.notify();
                }
            }))
            .w(px(NAVIGATION_WIDTH))
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
                                self.navigation_tooltip(
                                    self.navigation_control(
                                        self.icon_button("workbench-search-button", "search", 25.0),
                                        true,
                                    )
                                    .track_focus(&self.navigation.first)
                                    .h(px(28.0))
                                    .on_click(cx.listener(
                                        |shell, _, window, cx| {
                                            shell.open_workbench_search(window, cx)
                                        },
                                    )),
                                    "搜索项目和会话",
                                ),
                            )
                            .when(self.node.is_some(), |tools| {
                                tools.child(self.session_filter_button(cx))
                            })
                            .child(
                                self.navigation_tooltip(
                                    self.navigation_control(
                                        self.icon_button("fold-navigation", "panel-left", 25.0),
                                        true,
                                    )
                                    .h(px(28.0))
                                    .on_click(cx.listener(
                                        |shell, _, window, cx| shell.close_navigation(window, cx),
                                    )),
                                    "收起侧栏",
                                ),
                            )
                            .children(
                                self.session_filter_controls(cx)
                                    .map(|menu| gpui::deferred(menu).with_priority(1)),
                            ),
                    ),
            )
            .when(self.node.is_some() && self.project.is_some(), |sidebar| {
                let checking = self.creation_readback.is_some();
                let enabled =
                    self.creating_session.is_none() && (!checking || self.can_create_session());
                let label = if self.creating_session.is_some() {
                    "正在读取…"
                } else if self.creation_readback.is_some() {
                    "核对新会话"
                } else {
                    "新对话"
                };
                sidebar
                    .child(
                        self.navigation_control(self.button("new-session", ""), enabled)
                            .justify_start()
                            .gap(px(8.0))
                            .h(px(30.0))
                            .child(icon("compose", 15.0, palette.text))
                            .child(
                                self.text(label, 14.0, palette.text)
                                    .font_weight(FontWeight::MEDIUM),
                            )
                            .when(enabled, |button| {
                                button.on_click(cx.listener(move |shell, _, window, cx| {
                                    if checking {
                                        shell.close_resource_page();
                                        shell.dismiss_drawer(window);
                                        shell.focus_composer(window);
                                        shell.create_session(SessionCreation::New, cx);
                                    } else {
                                        shell.open_new_conversation(window, cx);
                                    }
                                }))
                            })
                            .when(!enabled, |button| button.opacity(0.6).cursor_default()),
                    )
                    .when(self.session.is_some(), |sidebar| {
                        sidebar.when_some(self.creation_error.as_ref(), |sidebar, error| {
                            sidebar.child(
                                self.text(error.message().to_owned(), 12.0, palette.danger)
                                    .p_2(),
                            )
                        })
                    })
            })
            .child(lists)
            .when(self.startup.is_some(), |sidebar| {
                sidebar.child(self.runtime_card(cx))
            })
            .when(self.node.is_some(), |sidebar| {
                sidebar.child(self.archives_button(cx))
            })
            .when(self.overlay == Some(Overlay::Navigation), |sidebar| {
                sidebar
                    .child(
                        self.navigation_control(self.button("mobile-resources", ""), true)
                            .h(px(36.0))
                            .gap_2()
                            .justify_start()
                            .child(icon("blocks", 16.0, palette.muted))
                            .child(self.text("技能与插件", 13.0, palette.text))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.open_resource_page(window, cx);
                            })),
                    )
                    .when(
                        self.preferences.entries.shows(feature_entries::Entry::Goal),
                        |sidebar| {
                            sidebar.child(
                                self.navigation_control(self.button("mobile-goal", ""), true)
                                    .h(px(36.0))
                                    .gap_2()
                                    .justify_start()
                                    .child(icon("flag", 16.0, palette.muted))
                                    .child(self.text("目标模式", 13.0, palette.text))
                                    .on_click(cx.listener(|shell, _, window, cx| {
                                        shell.open_feature_settings(window, cx);
                                    })),
                            )
                        },
                    )
                    .when(
                        self.preferences
                            .entries
                            .shows(feature_entries::Entry::Collaboration),
                        |sidebar| {
                            sidebar.child(
                                self.navigation_control(
                                    self.button("mobile-collaboration", ""),
                                    true,
                                )
                                .h(px(36.0))
                                .gap_2()
                                .justify_start()
                                .child(icon("globe", 16.0, palette.muted))
                                .child(self.text("工作协同", 13.0, palette.text))
                                .on_click(cx.listener(
                                    |shell, _, window, cx| {
                                        shell.dismiss_drawer(window);
                                        shell.open_collaboration_settings(window, cx);
                                    },
                                )),
                            )
                        },
                    )
                    .child(
                        self.navigation_control(self.button("mobile-donate", ""), true)
                            .h(px(36.0))
                            .gap_2()
                            .justify_start()
                            .child(icon("thumbs-up", 16.0, palette.muted))
                            .child(self.text("捐赠", 13.0, palette.text))
                            .on_click(|_, _, cx| cx.open_url(DONATION_URL)),
                    )
                    .child(
                        self.navigation_control(self.button("mobile-settings", ""), true)
                            .track_focus(&self.navigation.last)
                            .h(px(36.0))
                            .gap_2()
                            .justify_start()
                            .child(icon("settings", 16.0, palette.muted))
                            .child(self.text("设置", 13.0, palette.text))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.overlay = None;
                                shell.open_settings(window, cx);
                            })),
                    )
            })
            .when(self.node.is_none() && self.startup.is_none(), |sidebar| {
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
        depth: usize,
        nested: bool,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let palette = self.palette;
        let selected = self.session == Some(index);
        let title = session.display_title();
        let activity = match session.activity {
            Some(work_core::api::SessionActivity::Running) => Some(("执行中", palette.muted)),
            Some(work_core::api::SessionActivity::AwaitingApproval) => {
                Some(("待审批", palette.danger))
            }
            _ => None,
        };
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
            self.navigation_control(
                self.button(
                    (if nested { "project-session" } else { "session" }, index),
                    "",
                ),
                true,
            )
            .w_full()
            .justify_start()
            .min_w_0()
            .h(px(30.0))
            .pl(navigation::tree_inset(depth + usize::from(nested)))
            .pr(px(10.0))
            .gap_1()
            .when(selected, |row| {
                row.bg(palette.hover)
                    .border_color(palette.line)
                    .font_weight(FontWeight::MEDIUM)
            })
            .child(icon("message-circle", 13.0, palette.muted))
            .child(
                self.text(title.to_owned(), 13.0, palette.text)
                    .flex()
                    .flex_col()
                    .flex_1()
                    .w_0()
                    .min_w_0()
                    .truncate(),
            )
            .when(session.pinned, |row| {
                row.child(icon("pin", 12.0, palette.muted))
            })
            .when_some(activity, |row, (label, color)| {
                row.child(self.text(label, 11.0, color).flex_none())
            }),
            label,
        )
        .on_click(
            cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| {
                let drawer = shell.overlay == Some(Overlay::Navigation);
                shell.select_session(index, cx);
                if drawer || event.mouse_position().is_some() {
                    shell.focus_composer(window);
                }
            }),
        )
    }

    fn desktop_navigation_open(&self) -> bool {
        self.navigation_open
            .unwrap_or(self.navigation_layout == NavigationLayout::Expanded)
    }

    fn close_navigation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.project_menu = None;
        self.close_session_filter_menu();
        if self.overlay == Some(Overlay::Navigation) {
            self.overlay = None;
        } else {
            self.navigation_open = Some(false);
        }
        self.navigation.toggle.focus(window);
        cx.notify();
    }

    fn toggle_navigation(&mut self, narrow: bool, window: &mut Window, cx: &mut Context<Self>) {
        if narrow && self.overlay == Some(Overlay::Navigation)
            || !narrow && self.desktop_navigation_open()
        {
            self.close_navigation(window, cx);
            return;
        }
        self.project_menu = None;
        self.close_session_filter_menu();
        self.navigation.reopen();
        if narrow {
            self.overlay = Some(Overlay::Navigation);
            self.navigation.first.focus(window);
        } else {
            self.navigation_open = Some(true);
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
                self.toggle_navigation(width <= px(COMPACT_NAVIGATION_WIDTH), window, cx);
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
            Some(Overlay::Navigation) => width <= px(COMPACT_NAVIGATION_WIDTH),
            Some(Overlay::Inspector) => width <= px(900.0),
            None => false,
        };
        if visible {
            let navigation = self.overlay == Some(Overlay::Navigation);
            self.overlay = None;
            self.project_menu = None;
            self.close_session_filter_menu();
            if navigation {
                self.navigation.toggle.focus(window);
                return true;
            }
            if self.resource_page_open() {
                self.search.focus.focus(window);
            } else {
                self.focus_composer(window);
            }
        }
        visible
    }

    fn desktop_inspector_open(&self) -> bool {
        self.overview_open
            && !(self.auto_hide_empty_changes
                && self.session.is_none()
                && self.files.tab == project_files::Tab::Changes
                && (self.project.is_none() || self.files.changes_empty() == Some(true)))
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
            if same && self.overlay == Some(Overlay::Inspector) {
                self.dismiss_drawer(window);
            } else {
                self.overlay = Some(Overlay::Inspector);
                self.inspector_focus.focus(window);
            }
        } else {
            self.overview_open = returning || !same || !self.desktop_inspector_open();
            self.auto_hide_empty_changes = false;
        }
        self.show_project_tab(tab, cx);
        cx.notify();
    }

    fn rail(&self, inspector_overlay: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let inspector_open = if inspector_overlay {
            self.overlay == Some(Overlay::Inspector)
        } else {
            self.desktop_inspector_open()
        };
        let rail_button = |id, shape, label, active| {
            self.navigation_tooltip(
                self.navigation_control(self.icon_button(id, shape, 34.0), true)
                    .h(px(34.0))
                    .when(active, |button| button.bg(palette.hover)),
                label,
            )
        };
        let dock_button = |tab, id, shape, label, cx: &mut Context<Self>| {
            rail_button(id, shape, label, inspector_open && self.files.tab == tab).on_click(
                cx.listener(move |this, _, window, cx| {
                    this.toggle_inspector(tab, inspector_overlay, window, cx)
                }),
            )
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
                rail_button(
                    "rail-conversation",
                    "terminal",
                    "对话",
                    !self.resource_page_open(),
                )
                .on_click(cx.listener(|shell, _, window, cx| {
                    if !shell.return_from_resources(window, cx) {
                        shell.focus_composer(window);
                    }
                })),
            )
            .child(
                rail_button(
                    "rail-navigation",
                    "folder",
                    "切换侧栏",
                    self.desktop_navigation_open(),
                )
                .on_click(
                    cx.listener(|this, _, window, cx| this.toggle_navigation(false, window, cx)),
                ),
            )
            .child(
                rail_button(
                    "rail-resources",
                    "blocks",
                    "技能与插件",
                    self.resource_page_open(),
                )
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.open_resource_page(window, cx);
                })),
            )
            .when(
                self.preferences.entries.shows(feature_entries::Entry::Goal),
                |rail| {
                    rail.child(
                        rail_button("rail-goal", "flag", "目标模式", false).on_click(cx.listener(
                            |shell, _, window, cx| shell.open_feature_settings(window, cx),
                        )),
                    )
                },
            )
            .when(
                self.preferences
                    .entries
                    .shows(feature_entries::Entry::Collaboration),
                |rail| {
                    rail.child(
                        rail_button("rail-collaboration", "globe", "工作协同", false).on_click(
                            cx.listener(|shell, _, window, cx| {
                                shell.open_collaboration_settings(window, cx)
                            }),
                        ),
                    )
                },
            )
            .child(div().w(px(22.0)).h(px(1.0)).my(px(5.0)).bg(palette.line))
            .child(dock_button(
                self.browser_panel_tab(),
                "rail-project",
                "globe",
                "浏览器",
                cx,
            ))
            .child(dock_button(
                project_files::Tab::Files,
                "rail-files",
                "code-xml",
                "代码",
                cx,
            ))
            .child(div().flex_1())
            .child(
                rail_button("rail-donate", "thumbs-up", "捐赠", false)
                    .on_click(|_, _, cx| cx.open_url(DONATION_URL)),
            )
            .child(
                rail_button("settings", "settings", "设置", false)
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
                            self.navigation_tooltip(
                                self.navigation_control(self.icon_button(id, shape, 26.0), enabled)
                                    .h(px(26.0))
                                    .when(!enabled, |button| button.opacity(0.35).cursor_default())
                                    .when(enabled, |button| {
                                        button.on_click(cx.listener(move |this, _, window, cx| {
                                            this.step_history(forward, window, cx)
                                        }))
                                    }),
                                if forward { "前进" } else { "后退" },
                            )
                        }),
                    )
                    .child(
                        self.navigation_tooltip(
                            self.navigation_control(
                                self.icon_button("navigation", "panel-left", 28.0),
                                true,
                            )
                            .track_focus(&self.navigation.toggle)
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.toggle_navigation(compact, window, cx)
                                },
                            )),
                            "切换侧栏",
                        ),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(px(if compact {
                        112.0
                    } else {
                        (width * 0.22).max(166.0)
                    }))
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
                                    self.desktop_inspector_open()
                                };
                            let button = self
                                .navigation_control(self.button(id, ""), true)
                                .gap(px(6.0))
                                .when(compact, |button| button.w(px(28.0)).px_0())
                                .child(icon(
                                    shape,
                                    15.0,
                                    if active { palette.text } else { palette.muted },
                                ))
                                .when(!compact, |button| {
                                    button.child(self.text(
                                        label,
                                        12.0,
                                        if active { palette.text } else { palette.muted },
                                    ))
                                })
                                .when_some(
                                    (tab == project_files::Tab::Changes && width >= 1200.0)
                                        .then(|| self.files.line_totals())
                                        .flatten(),
                                    |button, counts| button.child(self.git_line_counts(counts)),
                                )
                                .when(active, |button| button.bg(palette.hover))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.toggle_inspector(tab, overlay, window, cx)
                                }));
                            self.navigation_tooltip(button, label)
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

    fn overview(
        &self,
        width: f32,
        height: f32,
        overlay: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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
            .id("project-inspector")
            .tab_index(0)
            .tab_stop(false)
            .track_focus(&self.inspector_focus)
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
                                    self.navigation_tooltip(
                                        self.navigation_control(
                                            self.icon_button("inspector-full", "panel-right", 26.0),
                                            self.project_keyboard_enabled(),
                                        )
                                        .when(self.inspector_full, |button| {
                                            button.bg(palette.subtle)
                                        }),
                                        if self.inspector_full {
                                            "还原项目面板宽度"
                                        } else {
                                            "展开项目面板"
                                        },
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.inspector_full = !this.inspector_full;
                                            cx.notify();
                                        },
                                    )),
                                )
                            })
                            .child(
                                self.navigation_tooltip(
                                    self.icon_button("close-overview", "x", 26.0),
                                    "关闭项目面板",
                                )
                                .tab_index(0)
                                .tab_stop(self.project_keyboard_enabled())
                                .focus(|button| button.bg(palette.hover))
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        if overlay {
                                            this.dismiss_drawer(window);
                                        } else {
                                            this.overview_open = false;
                                            this.auto_hide_empty_changes = false;
                                            this.focus_composer(window);
                                        }
                                        cx.notify();
                                    },
                                )),
                            ),
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
                self.project_contents(width, height, cx)
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
        let compact = self.preferences.compact;
        let prompt_width = (reading_width * 0.88).min((reading_width * 0.68).max(320.0));
        let show_rows = self.session.is_some() && self.session == self.transcript_session;
        let fork_replies = show_rows && self.can_fork() && self.supports_reply_fork();
        let empty_text = match (self.watcher.is_some() && self.frame.is_none(), self.session) {
            (true, _) => "正在读取会话…",
            (false, Some(_)) => "此会话暂无消息",
            (false, None) => "从侧栏选择会话",
        };
        let row_keys = self
            .rows
            .iter()
            .filter(|_| show_rows)
            .map(|row| (row.key().to_owned(), row.has_durable_key()))
            .collect();
        let reading_layout =
            self.reading
                .layout(self.transcript_scroll.clone(), row_keys, cx.weak_entity());
        let has_messages = show_rows && !self.rows.is_empty();
        let keyboard = self.navigation_keyboard_enabled() && self.overlay.is_none();
        let rows = self
            .rows
            .iter()
            .filter(|_| show_rows)
            .enumerate()
            .map(|(index, row)| {
                let previous = index.checked_sub(1).and_then(|index| self.rows.get(index));
                let content = match row {
                    Row::User {
                        text, attachments, ..
                    } => div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .when(!classic, |row| row.items_end())
                        .when(!text.trim().is_empty(), |row| {
                            row.child(
                                self.text(text.clone(), font_size, palette.text)
                                    .line_height(px(self.preferences.line_height()))
                                    .when(!classic, |text| {
                                        text.max_w(px(prompt_width))
                                            .px(px(16.0))
                                            .py(px(if compact { 8.0 } else { 11.0 }))
                                            .rounded(px(16.0))
                                            .bg(palette.subtle)
                                    })
                                    .when(classic, |text| text.font_weight(FontWeight::MEDIUM)),
                            )
                        })
                        .when(!attachments.is_empty(), |row| {
                            row.child(self.attachment_cards(attachments, prompt_width))
                        })
                        .into_any_element(),
                    Row::Assistant {
                        key,
                        view,
                        attachments,
                        reply,
                        ..
                    } => {
                        let following = &self.rows[index + 1..];
                        let progress = !classic
                            && matches!(
                                view.nodes.as_slice(),
                                [markdown::Node::Element {
                                    tag: markdown::Element::P,
                                    ..
                                }]
                            )
                            && matches!(following.first(), Some(Row::Activity { .. }))
                            && matches!(
                                following
                                    .iter()
                                    .find(|row| !matches!(row, Row::Activity { .. })),
                                Some(Row::Assistant { .. })
                            );
                        div()
                            .when(progress, |row| row.group(PROGRESS_REPLY_GROUP))
                            .when(!view.nodes.is_empty(), |row| {
                                row.child(self.rich_answer(key, view, progress))
                            })
                            .when(!attachments.is_empty(), |row| {
                                row.child(self.attachment_cards(attachments, reading_width))
                            })
                            .when_some(reply.as_ref(), |row, reply| {
                                row.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(5.0))
                                        .mt(px(if compact || progress { 6.0 } else { 10.0 }))
                                        .when(
                                            reply
                                                .parts
                                                .iter()
                                                .any(|part| !part.copy_text.trim().is_empty()),
                                            |actions| {
                                                actions.child(self.copy_reply_button(
                                                    key,
                                                    reply.parts.clone(),
                                                    progress,
                                                ))
                                            },
                                        )
                                        .children(reply.entry_id.as_ref().map(|entry| {
                                            self.reply_rating_buttons(entry, progress, cx)
                                        }))
                                        .children(
                                            reply.entry_id.as_ref().filter(|_| fork_replies).map(
                                                |entry| {
                                                    self.fork_reply_button(key, entry, progress, cx)
                                                },
                                            ),
                                        ),
                                )
                            })
                            .into_any_element()
                    }
                    Row::Activity {
                        key,
                        label,
                        collapsed,
                        attention,
                        items,
                    } => self.activity_group(key, label, *collapsed, *attention, items, cx),
                };
                div()
                    .id(SharedString::from(row.key().to_owned()))
                    .min_w_0()
                    .mt(px(row.spacing_before(previous, compact)))
                    .child(content)
            });
        let transcript = div()
            .id("transcript")
            .flex_1()
            .min_h_0()
            .w_full()
            .border_1()
            .border_color(gpui::transparent_black())
            .when(has_messages && keyboard, |region| {
                region
                    .track_focus(&self.transcript_focus)
                    .tab_index(0)
                    .focus(|style| style.border_color(palette.muted))
                    .on_key_down(
                        cx.listener(|shell, event: &gpui::KeyDownEvent, window, cx| {
                            if !shell.transcript_focus.is_focused(window) {
                                return;
                            }
                            let key = &event.keystroke;
                            let modifiers = key.modifiers;
                            if modifiers.control
                                || modifiers.alt
                                || modifiers.shift
                                || modifiers.platform
                            {
                                return;
                            }
                            let scroll = &shell.transcript_scroll;
                            let current = scroll.offset().y;
                            let bottom = -scroll.max_offset().height;
                            let line = px(24.0);
                            let page = (scroll.bounds().size.height - line).max(line);
                            let next = match key.key.as_str() {
                                "up" => current + line,
                                "down" => current - line,
                                "pageup" => current + page,
                                "pagedown" => current - page,
                                "home" => px(0.0),
                                "end" => bottom,
                                _ => return,
                            };
                            shell.reading.scroll_to(scroll, next.clamp(bottom, px(0.0)));
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
            })
            .overflow_y_scroll()
            .track_scroll(&self.transcript_scroll)
            .px(px(inset))
            .pt(px(if compact { 24.0 } else { 36.0 }))
            .pb(px(if compact { 12.0 } else { 20.0 }))
            .child(
                div()
                    .on_children_prepainted(reading_layout)
                    .flex()
                    .flex_col()
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
                    .when(self.load_error.is_none() && !has_messages, |column| {
                        column.child(self.text(empty_text, 14.0, palette.muted))
                    }),
            );
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(transcript)
            .when(has_messages && self.reading.later_content(), |region| {
                region.child(
                    div()
                        .absolute()
                        .bottom(px(8.0))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(
                            self.button("transcript-latest", "")
                                .h(px(32.0))
                                .px_3()
                                .gap(px(5.0))
                                .rounded_full()
                                .bg(palette.bg)
                                .border_1()
                                .border_color(palette.strong_line)
                                .shadow_md()
                                .tab_index(0)
                                .tab_stop(keyboard)
                                .focus(|button| {
                                    button.bg(palette.hover).border_color(palette.muted)
                                })
                                .child(icon("chevron-down", 14.0, palette.muted))
                                .child(self.text("回到最新消息", 12.0, palette.text))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    let scroll = &shell.transcript_scroll;
                                    shell.reading.scroll_to(scroll, -scroll.max_offset().height);
                                    shell.transcript_focus.focus(window);
                                    cx.notify();
                                })),
                        ),
                )
            })
            .into_any_element()
    }

    fn reply_action(
        &self,
        id: impl Into<gpui::ElementId>,
        reveal_on_interaction: bool,
    ) -> gpui::Stateful<Div> {
        self.button(id, "")
            .w(px(24.0))
            .h(px(25.0))
            .px_0()
            .tab_index(0)
            .tab_stop(self.navigation_keyboard_enabled() && self.overlay.is_none())
            .when(reveal_on_interaction, |button| {
                button
                    .opacity(0.0)
                    .group_hover(PROGRESS_REPLY_GROUP, |style| style.opacity(1.0))
            })
            .focus(|button| button.bg(self.palette.hover).opacity(1.0))
    }

    fn copy_reply_button(
        &self,
        key: &str,
        parts: Vec<Rc<markdown::MarkdownView>>,
        progress: bool,
    ) -> gpui::Stateful<Div> {
        let palette = self.palette;
        self.navigation_tooltip(
            self.reply_action(SharedString::from(format!("copy-reply/{key}")), progress)
                .child(icon("copy", 15.0, palette.muted))
                .on_click(move |_, _, cx| {
                    let text = parts
                        .iter()
                        .map(|part| part.copy_text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }),
            "复制回复",
        )
    }

    fn reply_rating_buttons(
        &self,
        entry: &str,
        progress: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        use reply_ratings::{Key, Rating, Ratings};

        let palette = self.palette;
        let scope = self
            .session
            .and_then(|index| self.snapshot.sessions.get(index))
            .map(|session| Key {
                node: self.navigation_node_key(),
                project: session.project_id.clone(),
                session: session.id.clone(),
                entry: entry.to_owned(),
            });
        div()
            .flex()
            .gap(px(3.0))
            .when_some(scope, |buttons, scope| {
                let current = cx.global::<Ratings>().get(&scope);
                buttons.children(
                    [
                        (Rating::Helpful, "thumbs-up", "有帮助（仅本设备）"),
                        (Rating::Unhelpful, "thumbs-down", "没有帮助（仅本设备）"),
                    ]
                    .map(|(rating, glyph, label)| {
                        let key = scope.clone();
                        let active = current == Some(rating);
                        self.navigation_tooltip(
                            self.reply_action(
                                SharedString::from(format!("{glyph}/{entry}")),
                                progress && !active,
                            )
                            .when(active, |button| button.bg(palette.hover))
                            .child(icon(
                                glyph,
                                15.0,
                                if active { palette.text } else { palette.muted },
                            ))
                            .on_click(move |_, _, cx| {
                                cx.global_mut::<Ratings>().toggle(key.clone(), rating)
                            }),
                            label,
                        )
                    }),
                )
            })
    }

    fn activity_group(
        &self,
        key: &str,
        label: &str,
        collapsed: bool,
        attention: bool,
        items: &[ActivityItem],
        cx: &Context<Self>,
    ) -> AnyElement {
        if let [
            ActivityItem::Tool {
                key: tool_key,
                block,
            },
        ] = items
        {
            return self.tool_card(key, tool_key, block, cx);
        }
        let palette = self.palette;
        let open = self.reading.is_open(self.session, key, !collapsed);
        let (label, summary) = match items {
            [ActivityItem::Note { label, .. }] => (*label, None),
            _ => (label, Some(format!("{} 项", items.len()))),
        };
        let state_key = key.to_owned();
        div()
            .id(SharedString::from(key.to_owned()))
            .flex()
            .flex_col()
            .items_start()
            .gap_1()
            .child(
                self.button("activity-summary", "")
                    .max_w_full()
                    .h(px(27.0))
                    .px(px(4.0))
                    .justify_start()
                    .gap(px(7.0))
                    .tab_index(0)
                    .tab_stop(self.navigation_keyboard_enabled() && self.overlay.is_none())
                    .focus(|button| button.bg(palette.hover))
                    .child(
                        icon(
                            if open {
                                "chevron-down"
                            } else {
                                "chevron-right"
                            },
                            12.0,
                            palette.muted,
                        )
                        .flex_none(),
                    )
                    .child(
                        self.text(label.to_owned(), 12.0, palette.muted)
                            .min_w_0()
                            .truncate(),
                    )
                    .when_some(summary, |button, summary| {
                        button.child(self.text(summary, 11.0, palette.muted).flex_none())
                    })
                    .when(attention, |button| {
                        button.child(
                            self.text("有进行中或未成功的工具", 11.0, palette.muted)
                                .min_w_0()
                                .truncate(),
                        )
                    })
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell
                            .reading
                            .set_open(shell.session, state_key.clone(), !open);
                        shell.schedule_reading_save(cx);
                        cx.notify();
                    })),
            )
            .when(open, |group| {
                group.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .pl(px(12.0))
                        .border_l_2()
                        .border_color(palette.line)
                        .children(items.iter().map(|item| {
                            match item {
                                ActivityItem::Tool {
                                    key: tool_key,
                                    block,
                                } => self.tool_card(key, tool_key, block, cx),
                                ActivityItem::Note { label, text, .. } => div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .when(items.len() > 1, |note| {
                                        note.child(self.text(*label, 11.0, palette.muted))
                                    })
                                    .child(
                                        self.text(text.clone(), 12.0, palette.muted)
                                            .line_height(px(20.4)),
                                    )
                                    .into_any_element(),
                            }
                        })),
                )
            })
            .into_any_element()
    }

    fn tool_card(
        &self,
        group: &str,
        key: &str,
        block: &ToolView,
        cx: &Context<Self>,
    ) -> AnyElement {
        let palette = &self.palette;
        let ToolView {
            name,
            arguments,
            output,
            status,
            status_label,
            attachments,
            ..
        } = block;
        let open = self.reading.is_open(self.session, key, false);
        let target = tool_display::target(name, arguments);
        let status_color = match *status {
            "completed" => palette.success,
            "failed" => palette.danger,
            _ => palette.muted,
        };
        let detail = open.then(|| {
            let has_arguments = !arguments.trim().is_empty();
            div()
                .flex()
                .flex_col()
                .px(px(12.0))
                .py(px(9.0))
                .border_t_1()
                .border_color(palette.line)
                .when(has_arguments, |detail| {
                    detail.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .pb(px(8.0))
                            .child(self.text(tool_detail_label(name), 11.0, palette.muted))
                            .child(self.tool_output("tool-arguments", arguments, None)),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .when(has_arguments, |result| {
                            result.pt(px(8.0)).border_t_1().border_color(palette.line)
                        })
                        .when(
                            output.trim().is_empty() && attachments.is_empty(),
                            |result| {
                                result.child(self.text(
                                    match *status {
                                        "completed" | "failed" | "cancelled" => "无输出",
                                        _ => "暂无输出",
                                    },
                                    12.0,
                                    palette.muted,
                                ))
                            },
                        )
                        .when(
                            !output.trim().is_empty() || !attachments.is_empty(),
                            |result| result.child(self.text("工具结果", 11.0, palette.muted)),
                        )
                        .when(!output.trim().is_empty(), |result| {
                            result.child(self.tool_output(
                                "tool-result",
                                output,
                                tool_display::is_read(name).then_some(target.as_str()),
                            ))
                        })
                        .when(!attachments.is_empty(), |result| {
                            result.child(self.attachment_cards(attachments, READING_WIDTH))
                        }),
                )
        });
        div()
            .id(SharedString::from(key.to_owned()))
            .flex()
            .flex_col()
            .min_w_0()
            .rounded(px(8.0))
            .border_1()
            .border_color(if open {
                palette.line
            } else {
                gpui::transparent_black()
            })
            .child(
                self.button("tool-summary", "")
                    .justify_start()
                    .h(px(31.0))
                    .px(px(9.0))
                    .when(open, |button| button.bg(palette.subtle).rounded_b(px(0.0)))
                    .tab_index(0)
                    .tab_stop(self.navigation_keyboard_enabled() && self.overlay.is_none())
                    .focus({
                        let hover = palette.hover;
                        move |button| button.bg(hover)
                    })
                    .child(
                        icon(
                            if open {
                                "chevron-down"
                            } else {
                                "chevron-right"
                            },
                            12.0,
                            palette.muted,
                        )
                        .flex_none(),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .ml(px(7.0))
                            .mr(px(12.0))
                            .items_center()
                            .child(
                                self.text(name.clone(), 12.0, palette.text)
                                    .max_w(gpui::relative(0.55))
                                    .min_w_0()
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate(),
                            )
                            .when(!target.is_empty(), |summary| {
                                summary.child(
                                    self.text(target, 12.0, palette.muted)
                                        .ml(px(8.0))
                                        .flex_1()
                                        .min_w_0()
                                        .truncate(),
                                )
                            }),
                    )
                    .child(self.text(*status_label, 11.0, status_color).flex_none())
                    .on_click(cx.listener({
                        let group = group.to_owned();
                        let key = key.to_owned();
                        move |shell, _, _, cx| {
                            shell.reading.set_open(shell.session, key.clone(), !open);
                            if !open {
                                shell.reading.set_open(shell.session, group.clone(), true);
                            }
                            shell.schedule_reading_save(cx);
                            cx.notify();
                        }
                    })),
            )
            .children(detail)
            .into_any_element()
    }

    fn composer_footer(&self, reading_width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let runtime = self
            .composer_config
            .observed
            .as_ref()
            .map(|state| &state.runtime);
        let (context, metrics, loaded) = self.composer_usage(runtime);
        let summary = if self.session.is_some() {
            let values = [
                ("输入", metrics.input),
                ("输出", metrics.output),
                ("缓存读", metrics.cached),
                ("缓存写", metrics.writes),
            ];
            let elapsed = self.run_timing.elapsed();
            (elapsed.is_some() || values.iter().any(|(_, value)| value.is_some())).then(|| {
                self.navigation_tooltip(
                    div()
                        .id("composer-footer-metrics")
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .min_h(px(18.0))
                        .gap(px(11.0))
                        .when(loaded, |row| {
                            row.child(self.text("已加载", 11.0, palette.muted).flex_none())
                        })
                        .children(values.into_iter().filter_map(|(label, value)| {
                            let value = value?;
                            Some(
                                self.text(
                                    format!("{label} {}", tokens(Some(value))),
                                    11.0,
                                    palette.muted,
                                )
                                .flex_none(),
                            )
                        }))
                        .when_some(elapsed, |row, elapsed| {
                            row.child(
                                self.text(format!("耗时 {elapsed}"), 11.0, palette.muted)
                                    .flex_none(),
                            )
                        }),
                    if loaded {
                        "已加载消息的 Token · 耗时为桌面观测的最近一轮"
                    } else {
                        "会话累计 Token · 耗时为桌面观测的最近一轮"
                    },
                )
            })
        } else {
            None
        };
        let (inline, below) = if reading_width >= 620.0 {
            (summary, None)
        } else {
            (None, summary)
        };
        let node = self.navigation_node_label();
        let label = node
            .split_once("://")
            .map_or(node.as_str(), |(_, host)| host);
        let row = div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(28.0))
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .min_w_0()
                    .gap(px(9.0))
                    .child(
                        self.navigation_tooltip(
                            div()
                                .id("composer-footer-node")
                                .flex()
                                .items_center()
                                .gap(px(5.0))
                                .min_w_0()
                                .when(self.observation.is_some(), |node| {
                                    node.max_w(relative(0.48))
                                })
                                .child(icon(self.navigation_node_icon(), 12.0, palette.muted))
                                .child(self.text(label.to_owned(), 11.0, palette.muted).truncate()),
                            node.clone(),
                        ),
                    )
                    .when_some(self.observation.as_ref(), |group, observation| {
                        let (dot, text) = match observation.tone {
                            "active" => (palette.keyword, palette.text),
                            "success" => (palette.success, palette.muted),
                            "danger" => (palette.danger, palette.danger),
                            "warning" => (palette.text, palette.text),
                            _ => (palette.muted, palette.muted),
                        };
                        group
                            .child(div().flex_none().w(px(1.0)).h(px(10.0)).bg(palette.line))
                            .child(
                                self.navigation_tooltip(
                                    div()
                                        .id("composer-footer-run")
                                        .flex()
                                        .items_center()
                                        .gap(px(5.0))
                                        .min_w_0()
                                        .child(
                                            div().flex_none().size(px(5.0)).rounded_full().bg(dot),
                                        )
                                        .child(self.text(observation.label, 11.0, text).truncate()),
                                    observation.label,
                                ),
                            )
                    }),
            );
        let right = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(12.0))
            .children(inline)
            .when(
                self.preferences.show_context && self.session.is_some(),
                |right| {
                    let used = context.percent;
                    let label = used.map_or_else(
                        || "上下文".to_owned(),
                        |used| format!("上下文 {}", percent(Some(used))),
                    );
                    right.child(
                        self.navigation_tooltip(
                            self.button("open-session-context", "")
                                .h(px(22.0))
                                .px(px(6.0))
                                .gap(px(6.0))
                                .when_some(used, |button, used| {
                                    button.child(
                                        div()
                                            .w(px(24.0))
                                            .h(px(3.0))
                                            .rounded_full()
                                            .bg(palette.strong_line)
                                            .overflow_hidden()
                                            .child(
                                                div()
                                                    .h_full()
                                                    .w(relative(used as f32 / 100.0))
                                                    .bg(palette.muted),
                                            ),
                                    )
                                })
                                .child(self.text(label, 11.0, palette.muted))
                                .child(icon("chevron-right", 10.0, palette.muted)),
                            "查看会话上下文",
                        )
                        .tab_index(0)
                        .focus(|button| button.bg(palette.hover))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.open_session_context(window, cx);
                        })),
                    )
                },
            );
        div()
            .flex()
            .flex_col()
            .pl(px(9.0))
            .pr(px(4.0))
            .child(row.child(right))
            .children(below)
    }

    fn restored_draft_notice(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px(px(4.0))
            .child(self.text(
                "上次发送结果未确认。请先检查会话和后续消息。",
                12.0,
                self.palette.muted,
            ))
            .child(
                self.navigation_control(
                    self.button("confirm-restored-draft", "允许再次发送"),
                    self.navigation_keyboard_enabled() && self.overlay.is_none(),
                )
                .on_click(cx.listener(|shell, _, window, cx| {
                    let key = shell.draft_key();
                    if let Some(draft) = shell.drafts.get_mut(&key) {
                        draft.confirm_restored_delivery = false;
                        shell.focus_composer(window);
                        cx.notify();
                    }
                })),
            )
    }

    fn composer(&self, inset: f32, reading_width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let key = self.draft_key();
        let draft = self.drafts.get(&key);
        let controls = self.session_controls();
        let welcome = self.node.is_some() && self.project.is_some() && self.session.is_none();
        let loading_attachments = self.selecting_attachments.contains(&key);
        let draft_save_failed = self
            .draft_storage
            .as_ref()
            .is_some_and(|storage| storage.save_failed(key));
        let can_send = self.welcome_can_send()
            || controls.send
                && !loading_attachments
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
                    .filter(|draft| draft.message_over_limit())
                    .map(|_| "消息、项目文件引用和反馈合计超过长度上限")
            })
            .or_else(|| loading_attachments.then_some("正在读取附件…"))
            .or_else(|| draft.and_then(|draft| draft.attachment_notice.as_deref()))
            .or_else(|| match welcome {
                true if self.creating_session.is_some() => Some("正在创建并核对新会话…"),
                true => self.creation_error.as_ref().map(CreationError::message),
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
                if self.startup.is_some() {
                    None
                } else if self.node.is_none() {
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
                    .children(self.file_reference_menu(cx))
                    .children(self.composer_command_menu(cx))
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
                                    .children(self.file_reference_chips(&draft.references, cx))
                                    .child(self.feedback_chips(&draft.feedback, cx))
                            })
                            .when(
                                self.comment.is_none()
                                    && !self.project_form.open
                                    && self.question.is_none()
                                    && !self.session_management.open()
                                    && self.startup.is_none(),
                                |composer| composer.child(self.input_element(cx)),
                            )
                            .when(
                                self.comment.is_some()
                                    || self.project_form.open
                                    || self.question.is_some()
                                    || self.session_management.open()
                                    || self.startup.is_some(),
                                |composer| composer.child(div().h(px(47.0))),
                            )
                            .when_some(status, |composer, status| {
                                composer.child(
                                    self.text(status.to_owned(), 12.0, palette.muted)
                                        .px(px(4.0))
                                        .py_1(),
                                )
                            })
                            .when(draft_save_failed, |composer| {
                                composer.child(
                                    self.text("无法保存本机草稿。", 12.0, palette.muted)
                                        .px(px(4.0))
                                        .py_1(),
                                )
                            })
                            .when(
                                draft.is_some_and(|draft| draft.confirm_restored_delivery),
                                |composer| composer.child(self.restored_draft_notice(cx)),
                            )
                            .when(
                                welcome
                                    && matches!(
                                        self.creation_error,
                                        Some(CreationError::HostUnavailable)
                                    ),
                                |composer| {
                                    composer.child(
                                        div().flex().child(
                                            self.button(
                                                "open-provider-after-create-failure",
                                                "打开供应商配置",
                                            )
                                            .on_click(
                                                cx.listener(|shell, _, window, cx| {
                                                    shell.open_provider_settings(window, cx);
                                                }),
                                            ),
                                        ),
                                    )
                                },
                            )
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
                                            .gap(px(2.0))
                                            .child(self.composer_add_button(cx))
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
                    .child(self.composer_footer(reading_width, cx)),
            )
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.traced {
            self.traced = true;
            if std::env::var_os("RPI_GPUI_TRACE").is_some() {
                let groups = self
                    .rows
                    .iter()
                    .filter(|row| matches!(row, Row::Activity { .. }))
                    .count();
                eprintln!(
                    "first frame: {} rows ({} transcript + {} activity groups), {} projects, {} sessions, metrics input={:?} context={:?}",
                    self.rows.len(),
                    self.rows.len() - groups,
                    groups,
                    self.snapshot.projects.len(),
                    self.snapshot.sessions.len(),
                    self.snapshot.metrics.input,
                    self.snapshot.context.percent,
                );
                for row in self.rows.iter().filter(|_| self.node.is_none()) {
                    eprintln!(
                        "  row {}",
                        match row {
                            Row::User { text, .. } => format!("user {:?}", short(text)),
                            Row::Assistant { source, .. } =>
                                format!("assistant {:?}", short(source)),
                            Row::Activity { items, .. } =>
                                format!("activity {} items", items.len()),
                        }
                    );
                }
            }
        }
        self.apply_appearance(window.appearance(), false);
        self.sync_composer_commands(window);
        self.sync_file_references(window, cx);
        let width = f32::from(window.viewport_size().width);
        let height = f32::from(window.viewport_size().height);
        let navigation_layout = NavigationLayout::for_width(width);
        if self.navigation_layout != navigation_layout {
            self.navigation_layout = navigation_layout;
            self.navigation.reopen();
            if self.navigation.scope.contains_focused(window, cx)
                && (navigation_layout == NavigationLayout::Drawer
                    || !self.desktop_navigation_open())
            {
                self.navigation.toggle.focus(window);
            }
            if self.overlay == Some(Overlay::Navigation) {
                self.overlay = None;
            }
            self.project_menu = None;
            self.close_session_filter_menu();
        }
        let compact = navigation_layout == NavigationLayout::Drawer;
        let overlay = width <= 900.0;
        let rail = if compact { 0.0 } else { 52.0 };
        let navigation = if !compact && self.desktop_navigation_open() {
            NAVIGATION_WIDTH
        } else {
            0.0
        };
        let remaining = width - rail - navigation;
        let dock_width = self
            .inspector_width
            .unwrap_or((width * 0.38).round())
            .clamp(320.0, (remaining - 350.0).max(320.0));
        let docked = self.desktop_inspector_open() && !overlay && !self.resource_page_open();
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
                        if shell.overlay == Some(Overlay::Navigation) {
                            shell.navigation.tab(modifiers.shift, window, cx);
                        } else if modifiers.shift {
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
                            || shell.dismiss_project_menu(window)
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
            .when_some(self.update_notice(cx), |body, notice| {
                body.child(div().flex().justify_end().px_3().py_1().child(notice))
            })
            .child(
                div()
                    .id("workbench-body")
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .when(!compact, |body| body.child(self.rail(overlay, cx)))
                    .when(navigation > 0.0, |body| {
                        body.child(self.sidebar(window, cx))
                    })
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
                                .when(
                                    navigation == 0.0
                                        && self.overlay != Some(Overlay::Navigation)
                                        && self.startup.is_some(),
                                    |area| {
                                        area.child(
                                            div()
                                                .w_full()
                                                .max_w(px(READING_WIDTH))
                                                .mx_auto()
                                                .px_3()
                                                .child(self.runtime_card(cx)),
                                        )
                                    },
                                )
                                .child(self.composer(inset, reading_width, cx)),
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
                                    height,
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
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_session_filter_menu();
                                        this.dismiss_drawer(window);
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
                                        .child(self.sidebar(window, cx)),
                                )
                            })
                            .when(
                                self.overlay == Some(Overlay::Inspector),
                                |body| {
                                    body.child(
                                        div()
                                            .absolute()
                                            .right_0()
                                            .top_0()
                                            .bottom_0()
                                            .occlude()
                                            .child(self.overview(
                                                if self.files.is_overview() {
                                                    (width - 20.0).min(560.0)
                                                } else {
                                                    (width - 20.0)
                                                        .min((width * 0.38).clamp(360.0, 560.0))
                                                },
                                                height,
                                                true,
                                                cx,
                                            )),
                                    )
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
            .when_some(self.project_menu_overlay(cx), |body, menu| {
                body.child(gpui::deferred(menu).with_priority(5))
            })
            .when(self.runtime_dialog_open(), |body| {
                body.child(gpui::deferred(self.runtime_dialog(cx)).with_priority(130))
            })
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
    pending: PendingRequests,
    previous: &[Row],
) -> (Vec<Row>, RunObservation) {
    let ordered = order.update(scope, history, blocks);
    let conversation = transcript_view::present_with_attachments(
        &ordered,
        history,
        blocks,
        projection.map(|run| &run.phase),
        projection.map(|run| &run.connection),
        pending,
        Some(message_attachments::claims),
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
    let mut reply_parts = HashMap::<String, Vec<_>>::new();
    for part in conversation.parts {
        match part {
            Part::Message {
                role: "user",
                key,
                reading_key,
                text,
                attachments,
                ..
            } => rows.push(Row::User {
                durable_key: reading_key.is_some(),
                key: reading_key.unwrap_or(key),
                text,
                attachments,
            }),
            Part::Message {
                key,
                reading_key,
                fork_entry_id,
                rating_entry_id,
                text,
                attachments,
                ..
            } => {
                let durable_key = reading_key.is_some();
                let key = reading_key.unwrap_or(key);
                let view = previous
                    .get(key.as_str())
                    .filter(|(source, _)| *source == &text)
                    .map_or_else(
                        || Rc::new(markdown::present(&text)),
                        |(_, view)| Rc::clone(view),
                    );
                let reply = match fork_entry_id {
                    Some(entry) => {
                        reply_parts.entry(entry).or_default().push(Rc::clone(&view));
                        rating_entry_id.and_then(|entry| {
                            let parts = reply_parts.remove(&entry)?;
                            Some(ReplyActions {
                                entry_id: Some(entry),
                                parts,
                            })
                        })
                    }
                    None => Some(ReplyActions {
                        entry_id: None,
                        parts: vec![Rc::clone(&view)],
                    }),
                };
                rows.push(Row::Assistant {
                    key,
                    durable_key,
                    source: text,
                    view,
                    attachments,
                    reply,
                });
            }
            Part::Activity {
                key,
                label,
                collapsed,
                attention,
                items,
            } => rows.push(Row::Activity {
                key,
                label,
                collapsed,
                attention,
                items,
            }),
        }
    }
    (
        rows,
        RunObservation {
            label: conversation.observation,
            tone: conversation.observation_tone,
        },
    )
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
