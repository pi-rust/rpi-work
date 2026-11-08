use super::LocalWorkspace;
use crate::ClientError;
use crate::local_workspace_adapter::into_agent_client;

impl LocalWorkspace {
    pub async fn agent_backends(&self) -> Result<Vec<crate::AgentBackendOption>, ClientError> {
        self.owner.agent_backends().await.map_err(Into::into)
    }
    pub async fn session_agent_binding(
        &self,
        id: &str,
    ) -> Result<crate::SessionAgentBinding, ClientError> {
        self.owner
            .session_agent_binding(id)
            .await
            .map_err(Into::into)
    }
    pub async fn create_session_with_backend(
        &self,
        project: Option<&str>,
        backend: &str,
    ) -> Result<String, ClientError> {
        self.owner
            .create_session_with_backend(project, backend)
            .await
            .map_err(Into::into)
    }
    pub async fn prepare_agent_session(
        &self,
        id: &str,
    ) -> Result<Option<crate::AgentClient>, ClientError> {
        self.owner
            .prepare_agent_session(id)
            .await
            .map(|agent| agent.map(into_agent_client))
            .map_err(Into::into)
    }
}
