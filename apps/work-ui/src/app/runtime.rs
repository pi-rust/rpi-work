use crate::features::resources::library as resources;
use serde_json::{Value, json};
use work_client::{Project, SessionSummary};
use work_core::{ConnectionState, RunPhase, RunProjection, SettlementOutcome};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InteractionState {
    Disconnected,
    Ready,
    Sending,
    Running,
    AwaitingInput,
    Cancelling,
}

impl InteractionState {
    pub(crate) fn is_active(self) -> bool {
        !matches!(self, Self::Disconnected | Self::Ready)
    }

    pub(crate) fn can_queue(self) -> bool {
        matches!(self, Self::Sending | Self::Running | Self::AwaitingInput)
    }

    pub(crate) fn activity_label(self, awaiting_reply: bool) -> Option<&'static str> {
        match self {
            Self::Sending => Some("正在发送…"),
            Self::Running if awaiting_reply => Some("等待回复…"),
            Self::Running => Some("正在处理…"),
            Self::AwaitingInput => Some("等待确认…"),
            Self::Cancelling => Some("正在停止…"),
            Self::Disconnected | Self::Ready => None,
        }
    }

    pub(crate) fn sidebar_activity(
        self,
        awaiting_reply: bool,
    ) -> crate::features::projects::management::SidebarActivity {
        use crate::features::projects::management::SidebarActivity;
        match self {
            Self::Disconnected => SidebarActivity::Disconnected,
            Self::Ready => SidebarActivity::Idle,
            Self::Sending => SidebarActivity::Sending,
            Self::Running if awaiting_reply => SidebarActivity::WaitingReply,
            Self::Running => SidebarActivity::Running,
            Self::AwaitingInput => SidebarActivity::AwaitingInput,
            Self::Cancelling => SidebarActivity::Cancelling,
        }
    }

    pub(crate) fn primary_action_is_stop(self) -> bool {
        matches!(self, Self::Running | Self::AwaitingInput | Self::Cancelling)
    }

    pub(crate) fn composer_loading(self, draft_empty: bool, submitting_original: bool) -> bool {
        self.is_active() && (draft_empty || submitting_original)
    }
}

/// Small, node-epoch-scoped evidence retained when a conversation is not selected.
/// Unlike a catalog error or latestSettlement, this phase belongs to this cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SessionObservation {
    pub cursor: Option<u64>,
    pub phase: RunPhase,
}

impl From<&RunProjection> for SessionObservation {
    fn from(run: &RunProjection) -> Self {
        Self {
            cursor: run.cursor,
            phase: run.phase.clone(),
        }
    }
}

/// A response can settle while this UI is watching another session. Retire only
/// the list's waiting badge, not the optimistic message or the user's draft.
pub(crate) fn switcher_waiting_reply(
    session: &SessionSummary,
    pending: &crate::features::conversation::activity::PendingReply,
    observed: Option<&SessionObservation>,
) -> bool {
    if pending.responded {
        return false;
    }
    let Some(before) = pending.cursor_before.filter(|_| pending.accepted) else {
        return true;
    };
    if observed.is_some_and(|observed| {
        matches!(observed.phase, RunPhase::Settled(_))
            && observed
                .cursor
                .is_some_and(|cursor| cursor > before && cursor >= session.last_seq)
    }) {
        return false;
    }
    let catalog_settled = matches!(
        session.status.as_str(),
        "completed"
            | "done"
            | "succeeded"
            | "failed"
            | "error"
            | "aborted"
            | "interrupted"
            | "cancelled"
    ) || (matches!(session.status.as_str(), "idle" | "ready")
        && session
            .latest_settlement
            .as_ref()
            .is_some_and(|value| value.get("outcome").is_some()));
    !(session.last_seq > before && catalog_settled)
}

pub(crate) fn switcher_session_state(
    session: &SessionSummary,
    activity: Option<crate::features::projects::management::SidebarActivity>,
    observed: Option<&SessionObservation>,
) -> (&'static str, &'static str) {
    use crate::features::projects::management::{SidebarActivity, switcher_session_status};
    // Local requests and current transport failures have no server cursor yet.
    if matches!(
        activity,
        Some(
            SidebarActivity::Loading
                | SidebarActivity::Sending
                | SidebarActivity::WaitingReply
                | SidebarActivity::Disconnected
        )
    ) {
        return switcher_session_status(session, activity);
    }
    let Some(observed) = observed else {
        return switcher_session_status(session, activity);
    };
    if !observed
        .cursor
        .is_some_and(|cursor| cursor >= session.last_seq)
    {
        // A newer server summary outranks an older stream/snapshot observation.
        return switcher_session_status(session, None);
    }
    match &observed.phase {
        RunPhase::Ready => switcher_session_status(session, activity),
        RunPhase::Accepted
        | RunPhase::Preparing
        | RunPhase::Text
        | RunPhase::Tool
        | RunPhase::AwaitingSettlement => {
            if activity == Some(SidebarActivity::Cancelling) {
                ("running", "停止中")
            } else {
                ("running", "运行中")
            }
        }
        RunPhase::AwaitingInput => ("waiting", "等待回答"),
        RunPhase::Cancelling => ("running", "停止中"),
        RunPhase::Settled(outcome) => match outcome {
            SettlementOutcome::Completed => ("completed", "完成"),
            SettlementOutcome::Failed => ("failed", "异常"),
            SettlementOutcome::Interrupted | SettlementOutcome::Declined => ("idle", "已停止"),
            SettlementOutcome::Suspended => ("waiting", "待恢复"),
            SettlementOutcome::Compacted => ("idle", "空闲"),
            SettlementOutcome::Unknown(_) => ("unknown", "状态未知"),
        },
    }
}

/// Retain the last observed protocol phase when the reader switches conversations.
/// This is not a background subscription or proof of an unobserved settlement.
pub(crate) fn sync_sidebar_session(session: &mut SessionSummary, run: &RunProjection) {
    session.status = match (&run.connection, &run.phase) {
        (ConnectionState::Offline, _) => "offline",
        (ConnectionState::Reconnecting | ConnectionState::Resyncing, _) => "loading",
        (_, RunPhase::Settled(SettlementOutcome::Failed)) => "failed",
        (_, RunPhase::Ready | RunPhase::Settled(_)) => "idle",
        (_, RunPhase::AwaitingInput) => "waiting_for_input",
        (_, RunPhase::Cancelling) => "cancelling",
        _ => "running",
    }
    .into();
}

pub(crate) fn interaction_state(
    projection: Option<&RunProjection>,
    transport_online: bool,
    submitting: bool,
    awaiting_reply: bool,
) -> InteractionState {
    if !transport_online {
        return InteractionState::Disconnected;
    }
    // A real protocol phase always wins over local request bookkeeping. In
    // particular, cancelling must not be presented as running while the
    // pre-token stop request is still waiting for settlement.
    match projection.map(|run| &run.phase) {
        Some(RunPhase::Cancelling) => return InteractionState::Cancelling,
        Some(RunPhase::AwaitingInput) => return InteractionState::AwaitingInput,
        Some(
            RunPhase::Accepted
            | RunPhase::Preparing
            | RunPhase::Text
            | RunPhase::Tool
            | RunPhase::AwaitingSettlement,
        ) => return InteractionState::Running,
        _ => {}
    }
    if submitting {
        return InteractionState::Sending;
    }
    if awaiting_reply {
        return InteractionState::Running;
    }
    InteractionState::Ready
}

/// Fail closed until the selected session is synchronized, online and idle.
/// Local submission acknowledgement is not proof of runtime settlement;
/// `interaction` must include the pending-reply window. UI preferences stay independent.
pub(crate) fn extension_config_locked(
    projection: Option<&RunProjection>,
    interaction: InteractionState,
    has_pending_dialogs: bool,
) -> bool {
    has_pending_dialogs
        || interaction != InteractionState::Ready
        || projection.is_none_or(|run| {
            run.connection != ConnectionState::Online
                || !matches!(run.phase, RunPhase::Ready | RunPhase::Settled(_))
        })
}

pub(crate) fn phase_tone(phase: &RunPhase) -> &'static str {
    match phase {
        RunPhase::Ready => "ready",
        RunPhase::Accepted
        | RunPhase::Preparing
        | RunPhase::Text
        | RunPhase::Tool
        | RunPhase::AwaitingSettlement => "running",
        RunPhase::AwaitingInput => "awaiting-input",
        RunPhase::Cancelling => "cancelling",
        RunPhase::Settled(outcome) => match outcome {
            SettlementOutcome::Completed => "completed",
            SettlementOutcome::Failed => "failed",
            SettlementOutcome::Interrupted | SettlementOutcome::Declined => "interrupted",
            SettlementOutcome::Suspended | SettlementOutcome::Compacted => "ready",
            SettlementOutcome::Unknown(_) => "unknown",
        },
    }
}

pub(crate) fn transport_online(
    projection: Option<&RunProjection>,
    connection: &super::connections::Phase,
) -> bool {
    matches!(connection, super::connections::Phase::Online)
        && projection.is_none_or(|run| run.connection == ConnectionState::Online)
}

pub(crate) fn task_status(
    projection: Option<&RunProjection>,
    connection: &super::connections::Phase,
    local: bool,
) -> (&'static str, &'static str) {
    let _ = local;
    let connection_online = matches!(connection, super::connections::Phase::Online);
    if !connection_online || projection.is_some_and(|run| run.connection != ConnectionState::Online)
    {
        return ("unknown", "任务状态未知");
    }
    projection.map_or(("ready", "就绪"), |run| {
        (phase_tone(&run.phase), phase_label(&run.phase))
    })
}

pub(crate) fn connection_status(
    projection: Option<&RunProjection>,
    connection: &super::connections::Phase,
    local: bool,
) -> (&'static str, &'static str) {
    use super::connections::Phase;
    if let Some(run) = projection {
        return match run.connection {
            ConnectionState::Online => ("online", "节点在线"),
            ConnectionState::Reconnecting => ("reconnecting", "节点重连中"),
            ConnectionState::Resyncing => ("resyncing", "节点同步中"),
            ConnectionState::Offline => ("offline", "节点已断开"),
        };
    }
    match connection {
        Phase::Online => (
            "online",
            if local {
                "本机节点在线"
            } else {
                "节点在线"
            },
        ),
        Phase::Connecting | Phase::Authenticating => ("reconnecting", "节点连接中"),
        Phase::PasswordRequired => ("offline", "节点需要登录"),
        Phase::Failed(_) => ("offline", "节点连接失败"),
        Phase::Idle => ("offline", "节点未连接"),
    }
}

pub(crate) fn projection_status(
    projection: Option<&RunProjection>,
    connection: &super::connections::Phase,
    local: bool,
) -> &'static str {
    if let Some(projection) = projection {
        phase_label(&projection.phase)
    } else {
        use super::connections::Phase;
        match (local, connection) {
            (true, Phase::Online) => "本机就绪",
            (true, Phase::Failed(_)) => "本机不可用",
            (true, _) => "准备本机环境",
            (false, Phase::Online) => "在线",
            (false, Phase::Connecting | Phase::Authenticating) => "连接中",
            (false, Phase::PasswordRequired) => "需要登录",
            (false, Phase::Failed(_)) => "连接错误",
            (false, Phase::Idle) => "未连接",
        }
    }
}

pub(crate) fn resource_snapshot(
    binding: &resources::SessionBinding,
    library: &resources::ResourceLibrary,
) -> Value {
    let packages = binding.package_pins.iter().map(|pin| {
        library.packages.iter().find(|item| item.id == pin.id && item.version == pin.version).map_or_else(
            || json!({"id":pin.id,"version":pin.version,"unresolved":true}),
            |item| json!({"id":item.id,"version":item.version,"manifest":item.manifest,"skills":item.skill_contents.iter().filter(|skill| !binding.disabled_skills.contains(&format!("{}@{}/{}",item.id,item.version,skill.id))).map(|skill| json!({"id":format!("{}@{}/{}",item.id,item.version,skill.id),"name":skill.name,"description":skill.description,"content":skill.content})).collect::<Vec<_>>(),"tools":item.tools,"prompts":item.prompts.iter().map(|prompt| json!({"id":format!("{}@{}/{}",item.id,item.version,prompt.id),"name":prompt.name,"kind":prompt.kind,"content":prompt.content})).collect::<Vec<_>>()})
        )
    }).collect::<Vec<_>>();
    let skills = resources::effective_skills(library, binding)
        .iter()
        .map(|skill| json!({"id":skill.id,"revision":skill.content.len(),"content":skill.content}))
        .collect::<Vec<_>>();
    // These are desired client trace metadata, not evidence of node loading.
    let extensions = binding.extension_pins.iter().map(|pin| {
        let manifest = library.extensions.iter().filter_map(|extension| extension.manifest.as_ref())
            .find(|manifest| manifest.pin() == *pin && manifest.agent.is_some());
        json!({"id":pin.id,"version":pin.version,"agent":manifest.and_then(|m| m.agent.as_ref()),"unresolved":manifest.is_none()})
    }).collect::<Vec<_>>();
    json!({
        "desiredRevision":binding.revision,
        "extensions":{"status":"desired-only","nodeApplied":null,"pins":extensions},
        "packages":packages,
        "skills":skills,
        "disabledSkills":binding.disabled_skills,
        "disabledTools":binding.disabled_tools,
        "disabledPrompts":binding.disabled_prompts,
        "instructions":binding.instructions
    })
}

pub(crate) fn default_node_url() -> String {
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    {
        // Web discovers its serving node; Android asks for the computer's
        // address. Phone loopback would point back to the phone itself.
        "about:blank".to_owned()
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        // Bootstrap validates explicit CLI/env selection before touching a lock.
        // The same selection initializes both the client and the startup effect.
        super::local_node::requested_address()
            .or_else(super::local_node::address)
            .unwrap_or("about:blank")
            .to_owned()
    }
}

pub(crate) fn conversation_title(message: &str) -> String {
    let line = message
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("新会话");
    let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title = line.chars().take(32).collect::<String>();
    if line.chars().count() > 32 {
        title.push('…');
    }
    title
}

#[allow(dead_code)]
pub(crate) fn phase_label(phase: &RunPhase) -> &'static str {
    match phase {
        RunPhase::Ready => "就绪",
        RunPhase::Accepted => "已接收",
        RunPhase::Preparing => "准备中",
        RunPhase::Text => "生成文本",
        RunPhase::Tool => "执行工具",
        RunPhase::AwaitingInput => "等待确认",
        RunPhase::AwaitingSettlement => "等待结算",
        RunPhase::Cancelling => "停止中",
        RunPhase::Settled(outcome) => match outcome {
            SettlementOutcome::Completed => "已完成",
            SettlementOutcome::Failed => "执行失败",
            SettlementOutcome::Interrupted => "已中断",
            SettlementOutcome::Suspended => "已挂起",
            SettlementOutcome::Compacted => "已压缩",
            SettlementOutcome::Declined => "未执行",
            SettlementOutcome::Unknown(_) => "已结算（未知结果）",
        },
    }
}

#[allow(dead_code)]
pub(crate) fn session_matches(session: &SessionSummary, query: &str, projects: &[Project]) -> bool {
    let project = projects
        .iter()
        .find(|p| session.project_id.as_deref() == Some(p.id.as_str()));
    format!(
        "{} {} {}",
        session.title.as_deref().unwrap_or("未命名会话"),
        session.status,
        project
            .map(|p| format!("{} {}", p.name, p.path))
            .unwrap_or_default()
    )
    .to_lowercase()
    .contains(&query.trim().to_lowercase())
}

#[cfg(test)]
mod status_tests {
    use super::{super::connections::Phase, interaction_state, projection_status, task_status};
    use work_core::{ConnectionState, RunPhase, RunProjection, SettlementOutcome};

    #[test]
    fn sidebar_activity_follows_protocol_and_pending_reply_without_a_draft_dependency() {
        use super::InteractionState;
        use crate::features::projects::management::SidebarActivity;
        for (state, pending, expected) in [
            (InteractionState::Sending, true, SidebarActivity::Sending),
            (
                InteractionState::Running,
                true,
                SidebarActivity::WaitingReply,
            ),
            (InteractionState::Running, false, SidebarActivity::Running),
            (
                InteractionState::AwaitingInput,
                true,
                SidebarActivity::AwaitingInput,
            ),
            (
                InteractionState::Cancelling,
                true,
                SidebarActivity::Cancelling,
            ),
            (
                InteractionState::Disconnected,
                true,
                SidebarActivity::Disconnected,
            ),
            (InteractionState::Ready, false, SidebarActivity::Idle),
        ] {
            assert_eq!(state.sidebar_activity(pending), expected);
        }
    }

    #[test]
    fn switcher_keeps_cursor_bound_outcomes_even_after_switching_away() {
        use crate::features::projects::management::SidebarActivity;
        let session: work_client::SessionSummary = serde_json::from_value(serde_json::json!({
            "sessionId":"observed", "status":"failed", "error":"previous run failed",
            "pendingResume":true, "latestSettlement":{"outcome":"completed"},
            "createdAt":0, "updatedAt":0, "lastSeq":10
        }))
        .unwrap();
        for (phase, expected) in [
            (RunPhase::Accepted, ("running", "运行中")),
            (RunPhase::Tool, ("running", "运行中")),
            (RunPhase::AwaitingInput, ("waiting", "等待回答")),
            (RunPhase::Cancelling, ("running", "停止中")),
            (
                RunPhase::Settled(SettlementOutcome::Completed),
                ("completed", "完成"),
            ),
            (
                RunPhase::Settled(SettlementOutcome::Failed),
                ("failed", "异常"),
            ),
            (
                RunPhase::Settled(SettlementOutcome::Interrupted),
                ("idle", "已停止"),
            ),
            (
                RunPhase::Settled(SettlementOutcome::Declined),
                ("idle", "已停止"),
            ),
            (
                RunPhase::Settled(SettlementOutcome::Suspended),
                ("waiting", "待恢复"),
            ),
            (
                RunPhase::Settled(SettlementOutcome::Compacted),
                ("idle", "空闲"),
            ),
            (
                RunPhase::Settled(SettlementOutcome::Unknown("new-outcome".into())),
                ("unknown", "状态未知"),
            ),
        ] {
            let observed = super::SessionObservation {
                cursor: Some(10),
                phase,
            };
            for activity in [None, Some(SidebarActivity::Idle)] {
                assert_eq!(
                    super::switcher_session_state(&session, activity, Some(&observed)),
                    expected
                );
            }
        }
    }

    #[test]
    fn switcher_newer_catalog_outranks_old_observations_and_does_not_invent_completion() {
        use crate::features::projects::management::SidebarActivity;
        let mut session: work_client::SessionSummary = serde_json::from_value(serde_json::json!({
            "sessionId":"observed", "status":"running", "createdAt":0, "updatedAt":0, "lastSeq":12
        }))
        .unwrap();
        let mut observed = super::SessionObservation {
            cursor: Some(11),
            phase: RunPhase::Settled(SettlementOutcome::Completed),
        };
        assert_eq!(
            super::switcher_session_state(&session, None, Some(&observed)),
            ("running", "运行中")
        );
        session.status = "failed".into();
        observed.phase = RunPhase::Tool;
        assert_eq!(
            super::switcher_session_state(
                &session,
                Some(SidebarActivity::Running),
                Some(&observed)
            ),
            ("failed", "异常")
        );
        for status in ["idle", "unknown"] {
            session.status = status.into();
            let expected = if status == "idle" {
                ("idle", "空闲")
            } else {
                ("unknown", "状态未知")
            };
            assert_eq!(
                super::switcher_session_state(&session, None, None),
                expected
            );
            assert_eq!(
                super::switcher_session_state(&session, None, Some(&observed)),
                expected
            );
        }
        observed.cursor = None;
        session.last_seq = 0;
        observed.phase = RunPhase::Settled(SettlementOutcome::Completed);
        assert_eq!(
            super::switcher_session_state(&session, None, Some(&observed)),
            ("unknown", "状态未知")
        );
    }

    #[test]
    fn switcher_only_retires_waiting_after_a_newer_settlement() {
        let mut session: work_client::SessionSummary = serde_json::from_value(serde_json::json!({
            "sessionId":"observed", "status":"idle", "createdAt":0, "updatedAt":0,
            "lastSeq":10, "latestSettlement":{"outcome":"completed"}
        }))
        .unwrap();
        let mut pending = crate::features::conversation::activity::PendingReply {
            text: "new send".into(),
            draft_text: None,
            content: None,
            users_before: 1,
            user_boundary: None,
            cursor_before: Some(10),
            accepted: true,
            responded: false,
        };
        assert!(super::switcher_waiting_reply(&session, &pending, None));
        let mut observed = super::SessionObservation {
            cursor: Some(10),
            phase: RunPhase::Settled(SettlementOutcome::Completed),
        };
        assert!(super::switcher_waiting_reply(
            &session,
            &pending,
            Some(&observed)
        ));
        session.last_seq = 12;
        assert!(!super::switcher_waiting_reply(&session, &pending, None));
        session.status = "running".into();
        assert!(super::switcher_waiting_reply(
            &session,
            &pending,
            Some(&observed)
        ));
        observed.cursor = Some(12);
        assert!(!super::switcher_waiting_reply(
            &session,
            &pending,
            Some(&observed)
        ));
        observed.phase = RunPhase::Text;
        assert!(super::switcher_waiting_reply(
            &session,
            &pending,
            Some(&observed)
        ));
        pending.accepted = false;
        session.status = "completed".into();
        assert!(super::switcher_waiting_reply(&session, &pending, None));
        pending.accepted = true;
        pending.cursor_before = None;
        assert!(super::switcher_waiting_reply(&session, &pending, None));
    }

    #[test]
    fn switcher_local_submission_and_transport_feedback_remain_visible() {
        use crate::features::projects::management::SidebarActivity;
        let session: work_client::SessionSummary = serde_json::from_value(serde_json::json!({
            "sessionId":"observed", "status":"idle", "createdAt":0, "updatedAt":0, "lastSeq":10
        }))
        .unwrap();
        let observed = super::SessionObservation {
            cursor: Some(10),
            phase: RunPhase::Settled(SettlementOutcome::Completed),
        };
        for (activity, expected) in [
            (SidebarActivity::Loading, ("loading", "加载中")),
            (SidebarActivity::Sending, ("running", "发送中")),
            (SidebarActivity::WaitingReply, ("waiting", "等待回答")),
            (SidebarActivity::Disconnected, ("failed", "连接异常")),
        ] {
            assert_eq!(
                super::switcher_session_state(&session, Some(activity), Some(&observed)),
                expected
            );
        }
    }

    #[test]
    fn sidebar_summary_retains_only_observed_phase_and_never_invents_a_settlement() {
        let mut session: work_client::SessionSummary = serde_json::from_value(serde_json::json!({
            "sessionId":"sidebar", "status":"idle", "createdAt":0, "updatedAt":0, "lastSeq":4
        }))
        .unwrap();
        let mut run = RunProjection::new("sidebar");
        for (phase, expected) in [
            (RunPhase::Accepted, "running"),
            (RunPhase::Text, "running"),
            (RunPhase::Tool, "running"),
            (RunPhase::AwaitingInput, "waiting_for_input"),
            (RunPhase::Cancelling, "cancelling"),
            (RunPhase::AwaitingSettlement, "running"),
            (RunPhase::Settled(SettlementOutcome::Completed), "idle"),
            (RunPhase::Settled(SettlementOutcome::Failed), "failed"),
            (RunPhase::Settled(SettlementOutcome::Interrupted), "idle"),
        ] {
            run.phase = phase;
            super::sync_sidebar_session(&mut session, &run);
            assert_eq!(session.status, expected);
            assert!(session.latest_settlement.is_none());
            assert_eq!(session.last_seq, 4);
        }
        run.connection = ConnectionState::Reconnecting;
        super::sync_sidebar_session(&mut session, &run);
        assert_eq!(session.status, "loading");
        run.connection = ConnectionState::Offline;
        super::sync_sidebar_session(&mut session, &run);
        assert_eq!(session.status, "offline");
    }

    #[test]
    fn extension_config_stays_locked_until_the_session_has_a_synchronized_projection() {
        let interaction = interaction_state(None, true, false, false);
        assert!(super::extension_config_locked(None, interaction, false));
    }

    #[test]
    fn extension_config_stays_locked_after_prompt_ack_before_runtime_events() {
        let ready = RunProjection::new("previous-run");
        // HTTP/RPC submission has finished, but no run event has arrived yet.
        let interaction = interaction_state(Some(&ready), true, false, true);
        assert!(super::extension_config_locked(
            Some(&ready),
            interaction,
            false
        ));
    }

    #[test]
    fn extension_config_requires_confirmed_online_idle_state() {
        let mut run = RunProjection::new("run");
        assert!(!super::extension_config_locked(
            Some(&run),
            super::InteractionState::Ready,
            false
        ));
        assert!(super::extension_config_locked(
            Some(&run),
            super::InteractionState::Ready,
            true
        ));
        assert!(super::extension_config_locked(
            Some(&run),
            super::InteractionState::Disconnected,
            false
        ));
        for connection in [
            ConnectionState::Reconnecting,
            ConnectionState::Resyncing,
            ConnectionState::Offline,
        ] {
            run.connection = connection;
            assert!(super::extension_config_locked(
                Some(&run),
                super::InteractionState::Ready,
                false
            ));
        }
        run.connection = ConnectionState::Online;
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
            assert!(super::extension_config_locked(
                Some(&run),
                super::InteractionState::Ready,
                false
            ));
        }
        for outcome in [
            SettlementOutcome::Completed,
            SettlementOutcome::Failed,
            SettlementOutcome::Interrupted,
        ] {
            run.phase = RunPhase::Settled(outcome);
            assert!(!super::extension_config_locked(
                Some(&run),
                super::InteractionState::Ready,
                false
            ));
        }
    }

    #[test]
    fn extension_snapshots_preserve_desired_versions_without_ui_or_applied_claims() {
        use crate::features::resources::library as resources;
        use work_core::extensions::{ExtensionManifest, ExtensionPin, UiExtensionPreference};

        let manifest: ExtensionManifest = serde_json::from_value(serde_json::json!({
            "schemaVersion":1,"kind":"rpi-work-extension-design","id":"team.review",
            "name":"Review","version":"1.0.0",
            "agent":{"runtime":"rpi-native","entry":"src/lib.rs"}
        }))
        .unwrap();
        let pin = manifest.pin();
        let mut library = resources::ResourceLibrary::default();
        library.extensions.push(resources::ExtensionResource {
            id: pin.key(),
            name: "Review".into(),
            description: String::new(),
            source: pin.key(),
            content: String::new(),
            manifest: Some(manifest),
            acquired: true,
            design_sample: false,
        });
        let binding = resources::SessionBinding {
            session_id: "node/session".into(),
            extension_pins: vec![
                pin.clone(),
                ExtensionPin {
                    id: "missing.extension".into(),
                    version: "0.1.0".into(),
                },
            ],
            ..resources::SessionBinding::default()
        };
        let snapshot = super::resource_snapshot(&binding, &library);
        assert_eq!(snapshot["extensions"]["status"], "desired-only");
        assert!(snapshot["extensions"]["nodeApplied"].is_null());
        assert_eq!(snapshot["extensions"]["pins"][0]["version"], "1.0.0");
        assert_eq!(snapshot["extensions"]["pins"][0]["unresolved"], false);
        assert_eq!(snapshot["extensions"]["pins"][1]["unresolved"], true);
        library.ui_preferences.push(UiExtensionPreference {
            extension: pin,
            contribution_id: "status".into(),
            visible: false,
        });
        assert_eq!(super::resource_snapshot(&binding, &library), snapshot);
        library.extensions[0].manifest.as_mut().unwrap().version = "2.0.0".into();
        let missing_old_version = super::resource_snapshot(&binding, &library);
        assert_eq!(
            missing_old_version["extensions"]["pins"][0]["version"],
            "1.0.0"
        );
        assert_eq!(
            missing_old_version["extensions"]["pins"][0]["unresolved"],
            true
        );
    }

    #[test]
    fn first_message_title_uses_a_bounded_unicode_first_line() {
        assert_eq!(
            super::conversation_title("\n  修复  登录问题 \n更多细节"),
            "修复 登录问题"
        );
        assert_eq!(
            super::conversation_title(&"界".repeat(40)),
            format!("{}…", "界".repeat(32))
        );
    }

    #[test]
    fn task_badge_uses_settlement_semantics_and_never_claims_completion_offline() {
        let mut completed = RunProjection::new("run-1");
        completed.phase = RunPhase::Settled(SettlementOutcome::Completed);
        assert_eq!(
            task_status(Some(&completed), &Phase::Online, true),
            ("completed", "已完成")
        );
        completed.phase = RunPhase::Settled(SettlementOutcome::Failed);
        assert_eq!(
            task_status(Some(&completed), &Phase::Online, true),
            ("failed", "执行失败")
        );
        completed.connection = ConnectionState::Reconnecting;
        assert_eq!(
            task_status(Some(&completed), &Phase::Online, true),
            ("unknown", "任务状态未知")
        );
        assert_eq!(
            task_status(None, &Phase::Failed("断开".into()), true),
            ("unknown", "任务状态未知")
        );
    }

    #[test]
    fn interaction_state_is_shared_by_send_queue_and_stop_controls() {
        let projection = RunProjection::new("run-2");
        assert_eq!(
            interaction_state(Some(&projection), true, false, false),
            super::InteractionState::Ready
        );
        assert_eq!(
            interaction_state(Some(&projection), true, true, false),
            super::InteractionState::Sending
        );
        assert_eq!(
            interaction_state(Some(&projection), true, false, true),
            super::InteractionState::Running
        );
        assert_eq!(
            interaction_state(Some(&projection), false, true, true),
            super::InteractionState::Disconnected
        );
        let mut cancelling = RunProjection::new("run-cancelling");
        cancelling.phase = RunPhase::Cancelling;
        assert_eq!(
            interaction_state(Some(&cancelling), true, false, true),
            super::InteractionState::Cancelling
        );
        let mut awaiting_input = RunProjection::new("run-awaiting-input");
        awaiting_input.phase = RunPhase::AwaitingInput;
        assert_eq!(
            interaction_state(Some(&awaiting_input), true, true, false),
            super::InteractionState::AwaitingInput
        );
    }

    #[test]
    fn activity_feedback_lasts_through_output_tools_and_settlement_wait() {
        for phase in [
            RunPhase::Accepted,
            RunPhase::Preparing,
            RunPhase::Text,
            RunPhase::Tool,
            RunPhase::AwaitingSettlement,
        ] {
            let mut projection = RunProjection::new("active");
            projection.phase = phase;
            let state = interaction_state(Some(&projection), true, false, false);
            assert_eq!(state.activity_label(false), Some("正在处理…"));
            assert!(state.composer_loading(true, false));
            assert!(!state.composer_loading(false, false));
        }
        for phase in [
            RunPhase::Ready,
            RunPhase::Settled(SettlementOutcome::Completed),
            RunPhase::Settled(SettlementOutcome::Failed),
            RunPhase::Settled(SettlementOutcome::Interrupted),
        ] {
            let mut projection = RunProjection::new("inactive");
            projection.phase = phase;
            let state = interaction_state(Some(&projection), true, false, false);
            assert_eq!(state.activity_label(false), None);
            assert!(!state.composer_loading(true, false));
        }
    }

    #[test]
    fn active_primary_action_stays_stop_independently_of_draft_or_queue_submission() {
        use super::InteractionState;
        for state in [
            InteractionState::Running,
            InteractionState::AwaitingInput,
            InteractionState::Cancelling,
        ] {
            assert!(state.primary_action_is_stop());
        }
        for state in [
            InteractionState::Ready,
            InteractionState::Disconnected,
            InteractionState::Sending,
        ] {
            assert!(!state.primary_action_is_stop());
        }
    }

    #[test]
    fn composer_loading_distinguishes_submitted_text_from_a_new_draft() {
        use super::InteractionState;
        assert!(InteractionState::Sending.composer_loading(false, true));
        assert!(!InteractionState::Sending.composer_loading(false, false));
        assert!(InteractionState::Running.composer_loading(true, false));
        assert_eq!(
            InteractionState::Sending.activity_label(true),
            Some("正在发送…")
        );
        assert_eq!(
            InteractionState::Running.activity_label(true),
            Some("等待回复…")
        );
        assert_eq!(
            InteractionState::AwaitingInput.activity_label(true),
            Some("等待确认…")
        );
        assert_eq!(
            InteractionState::Cancelling.activity_label(true),
            Some("正在停止…")
        );
        assert_eq!(InteractionState::Disconnected.activity_label(true), None);
        assert!(!InteractionState::Disconnected.composer_loading(true, true));
    }

    #[test]
    fn local_lifecycle_does_not_prompt_for_connection() {
        assert_eq!(projection_status(None, &Phase::Online, true), "本机就绪");
        assert_eq!(
            projection_status(None, &Phase::Connecting, true),
            "准备本机环境"
        );
        assert_eq!(
            projection_status(None, &Phase::Failed("runtime missing".into()), true),
            "本机不可用"
        );
        assert_eq!(
            projection_status(None, &Phase::PasswordRequired, false),
            "需要登录"
        );
    }
}
