#![cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use work_client::{AgentClient, LocalWorkspace};

// Configure only a subprocess environment; parallel test runners never mutate PATH/env.
#[test]
fn workspace_manager_scenarios() {
    if std::env::var_os("WORKSPACE_FIXTURE_WORKER").is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "workspace_manager_scenarios", "--nocapture"])
            .env("WORKSPACE_FIXTURE_WORKER", "1")
            .env(
                "RPI_WORK_CODEX_ACP_BIN",
                env!("CARGO_BIN_EXE_acp-workspace-fixture"),
            )
            .env("RPI_WORK_CODEX_ACP_ARGS", "[]")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(180), scenarios())
                .await
                .unwrap();
        });
}
fn workspace(data: &Path) -> LocalWorkspace {
    LocalWorkspace::new(PathBuf::from("unused-rpi"), data.to_owned()).unwrap()
}
async fn wait_file(path: PathBuf) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
async fn prompt(
    client: AgentClient,
    id: &str,
    text: &str,
) -> Result<Value, work_client::ClientError> {
    client.request(id, "prompt", json!({"message":text})).await
}
fn sidecar(data: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(data.join("rpc/agent-sessions.json")).unwrap()).unwrap()
}
// These lifecycle steps must share one isolated process and durable workspace.
#[allow(clippy::too_many_lines)]
async fn scenarios() {
    let data = std::env::temp_dir().join(format!("acp-workspace-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let host = workspace(&data);
    let root = data.join("rpc/control");

    // Cancelling session creation rolls the durable catalog back, not just returned Err.
    std::fs::write(root.join("hold-new"), "").unwrap();
    {
        let creation = host.create_session_with_backend(None, "codex");
        tokio::pin!(creation);
        tokio::select! { result = &mut creation => panic!("unexpected creation {result:?}"), () = wait_file(root.join("new.ready")) => {} }
    }
    assert!(sidecar(&data)["records"].as_object().unwrap().is_empty());
    std::fs::remove_file(root.join("hold-new")).unwrap();
    let id = host
        .create_session_with_backend(None, "codex")
        .await
        .unwrap();
    let other = host
        .create_session_with_backend(None, "codex")
        .await
        .unwrap();
    let a = host.prepare_agent_session(&id).await.unwrap().unwrap();
    let b = host.prepare_agent_session(&other).await.unwrap().unwrap();
    let before_generation = sidecar(&data)["records"][&id]["generation"].clone();
    let direct = host
        .node_owner()
        .prepare_agent_session(&id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(direct.descriptor(), a.descriptor());
    assert_eq!(
        sidecar(&data)["records"][&id]["generation"],
        before_generation
    );
    assert!(matches!(
        direct
            .request(&other, "prompt", json!({"message":"foreign"}))
            .await,
        Err(work_node_runtime::workspace::WorkspaceError::Runtime(
            work_node_runtime::runtime::RuntimeError::Acp(_)
        ))
    ));
    assert!(matches!(
        direct.request(&id, "get_state", Value::Null).await,
        Err(work_node_runtime::workspace::WorkspaceError::Runtime(
            work_node_runtime::runtime::RuntimeError::Unsupported { .. }
        ))
    ));

    assert_eq!(
        host.session_agent_binding(&id).await.unwrap().backend,
        "codex"
    );
    assert!(
        a.request(&other, "prompt", json!({"message":"foreign"}))
            .await
            .is_err()
    );

    // Abandon the caller while the child is still in a turn; manager keeps ownership.
    let _ = std::fs::remove_file(root.join("prompt.ready"));
    {
        let turn = prompt(a.clone(), &id, "hold");
        tokio::pin!(turn);
        tokio::select! { result = &mut turn => panic!("unexpected prompt {result:?}"), () = wait_file(root.join("prompt.ready")) => {} }
    }
    assert_eq!(host.get_session(&id).await.unwrap().status, "running");
    assert!(prompt(a.clone(), &id, "overlap").await.is_err());
    prompt(b.clone(), &other, "independent").await.unwrap();
    std::fs::write(root.join("release-prompt"), "").unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while host.get_session(&id).await.unwrap().status == "running" {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();

    // Equal-looking raw JSON-RPC IDs must remain distinct throughout UI scoping.
    let mut events = a.subscribe(&id, 0).await.unwrap();
    let permission_turn = prompt(a.clone(), &id, "typed-permissions");
    tokio::pin!(permission_turn);
    let mut keys = std::collections::HashMap::new();
    while keys.len() < 2 {
        tokio::select! {
            result = &mut permission_turn => panic!("premature permission settlement {result:?}"),
            event = events.next() => {
                let event = event.unwrap().unwrap();
                if event.event_type == "ui_dialog_request" {
                    assert_eq!(event.data["ui"]["acpPermission"]["options"][0]["optionId"], "original-allow");
                    assert_eq!(event.data["requestId"], event.data["jsonRpcRequestId"]);
                    let tool = event.data["toolCallId"].as_str().unwrap().to_owned();
                    assert!(keys.insert(tool, event.data["jsonRpcRequestId"].clone()).is_none());
                }
            }
        }
    }
    let key = keys["tool-number"].clone();
    let string_key = keys["tool-string"].clone();
    assert_ne!(key, string_key);
    assert!(key.as_str().unwrap().starts_with(&format!("{id}:")));
    assert!(string_key.as_str().unwrap().starts_with(&format!("{id}:")));
    let answer = json!({"outcome":{"outcome":"selected","optionId":"original-allow"}});
    assert!(
        b.respond_to_agent_request(key.clone(), answer.clone())
            .await
            .is_err()
    );
    // Answer the string ID first: a lossy number/string lookup must not close
    // whichever matching-looking card happens to be first in a map.
    a.respond_to_agent_request(string_key.clone(), answer.clone())
        .await
        .unwrap();
    loop {
        tokio::select! {
            result = &mut permission_turn => panic!("settled with numeric permission unanswered: {result:?}"),
            event = events.next() => {
                let event = event.unwrap().unwrap();
                if event.event_type == "ui_dialog_closed" {
                    assert_eq!(event.data["requestId"], string_key);
                    assert_eq!(event.data["jsonRpcRequestId"], json!("7"));
                    break;
                }
            }
        }
    }
    assert!(
        a.respond_to_agent_request(string_key.clone(), answer.clone())
            .await
            .is_err()
    );
    // Keep polling the actual prompt future while only one permission is answered.
    tokio::select! {
        result = &mut permission_turn => panic!("settled before both permissions answered: {result:?}"),
        event = events.next() => panic!("unexpected event with numeric permission open: {event:?}"),
        () = tokio::time::sleep(Duration::from_millis(50)) => {}
    }
    a.respond_to_agent_request(key.clone(), answer.clone())
        .await
        .unwrap();
    loop {
        let event = events.next().await.unwrap().unwrap();
        if event.event_type == "ui_dialog_closed" {
            assert_eq!(event.data["requestId"], key);
            assert_eq!(event.data["jsonRpcRequestId"], json!(7));
            break;
        }
    }
    permission_turn.await.unwrap();
    assert!(
        a.respond_to_agent_request(key.clone(), answer.clone())
            .await
            .is_err()
    );

    host.shutdown().await;
    assert!(matches!(
        direct
            .request(&id, "prompt", json!({"message":"stale"}))
            .await,
        Err(work_node_runtime::workspace::WorkspaceError::Runtime(
            work_node_runtime::runtime::RuntimeError::ChildExited(_)
        ))
    ));
    // The fixture has already proven ownership, cancellation, stale handles,
    // and clean shutdown above. A second owner restart requires a real ACP
    // runtime and is covered by the dedicated lifecycle suite; do not make
    // this deterministic protocol fixture race a replacement child on CI.
    drop((a, b, direct, events));
    drop(host);
    std::fs::remove_dir_all(data).unwrap();
}
