//! Native RPC workspace and HTTP/WebSocket clients for RPI Work.
#![allow(
    clippy::cast_lossless,
    clippy::ignored_unit_patterns,
    clippy::manual_let_else,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::needless_pass_by_value,
    clippy::too_many_lines
)]
//!
//! `WorkspaceClient` selects the desktop's in-process native workspace or an
//! explicit remote HTTP client. Agent execution uses native RPC; local project,
//! file and Git operations reuse the shared Rust core. Both event sources feed
//! the same `work-core` projection without turning request acceptance into run
//! completion.

use futures_core::Stream;
use futures_util::StreamExt;
use reqwest::{StatusCode, Url};
use serde::Deserialize;
use serde_json::{Value, json};
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use thiserror::Error;
use work_core::api::events::WireEventProjection as _;
use work_core::api::events::{
    content_text, has_image_content, has_tool_call_content, settlement, usage,
};
use work_core::{ApplyResult, Event, EventKind, RunProjection};
#[cfg(test)]
use work_core::{ConnectionState, OutputMode, SettlementOutcome};

mod components;
mod desktop_workspace;
mod extensions;
pub use components::download_component;
mod message_page;
mod message_queries;
mod node_api;
mod node_probe;
mod prompt_content;
mod providers;
#[cfg(not(target_arch = "wasm32"))]
mod realtime_client;
mod resources;
mod session_access;
mod session_configuration;
mod tool_program;
#[cfg(target_arch = "wasm32")]
mod wasm_realtime_client;
pub use message_page::{MESSAGE_PAGE_SIZE, MessagePage, MessagePageInfo};
pub use message_queries::{
    MessageSummary, MessageSummaryPage, MessageSummaryQuery, MessageWindowPage, MessageWindowQuery,
};
pub use node_probe::NodeProbe;
pub use work_core::api::{SessionAccess, SessionConfiguration};
#[cfg(not(target_arch = "wasm32"))]
mod native_runtime;
#[cfg(not(target_arch = "wasm32"))]
mod native_session;
#[cfg(not(target_arch = "wasm32"))]
pub use native_session::RememberedSession;
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

async fn realtime_command<T: for<'de> Deserialize<'de>>(
    client: &WorkClient,
    session_id: &str,
    payload: Value,
) -> Result<HostResponse<T>, ClientError> {
    #[cfg(target_arch = "wasm32")]
    {
        wasm_realtime_client::command(client, session_id, payload).await
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        realtime_client::command(client, session_id, payload).await
    }
}

async fn realtime_mutation(
    client: &WorkClient,
    session_id: &str,
    kind: work_core::api::realtime::RealtimeOperationKind,
    payload: Value,
) -> Result<Value, ClientError> {
    #[cfg(target_arch = "wasm32")]
    {
        wasm_realtime_client::mutation(client, session_id, kind, payload).await
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        realtime_client::mutation(client, session_id, kind, payload).await
    }
}

async fn realtime_subscribe(
    client: &WorkClient,
    session_id: &str,
    after: u64,
) -> Result<EventStream, ClientError> {
    #[cfg(target_arch = "wasm32")]
    {
        wasm_realtime_client::subscribe(client, session_id, after).await
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        realtime_client::subscribe(client, session_id, after).await
    }
}

pub mod workspace;
pub use workspace::WorkspaceClient;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
mod local_workspace;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
mod local_workspace_adapter;
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
    #[error("invalid protocol data: {0}")]
    Protocol(String),
    #[error("remembered session is invalid or expired")]
    InvalidRememberedSession,
    #[error("node did not confirm authentication")]
    AuthenticationNotConfirmed,
    #[error("QR authorization request failed")]
    QrAuthorizationFailed,
    #[error("operation `{operation}` is unsupported by {protocol}")]
    Unsupported { operation: String, protocol: String },
    #[error("RPC transport error: {0}")]
    Rpc(String),
    #[error("WebSocket transport handshake failed (HTTP status: {status:?})")]
    WebSocket { status: Option<u16> },
    #[error("authenticated node does not advertise realtime")]
    RealtimeNotAdvertised,
    #[error("operation outcome is unknown; query the returned operation reference before retrying")]
    OperationOutcomeUnknown {
        operation: work_core::api::realtime::OperationRef,
    },
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

impl From<work_core::api::AcpPermissionError> for ClientError {
    fn from(error: work_core::api::AcpPermissionError) -> Self {
        Self::Acp(error.to_string())
    }
}

impl From<work_core::api::EventDecodeError> for ClientError {
    fn from(error: work_core::api::EventDecodeError) -> Self {
        Self::Protocol(error.to_string())
    }
}

// Never propagate a response body that may echo a QR credential.
// map_err consumes the error; the source must not escape this boundary.
#[allow(clippy::needless_pass_by_value)]
fn qr_error(error: ClientError) -> ClientError {
    match error {
        ClientError::Api { status, .. } => ClientError::Api {
            status,
            message: "QR authorization failed".into(),
        },
        // Decode errors can quote a server-provided string too. Do not retain
        // their source chain (or a request URL) for these credential endpoints.
        _ => ClientError::QrAuthorizationFailed,
    }
}

pub use work_core::api::{
    AgentBackendOption, AuthState, AuthStatus, AuthStatusProvider, Capabilities, ChangedFile,
    CompactionEntry, Diff, FileContent, FileEntry, FileListing, GitStatus, Health, History,
    HostResponse, ModelDescriptor, PROJECT_IMAGE_MAX_BYTES, PendingDialog, Project,
    ProjectCreation, ProjectFolder, ProjectFolders, ProjectImage, QrAuthorization,
    QueueCancellation, QueueItem, QueueMutation, QueueSnapshot, RuntimeState, SessionAgentBinding,
    SessionBranch, SessionMutation, SessionPreferences, SessionSummary, Snapshot, SnapshotEvent,
    ToolCatalog, UiDialogOption, UiDialogPayload, WireEvent,
};

#[derive(Debug, Deserialize)]
struct AuthAck {
    authenticated: bool,
}

/// Convert wire events while preserving the existing client error category.
pub fn normalize_event(event_type: &str, data: &Value) -> Result<EventKind, ClientError> {
    work_core::api::events::normalize_event(event_type, data).map_err(ClientError::from)
}

use work_core::api::payload_with_type;

#[cfg(not(target_arch = "wasm32"))]
fn build_http_client(
    jar: std::sync::Arc<reqwest::cookie::Jar>,
) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .cookie_provider(jar)
        // A scanned authorization ticket is bound to one endpoint. Never
        // forward it (or a password) through an HTTP redirect.
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

#[cfg(target_arch = "wasm32")]
fn build_http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder().build()
}

fn reported_active_tools(state: &Value) -> Result<Vec<String>, ClientError> {
    state
        .get("activeTools")
        .and_then(Value::as_array)
        .ok_or_else(|| ClientError::Protocol("get_state omitted activeTools".into()))?
        .iter()
        .map(|tool| {
            tool.as_str().map(str::to_owned).ok_or_else(|| {
                ClientError::Protocol("get_state returned an invalid activeTools entry".into())
            })
        })
        .collect()
}

#[derive(Clone)]
pub struct WorkClient {
    http: reqwest::Client,
    base_url: Url,
    bearer_token: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    control_capability: Option<reqwest::header::HeaderValue>,
    #[cfg(not(target_arch = "wasm32"))]
    cookie_jar: std::sync::Arc<reqwest::cookie::Jar>,
    realtime_epoch: Arc<AtomicU64>,
    realtime_ids: Arc<AtomicU64>,
    realtime_client_id: Arc<String>,
    #[cfg(not(target_arch = "wasm32"))]
    realtime: Arc<realtime_client::Hub>,
    #[cfg(target_arch = "wasm32")]
    realtime: Arc<wasm_realtime_client::Hub>,
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
        #[cfg(not(target_arch = "wasm32"))]
        let cookie_jar = std::sync::Arc::new(reqwest::cookie::Jar::default());
        Ok(Self {
            #[cfg(not(target_arch = "wasm32"))]
            http: build_http_client(cookie_jar.clone())?,
            #[cfg(target_arch = "wasm32")]
            http: build_http_client()?,
            base_url,
            bearer_token,
            #[cfg(not(target_arch = "wasm32"))]
            control_capability: None,
            #[cfg(not(target_arch = "wasm32"))]
            cookie_jar,
            realtime_epoch: Arc::new(AtomicU64::new(0)),
            realtime_ids: Arc::new(AtomicU64::new(0)),
            realtime_client_id: Arc::new(format!(
                "work-client-{}",
                NEXT_REALTIME_CLIENT.fetch_add(1, Ordering::Relaxed)
            )),
            #[cfg(not(target_arch = "wasm32"))]
            realtime: Arc::new(realtime_client::Hub::default()),
            #[cfg(target_arch = "wasm32")]
            realtime: Arc::new(wasm_realtime_client::Hub::default()),
        })
    }

    pub async fn health(&self) -> Result<Health, ClientError> {
        self.get("api/v1/health").await
    }
    pub async fn node_identity(&self) -> Result<String, ClientError> {
        let health: Value = self.get("api/v1/health").await?;
        health
            .get("nodeId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 256)
            .map(str::to_owned)
            .ok_or_else(|| ClientError::ProtocolMismatch("Node identity missing".into()))
    }
    /// Construct only for a child process supervised by this application. The
    /// capability is never accepted by public node listeners or sent to proxies.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn local_control(address: &str, capability: &str) -> Result<Self, ClientError> {
        let mut client = Self::new(address)?;
        let u = &client.base_url;
        if u.scheme() != "http"
            || u.host_str()
                .and_then(|h| h.parse::<std::net::IpAddr>().ok())
                .is_none_or(|ip| !ip.is_loopback())
            || !u.username().is_empty()
            || u.password().is_some()
            || u.query().is_some()
            || u.fragment().is_some()
            || u.path() != "/"
            || capability.len() < 32
        {
            return Err(ClientError::ProtocolMismatch(
                "Invalid private node entrance".into(),
            ));
        }
        let mut value = reqwest::header::HeaderValue::from_str(capability)
            .map_err(|_| ClientError::ProtocolMismatch("Invalid private capability".into()))?;
        value.set_sensitive(true);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-rpi-node-control", value.clone());
        client.http = reqwest::Client::builder()
            .cookie_provider(client.cookie_jar.clone())
            .default_headers(headers)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        client.control_capability = Some(value);
        Ok(client)
    }
    pub fn is_local_control(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.control_capability.is_some()
        }
        #[cfg(target_arch = "wasm32")]
        {
            false
        }
    }

    pub async fn auth(&self) -> Result<AuthState, ClientError> {
        self.get("api/v1/auth").await
    }

    pub async fn login(&self, password: &str) -> Result<AuthState, ClientError> {
        let _auth_change = self.realtime_auth_change();
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

    /// Read public endpoint identity without sharing an authenticated transport.
    /// Callers probing an advertised QR address must construct a fresh client.
    pub async fn qr_authorization_node(&self) -> Result<Option<String>, ClientError> {
        let health: Value = self.get("api/v1/health").await?;
        Ok((health["sharingEnabled"].as_bool() == Some(true))
            .then(|| {
                health["nodeId"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned)
            })
            .flatten())
    }

    pub async fn issue_qr_authorization(&self) -> Result<QrAuthorization, ClientError> {
        self.post("api/v1/auth/qr", json!({}))
            .await
            .map_err(qr_error)
    }

    pub async fn redeem_qr_authorization(&self, ticket: &str) -> Result<AuthState, ClientError> {
        let _auth_change = self.realtime_auth_change();
        let ack: AuthAck = self
            .post("api/v1/auth/qr/redeem", json!({"ticket": ticket}))
            .await
            .map_err(qr_error)?;
        let _ = ack.authenticated;
        let state = self.auth().await.map_err(qr_error)?;
        if !state.authenticated {
            return Err(ClientError::AuthenticationNotConfirmed);
        }
        Ok(state)
    }

    pub async fn logout(&self) -> Result<AuthState, ClientError> {
        let _auth_change = self.realtime_auth_change();
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

    /// Fetch a bounded raster using the shared authenticated API client.
    pub async fn image(&self, project_id: &str, path: &str) -> Result<ProjectImage, ClientError> {
        let mut url = self.url("api/v1/projects/")?;
        url.path_segments_mut()
            .map_err(|()| ClientError::ProtocolMismatch("Invalid HTTP API URL".into()))?
            .pop_if_empty()
            .push(project_id)
            .push("image");
        url.query_pairs_mut().append_pair("path", path);
        let response = authenticated(
            self.http.get(url).header("x-rpi-web", "1"),
            self.bearer_token.as_deref(),
        )
        .send()
        .await?;
        let status = response.status();
        if !status.is_success() {
            // Do not consume an unbounded error body or expose server diagnostics.
            return Err(ClientError::Api {
                status,
                message: "project image read failed".into(),
            });
        }
        let reject = |status, message: &str| ClientError::Api {
            status,
            message: message.into(),
        };
        if response
            .content_length()
            .is_some_and(|len| len > PROJECT_IMAGE_MAX_BYTES as u64)
            || response
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|h| h.to_str().ok())
                .and_then(|h| h.parse::<u64>().ok())
                .is_some_and(|len| len > PROJECT_IMAGE_MAX_BYTES as u64)
        {
            return Err(reject(
                StatusCode::PAYLOAD_TOO_LARGE,
                "images are limited to 20 MiB",
            ));
        }
        let media_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if !matches!(
            media_type.as_str(),
            "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/bmp"
        ) {
            return Err(reject(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported image format",
            ));
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if chunk.len() > PROJECT_IMAGE_MAX_BYTES - bytes.len() {
                return Err(reject(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "images are limited to 20 MiB",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let matches = match media_type.as_str() {
            "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "image/jpeg" => bytes.starts_with(b"\xff\xd8\xff"),
            "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
            "image/webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
            "image/bmp" => bytes.starts_with(b"BM"),
            _ => false,
        };
        if !matches {
            return Err(reject(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "image signature does not match media type",
            ));
        }
        Ok(ProjectImage { bytes, media_type })
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
        self.set_model_with_provider(session_id, model_id, "").await
    }

    /// Disambiguate a catalog id reported by several providers. An empty
    /// provider keeps the legacy id-only request.
    pub async fn set_model_with_provider(
        &self,
        session_id: &str,
        model_id: &str,
        provider: &str,
    ) -> Result<Value, ClientError> {
        let mut payload = json!({ "modelId": model_id });
        if !provider.trim().is_empty() {
            payload["provider"] = json!(provider);
        }
        self.command_data(session_id, "set_model", payload).await
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
        let payload = json!({ "requestId": request_id, "answer": value });
        if let Some(value) = self
            .try_realtime_mutation(
                session_id,
                work_core::api::realtime::RealtimeOperationKind::DialogResponse,
                &payload,
            )
            .await?
        {
            return Ok(value);
        }
        self.post(&format!("api/v1/sessions/{session_id}/dialogs"), payload)
            .await
    }

    pub async fn cancel_dialog(
        &self,
        session_id: &str,
        request_id: &str,
    ) -> Result<Value, ClientError> {
        let payload = json!({ "requestId": request_id, "cancelled": true });
        if let Some(value) = self
            .try_realtime_mutation(
                session_id,
                work_core::api::realtime::RealtimeOperationKind::CancelDialog,
                &payload,
            )
            .await?
        {
            return Ok(value);
        }
        self.post(&format!("api/v1/sessions/{session_id}/dialogs"), payload)
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

    pub async fn snapshot_with_limit(
        &self,
        session_id: &str,
        limit: u64,
    ) -> Result<Snapshot, ClientError> {
        let mut url = self.url(&format!("api/v1/sessions/{session_id}/snapshot"))?;
        url.query_pairs_mut()
            .append_pair("limit", &limit.to_string());
        self.get_url(url).await
    }

    fn fence_realtime(&self) {
        let epoch = self.realtime_epoch.fetch_add(1, Ordering::AcqRel) + 1;
        self.realtime.invalidate(epoch);
    }

    pub(crate) fn realtime_epoch(&self) -> u64 {
        self.realtime_epoch.load(Ordering::Acquire)
    }

    pub async fn command<T: for<'de> Deserialize<'de>>(
        &self,
        session_id: &str,
        payload: Value,
    ) -> Result<HostResponse<T>, ClientError> {
        realtime_command::<T>(self, session_id, payload).await
    }

    async fn try_realtime_mutation(
        &self,
        session_id: &str,
        kind: work_core::api::realtime::RealtimeOperationKind,
        payload: &Value,
    ) -> Result<Option<Value>, ClientError> {
        Ok(Some(
            realtime_mutation(self, session_id, kind, payload.clone()).await?,
        ))
    }

    /// Queries a receipt without reserving or executing another operation.
    pub async fn operation_status(
        &self,
        operation: &work_core::api::realtime::OperationRef,
    ) -> Result<work_core::api::realtime::OperationStatus, ClientError> {
        operation.validate().map_err(|_| ClientError::Api {
            status: StatusCode::BAD_REQUEST,
            message: "invalid operation reference".into(),
        })?;
        let mut url = self.url("api/v1/realtime/operations/")?;
        url.path_segments_mut()
            .map_err(|()| ClientError::Api {
                status: StatusCode::BAD_REQUEST,
                message: "invalid operation endpoint".into(),
            })?
            .pop_if_empty()
            .push(&operation.owner_epoch)
            .push(&operation.operation_id);
        self.get_url(url).await
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
            .ok_or_else(|| ClientError::Protocol(format!("{command} returned no data")))
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
        realtime_subscribe(self, session_id, after).await
    }

    /// Open a session, restore its snapshot, then subscribe after the snapshot cursor.
    pub async fn connect_session(
        &self,
        session_id: impl Into<String>,
    ) -> Result<SessionConnection, ClientError> {
        WorkspaceClient::Remote(self.clone())
            .connect_session(session_id)
            .await
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
    snapshot_limit: bool,
    message_page_info: MessagePageInfo,
    history_len: usize,
    messages: Vec<Value>,
    pending_dialogs: Vec<PendingDialog>,
    reconnect_attempt: u32,
    reconnecting: bool,
    connection_epoch: u64,
    identity: MessageIdentity,
}

// Private provenance: a node ID that happens to start with `anonymous-` is
// still authoritative. Only IDs allocated by this client get this marker.
static NEXT_REALTIME_CLIENT: AtomicU64 = AtomicU64::new(1);

const CLIENT_ANONYMOUS_ID: &str = "__rpiClientAnonymousId";
const CLIENT_CANONICAL_ID: &str = "__rpiClientCanonicalId";
// A lifecycle anchor, not evidence that any particular historical body is current.
const CLIENT_STREAM_STARTED: &str = "__rpiClientStreamStarted";

/// Tracks the current message when a host omits its entry ID.
#[derive(Default)]
struct MessageIdentity {
    current: Option<String>,
    current_explicit: Option<String>,
    current_role: Option<String>,
    current_started: bool,
    counter: u64,
}

impl MessageIdentity {
    fn clear_current(&mut self) {
        self.current = None;
        self.current_explicit = None;
        self.current_role = None;
        self.current_started = false;
    }

    fn annotate(&mut self, data: &Value, seq: Option<u64>) -> Value {
        let mut data = data.clone();
        let kind = data
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if matches!(
            kind.as_str(),
            "operation_accepted" | "agent_start" | "agent_end" | "agent_settled"
        ) {
            self.clear_current();
        }
        if !matches!(
            kind.as_str(),
            "message_start" | "message_update" | "message_end"
        ) {
            return data;
        }
        // Native RPC places the durable entry identity on the event envelope.
        // Its default compact delta omits `message` entirely. Normalize only an
        // internal identity shell, never a synthetic body or an event ID.
        let envelope_id = ["messageId", "entryId"]
            .iter()
            .find_map(|key| data.get(*key)?.as_str().filter(|id| !id.trim().is_empty()))
            .map(str::to_owned);
        if kind == "message_update" && data.get("message").is_none() {
            // Compact message_update is assistant-only in the RPC contract;
            // it must not inherit an unfinished user segment's role.
            data["message"] = json!({"role":"assistant"});
        }
        let Some(message) = data.get_mut("message").and_then(Value::as_object_mut) else {
            return data;
        };
        let role = message
            .get("role")
            .or_else(|| message.get("kind"))
            .and_then(Value::as_str);
        if role.is_some_and(|role| !matches!(role, "assistant" | "user")) {
            return data;
        }
        let role = role
            .or_else(|| {
                (kind != "message_start")
                    .then_some(self.current_role.as_deref())
                    .flatten()
            })
            .unwrap_or("assistant")
            .to_owned();
        // Downstream live merging and event normalization must agree with the
        // identity tracker even when the host omits role on updates/end.
        message.entry("role").or_insert_with(|| json!(role));
        // User segments must not inherit an unfinished assistant's identity,
        // but their own start/update/end share one identity just like assistants.
        if self.current_role.as_ref().is_some_and(|old| old != &role) {
            self.clear_current();
        }
        let explicit = envelope_id.or_else(|| {
            ["entryId", "messageId", "id"]
                .iter()
                .find_map(|key| {
                    message
                        .get(*key)?
                        .as_str()
                        .filter(|id| !id.trim().is_empty())
                })
                .map(str::to_owned)
        });
        // A changed host ID is a boundary even if message_start was omitted.
        // The first host ID of an anonymous message is a promotion, not a new
        // message. A host may revise timestamps at persistence; lifecycle and
        // authoritative IDs, not mutable timestamps, delimit this open segment.
        let changed_id = explicit
            .as_ref()
            .is_some_and(|id| self.current_explicit.as_ref().is_some_and(|old| old != id));
        if kind == "message_start" || self.current.is_none() || changed_id {
            self.current_started = kind == "message_start";
            self.current_role = Some(role);
            self.current_explicit = explicit.clone();
            self.current = explicit.clone().or_else(|| {
                self.counter += 1;
                Some(format!(
                    "anonymous-{}-{}",
                    seq.unwrap_or_default(),
                    self.counter
                ))
            });
        } else if explicit.is_some() {
            self.current_explicit = explicit;
        }
        message.remove(CLIENT_ANONYMOUS_ID);
        message.remove(CLIENT_CANONICAL_ID);
        message.remove(CLIENT_STREAM_STARTED);
        if self.current_started {
            message.insert(CLIENT_STREAM_STARTED.into(), json!(true));
        }
        if let Some(id) = &self.current {
            message.insert("entryId".into(), json!(id));
            if self.current != self.current_explicit {
                message.insert(CLIENT_ANONYMOUS_ID.into(), json!(true));
                if let Some(canonical) = &self.current_explicit {
                    message.insert(CLIENT_CANONICAL_ID.into(), json!(canonical));
                }
            }
        }
        if kind == "message_end" {
            self.clear_current();
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
    pub fn message_page_info(&self) -> &MessagePageInfo {
        &self.message_page_info
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
        let applied =
            apply_live_message_event(&mut self.messages, &mut self.projection, normalized)?;
        if matches!(applied, ApplyResult::RebuiltForResync) {
            self.reconnecting = true;
        }
        if self.snapshot_limit
            && matches!(wire.event_type.as_str(), "agent_start" | "agent_settled")
        {
            // Native RPC does not emit the user's persisted message as a live
            // message event. Read it at run start, then reconcile final entry IDs
            // at settlement without resubscribing or advancing the event cursor.
            let snapshot = self
                .client
                .snapshot_with_limit(&self.session_id, MESSAGE_PAGE_SIZE);
            let timeout = reconnect_sleep(Duration::from_secs(5));
            futures_util::pin_mut!(snapshot, timeout);
            if let futures_util::future::Either::Left((Ok(snapshot), _)) =
                futures_util::future::select(snapshot, timeout).await
            {
                self.message_page_info = MessagePageInfo::from_snapshot(&snapshot);
                if wire.event_type == "agent_start" {
                    for message in snapshot
                        .messages
                        .iter()
                        .filter(|message| message_role(message) == Some("user"))
                    {
                        merge_live_message(&mut self.messages, &json!({"message":message}), None);
                    }
                } else {
                    reconcile_messages(
                        &mut self.messages,
                        &mut self.projection,
                        snapshot.messages,
                        snapshot.has_more || snapshot.truncated,
                    );
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
        let (snapshot, stream, snapshot_limit) =
            self.client.snapshot_stream(&self.session_id).await?;
        let pending_dialogs = snapshot_dialogs(&snapshot);
        let (messages, projection) = snapshot_state(&snapshot)?;
        // Publish the new snapshot only once its subscription is ready.
        self.projection = projection;
        self.identity = MessageIdentity::default();
        for item in &snapshot.events {
            self.identity.annotate(&item.event, Some(item.seq));
        }
        self.history_len = messages.len();
        self.message_page_info = MessagePageInfo::from_snapshot(&snapshot);
        self.messages = messages;
        self.pending_dialogs = pending_dialogs;
        self.stream = stream;
        self.snapshot_limit = snapshot_limit;
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
    ["entryId", "messageId", "id"].iter().find_map(|key| {
        message
            .get(*key)?
            .as_str()
            .filter(|id| !id.trim().is_empty())
    })
}

/// Authoritative message identity, including only client-proven promotions.
/// Unresolved client-generated IDs have no canonical identity; callers that
/// index loaded views may fall back to their physical entry ID.
#[must_use]
pub fn canonical_entry_id(message: &Value) -> Option<&str> {
    if message[CLIENT_ANONYMOUS_ID] == true {
        message
            .get(CLIENT_CANONICAL_ID)
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
    } else {
        entry_id(message)
    }
}

/// Host identity wins over weak metadata, regardless of candidate order.
/// Timestamp matching is only safe when an identity is missing, or during
/// canonical reconciliation of a client-generated anonymous identity. Even
/// then, ambiguous candidates must stay separate rather than corrupt a turn.
fn matching_message_index(
    messages: &[Value],
    incoming: &Value,
    reconcile_anonymous: bool,
) -> Option<usize> {
    if let Some(id) = entry_id(incoming) {
        if let Some(index) = messages.iter().position(|item| entry_id(item) == Some(id)) {
            return Some(index);
        }
    }
    if let Some(id) = canonical_entry_id(incoming) {
        if let Some(index) = messages
            .iter()
            .position(|item| canonical_entry_id(item) == Some(id))
        {
            return Some(index);
        }
    }
    let is_tool = |message: &Value| {
        matches!(
            message_role(message),
            Some("tool" | "toolResult" | "tool_result")
        )
    };
    if is_tool(incoming) {
        if let Some(call) = incoming.get("toolCallId").and_then(Value::as_str) {
            if let Some(index) = messages.iter().position(|item| {
                is_tool(item) && item.get("toolCallId").and_then(Value::as_str) == Some(call)
            }) {
                return Some(index);
            }
        }
    }
    let timestamp = incoming.get("timestamp").and_then(Value::as_u64)?;
    let mut candidates = messages.iter().enumerate().filter(|(_, item)| {
        message_role(item) == message_role(incoming)
            && item.get("timestamp").and_then(Value::as_u64) == Some(timestamp)
            && !(canonical_entry_id(item).is_some()
                && canonical_entry_id(incoming).is_some()
                && canonical_entry_id(item) != canonical_entry_id(incoming))
            && (entry_id(item).is_none()
                || entry_id(incoming).is_none()
                || (reconcile_anonymous
                    && (incoming[CLIENT_ANONYMOUS_ID] == true
                        || item[CLIENT_ANONYMOUS_ID] == true)))
    });
    let (index, _) = candidates.next()?;
    candidates.next().is_none().then_some(index)
}

fn reconcile_messages(
    messages: &mut Vec<Value>,
    projection: &mut RunProjection,
    mut canonical: Vec<Value>,
    truncated: bool,
) {
    // Resolve against the original snapshot, not live entries appended below.
    // Weak matches must be unique in both directions: one canonical row must
    // not consume multiple anonymous messages or steal an exact live match.
    let matches: Vec<_> = messages
        .iter()
        .enumerate()
        .map(|(index, live)| {
            matching_message_index(&canonical, live, true).filter(|&candidate| {
                (entry_id(live).is_some() && entry_id(live) == entry_id(&canonical[candidate]))
                    || matching_message_index(messages, &canonical[candidate], true) == Some(index)
            })
        })
        .collect();
    // A bounded snapshot is a suffix. Retain pre-boundary entries *before* it;
    // only missing recent live entries belong after the authoritative suffix.
    let boundary = if truncated {
        matches
            .iter()
            .position(Option::is_some)
            .unwrap_or(messages.len())
    } else {
        0
    };
    let mut prefix = Vec::new();
    for (index, live) in messages.iter().enumerate() {
        let owner = matches[index].map(|candidate| &canonical[candidate]);
        if let Some(owner) = owner {
            if let (Some(old), Some(new)) = (entry_id(live), entry_id(owner)) {
                for block in &mut projection.blocks {
                    let id = match block {
                        work_core::MessageBlock::Text { id, .. }
                        | work_core::MessageBlock::Thinking { id, .. }
                        | work_core::MessageBlock::ToolCall { id, .. }
                        | work_core::MessageBlock::PublicSummary { id, .. } => id,
                    };
                    if id == old {
                        new.clone_into(id);
                    } else if let Some(suffix) = id.strip_prefix(&format!("{old}-text-")) {
                        *id = format!("{new}-text-{suffix}");
                    } else if let Some(suffix) = id.strip_prefix(&format!("{old}-thinking-")) {
                        *id = format!("{new}-thinking-{suffix}");
                    }
                }
            }
        } else if index < boundary {
            prefix.push(live.clone());
        } else {
            // Older nodes may omit a just-finished live message from snapshots.
            canonical.push(live.clone());
        }
    }
    prefix.extend(canonical);
    *messages = prefix;
}

// Retained history owns its projection text, so public content-less deltas
// must update that same row. Preserve structured images/tools and never replace
// a body with an empty metadata-only event.
fn retained_text_delta(
    messages: &[Value],
    data: &Value,
    projection: Option<&RunProjection>,
) -> Option<Value> {
    let message = data.get("message")?;
    let kind = data
        .get("eventType")
        .or_else(|| data.get("assistantMessageEvent")?.get("type"))?
        .as_str()?;
    if kind != "text_delta"
        || message_role(message).is_some_and(|role| role != "assistant")
        || message
            .get("content")
            .is_some_and(|v| !v.is_null() && v != &json!([]) && v != "")
    {
        return None;
    }
    let delta = data.get("delta")?.as_str()?;
    let existing = &messages[matching_message_index(messages, message, false)?];
    let block_index = data
        .get("contentIndex")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let projected = projection.and_then(|projection| {
        [entry_id(message), entry_id(existing)]
            .into_iter()
            .flatten()
            .find_map(|id| {
                let block_id = format!("{id}-text-{block_index}");
                projection.blocks.iter().find_map(|block| match block {
                    work_core::MessageBlock::Text { id, text } if *id == block_id => {
                        Some(text.as_str())
                    }
                    _ => None,
                })
            })
    });
    if let Some(text) = existing.get("content").and_then(Value::as_str) {
        let mut patched = existing.clone();
        for (key, value) in message.as_object()? {
            patched[key] = value.clone();
        }
        let text = projected.unwrap_or_else(|| {
            if message[CLIENT_STREAM_STARTED] == true {
                ""
            } else {
                text
            }
        });
        patched["content"] = json!(format!("{text}{delta}"));
        return Some(patched);
    }
    let mut content = existing.get("content")?.as_array()?.clone();
    let index = data
        .get("contentIndex")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .unwrap_or_else(|| {
            content
                .iter()
                .position(|item| item["type"] == "text")
                .unwrap_or(content.len())
        });
    if let Some(item) = content.get_mut(index).filter(|item| {
        matches!(
            item["type"].as_str(),
            Some("text" | "input_text" | "output_text")
        )
    }) {
        let retained = item.get("text").and_then(Value::as_str).unwrap_or_default();
        let mut text = if index as u64 == block_index {
            projected.unwrap_or_else(|| {
                if message[CLIENT_STREAM_STARTED] == true {
                    ""
                } else {
                    retained
                }
            })
        } else {
            retained
        }
        .to_owned();
        text.push_str(delta);
        item["text"] = json!(text);
    } else {
        content.push(json!({"type":"text","text":delta}));
    }
    let mut patched = existing.clone();
    for (key, value) in message.as_object()? {
        patched[key] = value.clone();
    }
    patched["content"] = json!(content);
    Some(patched)
}

/// A proven identity transfer, not a text/timestamp-based display filter.
#[derive(Default)]
struct LiveMessageMerge {
    owner: Option<String>,
    previous_ids: Vec<String>,
}

fn known_live_owner(messages: &mut Vec<Value>, incoming: &Value) -> LiveMessageMerge {
    let Some(canonical) = canonical_entry_id(incoming) else {
        return LiveMessageMerge::default();
    };
    let role = message_role(incoming);
    let Some(owner) = messages
        .iter()
        .find(|item| message_role(item) == role && entry_id(item) == Some(canonical))
        .and_then(entry_id)
        .map(str::to_owned)
    else {
        return LiveMessageMerge::default();
    };
    let mut previous_ids = Vec::new();
    if let Some(id) = entry_id(incoming).filter(|id| *id != owner) {
        previous_ids.push(id.to_owned());
    }
    // Even a bodyless event can carry a proven promotion. Retire its old
    // client row, but never replace the retained owner's body with no content.
    messages.retain(|item| {
        let duplicate = message_role(item) == role
            && item[CLIENT_ANONYMOUS_ID] == true
            && (entry_id(item) == entry_id(incoming)
                || canonical_entry_id(item) == Some(canonical))
            && entry_id(item) != Some(owner.as_str());
        if duplicate {
            if let Some(id) = entry_id(item) {
                previous_ids.push(id.to_owned());
            }
        }
        !duplicate
    });
    LiveMessageMerge {
        owner: Some(owner),
        previous_ids,
    }
}

fn apply_live_message_event(
    messages: &mut Vec<Value>,
    projection: &mut RunProjection,
    mut wire: WireEvent,
) -> Result<ApplyResult, ClientError> {
    // Boundary/summary events (notably native turn_end) also carry `message`,
    // but do not publish another transcript entry. Only the message lifecycle
    // may merge bodies; still apply every event's phase/tool/control semantics.
    if !matches!(
        wire.event_type.as_str(),
        "message_start" | "message_update" | "message_end"
    ) {
        return Ok(projection.apply(wire.into_core_event()?));
    }
    // An empty start resets only this identity's public historical slots. Keep
    // their real indices; manufacturing slot zero would misaddress compact deltas.
    let reset_slots = if wire.event_type == "message_start"
        && wire.data.get("message").is_some_and(|message| {
            message_role(message) == Some("assistant")
                && message
                    .get("content")
                    .is_none_or(|body| body == &json!([]) || body == "" || body.is_null())
        }) {
        let incoming = &wire.data["message"];
        matching_message_index(messages, incoming, false).and_then(|index| {
            let existing = &messages[index];
            let owner = entry_id(existing)?;
            let normalized = normalize_event("message_start", &json!({"message":existing})).ok()?;
            let slots = match normalized {
                EventKind::TextSnapshots { snapshots, .. } => snapshots
                    .into_iter()
                    .map(|(id, _)| (id, String::new()))
                    .collect(),
                EventKind::TextSnapshot { block_id, .. } => vec![(block_id, String::new())],
                _ => return None,
            };
            Some((owner.to_owned(), slots))
        })
    } else {
        None
    };
    // A retained row must use this stream's current text, not blindly append
    // replayed deltas to a snapshot body that may already include those deltas.
    let mut merge_data = wire.data.clone();
    if let Some(message) = retained_text_delta(messages, &wire.data, Some(projection)) {
        merge_data["message"] = message;
    }
    let merged = merge_live_message(messages, &merge_data, wire.seq);
    let owner = merged.owner;
    if let Some(owner) = owner.as_ref() {
        // A snapshot may already own the canonical row while the restored
        // stream still has an anonymous display ID. Transfer its text blocks
        // before applying the next snapshot, keeping projection indices stable.
        for old in merged.previous_ids {
            for block in &mut projection.blocks {
                match block {
                    work_core::MessageBlock::Text { id, .. } => {
                        if let Some(suffix) = id.strip_prefix(&format!("{old}-text-")) {
                            *id = format!("{owner}-text-{suffix}");
                        }
                    }
                    work_core::MessageBlock::Thinking { id, .. } => {
                        if let Some(suffix) = id.strip_prefix(&format!("{old}-thinking-")) {
                            *id = format!("{owner}-thinking-{suffix}");
                        }
                    }
                    _ => {}
                }
            }
        }
        wire.data["message"]["entryId"] = json!(owner);
    }
    let block_index = wire
        .data
        .get("contentIndex")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let retained_text = owner.as_deref().and_then(|owner| {
        let content = messages
            .iter()
            .find(|item| entry_id(item) == Some(owner))?
            .get("content")?;
        content
            .as_str()
            .or_else(|| {
                let item = content
                    .as_array()?
                    .get(usize::try_from(block_index).ok()?)?;
                matches!(
                    item["type"].as_str(),
                    Some("text" | "input_text" | "output_text")
                )
                .then(|| item.get("text").and_then(Value::as_str))
                .flatten()
            })
            .map(str::to_owned)
    });
    let full_text_body = wire
        .data
        .get("message")
        .and_then(|message| message.get("content"))
        .is_some_and(|content| !content_text(content).is_empty());
    let mut event = wire.into_core_event()?;
    if let Some((message_id, snapshots)) = reset_slots {
        event.kind = EventKind::TextSnapshots {
            message_id,
            snapshots,
            thinking: Vec::new(),
            authoritative: true,
        };
    }
    let retained_block = match &event.kind {
        EventKind::TextDelta { block_id, .. } => Some(block_id.clone()),
        EventKind::TextSnapshot { block_id, .. } if full_text_body => Some(block_id.clone()),
        _ => None,
    };
    if let (Some(block_id), Some(text)) = (retained_block, retained_text) {
        // Keep each structured text part separate and the hidden projection in
        // sync with its retained row. Empty starts still reset the stream, so
        // replay cannot append a delta twice to an already persisted body.
        event.kind = EventKind::TextSnapshot { block_id, text };
    }
    Ok(projection.apply(event))
}

#[allow(clippy::too_many_lines)] // Keep identity selection and its atomic owner transfer together.
fn merge_live_message(
    messages: &mut Vec<Value>,
    data: &Value,
    seq: Option<u64>,
) -> LiveMessageMerge {
    let patched = retained_text_delta(messages, data, None);
    let Some(message) = patched
        .as_ref()
        .or_else(|| data.get("message"))
        .and_then(Value::as_object)
    else {
        return LiveMessageMerge::default();
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
        return LiveMessageMerge::default();
    }
    // During text streaming the assistant message_start/update often has no
    // body; projection owns that partial text until a complete snapshot
    // arrives. Tool-only snapshots still own the call's place in the transcript
    // and its arguments; dropping them makes results appear as unrelated cards
    // and bounded settlement snapshots cannot recover older call rows.
    if role == "assistant"
        && message.get("content").is_none_or(|content| {
            content_text(content).is_empty()
                && !has_image_content(content)
                && !has_tool_call_content(content)
        })
    {
        return known_live_owner(messages, &Value::Object(message.clone()));
    }
    let explicit_id = ["entryId", "messageId", "id"].iter().find_map(|key| {
        message
            .get(*key)?
            .as_str()
            .filter(|id| !id.trim().is_empty())
    });
    let id = explicit_id.map_or_else(
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
        object.remove(CLIENT_STREAM_STARTED);
        object.insert("entryId".into(), json!(id));
        if explicit_id.is_none() {
            object.insert(CLIENT_ANONYMOUS_ID.into(), json!(true));
        }
    }
    let canonical = canonical_entry_id(&incoming).map(str::to_owned);
    // Prefer an already persisted owner when the open anonymous segment gains
    // its real ID. An exact synthetic-ID match must not leave that proven
    // canonical row and the live row side by side.
    let canonical_owner = canonical.as_deref().and_then(|canonical| {
        messages
            .iter()
            .position(|item| message_role(item) == Some(role) && entry_id(item) == Some(canonical))
    });
    let index = canonical_owner.or_else(|| matching_message_index(messages, &incoming, false));
    let owner = if let Some(index) = index {
        // Legacy snapshots can match by metadata without having any host ID.
        // Give that local row the incoming ID rather than assuming one exists.
        let owner = entry_id(&messages[index]).unwrap_or(&id).to_owned();
        if let (Some(existing), Some(incoming)) =
            (messages[index].as_object_mut(), incoming.as_object())
        {
            for (key, value) in incoming {
                if key != "entryId" {
                    existing.insert(key.clone(), value.clone());
                }
            }
            existing.insert("entryId".into(), json!(owner));
            // Do not mark a real host ID as client-generated just because a
            // late anonymous stream has now resolved to this existing owner.
            if existing.get("entryId").and_then(Value::as_str) == canonical.as_deref() {
                existing.remove(CLIENT_ANONYMOUS_ID);
                existing.remove(CLIENT_CANONICAL_ID);
            }
        }
        owner
    } else {
        messages.push(incoming);
        id.clone()
    };
    let mut previous_ids = Vec::new();
    if id != owner {
        previous_ids.push(id.clone());
    }
    if let Some(canonical) = canonical.as_deref() {
        messages.retain(|item| {
            let duplicate = message_role(item) == Some(role)
                && item[CLIENT_ANONYMOUS_ID] == true
                && (canonical_entry_id(item) == Some(canonical)
                    || entry_id(item) == Some(id.as_str()))
                && entry_id(item) != Some(owner.as_str());
            if duplicate {
                if let Some(old) = entry_id(item) {
                    previous_ids.push(old.to_owned());
                }
            }
            !duplicate
        });
    }
    LiveMessageMerge {
        owner: Some(owner),
        previous_ids,
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
    work_core::api::events::dialog_from_wire(wire).map_err(ClientError::from)
}

mod snapshot_replay;

pub fn projection_from_snapshot(snapshot: &Snapshot) -> Result<RunProjection, ClientError> {
    snapshot_state(snapshot).map(|(_, projection)| projection)
}

fn snapshot_state(snapshot: &Snapshot) -> Result<(Vec<Value>, RunProjection), ClientError> {
    let mut projection = RunProjection::new(snapshot.session_id.clone());
    // The snapshot only includes events emitted while it was being read. A
    // silent long-running tool (including a child agent) may have no deltas,
    // so restore the host's boundary state before replaying newer events.
    projection.phase = match snapshot.status.as_deref() {
        Some("running") => work_core::RunPhase::Accepted,
        Some("awaiting_input" | "waiting_for_input") => work_core::RunPhase::AwaitingInput,
        Some("cancelling") => work_core::RunPhase::Cancelling,
        _ => work_core::RunPhase::Ready,
    };
    let mut identity = MessageIdentity::default();
    // Identity consumes the complete sequence, including covered messages, so
    // anonymous IDs stay consistent with the live connection's replay tracker.
    let annotated: Vec<_> = snapshot
        .events
        .iter()
        .map(|item| identity.annotate(&item.event, Some(item.seq)))
        .collect();
    let covered = snapshot_replay::covered_events(snapshot, &annotated);
    // Replay needs the same owner-transfer evidence as live delivery. Keep a
    // private copy: projection restoration must not rewrite snapshot history.
    let mut replay_messages = snapshot.messages.clone();
    let mut body_sources = std::collections::HashSet::new();
    let mut replayed_settlement = false;
    for (index, item) in snapshot.events.iter().enumerate() {
        let mut event = WireEvent {
            id: Some(item.seq.to_string()),
            seq: Some(item.seq),
            event_type: item
                .event
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .into(),
            data: annotated[index].clone(),
        };
        replayed_settlement |= event.event_type == "agent_settled";
        if covered[index] {
            continue;
        }
        if let Some(message) = event.data.get("message") {
            if let Some(id) = entry_id(message) {
                let has_body = message.get("content").is_some_and(|content| {
                    !content_text(content).is_empty()
                        || has_image_content(content)
                        || has_tool_call_content(content)
                });
                if event.event_type == "message_start" || has_body {
                    body_sources.insert(id.to_owned());
                } else if !body_sources.contains(id)
                    && event
                        .data
                        .get("eventType")
                        .or_else(|| event.data.get("assistantMessageEvent")?.get("type"))
                        .and_then(Value::as_str)
                        == Some("text_delta")
                    && let Some(existing) = matching_message_index(&replay_messages, message, false)
                    && let Some(content) = replay_messages[existing].get("content").cloned()
                {
                    // An unanchored event window may already be included in the
                    // persisted body. Without a start/full snapshot, keep that
                    // body instead of appending its deltas a second time.
                    event.data["message"]["content"] = content;
                }
            }
        }
        apply_live_message_event(&mut replay_messages, &mut projection, event)?;
    }
    // latestSettlement may belong to the previous turn. Only use this legacy
    // fallback when neither the host state nor replay establishes an active or
    // settled turn; newer deltas must always win over unsequenced history.
    if !replayed_settlement
        && matches!(projection.phase, work_core::RunPhase::Ready)
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
    // Publish replayed bodies for existing history owners too; otherwise
    // canonical projection text is hidden behind a stale snapshot body. Do not
    // append event-only rows here: their chronological slots belong to projection.
    let messages = snapshot
        .messages
        .iter()
        .map(|original| {
            entry_id(original)
                .and_then(|id| {
                    replay_messages.iter().find(|item| {
                        entry_id(item) == Some(id) && message_role(item) == message_role(original)
                    })
                })
                .unwrap_or(original)
                .clone()
        })
        .collect();
    Ok((messages, projection))
}

#[cfg(test)]
mod duplicate_regression_tests;
#[cfg(test)]
mod snapshot_dedup_tests;

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

                if method == "POST" && path.starts_with("/api/v1/auth/qr") {
                    assert!(
                        request_text
                            .lines()
                            .any(|line| line.eq_ignore_ascii_case("x-rpi-web: 1"))
                    );
                    let body: Value =
                        serde_json::from_slice(&request[header_end..header_end + content_length])
                            .expect("JSON body");
                    if path.ends_with("/redeem") {
                        assert_eq!(body, json!({"ticket": "a".repeat(64)}));
                    } else {
                        assert_eq!(body, json!({}));
                    }
                }
                let (body, extra_headers) = match (method, path) {
                    ("POST", "/api/v1/auth/qr") => (
                        r#"{"ticket":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","expiresAt":1234,"nodeId":"test-node"}"#,
                        "",
                    ),
                    ("POST", "/api/v1/auth" | "/api/v1/auth/qr/redeem") => (
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

    #[test]
    fn qr_errors_do_not_expose_server_echoes_or_decode_sources() {
        let secret = "test-only-credential";
        let api = qr_error(ClientError::Api {
            status: StatusCode::BAD_REQUEST,
            message: secret.into(),
        });
        assert!(!format!("{api:?} {api}").contains(secret));
        let decode = serde_json::from_str::<u64>(&format!("\"{secret}\"")).unwrap_err();
        let error = qr_error(ClientError::Json(decode));
        assert!(!format!("{error:?} {error}").contains(secret));
        assert!(std::error::Error::source(&error).is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn qr_redemption_never_forwards_credentials_through_redirects() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            sync::{
                Arc,
                atomic::{AtomicBool, Ordering},
            },
        };
        let redirected = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        redirected.set_nonblocking(true).unwrap();
        let redirect_target = format!("http://{}/stolen", redirected.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stop_probe = stop.clone();
        let probe = std::thread::spawn(move || {
            while !stop_probe.load(Ordering::SeqCst) {
                match redirected.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        return true;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                    Err(error) => panic!("redirect probe: {error}"),
                }
            }
            false
        });
        let origin = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let url = format!("http://{}/", origin.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = origin.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 8192];
            let _ = stream.read(&mut request).unwrap();
            write!(stream, "HTTP/1.1 307 Temporary Redirect\r\nLocation: {redirect_target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let client = WorkClient::new(&url).unwrap();
        let result = run_native(async {
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                client.redeem_qr_authorization(&"a".repeat(64)),
            )
            .await
        });
        stop.store(true, Ordering::SeqCst);
        server.join().unwrap();
        assert!(
            !probe.join().unwrap(),
            "must not send QR credentials to a redirect target"
        );
        assert!(result.unwrap().is_err());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn qr_issue_and_redeem_use_json_header_and_same_cookie_jar() {
        let (url, server) = auth_test_server(true, 3);
        let client = WorkspaceClient::new(&url).expect("client");
        let code = run_native(client.issue_qr_authorization()).expect("issue QR");
        assert_eq!(code.expires_at, 1234);
        assert_eq!(code.node_id, "test-node");
        let serialized = serde_json::to_value(&code).expect("serialize");
        assert_eq!(serialized["expiresAt"], 1234);
        assert_eq!(serialized["nodeId"], "test-node");
        assert!(
            run_native(client.redeem_qr_authorization(&code.ticket))
                .expect("redeem")
                .authenticated
        );
        assert_eq!(
            server.join().expect("server"),
            vec![
                "POST /api/v1/auth/qr cookie=false",
                "POST /api/v1/auth/qr/redeem cookie=false",
                "GET /api/v1/auth cookie=true",
            ]
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn qr_redeem_rejects_ack_without_authoritative_cookie_auth() {
        let (url, server) = auth_test_server(false, 2);
        let client = WorkClient::new(&url).expect("client");
        assert!(matches!(
            run_native(client.redeem_qr_authorization(&"a".repeat(64))),
            Err(ClientError::AuthenticationNotConfirmed)
        ));
        assert_eq!(
            server.join().expect("server"),
            vec![
                "POST /api/v1/auth/qr/redeem cookie=false",
                "GET /api/v1/auth cookie=false",
            ]
        );
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
    fn explicit_ids_with_the_same_timestamp_never_overwrite_previous_turns() {
        let old = json!({"entryId":"old","role":"assistant","timestamp":42,"content":"old answer"});
        let new = json!({"entryId":"new","role":"assistant","timestamp":42,"content":"new answer"});
        let mut messages = vec![old.clone()];
        merge_live_message(&mut messages, &json!({"message":new}), Some(2));
        assert_eq!(messages, vec![old.clone(), new.clone()]);
        let mut projection = RunProjection::new("session");
        for id in ["old", "new"] {
            projection.blocks.push(work_core::MessageBlock::Text {
                id: format!("{id}-text-0"),
                text: id.into(),
            });
        }
        // An exact match later in canonical history must win over the first
        // message's colliding timestamp, including projection ID reconciliation.
        reconcile_messages(
            &mut messages,
            &mut projection,
            vec![old.clone(), new.clone()],
            false,
        );
        assert_eq!(messages, vec![old, new]);
        assert_eq!(projection.blocks[0].id(), "old-text-0");
        assert_eq!(projection.blocks[1].id(), "new-text-0");
    }

    #[test]
    fn changed_explicit_id_is_a_boundary_without_message_start() {
        let mut identity = MessageIdentity::default();
        let mut projection = RunProjection::new("session");
        for (seq, key, id) in [
            (1, "entryId", "old"),
            (2, "messageId", "new"),
            (3, "id", "last"),
        ] {
            let data = identity.annotate(
                &json!({
                    "type":"message_update", "eventType":"text_delta", "delta":id,
                    "message":{"entryId":null,key:id,"role":"assistant","timestamp":42}
                }),
                Some(seq),
            );
            assert_eq!(data["message"]["entryId"], id);
            assert!(data["message"].get(CLIENT_ANONYMOUS_ID).is_none());
            projection.apply(Event {
                id: format!("seq-{seq}"),
                seq: Some(seq),
                kind: normalize_event("message_update", &data).unwrap(),
            });
        }
        assert_eq!(projection.blocks.len(), 3);
        for (block, id) in projection.blocks.iter().zip(["old", "new", "last"]) {
            assert!(
                matches!(block, work_core::MessageBlock::Text { id: block_id, text }
                if block_id == &format!("{id}-text-0") && text == id)
            );
        }
    }

    #[test]
    fn run_and_user_boundaries_clear_unfinished_anonymous_identity() {
        for boundary in [
            json!({"type":"operation_accepted"}),
            json!({"type":"agent_start"}),
            json!({"type":"agent_end"}),
            json!({"type":"agent_settled"}),
            json!({"type":"message_end","message":{"entryId":"user","role":"user","content":"next"}}),
        ] {
            let mut identity = MessageIdentity::default();
            let delta = json!({"type":"message_update","eventType":"text_delta","delta":"text","message":{"role":"assistant"}});
            let before = identity.annotate(&delta, Some(1));
            identity.annotate(&boundary, Some(2));
            assert!(identity.current.is_none(), "{boundary}");
            let after = identity.annotate(&delta, Some(3));
            assert_ne!(before["message"]["entryId"], after["message"]["entryId"]);
            assert_eq!(after["message"][CLIENT_ANONYMOUS_ID], true);
        }
        // Tool results are not a new assistant message boundary.
        let mut identity = MessageIdentity::default();
        let first = identity.annotate(
            &json!({"type":"message_start","message":{"role":"assistant"}}),
            Some(1),
        );
        identity.annotate(&json!({"type":"message_end","message":{"role":"toolResult","toolCallId":"tool","content":"result"}}), Some(2));
        let after = identity.annotate(
            &json!({"type":"message_update","message":{"role":"assistant"}}),
            Some(3),
        );
        assert_eq!(first["message"]["entryId"], after["message"]["entryId"]);
    }

    #[test]
    fn timestamp_fallback_is_unique_and_only_for_real_anonymous_reconciliation() {
        let canonical = vec![
            json!({"entryId":"a","role":"assistant","timestamp":42,"content":"a"}),
            json!({"entryId":"b","role":"assistant","timestamp":42,"content":"b"}),
        ];
        let synthetic = json!({"entryId":"local","role":"assistant","timestamp":42, "__rpiClientAnonymousId":true});
        assert_eq!(matching_message_index(&canonical, &synthetic, true), None);
        assert_eq!(
            matching_message_index(&canonical[..1], &synthetic, false),
            None
        );
        assert_eq!(
            matching_message_index(&canonical[..1], &synthetic, true),
            Some(0)
        );
        let host_id = json!({"entryId":"anonymous-host-id","role":"assistant","timestamp":42});
        assert_eq!(
            matching_message_index(&canonical[..1], &host_id, true),
            None
        );
        let exact =
            json!({"entryId":"b","role":"assistant","timestamp":42, "__rpiClientAnonymousId":true});
        assert_eq!(matching_message_index(&canonical, &exact, true), Some(1));
        let promoted = json!({"entryId":"local","role":"assistant","timestamp":42,
            "__rpiClientAnonymousId":true,"__rpiClientCanonicalId":"b"});
        assert_eq!(matching_message_index(&canonical, &promoted, true), Some(1));
        assert_eq!(
            matching_message_index(&canonical[..1], &promoted, true),
            None
        );
    }

    #[test]
    fn separate_anonymous_messages_with_shared_timestamps_stay_separate_live() {
        let mut identity = MessageIdentity::default();
        let mut messages = Vec::new();
        for (seq, kind, text) in [
            (1, "message_start", "first"),
            (2, "message_end", "first"),
            (3, "message_start", "second"),
            (4, "message_end", "second"),
        ] {
            let data = identity.annotate(
                &json!({"type":kind,"message":{"role":"assistant","timestamp":42,"content":text}}),
                Some(seq),
            );
            merge_live_message(&mut messages, &data, Some(seq));
        }
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["content"], "first");
        assert_eq!(messages[1]["content"], "second");
        let retained = messages.clone();
        // Appending the first missing entry must not turn it into a weak
        // canonical match that swallows the second one.
        reconcile_messages(
            &mut messages,
            &mut RunProjection::new("session"),
            Vec::new(),
            false,
        );
        assert_eq!(messages, retained);
        let canonical =
            vec![json!({"entryId":"host","role":"assistant","timestamp":42,"content":"canonical"})];
        reconcile_messages(
            &mut messages,
            &mut RunProjection::new("session"),
            canonical.clone(),
            false,
        );
        assert_eq!(
            messages,
            [canonical, retained].concat(),
            "ambiguous live candidates must not share one canonical identity"
        );
    }

    #[test]
    fn anonymous_promotion_keeps_host_identity_even_without_a_timestamp() {
        let mut identity = MessageIdentity::default();
        let delta = identity.annotate(&json!({"type":"message_update","eventType":"text_delta","delta":"answer","message":{"role":"assistant"}}), Some(1));
        let canonical = json!({"entryId":"host","role":"assistant","content":"answer"});
        let end = identity.annotate(&json!({"type":"message_end","message":canonical}), Some(2));
        assert_eq!(delta["message"]["entryId"], end["message"]["entryId"]);
        assert_eq!(end["message"][CLIENT_CANONICAL_ID], "host");
        let mut messages = Vec::new();
        merge_live_message(&mut messages, &end, Some(2));
        let mut projection = RunProjection::new("session");
        projection.blocks.push(work_core::MessageBlock::Text {
            id: format!("{}-text-0", entry_id(&end["message"]).unwrap()),
            text: "answer".into(),
        });
        reconcile_messages(
            &mut messages,
            &mut projection,
            vec![canonical.clone()],
            false,
        );
        assert_eq!(messages, vec![canonical]);
        assert_eq!(projection.blocks[0].id(), "host-text-0");
    }

    #[test]
    fn image_delta_does_not_patch_a_previous_message_with_a_shared_timestamp() {
        let messages = vec![
            json!({"entryId":"old","role":"assistant","timestamp":42,"content":[{"type":"image","mimeType":"image/png","data":"AA=="}]}),
        ];
        let delta = json!({"type":"message_update","eventType":"text_delta","delta":"new caption","message":{"entryId":"new","role":"assistant","timestamp":42}});
        assert!(retained_text_delta(&messages, &delta, None).is_none());
    }

    #[test]
    fn settlement_reconciliation_adds_native_user_messages_and_rekeys_live_blocks() {
        let mut messages = vec![
            json!({"role":"assistant","entryId":"anonymous","timestamp":42,"content":"answer","__rpiClientAnonymousId":true}),
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
        reconcile_messages(&mut messages, &mut projection, canonical.clone(), false);
        assert_eq!(messages, canonical);
        assert_eq!(projection.blocks[0].id(), "persisted-text-0");
        assert_eq!(projection.cursor, None);
        reconcile_messages(&mut messages, &mut projection, Vec::new(), false);
        assert_eq!(messages, canonical);
    }

    #[test]
    fn bounded_settlement_keeps_historical_prefix_before_the_new_suffix() {
        let message =
            |n| json!({"entryId":format!("e{n}"),"role":"user","content":format!("message {n}")});
        let canonical: Vec<_> = (3..=50).map(message).collect();
        for old_range in [1..=51, 1..=2] {
            let mut messages: Vec<_> = old_range.clone().map(message).collect();
            let mut projection = RunProjection::new("session");
            reconcile_messages(&mut messages, &mut projection, canonical.clone(), true);
            let last = if *old_range.end() == 51 { 51 } else { 50 };
            assert_eq!(messages, (1..=last).map(message).collect::<Vec<_>>());
        }
    }

    #[test]
    fn tool_only_assistant_snapshots_keep_call_rows_before_results_and_next_turn() {
        for role_key in ["role", "kind"] {
            for kind in ["toolCall", "tool_call", "tool_use"] {
                let call = json!({"entryId":"call-row",role_key:"assistant","content":[
                    {"type":kind,"id":"child","name":"subagent","arguments":{"task":"inspect"}}
                ]});
                let result = json!({"entryId":"result-row","role":"toolResult","toolCallId":"child","content":"done"});
                let user = json!({"entryId":"next-user","role":"user","content":"continue"});
                let mut messages = Vec::new();
                for (seq, message) in [
                    (1, &call),
                    (2, &call),
                    (3, &result),
                    (4, &result),
                    (5, &user),
                ] {
                    merge_live_message(&mut messages, &json!({"message":message}), Some(seq));
                }
                assert_eq!(messages, vec![call, result, user]);
            }
        }
    }

    #[test]
    fn bounded_tool_turn_reconciliation_keeps_old_calls_with_their_results() {
        let mut messages = vec![json!({"entryId":"user","role":"user","content":"work"})];
        let mut canonical = messages.clone();
        for index in 0..60 {
            let call = json!({"entryId":format!("call-{index}"),"role":"assistant","content":[
                {"type":"toolCall","id":format!("tool-{index}"),"name":"read","arguments":{"path":format!("file-{index}")}}
            ]});
            let result = json!({"entryId":format!("result-{index}"),"role":"toolResult",
                "toolCallId":format!("tool-{index}"),"content":"done"});
            for message in [call, result] {
                merge_live_message(&mut messages, &json!({"message":message}), None);
                canonical.push(message);
            }
        }
        let suffix = canonical[canonical.len() - 48..].to_vec();
        reconcile_messages(
            &mut messages,
            &mut RunProjection::new("session"),
            suffix,
            true,
        );
        assert_eq!(messages, canonical);
    }

    #[test]
    fn empty_and_thinking_only_snapshots_do_not_hide_public_streaming_blocks() {
        for content in [
            Value::Null,
            json!([]),
            json!([{"type":"thinking","thinking":"private"}]),
            json!([{"type":"thinking","content":[{"type":"toolCall","id":"private"}]}]),
            json!([{"type":"text","text":"","metadata":{"type":"toolCall"}}]),
        ] {
            let mut messages = Vec::new();
            merge_live_message(
                &mut messages,
                &json!({"message":{
                    "entryId":"partial","role":"assistant","content":content
                }}),
                None,
            );
            assert!(messages.is_empty(), "content: {content}");
        }
    }

    #[test]
    fn non_assistant_events_do_not_create_assistant_blocks_and_tools_keep_identity() {
        let mut identity = MessageIdentity::default();
        for role in ["user", "tool", "toolResult"] {
            for kind in ["message_start", "message_update", "message_end"] {
                let data = json!({"type":kind,"message":{"role":role,"content":"not assistant"}});
                let annotated = identity.annotate(&data, Some(1));
                assert_eq!(
                    normalize_event(kind, &annotated).unwrap(),
                    EventKind::Ignored
                );
                if role == "user" {
                    assert!(entry_id(&annotated["message"]).is_some());
                    assert_eq!(identity.current.is_none(), kind == "message_end");
                } else {
                    assert_eq!(annotated, data);
                    assert!(identity.current.is_none());
                }
            }
        }
        for key in ["entryId", "messageId", "id"] {
            let data = json!({"type":"message_start","message":{"role":"assistant",key:"host-id"}});
            let annotated = identity.annotate(&data, Some(2));
            assert_eq!(annotated["message"]["entryId"], "host-id");
        }
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
            EventKind::ThinkingDelta {
                block_id: "assistant-message-thinking-0".into(),
                delta: "secret".into()
            }
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
    fn image_only_assistant_snapshots_merge_but_empty_streaming_does_not() {
        let mut messages = Vec::new();
        for content in [json!([]), json!([{"type":"thinking","text":"private"}])] {
            merge_live_message(
                &mut messages,
                &json!({"message":{"role":"assistant","entryId":"answer","content":content}}),
                Some(1),
            );
        }
        assert_eq!(messages, Vec::<Value>::new());

        let image = json!({"type":"image","data":"aW1hZ2U=","mimeType":"image/png"});
        let snapshot = json!({"message":{"role":"assistant","entryId":"answer","content":[image]}});
        merge_live_message(&mut messages, &snapshot, Some(2));
        merge_live_message(&mut messages, &snapshot, Some(3));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], snapshot["message"]);

        // A later empty partial snapshot must not replace the image row.
        merge_live_message(
            &mut messages,
            &json!({"message":{"role":"assistant","entryId":"answer","content":[]}}),
            Some(4),
        );
        assert_eq!(messages[0], snapshot["message"]);
    }

    #[test]
    fn assistant_normalization_only_exposes_public_text() {
        let image =
            json!({"type":"image","data":"aW1hZ2U=","mimeType":"image/png","text":"not public"});
        let content = json!([
            {"type":"text","text":"public "},
            {"type":"input_text","text":"input "},
            {"type":"output_text","text":"output "},
            {"text":"legacy"},
            image,
            {"type":"thinking","text":"private"},
            {"thinking":"private","text":"also private"},
            {"type":"reasoning","content":[{"type":"text","text":"private"}]},
            {"type":"toolCall","name":"internal","arguments":{"text":"private"}},
            {"type":"unknown","text":"not public"},
            {"type":null,"text":"not public"},
            {"unexpected":"not public"},
            42,
            true
        ]);
        for kind in ["message_start", "message_update", "message_end"] {
            for (content, snapshots) in [
                (
                    content.clone(),
                    vec![
                        (0, "public "),
                        (1, "input "),
                        (2, "output "),
                        (3, "legacy"),
                        (11, ""),
                    ],
                ),
                (json!([image]), vec![]),
            ] {
                let event = normalize_event(
                    kind,
                    &json!({"message":{"role":"assistant","entryId":"answer","content":content}}),
                )
                .unwrap();
                assert_eq!(
                    event,
                    EventKind::TextSnapshots {
                        message_id: "answer".into(),
                        snapshots: snapshots
                            .into_iter()
                            .map(|(index, text)| (format!("answer-text-{index}"), text.into()))
                            .collect(),
                        thinking: Vec::new(),
                        authoritative: true,
                    }
                );
            }
            assert_eq!(normalize_event(kind, &json!({"message":{"role":"assistant","entryId":"answer","content":"legacy string"}})).unwrap(),
                EventKind::TextSnapshot { block_id: "answer-text-0".into(), text: "legacy string".into() });
        }
        // Even a malformed text delta must not stringify a content object.
        for delta in [image, json!({"type":"thinking","text":"private"})] {
            assert_eq!(
                normalize_event(
                    "message_update",
                    &json!({"eventType":"text_delta","delta":delta}),
                )
                .unwrap(),
                EventKind::TextDelta {
                    block_id: "assistant-message-text-0".into(),
                    delta: String::new(),
                }
            );
        }
    }

    #[test]
    fn retained_image_snapshots_receive_contentless_caption_deltas() {
        for content_index in [None, Some(1), Some(0)] {
            let mut messages = vec![
                json!({"entryId":"image","role":"assistant","content":[{"type":"image","mimeType":"image/png","data":"AA=="}]}),
            ];
            for delta in ["cap", "tion"] {
                let mut event =
                    json!({"eventType":"text_delta","delta":delta,"message":{"entryId":"image"}});
                if let Some(index) = content_index {
                    event["contentIndex"] = json!(index);
                }
                merge_live_message(&mut messages, &event, None);
            }
            assert_eq!(content_text(&messages[0]["content"]), "caption");
            assert!(has_image_content(&messages[0]["content"]));
            assert_eq!(messages.len(), 1);
            merge_live_message(
                &mut messages,
                &json!({"eventType":"thinking_delta","delta":"private","message":{"entryId":"image","role":"assistant"}}),
                None,
            );
            assert_eq!(content_text(&messages[0]["content"]), "caption");
        }
    }

    #[test]
    fn tool_image_results_remain_structured_for_progress_and_completion() {
        let image = json!({"type":"image","data":"aW1hZ2U=","mimeType":"image/png"});
        for result in [
            json!({"content":[{"type":"text","text":"caption"},image],"details":{"width":1}}),
            json!([image]),
            image,
        ] {
            for (kind, field) in [
                ("tool_execution_update", "partialResult"),
                ("tool_execution_end", "result"),
            ] {
                let event = normalize_event(kind, &json!({"toolCallId":"image-call",field:result}))
                    .unwrap();
                let encoded = match event {
                    EventKind::ToolOutput {
                        content,
                        mode: OutputMode::Snapshot,
                        ..
                    } => content,
                    EventKind::ToolFinished {
                        result,
                        error: None,
                        ..
                    } => result,
                    _ => panic!("expected tool result"),
                };
                assert_eq!(serde_json::from_str::<Value>(&encoded).unwrap(), result);
            }
        }
    }

    #[test]
    fn plain_tool_results_keep_their_display_text() {
        for (result, expected) in [
            (json!("plain"), "plain"),
            (
                json!({"content":[{"type":"text","text":"one"},{"type":"text","text":"two"}]}),
                "onetwo",
            ),
            (json!([{"type":"text","text":"array"}]), "array"),
            (json!({"answer":42}), r#"{"answer":42}"#),
            (json!([{"answer":42}, true, 7]), r#"{"answer":42}true7"#),
            (
                json!({"content":[{"type":"text","text":"caption"}],"details":{"type":"image","data":"not-content"}}),
                "caption",
            ),
            (
                json!({"type":"thinking","content":[{"type":"image","data":"private"}]}),
                "",
            ),
            (Value::Null, ""),
        ] {
            for (kind, field) in [
                ("tool_execution_update", "partialResult"),
                ("tool_execution_end", "result"),
            ] {
                let event = normalize_event(kind, &json!({"toolCallId":"plain-call",field:result}))
                    .unwrap();
                let text = match event {
                    EventKind::ToolOutput { content, .. } => content,
                    EventKind::ToolFinished { result, .. } => result,
                    _ => panic!("expected tool result"),
                };
                assert_eq!(text, expected);
            }
        }
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
    fn snapshot_status_preserves_long_running_tools_without_new_events() {
        for (status, expected) in [
            ("running", work_core::RunPhase::Accepted),
            ("awaiting_input", work_core::RunPhase::AwaitingInput),
            ("waiting_for_input", work_core::RunPhase::AwaitingInput),
            ("cancelling", work_core::RunPhase::Cancelling),
        ] {
            for latest in [Value::Null, json!({"outcome":"completed"})] {
                let snapshot: Snapshot = serde_json::from_value(json!({
                    "sessionId":"session-1", "status":status, "lastSeq":12,
                    "latestSettlement":latest, "events":[]
                }))
                .unwrap();
                let projection = projection_from_snapshot(&snapshot).unwrap();
                assert_eq!(
                    projection.phase, expected,
                    "status: {status}, latest: {latest}"
                );
                assert_eq!(projection.cursor, Some(12));
            }
        }
    }

    #[test]
    fn snapshot_active_events_without_prior_settlement_are_not_completed_by_fallback() {
        for (event, expected) in [
            (json!({"type":"agent_start"}), work_core::RunPhase::Accepted),
            (
                json!({"type":"tool_execution_start","toolCallId":"child","toolName":"subagent","args":{}}),
                work_core::RunPhase::Tool,
            ),
            (
                json!({"type":"tool_execution_start","toolCallId":"children","toolName":"dispatch","args":{}}),
                work_core::RunPhase::Tool,
            ),
            (
                json!({"type":"agent_end"}),
                work_core::RunPhase::AwaitingSettlement,
            ),
            (
                json!({"type":"ui_dialog_request"}),
                work_core::RunPhase::AwaitingInput,
            ),
        ] {
            // Legacy hosts may omit status; a current event still outranks the
            // previous turn's unsequenced settlement.
            let snapshot: Snapshot = serde_json::from_value(json!({
                "sessionId":"session-1", "lastSeq":13,
                "latestSettlement":{"outcome":"completed"},
                "events":[{"seq":13,"event":event}]
            }))
            .unwrap();
            let projection = projection_from_snapshot(&snapshot).unwrap();
            assert_eq!(projection.phase, expected, "event: {event}");
            assert_eq!(projection.cursor, Some(13));
        }
    }

    #[test]
    fn snapshot_delta_settlement_and_new_turn_outrank_initial_status() {
        for (status, events, expected) in [
            (
                "running",
                json!([{"seq":13,"event":{"type":"agent_settled","outcome":"aborted"}}]),
                work_core::RunPhase::Settled(SettlementOutcome::Interrupted),
            ),
            (
                "running",
                json!([
                    {"seq":13,"event":{"type":"agent_settled","outcome":"completed"}},
                    {"seq":14,"event":{"type":"agent_start"}}
                ]),
                work_core::RunPhase::Accepted,
            ),
            (
                "ready",
                json!([{"seq":13,"event":{"type":"agent_start"}}]),
                work_core::RunPhase::Accepted,
            ),
        ] {
            let snapshot: Snapshot = serde_json::from_value(json!({
                "sessionId":"session-1", "status":status, "lastSeq":14,
                "latestSettlement":{"outcome":"completed"}, "events":events
            }))
            .unwrap();
            let projection = projection_from_snapshot(&snapshot).unwrap();
            assert_eq!(projection.phase, expected);
            assert_eq!(projection.cursor, Some(14));
        }
    }

    #[test]
    fn idle_and_legacy_snapshot_statuses_still_restore_latest_settlement() {
        for status in [Value::Null, json!("ready"), json!("idle"), json!("unknown")] {
            let snapshot: Snapshot = serde_json::from_value(json!({
                "sessionId":"session-1", "status":status,
                "latestSettlement":{"outcome":"completed"}
            }))
            .unwrap();
            assert_eq!(
                projection_from_snapshot(&snapshot).unwrap().phase,
                work_core::RunPhase::Settled(SettlementOutcome::Completed)
            );
        }
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
