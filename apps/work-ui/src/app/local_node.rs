//! Platform capability adapter. Only an owning desktop launches a local host.
pub(crate) const LOCAL_ADDRESS: &str = "rpi-work://local";

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
static REQUESTED_ADDRESS: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Set before launching Dioxus. Explicit remote mode never touches the local
/// data directory, tries its lock, or starts another owner as a fallback.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub(crate) fn set_requested_address(address: String) {
    let _ = REQUESTED_ADDRESS.set(address);
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub(crate) fn requested_address() -> Option<&'static str> {
    REQUESTED_ADDRESS.get().map(String::as_str)
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub(crate) fn start() -> Option<work_host::LocalGuard> {
    if requested_address().is_some_and(|address| !is_local(address)) {
        None
    } else {
        work_host::start()
    }
}

pub(crate) fn client() -> Option<work_client::WorkspaceClient> {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        work_host::client()
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
        work_host::startup_error()
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
