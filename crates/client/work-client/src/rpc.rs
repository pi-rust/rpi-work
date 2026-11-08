#![cfg(not(target_arch = "wasm32"))]

//! Compatibility facade for the native `rpi --mode rpc` runtime.

use std::path::{Path, PathBuf};

use serde_json::Value;
use work_core::AgentDescriptor;

use crate::native_runtime::map_runtime_stream;
use crate::{ClientError, EventStream};

/// A live native `rpi --mode rpc` process.
#[derive(Clone)]
pub struct RpiRpcClient {
    pub(crate) runtime: work_node_runtime::runtime::RpiRpcClient,
}

impl RpiRpcClient {
    pub async fn spawn(
        executable: impl AsRef<Path>,
        cwd: impl Into<PathBuf>,
    ) -> Result<Self, ClientError> {
        work_node_runtime::runtime::RpiRpcClient::spawn(executable, cwd)
            .await
            .map(|runtime| Self { runtime })
            .map_err(ClientError::from)
    }

    #[must_use]
    pub fn descriptor(&self) -> AgentDescriptor {
        self.runtime.descriptor()
    }

    pub async fn request(&self, command: &str, payload: Value) -> Result<Value, ClientError> {
        self.runtime
            .request(command, payload)
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
