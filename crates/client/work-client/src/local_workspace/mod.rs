//! Local access facade: owns no workspace state, process or client projection.
use crate::{AuthState, Capabilities, ClientError, Health};
use std::{path::PathBuf, sync::Arc};
use work_core::AgentDescriptor;
pub use work_node_runtime::workspace::LocalError;
use work_node_runtime::workspace::LocalWorkspace as NodeWorkspace;

mod agents;
mod commands;
mod node_api;
mod projects;
mod runtime;
mod sessions;

/// Cloneable local access to the same node workspace used by the HTTP server.
#[derive(Clone)]
pub struct LocalWorkspace {
    owner: Arc<NodeWorkspace>,
}
impl LocalWorkspace {
    /// Wrap an existing owner without initializing another workspace or host.
    #[must_use]
    pub fn from_node_owner(owner: Arc<NodeWorkspace>) -> Self {
        Self { owner }
    }
    /// Share the exact owner with other node access adapters.
    #[must_use]
    pub fn node_owner(&self) -> Arc<NodeWorkspace> {
        self.owner.clone()
    }
    pub fn new(executable: PathBuf, data: PathBuf) -> Result<Self, ClientError> {
        NodeWorkspace::new(executable, data)
            .map(|owner| Self::from_node_owner(Arc::new(owner)))
            .map_err(Into::into)
    }
    pub fn with_session_dir(
        executable: PathBuf,
        data: PathBuf,
        session_dir: Option<PathBuf>,
    ) -> Result<Self, ClientError> {
        NodeWorkspace::with_session_dir(executable, data, session_dir)
            .map(|owner| Self::from_node_owner(Arc::new(owner)))
            .map_err(Into::into)
    }
    #[must_use]
    pub fn descriptor(&self) -> AgentDescriptor {
        self.owner.descriptor()
    }
    pub async fn health(&self) -> Result<Health, ClientError> {
        self.owner.health().await.map_err(Into::into)
    }
    pub async fn auth(&self) -> Result<AuthState, ClientError> {
        self.owner.auth().await.map_err(Into::into)
    }
    pub async fn login(&self, password: &str) -> Result<AuthState, ClientError> {
        self.owner.login(password).await.map_err(Into::into)
    }
    pub async fn logout(&self) -> Result<AuthState, ClientError> {
        self.owner.logout().await.map_err(Into::into)
    }
    pub async fn capabilities(&self) -> Result<Capabilities, ClientError> {
        self.owner.capabilities().await.map_err(Into::into)
    }
}
