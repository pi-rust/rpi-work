//! Native workbench layout. The shared view model owns transcript ordering, tool
//! labels, code tokens and metrics.

#[path = "browser.rs"]
mod browser;
#[path = "composer_config.rs"]
pub(crate) mod composer_config;
#[path = "composer.rs"]
pub(crate) mod editor;
#[path = "project_files.rs"]
pub(crate) mod project_files;
#[path = "rich_text.rs"]
mod rich_text;
#[path = "session_action.rs"]
pub(crate) mod session_action;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{
    AnyElement, Context, CursorStyle, Div, FocusHandle, FontWeight, Hsla, MouseButton,
    MouseMoveEvent, ScrollHandle, SharedString, Task, Window, WindowControlArea, div, point,
    prelude::*, px, relative,
};
use serde_json::Value;
use work_client::ClientError;
use work_core::{MessageBlock, RunProjection};
use work_viewmodel::conversation::transcript_view::{self, ActivityItem, Part, ToolView};
use work_viewmodel::conversation::{code_tokens, markdown, tool_display};
use work_viewmodel::observability::metrics::{percent, tokens};
use work_viewmodel::session_controls::{SessionControls, SessionObservation, controls};
use work_viewmodel::session_receipts::{command_admission, interpret_receipt};
use work_viewmodel::transcript::TranscriptOrder;

use crate::data::{Creation, Node, SessionFrame, Snapshot};
use crate::icons::icon;
use crate::prompt::Draft;
use crate::theme::Palette;

const READING_WIDTH: f32 = 768.0;
const READING_INSET: f32 = 24.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    Navigation,
    Inspector,
}

enum Row {
    User(String),
    Assistant {
        key: String,
        source: String,
        view: Rc<markdown::MarkdownView>,
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
    theme_id: String,
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
    browser: browser::Tabs,
    composer_config: composer_config::Panel,
    creating_session: Option<Task<()>>,
    creation_error: Option<(bool, String)>,
    creation_readback: Option<(usize, String)>,
    navigation_revision: u64,
    rows: Vec<Row>,
    project: Option<usize>,
    session: Option<usize>,
    transcript_session: Option<usize>,
    transcript_scroll: ScrollHandle,
    expanded: HashSet<String>,
    drafts: HashMap<Option<usize>, Draft>,
    focus: FocusHandle,
    input_layout: Option<editor::Layout>,
    input_dragging: bool,
    order: TranscriptOrder,
    /// One-shot diagnostic so a headless launch can prove the first frame ran.
    traced: bool,
}

impl Shell {
    pub fn new(snapshot: Snapshot, node: Option<Node>, cx: &mut Context<Self>) -> Self {
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
            theme_id: "codex".into(),
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
            browser: browser::Tabs::default(),
            composer_config: composer_config::Panel::default(),
            creating_session: None,
            creation_error: None,
            creation_readback: None,
            navigation_revision: 0,
            rows,
            expanded: HashSet::new(),
            drafts: HashMap::new(),
            focus: cx.focus_handle(),
            input_layout: None,
            input_dragging: false,
            order,
            traced: false,
        };
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

    fn select_project(&mut self, index: usize, cx: &mut Context<Self>) {
        self.set_project(Some(index), cx);
        let session = self
            .snapshot
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| session.project_id == self.snapshot.projects[index].id)
            .max_by_key(|(_, session)| session.updated_at)
            .map(|(index, _)| index);
        self.navigate_session(session, cx);
    }

    fn set_project(&mut self, project: Option<usize>, cx: &mut Context<Self>) {
        if self.project != project {
            self.project = project;
            self.navigation_revision += 1;
            self.reset_project_files(cx);
        }
    }

    fn select_session(&mut self, index: usize, cx: &mut Context<Self>) {
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
        let target = (self.project, session);
        if self.session_history[self.history_position] != target {
            self.session_history.truncate(self.history_position + 1);
            self.session_history.push(target);
            self.history_position += 1;
        }
        self.change_session(session, cx);
    }

    fn step_history(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.history_position = if forward {
            (self.history_position + 1).min(self.session_history.len() - 1)
        } else {
            self.history_position.saturating_sub(1)
        };
        let (project, session) = self.session_history[self.history_position];
        self.set_project(project, cx);
        self.change_session(session, cx);
    }

    fn can_create_session(&self) -> bool {
        self.node.is_some()
            && self.project.is_some()
            && self.creating_session.is_none()
            && !self
                .creation_error
                .as_ref()
                .is_some_and(|(unknown, _)| *unknown)
    }

    fn create_session(&mut self, cx: &mut Context<Self>) {
        if !self.can_create_session() {
            return;
        }
        let (Some(node), Some(project)) = (self.node.as_ref(), self.project) else {
            return;
        };
        let retry = self.creation_readback.take();
        let (project, id) = retry.map_or((project, None), |(project, id)| (project, Some(id)));
        let work = node.create_session(self.snapshot.projects[project].id.clone(), id);
        let revision = self.navigation_revision;
        self.creation_error = None;
        self.creating_session = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |shell, cx| {
                shell.creating_session = None;
                match result {
                    Ok(Creation::Created(session)) => {
                        let index = match shell
                            .snapshot
                            .sessions
                            .iter()
                            .position(|item| item.id == session.id)
                        {
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
                            shell.select_session(index, cx);
                        }
                    }
                    Ok(Creation::ReadFailed { id, error }) => {
                        shell.creation_readback = Some((project, id));
                        shell.creation_error =
                            Some((false, format!("节点已创建会话，读取详情失败：{error}")));
                    }
                    Ok(Creation::WriteFailed(error)) if rejected(&error) => {
                        shell.creation_error = Some((false, format!("新建对话失败：{error}")));
                    }
                    _ => {
                        shell.creation_error =
                            Some((true, "新建结果未知。重新连接后可查看节点会话列表。".into()));
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
            self.navigation_revision += 1;
            if let Some(draft) = self.drafts.get_mut(&self.session) {
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

    fn apply_frame(&mut self, index: usize, frame: SessionFrame, cx: &mut Context<Self>) {
        if self
            .actions
            .get(&index)
            .is_some_and(|request| request.observed(&frame))
        {
            self.actions.remove(&index);
        }
        let draft = self.drafts.entry(Some(index)).or_default();
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
        let refresh_config = frame.event.is_empty() || frame.event == "access";
        let clear_config = frame.access.is_none();
        self.frame = Some(frame);
        if refresh_config {
            self.observe_config(cx);
        } else if clear_config {
            self.composer_config.reset_observation();
        } else {
            self.composer_config
                .close_if_locked(&self.session_controls());
        }
    }

    fn session_controls(&self) -> SessionControls {
        let draft = self.drafts.get(&self.session);
        let mut pending = draft.map(Draft::pending).unwrap_or_default();
        if let Some(request) = self.session.and_then(|index| self.actions.get(&index)) {
            request.apply_pending(&mut pending);
        }
        if let Some(index) = self.session {
            self.composer_config.pending(index, &mut pending);
        }
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
            pending,
        })
    }

    fn send(&mut self, cx: &mut Context<Self>) {
        let Some((node, index)) = self.node.as_ref().zip(self.session) else {
            return;
        };
        let Some(frame) = self.frame.as_ref().filter(|_| self.session_controls().send) else {
            return;
        };
        let draft = self.drafts.entry(Some(index)).or_default();
        if !draft.input.can_send() {
            return;
        }
        if self
            .actions
            .get(&index)
            .is_some_and(|request| !request.locked())
        {
            self.actions.remove(&index);
        }
        draft.begin();
        let request = node.prompt(frame.session_id.clone(), draft.input.text.clone());
        cx.spawn(async move |this, cx| {
            let result = request.await;
            this.update(cx, |shell, cx| {
                let draft = shell.drafts.entry(Some(index)).or_default();
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
            .get(&Some(index))
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
                let draft = shell.drafts.entry(Some(index)).or_default();
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

    /// Cycle to the next preset from the shared contract.
    fn next_theme(&mut self, cx: &mut Context<Self>) {
        let presets = Palette::presets();
        let index = presets
            .iter()
            .position(|(id, _)| *id == self.theme_id)
            .unwrap_or(0);
        self.theme_id = presets[(index + 1) % presets.len()].0.clone();
        self.palette = Palette::from_preset(&self.theme_id, self.dark, false);
        cx.notify();
    }

    fn toggle_dark(&mut self, cx: &mut Context<Self>) {
        self.dark = !self.dark;
        self.palette = Palette::from_preset(&self.theme_id, self.dark, false);
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
            .unwrap_or("工作台")
    }

    fn button(&self, id: &'static str, label: &str) -> gpui::Stateful<Div> {
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
        let mut sessions: HashMap<&str, Vec<(usize, &crate::data::SessionItem)>> = HashMap::new();
        for (index, session) in self.snapshot.sessions.iter().enumerate() {
            sessions
                .entry(&session.project_id)
                .or_default()
                .push((index, session));
        }
        for group in sessions.values_mut() {
            group.sort_by_key(|(_, session)| std::cmp::Reverse(session.updated_at));
        }
        let projects = self
            .snapshot
            .projects
            .iter()
            .enumerate()
            .map(|(index, project)| {
                div()
                    .flex()
                    .flex_col()
                    .mb_2()
                    .child(
                        div()
                            .id(("project", index))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .h(px(30.0))
                            .px(px(10.0))
                            .rounded(px(10.0))
                            .cursor_pointer()
                            .hover(move |row| row.bg(palette.hover))
                            .child(icon("folder", 15.0, palette.muted))
                            .child(
                                self.text(project.name.clone(), 14.0, palette.text)
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate(),
                            )
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.select_project(index, cx)),
                            ),
                    )
                    .children(
                        sessions
                            .get(project.id.as_str())
                            .into_iter()
                            .flatten()
                            .map(|&(index, session)| self.session_row(index, session, cx)),
                    )
            });
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
                        self.icon_button("fold-navigation", "panel-left", 25.0)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_navigation(cx);
                            })),
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
                                    shell.focus_composer(window);
                                    shell.create_session(cx);
                                }))
                            })
                            .when(!enabled, |button| button.opacity(0.6).cursor_default()),
                    )
                    .when_some(self.creation_error.as_ref(), |sidebar, (_, error)| {
                        sidebar.child(self.text(error.clone(), 12.0, palette.danger).p_2())
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
                    .child(
                        self.text("最近", 12.0, palette.muted)
                            .px(px(10.0))
                            .pb(px(3.0)),
                    )
                    .children(projects)
                    .children(
                        sessions
                            .iter()
                            .filter(|(id, _)| {
                                !self
                                    .snapshot
                                    .projects
                                    .iter()
                                    .any(|project| project.id == **id)
                            })
                            .flat_map(|(_, group)| group.iter())
                            .map(|&(index, session)| self.session_row(index, session, cx)),
                    ),
            )
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
        let title = if session.title.is_empty() {
            "未命名会话"
        } else {
            &session.title
        };
        div()
            .id(("session", index))
            .flex()
            .items_center()
            .h(px(30.0))
            .pl(px(33.0))
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
                .truncate(),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.select_session(index, cx)))
    }

    fn close_navigation(&mut self, cx: &mut Context<Self>) {
        if self.overlay == Some(Overlay::Navigation) {
            self.overlay = None;
        } else {
            self.navigation_open = false;
        }
        cx.notify();
    }

    fn toggle_navigation(&mut self, narrow: bool, cx: &mut Context<Self>) {
        if narrow {
            self.overlay =
                (self.overlay != Some(Overlay::Navigation)).then_some(Overlay::Navigation);
        } else {
            self.navigation_open = !self.navigation_open;
        }
        cx.notify();
    }

    fn toggle_inspector(&mut self, tab: project_files::Tab, narrow: bool, cx: &mut Context<Self>) {
        let same = self.files.tab == tab;
        if narrow {
            self.overlay =
                (!(same && self.overlay == Some(Overlay::Inspector))).then_some(Overlay::Inspector);
        } else {
            self.overview_open = !same || !self.overview_open;
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
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_inspector(tab, inspector_overlay, cx)
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
                    .bg(palette.hover)
                    .on_click(cx.listener(|this, _, window, _| this.focus_composer(window))),
            )
            .child(
                self.icon_button("rail-navigation", "folder", 34.0)
                    .h(px(34.0))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_navigation(false, cx))),
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
                self.icon_button("appearance", "settings", 34.0)
                    .h(px(34.0))
                    .on_click(cx.listener(|this, event: &gpui::ClickEvent, _, cx| {
                        if event.modifiers().shift {
                            this.next_theme(cx);
                        } else {
                            this.toggle_dark(cx);
                        }
                    })),
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
                                self.history_position > 0,
                            ),
                            (
                                true,
                                "session-forward",
                                "history-forward",
                                self.history_position + 1 < self.session_history.len(),
                            ),
                        ]
                        .into_iter()
                        .filter(|_| !compact)
                        .map(|(forward, id, shape, enabled)| {
                            self.icon_button(id, shape, 24.0)
                                .when(!enabled, |button| button.opacity(0.35).cursor_default())
                                .when(enabled, |button| {
                                    button.on_click(cx.listener(move |this, _, _, cx| {
                                        this.step_history(forward, cx)
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
                    .window_control_area(WindowControlArea::Drag)
                    .child(
                        self.text(self.title().to_owned(), 14.0, palette.text)
                            .font_weight(FontWeight::MEDIUM)
                            .truncate(),
                    ),
            )
            .child(
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
                        let tab = if tab == project_files::Tab::Overview
                            && self.files.tab.browser_id().is_some()
                        {
                            self.files.tab
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
                            .when(active, |button| button.bg(palette.subtle))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_inspector(tab, overlay, cx)
                            }))
                    }),
                ),
            )
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
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.inspector_full = !this.inspector_full;
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
        let prompt_width = (reading_width * 0.9).min((reading_width * 0.77).max(320.0));
        let show_rows = self.session.is_some() && self.session == self.transcript_session;
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
                        .items_end()
                        .child(
                            self.text(text.clone(), 16.0, palette.text)
                                .line_height(px(24.8))
                                .max_w(px(prompt_width))
                                .px(px(15.0))
                                .py(px(10.0))
                                .rounded(px(19.0))
                                .bg(palette.subtle),
                        )
                        .into_any_element(),
                    Row::Assistant { key, view, .. } => {
                        self.rich_answer(key, view).into_any_element()
                    }
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
                    .gap(px(22.0))
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
        let draft = self.drafts.get(&self.session);
        let loaded_sample = self.node.is_none()
            && self.session.is_some()
            && self.session == self.transcript_session;
        let metrics = &self.snapshot.metrics;
        let controls = self.session_controls();
        let can_send = controls.send && draft.is_some_and(|draft| draft.input.can_send());
        let checking = self
            .session
            .is_some_and(|index| self.checking_receipts.contains(&index));
        let can_check = controls.online && !checking && draft.and_then(Draft::receipt).is_some();
        let action = self.session.and_then(|index| self.actions.get(&index));
        let notice = action
            .map(session_action::Request::status)
            .or_else(|| draft.and_then(|draft| draft.notice.as_deref()))
            .or_else(|| self.config_notice());
        let status = if checking {
            Some("正在核对发送回执…")
        } else {
            notice.or_else(|| {
                if self.node.is_none() {
                    Some("示例 · 草稿保存在当前窗口")
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
                    .child(self.approvals(&controls, cx))
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
                            .child(self.input_element(cx))
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
                                    .h(px(28.0))
                                    .pl(px(4.0))
                                    .child(self.config_toolbar(&controls, cx))
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
                            .when(loaded_sample, |bar| {
                                bar.child(
                                    self.text(
                                        format!(
                                            "↑ {}   ↓ {}   上下文 {}",
                                            tokens(metrics.input),
                                            tokens(metrics.output),
                                            percent(self.snapshot.context.percent)
                                        ),
                                        11.0,
                                        palette.muted,
                                    )
                                    .truncate(),
                                )
                            }),
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
        let width = f32::from(window.viewport_size().width);
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
        let docked = self.overview_open && !overlay;
        let full = docked && self.inspector_full;
        if full || overlay && self.overlay.is_some() {
            self.composer_config.close_menu();
        }
        let browser_visible = docked || overlay && self.overlay == Some(Overlay::Inspector);
        self.browser.hide_inactive(
            self.files
                .tab
                .browser_id()
                .filter(|_| browser_visible && !self.composer_config.menu_open()),
            window,
            cx,
        );
        let inset = if compact { 14.0 } else { READING_INSET };
        let reading_width =
            (remaining - if docked { dock_width } else { 0.0 } - inset * 2.0).min(READING_WIDTH);
        let palette = self.palette;
        div()
            .id("workbench")
            .on_key_down(cx.listener(|shell, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && shell.composer_config.close_menu() {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
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
                    .when(!full, |body| {
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
                                    .on_click(cx.listener(|this, _, _, cx| {
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
            Row::Assistant { key, source, view } => Some((key.as_str(), (source, view))),
            _ => None,
        })
        .collect();
    let mut rows = Vec::new();
    for part in conversation.parts {
        match part {
            Part::Message {
                role: "user", text, ..
            } => rows.push(Row::User(text)),
            Part::Message {
                key,
                reading_key,
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
