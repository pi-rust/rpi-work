//! Session metadata edits verified against the node's session record.

use gpui::{Context, Div, FocusHandle, Task, Window, div, prelude::*, px};
use work_client::WorkspaceClient;
use work_core::api::Capabilities;
use work_viewmodel::config_requests::bounded;

use super::{Shell, editor::Buffer, rejected};
use crate::{data::SessionItem, icons::icon};

#[derive(Clone)]
pub(crate) enum Edit {
    Rename(String),
    Pin(bool),
    Archive(bool),
}

#[derive(Clone)]
pub(crate) struct Request {
    index: usize,
    id: String,
    project: String,
    edit: Edit,
}

pub(crate) enum Outcome {
    Verified(SessionItem),
    Rejected(String),
    Unverified(String),
}

impl Request {
    pub(crate) async fn run(&self, client: &WorkspaceClient, verify_only: bool) -> Outcome {
        if !verify_only {
            let result = match &self.edit {
                Edit::Rename(title) => {
                    bounded("重命名会话", client.rename_session(&self.id, title))
                        .await
                        .map(|reply| reply.session_id)
                }
                Edit::Pin(pinned) => bounded(
                    "更新会话置顶",
                    client.session_preferences(&self.id, None, Some(*pinned)),
                )
                .await
                .map(|reply| reply.session_id),
                Edit::Archive(archived) => bounded(
                    "更新会话归档",
                    client.session_preferences(&self.id, Some(*archived), None),
                )
                .await
                .map(|reply| reply.session_id),
            };
            match result {
                Err(error) if rejected(&error) => return Outcome::Rejected(error.to_string()),
                Ok(id) if id != self.id => {
                    return Outcome::Unverified("节点返回了不同的会话标识。".into());
                }
                _ => {}
            }
        }
        let session = match bounded("核对会话资料", client.get_session(&self.id)).await {
            Ok(session) => SessionItem::from(session),
            Err(error) => return Outcome::Unverified(error.to_string()),
        };
        if session.id != self.id || session.project_id != self.project {
            return Outcome::Unverified("节点返回的会话或项目与请求不一致。".into());
        }
        let matched = match &self.edit {
            Edit::Rename(title) => session.title == *title,
            Edit::Pin(pinned) => session.pinned == *pinned,
            Edit::Archive(archived) => session.archived == *archived,
        };
        if matched {
            Outcome::Verified(session)
        } else {
            Outcome::Unverified("节点尚未显示请求的会话资料。".into())
        }
    }
}

#[derive(Clone, Copy)]
enum Dialog {
    Actions(usize),
    Archives,
}

#[derive(Default)]
pub(super) struct Panel {
    dialog: Option<Dialog>,
    dialog_revision: u64,
    capabilities: Option<Capabilities>,
    reading_capabilities: Option<Task<()>>,
    capability_error: Option<String>,
    pub(super) input: Buffer,
    pending: Option<Task<()>>,
    request: Option<Request>,
    error: Option<String>,
}

impl Panel {
    pub(super) fn open(&self) -> bool {
        self.dialog.is_some()
    }

    pub(super) fn has_request(&self) -> bool {
        self.request.is_some()
    }

    pub(super) fn editing(&self) -> bool {
        matches!(self.dialog, Some(Dialog::Actions(_)))
            && self.pending.is_none()
            && self.request.is_none()
    }

    pub(super) fn supports(&self, action: &str) -> bool {
        self.capabilities
            .as_ref()
            .is_some_and(|caps| caps.supported_actions.iter().any(|item| item == action))
    }

    pub(super) fn blocks_session(&self, session: Option<usize>) -> bool {
        self.request.as_ref().is_some_and(|request| {
            Some(request.index) == session && matches!(request.edit, Edit::Archive(true))
        })
    }
}

impl Shell {
    fn open_session_management(
        &mut self,
        dialog: Dialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.project_form.open || self.question.is_some() || self.comment.is_some() {
            return;
        }
        self.close_resource_page();
        if self.overlay == Some(super::Overlay::Navigation) {
            self.dismiss_drawer(window);
        }
        self.project_menu = None;
        self.close_session_filter_menu();
        self.read_session_management_capabilities(cx);
        if let Some(draft) = self.drafts.get_mut(&self.draft_key()) {
            draft.input.finish_composition();
        }
        if let Dialog::Actions(index) = dialog {
            let panel = &mut self.session_management;
            let title = match panel.request.as_ref() {
                Some(Request {
                    index: target,
                    edit: Edit::Rename(title),
                    ..
                }) if *target == index => title,
                _ => &self.snapshot.sessions[index].title,
            };
            panel.input = Buffer::default();
            panel.input.replace(None, title);
        }
        self.session_management.dialog = Some(dialog);
        self.session_management.dialog_revision += 1;
        self.input_layout = None;
        self.input_dragging = false;
        self.focus_composer(window);
        cx.notify();
    }

    pub(super) fn read_session_management_capabilities(&mut self, cx: &mut Context<Self>) {
        let panel = &mut self.session_management;
        if panel.capabilities.is_some() || panel.reading_capabilities.is_some() {
            return;
        }
        let Some(node) = self.node.as_ref() else {
            return;
        };
        let request = node.read_capabilities();
        panel.capability_error = None;
        panel.reading_capabilities = Some(cx.spawn(async move |this, cx| {
            let result = request
                .await
                .map_err(|error| error.to_string())
                .and_then(|reply| reply.map_err(|error| error.to_string()));
            let _ = this.update(cx, |shell, cx| {
                let panel = &mut shell.session_management;
                panel.reading_capabilities = None;
                match result {
                    Ok(capabilities) => panel.capabilities = Some(capabilities),
                    Err(error) => {
                        panel.capability_error = Some(format!("读取节点操作能力失败：{error}"))
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(super) fn close_session_management(&mut self) -> bool {
        if self.session_management.dialog.take().is_none() {
            return false;
        }
        self.input_layout = None;
        self.input_dragging = false;
        if self.session_management.request.is_none() {
            self.session_management.error = None;
        }
        true
    }

    pub(super) fn rename_from_dialog(&mut self, cx: &mut Context<Self>) {
        let panel = &self.session_management;
        let Some(Dialog::Actions(index)) = panel.dialog else {
            return;
        };
        let title = panel.input.text.trim();
        if panel.editing()
            && panel.input.can_send()
            && title.chars().count() <= 100
            && !title.chars().any(char::is_control)
        {
            self.edit_session(index, Edit::Rename(title.into()), cx);
        }
    }

    fn edit_session(&mut self, index: usize, edit: Edit, cx: &mut Context<Self>) {
        let Some(session) = self.snapshot.sessions.get(index) else {
            return;
        };
        if self.session_management.request.is_some()
            || self.creating_session.is_some()
            || self.session_fork.in_flight()
        {
            return;
        }
        let action = if matches!(edit, Edit::Rename(_)) {
            "session_organization"
        } else {
            "session_metadata"
        };
        if !self.session_management.supports(action) {
            return;
        }
        let id = if matches!(edit, Edit::Archive(false)) && session.archived {
            session.id.clone()
        } else if self.session == Some(index) && !session.archived {
            let Some(frame) = self.frame.as_ref() else {
                return;
            };
            let controls = self.session_controls();
            if !controls.online
                || matches!(edit, Edit::Archive(true)) && !self.session_idle_for_history_change()
            {
                return;
            }
            frame.session_id.clone()
        } else {
            return;
        };
        self.submit_session_edit(
            Request {
                index,
                id,
                project: session.project_id.clone(),
                edit,
            },
            false,
            cx,
        );
    }

    pub(super) fn session_idle_for_history_change(&self) -> bool {
        let controls = self.session_controls();
        let pending = self
            .drafts
            .get(&self.draft_key())
            .map(crate::prompt::Draft::pending)
            .unwrap_or_default();
        controls.online
            && !controls.busy
            && !pending.submitting
            && !pending.awaiting_reply
            && !pending.uncertain
            && self.action_available()
            && !self.files.checking_review()
            && self
                .frame
                .as_ref()
                .is_some_and(|frame| frame.dialogs.is_empty() && !frame.pending_resume)
    }

    fn submit_session_edit(&mut self, request: Request, verify_only: bool, cx: &mut Context<Self>) {
        let Some(node) = self.node.as_ref() else {
            return;
        };
        if self.session_management.pending.is_some() {
            return;
        }
        let dialog_revision = self.session_management.dialog_revision;
        self.session_management.request = Some(request.clone());
        self.session_management.error = None;
        let check = node.edit_session(request.clone(), verify_only);
        self.session_management.pending = Some(cx.spawn(async move |this, cx| {
            let outcome = check
                .await
                .unwrap_or_else(|error| Outcome::Unverified(error.to_string()));
            let _ = this.update(cx, |shell, cx| {
                shell.session_management.pending = None;
                match outcome {
                    Outcome::Verified(session) => {
                        let archived = session.archived;
                        let panel = &mut shell.session_management;
                        let original_archives = panel.dialog_revision == dialog_revision
                            && matches!(panel.dialog, Some(Dialog::Archives));
                        if matches!(request.edit, Edit::Rename(_))
                            && matches!(panel.dialog, Some(Dialog::Actions(index)) if index == request.index) {
                            shell.session_management.input = Buffer::default();
                            shell.session_management.input.replace(None, &session.title);
                            shell.input_layout = None;
                        }
                        shell.snapshot.sessions[request.index] = session;
                        shell.session_catalog_revision += 1;
                        shell.session_management.request = None;
                        if archived && shell.session == Some(request.index) {
                            shell.close_session_management();
                            if let Some(project) = shell.project {
                                shell.select_project(project, cx);
                            } else {
                                shell.navigate_session(None, cx);
                            }
                        } else if matches!(request.edit, Edit::Archive(false)) && original_archives {
                            shell.close_session_management();
                            shell.select_session(request.index, cx);
                        }
                    }
                    Outcome::Rejected(error) => {
                        shell.session_management.request = None;
                        shell.session_management.error = Some(error);
                    }
                    Outcome::Unverified(error) => shell.session_management.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn session_actions_button(&self, cx: &Context<Self>) -> impl IntoElement {
        self.icon_button("session-actions", "ellipsis", 26.0)
            .on_click(cx.listener(|shell, _, window, cx| {
                if let Some(index) = shell.session {
                    shell.open_session_management(Dialog::Actions(index), window, cx);
                }
            }))
    }

    pub(super) fn archives_button(&self, cx: &Context<Self>) -> impl IntoElement {
        self.navigation_control(self.button("archived-sessions", ""), true)
            .justify_start()
            .gap_2()
            .mb_2()
            .child(icon("folder", 15.0, self.palette.muted))
            .child(self.text("已归档", 12.0, self.palette.muted))
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.open_session_management(Dialog::Archives, window, cx);
            }))
    }

    pub(super) fn session_management_dialog(&self, cx: &mut Context<Self>) -> Option<Div> {
        let panel = &self.session_management;
        let dialog = panel.dialog?;
        let palette = self.palette;
        let enabled = panel.request.is_none()
            && self.creating_session.is_none()
            && !self.session_fork.in_flight();
        let metadata = enabled && panel.supports("session_metadata");
        let rename = enabled && panel.supports("session_organization");
        let title = match dialog {
            Dialog::Actions(_) => "会话操作",
            Dialog::Archives => "已归档",
        };
        let mut content = div()
            .id("session-management")
            .flex()
            .flex_col()
            .gap_3()
            .w_full()
            .max_w(px(450.0))
            .max_h_full()
            .overflow_y_scroll()
            .p_4()
            .bg(palette.bg)
            .border_1()
            .border_color(palette.line)
            .rounded(px(12.0))
            .shadow_lg()
            .when(!panel.editing(), |dialog| dialog.track_focus(&self.focus))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.text(title, 16.0, palette.text))
                    .child(
                        self.icon_button("close-session-management", "x", 24.0)
                            .on_click(cx.listener(|shell, _, _, cx| {
                                shell.close_session_management();
                                cx.notify();
                            })),
                    ),
            );
        match dialog {
            Dialog::Actions(index) => {
                let session = &self.snapshot.sessions[index];
                let online = self.session_controls().online;
                let can_fork = self.can_fork();
                content = content
                    .child(self.text("会话名称", 13.0, palette.text))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .border_1()
                                    .border_color(palette.strong_line)
                                    .rounded(px(6.0))
                                    .p_1()
                                    .when(panel.editing(), |field| {
                                        field.child(self.input_element(cx))
                                    })
                                    .when(!panel.editing(), |field| {
                                        field.child(self.text(
                                            panel.input.text.clone(),
                                            14.0,
                                            palette.muted,
                                        ))
                                    }),
                            )
                            .child(
                                self.button("save-session-title", "保存")
                                    .when(!(rename && online), |button| {
                                        button.opacity(0.45).cursor_default()
                                    })
                                    .when(rename && online, |button| {
                                        button.bg(palette.subtle).on_click(cx.listener(
                                            |shell, _, _, cx| shell.rename_from_dialog(cx),
                                        ))
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                self.button(
                                    "pin-session",
                                    if session.pinned {
                                        "取消置顶"
                                    } else {
                                        "置顶会话"
                                    },
                                )
                                .when(!(metadata && online), |button| {
                                    button.opacity(0.45).cursor_default()
                                })
                                .when(
                                    metadata && online,
                                    |button| {
                                        let desired = !session.pinned;
                                        button.on_click(cx.listener(move |shell, _, _, cx| {
                                            shell.edit_session(index, Edit::Pin(desired), cx)
                                        }))
                                    },
                                ),
                            )
                            .child(
                                self.button("fork-session", "从历史分叉")
                                    .when(!can_fork, |button| button.opacity(0.45).cursor_default())
                                    .when(can_fork, |button| {
                                        button.on_click(cx.listener(|shell, _, window, cx| {
                                            shell.close_session_management();
                                            shell.focus_composer(window);
                                            shell.start_fork(None, cx);
                                        }))
                                    }),
                            )
                            .child(
                                self.button("archive-session", "归档会话")
                                    .when(
                                        !(metadata && self.session_idle_for_history_change()),
                                        |button| button.opacity(0.45).cursor_default(),
                                    )
                                    .when(
                                        metadata && self.session_idle_for_history_change(),
                                        |button| {
                                            button.on_click(cx.listener(move |shell, _, _, cx| {
                                                shell.edit_session(index, Edit::Archive(true), cx)
                                            }))
                                        },
                                    ),
                            ),
                    );
            }
            Dialog::Archives => {
                content = content
                    .children(self.archived_project_rows(cx))
                    .child(self.text("已归档会话", 13.0, palette.muted))
                    .child(self.archived_sessions(None, cx));
            }
        }
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(gpui::rgba(0x00000039))
                .flex()
                .items_center()
                .justify_center()
                .px_4()
                .child(content.child(self.session_management_feedback(None, cx))),
        )
    }

    pub(super) fn archive_settings(&self, focus: &FocusHandle, cx: &Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.archived_sessions(Some(focus), cx))
            .child(self.session_management_feedback(Some(focus), cx))
    }

    fn archived_sessions(&self, focus: Option<&FocusHandle>, cx: &Context<Self>) -> Div {
        let panel = &self.session_management;
        let metadata = panel.supports("session_metadata")
            && panel.request.is_none()
            && self.creating_session.is_none()
            && !self.session_fork.in_flight();
        let palette = self.palette;
        let mut archived = self
            .snapshot
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| session.archived)
            .peekable();
        div()
            .flex()
            .flex_col()
            .when(archived.peek().is_none(), |list| {
                list.child(self.text("没有已归档的聊天。", 13.0, palette.muted))
            })
            .children(archived.map(|(index, session)| {
                let project = self
                    .snapshot
                    .projects
                    .iter()
                    .find(|project| project.id == session.project_id);
                let focus = focus.cloned();
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_h(px(60.0))
                    .border_t_1()
                    .border_color(palette.line)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                self.text(session.display_title().to_owned(), 14.0, palette.text)
                                    .truncate(),
                            )
                            .when_some(project, |label, project| {
                                label.child(
                                    self.text(project.name.clone(), 12.0, palette.muted)
                                        .truncate(),
                                )
                            }),
                    )
                    .child(
                        self.button(("restore-session", index), "恢复")
                            .when(focus.is_some(), |button| button.h(px(44.0)))
                            .when(!metadata, |button| button.opacity(0.45).cursor_default())
                            .when(metadata, |button| {
                                button
                                    .when(focus.is_some(), |button| button.tab_index(0))
                                    .focus(|button| button.bg(palette.hover))
                                    .on_click(cx.listener(move |shell, _, window, cx| {
                                        if let Some(focus) = &focus {
                                            focus.focus(window);
                                        }
                                        shell.edit_session(index, Edit::Archive(false), cx);
                                    }))
                            }),
                    )
            }))
    }

    fn session_management_feedback(&self, focus: Option<&FocusHandle>, cx: &Context<Self>) -> Div {
        let panel = &self.session_management;
        let palette = self.palette;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .when(panel.reading_capabilities.is_some(), |status| {
                status.child(self.text("正在读取节点操作能力…", 12.0, palette.muted))
            })
            .when_some(panel.capability_error.as_ref(), |status, error| {
                status.child(self.text(error.clone(), 12.0, palette.danger))
            })
            .when(
                focus.is_some()
                    && panel.capabilities.is_some()
                    && !panel.supports("session_metadata"),
                |status| {
                    status.child(self.text(
                        "当前节点未提供恢复归档会话的能力。",
                        12.0,
                        palette.muted,
                    ))
                },
            )
            .when(panel.pending.is_some(), |status| {
                status.child(self.text("正在核对节点会话资料…", 12.0, palette.muted))
            })
            .when_some(panel.error.as_ref(), |status, error| {
                status.child(self.text(error.clone(), 12.0, palette.danger))
            })
            .when_some(
                panel.request.as_ref().filter(|_| panel.pending.is_none()),
                |status, request| {
                    let request = request.clone();
                    let focus = focus.cloned();
                    status.child(
                        div().flex().child(
                            self.button("verify-session-edit", "核对会话资料")
                                .when(focus.is_some(), |button| button.tab_index(0))
                                .focus(|button| button.bg(palette.hover))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    if let Some(focus) = &focus {
                                        focus.focus(window);
                                    }
                                    shell.submit_session_edit(request.clone(), true, cx);
                                })),
                        ),
                    )
                },
            )
    }
}
