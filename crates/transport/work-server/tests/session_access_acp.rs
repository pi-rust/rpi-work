#![cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
//! Production HTTP/SSE + owner, using the existing credential-free ACP child.
//! Build once: cargo build -p work-client --bin acp-fixture --offline
//! `WORK_SESSION_ACCESS_ACP_FIXTURE` may override the sibling binary location.
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Duration};
use work_client::{LocalWorkspace, WorkClient, WorkspaceClient};
use work_server::{HttpServer, ServerConfig};

#[test]
fn production_acp_access_dialogs_and_unified_sse() {
    if std::env::var_os("WORK_ACCESS_HTTP_WORKER").is_none() {
        let executable = std::env::current_exe().unwrap();
        let fixture = std::env::var_os("WORK_SESSION_ACCESS_ACP_FIXTURE").map_or_else(
            || {
                executable
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join(format!("acp-fixture{}", std::env::consts::EXE_SUFFIX))
            },
            PathBuf::from,
        );
        assert!(
            fixture.is_file(),
            "build existing acp-fixture first: {}",
            fixture.display()
        );
        assert!(
            std::process::Command::new(executable)
                .args([
                    "--exact",
                    "production_acp_access_dialogs_and_unified_sse",
                    "--nocapture"
                ])
                .env("WORK_ACCESS_HTTP_WORKER", "1")
                .env("RPI_WORK_CODEX_ACP_BIN", fixture)
                .env(
                    "RPI_WORK_CODEX_ACP_ARGS",
                    r#"["--wait-permission","--load-session"]"#
                )
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
            tokio::task::LocalSet::new()
                .run_until(async {
                    Box::pin(tokio::time::timeout(Duration::from_secs(50), scenario()))
                        .await
                        .unwrap();
                })
                .await;
        });
}

async fn next_dialog(connection: &mut work_client::SessionConnection) -> String {
    loop {
        if let Some(dialog) = connection.pending_dialogs().first() {
            return dialog.request_id.clone();
        }
        connection.next().await.unwrap().unwrap();
    }
}

#[allow(clippy::too_many_lines)]
async fn scenario() {
    let root = tempfile::tempdir().unwrap();
    let local = Arc::new(
        LocalWorkspace::new(PathBuf::from("unused-rpi"), root.path().join("data")).unwrap(),
    );
    let owner = local.node_owner();
    let native = WorkspaceClient::Local(local.clone());
    let id = local
        .create_session_with_backend(None, "codex")
        .await
        .unwrap();
    let other = local
        .create_session_with_backend(None, "codex")
        .await
        .unwrap();
    let mut server = HttpServer::start(
        owner.clone(),
        ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            password: "fixture-password".into(),
            allowed_hosts: vec![],
            node_id: "access-acp-fixture".into(),
            assets: None,
            web_proxy: false,
            allow_sharing: true,
        },
    )
    .unwrap();
    let base = format!("http://{}/", server.address());
    let http = WorkClient::new(&base).unwrap();
    let remote = WorkspaceClient::Remote(http.clone());
    assert!(remote.session_access(&id).await.is_err());
    http.login("fixture-password").await.unwrap();
    let access = remote.session_access(&id).await.unwrap();
    assert_eq!(access, native.session_access(&id).await.unwrap());
    assert_eq!(access.supported_commands, ["prompt", "abort", "cancel"]);
    assert!(!access.snapshot_limit);
    assert!(access.dialog_reply);
    let raw = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(
            [(
                reqwest::header::HeaderName::from_static("x-rpi-web"),
                reqwest::header::HeaderValue::from_static("1"),
            )]
            .into_iter()
            .collect(),
        )
        .build()
        .unwrap();
    raw.post(format!("{base}api/v1/auth"))
        .json(&json!({"password":"fixture-password"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let endpoint = format!("{base}api/v1/sessions/{id}/dialogs");
    let access_url = format!("{base}api/v1/sessions/{id}/access");
    let anonymous = reqwest::Client::new();
    assert_eq!(
        anonymous
            .post(&endpoint)
            .header("x-rpi-web", "1")
            .json(&json!({"requestId":"x","cancelled":true}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        raw.get(&access_url)
            .header("Origin", "https://foreign.invalid")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        raw.post(&endpoint)
            .header("Origin", "https://foreign.invalid")
            .json(&json!({"requestId":"x","cancelled":true}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );

    let mut stream = remote.connect_session(id.clone()).await.unwrap();
    let initial = native.connect_session(id.clone()).await.unwrap();
    assert_eq!(stream.messages(), initial.messages());
    drop(initial);
    for cancelled in [false, true] {
        let prompt_client = remote.clone();
        let prompt_id = id.clone();
        let prompt = tokio::task::spawn_local(async move {
            prompt_client.prompt(&prompt_id, "permission").await
        });
        let request_id = next_dialog(&mut stream).await;
        let snapshot = remote.connect_session(id.clone()).await.unwrap();
        assert_eq!(snapshot.pending_dialogs(), stream.pending_dialogs());
        drop(snapshot);
        let answer = json!({"kind":"selector","values":["Allow"]});
        assert!(
            remote
                .dialog_response(&other, &request_id, answer.clone())
                .await
                .is_err()
        );
        for payload in [
            json!({"requestId":request_id,"answer":{"kind":"selector","values":["foreign"]}}),
            json!({"requestId":request_id,"answer":{"kind":"selector","values":[]}}),
            json!({"requestId":request_id,"answer":{"kind":"selector","values":["Allow","Reject"]}}),
            json!({"requestId":request_id,"answer":{"outcome":{"outcome":"selected","optionId":"grant-42"}}}),
            json!({"requestId":request_id,"cancelled":true,"responseId":4}),
            json!({"requestId":request_id,"result":{"outcome":{"outcome":"cancelled"}}}),
        ] {
            assert_eq!(
                raw.post(&endpoint)
                    .json(&payload)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                400,
                "{payload}"
            );
        }
        assert!(
            !prompt.is_finished(),
            "invalid input must not silently cancel/allow"
        );
        if cancelled {
            remote.cancel_dialog(&id, &request_id).await.unwrap();
        } else {
            remote
                .dialog_response(&id, &request_id, answer.clone())
                .await
                .unwrap();
        }
        prompt.await.unwrap().unwrap(); // fixture validates original wire ID and exact option/cancel result
        while !stream.pending_dialogs().is_empty() {
            stream.next().await.unwrap().unwrap();
        }
        assert!(
            remote
                .dialog_response(&id, &request_id, answer.clone())
                .await
                .is_err()
        );
        assert!(remote.cancel_dialog(&id, &request_id).await.is_err());
        let observed = remote.connect_session(id.clone()).await.unwrap();
        let direct = http.connect_session(id.clone()).await.unwrap();
        assert_eq!(observed.messages(), direct.messages());
        assert_eq!(observed.pending_dialogs(), direct.pending_dialogs());
        drop(direct);
        let local_observed = native.connect_session(id.clone()).await.unwrap();
        assert_eq!(observed.messages(), local_observed.messages());
        assert_eq!(observed.pending_dialogs(), []);
        drop((observed, local_observed));
        local.suspend().await;
        // Old generation is invalid after prepare/load, despite a reused wire permission ID.
        remote.session_access(&id).await.unwrap();
        assert!(remote.cancel_dialog(&id, &request_id).await.is_err());
        stream = remote.connect_session(id.clone()).await.unwrap();
    }
    server.sharing_control().set_enabled(false);
    assert_eq!(raw.get(&access_url).send().await.unwrap().status(), 403);
    assert_eq!(
        raw.post(&endpoint)
            .json(&json!({"requestId":"x","cancelled":true}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert!(native.session_access(&id).await.is_ok());
    drop(stream);
    server.drain().await.unwrap();
    local.shutdown().await;
}
