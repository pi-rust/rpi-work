//! Fork the observed native session and verify the destination in the node catalog.

use gpui::{Context, Div, Stateful, Task, div, prelude::*};
use work_client::{WorkspaceClient, canonical_entry_id};
use work_viewmodel::config_requests::bounded;

use super::{Shell, rejected};
use crate::{data::SessionItem, icons::icon};

pub(crate) struct Request {
    index: usize,
    source: String,
    project: String,
    anchor: Option<String>,
    title: String,
}

pub(crate) enum Outcome {
    Created(SessionItem),
    Rejected(String),
    ReadFailed { id: String, error: String },
    Unknown,
}

impl Request {
    pub(crate) async fn run(&self, client: &WorkspaceClient, readback: Option<String>) -> Outcome {
        let id = match readback {
            Some(id) => id,
            None => match bounded(
                "分叉会话",
                client.fork_session(&self.source, self.anchor.as_deref(), Some(&self.title)),
            )
            .await
            {
                Ok(reply)
                    if !reply.session_id.trim().is_empty()
                        && reply.session_id != self.source
                        && reply.source_session_id.as_deref() == Some(self.source.as_str())
                        && reply.entry_id == self.anchor
                        && reply.title.as_deref() == Some(self.title.as_str()) =>
                {
                    reply.session_id
                }
                Err(error) if rejected(&error) => return Outcome::Rejected(error.to_string()),
                _ => return Outcome::Unknown,
            },
        };
        let error = match bounded("核对分叉会话列表", client.sessions()).await {
            Ok(listed) => {
                if let Some(session) = listed.into_iter().find(|session| {
                    session.session_id == id
                        && !session.archived
                        && session.project_id.as_deref() == Some(self.project.as_str())
                        && session.title.as_deref() == Some(self.title.as_str())
                }) {
                    return Outcome::Created(session.into());
                }
                "节点尚未确认同项目中的分叉会话。".into()
            }
            Err(error) => error.to_string(),
        };
        Outcome::ReadFailed { id, error }
    }
}

#[derive(Default)]
pub(super) struct Panel {
    task: Option<Task<()>>,
    source: Option<usize>,
    follow: bool,
    readback: Option<(Request, String)>,
    unknown: bool,
    message: Option<String>,
}

impl Panel {
    pub(super) fn in_flight(&self) -> bool {
        self.task.is_some()
    }

    pub(super) fn blocks_session(&self, index: Option<usize>) -> bool {
        self.in_flight() && self.source == index
    }

    pub(super) fn invalidate_follow(&mut self) {
        self.follow = false;
    }
}

fn fork_title(source: &str) -> String {
    let suffix = "（分叉）";
    let base: String = source
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(120 - suffix.chars().count())
        .collect();
    format!("{}{suffix}", base.trim_end())
}

impl Shell {
    pub(super) fn can_fork(&self) -> bool {
        let Some(session) = self
            .session
            .and_then(|index| self.snapshot.sessions.get(index))
        else {
            return false;
        };
        let pending = self.pending_session_requests();
        if self.node.is_none()
            || session.archived
            || session.project_id.is_empty()
            || self.creating_session.is_some()
            || self.creation_readback.is_some()
            || self.session_management.has_request()
            || !self.session_management.supports("fork")
            || self.session_fork.in_flight()
            || self.session_fork.unknown
            || self.session_fork.readback.is_some()
            || self.settings.open
            || self.question.is_some()
            || self.project_form.open
            || self.comment.is_some()
            || pending.configuring
            || pending.uncertain
            || !self.session_idle_for_history_change()
        {
            return false;
        }
        let Some(frame) = &self.frame else {
            return false;
        };
        frame.session_id == session.id
            && frame
                .access
                .as_ref()
                .is_some_and(|access| access.descriptor.identity.protocol == "rpi")
    }

    pub(super) fn supports_reply_fork(&self) -> bool {
        self.frame
            .as_ref()
            .and_then(|frame| frame.access.as_ref())
            .is_some_and(|access| access.supports_command("navigate"))
    }

    pub(super) fn start_fork(&mut self, anchor: Option<String>, cx: &mut Context<Self>) {
        let Some(index) = self.session.filter(|_| self.can_fork()) else {
            return;
        };
        if let Some(id) = anchor.as_deref()
            && !(self.supports_reply_fork()
                && self.frame.as_ref().is_some_and(|frame| {
                    frame.history.iter().any(|entry| {
                        entry["role"] == "assistant" && canonical_entry_id(entry) == Some(id)
                    })
                }))
        {
            return;
        }
        let session = &self.snapshot.sessions[index];
        let request = Request {
            index,
            source: session.id.clone(),
            project: session.project_id.clone(),
            anchor,
            title: fork_title(session.display_title()),
        };
        self.session_fork.follow = true;
        self.submit_fork(request, None, cx);
    }

    fn submit_fork(&mut self, request: Request, id: Option<String>, cx: &mut Context<Self>) {
        let Some(node) = self.node.as_ref() else {
            return;
        };
        let index = request.index;
        let work = node.fork_session(request, id);
        self.session_fork.source = Some(index);
        self.session_fork.message = None;
        self.session_fork.task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |shell, cx| {
                shell.session_fork.task = None;
                let (request, outcome) = match result {
                    Ok(result) => result,
                    Err(_) => {
                        shell.session_fork.unknown = true;
                        shell.session_fork.message =
                            Some("分叉结果未知。重新连接后可查看会话列表。".into());
                        cx.notify();
                        return;
                    }
                };
                match outcome {
                    Outcome::Created(session) => {
                        let destination = shell
                            .snapshot
                            .sessions
                            .iter()
                            .position(|item| item.id == session.id)
                            .unwrap_or(shell.snapshot.sessions.len());
                        if let Some(item) = shell.snapshot.sessions.get_mut(destination) {
                            *item = session;
                        } else {
                            shell.snapshot.sessions.push(session);
                        }
                        shell.session_catalog_revision += 1;
                        if shell.session_fork.follow
                            && shell.session == Some(index)
                            && shell.session_controls().online
                            && !shell.settings.open
                            && !shell.project_form.open
                            && !shell.session_management.open()
                            && shell.question.is_none()
                            && shell.comment.is_none()
                        {
                            shell.select_session(destination, cx);
                        } else {
                            shell.session_fork.message = Some("分叉会话已加入列表。".into());
                        }
                    }
                    Outcome::Rejected(error) => {
                        shell.session_fork.message = Some(format!("分叉失败：{error}"))
                    }
                    Outcome::ReadFailed { id, error } => {
                        shell.session_fork.readback = Some((request, id));
                        shell.session_fork.message =
                            Some(format!("节点已受理分叉，读取新会话失败：{error}"));
                    }
                    Outcome::Unknown => {
                        shell.session_fork.unknown = true;
                        shell.session_fork.message =
                            Some("分叉结果未知。重新连接后可查看会话列表。".into());
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn fork_reply_button(
        &self,
        key: &str,
        anchor: &str,
        progress: bool,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let anchor = anchor.to_owned();
        let palette = self.palette;
        let button = self
            .reply_action(
                gpui::SharedString::from(format!("fork-reply/{key}")),
                progress,
            )
            .child(icon("git-branch", 15.0, palette.muted))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.focus_composer(window);
                shell.start_fork(Some(anchor.clone()), cx);
            }));
        self.navigation_tooltip(button, "从此处开始新对话")
    }

    pub(super) fn fork_status(&self, cx: &Context<Self>) -> Option<Div> {
        let panel = &self.session_fork;
        let status = if panel.in_flight() {
            "正在分叉并核对新会话…"
        } else {
            panel.message.as_deref()?
        };
        Some(
            div()
                .flex()
                .items_center()
                .flex_wrap()
                .gap_2()
                .child(
                    self.text(status.to_owned(), 12.0, self.palette.muted)
                        .min_w_0()
                        .max_w_full()
                        .whitespace_normal(),
                )
                .when(panel.readback.is_some(), |row| {
                    row.child(
                        self.button("verify-fork", "核对新会话")
                            .tab_index(0)
                            .tab_stop(self.navigation_keyboard_enabled() && self.overlay.is_none())
                            .focus(|button| button.bg(self.palette.hover))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.focus_composer(window);
                                if let Some((request, id)) = shell.session_fork.readback.take() {
                                    shell.submit_fork(request, Some(id), cx);
                                }
                            })),
                    )
                }),
        )
    }
}
