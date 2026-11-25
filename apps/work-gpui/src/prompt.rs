use work_core::api::realtime::OperationRef;
use work_core::{ConnectionState, RunPhase};
use work_viewmodel::session_controls::PendingRequests;

#[derive(Default)]
pub struct Draft {
    pub text: String,
    pub revision: u64,
    request: Option<Submission>,
    pub notice: Option<String>,
}

struct Submission {
    stage: Stage,
    revision: u64,
    saw_activity: bool,
    settled: bool,
}

enum Stage {
    Sending,
    Accepted,
    Unknown(Option<OperationRef>),
}

impl Draft {
    pub fn begin(&mut self) {
        self.notice = None;
        self.request = Some(Submission {
            stage: Stage::Sending,
            revision: self.revision,
            saw_activity: false,
            settled: false,
        });
    }

    pub fn accept(&mut self) {
        let Some(request) = &mut self.request else {
            return;
        };
        if self.revision == request.revision {
            self.text.clear();
        }
        self.notice = None;
        request.stage = Stage::Accepted;
        if request.settled {
            self.request = None;
        }
    }

    pub fn reject(&mut self, reason: String) {
        self.request = None;
        self.notice = Some(reason);
    }

    pub fn unknown(&mut self, operation: Option<OperationRef>) {
        if let Some(request) = &mut self.request {
            request.stage = Stage::Unknown(operation);
        }
        self.notice = Some("发送结果未知，草稿已保留".into());
    }

    pub fn receipt(&self) -> Option<&OperationRef> {
        match &self.request.as_ref()?.stage {
            Stage::Unknown(operation) => operation.as_ref(),
            _ => None,
        }
    }

    pub fn pending(&self) -> PendingRequests {
        let stage = self.request.as_ref().map(|request| &request.stage);
        PendingRequests {
            submitting: matches!(stage, Some(Stage::Sending)),
            awaiting_reply: matches!(stage, Some(Stage::Accepted)),
            uncertain: matches!(stage, Some(Stage::Unknown(_))),
            ..Default::default()
        }
    }

    pub fn observe(&mut self, event: &str, phase: &RunPhase, connection: &ConnectionState) {
        let Some(request) = &mut self.request else {
            return;
        };
        if *connection != ConnectionState::Online {
            return;
        }
        let terminal = matches!(phase, RunPhase::Settled(_));
        if request.saw_activity
            && terminal
            && matches!(event, "agent_settled" | "connection_online" | "")
        {
            request.settled = true;
            if matches!(request.stage, Stage::Accepted) {
                self.request = None;
            }
        } else if !matches!(phase, RunPhase::Ready | RunPhase::Settled(_))
            && !matches!(event, "" | "access" | "connection_online")
        {
            request.saw_activity = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use work_core::SettlementOutcome;

    const ONLINE: ConnectionState = ConnectionState::Online;
    const COMPLETED: RunPhase = RunPhase::Settled(SettlementOutcome::Completed);

    #[test]
    fn acceptance_requires_new_activity_and_settlement() {
        let mut draft = Draft {
            text: "hello".into(),
            ..Default::default()
        };
        draft.begin();
        draft.accept();
        draft.observe("", &RunPhase::Accepted, &ONLINE);
        draft.observe("connection_online", &COMPLETED, &ONLINE);
        assert!(draft.pending().awaiting_reply);
        draft.observe("agent_start", &RunPhase::Accepted, &ONLINE);
        draft.observe("agent_settled", &COMPLETED, &ONLINE);
        assert!(!draft.pending().awaiting_reply);
        assert!(draft.text.is_empty());
    }

    #[test]
    fn late_acceptance_preserves_edits_and_observed_settlement() {
        let mut draft = Draft {
            text: "hello".into(),
            ..Default::default()
        };
        draft.begin();
        draft.revision += 1;
        draft.observe("agent_start", &RunPhase::Accepted, &ONLINE);
        draft.observe("connection_online", &COMPLETED, &ONLINE);
        draft.unknown(None);
        assert!(draft.pending().uncertain);
        draft.accept();
        assert_eq!(draft.text, "hello");
        assert!(draft.request.is_none());
    }
}
