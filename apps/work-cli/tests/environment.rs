//! OS-level regressions: no network, serve, or real rpi process is used.
#![cfg(any(unix, windows))]

use std::{
    ffi::OsString,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
fn invalid_unicode() -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(b"private-env-marker-\xff".to_vec())
}

#[cfg(windows)]
fn invalid_unicode() -> OsString {
    use std::os::windows::ffi::OsStringExt;
    let mut wide: Vec<u16> = "private-env-marker-".encode_utf16().collect();
    wide.push(0xd800); // Unpaired surrogate: valid OS input, not Unicode.
    OsString::from_wide(&wide)
}

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_work-cli"));
    command
        .env_remove("RPI_WORK_URL")
        .env_remove("RPI_WORK_TRANSPORT")
        .env_remove("RPI_WORK_PASSWORD")
        .env("RPI_WORK_RPI_BIN", "DO_NOT_SPAWN_REAL_RPI_INVALID_BINARY");
    command
}

// Keep stdin open with no bytes: a parser rejection must not read a password.
// Bound the wait so accidental fallback to stdin/network cannot hang the suite.
fn output_before_stdin(mut command: Command) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run CLI");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().expect("poll CLI").is_some() {
            return child.wait_with_output().expect("collect CLI output");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("CLI did not reject input before stdin/network use");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn assert_parser_rejection(output: &Output) {
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, [] as [u8; 0]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("private-env-marker"));
    assert!(!stderr.contains("panicked"));
    assert!(!stderr.contains("native compatibility operation failed"));
    assert!(!stderr.contains("DO_NOT_SPAWN"));
}

#[test]
fn non_unicode_transport_never_falls_back_for_valid_or_invalid_commands() {
    for args in [
        &["--password-stdin", "auth"][..],
        &["agent-info"][..],
        &["definitely-unknown"][..],
        &["--node", "http://127.0.0.1:13031", "health"][..],
    ] {
        let mut command = command();
        command
            .args(args)
            .env("RPI_WORK_TRANSPORT", invalid_unicode());
        let output = output_before_stdin(command);
        assert_parser_rejection(&output);
        assert_eq!(
            output.stderr,
            b"RPI_WORK_TRANSPORT must be UTF-8 (input omitted)\n"
        );
    }
}

#[test]
fn non_unicode_url_never_falls_back_for_valid_or_invalid_commands() {
    for transport in [None, Some("http")] {
        for args in [
            &["--password-stdin", "auth"][..],
            &["project-create", "name"][..],
            &["definitely-unknown"][..],
            &["prompt", "s", "--", "--url"][..],
        ] {
            let mut command = command();
            command.args(args).env("RPI_WORK_URL", invalid_unicode());
            if let Some(transport) = transport {
                command.env("RPI_WORK_TRANSPORT", transport);
            }
            let output = output_before_stdin(command);
            assert_parser_rejection(&output);
            if args[0] != "definitely-unknown" {
                assert_eq!(
                    output.stderr,
                    b"RPI_WORK_URL must be UTF-8 (input omitted)\n"
                );
            }
        }
    }
}

#[test]
fn help_and_no_arguments_ignore_non_unicode_client_environment() {
    for args in [&[][..], &["help"][..], &["--help"][..], &["-h"][..]] {
        let mut command = command();
        command
            .args(args)
            .env("RPI_WORK_URL", invalid_unicode())
            .env("RPI_WORK_TRANSPORT", invalid_unicode());
        let output = output_before_stdin(command);
        assert!(output.status.success());
        assert_eq!(output.stderr, [] as [u8; 0]);
        assert!(String::from_utf8_lossy(&output.stdout).contains("never starts an agent"));
    }
}

#[test]
fn non_unicode_argv_is_redacted_instead_of_panicking_even_for_help() {
    for args in [&["--help"][..], &["health"][..]] {
        let mut command = command();
        command.args(args).arg(invalid_unicode());
        let output = output_before_stdin(command);
        assert_parser_rejection(&output);
        assert_eq!(output.stderr, b"arguments must be UTF-8 (input omitted)\n");
    }
}
