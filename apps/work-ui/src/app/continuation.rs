//! Explicit continuation after a truthful terminal state, never a tool failure or disconnect.
use work_client::SessionSummary;
use work_core::{ConnectionState, RunPhase, RunProjection, SettlementOutcome};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ContinueAction {
    Resume,
    Prompt,
}

#[derive(Clone, Copy)]
pub(super) struct ResumeAttempt {
    pub ticket: u64,
    pub accepted: bool,
}

pub(super) fn action(
    run: Option<&RunProjection>,
    session: Option<&SessionSummary>,
    draft_empty: bool,
    attachments_pending: bool,
    resume_supported: bool,
) -> Option<ContinueAction> {
    let run = run?;
    if !draft_empty
        || attachments_pending
        || run.connection != ConnectionState::Online
        || !matches!(run.phase, RunPhase::Ready | RunPhase::Settled(_))
    {
        return None;
    }
    let pending_resume = session.is_some_and(|session| session.pending_resume);
    let failed = run.phase == RunPhase::Settled(SettlementOutcome::Failed)
        || session.is_some_and(|session| {
            session.error.is_some()
                || session.status == "failed"
                || session
                    .latest_settlement
                    .as_ref()
                    .and_then(|settlement| settlement.get("outcome"))
                    .and_then(serde_json::Value::as_str)
                    == Some("failed")
        });
    if pending_resume && resume_supported {
        Some(ContinueAction::Resume)
    } else if failed || pending_resume {
        Some(ContinueAction::Prompt)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session(pending_resume: bool) -> SessionSummary {
        serde_json::from_value(json!({"sessionId":"test", "projectId":null,"title":null,
            "status":"idle","createdAt":0,"updatedAt":0,"lastSeq":0,"pendingResume":pending_resume})).unwrap()
    }

    #[test]
    fn only_terminal_failure_or_explicit_pending_resume_offers_continue() {
        let mut run = RunProjection::new("test");
        for phase in [
            RunPhase::Ready,
            RunPhase::Text,
            RunPhase::AwaitingInput,
            RunPhase::AwaitingSettlement,
            RunPhase::Cancelling,
            RunPhase::Settled(SettlementOutcome::Completed),
            RunPhase::Settled(SettlementOutcome::Interrupted),
        ] {
            run.phase = phase;
            assert_eq!(
                action(Some(&run), Some(&session(false)), true, false, true),
                None
            );
        }
        run.phase = RunPhase::Settled(SettlementOutcome::Failed);
        assert_eq!(
            action(Some(&run), Some(&session(false)), true, false, true),
            Some(ContinueAction::Prompt)
        );
        let mut errored = session(false);
        errored.status = "idle".into();
        errored.error = Some("provider failed".into());
        run.phase = RunPhase::Ready;
        assert_eq!(
            action(Some(&run), Some(&errored), true, false, true),
            Some(ContinueAction::Prompt)
        );
        assert_eq!(
            action(Some(&run), Some(&session(true)), true, false, true),
            Some(ContinueAction::Resume)
        );
        assert_eq!(
            action(Some(&run), Some(&session(true)), true, false, false),
            Some(ContinueAction::Prompt)
        );
    }

    #[test]
    fn drafts_attachments_disconnect_and_active_runtime_never_continue() {
        let mut run = RunProjection::new("test");
        run.phase = RunPhase::Settled(SettlementOutcome::Failed);
        assert_eq!(
            action(Some(&run), Some(&session(true)), false, false, true),
            None
        );
        assert_eq!(
            action(Some(&run), Some(&session(true)), true, true, true),
            None
        );
        run.connection = ConnectionState::Reconnecting;
        assert_eq!(
            action(Some(&run), Some(&session(true)), true, false, true),
            None
        );
        run.connection = ConnectionState::Online;
        run.phase = RunPhase::Text;
        assert_eq!(
            action(Some(&run), Some(&session(true)), true, false, true),
            None
        );
        assert_eq!(action(None, None, true, false, true), None);
    }
}
