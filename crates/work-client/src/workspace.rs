//! Workspace API shared by HTTP clients and the desktop in-process RPC host.
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
use crate::LocalWorkspace;
use crate::{
    AuthState, AuthStatus, Capabilities, ClientError, Diff, EventStream, FileContent, FileListing,
    GitStatus, Health, History, HostResponse, MessageIdentity, ModelDescriptor, Project,
    ProjectFolders, QueueCancellation, QueueMutation, QueueSnapshot, RuntimeState, SessionBranch,
    SessionConnection, SessionMutation, SessionPreferences, SessionSummary, Snapshot, ToolCatalog,
    WorkClient, projection_from_snapshot, snapshot_dialogs, unsupported,
};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::Value;

/// HTTP everywhere; an in-process native workspace on desktop platforms only.
#[derive(Clone)]
pub enum WorkspaceClient {
    Remote(WorkClient),
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    Local(std::sync::Arc<LocalWorkspace>),
}

impl From<WorkClient> for WorkspaceClient {
    fn from(client: WorkClient) -> Self {
        Self::Remote(client)
    }
}

impl WorkspaceClient {
    pub fn new(base_url: &str) -> Result<Self, ClientError> {
        WorkClient::new(base_url).map(Self::Remote)
    }
    pub fn new_with_token(base_url: &str, token: Option<&str>) -> Result<Self, ClientError> {
        WorkClient::new_with_token(base_url, token).map(Self::Remote)
    }
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    pub fn local_rpc(
        executable: std::path::PathBuf,
        data: std::path::PathBuf,
    ) -> Result<Self, ClientError> {
        LocalWorkspace::new(executable, data).map(|client| Self::Local(std::sync::Arc::new(client)))
    }
    #[must_use]
    pub fn is_local(&self) -> bool {
        match self {
            Self::Remote(_) => false,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(_) => true,
        }
    }
    #[must_use]
    pub fn descriptor(&self) -> work_core::AgentDescriptor {
        match self {
            Self::Remote(client) => client.descriptor(),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.descriptor(),
        }
    }
    /// Stop native children while keeping the catalog and facade reusable.
    /// Remote workspaces are not owned by this client and are left running.
    pub async fn suspend(&self) {
        match self {
            Self::Remote(_) => {}
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.suspend().await,
        }
    }
    pub async fn shutdown(&self) {
        match self {
            Self::Remote(_) => {}
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.shutdown().await,
        }
    }
    /// Create an exact selected directory, not a path relative to an HTTP node root.
    pub async fn create_project_directory(
        &self,
        name: &str,
        path: impl AsRef<std::path::Path>,
        init_git: bool,
    ) -> Result<Project, ClientError> {
        let _ = (name, path.as_ref(), init_git);
        match self {
            Self::Remote(_) => Err(unsupported("create_project_directory", "http-sse")),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => {
                client
                    .create_project_directory(name, path.as_ref(), init_git)
                    .await
            }
        }
    }
    /// Native model/auth control context, created lazily without a user project.
    pub async fn control_session(&self) -> Result<String, ClientError> {
        match self {
            Self::Remote(_) => Err(unsupported("control_session", "http-sse")),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.control_session().await,
        }
    }
    pub async fn health(&self) -> Result<Health, ClientError> {
        match self {
            Self::Remote(client) => client.health().await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.health().await,
        }
    }

    pub async fn auth(&self) -> Result<AuthState, ClientError> {
        match self {
            Self::Remote(client) => client.auth().await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.auth().await,
        }
    }

    pub async fn login(&self, password: &str) -> Result<AuthState, ClientError> {
        match self {
            Self::Remote(client) => client.login(password).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.login(password).await,
        }
    }

    pub async fn logout(&self) -> Result<AuthState, ClientError> {
        match self {
            Self::Remote(client) => client.logout().await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.logout().await,
        }
    }

    pub async fn refresh_auth(&self, provider: &str) -> Result<Value, ClientError> {
        match self {
            Self::Remote(client) => client.refresh_auth(provider).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.refresh_auth(provider).await,
        }
    }

    pub async fn capabilities(&self) -> Result<Capabilities, ClientError> {
        match self {
            Self::Remote(client) => client.capabilities().await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.capabilities().await,
        }
    }

    pub async fn projects(&self) -> Result<Vec<Project>, ClientError> {
        match self {
            Self::Remote(client) => client.projects().await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.projects().await,
        }
    }

    pub async fn rename_project(
        &self,
        project_id: &str,
        name: &str,
    ) -> Result<Project, ClientError> {
        match self {
            Self::Remote(client) => client.rename_project(project_id, name).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.rename_project(project_id, name).await,
        }
    }

    pub async fn project_folders(&self, path: &str) -> Result<ProjectFolders, ClientError> {
        match self {
            Self::Remote(client) => client.project_folders(path).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.project_folders(path).await,
        }
    }

    pub async fn create_project(
        &self,
        name: &str,
        path: Option<&str>,
        init_git: bool,
    ) -> Result<Project, ClientError> {
        match self {
            Self::Remote(client) => client.create_project(name, path, init_git).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.create_project(name, path, init_git).await,
        }
    }

    pub async fn files(&self, project_id: &str, path: &str) -> Result<FileListing, ClientError> {
        match self {
            Self::Remote(client) => client.files(project_id, path).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.files(project_id, path).await,
        }
    }

    pub async fn file(&self, project_id: &str, path: &str) -> Result<FileContent, ClientError> {
        match self {
            Self::Remote(client) => client.file(project_id, path).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.file(project_id, path).await,
        }
    }

    pub async fn git_status(&self, project_id: &str) -> Result<GitStatus, ClientError> {
        match self {
            Self::Remote(client) => client.git_status(project_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.git_status(project_id).await,
        }
    }

    pub async fn diff(
        &self,
        project_id: &str,
        path: &str,
        staged: bool,
    ) -> Result<Diff, ClientError> {
        match self {
            Self::Remote(client) => client.diff(project_id, path, staged).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.diff(project_id, path, staged).await,
        }
    }

    pub async fn sessions(&self) -> Result<Vec<SessionSummary>, ClientError> {
        match self {
            Self::Remote(client) => client.sessions().await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.sessions().await,
        }
    }

    pub async fn create_session(&self, project_id: Option<&str>) -> Result<String, ClientError> {
        match self {
            Self::Remote(client) => client.create_session(project_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.create_session(project_id).await,
        }
    }

    pub async fn get_session(&self, session_id: &str) -> Result<SessionSummary, ClientError> {
        match self {
            Self::Remote(client) => client.get_session(session_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.get_session(session_id).await,
        }
    }

    pub async fn rename_session(
        &self,
        session_id: &str,
        title: &str,
    ) -> Result<SessionMutation, ClientError> {
        match self {
            Self::Remote(client) => client.rename_session(session_id, title).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.rename_session(session_id, title).await,
        }
    }

    pub async fn session_preferences(
        &self,
        session_id: &str,
        archived: Option<bool>,
        pinned: Option<bool>,
    ) -> Result<SessionPreferences, ClientError> {
        match self {
            Self::Remote(client) => {
                client
                    .session_preferences(session_id, archived, pinned)
                    .await
            }
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => {
                client
                    .session_preferences(session_id, archived, pinned)
                    .await
            }
        }
    }

    pub async fn fork_session(
        &self,
        session_id: &str,
        entry_id: Option<&str>,
        title: Option<&str>,
    ) -> Result<SessionMutation, ClientError> {
        match self {
            Self::Remote(client) => client.fork_session(session_id, entry_id, title).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.fork_session(session_id, entry_id, title).await,
        }
    }

    pub async fn clone_session(
        &self,
        session_id: &str,
        title: Option<&str>,
    ) -> Result<SessionMutation, ClientError> {
        match self {
            Self::Remote(client) => client.clone_session(session_id, title).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.clone_session(session_id, title).await,
        }
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), ClientError> {
        match self {
            Self::Remote(client) => client.delete_session(session_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.delete_session(session_id).await,
        }
    }

    pub async fn session_branch(&self, session_id: &str) -> Result<SessionBranch, ClientError> {
        match self {
            Self::Remote(client) => client.session_branch(session_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.session_branch(session_id).await,
        }
    }

    pub async fn history(
        &self,
        session_id: &str,
        after_seq: Option<u64>,
        before_seq: Option<u64>,
        limit: u64,
    ) -> Result<History, ClientError> {
        match self {
            Self::Remote(client) => {
                client
                    .history(session_id, after_seq, before_seq, limit)
                    .await
            }
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => {
                client
                    .history(session_id, after_seq, before_seq, limit)
                    .await
            }
        }
    }

    pub async fn runtime_state(&self, session_id: &str) -> Result<RuntimeState, ClientError> {
        match self {
            Self::Remote(client) => client.runtime_state(session_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.runtime_state(session_id).await,
        }
    }

    pub async fn active_tools_state(&self, session_id: &str) -> Result<Vec<String>, ClientError> {
        match self {
            Self::Remote(client) => client.active_tools_state(session_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.active_tools_state(session_id).await,
        }
    }

    pub async fn models(&self, session_id: &str) -> Result<Vec<ModelDescriptor>, ClientError> {
        match self {
            Self::Remote(client) => client.models(session_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.models(session_id).await,
        }
    }

    pub async fn auth_status(&self, session_id: &str) -> Result<AuthStatus, ClientError> {
        match self {
            Self::Remote(client) => client.auth_status(session_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.auth_status(session_id).await,
        }
    }

    pub async fn thinking_levels(&self, session_id: &str) -> Result<Vec<String>, ClientError> {
        match self {
            Self::Remote(client) => client.thinking_levels(session_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.thinking_levels(session_id).await,
        }
    }

    pub async fn tools(&self, session_id: &str) -> Result<ToolCatalog, ClientError> {
        match self {
            Self::Remote(client) => client.tools(session_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.tools(session_id).await,
        }
    }

    pub async fn set_model(&self, session_id: &str, model_id: &str) -> Result<Value, ClientError> {
        match self {
            Self::Remote(client) => client.set_model(session_id, model_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.set_model(session_id, model_id).await,
        }
    }

    pub async fn set_thinking_level(
        &self,
        session_id: &str,
        level: &str,
    ) -> Result<Value, ClientError> {
        match self {
            Self::Remote(client) => client.set_thinking_level(session_id, level).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.set_thinking_level(session_id, level).await,
        }
    }

    pub async fn set_active_tools(
        &self,
        session_id: &str,
        tools: &[String],
    ) -> Result<Value, ClientError> {
        match self {
            Self::Remote(client) => client.set_active_tools(session_id, tools).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.set_active_tools(session_id, tools).await,
        }
    }

    pub async fn resume_pending(&self, session_id: &str) -> Result<Value, ClientError> {
        match self {
            Self::Remote(client) => client.resume_pending(session_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.resume_pending(session_id).await,
        }
    }

    pub async fn compact(
        &self,
        session_id: &str,
        custom_instructions: Option<&str>,
    ) -> Result<Value, ClientError> {
        match self {
            Self::Remote(client) => client.compact(session_id, custom_instructions).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.compact(session_id, custom_instructions).await,
        }
    }

    pub async fn queue(
        &self,
        session_id: &str,
        include_completed: bool,
    ) -> Result<QueueSnapshot, ClientError> {
        match self {
            Self::Remote(client) => client.queue(session_id, include_completed).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.queue(session_id, include_completed).await,
        }
    }

    pub async fn queue_message(
        &self,
        session_id: &str,
        message: &str,
        deliver_as: &str,
    ) -> Result<QueueMutation, ClientError> {
        match self {
            Self::Remote(client) => client.queue_message(session_id, message, deliver_as).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.queue_message(session_id, message, deliver_as).await,
        }
    }

    pub async fn cancel_queued(
        &self,
        session_id: &str,
        entry_id: &str,
    ) -> Result<QueueCancellation, ClientError> {
        match self {
            Self::Remote(client) => client.cancel_queued(session_id, entry_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.cancel_queued(session_id, entry_id).await,
        }
    }

    pub async fn clear_queue(&self, session_id: &str) -> Result<Value, ClientError> {
        match self {
            Self::Remote(client) => client.clear_queue(session_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.clear_queue(session_id).await,
        }
    }

    pub async fn dialog_response(
        &self,
        session_id: &str,
        request_id: &str,
        value: Value,
    ) -> Result<Value, ClientError> {
        match self {
            Self::Remote(client) => client.dialog_response(session_id, request_id, value).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.dialog_response(session_id, request_id, value).await,
        }
    }

    pub async fn cancel_dialog(
        &self,
        session_id: &str,
        request_id: &str,
    ) -> Result<Value, ClientError> {
        match self {
            Self::Remote(client) => client.cancel_dialog(session_id, request_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.cancel_dialog(session_id, request_id).await,
        }
    }

    pub async fn open_session(&self, session_id: &str) -> Result<SessionSummary, ClientError> {
        match self {
            Self::Remote(client) => client.open_session(session_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.open_session(session_id).await,
        }
    }

    pub async fn snapshot(&self, session_id: &str) -> Result<Snapshot, ClientError> {
        match self {
            Self::Remote(client) => client.snapshot(session_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.snapshot(session_id).await,
        }
    }

    pub async fn command<T: for<'de> Deserialize<'de>>(
        &self,
        session_id: &str,
        payload: Value,
    ) -> Result<HostResponse<T>, ClientError> {
        match self {
            Self::Remote(client) => client.command(session_id, payload).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.command(session_id, payload).await,
        }
    }

    pub async fn command_data<T: for<'de> Deserialize<'de>>(
        &self,
        session_id: &str,
        command: &str,
        payload: Value,
    ) -> Result<T, ClientError> {
        match self {
            Self::Remote(client) => client.command_data(session_id, command, payload).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.command_data(session_id, command, payload).await,
        }
    }

    pub async fn prompt(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<HostResponse<Value>, ClientError> {
        match self {
            Self::Remote(client) => client.prompt(session_id, message).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.prompt(session_id, message).await,
        }
    }

    pub async fn abort(&self, session_id: &str) -> Result<HostResponse<Value>, ClientError> {
        match self {
            Self::Remote(client) => client.abort(session_id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.abort(session_id).await,
        }
    }

    pub async fn subscribe(
        &self,
        session_id: &str,
        after: u64,
    ) -> Result<EventStream, ClientError> {
        match self {
            Self::Remote(client) => client.subscribe(session_id, after).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.subscribe(session_id, after).await,
        }
    }

    pub(crate) async fn snapshot_stream(
        &self,
        session_id: &str,
    ) -> Result<(Snapshot, EventStream), ClientError> {
        if self.is_local() {
            // subscribe() acquires the bounded core replay and receiver eagerly.
            let stream = self.subscribe(session_id, 0).await?;
            let snapshot = self.snapshot(session_id).await?;
            let after = snapshot.last_seq;
            let stream = stream.filter_map(move |event| async move {
                match &event {
                    Ok(wire) if wire.seq.is_some_and(|seq| seq <= after) => None,
                    // The initial replay starts at zero; its old-history gap is
                    // covered by this snapshot. Live lag notices contain dropped.
                    Ok(wire)
                        if wire.event_type == "resync_required"
                            && wire.data.get("dropped").is_none() =>
                    {
                        None
                    }
                    _ => Some(event),
                }
            });
            Ok((snapshot, Box::pin(stream)))
        } else {
            let snapshot = self.snapshot(session_id).await?;
            let stream = self.subscribe(session_id, snapshot.last_seq).await?;
            Ok((snapshot, stream))
        }
    }
    pub async fn connect_session(
        &self,
        session_id: impl Into<String>,
    ) -> Result<SessionConnection, ClientError> {
        let session_id = session_id.into();
        self.open_session(&session_id).await?;
        let (snapshot, stream) = self.snapshot_stream(&session_id).await?;
        let projection = projection_from_snapshot(&snapshot)?;
        let mut identity = MessageIdentity::default();
        for item in &snapshot.events {
            identity.annotate(&item.event, Some(item.seq));
        }
        let pending_dialogs = snapshot_dialogs(&snapshot);
        let messages = snapshot.messages;
        Ok(SessionConnection {
            client: self.clone(),
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
}
