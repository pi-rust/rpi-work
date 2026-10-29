//! Native RPC workspace and HTTP/SSE clients for RPI Work.
#![allow(clippy::missing_errors_doc)]
//!
//! `WorkspaceClient` selects the desktop's in-process native workspace or an
//! explicit remote HTTP client. Agent execution uses native RPC; local project,
//! file and Git operations reuse the shared Rust core. Both event sources feed
//! the same `work-core` projection without turning request acceptance into run
//! completion.

use async_stream::try_stream;
use futures_core::Stream;
use futures_util::StreamExt;
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::pin::Pin;
use std::time::Duration;
use thiserror::Error;
use work_core::{
    ApplyResult, ConnectionState, Event, EventKind, OutputMode, RunProjection, SettlementOutcome,
    Usage,
};

#[cfg(not(target_arch = "wasm32"))]
mod native_lifecycle;
mod permissions;
mod transport;
pub use transport::{
    AgentClient, AgentConnection, AgentSession, AgentSessionConnection, AgentTransport,
    unsupported, wire_event,
};

#[cfg(not(target_arch = "wasm32"))]
mod acp;
#[cfg(not(target_arch = "wasm32"))]
pub use acp::AcpClient;

#[cfg(not(target_arch = "wasm32"))]
mod rpc;
#[cfg(not(target_arch = "wasm32"))]
pub use rpc::RpiRpcClient;

#[cfg(not(target_arch = "wasm32"))]
pub type EventStream = Pin<Box<dyn Stream<Item = Result<WireEvent, ClientError>> + Send>>;
#[cfg(target_arch = "wasm32")]
pub type EventStream = Pin<Box<dyn Stream<Item = Result<WireEvent, ClientError>>>>;

pub mod workspace;
pub use workspace::WorkspaceClient;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
mod local_workspace;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub use local_workspace::{LocalError, LocalWorkspace};

#[derive(Debug, Error)]
pub enum ClientError {
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    #[error(transparent)]
    Local(#[from] LocalError),
    #[error("invalid base URL: {0}")]
    InvalidBaseUrl(#[from] url::ParseError),
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API returned HTTP {status}: {message}")]
    Api { status: StatusCode, message: String },
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid SSE stream: {0}")]
    Sse(String),
    #[error("node did not confirm authentication")]
    AuthenticationNotConfirmed,
    #[error("operation `{operation}` is unsupported by {protocol}")]
    Unsupported { operation: String, protocol: String },
    #[error("RPC transport error: {0}")]
    Rpc(String),
    #[error("ACP transport error: {0}")]
    Acp(String),
    #[error("agent protocol mismatch: {0}")]
    ProtocolMismatch(String),
    #[error("agent request timed out: {0}")]
    RequestTimeout(String),
    #[error("agent process failed: {0}")]
    Process(String),
    #[error("agent process exited: {0}")]
    ChildExited(String),
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Health {
    #[serde(default = "default_health_ok")]
    pub ok: bool,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub protocol: Option<String>,
}

fn default_health_ok() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Capabilities {
    pub protocol: String,
    #[serde(default, rename = "piCompatibilityTarget")]
    pub pi_compatibility_target: Option<String>,
    #[serde(default, rename = "supportedCommands")]
    pub supported_commands: Vec<String>,
    #[serde(default, rename = "supportedActions")]
    pub supported_actions: Vec<String>,
    #[serde(default)]
    pub semantics: Option<Value>,
    #[serde(default)]
    pub limits: Option<Value>,
    #[serde(default, rename = "parityComplete")]
    pub parity_complete: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct AuthState {
    pub required: bool,
    pub authenticated: bool,
}

#[derive(Debug, Deserialize)]
struct AuthAck {
    authenticated: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ProjectCreation {
    pub project: Project,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ProjectFolder {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ProjectFolders {
    pub root: String,
    pub path: String,
    pub folders: Vec<ProjectFolder>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct FileListing {
    pub path: String,
    pub entries: Vec<FileEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct FileContent {
    pub path: String,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ChangedFile {
    pub path: String,
    #[serde(rename = "originalPath")]
    pub original_path: Option<String>,
    pub status: String,
    pub staged: bool,
    pub unstaged: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct GitStatus {
    #[serde(rename = "isRepository")]
    pub is_repository: bool,
    pub branch: Option<String>,
    #[serde(rename = "repositoryRoot")]
    pub repository_root: Option<String>,
    #[serde(rename = "worktreePath")]
    pub worktree_path: Option<String>,
    pub files: Vec<ChangedFile>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Diff {
    pub path: String,
    pub patch: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct QueueItem {
    #[serde(rename = "entryId")]
    pub entry_id: String,
    pub queue: String,
    pub text: String,
    pub status: Option<String>,
    #[serde(rename = "completedSeq")]
    pub completed_seq: Option<u64>,
    pub message: Option<Value>,
    #[serde(rename = "runId")]
    pub run_id: Option<String>,
    pub seq: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct QueueSnapshot {
    pub items: Vec<QueueItem>,
    pub count: u64,
}

/// Presentation data only; unknown fields (including questions[]) remain available
/// to consumers so an unsupported request cannot silently become a different one.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiDialogPayload {
    pub purpose: Option<String>,
    pub question: Option<String>,
    pub message: Option<String>,
    pub context: Option<String>,
    pub kind: Option<String>,
    pub options: Vec<UiDialogOption>,
    pub allow_freeform: bool,
    pub allow_multiple: bool,
    pub raw: Value,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiDialogOption {
    pub title: Option<String>,
    pub value: Option<Value>,
    pub permission: Option<String>,
    pub description: Option<String>,
    pub raw: Value,
}

impl<'de> Deserialize<'de> for UiDialogPayload {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(deserializer)?;
        if !raw.is_object() {
            return Err(serde::de::Error::custom("dialog UI must be an object"));
        }
        let text = |key: &str| raw.get(key).and_then(Value::as_str).map(str::to_owned);
        let flag = |keys: &[&str]| {
            keys.iter()
                .any(|key| raw.get(*key).and_then(Value::as_bool) == Some(true))
        };
        let options = raw
            .get("options")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .cloned()
                    .map(serde_json::from_value)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()
            .map_err(serde::de::Error::custom)?
            .unwrap_or_default();
        Ok(Self {
            purpose: text("purpose"),
            question: text("question"),
            message: text("message"),
            context: text("context"),
            kind: text("kind"),
            options,
            allow_freeform: flag(&["allowFreeform", "allow_freeform", "allow_custom"]),
            allow_multiple: flag(&["allowMultiple", "allow_multiple", "multiple"]),
            raw,
        })
    }
}

impl<'de> Deserialize<'de> for UiDialogOption {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(deserializer)?;
        let text = |keys: &[&str]| {
            keys.iter()
                .find_map(|key| raw.get(*key).and_then(Value::as_str).map(str::to_owned))
        };
        Ok(Self {
            title: raw
                .as_str()
                .map(str::to_owned)
                .or_else(|| text(&["title", "label", "text", "value", "name"])),
            value: raw.get("value").cloned(),
            permission: text(&["permission"]),
            description: text(&["description"]),
            raw,
        })
    }
}

impl Serialize for UiDialogPayload {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.raw.is_object()
            && serde_json::from_value::<Self>(self.raw.clone()).is_ok_and(|parsed| &parsed == self)
        {
            return self.raw.serialize(serializer);
        }
        let mut raw = self.raw.as_object().cloned().unwrap_or_default();
        for (key, value) in [
            ("purpose", &self.purpose),
            ("question", &self.question),
            ("message", &self.message),
            ("context", &self.context),
            ("kind", &self.kind),
        ] {
            if let Some(value) = value {
                raw.insert(key.into(), json!(value));
            }
        }
        if !self.options.is_empty() {
            raw.insert("options".into(), json!(self.options));
        }
        if self.allow_freeform {
            raw.insert("allowFreeform".into(), json!(true));
        }
        if self.allow_multiple {
            raw.insert("allowMultiple".into(), json!(true));
        }
        raw.serialize(serializer)
    }
}

impl Serialize for UiDialogOption {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serde_json::from_value::<Self>(self.raw.clone()).is_ok_and(|parsed| &parsed == self) {
            return self.raw.serialize(serializer);
        }
        let mut raw = self.raw.as_object().cloned().unwrap_or_default();
        for (key, value) in [
            ("title", &self.title),
            ("permission", &self.permission),
            ("description", &self.description),
        ] {
            if let Some(value) = value {
                raw.insert(key.into(), json!(value));
            }
        }
        if let Some(value) = &self.value {
            raw.insert("value".into(), value.clone());
        }
        raw.serialize(serializer)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct PendingDialog {
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "toolCallId")]
    pub tool_call_id: Option<String>,
    #[serde(rename = "responseId")]
    pub response_id: Option<Value>,
    pub ui: UiDialogPayload,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct QueueMutation {
    pub accepted: Option<bool>,
    #[serde(rename = "entryId")]
    pub entry_id: Option<String>,
    pub queue: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct QueueCancellation {
    #[serde(rename = "entryId")]
    pub entry_id: String,
    pub outcome: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SessionSummary {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "projectId")]
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub status: String,
    #[serde(rename = "createdAt")]
    pub created_at: u64,
    #[serde(rename = "updatedAt")]
    pub updated_at: u64,
    #[serde(rename = "lastSeq")]
    pub last_seq: u64,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default, rename = "pendingResume")]
    pub pending_resume: bool,
    #[serde(default, rename = "latestSettlement")]
    pub latest_settlement: Option<Value>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SessionPreferences {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub archived: bool,
    pub pinned: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct RuntimeState {
    #[serde(rename = "modelId")]
    pub model_id: Option<String>,
    pub provider: Option<String>,
    #[serde(rename = "thinkingLevel")]
    pub thinking_level: Option<String>,
    #[serde(rename = "activeTools", default)]
    pub active_tools: Vec<String>,
    pub context: Option<Value>,
    pub queue: Option<Value>,
    pub stats: Option<Value>,
    #[serde(rename = "activeOperation")]
    pub active_operation: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ModelDescriptor {
    pub id: String,
    pub name: String,
    pub provider: String,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub api: Option<String>,
    #[serde(rename = "contextWindow", default)]
    pub context_window: Option<u64>,
    #[serde(rename = "maxTokens", default)]
    pub max_tokens: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct AuthStatusProvider {
    pub provider: String,
    #[serde(default)]
    pub ready: bool,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub oauth: bool,
    #[serde(rename = "oauthAvailable", default)]
    pub oauth_available: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Default)]
pub struct AuthStatus {
    #[serde(default)]
    pub providers: Vec<AuthStatusProvider>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ToolCatalog {
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(rename = "activeTools", default)]
    pub active_tools: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct CompactionEntry {
    pub id: Option<String>,
    pub seq: Option<u64>,
    pub summary: Option<String>,
    #[serde(rename = "tokensBefore")]
    pub tokens_before: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SessionBranch {
    #[serde(default)]
    pub entries: Vec<Value>,
    #[serde(default)]
    pub count: u64,
}

impl SessionBranch {
    /// Extract only public compaction summaries. Message entries, including
    /// assistant thinking, are deliberately never returned to the UI.
    #[must_use]
    pub fn latest_compaction(&self) -> Option<CompactionEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("compaction"))
            .map(|entry| CompactionEntry {
                id: entry.get("id").and_then(Value::as_str).map(str::to_owned),
                seq: entry.get("seq").and_then(Value::as_u64),
                summary: entry
                    .get("summary")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                tokens_before: entry.get("tokensBefore").and_then(Value::as_u64),
            })
            .max_by_key(|entry| entry.seq.unwrap_or_default())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct History {
    #[serde(default)]
    pub records: Vec<Value>,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub limit: u64,
    #[serde(rename = "afterSeq", default)]
    pub after_seq: Option<u64>,
    #[serde(rename = "beforeSeq", default)]
    pub before_seq: Option<u64>,
    #[serde(rename = "firstSeq", default)]
    pub first_seq: Option<u64>,
    #[serde(rename = "lastSeq", default)]
    pub last_seq: Option<u64>,
    #[serde(rename = "hasMore", default)]
    pub has_more: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SessionMutation {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "sourceSessionId")]
    pub source_session_id: Option<String>,
    pub title: Option<String>,
    pub status: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Snapshot {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(default)]
    pub messages: Vec<Value>,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default, rename = "lastSeq")]
    pub last_seq: u64,
    #[serde(default, rename = "pendingResume")]
    pub pending_resume: bool,
    #[serde(default, rename = "pendingDialog")]
    pub pending_dialog: Option<PendingDialog>,
    #[serde(default, rename = "pendingDialogs")]
    pub pending_dialogs: Vec<PendingDialog>,
    #[serde(default, rename = "latestSettlement")]
    pub latest_settlement: Option<Value>,
    #[serde(default, rename = "firstId")]
    pub first_id: Option<String>,
    #[serde(default, rename = "lastId")]
    pub last_id: Option<String>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub events: Vec<SnapshotEvent>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SnapshotEvent {
    pub seq: u64,
    pub event: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct HostResponse<T> {
    #[serde(rename = "type")]
    pub response_type: String,
    pub id: Option<Value>,
    pub command: String,
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct WireEvent {
    pub id: Option<String>,
    pub seq: Option<u64>,
    pub event_type: String,
    pub data: Value,
}

impl WireEvent {
    pub fn into_core_event(self) -> Result<Event, ClientError> {
        let id = self
            .id
            .clone()
            .or_else(|| self.seq.map(|seq| format!("seq-{seq}")))
            .unwrap_or_else(|| {
                let fingerprint = serde_json::to_string(&self.data).unwrap_or_default();
                format!("event-{}-{fingerprint}", self.event_type)
            });
        let seq = self.seq;
        let kind = normalize_event(&self.event_type, &self.data)?;
        Ok(Event { id, seq, kind })
    }
}

/// Convert rpi HTTP API wire event names to the transport-neutral core model.
#[allow(clippy::too_many_lines)]
pub fn normalize_event(event_type: &str, data: &Value) -> Result<EventKind, ClientError> {
    let object = data.as_object();
    let string = |key: &str| object.and_then(|map| map.get(key)).and_then(Value::as_str);
    let value_text = |value: Option<&Value>| -> Result<String, ClientError> {
        Ok(value.map(display_text).unwrap_or_default())
    };
    let message = || {
        object
            .and_then(|map| map.get("message"))
            .and_then(Value::as_object)
    };
    let message_id = || {
        message()
            .and_then(|map| {
                map.get("entryId")
                    .or_else(|| map.get("messageId"))
                    .or_else(|| map.get("id"))
            })
            .and_then(Value::as_str)
            .or_else(|| string("messageId"))
            .or_else(|| string("entryId"))
            .unwrap_or("assistant-message")
    };
    if matches!(
        event_type,
        "message_start" | "message_update" | "message_end"
    ) && message()
        .and_then(|map| map.get("role").or_else(|| map.get("kind")))
        .and_then(Value::as_str)
        .is_some_and(|role| role != "assistant")
    {
        return Ok(EventKind::Ignored);
    }

    match event_type {
        // SSE ready confirms the subscription, not a newly accepted agent run.
        // Treating it as Accepted makes an idle/reopened chat show Stop and
        // sends the first user message to the queue instead of starting a run.
        "operation_accepted" | "agent_start" => Ok(EventKind::Accepted),
        "agent_end" => Ok(EventKind::AgentEnd),
        "agent_settled" => Ok(EventKind::AgentSettled {
            outcome: settlement(string("outcome"), data),
            usage: usage(object.and_then(|map| map.get("usage"))),
        }),
        "resync_required" => Ok(EventKind::ResyncRequired {
            reason: string("reason")
                .unwrap_or("event history requires a snapshot")
                .into(),
        }),
        "ui_dialog_request" => Ok(EventKind::AwaitingInput),
        "tool_execution_start" => Ok(EventKind::ToolStarted {
            tool_id: required_string(object, "toolCallId")?,
            name: string("toolName").unwrap_or("tool").into(),
            arguments: value_text(
                object.and_then(|map| map.get("args").or_else(|| map.get("arguments"))),
            )?,
        }),
        "tool_execution_update" => Ok(EventKind::ToolOutput {
            tool_id: required_string(object, "toolCallId")?,
            content: value_text(object.and_then(|map| map.get("partialResult")))?,
            mode: OutputMode::Snapshot,
        }),
        "tool_execution_end" => {
            let error = string("error").map(str::to_owned).or_else(|| {
                object.and_then(|map| map.get("isError")).and_then(|value| {
                    value
                        .as_bool()
                        .filter(|is_error| *is_error)
                        .map(|_| "tool failed".into())
                })
            });
            Ok(EventKind::ToolFinished {
                tool_id: required_string(object, "toolCallId")?,
                result: value_text(object.and_then(|map| map.get("result")))?,
                error,
            })
        }
        "message_update" => {
            let event_kind = string("eventType").or_else(|| {
                object
                    .and_then(|map| map.get("assistantMessageEvent"))
                    .and_then(|value| value.get("type"))
                    .and_then(Value::as_str)
            });
            if matches!(event_kind, Some("text_delta")) {
                return Ok(EventKind::TextDelta {
                    block_id: format!(
                        "{}-text-{}",
                        message_id(),
                        object
                            .and_then(|map| map.get("contentIndex"))
                            .and_then(Value::as_u64)
                            .unwrap_or(0)
                    ),
                    delta: value_text(object.and_then(|map| map.get("delta")))?,
                });
            }
            if matches!(event_kind, Some("thinking_delta" | "tool_call_delta")) {
                return Ok(EventKind::Ignored);
            }
            let text = message()
                .and_then(|map| map.get("content"))
                .map(content_text)
                .unwrap_or_default();
            Ok(EventKind::TextSnapshot {
                block_id: format!("{}-text-0", message_id()),
                text,
            })
        }
        "message_start" | "message_end" => {
            let text = message()
                .and_then(|map| map.get("content"))
                .map(content_text)
                .unwrap_or_default();
            Ok(EventKind::TextSnapshot {
                block_id: format!("{}-text-0", message_id()),
                text,
            })
        }
        "connection_reconnecting" => {
            Ok(EventKind::ConnectionChanged(ConnectionState::Reconnecting))
        }
        "ready" | "connection_online" => Ok(EventKind::ConnectionChanged(ConnectionState::Online)),
        "connection_offline" => Ok(EventKind::ConnectionChanged(ConnectionState::Offline)),
        _ => Ok(EventKind::Ignored),
    }
}

fn required_string(
    object: Option<&serde_json::Map<String, Value>>,
    key: &str,
) -> Result<String, ClientError> {
    object
        .and_then(|map| map.get(key))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ClientError::Sse(format!("event is missing {key}")))
}

fn settlement(value: Option<&str>, data: &Value) -> SettlementOutcome {
    let value = value.or_else(|| {
        data.get("aborted")
            .and_then(|aborted| aborted.as_bool().filter(|value| *value).map(|_| "aborted"))
    });
    match value.unwrap_or("unknown") {
        "completed" => SettlementOutcome::Completed,
        "failed" => SettlementOutcome::Failed,
        "interrupted" | "aborted" => SettlementOutcome::Interrupted,
        "suspended" => SettlementOutcome::Suspended,
        "compacted" => SettlementOutcome::Compacted,
        "declined" | "skipped" => SettlementOutcome::Declined,
        other => SettlementOutcome::Unknown(if other == "unknown" {
            data.get("outcome")
                .and_then(Value::as_str)
                .unwrap_or(other)
                .to_owned()
        } else {
            other.to_owned()
        }),
    }
}

fn usage(value: Option<&Value>) -> Option<Usage> {
    let object = value?.as_object()?;
    Some(Usage {
        input_tokens: number(object, "input").or_else(|| number(object, "inputTokens")),
        output_tokens: number(object, "output").or_else(|| number(object, "outputTokens")),
        cache_read_tokens: number(object, "cache_read").or_else(|| number(object, "cacheRead")),
        cache_write_tokens: number(object, "cache_write").or_else(|| number(object, "cacheWrite")),
    })
}

fn number(object: &serde_json::Map<String, Value>, key: &str) -> Option<u64> {
    object.get(key).and_then(Value::as_u64)
}

fn content_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(display_text)
            .filter(|text| !text.is_empty())
            .collect(),
        Value::Object(object) => object
            .get("content")
            .or_else(|| object.get("text"))
            .map(display_text)
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn display_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(_) => content_text(value),
        Value::Object(object) if object.contains_key("content") || object.contains_key("text") => {
            content_text(value)
        }
        Value::Object(_) | Value::Number(_) | Value::Bool(_) => {
            serde_json::to_string(value).unwrap_or_default()
        }
        Value::Null => String::new(),
    }
}

#[derive(Default)]
pub struct SseDecoder {
    buffer: Vec<u8>,
    id: Option<String>,
    event: Option<String>,
    data: Vec<String>,
}

impl SseDecoder {
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<WireEvent>, ClientError> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(index) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line = self.buffer.drain(..=index).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8(line)
                .map_err(|error| ClientError::Sse(format!("invalid UTF-8 line: {error}")))?;
            self.line(&line, &mut events)?;
        }
        Ok(events)
    }

    pub fn finish(&mut self) -> Result<Vec<WireEvent>, ClientError> {
        if !self.buffer.is_empty() {
            let line = String::from_utf8(std::mem::take(&mut self.buffer))
                .map_err(|error| ClientError::Sse(format!("invalid UTF-8 line: {error}")))?;
            let mut events = Vec::new();
            self.line(&line, &mut events)?;
            if !self.data.is_empty() {
                self.dispatch(&mut events)?;
            }
            return Ok(events);
        }
        let mut events = Vec::new();
        if !self.data.is_empty() {
            self.dispatch(&mut events)?;
        }
        Ok(events)
    }

    fn line(&mut self, line: &str, events: &mut Vec<WireEvent>) -> Result<(), ClientError> {
        if line.is_empty() {
            return self.dispatch(events);
        }
        if line.starts_with(':') {
            return Ok(());
        }
        let (field, value) = line.split_once(':').map_or((line, ""), |(field, value)| {
            (field, value.strip_prefix(' ').unwrap_or(value))
        });
        match field {
            "id" => self.id = Some(value.to_owned()),
            "event" => self.event = Some(value.to_owned()),
            "data" => self.data.push(value.to_owned()),
            _ => {}
        }
        Ok(())
    }

    fn dispatch(&mut self, events: &mut Vec<WireEvent>) -> Result<(), ClientError> {
        if self.data.is_empty() {
            self.reset();
            return Ok(());
        }
        let data = serde_json::from_str::<Value>(&self.data.join("\n"))?;
        let seq = self.id.as_deref().and_then(|id| id.parse::<u64>().ok());
        let event_type = self
            .event
            .clone()
            .or_else(|| data.get("type").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_else(|| "message".into());
        events.push(WireEvent {
            id: self.id.take(),
            seq,
            event_type,
            data,
        });
        self.reset();
        Ok(())
    }

    fn reset(&mut self) {
        self.id = None;
        self.event = None;
        self.data.clear();
    }
}

fn payload_with_type(command: &str, payload: Value) -> Value {
    let mut object = match payload {
        Value::Object(object) => object,
        _ => serde_json::Map::new(),
    };
    object.insert("type".into(), Value::String(command.into()));
    Value::Object(object)
}

#[cfg(not(target_arch = "wasm32"))]
fn build_http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder().cookie_store(true).build()
}

#[cfg(target_arch = "wasm32")]
fn build_http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder().build()
}

fn reported_active_tools(state: &Value) -> Result<Vec<String>, ClientError> {
    state
        .get("activeTools")
        .and_then(Value::as_array)
        .ok_or_else(|| ClientError::Sse("get_state omitted activeTools".into()))?
        .iter()
        .map(|tool| {
            tool.as_str().map(str::to_owned).ok_or_else(|| {
                ClientError::Sse("get_state returned an invalid activeTools entry".into())
            })
        })
        .collect()
}

#[derive(Clone)]
pub struct WorkClient {
    http: reqwest::Client,
    base_url: Url,
    bearer_token: Option<String>,
}

fn authenticated(
    request: reqwest::RequestBuilder,
    bearer_token: Option<&str>,
) -> reqwest::RequestBuilder {
    let request = if let Some(token) = bearer_token.filter(|token| !token.trim().is_empty()) {
        request.bearer_auth(token.trim())
    } else {
        request
    };
    #[cfg(target_arch = "wasm32")]
    {
        request.fetch_credentials_include()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        request
    }
}

impl WorkClient {
    pub fn new(base_url: &str) -> Result<Self, ClientError> {
        Self::new_with_token(base_url, None)
    }

    pub fn new_with_token(base_url: &str, bearer_token: Option<&str>) -> Result<Self, ClientError> {
        let mut base_url = Url::parse(base_url)?;
        if !base_url.path().ends_with('/') {
            let path = format!("{}/", base_url.path());
            base_url.set_path(&path);
        }
        let bearer_token = bearer_token
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_owned);
        Ok(Self {
            http: build_http_client()?,
            base_url,
            bearer_token,
        })
    }

    pub async fn health(&self) -> Result<Health, ClientError> {
        self.get("api/v1/health").await
    }

    pub async fn auth(&self) -> Result<AuthState, ClientError> {
        self.get("api/v1/auth").await
    }

    pub async fn login(&self, password: &str) -> Result<AuthState, ClientError> {
        let ack: AuthAck = self
            .post("api/v1/auth", json!({ "password": password }))
            .await?;
        let _ = ack.authenticated;
        let state = self.auth().await?;
        if !state.authenticated {
            return Err(ClientError::AuthenticationNotConfirmed);
        }
        Ok(state)
    }

    pub async fn logout(&self) -> Result<AuthState, ClientError> {
        let ack: AuthAck = self.delete_json("api/v1/auth").await?;
        let _ = ack.authenticated;
        self.auth().await
    }

    pub async fn refresh_auth(&self, provider: &str) -> Result<Value, ClientError> {
        self.post("api/v1/auth/refresh", json!({ "provider": provider }))
            .await
    }

    pub async fn capabilities(&self) -> Result<Capabilities, ClientError> {
        self.get("api/v1/capabilities").await
    }

    pub async fn projects(&self) -> Result<Vec<Project>, ClientError> {
        #[derive(Deserialize)]
        struct Response {
            projects: Vec<Project>,
        }
        Ok(self.get::<Response>("api/v1/projects").await?.projects)
    }

    pub async fn rename_project(
        &self,
        project_id: &str,
        name: &str,
    ) -> Result<Project, ClientError> {
        let result: ProjectCreation = self
            .post(
                &format!("api/v1/projects/{project_id}/rename"),
                json!({"name":name}),
            )
            .await?;
        Ok(result.project)
    }

    /// Browse relative to the node's projects root; an empty path lists that root.
    pub async fn project_folders(&self, path: &str) -> Result<ProjectFolders, ClientError> {
        let mut url = self.url("api/v1/project-folders")?;
        url.query_pairs_mut().append_pair("path", path);
        self.get_url(url).await
    }

    pub async fn create_project(
        &self,
        name: &str,
        path: Option<&str>,
        init_git: bool,
    ) -> Result<Project, ClientError> {
        let response: ProjectCreation = self
            .post(
                "api/v1/projects",
                json!({ "name": name, "path": path, "initGit": init_git }),
            )
            .await?;
        Ok(response.project)
    }

    pub async fn files(&self, project_id: &str, path: &str) -> Result<FileListing, ClientError> {
        let mut url = self.url(&format!("api/v1/projects/{project_id}/files"))?;
        url.query_pairs_mut().append_pair("path", path);
        self.get_url(url).await
    }

    pub async fn file(&self, project_id: &str, path: &str) -> Result<FileContent, ClientError> {
        let mut url = self.url(&format!("api/v1/projects/{project_id}/file"))?;
        url.query_pairs_mut().append_pair("path", path);
        self.get_url(url).await
    }

    pub async fn git_status(&self, project_id: &str) -> Result<GitStatus, ClientError> {
        self.get(&format!("api/v1/projects/{project_id}/git/status"))
            .await
    }

    pub async fn diff(
        &self,
        project_id: &str,
        path: &str,
        staged: bool,
    ) -> Result<Diff, ClientError> {
        let mut url = self.url(&format!("api/v1/projects/{project_id}/git/diff"))?;
        url.query_pairs_mut()
            .append_pair("path", path)
            .append_pair("staged", &staged.to_string());
        self.get_url(url).await
    }
    pub async fn sessions(&self) -> Result<Vec<SessionSummary>, ClientError> {
        #[derive(Deserialize)]
        struct Response {
            sessions: Vec<SessionSummary>,
        }
        Ok(self.get::<Response>("api/v1/sessions").await?.sessions)
    }

    pub async fn create_session(&self, project_id: Option<&str>) -> Result<String, ClientError> {
        #[derive(Deserialize)]
        struct Response {
            #[serde(rename = "sessionId")]
            session_id: String,
        }
        let response: Response = self
            .post("api/v1/sessions", json!({ "projectId": project_id }))
            .await?;
        Ok(response.session_id)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<SessionSummary, ClientError> {
        self.get(&format!("api/v1/sessions/{session_id}")).await
    }

    pub async fn rename_session(
        &self,
        session_id: &str,
        title: &str,
    ) -> Result<SessionMutation, ClientError> {
        self.post(
            &format!("api/v1/sessions/{session_id}/rename"),
            json!({ "title": title }),
        )
        .await
    }

    pub async fn session_preferences(
        &self,
        session_id: &str,
        archived: Option<bool>,
        pinned: Option<bool>,
    ) -> Result<SessionPreferences, ClientError> {
        self.post(
            &format!("api/v1/sessions/{session_id}/metadata"),
            json!({"archived":archived,"pinned":pinned}),
        )
        .await
    }

    pub async fn fork_session(
        &self,
        session_id: &str,
        entry_id: Option<&str>,
        title: Option<&str>,
    ) -> Result<SessionMutation, ClientError> {
        self.post(
            &format!("api/v1/sessions/{session_id}/fork"),
            json!({ "entryId": entry_id, "title": title }),
        )
        .await
    }

    pub async fn clone_session(
        &self,
        session_id: &str,
        title: Option<&str>,
    ) -> Result<SessionMutation, ClientError> {
        self.post(
            &format!("api/v1/sessions/{session_id}/clone"),
            json!({ "title": title }),
        )
        .await
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), ClientError> {
        self.delete(&format!("api/v1/sessions/{session_id}")).await
    }

    pub async fn session_branch(&self, session_id: &str) -> Result<SessionBranch, ClientError> {
        self.command_data(session_id, "get_session_branch", Value::Null)
            .await
    }

    pub async fn history(
        &self,
        session_id: &str,
        after_seq: Option<u64>,
        before_seq: Option<u64>,
        limit: u64,
    ) -> Result<History, ClientError> {
        self.command_data(
            session_id,
            "get_history",
            json!({ "afterSeq": after_seq, "beforeSeq": before_seq, "limit": limit }),
        )
        .await
    }

    pub async fn runtime_state(&self, session_id: &str) -> Result<RuntimeState, ClientError> {
        self.command_data(session_id, "get_state", Value::Null)
            .await
    }

    /// Read the explicitly reported active-tools field without allowing the
    /// `RuntimeState` serde default to turn a missing field into an empty list.
    pub async fn active_tools_state(&self, session_id: &str) -> Result<Vec<String>, ClientError> {
        let state: Value = self
            .command_data(session_id, "get_state", Value::Null)
            .await?;
        reported_active_tools(&state)
    }

    pub async fn models(&self, session_id: &str) -> Result<Vec<ModelDescriptor>, ClientError> {
        #[derive(Deserialize)]
        struct Response {
            models: Vec<ModelDescriptor>,
        }
        Ok(self
            .command_data::<Response>(session_id, "get_available_models", Value::Null)
            .await?
            .models)
    }

    pub async fn auth_status(&self, session_id: &str) -> Result<AuthStatus, ClientError> {
        self.command_data(session_id, "get_auth_status", Value::Null)
            .await
    }

    pub async fn thinking_levels(&self, session_id: &str) -> Result<Vec<String>, ClientError> {
        #[derive(Deserialize)]
        struct Response {
            levels: Vec<String>,
        }
        Ok(self
            .command_data::<Response>(session_id, "get_available_thinking_levels", Value::Null)
            .await?
            .levels)
    }

    pub async fn tools(&self, session_id: &str) -> Result<ToolCatalog, ClientError> {
        self.command_data(session_id, "get_available_tools", Value::Null)
            .await
    }

    pub async fn set_model(&self, session_id: &str, model_id: &str) -> Result<Value, ClientError> {
        self.command_data(session_id, "set_model", json!({ "modelId": model_id }))
            .await
    }

    pub async fn set_thinking_level(
        &self,
        session_id: &str,
        level: &str,
    ) -> Result<Value, ClientError> {
        self.command_data(session_id, "set_thinking_level", json!({ "level": level }))
            .await
    }

    pub async fn set_active_tools(
        &self,
        session_id: &str,
        tools: &[String],
    ) -> Result<Value, ClientError> {
        self.command_data(session_id, "set_active_tools", json!({ "tools": tools }))
            .await
    }

    pub async fn resume_pending(&self, session_id: &str) -> Result<Value, ClientError> {
        self.command_data(session_id, "resume_pending", Value::Null)
            .await
    }

    pub async fn compact(
        &self,
        session_id: &str,
        custom_instructions: Option<&str>,
    ) -> Result<Value, ClientError> {
        self.command_data(
            session_id,
            "compact",
            json!({ "customInstructions": custom_instructions }),
        )
        .await
    }

    pub async fn queue(
        &self,
        session_id: &str,
        include_completed: bool,
    ) -> Result<QueueSnapshot, ClientError> {
        self.command_data(
            session_id,
            "get_queue",
            json!({ "includeCompleted": include_completed }),
        )
        .await
    }

    pub async fn queue_message(
        &self,
        session_id: &str,
        message: &str,
        deliver_as: &str,
    ) -> Result<QueueMutation, ClientError> {
        self.command_data(
            session_id,
            "queue_message",
            json!({ "message": message, "deliverAs": deliver_as }),
        )
        .await
    }

    pub async fn cancel_queued(
        &self,
        session_id: &str,
        entry_id: &str,
    ) -> Result<QueueCancellation, ClientError> {
        self.command_data(session_id, "cancel_queued", json!({ "entryId": entry_id }))
            .await
    }

    pub async fn clear_queue(&self, session_id: &str) -> Result<Value, ClientError> {
        self.command_data(session_id, "clear_queue", Value::Null)
            .await
    }

    pub async fn dialog_response(
        &self,
        session_id: &str,
        request_id: &str,
        value: Value,
    ) -> Result<Value, ClientError> {
        self.command_data(
            session_id,
            "ui_dialog_response",
            json!({ "requestId": request_id, "answer": value }),
        )
        .await
    }

    pub async fn cancel_dialog(
        &self,
        session_id: &str,
        request_id: &str,
    ) -> Result<Value, ClientError> {
        self.command_data(
            session_id,
            "ui_dialog_response",
            json!({ "requestId": request_id, "cancelled": true }),
        )
        .await
    }

    pub async fn open_session(&self, session_id: &str) -> Result<SessionSummary, ClientError> {
        self.post_empty(&format!("api/v1/sessions/{session_id}/open"))
            .await
    }

    pub async fn snapshot(&self, session_id: &str) -> Result<Snapshot, ClientError> {
        self.get(&format!("api/v1/sessions/{session_id}/snapshot"))
            .await
    }

    pub async fn command<T: for<'de> Deserialize<'de>>(
        &self,
        session_id: &str,
        payload: Value,
    ) -> Result<HostResponse<T>, ClientError> {
        self.post(&format!("api/v1/sessions/{session_id}/commands"), payload)
            .await
    }

    pub async fn command_data<T: for<'de> Deserialize<'de>>(
        &self,
        session_id: &str,
        command: &str,
        payload: Value,
    ) -> Result<T, ClientError> {
        let response: HostResponse<T> = self
            .command(session_id, payload_with_type(command, payload))
            .await?;
        if !response.success {
            return Err(ClientError::Api {
                status: StatusCode::BAD_REQUEST,
                message: response
                    .error
                    .unwrap_or_else(|| format!("{command} failed")),
            });
        }
        response
            .data
            .ok_or_else(|| ClientError::Sse(format!("{command} returned no data")))
    }

    pub async fn prompt(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<HostResponse<Value>, ClientError> {
        self.command(session_id, json!({ "type": "prompt", "message": message }))
            .await
    }

    pub async fn abort(&self, session_id: &str) -> Result<HostResponse<Value>, ClientError> {
        self.command(session_id, json!({ "type": "abort" })).await
    }

    pub async fn subscribe(
        &self,
        session_id: &str,
        after: u64,
    ) -> Result<EventStream, ClientError> {
        let url = self.url(&format!(
            "api/v1/sessions/{session_id}/events?after={after}"
        ))?;
        let response = authenticated(
            self.http
                .get(url)
                .header("x-rpi-web", "1")
                .header("accept", "text/event-stream")
                .header("last-event-id", after.to_string()),
            self.bearer_token.as_deref(),
        )
        .send()
        .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(ClientError::Api {
                status,
                message: response.text().await.unwrap_or_default(),
            });
        }
        let mut body = response.bytes_stream();
        let stream = try_stream! {
            let mut decoder = SseDecoder::default();
            while let Some(chunk) = futures_util::StreamExt::next(&mut body).await {
                let bytes = chunk?;
                for event in decoder.feed(&bytes)? {
                    yield event;
                }
            }
            for event in decoder.finish()? {
                yield event;
            }
        };
        Ok(Box::pin(stream))
    }

    /// Open a session, restore its snapshot, then subscribe after the snapshot cursor.
    pub async fn connect_session(
        &self,
        session_id: impl Into<String>,
    ) -> Result<SessionConnection, ClientError> {
        let session_id = session_id.into();
        self.open_session(&session_id).await?;
        let snapshot = self.snapshot(&session_id).await?;
        let after = snapshot.last_seq;
        let projection = projection_from_snapshot(&snapshot)?;
        let mut identity = MessageIdentity::default();
        for item in &snapshot.events {
            identity.annotate(&item.event, Some(item.seq));
        }
        let stream = self.subscribe(&session_id, after).await?;
        let pending_dialogs = snapshot_dialogs(&snapshot);
        let messages = snapshot.messages;
        Ok(SessionConnection {
            client: WorkspaceClient::Remote(self.clone()),
            session_id,
            projection,
            stream,
            history_len: messages.len(),
            messages,
            pending_dialogs,
            reconnect_attempt: 0,
            reconnecting: false,
            connection_epoch: 0,
            identity,
        })
    }

    async fn get_url<T: for<'de> Deserialize<'de>>(&self, url: Url) -> Result<T, ClientError> {
        let response = authenticated(
            self.http.get(url).header("x-rpi-web", "1"),
            self.bearer_token.as_deref(),
        )
        .send()
        .await?;
        self.decode(response).await
    }
    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, ClientError> {
        let response = authenticated(
            self.http.get(self.url(path)?).header("x-rpi-web", "1"),
            self.bearer_token.as_deref(),
        )
        .send()
        .await?;
        self.decode(response).await
    }

    async fn delete(&self, path: &str) -> Result<(), ClientError> {
        let response = authenticated(
            self.http.delete(self.url(path)?).header("x-rpi-web", "1"),
            self.bearer_token.as_deref(),
        )
        .send()
        .await?;
        let status = response.status();
        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, message });
        }
        Ok(())
    }

    async fn delete_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
    ) -> Result<T, ClientError> {
        let response = authenticated(
            self.http.delete(self.url(path)?).header("x-rpi-web", "1"),
            self.bearer_token.as_deref(),
        )
        .send()
        .await?;
        self.decode(response).await
    }
    async fn post_empty<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, ClientError> {
        let response = authenticated(
            self.http.post(self.url(path)?).header("x-rpi-web", "1"),
            self.bearer_token.as_deref(),
        )
        .send()
        .await?;
        self.decode(response).await
    }

    async fn post<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: Value,
    ) -> Result<T, ClientError> {
        let response = authenticated(
            self.http
                .post(self.url(path)?)
                .header("x-rpi-web", "1")
                .json(&body),
            self.bearer_token.as_deref(),
        )
        .send()
        .await?;
        self.decode(response).await
    }

    async fn decode<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, ClientError> {
        let status = response.status();
        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, message });
        }
        Ok(response.json().await?)
    }

    fn url(&self, path: &str) -> Result<Url, ClientError> {
        Ok(self.base_url.join(path)?)
    }
}

/// One live wire event and the result of applying it to the projection.
#[derive(Debug)]
pub struct SessionEvent {
    pub wire: WireEvent,
    pub applied: ApplyResult,
}

pub struct SessionConnection {
    client: WorkspaceClient,
    session_id: String,
    projection: RunProjection,
    stream: EventStream,
    history_len: usize,
    messages: Vec<Value>,
    pending_dialogs: Vec<PendingDialog>,
    reconnect_attempt: u32,
    reconnecting: bool,
    connection_epoch: u64,
    identity: MessageIdentity,
}

/// Tracks the current message when a host omits its entry ID.
#[derive(Default)]
struct MessageIdentity {
    current: Option<String>,
    counter: u64,
}

impl MessageIdentity {
    fn annotate(&mut self, data: &Value, seq: Option<u64>) -> Value {
        let mut data = data.clone();
        let kind = data
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if kind == "agent_start" {
            self.current = None;
        }
        if !matches!(
            kind.as_str(),
            "message_start" | "message_update" | "message_end"
        ) {
            return data;
        }
        let Some(message) = data.get_mut("message").and_then(Value::as_object_mut) else {
            return data;
        };
        if message
            .get("role")
            .or_else(|| message.get("kind"))
            .and_then(Value::as_str)
            .is_some_and(|role| role != "assistant")
        {
            return data;
        }
        let explicit = message
            .get("entryId")
            .or_else(|| message.get("messageId"))
            .or_else(|| message.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        if kind == "message_start" || self.current.is_none() {
            self.current = explicit.or_else(|| {
                self.counter += 1;
                Some(format!(
                    "anonymous-{}-{}",
                    seq.unwrap_or_default(),
                    self.counter
                ))
            });
        }
        if let Some(id) = &self.current {
            message.insert("entryId".into(), json!(id));
        }
        if kind == "message_end" {
            self.current = None;
        }
        data
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn reconnect_sleep(delay: Duration) {
    tokio::time::sleep(delay).await;
}

#[cfg(target_arch = "wasm32")]
async fn reconnect_sleep(delay: Duration) {
    gloo_timers::future::TimeoutFuture::new(u32::try_from(delay.as_millis()).unwrap_or(u32::MAX))
        .await;
}

impl SessionConnection {
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    #[must_use]
    pub fn projection(&self) -> &RunProjection {
        &self.projection
    }

    #[must_use]
    pub fn history_len(&self) -> usize {
        self.history_len
    }

    #[must_use]
    pub fn messages(&self) -> &[Value] {
        &self.messages
    }

    #[must_use]
    pub fn pending_dialogs(&self) -> &[PendingDialog] {
        &self.pending_dialogs
    }

    pub async fn next(&mut self) -> Result<Option<SessionEvent>, ClientError> {
        if self.reconnecting {
            let delay = Duration::from_millis(250 * (1u64 << self.reconnect_attempt.min(4)));
            reconnect_sleep(delay).await;
            return match self.restore_snapshot().await {
                Ok(()) => {
                    self.reconnecting = false;
                    // Reset only after receiving an event. Repeated empty streams
                    // must also back off instead of repeatedly opening snapshots.
                    Ok(Some(self.connection_event("connection_online", None)?))
                }
                Err(error) => {
                    self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
                    Ok(Some(self.connection_event(
                        "connection_reconnecting",
                        Some(&error.to_string()),
                    )?))
                }
            };
        }
        let wire = match self.stream.next().await {
            Some(Ok(wire)) => wire,
            result => {
                self.reconnecting = true;
                self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
                let error = result.and_then(Result::err).map(|e| e.to_string());
                return Ok(Some(
                    self.connection_event("connection_reconnecting", error.as_deref())?,
                ));
            }
        };
        self.reconnect_attempt = 0;
        // Reject replay before mutating the anonymous-message identity tracker.
        if wire.event_type != "resync_required"
            && wire
                .seq
                .is_some_and(|seq| self.projection.cursor.is_some_and(|cursor| seq <= cursor))
        {
            return Ok(Some(SessionEvent {
                wire,
                applied: ApplyResult::IgnoredStale,
            }));
        }
        let mut normalized = wire.clone();
        normalized.data = self.identity.annotate(&wire.data, wire.seq);
        merge_live_message(&mut self.messages, &normalized.data, wire.seq);
        let event = normalized.into_core_event()?;
        let applied = self.projection.apply(event);
        if matches!(applied, ApplyResult::RebuiltForResync) {
            self.reconnecting = true;
        }
        if matches!(wire.event_type.as_str(), "agent_start" | "agent_settled") {
            // Native RPC does not emit the user's persisted message as a live
            // message event. Read it at run start, then reconcile final entry IDs
            // at settlement without resubscribing or advancing the SSE cursor.
            let snapshot = self.client.snapshot(&self.session_id);
            let timeout = reconnect_sleep(Duration::from_secs(5));
            futures_util::pin_mut!(snapshot, timeout);
            if let futures_util::future::Either::Left((Ok(snapshot), _)) =
                futures_util::future::select(snapshot, timeout).await
            {
                if wire.event_type == "agent_start" {
                    for message in snapshot
                        .messages
                        .iter()
                        .filter(|message| message_role(message) == Some("user"))
                    {
                        merge_live_message(&mut self.messages, &json!({"message":message}), None);
                    }
                } else {
                    reconcile_messages(&mut self.messages, &mut self.projection, snapshot.messages);
                }
            }
        }
        self.update_dialogs(&wire);
        Ok(Some(SessionEvent { wire, applied }))
    }

    fn connection_event(
        &mut self,
        event_type: &str,
        error: Option<&str>,
    ) -> Result<SessionEvent, ClientError> {
        self.connection_epoch += 1;
        let wire = WireEvent {
            id: Some(format!("connection-{}", self.connection_epoch)),
            seq: None,
            event_type: event_type.into(),
            data: json!({"type": event_type, "error": error}),
        };
        let applied = self.projection.apply(wire.clone().into_core_event()?);
        Ok(SessionEvent { wire, applied })
    }

    async fn restore_snapshot(&mut self) -> Result<(), ClientError> {
        let (snapshot, stream) = self.client.snapshot_stream(&self.session_id).await?;
        let pending_dialogs = snapshot_dialogs(&snapshot);
        let projection = projection_from_snapshot(&snapshot)?;
        // Publish the new snapshot only once its subscription is ready.
        self.projection = projection;
        self.identity = MessageIdentity::default();
        for item in &snapshot.events {
            self.identity.annotate(&item.event, Some(item.seq));
        }
        self.history_len = snapshot.messages.len();
        self.messages = snapshot.messages;
        self.pending_dialogs = pending_dialogs;
        self.stream = stream;
        Ok(())
    }

    fn update_dialogs(&mut self, wire: &WireEvent) {
        match wire.event_type.as_str() {
            "ui_dialog_request" => {
                if let Ok(Some(dialog)) = dialog_from_wire(wire) {
                    if !self
                        .pending_dialogs
                        .iter()
                        .any(|item| item.request_id == dialog.request_id)
                    {
                        self.pending_dialogs.push(dialog);
                    }
                }
            }
            "ui_dialog_closed" => {
                if let Some(request_id) = wire.data.get("requestId").and_then(Value::as_str) {
                    self.pending_dialogs
                        .retain(|dialog| dialog.request_id != request_id);
                }
            }
            _ => {}
        }
    }
}

fn message_role(message: &Value) -> Option<&str> {
    message
        .get("role")
        .or_else(|| message.get("kind"))
        .and_then(Value::as_str)
}

fn entry_id(message: &Value) -> Option<&str> {
    message
        .get("entryId")
        .or_else(|| message.get("messageId"))
        .or_else(|| message.get("id"))
        .and_then(Value::as_str)
}

fn reconcile_messages(
    messages: &mut Vec<Value>,
    projection: &mut RunProjection,
    mut canonical: Vec<Value>,
) {
    for live in messages.iter() {
        let matched = canonical.iter().find(|item| {
            (entry_id(live).is_some() && entry_id(item) == entry_id(live))
                || (message_role(live) == message_role(item)
                    && ((live.get("timestamp").and_then(Value::as_u64).is_some()
                        && item.get("timestamp") == live.get("timestamp"))
                        || (live.get("toolCallId").and_then(Value::as_str).is_some()
                            && item.get("toolCallId") == live.get("toolCallId"))))
        });
        if let Some(matched) = matched {
            if let (Some(old), Some(new)) = (entry_id(live), entry_id(matched)) {
                for block in &mut projection.blocks {
                    let id = match block {
                        work_core::MessageBlock::Text { id, .. }
                        | work_core::MessageBlock::ToolCall { id, .. }
                        | work_core::MessageBlock::PublicSummary { id, .. } => id,
                    };
                    if id == old {
                        new.clone_into(id);
                    } else if let Some(suffix) = id.strip_prefix(&format!("{old}-text-")) {
                        *id = format!("{new}-text-{suffix}");
                    }
                }
            }
        } else {
            // Older nodes may omit a just-finished live message from snapshots.
            canonical.push(live.clone());
        }
    }
    *messages = canonical;
}

fn merge_live_message(messages: &mut Vec<Value>, data: &Value, seq: Option<u64>) {
    let Some(message) = data.get("message").and_then(Value::as_object) else {
        return;
    };
    let role = message
        .get("role")
        .or_else(|| message.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(
        role,
        "user" | "assistant" | "tool" | "toolResult" | "tool_result"
    ) {
        return;
    }
    // During text streaming the assistant message_start/update often has no
    // body; projection owns that partial text until a complete snapshot
    // arrives. Keeping an empty history row would make the UI's stable-ID
    // deduplication hide the live projection.
    if role == "assistant"
        && message
            .get("content")
            .map(content_text)
            .unwrap_or_default()
            .is_empty()
    {
        return;
    }
    let id = message
        .get("entryId")
        .or_else(|| message.get("messageId"))
        .or_else(|| message.get("id"))
        .and_then(Value::as_str)
        .map_or_else(
            || {
                if let Some(call) = message.get("toolCallId").and_then(Value::as_str) {
                    format!("tool-result-{call}")
                } else if let Some(timestamp) = message.get("timestamp").and_then(Value::as_u64) {
                    format!("live-{role}-{timestamp}")
                } else {
                    format!("live-{role}-{}", seq.unwrap_or_default())
                }
            },
            str::to_owned,
        );
    let mut incoming = Value::Object(message.clone());
    if let Some(object) = incoming.as_object_mut() {
        object.insert("entryId".into(), json!(id));
    }
    if let Some(existing) = messages.iter_mut().find(|item| {
        item.get("entryId")
            .or_else(|| item.get("messageId"))
            .or_else(|| item.get("id"))
            .and_then(Value::as_str)
            == Some(id.as_str())
            || (item.get("toolCallId").and_then(Value::as_str).is_some()
                && item.get("toolCallId") == message.get("toolCallId")
                && matches!(role, "tool" | "toolResult" | "tool_result"))
            || (item.get("timestamp").and_then(Value::as_u64).is_some()
                && item.get("timestamp") == message.get("timestamp")
                && item
                    .get("role")
                    .or_else(|| item.get("kind"))
                    .and_then(Value::as_str)
                    == Some(role))
    }) {
        if let (Some(existing), Some(incoming)) = (existing.as_object_mut(), incoming.as_object()) {
            for (key, value) in incoming {
                if key != "entryId" {
                    existing.insert(key.clone(), value.clone());
                }
            }
        }
    } else {
        messages.push(incoming);
    }
}

fn snapshot_dialogs(snapshot: &Snapshot) -> Vec<PendingDialog> {
    let mut dialogs = snapshot.pending_dialogs.clone();
    if let Some(dialog) = snapshot.pending_dialog.clone()
        && !dialogs
            .iter()
            .any(|item| item.request_id == dialog.request_id)
    {
        dialogs.push(dialog);
    }
    dialogs
}

pub fn dialog_from_wire(wire: &WireEvent) -> Result<Option<PendingDialog>, ClientError> {
    let Some(request_id) = wire.data.get("requestId").and_then(Value::as_str) else {
        return Ok(None);
    };
    let ui = wire
        .data
        .get("ui")
        .cloned()
        .unwrap_or_else(|| wire.data.clone());
    Ok(Some(PendingDialog {
        request_id: request_id.to_owned(),
        tool_call_id: wire
            .data
            .get("toolCallId")
            .and_then(Value::as_str)
            .map(str::to_owned),
        response_id: wire.data.get("jsonRpcRequestId").cloned(),
        ui: serde_json::from_value(ui)?,
    }))
}

pub fn projection_from_snapshot(snapshot: &Snapshot) -> Result<RunProjection, ClientError> {
    let mut projection = RunProjection::new(snapshot.session_id.clone());
    let mut identity = MessageIdentity::default();
    let mut replayed_settlement = false;
    for item in &snapshot.events {
        let event = WireEvent {
            id: Some(item.seq.to_string()),
            seq: Some(item.seq),
            event_type: item
                .event
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .into(),
            data: identity.annotate(&item.event, Some(item.seq)),
        };
        replayed_settlement |= event.event_type == "agent_settled";
        projection.apply(event.into_core_event()?);
    }
    // Sequenced settlements already place completion in the event history. The
    // unsequenced fallback may describe an older turn, not the active one after it.
    if !replayed_settlement
        && !matches!(projection.phase, work_core::RunPhase::Settled(_))
        && let Some(settlement_data) = snapshot.latest_settlement.as_ref()
    {
        let outcome = settlement(
            settlement_data.get("outcome").and_then(Value::as_str),
            settlement_data,
        );
        projection.apply(Event {
            id: "snapshot-latest-settlement".into(),
            seq: None,
            kind: EventKind::AgentSettled {
                outcome,
                usage: usage(settlement_data.get("usage")),
            },
        });
    }
    if snapshot.pending_dialog.is_some() || !snapshot.pending_dialogs.is_empty() {
        projection.apply(Event {
            id: "snapshot-pending-dialog".into(),
            seq: None,
            kind: EventKind::AwaitingInput,
        });
    }
    projection.cursor = Some(snapshot.last_seq);
    Ok(projection)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_bearer_token_is_attached_without_exposing_empty_tokens() {
        let http = reqwest::Client::new();
        let request = authenticated(http.get("http://localhost/"), Some("secret-token"))
            .build()
            .expect("request");
        assert_eq!(
            request.headers().get("authorization").unwrap(),
            "Bearer secret-token"
        );

        let request = authenticated(http.get("http://localhost/"), Some("  "))
            .build()
            .expect("request");
        assert!(request.headers().get("authorization").is_none());
    }
    #[test]
    fn subscription_ready_does_not_start_or_reopen_a_run() {
        for phase in [
            work_core::RunPhase::Ready,
            work_core::RunPhase::Text,
            work_core::RunPhase::Settled(SettlementOutcome::Completed),
        ] {
            let mut projection = RunProjection::new("actual-session");
            projection.phase = phase.clone();
            projection.connection = ConnectionState::Reconnecting;
            projection.apply(Event {
                id: "subscription-ready".into(),
                seq: None,
                kind: normalize_event("ready", &json!({"sessionId":"actual-session"})).unwrap(),
            });
            assert_eq!(projection.phase, phase);
            assert_eq!(projection.connection, ConnectionState::Online);
        }
        assert!(matches!(
            normalize_event("agent_start", &json!({})).unwrap(),
            EventKind::Accepted
        ));
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn run_native<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(future)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn auth_test_server(
        set_cookie_on_login: bool,
        request_count: usize,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let handle = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for _ in 0..request_count {
                let (mut stream, _) = listener.accept().expect("accept test request");
                let mut request = Vec::new();
                let header_end;
                loop {
                    let mut chunk = [0u8; 4096];
                    let read = stream.read(&mut chunk).expect("read test request");
                    assert!(read > 0, "request ended before headers");
                    request.extend_from_slice(&chunk[..read]);
                    if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                        header_end = end + 4;
                        break;
                    }
                }
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Content-Length:")
                            .or_else(|| line.strip_prefix("content-length:"))
                    })
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or_default();
                while request.len() < header_end + content_length {
                    let mut chunk = [0u8; 4096];
                    let read = stream.read(&mut chunk).expect("read test request body");
                    assert!(read > 0, "request ended before body");
                    request.extend_from_slice(&chunk[..read]);
                }

                let request_text = String::from_utf8_lossy(&request);
                let request_line = request_text.lines().next().expect("request line");
                let mut request_parts = request_line.split_whitespace();
                let method = request_parts.next().expect("request method");
                let path = request_parts.next().expect("request path");
                let has_cookie = request_text.lines().any(|line| {
                    line.to_ascii_lowercase().starts_with("cookie:") && line.contains("auth=1")
                });
                requests.push(format!("{method} {path} cookie={has_cookie}"));

                let (body, extra_headers) = match (method, path) {
                    ("POST", "/api/v1/auth") => (
                        r#"{"authenticated":true}"#,
                        if set_cookie_on_login {
                            "Set-Cookie: auth=1; Path=/; HttpOnly\r\n"
                        } else {
                            ""
                        },
                    ),
                    ("DELETE", "/api/v1/auth") => (
                        r#"{"authenticated":false}"#,
                        "Set-Cookie: auth=; Max-Age=0; Path=/\r\n",
                    ),
                    ("GET", "/api/v1/auth") => (
                        if has_cookie {
                            r#"{"required":true,"authenticated":true}"#
                        } else {
                            r#"{"required":true,"authenticated":false}"#
                        },
                        "",
                    ),
                    _ => panic!("unexpected request: {method} {path}"),
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write test response");
            }
            requests
        });
        (format!("http://{address}/"), handle)
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn login_reads_sparse_ack_and_authoritative_cookie_auth_state() {
        let (url, server) = auth_test_server(true, 2);
        let client = WorkClient::new(&url).expect("client");
        let state = run_native(client.login("test-only-password"))
            .expect("sparse login acknowledgement should be accepted");
        assert_eq!(
            state,
            AuthState {
                required: true,
                authenticated: true,
            }
        );
        assert_eq!(
            server.join().expect("server"),
            vec![
                "POST /api/v1/auth cookie=false",
                "GET /api/v1/auth cookie=true",
            ]
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn logout_reads_sparse_ack_and_authoritative_cleared_cookie_state() {
        let (url, server) = auth_test_server(true, 4);
        let client = WorkClient::new(&url).expect("client");
        run_native(client.login("test-only-password")).expect("login");
        let state = run_native(client.logout()).expect("sparse logout acknowledgement");
        assert_eq!(
            state,
            AuthState {
                required: true,
                authenticated: false,
            }
        );
        assert_eq!(
            server.join().expect("server"),
            vec![
                "POST /api/v1/auth cookie=false",
                "GET /api/v1/auth cookie=true",
                "DELETE /api/v1/auth cookie=true",
                "GET /api/v1/auth cookie=false",
            ]
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn login_fails_when_authoritative_state_does_not_confirm_authentication() {
        let (url, server) = auth_test_server(false, 2);
        let client = WorkClient::new(&url).expect("client");
        assert!(matches!(
            run_native(client.login("test-only-password")),
            Err(ClientError::AuthenticationNotConfirmed)
        ));
        assert_eq!(
            server.join().expect("server"),
            vec![
                "POST /api/v1/auth cookie=false",
                "GET /api/v1/auth cookie=false",
            ]
        );
    }

    #[test]
    fn anonymous_assistant_identity_is_stable_within_message_and_distinct_between_turns() {
        let mut identity = MessageIdentity::default();
        let start = identity.annotate(
            &json!({"type":"message_start","message":{"role":"assistant"}}),
            Some(1),
        );
        let update = identity.annotate(
            &json!({"type":"message_update","message":{"role":"assistant"}}),
            Some(2),
        );
        let end = identity.annotate(
            &json!({"type":"message_end","message":{"role":"assistant"}}),
            Some(3),
        );
        assert_eq!(start["message"]["entryId"], update["message"]["entryId"]);
        assert_eq!(start["message"]["entryId"], end["message"]["entryId"]);
        let next = identity.annotate(
            &json!({"type":"message_start","message":{"role":"assistant"}}),
            Some(4),
        );
        assert_ne!(start["message"]["entryId"], next["message"]["entryId"]);
    }

    #[test]
    fn persisted_ids_and_tool_message_boundaries_do_not_duplicate_live_rows() {
        let mut identity = MessageIdentity::default();
        let mut messages = Vec::new();
        let start = identity.annotate(&json!({"type":"message_start","message":{"role":"assistant","timestamp":42,"content":"first"}}), Some(1));
        let end = identity.annotate(&json!({"type":"message_end","message":{"role":"assistant","timestamp":42,"entryId":"persisted-id","content":"complete"}}), Some(2));
        assert_eq!(start["message"]["entryId"], end["message"]["entryId"]);
        merge_live_message(&mut messages, &start, Some(1));
        merge_live_message(&mut messages, &end, Some(2));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["content"], "complete");
        for (seq, kind) in [(3, "message_start"), (4, "message_end")] {
            merge_live_message(
                &mut messages,
                &json!({"type":kind,"message":{"role":"toolResult","toolCallId":"call-1","content":"result"}}),
                Some(seq),
            );
        }
        assert_eq!(messages.len(), 2);
        // Repeated content in a genuinely different message remains distinct.
        let next = identity.annotate(&json!({"type":"message_start","message":{"role":"assistant","timestamp":43,"content":"complete"}}), Some(5));
        merge_live_message(&mut messages, &next, Some(5));
        assert_eq!(messages.len(), 3);
    }

    #[test]
    fn settlement_reconciliation_adds_native_user_messages_and_rekeys_live_blocks() {
        let mut messages = vec![
            json!({"role":"assistant","entryId":"anonymous","timestamp":42,"content":"answer"}),
        ];
        let mut projection = RunProjection::new("session");
        projection.blocks.push(work_core::MessageBlock::Text {
            id: "anonymous-text-0".into(),
            text: "answer".into(),
        });
        let canonical = vec![
            json!({"role":"user","entryId":"user-id","timestamp":41,"content":"question"}),
            json!({"role":"assistant","entryId":"persisted","timestamp":42,"content":"answer"}),
        ];
        reconcile_messages(&mut messages, &mut projection, canonical.clone());
        assert_eq!(messages, canonical);
        assert_eq!(projection.blocks[0].id(), "persisted-text-0");
        assert_eq!(projection.cursor, None);
        reconcile_messages(&mut messages, &mut projection, Vec::new());
        assert_eq!(messages, canonical);
    }

    #[test]
    fn non_assistant_events_do_not_mutate_identity_or_create_assistant_blocks() {
        let mut identity = MessageIdentity::default();
        for role in ["user", "tool", "toolResult"] {
            for kind in ["message_start", "message_update", "message_end"] {
                let data = json!({"type":kind,"message":{"role":role,"content":"not assistant"}});
                assert_eq!(identity.annotate(&data, Some(1)), data);
                assert_eq!(normalize_event(kind, &data).unwrap(), EventKind::Ignored);
                assert!(identity.current.is_none());
            }
        }
        for key in ["entryId", "messageId", "id"] {
            let data = json!({"type":"message_start","message":{"role":"assistant",key:"host-id"}});
            let annotated = identity.annotate(&data, Some(2));
            assert_eq!(annotated["message"]["entryId"], "host-id");
        }
    }

    #[test]
    fn decoder_handles_split_frames_and_comments() {
        let mut decoder = SseDecoder::default();
        assert_eq!(
            decoder
                .feed(b": heartbeat\n\nid: 7\nevent: agent_settled\nda")
                .unwrap()
                .len(),
            0
        );
        let events = decoder
            .feed(b"ta: {\"type\":\"agent_settled\",\"outcome\":\"completed\"}\n\n")
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].seq, Some(7));
        assert_eq!(events[0].event_type, "agent_settled");
    }

    #[test]
    fn health_accepts_rpi_web_payload_without_explicit_ok() {
        let health: Health = serde_json::from_value(json!({
            "service": "rpi-web",
            "protocol": "rpi"
        }))
        .unwrap();
        assert!(health.ok);
        assert_eq!(health.service.as_deref(), Some("rpi-web"));
        assert_eq!(health.protocol.as_deref(), Some("rpi"));
    }
    #[test]
    fn latest_compaction_filters_private_entries_and_uses_latest_seq() {
        let branch = SessionBranch {
            entries: vec![
                json!({"type":"message","seq":99,"content":[{"type":"thinking","text":"private"}]}),
                json!({"type":"compaction","seq":2,"id":"old","summary":"old","retainedTail":[{"thinking":"private"}]}),
                json!({"type":"compaction","seq":7,"id":"new","summary":"public","tokensBefore":42,"retainedTail":[{"thinking":"private"}]}),
            ],
            count: 3,
        };
        let latest = branch.latest_compaction().unwrap();
        assert_eq!(latest.seq, Some(7));
        assert_eq!(latest.id.as_deref(), Some("new"));
        assert_eq!(latest.summary.as_deref(), Some("public"));
        assert_eq!(latest.tokens_before, Some(42));
    }

    #[test]
    fn empty_or_non_compaction_branch_has_no_public_summary() {
        assert!(
            SessionBranch {
                entries: vec![],
                count: 0
            }
            .latest_compaction()
            .is_none()
        );
        assert!(
            SessionBranch {
                entries: vec![json!({"type":"message","seq":4,"content":"not a summary"})],
                count: 1,
            }
            .latest_compaction()
            .is_none()
        );
    }

    #[test]
    fn snapshot_preserves_history_queue_and_dialog_metadata() {
        let snapshot: Snapshot = serde_json::from_value(json!({
            "sessionId": "session-1",
            "status": "awaiting_input",
            "messages": [{"role": "user", "content": "hello"}],
            "total": 1,
            "lastSeq": 4,
            "pendingDialog": {
                "requestId": "dialog-1",
                "ui": {"question": "Continue?", "allowFreeform": true}
            },
            "pendingDialogs": [{
                "requestId": "dialog-1",
                "ui": {"question": "Continue?", "allowFreeform": true}
            }],
            "events": []
        }))
        .unwrap();
        assert_eq!(snapshot.messages.len(), 1);
        assert_eq!(snapshot.pending_dialogs.len(), 1);
        assert_eq!(snapshot_dialogs(&snapshot).len(), 1);
        assert_eq!(
            snapshot.pending_dialog.as_ref().unwrap().request_id,
            "dialog-1"
        );
        assert!(snapshot.pending_dialog.as_ref().unwrap().ui.allow_freeform);
    }

    #[test]
    fn dialog_options_accept_strings_and_preserve_unknown_question_fields() {
        let raw = json!({
            "purpose":"question", "question":"Pick", "allowMultiple":true,
            "allowFreeform":true, "header":"Context",
            "options":["Rust", {"label":"TypeScript", "description":"Web", "value":"ts", "extra":7}],
            "questions":[{"id":"nested", "question":"Q", "multiple":true}],
            "future":{"enabled":true}
        });
        let ui: UiDialogPayload = serde_json::from_value(raw.clone()).unwrap();
        assert!(ui.allow_multiple && ui.allow_freeform);
        assert_eq!(ui.options[0].title.as_deref(), Some("Rust"));
        assert_eq!(ui.options[1].title.as_deref(), Some("TypeScript"));
        assert_eq!(ui.options[1].description.as_deref(), Some("Web"));
        assert_eq!(ui.options[1].value, Some(json!("ts")));
        let encoded = serde_json::to_value(&ui).unwrap();
        assert_eq!(encoded["questions"], raw["questions"]);
        assert_eq!(encoded["future"], raw["future"]);
        assert_eq!(encoded["options"][0], "Rust");
        assert_eq!(encoded["options"][1]["extra"], 7);
        assert_eq!(
            serde_json::from_value::<UiDialogPayload>(encoded).unwrap(),
            ui
        );
    }

    #[test]
    fn dialog_flag_aliases_and_malformed_options_remain_reviewable() {
        for multiple in ["allowMultiple", "allow_multiple", "multiple"] {
            for free in ["allowFreeform", "allow_freeform", "allow_custom"] {
                let ui: UiDialogPayload =
                    serde_json::from_value(json!({multiple:true,free:true})).unwrap();
                assert!(ui.allow_multiple && ui.allow_freeform);
            }
        }
        let raw = json!({"question":"Q", "options":[null, 12, {"permission":"unknown"}]});
        let ui: UiDialogPayload = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(ui.options.len(), 3);
        assert!(ui.options.iter().all(|option| option.title.is_none()));
        assert_eq!(ui.raw, raw);
    }

    #[test]
    fn permission_wire_preserves_acp_response_id() {
        let wire = WireEvent {
            id: None,
            seq: None,
            event_type: "ui_dialog_request".into(),
            data: json!({
                "requestId":"1",
                "jsonRpcRequestId":1,
                "ui":{"purpose":"permission", "options":[{"optionId":"allow", "title":"Allow"}]}
            }),
        };
        let pending = dialog_from_wire(&wire).unwrap().unwrap();
        assert_eq!(pending.response_id, Some(json!(1)));
        assert_eq!(pending.ui.options[0].raw["optionId"], "allow");
    }
    #[test]
    fn wire_and_snapshot_keep_string_options_and_multiple_pending_requests() {
        let dialogs = json!([
            {"requestId":"one","ui":{"question":"Q1", "options":["A","B"], "multiple":true}},
            {"requestId":"two","ui":{"questions":[{"question":"Q2"},{"question":"Q3"}]}}
        ]);
        let snapshot: Snapshot =
            serde_json::from_value(json!({"sessionId":"s","pendingDialogs":dialogs})).unwrap();
        assert_eq!(snapshot_dialogs(&snapshot).len(), 2);
        for raw in dialogs.as_array().unwrap() {
            let wire = WireEvent {
                id: None,
                seq: None,
                event_type: "ui_dialog_request".into(),
                data: raw.clone(),
            };
            let pending = dialog_from_wire(&wire).unwrap().unwrap();
            assert_eq!(pending.request_id, raw["requestId"]);
            assert_eq!(pending.ui.raw, raw["ui"]);
        }
        assert!(snapshot.pending_dialogs[0].ui.allow_multiple);
    }

    #[test]
    fn command_payload_uses_protocol_type_and_dialog_answer() {
        assert_eq!(
            payload_with_type("ui_dialog_response", json!({"requestId": "d"})),
            json!({"type": "ui_dialog_response", "requestId": "d"})
        );
    }
    #[test]
    fn project_and_diff_payloads_deserialize() {
        let listing: FileListing = serde_json::from_value(json!({
            "path": "src",
            "entries": [{"name": "lib.rs", "path": "src/lib.rs", "kind": "file", "size": 12}]
        }))
        .unwrap();
        assert_eq!(listing.entries[0].path, "src/lib.rs");

        let status: GitStatus = serde_json::from_value(json!({
            "isRepository": true,
            "branch": "main",
            "files": [{"path": "src/lib.rs", "status": "modified", "staged": false, "unstaged": true}]
        }))
        .unwrap();
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.files[0].status, "modified");

        let patch: Diff = serde_json::from_value(json!({
            "path": "src/lib.rs",
            "patch": "@@ -1 +1 @@"
        }))
        .unwrap();
        assert!(patch.patch.starts_with("@@"));
    }
    #[test]
    fn normalizer_handles_native_message_deltas_and_tool_errors() {
        let delta = normalize_event(
            "message_update",
            &json!({
                "message": {"kind": "assistant", "content": []},
                "eventType": "text_delta",
                "contentIndex": 0,
                "delta": "hello"
            }),
        )
        .unwrap();
        assert_eq!(
            delta,
            EventKind::TextDelta {
                block_id: "assistant-message-text-0".into(),
                delta: "hello".into()
            }
        );
        assert_eq!(
            normalize_event(
                "message_update",
                &json!({"eventType":"thinking_delta","delta":"secret"})
            )
            .unwrap(),
            EventKind::Ignored
        );

        let started = normalize_event(
            "tool_execution_start",
            &json!({"toolCallId":"call-1","toolName":"read","args":{"path":"a.txt"}}),
        )
        .unwrap();
        assert!(matches!(
            started,
            EventKind::ToolStarted { arguments, .. } if arguments == r#"{"path":"a.txt"}"#
        ));

        let tool = normalize_event(
            "tool_execution_end",
            &json!({
                "toolCallId": "call-1",
                "toolName": "read",
                "isError": true,
                "result": {"content": [{"type":"text","text":"missing"}]}
            }),
        )
        .unwrap();
        assert!(matches!(
            tool,
            EventKind::ToolFinished { result, error: Some(_), .. } if result == "missing"
        ));
    }

    #[test]
    fn event_identity_without_server_id_includes_payload() {
        let first = WireEvent {
            id: None,
            seq: None,
            event_type: "connection_online".into(),
            data: json!({"type":"connection_online","attempt":1}),
        }
        .into_core_event()
        .unwrap();
        let second = WireEvent {
            id: None,
            seq: None,
            event_type: "connection_online".into(),
            data: json!({"type":"connection_online","attempt":2}),
        }
        .into_core_event()
        .unwrap();
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn snapshot_latest_settlement_rebuilds_terminal_phase() {
        let snapshot: Snapshot = serde_json::from_value(json!({
            "sessionId": "session-1",
            "lastSeq": 12,
            "latestSettlement": {"outcome":"aborted"},
            "events": []
        }))
        .unwrap();
        let projection = projection_from_snapshot(&snapshot).unwrap();
        assert_eq!(
            projection.phase,
            work_core::RunPhase::Settled(SettlementOutcome::Interrupted)
        );
        assert_eq!(projection.cursor, Some(12));
    }

    #[test]
    fn old_snapshot_settlement_does_not_complete_a_newer_running_turn() {
        for (event, expected) in [
            (json!({"type":"agent_start"}), work_core::RunPhase::Accepted),
            (
                json!({"type":"message_update","message":{"entryId":"new-answer","role":"assistant"},"eventType":"text_delta","delta":"Working"}),
                work_core::RunPhase::Text,
            ),
            (
                json!({"type":"tool_execution_start","toolCallId":"new-tool","toolName":"read","args":{}}),
                work_core::RunPhase::Tool,
            ),
            (
                json!({"type":"agent_end"}),
                work_core::RunPhase::AwaitingSettlement,
            ),
        ] {
            let snapshot: Snapshot = serde_json::from_value(json!({
                "sessionId":"session-1", "lastSeq":4,
                "latestSettlement":{"outcome":"completed"},
                "events":[
                    {"seq":1,"event":{"type":"agent_start"}},
                    {"seq":2,"event":{"type":"agent_settled","outcome":"completed"}},
                    {"seq":3,"event":{"type":"agent_start"}},
                    {"seq":4,"event":event}
                ]
            }))
            .unwrap();
            let projection = projection_from_snapshot(&snapshot).unwrap();
            assert_eq!(projection.phase, expected);
            assert_eq!(projection.cursor, Some(4));
        }
    }

    #[test]
    fn project_folder_and_runtime_payloads_deserialize() {
        let folders: ProjectFolders = serde_json::from_value(json!({
            "root": "D:/workspace",
            "path": "src",
            "folders": [{"name":"lib","path":"src/lib"}]
        }))
        .unwrap();
        assert_eq!(folders.folders[0].path, "src/lib");
        let state: RuntimeState = serde_json::from_value(json!({
            "modelId":"m",
            "thinkingLevel":"high",
            "activeTools":["read"]
        }))
        .unwrap();
        assert_eq!(state.model_id.as_deref(), Some("m"));
        assert_eq!(state.active_tools, vec!["read"]);
    }

    #[test]
    fn normalizer_preserves_tool_snapshots_and_settlement() {
        let event = WireEvent {
            id: Some("2".into()),
            seq: Some(2),
            event_type: "tool_execution_update".into(),
            data: json!({"type":"tool_execution_update","toolCallId":"call-1","partialResult":"out"}),
        };
        assert!(matches!(
            event.into_core_event().unwrap().kind,
            EventKind::ToolOutput {
                mode: OutputMode::Snapshot,
                ..
            }
        ));
        let settled = normalize_event("agent_settled", &json!({"outcome":"aborted"})).unwrap();
        assert_eq!(
            settled,
            EventKind::AgentSettled {
                outcome: SettlementOutcome::Interrupted,
                usage: None
            }
        );
    }
}

#[cfg(test)]
mod active_tools_tests {
    use super::*;

    #[test]
    fn tools_state_requires_explicit_array_of_strings() {
        for state in [
            json!({}),
            json!({"activeTools": null}),
            json!({"activeTools": ""}),
            json!({"activeTools": [null]}),
        ] {
            assert!(reported_active_tools(&state).is_err());
        }
        assert_eq!(
            reported_active_tools(&json!({"activeTools": []})).unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(
            reported_active_tools(&json!({"activeTools": ["read"]})).unwrap(),
            vec!["read"]
        );
    }
}
