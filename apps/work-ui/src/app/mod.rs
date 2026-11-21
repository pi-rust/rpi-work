mod background;
mod bootstrap;
pub(crate) use work_viewmodel::config_requests;
#[cfg(any(target_os = "android", test))]
pub(crate) use work_viewmodel::connection::auth as connection_auth;
#[cfg(any(target_os = "android", test))]
pub(crate) use work_viewmodel::connection::memory as connection_memory;
#[cfg(target_os = "android")]
mod connection_memory_android;
pub(crate) mod connections;
mod continuation;
pub(crate) mod desktop_background;
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
mod desktop_launch;
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub(crate) mod desktop_setup;
pub(crate) mod device_mode;
pub(crate) mod entry;
pub(crate) use work_viewmodel::transcript as history;
pub(crate) mod identity;
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub(crate) mod local_http;
pub(crate) mod local_node;
#[cfg(any(target_os = "android", test))]
pub(crate) mod mobile_entry;
pub(crate) mod node_navigation;
pub(crate) use work_viewmodel::qr as qr_codec;
mod runtime;
mod sidebar_resize;
pub(crate) use work_viewmodel::transcript as transcript_order;
mod workspace;
mod workspace_changes;
mod workspace_connection;
mod workspace_home;
mod workspace_protocol;
mod workspace_sessions;
mod workspace_sync;
pub use bootstrap::launch;
