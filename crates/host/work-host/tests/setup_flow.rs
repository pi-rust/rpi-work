#![cfg(feature = "test-fixtures")]
use std::{path::Path, process::Command, time::Duration};
use work_host::setup::{DesktopSetup, Phase, Selection};

fn child(mode: &str) {
    let directory = tempfile::tempdir().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "setup_worker", "--nocapture"])
        .env("RPI_SETUP_TEST_MODE", mode)
        .env("RPI_SETUP_TEST_ROOT", directory.path())
        .env("HOME", directory.path())
        .env("USERPROFILE", directory.path())
        .env("LOCALAPPDATA", directory.path())
        .env("XDG_DATA_HOME", directory.path())
        .env("CARGO_HOME", directory.path().join("cargo"))
        .env("RPI_WORK_RPI_BIN", directory.path().join("missing-rpi"))
        .env("RPI_WORK_DATA_DIR", directory.path().join("node"))
        .env("RPI_CODING_AGENT_DIR", directory.path().join("agent"))
        .env("RPI_WORK_HTTP_BIND", "127.0.0.1")
        .env("RPI_WORK_HTTP_PORT", "0")
        .env_remove("RPI_WORK_HTTP_PASSWORD")
        .env_remove("RPI_HTTP_PASSWORD")
        .env_remove("RPI_WORK_HTTP_PUBLIC_URL")
        .env_remove("RPI_WORK_HTTP_ASSETS")
        .env_remove("RPI_WORK_HTTP_ALLOWED_HOSTS")
        .env(
            "RPI_SETUP_FIXTURE_MODE",
            match mode {
                "incompatible" => "invalid-rpc",
                "cancel" => "rpc-hang",
                _ => "",
            },
        )
        .env(
            "RPI_SETUP_FIXTURE_MARKER",
            directory.path().join("probe-location"),
        );
    if mode == "missing" {
        command
            .env_remove("RPI_WORK_RPI_BIN")
            .env("PATH", directory.path());
    }
    if mode == "download" {
        command.env_remove("RPI_WORK_RPI_BIN");
        command
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());
        #[cfg(windows)]
        {
            let system = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap());
            command.env(
                "PATH",
                std::env::join_paths([system.join("System32"), system]).unwrap(),
            );
        }
        #[cfg(not(windows))]
        command.env("PATH", "/usr/bin:/bin");
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "worker {mode}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn missing_runtime_waits_for_an_explicit_choice() {
    child("missing");
}

#[test]
fn invalid_override_can_be_repaired_and_reused_without_restart() {
    child("repair");
}

#[test]
fn incompatible_runtime_never_publishes_an_owner() {
    child("incompatible");
}

#[test]
fn cancellation_terminates_the_probe_before_removing_its_directory() {
    child("cancel");
}

#[test]
fn failed_host_preparation_is_retryable_and_never_creates_two_owners() {
    child("retry-host");
}

#[test]
#[ignore = "downloads the official release into an isolated user directory"]
fn official_release_download_bootstraps_without_restart() {
    child("download");
}

#[test]
#[ignore = "requires RPI_SETUP_TEST_EXECUTABLE pointing to a verified real rpi"]
fn selected_official_runtime_passes_isolated_preflight() {
    assert!(std::env::var_os("RPI_SETUP_TEST_EXECUTABLE").is_some());
    child("real");
}

#[test]
fn setup_worker() {
    let Ok(mode) = std::env::var("RPI_SETUP_TEST_MODE") else {
        return;
    };
    let root = std::path::PathBuf::from(std::env::var_os("RPI_SETUP_TEST_ROOT").unwrap());
    let executable = if mode == "real" {
        std::path::PathBuf::from(std::env::var_os("RPI_SETUP_TEST_EXECUTABLE").unwrap())
    } else {
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_setup-rpi-fixture"))
    };
    if mode == "retry-host" {
        for _ in 0..2 {
            let error = work_host::try_start_with(work_host::LaunchOverrides {
                rpi_bin: Some(root.join("missing")),
                ..Default::default()
            })
            .err()
            .unwrap();
            assert!(error.to_string().contains("RPI_WORK_RPI_BIN"), "{error}");
        }
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let setup = DesktopSetup::new();
        if mode == "missing" {
            let application = std::env::current_exe().unwrap();
            // The desktop lookup can still find a Homebrew or bundle-sibling runtime.
            if work_node_runtime::desktop::find_runtime("rpi", None, application.parent().unwrap())
                .is_some()
            {
                eprintln!("Skipping missing-runtime check because a local rpi is available");
                return;
            }
            let installation = work_update::native::rpi_runtime::install_dir().unwrap();
            for _ in 0..2 {
                setup.begin(Selection::Automatic);
                wait(&setup, Phase::NeedsRuntime).await;
                let status = setup.snapshot();
                assert!(!status.busy() && status.can_install);
                assert!(!installation.exists());
                assert!(work_host::client().is_none());
            }
            setup.shutdown();
            return;
        }
        if mode == "repair" {
            setup.begin(Selection::Automatic);
            wait(&setup, Phase::Failed).await;
            assert!(setup.snapshot().message.contains("missing-rpi"));
            assert!(work_host::client().is_none());
        }
        setup.begin(if mode == "download" {
            Selection::Managed
        } else {
            Selection::File(executable.clone())
        });
        // A second simultaneous request must not replace or duplicate this attempt.
        setup.begin(Selection::File(root.join("must-not-run")));
        if mode == "cancel" {
            tokio::time::timeout(Duration::from_secs(10), async {
                while !root.join("probe-location").exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            setup.cancel();
            wait(&setup, Phase::Cancelled).await;
            let probe_directory = std::fs::read_to_string(root.join("probe-location")).unwrap();
            assert!(
                !Path::new(&probe_directory).exists(),
                "cancel waits for process exit before cleanup"
            );
            assert!(work_host::client().is_none());
        } else if mode == "incompatible" {
            wait(&setup, Phase::Failed).await;
            assert!(setup.snapshot().message.contains("RPC"));
            assert!(setup.snapshot().can_install);
            assert!(work_host::client().is_none());
        } else {
            wait(&setup, Phase::Ready).await;
            let client = work_host::client().unwrap();
            assert!(client.is_local());
            client.health().await.unwrap();
            if mode == "download" {
                let status = setup.snapshot();
                assert!(semver::Version::parse(status.version.as_deref().unwrap()).is_ok());
                let binary = status.path.unwrap();
                assert!(
                    binary.starts_with(work_update::native::rpi_runtime::install_dir().unwrap())
                );
                assert!(std::fs::metadata(binary).unwrap().len() > 1_000_000);
            } else if mode == "real" {
                assert!(
                    semver::Version::parse(setup.snapshot().version.as_deref().unwrap()).is_ok()
                );
                assert!(
                    work_update::native::rpi_runtime::install_dir()
                        .unwrap()
                        .join("selection.json")
                        .exists()
                );
            } else {
                assert_eq!(setup.snapshot().version.as_deref(), Some("0.24.0"));
                assert!(
                    work_update::native::rpi_runtime::install_dir()
                        .unwrap()
                        .join("selection.json")
                        .exists()
                );
            }
            let error = work_host::try_start_with(work_host::LaunchOverrides {
                rpi_bin: Some(executable),
                ..Default::default()
            })
            .err()
            .unwrap();
            assert!(
                error.to_string().contains("only be started once"),
                "{error}"
            );
            setup.shutdown();
            assert!(client.health().await.is_err());
        }
        setup.shutdown();
    });
}

async fn wait(setup: &DesktopSetup, phase: Phase) {
    let mut updates = setup.subscribe();
    let seconds = if std::env::var("RPI_SETUP_TEST_MODE").as_deref() == Ok("download") {
        480
    } else {
        30
    };
    let result = tokio::time::timeout(Duration::from_secs(seconds), async {
        let mut previous = None;
        loop {
            let current = updates.borrow_and_update().clone();
            let observed = (
                current.phase,
                current.received.saturating_mul(4) / current.total.max(1),
            );
            if seconds > 30 && previous != Some(observed) {
                eprintln!(
                    "runtime preparation {:?}, {}/{} bytes",
                    current.phase, current.received, current.total
                );
                previous = Some(observed);
            }
            if current.phase == phase {
                return;
            }
            assert!(
                !matches!(current.phase, Phase::Failed | Phase::Cancelled),
                "{current:?}"
            );
            updates.changed().await.unwrap();
        }
    })
    .await;
    assert!(
        result.is_ok(),
        "preparation timed out: {:?}",
        setup.snapshot()
    );
}
