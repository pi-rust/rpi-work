use super::LocalWorkspace;
use crate::{
    ClientError, HostResponse, SessionMutation, SessionPreferences, SessionSummary, Snapshot,
};
use serde::Deserialize;
use serde_json::Value;

impl LocalWorkspace {
    pub async fn sessions(&self) -> Result<Vec<SessionSummary>, ClientError> {
        self.owner.sessions().await.map_err(Into::into)
    }
    pub async fn create_session(&self, project_id: Option<&str>) -> Result<String, ClientError> {
        self.owner
            .create_session(project_id)
            .await
            .map_err(Into::into)
    }
    pub async fn control_session(&self) -> Result<String, ClientError> {
        self.owner.control_session().await.map_err(Into::into)
    }
    pub async fn get_session(&self, session_id: &str) -> Result<SessionSummary, ClientError> {
        self.owner.get_session(session_id).await.map_err(Into::into)
    }
    pub async fn rename_session(
        &self,
        session_id: &str,
        title: &str,
    ) -> Result<SessionMutation, ClientError> {
        self.owner
            .rename_session(session_id, title)
            .await
            .map_err(Into::into)
    }
    pub async fn session_preferences(
        &self,
        session_id: &str,
        archived: Option<bool>,
        pinned: Option<bool>,
    ) -> Result<SessionPreferences, ClientError> {
        self.owner
            .session_preferences(session_id, archived, pinned)
            .await
            .map_err(Into::into)
    }
    pub async fn fork_session(
        &self,
        session_id: &str,
        entry_id: Option<&str>,
        title: Option<&str>,
    ) -> Result<SessionMutation, ClientError> {
        self.owner
            .fork_session(session_id, entry_id, title)
            .await
            .map_err(Into::into)
    }
    pub async fn clone_session(
        &self,
        session_id: &str,
        title: Option<&str>,
    ) -> Result<SessionMutation, ClientError> {
        self.owner
            .clone_session(session_id, title)
            .await
            .map_err(Into::into)
    }
    pub async fn delete_session(&self, session_id: &str) -> Result<(), ClientError> {
        self.owner
            .delete_session(session_id)
            .await
            .map_err(Into::into)
    }
    pub async fn open_session(&self, session_id: &str) -> Result<SessionSummary, ClientError> {
        self.owner
            .open_session(session_id)
            .await
            .map_err(Into::into)
    }
    pub async fn snapshot(&self, session_id: &str) -> Result<Snapshot, ClientError> {
        self.owner.snapshot(session_id).await.map_err(Into::into)
    }
    pub async fn snapshot_with_limit(
        &self,
        session_id: &str,
        limit: u64,
    ) -> Result<Snapshot, ClientError> {
        self.owner
            .snapshot_with_limit(session_id, limit)
            .await
            .map_err(Into::into)
    }
    pub async fn tool_program_children(
        &self,
        session_id: &str,
        payload: Value,
    ) -> Result<Value, ClientError> {
        self.owner
            .tool_program_children(session_id, payload)
            .await
            .map_err(Into::into)
    }
    pub async fn tool_program_phases(
        &self,
        session_id: &str,
        payload: Value,
    ) -> Result<Value, ClientError> {
        self.owner
            .tool_program_phases(session_id, payload)
            .await
            .map_err(Into::into)
    }
    pub async fn tool_program_body(
        &self,
        session_id: &str,
        payload: Value,
    ) -> Result<Value, ClientError> {
        self.owner
            .tool_program_body(session_id, payload)
            .await
            .map_err(Into::into)
    }
    pub async fn tool_program_snapshot(
        &self,
        session_id: &str,
        payload: Value,
    ) -> Result<Value, ClientError> {
        self.owner
            .tool_program_snapshot(session_id, payload)
            .await
            .map_err(Into::into)
    }
    pub async fn command<T: for<'de> Deserialize<'de>>(
        &self,
        session_id: &str,
        payload: Value,
    ) -> Result<HostResponse<T>, ClientError> {
        self.owner
            .command(session_id, payload)
            .await
            .map_err(Into::into)
    }
    pub async fn command_data<T: for<'de> Deserialize<'de>>(
        &self,
        session_id: &str,
        command: &str,
        payload: Value,
    ) -> Result<T, ClientError> {
        self.owner
            .command_data(session_id, command, payload)
            .await
            .map_err(Into::into)
    }
}
