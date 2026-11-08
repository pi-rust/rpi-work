//! Optional desktop-owner HTTP endpoints, with legacy standalone fallback only
//! on an explicit 404. Authentication/network failures must remain visible.
use crate::{AgentBackendOption, ClientError, SessionAgentBinding, WorkClient};
use serde::Deserialize;
use serde_json::json;
impl WorkClient {
    pub async fn agent_backends(&self) -> Result<Vec<AgentBackendOption>, ClientError> {
        #[derive(Deserialize)]
        struct Backends {
            backends: Vec<AgentBackendOption>,
        }
        match self.get::<Backends>("api/v1/agent-backends").await {
            Ok(reply) => Ok(reply.backends),
            Err(ClientError::Api { status, .. }) if status == reqwest::StatusCode::NOT_FOUND => {
                Ok(vec![AgentBackendOption {
                    id: "rpi".into(),
                    name: "RPI".into(),
                    available: true,
                    reason: None,
                }])
            }
            Err(error) => Err(error),
        }
    }
    pub async fn session_agent_binding(
        &self,
        id: &str,
    ) -> Result<SessionAgentBinding, ClientError> {
        match self
            .get(&format!("api/v1/sessions/{id}/agent-binding"))
            .await
        {
            Ok(reply) => Ok(reply),
            Err(ClientError::Api { status, .. }) if status == reqwest::StatusCode::NOT_FOUND => {
                self.get_session(id).await?;
                Ok(SessionAgentBinding {
                    backend: "rpi".into(),
                    external_session_id: None,
                })
            }
            Err(error) => Err(error),
        }
    }
    pub async fn create_session_with_backend(
        &self,
        project: Option<&str>,
        backend: &str,
    ) -> Result<String, ClientError> {
        #[derive(Deserialize)]
        struct Created {
            #[serde(rename = "sessionId")]
            session_id: String,
        }
        let reply: Created = self
            .post(
                "api/v1/sessions",
                json!({"projectId":project,"backend":backend}),
            )
            .await?;
        Ok(reply.session_id)
    }
}
