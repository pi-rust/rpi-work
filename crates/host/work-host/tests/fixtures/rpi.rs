//! Runtime bootstrap fixture. Never invokes a model or reads a user's config.
use std::io::{self, BufRead, Write};

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let mode = std::env::var("RPI_SETUP_FIXTURE_MODE").unwrap_or_default();
    if arguments.iter().any(|argument| argument == "--version") {
        if mode == "version-hang" {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
        println!(
            "{}",
            if mode == "invalid-version" {
                "unrelated-tool 1.2.3"
            } else {
                "rpi 0.24.0"
            }
        );
        return;
    }
    assert!(
        arguments
            .windows(2)
            .any(|arguments| arguments == ["--mode", "rpc"])
    );
    assert!(
        arguments
            .iter()
            .any(|argument| argument == "--no-extensions")
    );
    assert_eq!(std::env::var("RPI_OFFLINE").as_deref(), Ok("1"));
    let directory = std::env::current_dir().unwrap();
    assert_eq!(
        std::env::var_os("RPI_CODING_AGENT_DIR").unwrap(),
        directory.join("agent")
    );
    assert!(directory.join("agent/models.json").is_file());
    if let Ok(marker) = std::env::var("RPI_SETUP_FIXTURE_MARKER") {
        std::fs::write(marker, directory.to_string_lossy().as_bytes()).unwrap();
    }
    if mode == "rpc-hang" {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
    if mode == "invalid-rpc" {
        println!(r#"{{"type":"ready","protocol":"other","supportedCommands":[]}}"#);
    } else {
        println!(
            r#"{{"type":"ready","protocol":"rpi","supportedCommands":["prompt","get_state"]}}"#
        );
    }
    io::stdout().flush().unwrap();
    for _ in io::stdin().lock().lines() {}
}
