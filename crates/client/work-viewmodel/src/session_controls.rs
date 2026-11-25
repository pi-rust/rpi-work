//! Renderer-independent affordances for one synchronized, observed session.
//!
//! This is a pure view of the existing client projection and observations, not
//! an execution state machine or an authorization decision. In particular,
//! local request completion must not substitute for `agent_settled`.
use serde::Serialize;
use work_client::{RuntimeState, SessionAccess, SessionConfiguration};
use work_core::{ConnectionState, RunPhase, RunProjection};

use crate::config_requests::{CatalogAccess, ConfigField};

/// Client-local request bookkeeping; none of these flags declares a run phase.
#[derive(Clone, Copy, Debug, Default)]
pub struct PendingRequests {
    pub submitting: bool,
    pub awaiting_reply: bool,
    pub cancelling: bool,
    pub configuring: bool,
    pub uncertain: bool,
}

/// References must all belong to the same node/session/connection generation.
pub struct SessionObservation<'a> {
    pub projection: Option<&'a RunProjection>,
    pub access: Option<&'a SessionAccess>,
    pub configuration: Option<&'a SessionConfiguration>,
    pub runtime: Option<&'a RuntimeState>,
    pub has_dialogs: bool,
    pub pending_resume: bool,
    pub pending: PendingRequests,
}

/// Explicit UI affordances; consumers do not infer actions from protocol names.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionControls {
    pub busy: bool,
    pub online: bool,
    pub send: bool,
    pub resume_pending: Option<bool>,
    pub queue: bool,
    pub queue_read: bool,
    pub cancel_queued: bool,
    pub clear_queue: bool,
    pub abort: bool,
    pub reply: bool,
    pub model: bool,
    pub thinking: bool,
    pub configuration_locked: bool,
    pub reason: String,
}

impl Default for SessionControls {
    fn default() -> Self {
        controls(&SessionObservation {
            projection: None,
            access: None,
            configuration: None,
            runtime: None,
            has_dialogs: false,
            pending_resume: false,
            pending: PendingRequests::default(),
        })
    }
}

/// Unknown projections and absent observations fail closed. Acknowledged local
/// submissions continue to lock configuration through the pending-reply window,
/// even if the first runtime event has not arrived yet. Display preferences are
/// deliberately absent: changing a theme/layout never changes Agent admission.
#[must_use]
pub fn controls(observed: &SessionObservation<'_>) -> SessionControls {
    let pending = observed.pending;
    let online = observed
        .projection
        .is_some_and(|run| run.connection == ConnectionState::Online);
    let active = observed
        .projection
        .is_some_and(|run| !matches!(run.phase, RunPhase::Ready | RunPhase::Settled(_)));
    let cancelling = pending.cancelling
        || observed
            .projection
            .is_some_and(|run| run.phase == RunPhase::Cancelling);
    let busy = active || pending.submitting || pending.awaiting_reply || cancelling;
    let request_locked = pending.submitting || pending.configuring || pending.uncertain;
    let idle = online && !busy && !observed.has_dialogs && !request_locked;
    let command = |name| {
        observed
            .access
            .is_some_and(|access| access.supports_command(name))
    };
    let capability = |name| {
        observed
            .access
            .is_some_and(|access| access.descriptor.supports(name))
    };
    let catalog = observed
        .configuration
        .map(CatalogAccess::for_configuration)
        .unwrap_or_default();
    let reported = |field: ConfigField| {
        observed
            .runtime
            .is_some_and(|state| field.reported_by(state))
    };
    let queue = online && !request_locked && !cancelling && capability("queue");
    let reason = if observed.projection.is_none() {
        "会话尚未同步，请重新核对"
    } else if !online {
        "会话连接恢复中，当前操作已锁定"
    } else if observed.access.is_none() {
        "未取得当前会话能力，请重新核对"
    } else if pending.uncertain {
        "上次请求结果未知，不能自动重试"
    } else if pending.configuring {
        "正在提交并核对会话配置"
    } else if pending.submitting {
        "正在等待节点受理"
    } else if busy {
        "运行尚未结算"
    } else if observed.has_dialogs {
        "请先回答当前会话的待处理请求"
    } else if !capability("prompt") || !command("prompt") {
        "当前会话未声明发送能力"
    } else {
        ""
    };
    SessionControls {
        busy,
        online,
        send: idle && capability("prompt") && command("prompt"),
        resume_pending: (observed.pending_resume
            && capability("history")
            && command("resume_pending"))
        .then_some(idle),
        queue: queue && command("queue_message"),
        queue_read: online && capability("queue") && command("get_queue"),
        cancel_queued: queue && command("cancel_queued"),
        clear_queue: queue && command("clear_queue"),
        abort: online
            && busy
            && !cancelling
            && !pending.uncertain
            && capability("abort")
            && command("abort"),
        reply: online
            && observed.has_dialogs
            && !request_locked
            && observed
                .projection
                .is_some_and(|run| !matches!(run.phase, RunPhase::Settled(_)))
            && observed.access.is_some_and(|access| access.dialog_reply)
            && capability("dialogs"),
        model: idle
            && observed.access.is_some()
            && catalog.models_write
            && catalog.models_read
            && reported(ConfigField::Model),
        thinking: idle
            && observed.access.is_some()
            && catalog.thinking_write
            && catalog.thinking_read
            && reported(ConfigField::Thinking),
        configuration_locked: !idle
            || observed.access.is_none()
            || observed.configuration.is_none(),
        reason: reason.into(),
    }
}

/// Shared block serialization for fixture and canonical-client presenters.
#[must_use]
pub fn block_wire(block: &work_core::MessageBlock) -> serde_json::Value {
    use serde_json::json;
    use work_core::{MessageBlock, ToolStatus};
    match block {
        MessageBlock::Text { id, text } => json!({ "kind": "text", "id": id, "text": text }),
        MessageBlock::Thinking { id, text } => {
            json!({ "kind": "thinking", "id": id, "text": text })
        }
        MessageBlock::PublicSummary { id, text } => {
            json!({ "kind": "summary", "id": id, "text": text })
        }
        MessageBlock::ToolCall {
            id,
            name,
            arguments,
            output,
            status,
        } => json!({
            "kind": "tool", "id": id, "name": name, "arguments": arguments, "output": output,
            "status": match status {
                ToolStatus::Running => "running",
                ToolStatus::Failed => "failed",
                ToolStatus::Cancelled => "cancelled",
                ToolStatus::Completed => "completed",
            },
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use work_core::{AgentDescriptor, CapabilitySet, SettlementOutcome, TransportIdentity};

    #[test]
    fn default_controls_are_explicitly_locked() {
        let view = SessionControls::default();
        assert!(view.configuration_locked);
        assert!(
            !view.send
                && !view.queue
                && !view.abort
                && !view.reply
                && !view.model
                && !view.thinking
        );
        assert!(!view.reason.is_empty());
    }

    fn access() -> SessionAccess {
        SessionAccess {
            descriptor: AgentDescriptor {
                identity: TransportIdentity {
                    protocol: "rpi".into(),
                    version: None,
                    transport: "test".into(),
                    agent_name: None,
                },
                capabilities: CapabilitySet {
                    prompt: true,
                    abort: true,
                    dialogs: true,
                    queue: true,
                    ..CapabilitySet::default()
                },
            },
            supported_commands: [
                "prompt",
                "abort",
                "queue_message",
                "get_queue",
                "cancel_queued",
                "clear_queue",
            ]
            .map(str::to_owned)
            .into(),
            snapshot_limit: true,
            dialog_reply: true,
            prompt_content: None,
        }
    }

    fn configuration() -> SessionConfiguration {
        SessionConfiguration {
            protocol: "rpi".into(),
            supported_commands: [
                "get_available_models",
                "set_model",
                "get_available_thinking_levels",
                "set_thinking_level",
            ]
            .map(str::to_owned)
            .into(),
            config_options: vec![],
        }
    }

    fn runtime() -> RuntimeState {
        serde_json::from_value(
            serde_json::json!({"modelId":"provider/raw-id","thinkingLevel":"high"}),
        )
        .unwrap()
    }

    #[test]
    fn only_synchronized_observed_idle_session_is_writable() {
        let run = RunProjection::new("session");
        let access = access();
        let config = configuration();
        let runtime = runtime();
        let mut observed = SessionObservation {
            projection: Some(&run),
            access: Some(&access),
            configuration: Some(&config),
            runtime: Some(&runtime),
            has_dialogs: false,
            pending_resume: false,
            pending: PendingRequests::default(),
        };
        let allowed = controls(&observed);
        assert!(allowed.send && allowed.queue && allowed.model && allowed.thinking);
        observed.projection = None;
        let unknown = controls(&observed);
        assert!(!unknown.send && !unknown.queue && !unknown.model && !unknown.thinking);
        observed.projection = Some(&run);
        observed.access = None;
        let missing = controls(&observed);
        assert!(!missing.send && !missing.queue && !missing.model && !missing.reply);
    }

    #[test]
    fn every_active_phase_and_pending_reply_lock_configuration() {
        let mut run = RunProjection::new("session");
        let access = access();
        let config = configuration();
        let runtime = runtime();
        for phase in [
            RunPhase::Accepted,
            RunPhase::Preparing,
            RunPhase::Text,
            RunPhase::Tool,
            RunPhase::AwaitingInput,
            RunPhase::AwaitingSettlement,
            RunPhase::Cancelling,
        ] {
            run.phase = phase;
            let observed = SessionObservation {
                projection: Some(&run),
                access: Some(&access),
                configuration: Some(&config),
                runtime: Some(&runtime),
                has_dialogs: false,
                pending_resume: false,
                pending: PendingRequests::default(),
            };
            let view = controls(&observed);
            assert!(view.busy && view.configuration_locked);
            assert!(!view.send && !view.model && !view.thinking);
        }
        run.phase = RunPhase::Settled(SettlementOutcome::Completed);
        let observed = SessionObservation {
            projection: Some(&run),
            access: Some(&access),
            configuration: Some(&config),
            runtime: Some(&runtime),
            has_dialogs: false,
            pending_resume: false,
            pending: PendingRequests {
                awaiting_reply: true,
                ..PendingRequests::default()
            },
        };
        assert!(
            controls(&observed).configuration_locked,
            "old settlement cannot close a new accepted-request window"
        );
        let observed = SessionObservation {
            pending: PendingRequests::default(),
            ..observed
        };
        assert!(controls(&observed).model);
    }

    #[test]
    fn exact_commands_protocol_and_authoritative_field_are_independent() {
        let run = RunProjection::new("session");
        let mut access = access();
        let mut config = configuration();
        let mut runtime = runtime();
        access
            .supported_commands
            .retain(|command| command != "prompt" && command != "queue_message");
        config.supported_commands = vec!["set_model_extra".into(), "get_available_models".into()];
        let view =
            |access: &SessionAccess, config: &SessionConfiguration, runtime: &RuntimeState| {
                controls(&SessionObservation {
                    projection: Some(&run),
                    access: Some(access),
                    configuration: Some(config),
                    runtime: Some(runtime),
                    has_dialogs: false,
                    pending_resume: false,
                    pending: PendingRequests::default(),
                })
            };
        let restricted = view(&access, &config, &runtime);
        assert!(!restricted.send && !restricted.queue && !restricted.model);
        config = configuration();
        config.protocol = "acp".into();
        assert!(!view(&access, &config, &runtime).model);
        config.protocol = "rpi".into();
        runtime.model_id = None;
        assert!(!view(&access, &config, &runtime).model);
    }

    #[test]
    fn offline_unknown_write_and_historical_dialog_never_unlock() {
        let mut run = RunProjection::new("session");
        let access = access();
        for state in [
            ConnectionState::Offline,
            ConnectionState::Reconnecting,
            ConnectionState::Resyncing,
        ] {
            run.connection = state;
            let view = controls(&SessionObservation {
                projection: Some(&run),
                access: Some(&access),
                configuration: None,
                runtime: None,
                has_dialogs: true,
                pending_resume: false,
                pending: PendingRequests::default(),
            });
            assert!(!view.send && !view.abort && !view.reply && !view.queue);
        }
        run.connection = ConnectionState::Online;
        run.phase = RunPhase::Settled(SettlementOutcome::Completed);
        let view = controls(&SessionObservation {
            projection: Some(&run),
            access: Some(&access),
            configuration: None,
            runtime: None,
            has_dialogs: true,
            pending_resume: false,
            pending: PendingRequests {
                uncertain: true,
                ..PendingRequests::default()
            },
        });
        assert!(!view.send && !view.queue && !view.abort && !view.reply);
        assert!(view.configuration_locked);
    }
}
