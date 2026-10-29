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

    pub(crate) fn composer_loading(self, draft: &str, submitting_original: bool) -> bool {
        self.is_active() && (draft.trim().is_empty() || submitting_original)
    }
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
    json!({
        "desiredRevision":binding.revision,
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
        super::local_node::address()
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
            assert!(state.composer_loading(" \n", false));
            assert!(!state.composer_loading("继续检查", false));
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
            assert!(!state.composer_loading("", false));
        }
    }

    #[test]
    fn composer_loading_distinguishes_submitted_text_from_a_new_draft() {
        use super::InteractionState;
        assert!(InteractionState::Sending.composer_loading("提交中的原文", true));
        assert!(!InteractionState::Sending.composer_loading("新的草稿", false));
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
        assert!(!InteractionState::Disconnected.composer_loading("", true));
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
