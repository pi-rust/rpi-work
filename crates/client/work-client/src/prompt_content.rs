//! Ordered inline prompt input; never coerce images into a legacy message.
use crate::{ClientError, HostResponse, WorkClient, WorkspaceClient};
use serde_json::{Value, json};

impl WorkClient {
    pub async fn prompt_content(
        &self,
        session_id: &str,
        content_blocks: &Value,
    ) -> Result<HostResponse<Value>, ClientError> {
        // This protects old nodes whose legacy prompt handler ignores blocks.
        // The actual owner must revalidate its live host profile and payload.
        if !self
            .session_access(session_id)
            .await?
            .supports_prompt_content()
        {
            return Err(ClientError::Unsupported {
                operation: "prompt_content".into(),
                protocol: "native ordered prompt profile".into(),
            });
        }
        self.command(
            session_id,
            json!({"type": "prompt", "contentBlocks": content_blocks}),
        )
        .await
    }
}

impl WorkspaceClient {
    pub async fn prompt_content(
        &self,
        session_id: &str,
        content_blocks: &Value,
    ) -> Result<HostResponse<Value>, ClientError> {
        match self {
            Self::Remote(client) => client.prompt_content(session_id, content_blocks).await,
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => client.prompt_content(session_id, content_blocks).await,
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
impl crate::LocalWorkspace {
    pub async fn prompt_content(
        &self,
        session_id: &str,
        content_blocks: &Value,
    ) -> Result<HostResponse<Value>, ClientError> {
        self.node_owner()
            .prompt_content(session_id, content_blocks)
            .await
            .map_err(Into::into)
    }
}
