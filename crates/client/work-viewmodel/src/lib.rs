//! Renderer-independent presentation logic shared by every RPI Work client.
//!
//! These items were crate-private display helpers inside `work-ui`. They are
//! public here so a second renderer can reuse them, which turns the pedantic
//! "document every public item" lints into documentation churn on moved code;
//! the same exceptions `work-ui` keeps for these modules apply.
#![allow(
    clippy::cast_possible_truncation,
    clippy::map_unwrap_or,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::struct_excessive_bools,
    clippy::too_many_lines
)]
//!
//! Nothing in this crate knows about a widget toolkit: no `dioxus`, no HTML,
//! no DOM, no CSS. It owns the rules that decide *what* a screen shows —
//! transcript ordering, tool-call formatting, theme tokens, history paging and
//! request shaping — so a second renderer (for example a native desktop shell)
//! can reuse them instead of copying behaviour out of the existing UI crate.
//!
//! Widgets, hooks, layout and platform glue stay in `work-ui`.

pub mod config_requests;
pub mod conversation;
pub mod qr;
pub mod resources;
pub mod theme;
