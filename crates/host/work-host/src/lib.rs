//! Work owns its local workspace; HTTP is an optional transport over that owner.
pub use work_node_host::{
    LaunchOverrides, LocalGuard, StartupFailure, StartupFailureKind, report_error, run_headless,
    start, startup_error, startup_failure, try_start_with,
};
pub mod http;
pub mod setup;

/// Return the same local owner used by the optional public HTTP listener.
#[must_use]
pub fn client() -> Option<work_client::WorkspaceClient> {
    work_node_host::client().map(|owner| {
        work_client::WorkspaceClient::Local(std::sync::Arc::new(
            work_client::LocalWorkspace::from_node_owner(owner),
        ))
    })
}
