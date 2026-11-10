#![cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
//! Uses only the test ACP child in a subprocess-scoped environment.
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Duration};
use work_client::{ClientError, LocalError, LocalWorkspace, WorkspaceClient};

#[test]
fn acp_configuration_is_owned_bound_and_validated_at_the_node() {
    if std::env::var_os("WORK_CONFIG_FIXTURE_WORKER").is_none() {
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "acp_configuration_is_owned_bound_and_validated_at_the_node",
                    "--nocapture"
                ])
                .env("WORK_CONFIG_FIXTURE_WORKER", "1")
                .env(
                    "RPI_WORK_CODEX_ACP_BIN",
                    env!("CARGO_BIN_EXE_acp-workspace-fixture")
                )
                .env("RPI_WORK_CODEX_ACP_ARGS", "[]")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(30), scenario())
                .await
                .unwrap();
        });
}

#[allow(clippy::too_many_lines)] // Keep one isolated owner's prepare/config/restore/shutdown proof together.
async fn scenario() {
    let root = std::env::temp_dir().join(format!("work-config-fixture-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let local =
        Arc::new(LocalWorkspace::new(PathBuf::from("unused-rpi"), root.join("data")).unwrap());
    let node = local.node_owner();
    let client = WorkspaceClient::Local(local.clone());
    let id = local
        .create_session_with_backend(None, "codex")
        .await
        .unwrap();
    let other = local
        .create_session_with_backend(None, "codex")
        .await
        .unwrap();
    let observation = client.session_configuration(&id).await.unwrap();
    assert_eq!(observation, node.session_configuration(&id).await.unwrap());
    assert_eq!(observation.protocol, "acp");
    assert_eq!(observation.supported_commands, Vec::<String>::new());
    assert_eq!(observation.config_options[0]["id"], "mode-choice");
    assert_eq!(observation.config_options[0]["currentValue"], "balanced");
    assert_eq!(
        observation.config_options[0]["options"][0]["options"][1]["value"],
        "fast"
    );
    let handle = local.prepare_agent_session(&id).await.unwrap().unwrap();
    let result = client
        .set_session_config_option(&id, "mode-choice", json!("fast"))
        .await
        .unwrap();
    assert!(result["fixturePid"].is_u64());
    let same_runtime = handle
        .set_config_option(&id, "mode-choice", json!("fast"))
        .await
        .unwrap();
    assert_eq!(result["fixturePid"], same_runtime["fixturePid"]);
    // Preparing configuration access does not replace the existing runtime handle.
    assert_eq!(
        handle.session_config_options(&id)[0]["currentValue"],
        "fast"
    );
    assert_eq!(
        node.session_configuration(&id)
            .await
            .unwrap()
            .config_options[0]["currentValue"],
        "fast"
    );
    assert_eq!(
        client
            .session_configuration(&other)
            .await
            .unwrap()
            .config_options[0]["currentValue"],
        "balanced"
    );
    assert!(matches!(
        client
            .set_session_config_option(&id, "unknown", json!("fast"))
            .await,
        Err(ClientError::Unsupported { .. })
    ));
    assert!(matches!(
        client
            .set_session_config_option(&id, "mode-choice", json!("foreign"))
            .await,
        Err(ClientError::Acp(_))
    ));
    assert!(matches!(
        client
            .set_session_config_option(&id, "mode-choice", json!(7))
            .await,
        Err(ClientError::Acp(_))
    ));
    assert!(matches!(
        client.session_configuration("missing").await,
        Err(ClientError::Local(LocalError::NotFound(_)))
    ));
    // Service command white-list remains unchanged: config is a separate typed operation.
    assert!(
        local
            .command::<serde_json::Value>(&id, json!({"type":"set_model"}))
            .await
            .is_err()
    );
    // Owner-wide ACP suspend/restart is covered by the dedicated lifecycle
    // tests. Keep this contract test focused on node-owned configuration and
    // avoid racing a live ACP child with the service teardown.
    local.shutdown().await;
    assert!(matches!(
        client.session_configuration(&id).await,
        Err(ClientError::Local(LocalError::Shutdown))
    ));
    assert!(matches!(
        client
            .set_session_config_option(&id, "mode-choice", json!("fast"))
            .await,
        Err(ClientError::Local(LocalError::Shutdown))
    ));
    drop((handle, node, client, local));
    std::fs::remove_dir_all(root).unwrap();
}
