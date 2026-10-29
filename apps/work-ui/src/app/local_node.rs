//! Desktop RPC workspace ownership. Mobile and Web only connect remotely.
pub(crate) const LOCAL_ADDRESS: &str = "rpi-work://local";

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
mod native {
    use std::sync::OnceLock;
    use work_client::WorkspaceClient;
    use work_node::local::LocalRuntime;

    pub(super) static CONNECTION: OnceLock<Result<WorkspaceClient, String>> = OnceLock::new();

    pub(crate) struct LocalGuard(LocalRuntime);

    pub(crate) fn start() -> Option<LocalGuard> {
        let result = LocalRuntime::prepare().and_then(|runtime| {
            let client = WorkspaceClient::local_rpc(runtime.rpi.clone(), runtime.data.clone())
                .map_err(|error| error.to_string())?;
            Ok((runtime, client))
        });
        match result {
            Ok((runtime, client)) => {
                let _ = CONNECTION.set(Ok(client));
                Some(LocalGuard(runtime))
            }
            Err(error) => {
                let _ = CONNECTION.set(Err(error));
                None
            }
        }
    }

    impl LocalGuard {
        /// Never block an executor on itself. The shutdown thread owns the data
        /// lock until every core child has stopped. Bound the event-loop wait;
        /// on timeout the thread retains the lock rather than releasing it early.
        pub(crate) fn shutdown(self) {
            let client = super::client();
            let (done, completed) = std::sync::mpsc::sync_channel(1);
            let guard = std::sync::Arc::new(std::sync::Mutex::new(Some(self)));
            let owned_guard = guard.clone();
            let spawned = std::thread::Builder::new()
                .name("workspace-shutdown".into())
                .spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build();
                    if let Ok(runtime) = runtime {
                        runtime.block_on(async {
                            if let Some(client) = client {
                                client.shutdown().await;
                            }
                        });
                        // Explicitly release the LocalRuntime only after shutdown.
                        if let Some(LocalGuard(lock)) = owned_guard.lock().unwrap().take() {
                            drop(lock);
                        }
                        let _ = done.send(());
                    } else {
                        // No executor means shutdown could not be confirmed.
                        std::mem::forget(owned_guard);
                    }
                });
            if spawned.is_err() {
                std::mem::forget(guard);
                eprintln!("Unable to start workspace shutdown; retaining data lock until exit");
                return;
            }
            if completed
                .recv_timeout(std::time::Duration::from_secs(10))
                .is_err()
            {
                eprintln!(
                    "Workspace shutdown exceeded 10 seconds; data lock retained until cleanup or exit"
                );
            }
        }
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub(crate) use native::start;

pub(crate) fn client() -> Option<work_client::WorkspaceClient> {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        native::CONNECTION
            .get()
            .and_then(|result| result.as_ref().ok())
            .cloned()
    }
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    {
        None
    }
}

pub(crate) fn address() -> Option<&'static str> {
    client().map(|_| LOCAL_ADDRESS)
}

pub(crate) fn is_local(url: &str) -> bool {
    url.trim().trim_end_matches('/') == LOCAL_ADDRESS
}

// Existing management callers need no authentication for the owned workspace.
pub(crate) fn authorize(
    _client: &work_client::WorkspaceClient,
    _url: &str,
) -> std::future::Ready<Result<(), work_client::ClientError>> {
    std::future::ready(Ok(()))
}

pub(crate) fn startup_error() -> Option<&'static str> {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        native::CONNECTION
            .get()
            .and_then(|result| result.as_ref().err())
            .map(String::as_str)
    }
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn local_identity_is_stable_without_runtime_startup() {
        assert!(super::is_local("rpi-work://local"));
        assert!(super::is_local("rpi-work://local/"));
        assert!(!super::is_local("http://localhost:3030"));
        assert!(!super::is_local("rpi-work://local/other"));
    }
}
