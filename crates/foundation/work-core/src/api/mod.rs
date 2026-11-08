//! Compatibility exports of the shared node API; projection stays client-owned.
pub use work_node_api::api::*;
pub mod events;
pub mod tool_program;
pub use tool_program::*;
