use dioxus::core::Task;
use dioxus::prelude::*;
use work_client::{AuthState, WorkspaceClient};

use super::connections::{self, ConnectionOutcome, ConnectionStore, Workspace};
use crate::shared::state;

/// Result of a successful node connection, kept separate from UI state so the
/// connection workflow does not need to know how the workspace page presents it.
pub struct ReadyConnection {
    pub workspace: Box<Workspace>,
    pub attempt: u64,
    pub url: String,
}

/// UI-owned callbacks for connection outcomes. The transport workflow stays in
/// this module; the App decides how to update its many page-specific signals.
#[derive(Clone, Copy)]
pub struct Handlers {
    pub password_required: Callback<AuthState>,
    pub ready: Callback<ReadyConnection>,
    pub failed: Callback<String>,
}

/// Start one authenticated connection attempt and discard stale epochs before
/// invoking UI callbacks. This is deliberately transport-only: it does not
/// create an owner, mutate session state, or retry an uncertain operation.
#[allow(clippy::too_many_arguments)]
pub fn finish(
    requests: Signal<Vec<Task>>,
    node_connection: ConnectionStore,
    mut shared_client: Signal<Option<WorkspaceClient>>,
    client: WorkspaceClient,
    secret: Option<String>,
    attempt: u64,
    url: String,
    handlers: Handlers,
) {
    state::scoped_spawn(requests, async move {
        if *node_connection.epoch.peek() != attempt {
            return;
        }
        shared_client.set(Some(client.clone()));
        let result = connections::open(client, secret).await;
        if *node_connection.epoch.peek() != attempt {
            return;
        }
        match result {
            Ok(ConnectionOutcome::PasswordRequired(auth)) => {
                handlers.password_required.call(auth);
            }
            Ok(ConnectionOutcome::Ready(workspace)) => {
                handlers.ready.call(ReadyConnection {
                    workspace,
                    attempt,
                    url,
                });
            }
            Err(message) => handlers.failed.call(message),
        }
    });
}
