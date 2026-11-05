//! Typed session configuration access; failures never fall back to native RPC.
use crate::{ClientError, SessionConfiguration, WorkClient, WorkspaceClient};
use serde_json::{Value, json};

impl WorkClient {
    pub async fn session_configuration(
        &self,
        id: &str,
    ) -> Result<SessionConfiguration, ClientError> {
        self.get(&format!("api/v1/sessions/{id}/configuration"))
            .await
    }

    pub async fn set_session_config_option(
        &self,
        id: &str,
        config_id: &str,
        value: Value,
    ) -> Result<Value, ClientError> {
        let payload = json!({"configId": config_id, "value": value});
        if let Some(value) = self
            .try_realtime_mutation(
                id,
                work_core::api::realtime::RealtimeOperationKind::SetConfiguration,
                &payload,
            )
            .await?
        {
            return Ok(value);
        }
        self.post(&format!("api/v1/sessions/{id}/configuration"), payload)
            .await
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
impl crate::LocalWorkspace {
    pub async fn session_configuration(
        &self,
        id: &str,
    ) -> Result<SessionConfiguration, ClientError> {
        self.node_owner()
            .session_configuration(id)
            .await
            .map_err(Into::into)
    }

    pub async fn set_session_config_option(
        &self,
        id: &str,
        config_id: &str,
        value: Value,
    ) -> Result<Value, ClientError> {
        self.node_owner()
            .set_session_config_option(id, config_id, value)
            .await
            .map_err(Into::into)
    }
}

impl WorkspaceClient {
    pub async fn session_configuration(
        &self,
        id: &str,
    ) -> Result<SessionConfiguration, ClientError> {
        match self {
            Self::Remote(client) => client.session_configuration(id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.session_configuration(id).await,
        }
    }

    pub async fn set_session_config_option(
        &self,
        id: &str,
        config_id: &str,
        value: Value,
    ) -> Result<Value, ClientError> {
        match self {
            Self::Remote(client) => client.set_session_config_option(id, config_id, value).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.set_session_config_option(id, config_id, value).await,
        }
    }
}
