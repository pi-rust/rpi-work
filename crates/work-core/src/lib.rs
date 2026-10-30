//! UI- and transport-independent state projection for an RPI Work run.
//!
//! The client layer is expected to translate HTTP/SSE events into [`Event`]s.
//! This crate deliberately does not know about HTTP, JSON, or a particular UI.

pub mod extensions;

use std::collections::HashSet;

/// The lifecycle of one agent run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunPhase {
    Ready,
    Accepted,
    Preparing,
    Text,
    Tool,
    AwaitingInput,
    AwaitingSettlement,
    Cancelling,
    Settled(SettlementOutcome),
}

/// The transport connection is independent from [`RunPhase`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectionState {
    Online,
    Reconnecting,
    Resyncing,
    Offline,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettlementOutcome {
    Completed,
    Failed,
    Interrupted,
    Suspended,
    Compacted,
    Declined,
    Unknown(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutputMode {
    /// Replace the current partial result with the latest provider snapshot.
    Snapshot,
    /// Append a new log fragment to the existing output.
    Append,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportIdentity {
    pub protocol: String,
    pub version: Option<String>,
    pub transport: String,
    pub agent_name: Option<String>,
}

impl TransportIdentity {
    #[must_use]
    pub fn new(
        protocol: impl Into<String>,
        version: Option<String>,
        transport: impl Into<String>,
    ) -> Self {
        Self {
            protocol: protocol.into(),
            version,
            transport: transport.into(),
            agent_name: None,
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapabilitySet {
    pub sessions: bool,
    pub session_load: bool,
    pub session_resume: bool,
    pub prompt: bool,
    pub streaming: bool,
    pub abort: bool,
    pub dialogs: bool,
    pub projects: bool,
    pub files: bool,
    pub git_diff: bool,
    pub models: bool,
    pub thinking_levels: bool,
    pub tools: bool,
    pub provider_management: bool,
    pub queue: bool,
    pub history: bool,
}

impl CapabilitySet {
    #[must_use]
    pub fn supports(&self, operation: &str) -> bool {
        match operation {
            "sessions" => self.sessions,
            "session_load" => self.session_load,
            "session_resume" => self.session_resume,
            "prompt" => self.prompt,
            "streaming" => self.streaming,
            "abort" => self.abort,
            "dialogs" => self.dialogs,
            "projects" => self.projects,
            "files" => self.files,
            "git_diff" => self.git_diff,
            "models" => self.models,
            "thinking_levels" => self.thinking_levels,
            "tools" => self.tools,
            "provider_management" => self.provider_management,
            "queue" => self.queue,
            "history" => self.history,
            _ => false,
        }
    }

    #[must_use]
    pub const fn http_default() -> Self {
        Self {
            sessions: true,
            session_load: true,
            session_resume: false,
            prompt: true,
            streaming: true,
            abort: true,
            dialogs: true,
            projects: true,
            files: true,
            git_diff: true,
            models: true,
            thinking_levels: true,
            tools: true,
            provider_management: true,
            queue: true,
            history: true,
        }
    }

    #[must_use]
    pub const fn rpc_default() -> Self {
        Self {
            sessions: true,
            session_load: false,
            session_resume: false,
            prompt: true,
            streaming: true,
            abort: true,
            dialogs: true,
            projects: false,
            files: false,
            git_diff: false,
            models: true,
            thinking_levels: true,
            tools: true,
            provider_management: false,
            queue: true,
            history: true,
        }
    }

    #[must_use]
    pub const fn acp_default() -> Self {
        Self {
            sessions: true,
            session_load: false,
            session_resume: false,
            prompt: true,
            streaming: true,
            abort: true,
            dialogs: true,
            projects: false,
            files: false,
            git_diff: false,
            models: false,
            thinking_levels: false,
            // Streamed tool events do not imply a tool catalog or mutation API.
            tools: false,
            provider_management: false,
            queue: false,
            history: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentDescriptor {
    pub identity: TransportIdentity,
    pub capabilities: CapabilitySet,
}

impl AgentDescriptor {
    #[must_use]
    pub fn supports(&self, operation: &str) -> bool {
        self.capabilities.supports(operation)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MessageBlock {
    PublicSummary {
        id: String,
        text: String,
    },
    Text {
        id: String,
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: String,
        output: String,
        status: ToolStatus,
    },
}

impl MessageBlock {
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::PublicSummary { id, .. } | Self::Text { id, .. } | Self::ToolCall { id, .. } => {
                id
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventKind {
    Accepted,
    Preparing {
        summary: String,
    },
    TextDelta {
        block_id: String,
        delta: String,
    },
    TextSnapshot {
        block_id: String,
        text: String,
    },
    ToolStarted {
        tool_id: String,
        name: String,
        arguments: String,
    },
    ToolOutput {
        tool_id: String,
        content: String,
        mode: OutputMode,
    },
    ToolFinished {
        tool_id: String,
        result: String,
        error: Option<String>,
    },
    AwaitingInput,
    AgentEnd,
    AgentSettled {
        outcome: SettlementOutcome,
        usage: Option<Usage>,
    },
    CancelRequested,
    ConnectionChanged(ConnectionState),
    /// An event recognized by the transport but not relevant to the run projection.
    Ignored,
    ResyncRequired {
        reason: String,
    },
}

/// A transport-normalized event. `id` is the stable event identity supplied by
/// the client adapter; `seq` is the node/session cursor when available.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Event {
    pub id: String,
    pub seq: Option<u64>,
    pub kind: EventKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplyResult {
    Applied,
    IgnoredDuplicate,
    IgnoredStale,
    RebuiltForResync,
}

/// A pure, incrementally updated projection of one run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunProjection {
    pub run_id: String,
    pub generation: u64,
    pub phase: RunPhase,
    pub connection: ConnectionState,
    pub cursor: Option<u64>,
    pub blocks: Vec<MessageBlock>,
    pub usage: Option<Usage>,
    seen_event_ids: HashSet<String>,
}

impl RunProjection {
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            generation: 0,
            phase: RunPhase::Ready,
            connection: ConnectionState::Online,
            cursor: None,
            blocks: Vec::new(),
            usage: None,
            seen_event_ids: HashSet::new(),
        }
    }

    /// Apply one normalized event.
    ///
    /// A duplicate event is ignored before touching the projection. A lower or
    /// equal sequence number is also ignored when it has not been seen, which
    /// protects reconnect replay from appending stale data twice. A resync is
    /// the explicit exception: it starts a new local generation and cursor.
    pub fn apply(&mut self, event: Event) -> ApplyResult {
        if matches!(event.kind, EventKind::ResyncRequired { .. }) {
            self.generation = self.generation.saturating_add(1);
            self.phase = RunPhase::Ready;
            self.connection = ConnectionState::Resyncing;
            self.cursor = None;
            self.blocks.clear();
            self.usage = None;
            self.seen_event_ids.clear();
            return ApplyResult::RebuiltForResync;
        }

        if self.seen_event_ids.contains(&event.id) {
            return ApplyResult::IgnoredDuplicate;
        }
        if let (Some(previous), Some(current)) = (self.cursor, event.seq) {
            if current <= previous {
                return ApplyResult::IgnoredStale;
            }
        }

        self.seen_event_ids.insert(event.id);
        if let Some(seq) = event.seq {
            self.cursor = Some(seq);
        }
        self.apply_kind(event.kind);
        ApplyResult::Applied
    }

    fn apply_kind(&mut self, kind: EventKind) {
        match kind {
            EventKind::Accepted => self.phase = RunPhase::Accepted,
            EventKind::Preparing { summary } => {
                self.phase = RunPhase::Preparing;
                self.upsert_summary(summary);
            }
            EventKind::TextDelta { block_id, delta } => {
                self.phase = RunPhase::Text;
                match self.find_block_mut(&block_id) {
                    Some(MessageBlock::Text { text, .. }) => text.push_str(&delta),
                    Some(_) => {}
                    None => self.blocks.push(MessageBlock::Text {
                        id: block_id,
                        text: delta,
                    }),
                }
            }
            EventKind::TextSnapshot { block_id, text } => {
                self.phase = RunPhase::Text;
                match self.find_block_mut(&block_id) {
                    Some(MessageBlock::Text { text: current, .. }) => *current = text,
                    Some(_) => {}
                    None => self.blocks.push(MessageBlock::Text { id: block_id, text }),
                }
            }
            EventKind::ToolStarted {
                tool_id,
                name,
                arguments,
            } => {
                self.phase = RunPhase::Tool;
                if let Some(MessageBlock::ToolCall {
                    name: old_name,
                    arguments: old_arguments,
                    status,
                    ..
                }) = self.find_block_mut(&tool_id)
                {
                    *old_name = name;
                    *old_arguments = arguments;
                    *status = ToolStatus::Running;
                } else {
                    self.blocks.push(MessageBlock::ToolCall {
                        id: tool_id,
                        name,
                        arguments,
                        output: String::new(),
                        status: ToolStatus::Running,
                    });
                }
            }
            EventKind::ToolOutput {
                tool_id,
                content,
                mode,
            } => {
                self.phase = RunPhase::Tool;
                if let Some(MessageBlock::ToolCall { output, .. }) = self.find_block_mut(&tool_id) {
                    match mode {
                        OutputMode::Snapshot => *output = content,
                        OutputMode::Append => output.push_str(&content),
                    }
                }
            }
            EventKind::ToolFinished {
                tool_id,
                result,
                error,
            } => {
                self.phase = RunPhase::Tool;
                if let Some(MessageBlock::ToolCall { output, status, .. }) =
                    self.find_block_mut(&tool_id)
                {
                    *output = result;
                    *status = if error.is_some() {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Completed
                    };
                }
            }
            EventKind::AwaitingInput => self.phase = RunPhase::AwaitingInput,
            EventKind::AgentEnd => self.phase = RunPhase::AwaitingSettlement,
            EventKind::AgentSettled { outcome, usage } => {
                self.phase = RunPhase::Settled(outcome);
                self.usage = usage;
            }
            EventKind::CancelRequested => self.phase = RunPhase::Cancelling,
            EventKind::ConnectionChanged(connection) => self.connection = connection,
            EventKind::Ignored => {}
            EventKind::ResyncRequired { .. } => unreachable!("handled before deduplication"),
        }
    }

    fn upsert_summary(&mut self, text: String) {
        if let Some(MessageBlock::PublicSummary { text: current, .. }) = self
            .blocks
            .iter_mut()
            .find(|block| matches!(block, MessageBlock::PublicSummary { .. }))
        {
            *current = text;
        } else {
            self.blocks.insert(
                0,
                MessageBlock::PublicSummary {
                    id: "public-summary".into(),
                    text,
                },
            );
        }
    }

    fn find_block_mut(&mut self, id: &str) -> Option<&mut MessageBlock> {
        self.blocks.iter_mut().find(|block| block.id() == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(seq: u64, id: &str, kind: EventKind) -> Event {
        Event {
            id: id.into(),
            seq: Some(seq),
            kind,
        }
    }

    #[test]
    fn settlement_is_the_only_completion_signal() {
        let mut run = RunProjection::new("run-1");
        run.apply(event(1, "end", EventKind::AgentEnd));
        assert_eq!(run.phase, RunPhase::AwaitingSettlement);

        run.apply(event(
            2,
            "settled",
            EventKind::AgentSettled {
                outcome: SettlementOutcome::Completed,
                usage: None,
            },
        ));
        assert_eq!(run.phase, RunPhase::Settled(SettlementOutcome::Completed));
    }

    #[test]
    fn transport_capabilities_are_explicit_about_protocol_boundaries() {
        assert!(CapabilitySet::http_default().projects);
        assert!(!CapabilitySet::rpc_default().projects);
        assert!(CapabilitySet::http_default().session_load);
        assert!(!CapabilitySet::rpc_default().session_load);
        assert!(!CapabilitySet::acp_default().provider_management);
        assert!(CapabilitySet::rpc_default().supports("prompt"));
        assert!(!CapabilitySet::rpc_default().supports("projects"));
        assert!(!CapabilitySet::acp_default().supports("unknown"));
        let descriptor = AgentDescriptor {
            identity: TransportIdentity::new("acp", Some("1".into()), "stdio"),
            capabilities: CapabilitySet::acp_default(),
        };
        assert!(descriptor.supports("sessions"));
        assert!(!descriptor.supports("files"));
        assert!(!descriptor.supports("session_load"));
        assert!(!descriptor.supports("session_resume"));
    }
}
