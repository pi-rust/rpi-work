//! Stop and approval requests for the currently observed native session.

use gpui::{Context, IntoElement, div, prelude::*, px};
use serde_json::Value;
use work_client::ClientError;
use work_core::api::{
    PendingDialog,
    realtime::{OperationOutcome, OperationRef, OperationStatus},
};
use work_core::{ConnectionState, RunPhase};
use work_viewmodel::session_controls::{PendingRequests, SessionControls};
use work_viewmodel::session_receipts::interpret_receipt;

use super::{READING_WIDTH, Shell, rejected};
use crate::data::SessionFrame;

#[derive(Clone)]
pub(crate) enum Action {
    Abort,
    Reply { request_id: String, value: String },
}

enum Stage {
    Sending,
    Accepted,
    Unknown(Option<OperationRef>),
    Rejected(String),
}

pub(super) struct Request {
    ticket: u64,
    action: Action,
    stage: Stage,
    checking: bool,
}

impl Request {
    pub(super) fn locked(&self) -> bool {
        !matches!(self.stage, Stage::Rejected(_))
    }

    pub(super) fn apply_pending(&self, pending: &mut PendingRequests) {
        pending.cancelling |= self.locked() && matches!(self.action, Action::Abort);
        pending.uncertain |= matches!(self.stage, Stage::Unknown(_));
    }

    pub(super) fn observed(&self, frame: &SessionFrame) -> bool {
        if frame.projection.connection != ConnectionState::Online {
            return false;
        }
        match &self.action {
            Action::Abort => {
                matches!(frame.projection.phase, RunPhase::Settled(_))
                    && matches!(
                        frame.event.as_str(),
                        "agent_settled" | "connection_online" | ""
                    )
            }
            Action::Reply { request_id, .. } => !frame
                .dialogs
                .iter()
                .any(|item| item.request_id == *request_id),
        }
    }

    pub(super) fn status(&self) -> &str {
        if self.checking {
            return "正在核对操作回执…";
        }
        match (&self.action, &self.stage) {
            (_, Stage::Rejected(reason)) => reason,
            (_, Stage::Unknown(_)) => "操作结果未知，等待节点状态或回执",
            (Action::Abort, Stage::Sending) => "正在请求停止…",
            (Action::Abort, Stage::Accepted) => "节点已受理停止，等待运行结算…",
            (Action::Reply { .. }, Stage::Sending) => "正在提交审批回复…",
            (Action::Reply { .. }, Stage::Accepted) => "节点已受理回复，等待审批关闭…",
        }
    }

    pub(super) fn can_check(&self) -> bool {
        !self.checking && matches!(self.stage, Stage::Unknown(Some(_)))
    }
}

pub(crate) fn dialog_admission(reply: &Value) -> Option<Result<(), String>> {
    reply
        .get("accepted")
        .or_else(|| reply.get("ok"))
        .and_then(Value::as_bool)
        .map(|accepted| {
            if accepted {
                Ok(())
            } else {
                Err("节点拒绝审批回复".into())
            }
        })
}

fn control_receipt(status: OperationStatus, abort: bool) -> Option<Result<(), String>> {
    if !abort
        && let OperationStatus::KnownResult {
            outcome: OperationOutcome::Succeeded { value },
        } = &status
    {
        return dialog_admission(value);
    }
    interpret_receipt(status, abort.then_some("abort"), !abort)
}

fn options(dialog: &PendingDialog) -> impl Iterator<Item = (&str, &str, Option<&str>)> {
    let ui = &dialog.ui;
    ui.options.iter().filter_map(move |option| {
        if ui.allow_multiple
            || ui.allow_freeform
            || ui.raw.get("questions").is_some()
            || !matches!(ui.kind.as_deref(), None | Some("selector" | "confirm"))
        {
            return None;
        }
        let value = option
            .value
            .as_ref()?
            .as_str()
            .filter(|value| !value.trim().is_empty())?;
        Some((
            option.title.as_deref().unwrap_or(value),
            value,
            option.description.as_deref(),
        ))
    })
}

impl Shell {
    fn action_available(&self) -> bool {
        self.session
            .and_then(|index| self.actions.get(&index))
            .is_none_or(|request| !request.locked())
    }

    fn can_abort(&self, controls: &SessionControls) -> bool {
        controls.abort
            && self.action_available()
            && self.frame.as_ref().is_some_and(|frame| {
                !matches!(
                    frame.projection.phase,
                    RunPhase::Ready | RunPhase::Settled(_)
                )
            })
    }

    fn start_control(
        &mut self,
        expected_session: Option<usize>,
        action: Action,
        cx: &mut Context<Self>,
    ) {
        if self.session != expected_session {
            return;
        }
        let (Some(node), Some(index), Some(frame)) = (&self.node, self.session, &self.frame) else {
            return;
        };
        let controls = self.session_controls();
        let permitted = match &action {
            Action::Abort => self.can_abort(&controls),
            Action::Reply { request_id, value } => {
                controls.reply
                    && self.action_available()
                    && frame.dialogs.iter().any(|dialog| {
                        dialog.request_id == *request_id
                            && options(dialog).any(|(_, option, _)| option == value)
                    })
            }
        };
        if !permitted {
            return;
        }
        self.next_action = self.next_action.wrapping_add(1);
        let ticket = self.next_action;
        let task = node.control(frame.session_id.clone(), action.clone());
        self.actions.insert(
            index,
            Request {
                ticket,
                action,
                stage: Stage::Sending,
                checking: false,
            },
        );
        cx.spawn(async move |this, cx| {
            let stage = match task.await {
                Ok(Ok(Some(Ok(())))) => Stage::Accepted,
                Ok(Ok(Some(Err(reason)))) => Stage::Rejected(reason),
                Ok(Err(ClientError::OperationOutcomeUnknown { operation })) => {
                    Stage::Unknown(Some(operation))
                }
                Ok(Err(error)) if rejected(&error) => Stage::Rejected(error.to_string()),
                _ => Stage::Unknown(None),
            };
            this.update(cx, |shell, cx| {
                if std::env::var_os("RPI_GPUI_TRACE").is_some() {
                    let status = match &stage {
                        Stage::Accepted => "accepted",
                        Stage::Rejected(_) => "rejected",
                        _ => "unknown",
                    };
                    eprintln!("session control acknowledgement: {status}");
                }
                if let Some(request) = shell
                    .actions
                    .get_mut(&index)
                    .filter(|request| request.ticket == ticket)
                {
                    request.stage = stage;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn check_control(&mut self, cx: &mut Context<Self>) {
        let (Some(node), Some(index)) = (&self.node, self.session) else {
            return;
        };
        if !self.session_controls().online {
            return;
        }
        let Some(request) = self
            .actions
            .get_mut(&index)
            .filter(|request| request.can_check())
        else {
            return;
        };
        let Stage::Unknown(Some(operation)) = &request.stage else {
            return;
        };
        let task = node.receipt(operation.clone());
        let abort = matches!(request.action, Action::Abort);
        let ticket = request.ticket;
        request.checking = true;
        cx.spawn(async move |this, cx| {
            let result = task
                .await
                .ok()
                .and_then(Result::ok)
                .and_then(|status| control_receipt(status, abort));
            this.update(cx, |shell, cx| {
                if let Some(request) = shell
                    .actions
                    .get_mut(&index)
                    .filter(|request| request.ticket == ticket)
                {
                    request.checking = false;
                    match result {
                        Some(Ok(())) => request.stage = Stage::Accepted,
                        Some(Err(reason)) => request.stage = Stage::Rejected(reason),
                        None => {}
                    }
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn run_button(
        &self,
        can_send: bool,
        controls: &SessionControls,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.palette;
        let can_abort = self.can_abort(controls);
        let enabled = can_send || can_abort;
        let stop = controls.busy;
        let session = self.session;
        div()
            .id(if stop { "stop" } else { "send" })
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(28.0))
            .rounded_full()
            .bg(if enabled && !stop {
                palette.text
            } else {
                palette.hover
            })
            .child(self.text(
                if stop { "■" } else { "↑" },
                if stop { 11.0 } else { 15.0 },
                if can_send {
                    palette.bg
                } else if can_abort {
                    palette.text
                } else {
                    palette.muted
                },
            ))
            .when(enabled, |button| {
                button
                    .cursor_pointer()
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        if stop {
                            shell.start_control(session, Action::Abort, cx);
                        } else if shell.session == session {
                            shell.send(cx);
                        }
                    }))
            })
    }

    pub(super) fn approvals(
        &self,
        controls: &SessionControls,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.palette;
        let reply = controls.reply && self.action_available();
        let session = self.session;
        div()
            .id("pending-approvals")
            .max_h(px(158.0))
            .overflow_y_scroll()
            .w_full()
            .max_w(px(READING_WIDTH))
            .children(
                self.frame
                    .iter()
                    .flat_map(|frame| &frame.dialogs)
                    .map(|dialog| {
                        let ui = &dialog.ui;
                        let title = ui
                            .question
                            .as_deref()
                            .or(ui.message.as_deref())
                            .unwrap_or("等待会话回复");
                        let mut options = options(dialog).peekable();
                        div()
                            .id(gpui::SharedString::from(format!(
                                "approval-{}",
                                dialog.request_id
                            )))
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .rounded(px(12.0))
                            .border_1()
                            .border_color(palette.strong_line)
                            .p_3()
                            .mb_2()
                            .child(self.text(title.to_owned(), 14.0, palette.text))
                            .when_some(
                                ui.context.as_ref().or_else(|| {
                                    ui.message.as_ref().filter(|text| text.as_str() != title)
                                }),
                                |card, detail| {
                                    card.child(self.text(detail.clone(), 12.0, palette.muted))
                                },
                            )
                            .when(options.peek().is_none(), |card| {
                                card.child(self.text(
                                    "此请求暂无可直接提交的单选项",
                                    12.0,
                                    palette.muted,
                                ))
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .children(options.enumerate().map(
                                        |(index, (title, value, description))| {
                                            let action = Action::Reply {
                                                request_id: dialog.request_id.clone(),
                                                value: value.into(),
                                            };
                                            div()
                                                .id(("approval-option", index))
                                                .rounded(px(8.0))
                                                .bg(palette.hover)
                                                .px_2()
                                                .py_1()
                                                .child(self.text(
                                                    title.to_owned(),
                                                    12.0,
                                                    if reply {
                                                        palette.text
                                                    } else {
                                                        palette.muted
                                                    },
                                                ))
                                                .when_some(description, |button, detail| {
                                                    button.child(self.text(
                                                        detail.to_owned(),
                                                        11.0,
                                                        palette.muted,
                                                    ))
                                                })
                                                .when(reply, |button| {
                                                    button.cursor_pointer().on_click(cx.listener(
                                                        move |shell, _, _, cx| {
                                                            shell.start_control(
                                                                session,
                                                                action.clone(),
                                                                cx,
                                                            );
                                                        },
                                                    ))
                                                })
                                        },
                                    )),
                            )
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use work_core::{RunProjection, SettlementOutcome};

    #[test]
    fn approval_uses_exact_values_and_only_supported_single_options() {
        let mut dialog: PendingDialog = serde_json::from_value(json!({
            "requestId": "id",
            "ui": {"kind": "selector", "options": [
                {"title": "拒绝", "value": "deny-exact"},
                {"title": "invalid", "value": 3},
                "display only"
            ]}
        }))
        .unwrap();
        assert_eq!(
            options(&dialog).collect::<Vec<_>>(),
            vec![("拒绝", "deny-exact", None)]
        );
        dialog.ui.allow_multiple = true;
        assert_eq!(options(&dialog).count(), 0);
        assert_eq!(dialog_admission(&json!({"accepted": "true"})), None);
        assert_eq!(dialog_admission(&json!({"ok": true})), Some(Ok(())));
    }

    #[test]
    fn acceptance_does_not_clear_the_request_before_authoritative_state() {
        let mut frame = SessionFrame {
            session_id: "session".into(),
            history: Vec::new(),
            projection: RunProjection::new("session"),
            access: None,
            dialogs: Vec::new(),
            pending_resume: false,
            event: "agent_end".into(),
        };
        let mut request = Request {
            action: Action::Abort,
            ticket: 1,
            stage: Stage::Accepted,
            checking: false,
        };
        frame.projection.phase = RunPhase::Settled(SettlementOutcome::Interrupted);
        assert!(!request.observed(&frame));
        frame.event = "agent_settled".into();
        assert!(request.observed(&frame));
        frame
            .dialogs
            .push(serde_json::from_value(json!({"requestId": "pending", "ui": {}})).unwrap());
        request.action = Action::Reply {
            request_id: "pending".into(),
            value: "deny".into(),
        };
        assert!(!request.observed(&frame));
        frame.dialogs.clear();
        frame.projection.connection = ConnectionState::Reconnecting;
        assert!(!request.observed(&frame));
        frame.projection.connection = ConnectionState::Online;
        assert!(request.observed(&frame));
    }
}
