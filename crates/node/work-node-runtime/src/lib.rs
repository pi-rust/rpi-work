//! Shared workspace ownership and native runtime resolution.
#![allow(clippy::missing_errors_doc)]
//!
//! Native GUI/headless hosts retain `local::LocalRuntime` around the same RPC
//! workspace. `work-node-host` attaches the optional in-process HTTP adapter;
//! this crate does not start that listener or supervise a standalone HTTP node.

pub mod desktop;
pub mod local;
#[cfg(not(target_arch = "wasm32"))]
pub mod runtime;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub mod workspace;
