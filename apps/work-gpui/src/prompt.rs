use work_core::api::realtime::OperationRef;
use work_core::{ConnectionState, RunPhase};
use work_viewmodel::{diff_view::DraftFeedback, session_controls::PendingRequests};

use crate::shell::editor::Buffer;

const MAX_FEEDBACK: usize = 32;
const MAX_DRAFT: usize = 32_000;

pub struct Feedback {
    pub id: u64,
    pub path: String,
    pub staged: bool,
    pub index: usize,
    pub revision: String,
    pub location: String,
    pub comment: String,
    message: String,
}

#[derive(Default)]
pub struct Draft {
    pub input: Buffer,
    pub feedback: Vec<Feedback>,
    next_feedback: u64,
    request: Option<Submission>,
    pub notice: Option<String>,
}

struct Submission {
    stage: Stage,
    revision: Option<u64>,
    last_feedback: Option<u64>,
    saw_activity: bool,
    settled: bool,
}

enum Stage {
    Sending,
    Accepted,
    Unknown(Option<OperationRef>),
}

impl Draft {
    pub fn add_feedback(&mut self, item: DraftFeedback<'_>) -> bool {
        let separator = usize::from(!self.input.text.is_empty() || !self.feedback.is_empty()) * 2;
        if self.feedback.len() >= MAX_FEEDBACK
            || self.message_size() + separator + item.message.encode_utf16().count() > MAX_DRAFT
        {
            return false;
        }
        self.next_feedback += 1;
        self.feedback.push(Feedback {
            id: self.next_feedback,
            path: item.path.into(),
            staged: item.staged,
            index: item.index,
            revision: item.revision,
            location: item.location,
            comment: item.comment.into(),
            message: item.message,
        });
        true
    }

    fn message_size(&self) -> usize {
        let separators = self.feedback.len() + usize::from(!self.input.text.is_empty());
        self.input.text.encode_utf16().count()
            + self
                .feedback
                .iter()
                .map(|item| item.message.encode_utf16().count())
                .sum::<usize>()
            + separators.saturating_sub(1) * 2
    }

    pub fn feedback_over_limit(&self) -> bool {
        !self.feedback.is_empty() && self.message_size() > MAX_DRAFT
    }

    pub fn can_send(&self) -> bool {
        self.input.is_committed()
            && (self.input.can_send() || !self.feedback.is_empty())
            && !self.feedback_over_limit()
    }

    pub fn message(&self) -> String {
        std::iter::once(self.input.text.as_str())
            .chain(self.feedback.iter().map(|item| item.message.as_str()))
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub fn begin(&mut self, include_draft: bool) {
        self.notice = None;
        self.request = Some(Submission {
            stage: Stage::Sending,
            revision: include_draft.then_some(self.input.revision),
            last_feedback: self
                .feedback
                .last()
                .filter(|_| include_draft)
                .map(|item| item.id),
            saw_activity: false,
            settled: false,
        });
    }

    pub fn accept(&mut self) {
        let Some(request) = &mut self.request else {
            return;
        };
        if Some(self.input.revision) == request.revision {
            self.input.clear_after_send();
        }
        if let Some(last) = request.last_feedback {
            self.feedback.retain(|item| item.id > last);
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
        let mut draft = Draft::default();
        draft.input.replace(None, "hello");
        draft.begin(true);
        draft.accept();
        draft.observe("", &RunPhase::Accepted, &ONLINE);
        draft.observe("connection_online", &COMPLETED, &ONLINE);
        assert!(draft.pending().awaiting_reply);
        draft.observe("agent_start", &RunPhase::Accepted, &ONLINE);
        draft.observe("agent_settled", &COMPLETED, &ONLINE);
        assert!(!draft.pending().awaiting_reply);
        assert!(draft.input.text.is_empty());
    }

    #[test]
    fn acceptance_removes_only_feedback_in_the_submitted_draft() {
        use work_viewmodel::diff_view::feedback_draft;

        let patch = "--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-before\n+after\n";
        let feedback =
            |comment| feedback_draft(patch, patch, "file.rs", false, 4, comment).unwrap();
        let mut draft = Draft::default();
        assert!(draft.add_feedback(feedback("first")));
        assert!(draft.can_send());
        draft.begin(false);
        draft.accept();
        assert_eq!(draft.feedback.len(), 1);
        draft.begin(true);
        assert!(draft.add_feedback(feedback("later")));
        draft.accept();
        assert_eq!(draft.feedback.len(), 1);
        assert!(draft.message().ends_with("later"));
    }

    #[test]
    fn late_acceptance_preserves_edits_and_observed_settlement() {
        let mut draft = Draft::default();
        draft.input.replace(None, "hello");
        draft.begin(true);
        draft.input.replace(None, " again");
        draft.observe("agent_start", &RunPhase::Accepted, &ONLINE);
        draft.observe("connection_online", &COMPLETED, &ONLINE);
        draft.unknown(None);
        assert!(draft.pending().uncertain);
        draft.accept();
        assert_eq!(draft.input.text, "hello again");
        assert!(draft.request.is_none());
    }
}
