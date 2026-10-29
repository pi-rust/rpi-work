use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{Value, json};
use work_core::{AgentDescriptor, CapabilitySet, RunProjection, TransportIdentity};

use crate::{
    ClientError, EventStream, MessageIdentity, PendingDialog, SessionConnection, SessionEvent,
    WireEvent, WorkClient,
};

/// Native streams terminate on process failure even while the client still owns
/// its broadcast sender. Buffered events are delivered before the terminal error.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn native_event_stream(
    mut receiver: tokio::sync::broadcast::Receiver<WireEvent>,
    mut terminal: tokio::sync::watch::Receiver<Option<String>>,
    protocol: &'static str,
) -> EventStream {
    use tokio::sync::broadcast::error::{RecvError, TryRecvError};
    let stream = async_stream::try_stream! {
        let lag_error = |count| {
            let message = format!("{protocol} event stream lagged by {count} events");
            if protocol == "ACP" { ClientError::Acp(message) } else { ClientError::Rpc(message) }
        };
        loop {
            let reason = terminal.borrow().clone();
            if let Some(reason) = reason {
                loop {
                    match receiver.try_recv() {
                        Ok(event) => yield event,
                        Err(TryRecvError::Lagged(count)) => Err(lag_error(count))?,
                        Err(TryRecvError::Empty | TryRecvError::Closed) => break,
                    }
                }
                Err(ClientError::ChildExited(reason))?;
            }
            let event = tokio::select! {
                biased;
                event = receiver.recv() => Some(event),
                changed = terminal.changed() => {
                    if changed.is_err() { break; }
                    None
                }
            };
            if let Some(event) = event {
                match event {
                    Ok(event) => yield event,
                    Err(RecvError::Lagged(count)) => Err(lag_error(count))?,
                    Err(RecvError::Closed) => break,
                }
            }
        }
    };
    Box::pin(stream)
}

/// Write a frame with a single request deadline and interruption on terminal
/// state. A caller must invalidate the transport after any failed/partial write.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn write_native_frame<W: tokio::io::AsyncWrite + Unpin>(
    stdin: &tokio::sync::Mutex<W>,
    line: &[u8],
    mut terminal: tokio::sync::watch::Receiver<Option<String>>,
    deadline: tokio::time::Instant,
    on_incomplete: impl FnOnce() + Send + Sync + 'static,
) -> Result<(), ClientError> {
    use tokio::io::AsyncWriteExt;
    let write_terminal = terminal.clone();
    let write = async move {
        let mut stdin = stdin.lock().await;
        // The terminal branch may have been polled before a cancelled writer
        // poisoned the stream. Recheck after obtaining exclusive stdin access.
        let reason = write_terminal.borrow().clone();
        if let Some(reason) = reason {
            return Err(ClientError::ChildExited(reason));
        }
        // Declared after stdin, so it invalidates synchronously BEFORE stdin
        // unlocks on cancellation, write error, timeout, or EOF interruption.
        let mut incomplete = crate::native_lifecycle::CleanupGuard::new(on_incomplete);
        stdin
            .write_all(line)
            .await
            .map_err(|error| ClientError::Process(error.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|error| ClientError::Process(error.to_string()))?;
        incomplete.disarm();
        Ok(())
    };
    let stopped = async {
        loop {
            let reason = terminal.borrow().clone();
            if let Some(reason) = reason {
                return reason;
            }
            if terminal.changed().await.is_err() {
                return "native transport closed".to_owned();
            }
        }
    };
    tokio::select! {
        biased;
        reason = stopped => Err(ClientError::ChildExited(reason)),
        result = tokio::time::timeout_at(deadline, write) => result
            .map_err(|_| ClientError::RequestTimeout("writing native frame".into()))?,
    }
}

/// Protocol-neutral request surface shared by HTTP/SSE, native RPI RPC and ACP.
#[async_trait(?Send)]
pub trait AgentTransport {
    fn descriptor(&self) -> AgentDescriptor;

    /// Command-level availability within a broader descriptor capability.
    fn supports_command(&self, command: &str) -> bool {
        let capability = match command {
            "get_state" => "sessions",
            "get_messages" | "get_history" | "get_session_branch" | "get_stats"
            | "resume_pending" | "compact" | "navigate" | "append_entry" => "history",
            "get_available_models" | "set_model" => "models",
            "get_available_thinking_levels" | "set_thinking_level" => "thinking_levels",
            "get_available_tools" | "set_active_tools" => "tools",
            "get_queue" | "queue_message" | "cancel_queued" | "clear_queue" => "queue",
            "get_auth_status" | "set_api_key" | "oauth" => "provider_management",
            "prompt" => "prompt",
            "abort" | "cancel" => "abort",
            "ui_dialog_response" => "dialogs",
            _ => return false,
        };
        self.descriptor().supports(capability)
    }

    /// A negotiated config setter is independent of a discovery/catalog API.
    fn supports_config_option(&self, _session_id: &str, config_id: &str) -> bool {
        match config_id {
            "model" => self.descriptor().supports("models"),
            "thinking" | "thought_level" => self.descriptor().supports("thinking_levels"),
            _ => false,
        }
    }

    async fn open_session(&self, _cwd: &str) -> Result<String, ClientError> {
        Err(unsupported(
            "session/new",
            &self.descriptor().identity.protocol,
        ))
    }

    async fn stop(&self) -> Result<(), ClientError> {
        Err(unsupported("stop", &self.descriptor().identity.protocol))
    }

    async fn respond_to_agent_request(
        &self,
        _request_id: Value,
        _result: Value,
    ) -> Result<(), ClientError> {
        Err(unsupported(
            "session/request_permission",
            &self.descriptor().identity.protocol,
        ))
    }

    async fn load_session(&self, _session_id: &str, _cwd: &str) -> Result<Value, ClientError> {
        Err(unsupported(
            "session/load",
            &self.descriptor().identity.protocol,
        ))
    }

    async fn resume_session(&self, _session_id: &str, _cwd: &str) -> Result<Value, ClientError> {
        Err(unsupported(
            "session/resume",
            &self.descriptor().identity.protocol,
        ))
    }

    async fn set_config_option(
        &self,
        _session_id: &str,
        _config_id: &str,
        _value: Value,
    ) -> Result<Value, ClientError> {
        Err(unsupported(
            "session/set_config_option",
            &self.descriptor().identity.protocol,
        ))
    }

    async fn request(
        &self,
        session_id: &str,
        command: &str,
        payload: Value,
    ) -> Result<Value, ClientError>;

    async fn subscribe(&self, session_id: &str, after: u64) -> Result<EventStream, ClientError>;
}

pub struct AgentClient {
    backend: Arc<dyn AgentTransport>,
}

impl Clone for AgentClient {
    fn clone(&self) -> Self {
        Self {
            backend: Arc::clone(&self.backend),
        }
    }
}

impl AgentClient {
    #[must_use]
    pub fn from_transport<T>(backend: T) -> Self
    where
        T: AgentTransport + 'static,
    {
        Self {
            backend: Arc::new(backend),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn spawn_rpc(
        executable: impl AsRef<std::path::Path>,
        cwd: impl Into<std::path::PathBuf>,
    ) -> Result<Self, ClientError> {
        Ok(Self::from_transport(
            crate::RpiRpcClient::spawn(executable, cwd).await?,
        ))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn spawn_acp(
        executable: impl AsRef<std::path::Path>,
        args: &[String],
        cwd: impl Into<std::path::PathBuf>,
    ) -> Result<Self, ClientError> {
        Ok(Self::from_transport(
            crate::AcpClient::spawn(executable, args, cwd).await?,
        ))
    }

    #[must_use]
    pub fn descriptor(&self) -> AgentDescriptor {
        self.backend.descriptor()
    }

    #[must_use]
    pub fn supports(&self, operation: &str) -> bool {
        self.descriptor().supports(operation)
    }

    #[must_use]
    pub fn supports_command(&self, command: &str) -> bool {
        self.backend.supports_command(command)
    }

    #[must_use]
    pub fn supports_config_option(&self, session_id: &str, config_id: &str) -> bool {
        self.backend.supports_config_option(session_id, config_id)
    }

    pub fn require(&self, operation: &str) -> Result<(), ClientError> {
        if self.supports(operation) {
            Ok(())
        } else {
            Err(unsupported(operation, &self.descriptor().identity.protocol))
        }
    }

    #[must_use]
    pub fn session(&self, session_id: impl Into<String>) -> AgentSession {
        AgentSession {
            client: self.clone(),
            id: session_id.into(),
        }
    }

    pub async fn open_session(&self, cwd: &str) -> Result<AgentSession, ClientError> {
        self.require("sessions")?;
        let id = self.backend.open_session(cwd).await?;
        Ok(AgentSession {
            client: self.clone(),
            id,
        })
    }

    pub async fn prompt(&self, session_id: &str, message: &str) -> Result<Value, ClientError> {
        self.require("prompt")?;
        self.backend
            .request(session_id, "prompt", json!({"message": message}))
            .await
    }

    pub async fn abort(&self, session_id: &str) -> Result<Value, ClientError> {
        self.require("abort")?;
        self.backend.request(session_id, "abort", Value::Null).await
    }

    pub async fn respond_to_agent_request(
        &self,
        request_id: Value,
        result: Value,
    ) -> Result<(), ClientError> {
        self.backend
            .respond_to_agent_request(request_id, result)
            .await
    }

    pub async fn stop(&self) -> Result<(), ClientError> {
        self.backend.stop().await
    }

    pub async fn load_session(
        &self,
        session_id: &str,
        cwd: &str,
    ) -> Result<AgentSession, ClientError> {
        self.require("session_load")?;
        let result = self.backend.load_session(session_id, cwd).await?;
        let id = result
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or(session_id)
            .to_owned();
        Ok(AgentSession {
            client: self.clone(),
            id,
        })
    }

    pub async fn resume_session(
        &self,
        session_id: &str,
        cwd: &str,
    ) -> Result<AgentSession, ClientError> {
        self.require("session_resume")?;
        let result = self.backend.resume_session(session_id, cwd).await?;
        let id = result
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or(session_id)
            .to_owned();
        Ok(AgentSession {
            client: self.clone(),
            id,
        })
    }

    pub async fn set_config_option(
        &self,
        session_id: &str,
        config_id: &str,
        value: Value,
    ) -> Result<Value, ClientError> {
        if !self.supports_config_option(session_id, config_id) {
            return Err(unsupported(
                "session/set_config_option",
                &self.descriptor().identity.protocol,
            ));
        }
        self.backend
            .set_config_option(session_id, config_id, value)
            .await
    }

    pub async fn request(
        &self,
        session_id: &str,
        command: &str,
        payload: Value,
    ) -> Result<Value, ClientError> {
        self.backend.request(session_id, command, payload).await
    }

    async fn request_capability(
        &self,
        capability: &str,
        command: &str,
        session_id: &str,
        payload: Value,
    ) -> Result<Value, ClientError> {
        self.require(capability)?;
        if !self.supports_command(command) {
            return Err(unsupported(command, &self.descriptor().identity.protocol));
        }
        self.request(session_id, command, payload).await
    }

    pub async fn state(&self, session_id: &str) -> Result<Value, ClientError> {
        self.request_capability("sessions", "get_state", session_id, Value::Null)
            .await
    }

    pub async fn messages(
        &self,
        session_id: &str,
        limit: Option<u64>,
        before_id: Option<&str>,
    ) -> Result<Value, ClientError> {
        let mut payload = serde_json::Map::new();
        if let Some(limit) = limit {
            payload.insert("limit".into(), json!(limit));
        }
        if let Some(before_id) = before_id {
            payload.insert("beforeId".into(), json!(before_id));
        }
        self.request_capability(
            "history",
            "get_messages",
            session_id,
            Value::Object(payload),
        )
        .await
    }

    pub async fn history(
        &self,
        session_id: &str,
        limit: Option<u64>,
        after_seq: Option<u64>,
        before_seq: Option<u64>,
    ) -> Result<Value, ClientError> {
        let mut payload = serde_json::Map::new();
        if let Some(limit) = limit {
            payload.insert("limit".into(), json!(limit));
        }
        if let Some(after_seq) = after_seq {
            payload.insert("afterSeq".into(), json!(after_seq));
        }
        if let Some(before_seq) = before_seq {
            payload.insert("beforeSeq".into(), json!(before_seq));
        }
        self.request_capability("history", "get_history", session_id, Value::Object(payload))
            .await
    }

    pub async fn session_branch(
        &self,
        session_id: &str,
        limit: Option<u64>,
        after_seq: Option<u64>,
    ) -> Result<Value, ClientError> {
        let mut payload = serde_json::Map::new();
        if let Some(limit) = limit {
            payload.insert("limit".into(), json!(limit));
        }
        if let Some(after_seq) = after_seq {
            payload.insert("afterSeq".into(), json!(after_seq));
        }
        match self.request_capability(
            "history",
            "get_session_branch",
            session_id,
            Value::Object(payload),
        )
        .await {
            // Some runtimes expose only a custom-entry preview under this
            // command. Do not treat that filtered view as a full branch (in
            // particular it cannot supply compaction entries).
            Err(ClientError::Rpc(message)) if message.contains(
                "get_session_branch: `customTypes` is required and must be a non-empty string array"
            ) => Err(ClientError::Unsupported {
                operation: "unfiltered_session_branch".into(),
                protocol: self.descriptor().identity.protocol,
            }),
            result => result,
        }
    }

    pub async fn queue(
        &self,
        session_id: &str,
        include_completed: bool,
    ) -> Result<Value, ClientError> {
        self.request_capability(
            "queue",
            "get_queue",
            session_id,
            json!({"includeCompleted": include_completed}),
        )
        .await
    }

    pub async fn queue_message(
        &self,
        session_id: &str,
        message: &str,
        deliver_as: &str,
    ) -> Result<Value, ClientError> {
        self.request_capability(
            "queue",
            "queue_message",
            session_id,
            json!({"message": message, "deliverAs": deliver_as}),
        )
        .await
    }

    pub async fn cancel_queued(
        &self,
        session_id: &str,
        entry_id: &str,
    ) -> Result<Value, ClientError> {
        self.request_capability(
            "queue",
            "cancel_queued",
            session_id,
            json!({"entryId": entry_id}),
        )
        .await
    }

    pub async fn clear_queue(&self, session_id: &str) -> Result<Value, ClientError> {
        self.request_capability("queue", "clear_queue", session_id, Value::Null)
            .await
    }

    pub async fn models(&self, session_id: &str) -> Result<Value, ClientError> {
        self.request_capability("models", "get_available_models", session_id, Value::Null)
            .await
    }

    pub async fn thinking_levels(&self, session_id: &str) -> Result<Value, ClientError> {
        self.request_capability(
            "thinking_levels",
            "get_available_thinking_levels",
            session_id,
            Value::Null,
        )
        .await
    }

    pub async fn tools(&self, session_id: &str) -> Result<Value, ClientError> {
        self.request_capability("tools", "get_available_tools", session_id, Value::Null)
            .await
    }

    pub async fn set_active_tools(
        &self,
        session_id: &str,
        tools: Vec<String>,
    ) -> Result<Value, ClientError> {
        self.request_capability(
            "tools",
            "set_active_tools",
            session_id,
            json!({"tools": tools}),
        )
        .await
    }

    pub async fn stats(&self, session_id: &str) -> Result<Value, ClientError> {
        self.request_capability("history", "get_stats", session_id, Value::Null)
            .await
    }

    pub async fn resume_pending(&self, session_id: &str) -> Result<Value, ClientError> {
        self.request_capability("history", "resume_pending", session_id, Value::Null)
            .await
    }

    pub async fn compact(
        &self,
        session_id: &str,
        custom_instructions: Option<&str>,
    ) -> Result<Value, ClientError> {
        self.request_capability(
            "history",
            "compact",
            session_id,
            json!({"customInstructions": custom_instructions}),
        )
        .await
    }

    pub async fn navigate(
        &self,
        session_id: &str,
        target_id: Option<&str>,
    ) -> Result<Value, ClientError> {
        self.request_capability(
            "history",
            "navigate",
            session_id,
            json!({"targetId": target_id}),
        )
        .await
    }

    pub async fn append_entry(
        &self,
        session_id: &str,
        custom_type: &str,
        data: Value,
    ) -> Result<Value, ClientError> {
        self.request_capability(
            "history",
            "append_entry",
            session_id,
            json!({"customType": custom_type, "data": data}),
        )
        .await
    }

    pub async fn execute_command(
        &self,
        session_id: &str,
        command: &str,
        args: Option<&str>,
    ) -> Result<Value, ClientError> {
        let mut payload = json!({"command": command});
        if let Some(args) = args {
            payload["args"] = json!(args);
        }
        self.request(session_id, "execute_command", payload).await
    }

    pub async fn dialog_response(
        &self,
        session_id: &str,
        request_id: &str,
        answer: Value,
    ) -> Result<Value, ClientError> {
        self.request_capability(
            "dialogs",
            "ui_dialog_response",
            session_id,
            json!({"requestId": request_id, "answer": answer}),
        )
        .await
    }

    pub async fn cancel_dialog(
        &self,
        session_id: &str,
        request_id: &str,
    ) -> Result<Value, ClientError> {
        self.request_capability(
            "dialogs",
            "ui_dialog_response",
            session_id,
            json!({"requestId": request_id, "cancelled": true}),
        )
        .await
    }

    pub async fn commands(&self, session_id: &str) -> Result<Value, ClientError> {
        self.request(session_id, "get_commands", Value::Null).await
    }

    pub async fn auth_status(&self, session_id: &str) -> Result<Value, ClientError> {
        self.request_capability(
            "provider_management",
            "get_auth_status",
            session_id,
            Value::Null,
        )
        .await
    }

    pub async fn set_api_key(
        &self,
        session_id: &str,
        provider: &str,
        api_key: &str,
    ) -> Result<Value, ClientError> {
        self.request_capability(
            "provider_management",
            "set_api_key",
            session_id,
            json!({"provider": provider, "apiKey": api_key}),
        )
        .await
    }

    pub async fn subscribe(
        &self,
        session_id: &str,
        after: u64,
    ) -> Result<EventStream, ClientError> {
        self.require("streaming")?;
        self.backend.subscribe(session_id, after).await
    }
}

#[derive(Clone)]
pub struct AgentSession {
    client: AgentClient,
    id: String,
}

impl AgentSession {
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    pub async fn prompt(&self, message: &str) -> Result<Value, ClientError> {
        self.client.prompt(&self.id, message).await
    }

    pub async fn abort(&self) -> Result<Value, ClientError> {
        self.client.abort(&self.id).await
    }

    pub async fn state(&self) -> Result<Value, ClientError> {
        self.client.state(&self.id).await
    }

    pub async fn messages(
        &self,
        limit: Option<u64>,
        before_id: Option<&str>,
    ) -> Result<Value, ClientError> {
        self.client.messages(&self.id, limit, before_id).await
    }

    pub async fn history(
        &self,
        limit: Option<u64>,
        after_seq: Option<u64>,
        before_seq: Option<u64>,
    ) -> Result<Value, ClientError> {
        self.client
            .history(&self.id, limit, after_seq, before_seq)
            .await
    }

    pub async fn queue(&self, include_completed: bool) -> Result<Value, ClientError> {
        self.client.queue(&self.id, include_completed).await
    }

    pub async fn models(&self) -> Result<Value, ClientError> {
        self.client.models(&self.id).await
    }

    pub async fn thinking_levels(&self) -> Result<Value, ClientError> {
        self.client.thinking_levels(&self.id).await
    }

    pub async fn tools(&self) -> Result<Value, ClientError> {
        self.client.tools(&self.id).await
    }

    pub async fn set_active_tools(&self, tools: Vec<String>) -> Result<Value, ClientError> {
        self.client.set_active_tools(&self.id, tools).await
    }

    pub async fn stats(&self) -> Result<Value, ClientError> {
        self.client.stats(&self.id).await
    }

    pub async fn set_config_option(
        &self,
        config_id: &str,
        value: Value,
    ) -> Result<Value, ClientError> {
        self.client
            .set_config_option(&self.id, config_id, value)
            .await
    }

    pub async fn subscribe(&self, after: u64) -> Result<EventStream, ClientError> {
        self.client.subscribe(&self.id, after).await
    }

    pub async fn connect(&self) -> Result<AgentSessionConnection, ClientError> {
        let stream = self.subscribe(0).await?;
        Ok(AgentSessionConnection {
            session_id: self.id.clone(),
            projection: RunProjection::new(self.id.clone()),
            stream,
            identity: MessageIdentity::default(),
            messages: Vec::new(),
            pending_dialogs: Vec::new(),
        })
    }
}

pub enum AgentConnection {
    Http(SessionConnection),
    Native(AgentSessionConnection),
}

impl AgentConnection {
    #[must_use]
    pub fn projection(&self) -> &RunProjection {
        match self {
            Self::Http(connection) => connection.projection(),
            Self::Native(connection) => connection.projection(),
        }
    }

    #[must_use]
    pub fn messages(&self) -> &[Value] {
        match self {
            Self::Http(connection) => connection.messages(),
            Self::Native(connection) => connection.messages(),
        }
    }

    #[must_use]
    pub fn pending_dialogs(&self) -> &[PendingDialog] {
        match self {
            Self::Http(connection) => connection.pending_dialogs(),
            Self::Native(connection) => connection.pending_dialogs(),
        }
    }

    pub async fn next(&mut self) -> Result<Option<SessionEvent>, ClientError> {
        match self {
            Self::Http(connection) => connection.next().await,
            Self::Native(connection) => connection.next().await,
        }
    }
}

pub struct AgentSessionConnection {
    session_id: String,
    projection: RunProjection,
    stream: EventStream,
    identity: MessageIdentity,
    messages: Vec<Value>,
    pending_dialogs: Vec<PendingDialog>,
}

impl AgentSessionConnection {
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    #[must_use]
    pub fn projection(&self) -> &RunProjection {
        &self.projection
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
        let Some(result) = self.stream.next().await else {
            return Ok(None);
        };
        let mut wire = result?;
        wire.data = self.identity.annotate(&wire.data, wire.seq);
        if let Some(message) = wire.data.get("message")
            && matches!(
                wire.event_type.as_str(),
                "message_start" | "message_update" | "message_end"
            )
            && message.is_object()
        {
            self.messages.push(message.clone());
        }
        if wire.event_type == "ui_dialog_request" {
            if let Some(dialog) = crate::dialog_from_wire(&wire)?
                && !self
                    .pending_dialogs
                    .iter()
                    .any(|item| item.request_id == dialog.request_id)
            {
                self.pending_dialogs.push(dialog);
            }
        } else if wire.event_type == "ui_dialog_closed"
            && let Some(request_id) = wire.data.get("requestId").and_then(Value::as_str)
        {
            self.pending_dialogs
                .retain(|item| item.request_id != request_id);
        }
        let applied = self.projection.apply(wire.clone().into_core_event()?);
        Ok(Some(SessionEvent { wire, applied }))
    }
}

impl WorkClient {
    #[must_use]
    pub fn as_agent_client(&self) -> AgentClient {
        AgentClient::from_transport(self.clone())
    }

    /// Describes the currently selected protocol without exposing its private
    /// HTTP client or future native process handle.
    #[must_use]
    pub fn descriptor(&self) -> AgentDescriptor {
        AgentDescriptor {
            identity: TransportIdentity {
                protocol: "rpi".into(),
                version: None,
                transport: "http-sse".into(),
                agent_name: Some("rpi-http".into()),
            },
            capabilities: CapabilitySet::http_default(),
        }
    }
}

#[async_trait(?Send)]
impl AgentTransport for WorkClient {
    fn descriptor(&self) -> AgentDescriptor {
        self.descriptor()
    }

    async fn open_session(&self, _cwd: &str) -> Result<String, ClientError> {
        self.create_session(None).await
    }

    async fn load_session(&self, session_id: &str, _cwd: &str) -> Result<Value, ClientError> {
        self.get_session(session_id).await?;
        Ok(json!({"sessionId": session_id}))
    }

    async fn request(
        &self,
        session_id: &str,
        command: &str,
        payload: Value,
    ) -> Result<Value, ClientError> {
        self.command_data(session_id, command, payload).await
    }

    async fn subscribe(&self, session_id: &str, after: u64) -> Result<EventStream, ClientError> {
        WorkClient::subscribe(self, session_id, after).await
    }
}

#[must_use]
pub fn unsupported(operation: &str, protocol: &str) -> ClientError {
    ClientError::Unsupported {
        operation: operation.into(),
        protocol: protocol.into(),
    }
}

pub fn wire_event(
    id: Option<String>,
    seq: Option<u64>,
    event_type: impl Into<String>,
    data: Value,
) -> WireEvent {
    WireEvent {
        id,
        seq,
        event_type: event_type.into(),
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_work_client_exposes_full_workspace_descriptor() {
        let client = WorkClient::new("http://127.0.0.1:3030").expect("url");
        let descriptor = AgentTransport::descriptor(&client);
        assert_eq!(descriptor.identity.protocol, "rpi");
        assert_eq!(descriptor.identity.transport, "http-sse");
        assert!(descriptor.capabilities.projects);
        assert!(descriptor.capabilities.git_diff);
        assert!(descriptor.capabilities.session_load);
    }

    #[test]
    fn facade_capability_checks_return_unsupported_without_dispatching() {
        let http = WorkClient::new("http://127.0.0.1:3030").expect("url");
        let client = AgentClient::from_transport(http);
        assert!(client.supports("projects"));
        assert!(!client.supports("native-only"));
        let error = client.require("native-only").expect_err("unsupported");
        assert!(matches!(error, ClientError::Unsupported { .. }));
    }

    #[test]
    fn unsupported_operation_keeps_protocol_boundary_explicit() {
        let error = unsupported("projects", "acp");
        assert_eq!(
            error.to_string(),
            "operation `projects` is unsupported by acp"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn native_write_interrupts_backpressure_and_lock_wait_on_eof_or_deadline() {
        use tokio::{
            io::duplex,
            sync::{Mutex, watch},
            time::{Duration, Instant, timeout},
        };
        for locked in [false, true] {
            for stopped in [false, true] {
                let (writer, _unread_pipe) = duplex(1);
                let stdin = Mutex::new(writer);
                let held_lock = if locked {
                    Some(stdin.lock().await)
                } else {
                    None
                };
                let (terminal, receiver) = watch::channel(None);
                let deadline =
                    Instant::now() + Duration::from_millis(if stopped { 1000 } else { 30 });
                let mut write = std::pin::pin!(write_native_frame(
                    &stdin,
                    b"blocked frame",
                    receiver,
                    deadline,
                    || {},
                ));
                tokio::select! {
                    result = &mut write => panic!("write should be blocked: {result:?}"),
                    () = tokio::time::sleep(Duration::from_millis(5)) => {}
                }
                if stopped {
                    terminal.send_replace(Some("fixture EOF".into()));
                }
                let result = timeout(Duration::from_secs(1), write).await.unwrap();
                if stopped {
                    assert!(matches!(result, Err(ClientError::ChildExited(_))));
                } else {
                    assert!(matches!(result, Err(ClientError::RequestTimeout(_))));
                }
                drop(held_lock);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn incomplete_frame_poisons_queued_writer_before_unlocking_stdin() {
        use tokio::{
            io::{AsyncReadExt, duplex},
            sync::{Mutex, watch},
            time::{Duration, Instant},
        };
        let (writer, mut reader) = duplex(1);
        let stdin = Arc::new(Mutex::new(writer));
        let (terminal, receiver) = watch::channel(None);
        let deadline = Instant::now() + Duration::from_secs(1);
        let observed_lock = Arc::clone(&stdin);
        let poison = terminal.clone();
        let mut first = Box::pin(write_native_frame(
            &stdin,
            b"partial",
            receiver.clone(),
            deadline,
            move || {
                assert!(
                    observed_lock.try_lock().is_err(),
                    "poison must run while stdin is still locked"
                );
                poison.send_replace(Some("incomplete frame".into()));
            },
        ));
        assert!(futures_util::poll!(&mut first).is_pending());
        let mut queued = Box::pin(write_native_frame(
            &stdin,
            b"next frame",
            receiver,
            deadline,
            || {},
        ));
        assert!(futures_util::poll!(&mut queued).is_pending());
        drop(first);
        assert!(matches!(queued.await, Err(ClientError::ChildExited(_))));
        let mut byte = [0];
        reader.read_exact(&mut byte).await.unwrap();
        assert_eq!(byte, [b'p']);
        // The queued writer must not have appended any bytes.
        assert!(futures_util::poll!(Box::pin(reader.read(&mut byte))).is_pending());
        assert!(terminal.borrow().is_some());
    }

    #[test]
    fn event_helper_preserves_native_wire_identity() {
        let event = wire_event(
            Some("event-1".into()),
            Some(7),
            "agent_settled",
            serde_json::json!({"type":"agent_settled"}),
        );
        assert_eq!(event.id.as_deref(), Some("event-1"));
        assert_eq!(event.seq, Some(7));
        assert_eq!(event.event_type, "agent_settled");
    }
}
