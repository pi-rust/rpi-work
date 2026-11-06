//! Generic component protocol. No component identity or panel-specific policy.
use crate::{ClientError, WorkClient, WorkspaceClient};
use futures_util::StreamExt;
use serde_json::{Value, json};
use work_component_api::{Bundle, Call, InstallRequest, Installed, MAX_BUNDLE, valid_id};

fn check_id(id: &str) -> Result<(), ClientError> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(ClientError::ProtocolMismatch(
            "Invalid component identity".into(),
        ))
    }
}

impl WorkClient {
    pub async fn component_catalog(&self) -> Result<Vec<Installed>, ClientError> {
        self.get("api/v1/components").await
    }
    pub async fn component_panel(&self, id: &str, panel: &str) -> Result<String, ClientError> {
        #[derive(serde::Deserialize)]
        struct Document {
            html: String,
        }
        check_id(id)?;
        check_id(panel)?;
        let document: Document = self
            .get(&format!("api/v1/components/{id}/panels/{panel}"))
            .await?;
        Ok(document.html)
    }
    pub async fn component_call(&self, id: &str, call: Call) -> Result<Value, ClientError> {
        check_id(id)?;
        self.post(&format!("api/v1/components/{id}/call"), json!(call))
            .await
    }
    pub async fn component_install(
        &self,
        request: InstallRequest,
    ) -> Result<Installed, ClientError> {
        self.post("api/v1/components", json!(request)).await
    }
    pub async fn component_remove(&self, id: &str) -> Result<(), ClientError> {
        check_id(id)?;
        self.delete(&format!("api/v1/components/{id}")).await
    }
}

impl WorkspaceClient {
    /// Resolve a granted capability through the node's package declarations.
    /// Clients composing node functions need no component ID or presence check.
    pub async fn component_capability(
        &self,
        capability: &str,
        arguments: Value,
    ) -> Result<Value, ClientError> {
        let mut catalog = self.component_catalog().await?;
        catalog.sort_by_key(|component| component.source != "bundled");
        for component in catalog {
            if !component.enabled || !component.grants.contains(capability) {
                continue;
            }
            if let Some((action, _)) = component
                .manifest
                .actions
                .iter()
                .find(|(_, action)| action.capability == capability)
            {
                return self
                    .component_call(
                        &component.manifest.id,
                        Call {
                            action: action.clone(),
                            arguments,
                        },
                    )
                    .await;
            }
        }
        Err(crate::unsupported(capability, "node components"))
    }
    pub async fn component_catalog(&self) -> Result<Vec<Installed>, ClientError> {
        match self {
            Self::Remote(c) => c.component_catalog().await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(c) => c.node_owner().component_catalog().map_err(Into::into),
        }
    }
    pub async fn component_panel(&self, id: &str, panel: &str) -> Result<String, ClientError> {
        check_id(id)?;
        check_id(panel)?;
        match self {
            Self::Remote(c) => c.component_panel(id, panel).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(c) => c
                .node_owner()
                .component_panel(id, panel)
                .map_err(Into::into),
        }
    }
    pub async fn component_call(&self, id: &str, call: Call) -> Result<Value, ClientError> {
        check_id(id)?;
        match self {
            Self::Remote(c) => c.component_call(id, call).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(c) => c
                .node_owner()
                .component_call(id, call, true)
                .map_err(Into::into),
        }
    }
    pub async fn component_install(
        &self,
        request: InstallRequest,
    ) -> Result<Installed, ClientError> {
        match self {
            Self::Remote(c) => c.component_install(request).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(c) => c
                .node_owner()
                .component_install(request)
                .map_err(Into::into),
        }
    }
    pub async fn component_remove(&self, id: &str) -> Result<(), ClientError> {
        check_id(id)?;
        match self {
            Self::Remote(c) => c.component_remove(id).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(c) => c.node_owner().component_remove(id).map_err(Into::into),
        }
    }
}

/// Fetch without node cookies or credentials, before explicit installation.
/// The caller reviews identity and grants; the digest validates content integrity.
pub async fn download_component(url: &str) -> Result<Bundle, ClientError> {
    let url = reqwest::Url::parse(url)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(ClientError::ProtocolMismatch(
            "Component download requires an HTTPS URL without credentials".into(),
        ));
    }
    let builder = reqwest::Client::builder();
    #[cfg(not(target_arch = "wasm32"))]
    let builder = builder
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let url = attempt.url();
            if attempt.previous().len() >= 5
                || url.scheme() != "https"
                || !url.username().is_empty()
                || url.password().is_some()
            {
                attempt.error("Unsafe component download redirect")
            } else {
                attempt.follow()
            }
        }))
        .timeout(std::time::Duration::from_secs(30));
    let response = builder.build()?.get(url).send().await?.error_for_status()?;
    if response.url().scheme() != "https"
        || !response.url().username().is_empty()
        || response.url().password().is_some()
    {
        return Err(ClientError::ProtocolMismatch(
            "Invalid final component download address".into(),
        ));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BUNDLE as u64)
    {
        return Err(ClientError::ProtocolMismatch(
            "Component package too large".into(),
        ));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if bytes.len() + chunk.len() > MAX_BUNDLE {
            return Err(ClientError::ProtocolMismatch(
                "Component package too large".into(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let bundle: Bundle = serde_json::from_slice(&bytes)?;
    bundle.validate().map_err(ClientError::ProtocolMismatch)?;
    Ok(bundle)
}
