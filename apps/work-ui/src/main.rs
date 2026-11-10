#![allow(
    clippy::assertions_on_constants,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::clone_on_copy,
    clippy::collapsible_if,
    clippy::doc_markdown,
    clippy::fn_params_excessive_bools,
    clippy::if_not_else,
    clippy::implicit_clone,
    clippy::map_unwrap_or,
    clippy::match_wild_err_arm,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::redundant_closure,
    clippy::semicolon_if_nothing_returned,
    clippy::single_match_else,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::unnested_or_patterns,
    clippy::unnecessary_lazy_evaluations,
    clippy::unused_async
)]
//! Application entry point. Runtime coordination lives in app; screens in features.
mod app;
mod features;
mod shared;

fn main() {
    app::launch();
}
