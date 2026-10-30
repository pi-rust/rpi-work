//! Keep Codex's built-in CSS identical to the Rust/portable theme tokens.
use std::{env, fs, path::PathBuf};

#[allow(dead_code)]
#[path = "../src/shared/theme/theme_contract.rs"]
mod theme_contract;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/shared/theme/codex.css");
    let css = theme_contract::codex_stylesheet();
    if env::args().any(|argument| argument == "--check") {
        if fs::read_to_string(&path)? != css {
            return Err(
                "Codex CSS is stale; run cargo run -p work-ui --example sync-codex-theme".into(),
            );
        }
        println!("PASS Codex CSS matches Rust theme tokens");
    } else {
        fs::write(&path, css)?;
        println!("Generated {}", path.display());
    }
    Ok(())
}
