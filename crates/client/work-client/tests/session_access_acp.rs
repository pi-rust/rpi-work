#![cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
//! No credentials, real agents, global env mutation or `AgentClient` UI path.
use std::{path::PathBuf, sync::Arc, time::Duration};
use work_client::{LocalWorkspace, WorkspaceClient};

#[test]
fn acp_unified_connect_and_restore() {
    if std::env::var_os("WORK_ACCESS_FIXTURE_WORKER").is_none() {
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "acp_unified_connect_and_restore", "--nocapture"])
                .env("WORK_ACCESS_FIXTURE_WORKER", "1")
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
            tokio::time::timeout(Duration::from_secs(40), scenario())
                .await
                .unwrap();
        });
}

async fn scenario() {
    let root = std::env::temp_dir().join(format!("work-access-fixture-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let local =
        Arc::new(LocalWorkspace::new(PathBuf::from("unused-rpi"), root.join("data")).unwrap());
    let client = WorkspaceClient::Local(local.clone());
    let id = local
        .create_session_with_backend(None, "codex")
        .await
        .unwrap();
    let access = client.session_access(&id).await.unwrap();
    assert_eq!(
        access,
        local.node_owner().session_access(&id).await.unwrap()
    );
    assert!(!access.snapshot_limit);
    assert!(!access.supports_command("get_messages"));
    // Strict ACP command validation rejects get_messages: successful connect and
    // restore thus prove neither path needs native pagination commands.
    client.prompt(&id, "fresh").await.unwrap();
    let connection = client.connect_session(id.clone()).await.unwrap();
    let before = connection.messages().to_vec();
    assert_ne!(before, [] as [serde_json::Value; 0]);
    assert!(
        !serde_json::to_string(&before)
            .unwrap()
            .contains("HISTORIC-REPLAY")
    );
    let second = client.connect_session(id.clone()).await.unwrap();
    assert_eq!(second.messages(), before);
    assert!(
        !serde_json::to_string(second.messages())
            .unwrap()
            .contains("historical-tool")
    );
    drop(second);
    // A second unified connect restores the persisted snapshot without relying
    // on an ACP child restart. Owner-wide suspend/restart is covered by the
    // dedicated lifecycle tests; this test focuses on the ACP access contract.
    local.shutdown().await;
    drop((connection, client, local));
    std::fs::remove_dir_all(root).unwrap();
}
