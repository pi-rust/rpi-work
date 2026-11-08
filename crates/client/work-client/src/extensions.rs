//! Explicit extension-management boundary for both native and remote workspaces.
//!
//! Ordinary tool discovery does not attest to plugin installation. Until an
//! actual node protocol is implemented and negotiated, do not guess endpoints,
//! scan a client's filesystem, or infer plugins from tool names.
use crate::{ClientError, WorkspaceClient, unsupported};
use work_core::extensions::{ExtensionCapabilities, NodeExtensionObservation};

impl WorkspaceClient {
    #[must_use]
    pub fn extension_capabilities(&self) -> ExtensionCapabilities {
        ExtensionCapabilities::default()
    }

    pub fn extensions(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<NodeExtensionObservation>, ClientError>> {
        std::future::ready(Err(unsupported(
            "extensions/catalog",
            &self.descriptor().identity.protocol,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unsupported_remote_extensions_do_not_make_a_network_request() {
        // No server: a speculative HTTP request would fail with Http, not Unsupported.
        let client = WorkspaceClient::new("http://127.0.0.1:1/").unwrap();
        assert!(client.descriptor().capabilities.tools);
        assert!(!client.extension_capabilities().catalog);
        assert!(matches!(
            client.extensions().await,
            Err(ClientError::Unsupported { .. })
        ));
    }

    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    #[test]
    fn local_extensions_are_not_inferred_from_rpc_tools() {
        let data =
            std::env::temp_dir().join(format!("work-extension-boundary-{}", std::process::id()));
        let client = WorkspaceClient::local_rpc("rpi".into(), data.clone()).unwrap();
        assert_eq!(
            client.extension_capabilities(),
            ExtensionCapabilities::default()
        );
        let _ = std::fs::remove_dir_all(data);
    }
}
