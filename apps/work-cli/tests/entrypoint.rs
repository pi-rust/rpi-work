//! These tests execute only help and invalid input. They never run serve,
//! connect to a node, or invoke a real rpi executable.
use std::process::{Command, Output};

fn cli(args: &[&str], transport: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_work-cli"));
    command
        .args(args)
        .env_remove("RPI_WORK_URL")
        .env_remove("RPI_WORK_TRANSPORT")
        .env_remove("RPI_WORK_PASSWORD")
        // This must never be executed; unknown native commands fail in parsing.
        .env("RPI_WORK_RPI_BIN", "DO_NOT_SPAWN_REAL_RPI_INVALID_BINARY");
    if let Some(transport) = transport {
        command.env("RPI_WORK_TRANSPORT", transport);
    }
    command.output().expect("run CLI")
}

#[test]
fn help_and_no_arguments_have_no_connection_or_spawn() {
    for transport in [None, Some("http"), Some("rpc"), Some("acp")] {
        for args in [&[][..], &["--help"][..], &["help"][..]] {
            let output = cli(args, transport);
            assert!(output.status.success());
            assert_eq!(output.stderr, [] as [u8; 0]);
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains("13031"));
            assert!(stdout.contains("never starts an agent"));
        }
    }
}

#[test]
fn invalid_commands_and_missing_parameters_fail_in_parser() {
    for transport in [None, Some("rpc"), Some("acp")] {
        for args in [
            &["definitely-unknown"][..],
            &["prompt"][..],
            &["set-config", "option"][..],
        ] {
            let output = cli(args, transport);
            assert_eq!(output.status.code(), Some(2));
            assert_eq!(output.stdout, [] as [u8; 0]);
            assert!(
                !String::from_utf8_lossy(&output.stderr)
                    .contains("native compatibility operation failed")
            );
        }
    }
}

#[test]
fn invalid_target_and_json_do_not_echo_sensitive_arguments() {
    for args in [
        &[
            "--url",
            "http://user:secret-password@example.org/private",
            "projects",
        ][..],
        &["dialog-reply", "session", "request", "secret-password"][..],
        &["--password", "secret-password", "auth"][..],
    ] {
        let output = cli(args, None);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stdout, [] as [u8; 0]);
        assert!(!String::from_utf8_lossy(&output.stderr).contains("secret-password"));
    }
}

#[test]
fn unknown_transport_is_an_explicit_error() {
    let output = cli(&["health"], Some("typo"));
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("RPI_WORK_TRANSPORT"));
}
