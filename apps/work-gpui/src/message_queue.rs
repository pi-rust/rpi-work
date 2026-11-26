//! Follow-up delivery and queue controls for the bound session.

use std::collections::{HashMap, HashSet};

use gpui::{Context, Div, div, prelude::*, px};
use serde_json::{Value, json};
use tokio::task::AbortHandle;
use work_client::{ClientError, QueueSnapshot};
use work_core::api::{
    HostResponse,
    realtime::{OperationOutcome, OperationRef, OperationStatus},
};
use work_core::{ConnectionState, RunPhase};
use work_viewmodel::session_controls::{PendingRequests, SessionControls};
use work_viewmodel::session_receipts::{
    ResumeAdmission, command_admission, interpret_receipt, resume_admission,
};

use super::{Shell, rejected};
use crate::{data::SessionFrame, prompt::SubmittedDraft};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Delivery {
    #[default]
    FollowUp,
    Steer,
}

impl Delivery {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::FollowUp => "排队",
            Self::Steer => "引导本轮",
        }
    }

    pub(super) fn wire(self) -> &'static str {
        match self {
            Self::FollowUp => "followUp",
            Self::Steer => "steer",
        }
    }

    pub(super) fn from_wire(value: &str) -> Self {
        match value {
            "steer" => Self::Steer,
            _ => Self::FollowUp,
        }
    }

    fn other(self) -> Self {
        match self {
            Self::FollowUp => Self::Steer,
            Self::Steer => Self::FollowUp,
        }
    }
}

enum Action {
    Enqueue(SubmittedDraft),
    Cancel(String),
    Resume,
}

impl Action {
    fn command(&self) -> &'static str {
        match self {
            Self::Enqueue(_) => "queue_message",
            Self::Cancel(_) => "cancel_queued",
            Self::Resume => "resume_pending",
        }
    }

    fn admission(&self, reply: HostResponse<Value>) -> Option<Result<Admission, String>> {
        if matches!(self, Self::Resume) {
            return resume_admission(reply).map(|result| {
                result.map(|admission| match admission {
                    ResumeAdmission::Accepted => Admission::Running,
                    ResumeAdmission::Skipped => Admission::NothingToResume,
                })
            });
        }
        let data = reply.data.clone();
        let requires_acceptance = reply.success && matches!(self, Self::Enqueue(_));
        if let Err(error) = command_admission(reply, Some(self.command()), requires_acceptance)? {
            return Some(Err(error));
        }
        if let Self::Cancel(entry) = self {
            let data = data?;
            if data["entryId"].as_str()? != entry {
                return None;
            }
            return match data["outcome"].as_str()? {
                "cancelled" | "alreadycleared" => Some(Ok(Admission::Complete)),
                "alreadyconsumed" => Some(Err("节点已开始处理这条消息。".into())),
                _ => None,
            };
        }
        Some(Ok(Admission::Complete))
    }

    fn receipt(&self, status: OperationStatus) -> Option<Result<Admission, String>> {
        if let OperationStatus::KnownResult {
            outcome: OperationOutcome::Succeeded { value },
        } = status
        {
            return self.admission(serde_json::from_value(value).ok()?);
        }
        interpret_receipt(status, Some(self.command()), false)?
            .err()
            .map(Err)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Admission {
    Complete,
    Running,
    NothingToResume,
}

enum Stage {
    Sending,
    Unknown(Option<OperationRef>),
    Running,
}

struct Request {
    action: Action,
    stage: Stage,
    saw_activity: bool,
    settled: bool,
    checking: bool,
}

#[derive(Default)]
pub(super) struct Panel {
    modes: HashMap<usize, Delivery>,
    requests: HashMap<usize, Request>,
    notices: HashMap<usize, String>,
    skipped_resume: HashSet<usize>,
    snapshot: Option<QueueSnapshot>,
    read: Option<AbortHandle>,
    read_revision: u64,
    read_failed: bool,
}

impl Panel {
    pub(super) fn select(&mut self, index: Option<usize>, default: Delivery) {
        if let Some(index) = index {
            self.modes.entry(index).or_insert(default);
        }
        self.snapshot = None;
        self.read_failed = false;
        self.cancel_read();
    }

    fn cancel_read(&mut self) {
        self.read_revision = self.read_revision.wrapping_add(1);
        if let Some(read) = self.read.take() {
            read.abort();
        }
    }

    pub(super) fn pending(&self, index: usize, pending: &mut PendingRequests) {
        if let Some(request) = self.requests.get(&index) {
            pending.submitting |= matches!(request.stage, Stage::Sending);
            pending.uncertain |= matches!(request.stage, Stage::Unknown(_));
            pending.awaiting_reply |= matches!(request.stage, Stage::Running);
        }
    }

    pub(super) fn observe(&mut self, index: usize, frame: &SessionFrame) {
        if frame.projection.connection != ConnectionState::Online {
            self.cancel_read();
            return;
        }
        if !frame.pending_resume || frame.event == "agent_settled" {
            self.skipped_resume.remove(&index);
        }
        let Some(request) = self.requests.get_mut(&index) else {
            return;
        };
        if !matches!(request.action, Action::Resume) {
            return;
        }
        match frame.projection.phase {
            RunPhase::Settled(_)
                if request.saw_activity
                    && matches!(
                        frame.event.as_str(),
                        "agent_settled" | "connection_online" | ""
                    ) =>
            {
                request.settled = true;
                if matches!(request.stage, Stage::Running) {
                    self.requests.remove(&index);
                }
            }
            RunPhase::Ready | RunPhase::Settled(_) => {}
            _ if !matches!(frame.event.as_str(), "" | "access" | "connection_online") => {
                request.saw_activity = true;
            }
            _ => {}
        }
    }

    pub(super) fn clear_notice(&mut self, index: usize) {
        self.notices.remove(&index);
    }

    pub(super) fn notice(&self, index: usize) -> Option<&str> {
        if let Some(request) = self.requests.get(&index) {
            return Some(if request.checking {
                "正在核对队列操作回执…"
            } else {
                match request.stage {
                    Stage::Sending => "正在等待节点确认队列操作…",
                    Stage::Unknown(Some(_)) => "队列操作结果未知，请核对回执。",
                    Stage::Unknown(None) => "队列操作结果未知，尚无可核对的回执。",
                    Stage::Running => "节点已受理恢复请求，等待运行结算…",
                }
            });
        }
        self.notices.get(&index).map(String::as_str)
    }
}

impl Shell {
    pub(super) fn refresh_queue(&mut self, cx: &mut Context<Self>) {
        let (Some(node), Some(index), Some(frame)) = (&self.node, self.session, &self.frame) else {
            return;
        };
        if !self.session_controls().queue_read {
            return;
        }
        let read = node.read_queue(frame.session_id.clone());
        self.queue.cancel_read();
        let revision = self.queue.read_revision;
        self.queue.read = Some(read.abort_handle());
        cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, move |shell, cx| {
                if shell.session != Some(index) || shell.queue.read_revision != revision {
                    return;
                }
                shell.queue.read = None;
                shell.queue.read_failed = match result {
                    Ok(Ok(snapshot)) => {
                        shell.queue.snapshot = Some(snapshot);
                        false
                    }
                    _ => true,
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn queue_available(&self, controls: &SessionControls) -> bool {
        controls.queue
            && self
                .session
                .is_some_and(|index| !self.queue.requests.contains_key(&index))
    }

    pub(super) fn queue_active(&self) -> bool {
        self.frame.as_ref().is_some_and(|frame| {
            !matches!(
                frame.projection.phase,
                RunPhase::Ready | RunPhase::Settled(_) | RunPhase::Cancelling
            ) && frame.access.as_ref().is_some_and(|access| {
                access.descriptor.supports("queue") && access.supports_command("queue_message")
            })
        })
    }

    fn queue_delivery(&self, index: usize) -> Delivery {
        self.queue
            .modes
            .get(&index)
            .copied()
            .unwrap_or(self.preferences.follow_up)
    }

    pub(super) fn can_enqueue(&self, controls: &SessionControls) -> bool {
        self.queue_available(controls)
            && self.queue_active()
            && self
                .drafts
                .get(&self.session)
                .is_some_and(|draft| draft.attachments.is_empty() && draft.can_send())
    }

    pub(super) fn enqueue(&mut self, cx: &mut Context<Self>) {
        if !self.can_enqueue(&self.session_controls()) {
            return;
        }
        let Some((index, draft)) = self.session.zip(self.drafts.get(&self.session)) else {
            return;
        };
        let delivery = self.queue_delivery(index);
        let action = Action::Enqueue(draft.submitted());
        let payload =
            json!({"type":action.command(),"message":draft.message(),"deliverAs":delivery.wire()});
        self.start_queue_operation(index, action, payload, cx);
    }

    fn cancel_queued(&mut self, index: usize, id: String, cx: &mut Context<Self>) {
        if self.session != Some(index)
            || self.queue.requests.contains_key(&index)
            || !self.session_controls().cancel_queued
            || self.queue.read_failed
            || !self
                .queue
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.items.iter().any(|item| item.entry_id == id))
        {
            return;
        }
        let action = Action::Cancel(id.clone());
        let payload = json!({"type":action.command(),"entryId":id});
        self.start_queue_operation(index, action, payload, cx);
    }

    fn resume_queue(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.session != Some(index)
            || self.queue.requests.contains_key(&index)
            || self.queue.skipped_resume.contains(&index)
            || self.session_controls().resume_pending != Some(true)
        {
            return;
        }
        let action = Action::Resume;
        self.start_queue_operation(index, action, json!({"type":"resume_pending"}), cx);
    }

    fn start_queue_operation(
        &mut self,
        index: usize,
        action: Action,
        payload: Value,
        cx: &mut Context<Self>,
    ) {
        let (Some(node), Some(frame)) = (&self.node, &self.frame) else {
            return;
        };
        let task = node.queue_command(frame.session_id.clone(), payload);
        self.queue.notices.remove(&index);
        self.queue.requests.insert(
            index,
            Request {
                action,
                stage: Stage::Sending,
                saw_activity: false,
                settled: false,
                checking: false,
            },
        );
        cx.spawn(async move |this, cx| {
            let reply = task.await;
            let _ = this.update(cx, move |shell, cx| {
                let Some(request) = shell.queue.requests.get_mut(&index) else {
                    return;
                };
                let outcome = match reply {
                    Ok(Ok(reply)) => request.action.admission(reply),
                    Ok(Err(ClientError::OperationOutcomeUnknown { operation })) => {
                        request.stage = Stage::Unknown(Some(operation));
                        cx.notify();
                        return;
                    }
                    Ok(Err(error)) if rejected(&error) => Some(Err(error.to_string())),
                    _ => None,
                };
                if let Some(outcome) = outcome {
                    shell.finish_queue_operation(index, outcome, cx);
                } else {
                    request.stage = Stage::Unknown(None);
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_queue_operation(
        &mut self,
        index: usize,
        outcome: Result<Admission, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(mut request) = self.queue.requests.remove(&index) else {
            return;
        };
        match outcome {
            Ok(Admission::Complete) => {
                if let Action::Enqueue(submitted) = request.action {
                    self.drafts
                        .entry(Some(index))
                        .or_default()
                        .clear_submitted(submitted);
                    self.queue
                        .notices
                        .insert(index, "节点已受理后续消息。".into());
                }
            }
            Ok(Admission::NothingToResume) => {
                if !request.settled {
                    self.queue.skipped_resume.insert(index);
                    self.queue
                        .notices
                        .insert(index, "节点确认没有待恢复的运行。".into());
                }
            }
            Ok(Admission::Running) => {
                if !request.settled {
                    request.stage = Stage::Running;
                    self.queue.requests.insert(index, request);
                }
            }
            Err(error) => {
                self.queue.notices.insert(index, error);
            }
        }
        if self.session == Some(index) {
            self.refresh_queue(cx);
        }
        cx.notify();
    }

    fn check_queue_operation(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(node) = self
            .node
            .as_ref()
            .filter(|_| self.session == Some(index) && self.session_controls().online)
        else {
            return;
        };
        let Some(request) = self
            .queue
            .requests
            .get_mut(&index)
            .filter(|request| !request.checking)
        else {
            return;
        };
        let Stage::Unknown(Some(operation)) = &request.stage else {
            return;
        };
        let task = node.receipt(operation.clone());
        request.checking = true;
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, move |shell, cx| {
                let Some(request) = shell.queue.requests.get_mut(&index) else {
                    return;
                };
                request.checking = false;
                if let Some(outcome) = result
                    .ok()
                    .and_then(Result::ok)
                    .and_then(|status| request.action.receipt(status))
                {
                    shell.finish_queue_operation(index, outcome, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn queue_toolbar(&self, controls: &SessionControls, cx: &mut Context<Self>) -> Div {
        let mut toolbar = div().flex().items_center().gap_1();
        let Some(index) = self.session else {
            return toolbar;
        };
        let palette = self.palette;
        if self.queue_active() {
            let delivery = self.queue_delivery(index);
            toolbar = toolbar.child(
                self.button("queue-mode", delivery.label())
                    .child(crate::icons::icon("chevron-down", 10.0, palette.muted))
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        if shell.session == Some(index) {
                            shell.queue.modes.insert(index, delivery.other());
                            cx.notify();
                        }
                    })),
            );
            if self.can_enqueue(controls) {
                toolbar = toolbar.child(
                    self.button("queue-submit", "提交")
                        .bg(palette.subtle)
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            if shell.session == Some(index) {
                                shell.enqueue(cx);
                            }
                        })),
                );
            }
        }
        if controls.online
            && self.queue.requests.get(&index).is_some_and(|request| {
                !request.checking && matches!(request.stage, Stage::Unknown(Some(_)))
            })
        {
            toolbar = toolbar.child(self.button("queue-receipt", "核对队列回执").on_click(
                cx.listener(move |shell, _, _, cx| shell.check_queue_operation(index, cx)),
            ));
        }
        toolbar
    }

    pub(super) fn queue_panel(&self, controls: &SessionControls, cx: &mut Context<Self>) -> Div {
        let panel = div().flex().flex_col();
        let Some(index) = self.session else {
            return panel;
        };
        let resume = controls
            .resume_pending
            .filter(|_| !self.queue.skipped_resume.contains(&index));
        let snapshot = self.queue.snapshot.as_ref();
        if snapshot.is_none_or(|snapshot| snapshot.items.is_empty())
            && resume.is_none()
            && !self.queue.read_failed
        {
            return panel;
        }
        let palette = self.palette;
        let cancel = controls.cancel_queued
            && !self.queue.requests.contains_key(&index)
            && !self.queue.read_failed;
        let mut header = div().flex().items_center().gap_2().child(
            self.text(
                format!(
                    "后续消息 · {}",
                    snapshot.map_or(0, |snapshot| snapshot.count)
                ),
                12.0,
                palette.text,
            )
            .flex_1(),
        );
        if let Some(enabled) = resume {
            header = header.child(self.button("queue-resume", "继续队列").when(
                enabled,
                |button| {
                    button
                        .on_click(cx.listener(move |shell, _, _, cx| shell.resume_queue(index, cx)))
                },
            ));
        }
        if self.queue.read_failed {
            header = header
                .child(self.text("队列读取失败", 11.0, palette.muted))
                .when(controls.queue_read && self.queue.read.is_none(), |bar| {
                    bar.child(
                        self.button("queue-refresh", "重新读取")
                            .on_click(cx.listener(|shell, _, _, cx| shell.refresh_queue(cx))),
                    )
                });
        }
        panel
            .bg(palette.side)
            .border_1()
            .border_color(palette.line)
            .rounded(px(10.0))
            .px_2()
            .mb_2()
            .child(header)
            .child(
                div()
                    .id("queue-messages")
                    .overflow_y_scroll()
                    .max_h(px(132.0))
                    .children(
                        snapshot
                            .into_iter()
                            .flat_map(|snapshot| &snapshot.items)
                            .map(|item| {
                                let id = item.entry_id.clone();
                                let label = match item.queue.as_str() {
                                    "steer" => "引导本轮",
                                    "nextRun" | "nextTurn" => "下一轮",
                                    "followUp" | "follow_up" => "排队",
                                    _ => "待交付",
                                };
                                let text = if item.text.is_empty() {
                                    "非文本消息".into()
                                } else {
                                    item.text.clone()
                                };
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .border_t_1()
                                    .border_color(palette.line)
                                    .min_h(px(30.0))
                                    .child(self.text(label, 11.0, palette.muted).flex_none())
                                    .child(
                                        self.text(text, 12.0, palette.text)
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .truncate(),
                                    )
                                    .when(cancel, |row| {
                                        row.child(
                                            self.button(
                                                gpui::SharedString::from(format!(
                                                    "queue-remove-{id}"
                                                )),
                                                "移除",
                                            )
                                            .on_click(
                                                cx.listener(move |shell, _, _, cx| {
                                                    shell.cancel_queued(index, id.clone(), cx)
                                                }),
                                            ),
                                        )
                                    })
                            }),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::Draft;
    use work_core::{RunProjection, SettlementOutcome};

    fn response(command: &str, data: Value) -> HostResponse<Value> {
        serde_json::from_value(json!({
            "type": "response",
            "success": true,
            "command": command,
            "data": data,
        }))
        .unwrap()
    }

    #[test]
    fn queue_acknowledgements_match_command_and_entry() {
        let enqueue = Action::Enqueue(Draft::default().submitted());
        for data in [json!({}), json!({"accepted": "true"})] {
            assert_eq!(enqueue.admission(response("queue_message", data)), None);
        }
        assert_eq!(
            enqueue.admission(response("prompt", json!({"accepted": true}))),
            None
        );
        let accepted = response("queue_message", json!({"accepted": true}));
        assert_eq!(
            enqueue.admission(accepted.clone()),
            Some(Ok(Admission::Complete))
        );
        assert_eq!(
            enqueue.receipt(OperationStatus::KnownResult {
                outcome: OperationOutcome::Succeeded {
                    value: json!(accepted)
                },
            }),
            Some(Ok(Admission::Complete))
        );
        let cancel = Action::Cancel("entry-a".into());
        assert_eq!(
            cancel.admission(response(
                "cancel_queued",
                json!({"entryId": "entry-b", "outcome": "cancelled"})
            )),
            None
        );
        assert!(matches!(
            cancel.admission(response(
                "cancel_queued",
                json!({"entryId": "entry-a", "outcome": "alreadyconsumed"})
            )),
            Some(Err(_))
        ));
        assert_eq!(
            Action::Resume.admission(response("resume_pending", json!({"resumed": false}))),
            Some(Ok(Admission::NothingToResume))
        );
    }

    #[test]
    fn resume_ignores_old_settlements_and_records_settlement_before_ack() {
        let mut panel = Panel::default();
        panel.requests.insert(
            0,
            Request {
                action: Action::Resume,
                stage: Stage::Sending,
                saw_activity: false,
                settled: false,
                checking: false,
            },
        );
        let mut frame = SessionFrame {
            session_id: "session".into(),
            history: Vec::new(),
            projection: RunProjection::new("session"),
            access: None,
            dialogs: Vec::new(),
            pending_resume: true,
            event: "connection_online".into(),
        };
        frame.projection.phase = RunPhase::Settled(SettlementOutcome::Interrupted);
        panel.observe(0, &frame);
        assert!(!panel.requests[&0].settled);
        frame.event = "agent_start".into();
        frame.projection.phase = RunPhase::Accepted;
        panel.observe(0, &frame);
        frame.event = "agent_settled".into();
        frame.projection.phase = RunPhase::Settled(SettlementOutcome::Completed);
        panel.observe(0, &frame);
        assert!(panel.requests[&0].settled);
    }
}
