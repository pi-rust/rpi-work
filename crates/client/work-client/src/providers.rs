//! Provider wire adapters. Configuration authority remains with the node owner.
use crate::{ClientError, WorkClient, WorkspaceClient};
use serde_json::json;
use work_core::api::{NativeReleaseReport, ProviderCatalog, ProviderMutation, ProviderReceipt};

// A peer may echo a write-only key in its error body; never expose that body.
fn provider_error(error: ClientError) -> ClientError {
    match error {
        ClientError::Api { status, .. } => ClientError::Api {
            status,
            message: "provider operation failed".into(),
        },
        error => error,
    }
}

impl WorkClient {
    pub async fn provider_catalog(&self) -> Result<ProviderCatalog, ClientError> {
        self.get("api/v1/providers").await.map_err(provider_error)
    }

    pub async fn provider_mutate(
        &self,
        request: ProviderMutation,
    ) -> Result<ProviderReceipt, ClientError> {
        self.post("api/v1/providers", json!(request))
            .await
            .map_err(provider_error)
    }

    pub async fn provider_release_hosts(&self) -> Result<NativeReleaseReport, ClientError> {
        self.post("api/v1/providers/release-idle-hosts", json!({}))
            .await
            .map_err(provider_error)
    }
}

impl WorkspaceClient {
    pub async fn provider_catalog(&self) -> Result<ProviderCatalog, ClientError> {
        match self {
            Self::Remote(client) => client.provider_catalog().await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client
                .node_owner()
                .provider_catalog()
                .await
                .map_err(Into::into),
        }
    }

    pub async fn provider_mutate(
        &self,
        request: ProviderMutation,
    ) -> Result<ProviderReceipt, ClientError> {
        match self {
            Self::Remote(client) => client.provider_mutate(request).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client
                .node_owner()
                .provider_mutate(request)
                .await
                .map_err(Into::into),
        }
    }

    pub async fn provider_release_hosts(&self) -> Result<NativeReleaseReport, ClientError> {
        match self {
            Self::Remote(client) => client.provider_release_hosts().await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client
                .node_owner()
                .provider_release_hosts()
                .await
                .map_err(Into::into),
        }
    }
}
