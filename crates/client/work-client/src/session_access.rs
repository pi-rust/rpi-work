//! Effective session operations observed from the bound node runtime.
//! Access errors are explicit: never infer support or fall back to raw RPC.
use crate::{ClientError, SessionAccess, WorkClient, WorkspaceClient};

impl WorkClient {
    pub async fn session_access(&self, id: &str) -> Result<SessionAccess, ClientError> {
        self.get(&format!("api/v1/sessions/{id}/access")).await
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
impl crate::LocalWorkspace {
    pub async fn session_access(&self, id: &str) -> Result<SessionAccess, ClientError> {
        self.node_owner()
            .session_access(id)
            .await
            .map_err(Into::into)
    }
}

impl WorkspaceClient {
    pub async fn session_access(&self, id: &str) -> Result<SessionAccess, ClientError> {
        match self {
            Self::Remote(client) => client.session_access(id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.session_access(id).await,
        }
    }
}
