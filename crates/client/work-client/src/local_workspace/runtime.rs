use super::LocalWorkspace;
use crate::local_workspace_adapter::map_workspace_stream;
use crate::{
    AuthStatus, ClientError, EventStream, History, ModelDescriptor, RuntimeState, SessionBranch,
    SessionConnection, ToolCatalog, WorkspaceClient,
};
use serde_json::Value;
use std::sync::Arc;

impl LocalWorkspace {
    pub async fn refresh_auth(&self, provider: &str) -> Result<Value, ClientError> {
        self.owner.refresh_auth(provider).await.map_err(Into::into)
    }
    pub async fn subscribe(
        &self,
        session_id: &str,
        after: u64,
    ) -> Result<EventStream, ClientError> {
        self.owner
            .subscribe(session_id, after)
            .await
            .map(map_workspace_stream)
            .map_err(Into::into)
    }
    pub async fn connect_session(
        &self,
        session_id: impl Into<String>,
    ) -> Result<SessionConnection, ClientError> {
        let session_id = session_id.into();
        WorkspaceClient::Local(Arc::new(self.clone()))
            .connect_session(session_id)
            .await
    }
    pub async fn suspend(&self) {
        self.owner.suspend().await;
    }
    pub async fn shutdown(&self) {
        self.owner.shutdown().await;
    }
    pub async fn session_branch(&self, session_id: &str) -> Result<SessionBranch, ClientError> {
        self.owner
            .session_branch(session_id)
            .await
            .map_err(Into::into)
    }
    pub async fn history(
        &self,
        session_id: &str,
        after_seq: Option<u64>,
        before_seq: Option<u64>,
        limit: u64,
    ) -> Result<History, ClientError> {
        self.owner
            .history(session_id, after_seq, before_seq, limit)
            .await
            .map_err(Into::into)
    }
    pub async fn runtime_state(&self, session_id: &str) -> Result<RuntimeState, ClientError> {
        self.owner
            .runtime_state(session_id)
            .await
            .map_err(Into::into)
    }
    pub async fn active_tools_state(&self, session_id: &str) -> Result<Vec<String>, ClientError> {
        self.owner
            .active_tools_state(session_id)
            .await
            .map_err(Into::into)
    }
    pub async fn models(&self, session_id: &str) -> Result<Vec<ModelDescriptor>, ClientError> {
        self.owner.models(session_id).await.map_err(Into::into)
    }
    pub async fn auth_status(&self, session_id: &str) -> Result<AuthStatus, ClientError> {
        self.owner.auth_status(session_id).await.map_err(Into::into)
    }
    pub async fn thinking_levels(&self, session_id: &str) -> Result<Vec<String>, ClientError> {
        self.owner
            .thinking_levels(session_id)
            .await
            .map_err(Into::into)
    }
    pub async fn tools(&self, session_id: &str) -> Result<ToolCatalog, ClientError> {
        self.owner.tools(session_id).await.map_err(Into::into)
    }
}
