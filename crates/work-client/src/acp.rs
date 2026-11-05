#![cfg(not(target_arch = "wasm32"))]

//! Compatibility facade for the native ACP v1 JSON-RPC runtime.

use std::path::{Path, PathBuf};

use serde_json::Value;
use work_core::AgentDescriptor;

use crate::native_runtime::map_runtime_stream;
use crate::{ClientError, EventStream};

#[derive(Clone)]
pub struct AcpClient {
    pub(crate) runtime: work_node::runtime::AcpClient,
}

impl AcpClient {
    pub async fn spawn(
        executable: impl AsRef<Path>,
        args: &[String],
        cwd: impl Into<PathBuf>,
    ) -> Result<Self, ClientError> {
        work_node::runtime::AcpClient::spawn(executable, args, cwd)
            .await
            .map(|runtime| Self { runtime })
            .map_err(ClientError::from)
    }

    #[must_use]
    pub fn descriptor(&self) -> AgentDescriptor {
        self.runtime.descriptor()
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value, ClientError> {
        self.runtime
            .request(method, params)
            .await
            .map_err(ClientError::from)
    }

    pub async fn new_session(&self, cwd: impl Into<String>) -> Result<String, ClientError> {
        self.runtime
            .new_session(cwd)
            .await
            .map_err(ClientError::from)
    }

    pub async fn load_session(&self, session_id: &str, cwd: &str) -> Result<Value, ClientError> {
        self.runtime
            .load_session(session_id, cwd)
            .await
            .map_err(ClientError::from)
    }

    pub async fn resume_session(&self, session_id: &str, cwd: &str) -> Result<Value, ClientError> {
        self.runtime
            .resume_session(session_id, cwd)
            .await
            .map_err(ClientError::from)
    }

    /// A snapshot of the full options negotiated for this session, not a catalog.
    #[must_use]
    pub fn session_config_options(&self, session_id: &str) -> Vec<Value> {
        self.runtime.session_config_options(session_id)
    }

    pub async fn set_config_option(
        &self,
        session_id: &str,
        config_id: &str,
        value: Value,
    ) -> Result<Value, ClientError> {
        self.runtime
            .set_config_option(session_id, config_id, value)
            .await
            .map_err(ClientError::from)
    }

    pub async fn prompt(&self, session_id: &str, text: &str) -> Result<Value, ClientError> {
        self.runtime
            .prompt(session_id, text)
            .await
            .map_err(ClientError::from)
    }

    pub async fn cancel(&self, session_id: &str) -> Result<Value, ClientError> {
        self.runtime
            .cancel(session_id)
            .await
            .map_err(ClientError::from)
    }

    pub async fn respond_to_agent_request(
        &self,
        request_id: Value,
        result: Value,
    ) -> Result<(), ClientError> {
        self.runtime
            .respond_to_agent_request(request_id, result)
            .await
            .map_err(ClientError::from)
    }

    pub fn subscribe_events(&self) -> Result<EventStream, ClientError> {
        self.runtime
            .subscribe_events()
            .map(map_runtime_stream)
            .map_err(ClientError::from)
    }

    pub async fn stop(&self) -> Result<(), ClientError> {
        self.runtime.stop().await.map_err(ClientError::from)
    }
}
