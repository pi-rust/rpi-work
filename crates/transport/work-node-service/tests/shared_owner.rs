#[path = "../../../../tests/support/legacy_fixture.rs"]
mod legacy_fixture;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use work_client::{LocalWorkspace, WorkClient};
use work_node_service::{HttpServer, ServerConfig};

fn config() -> ServerConfig {
    ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        password: "111111".into(),
        allowed_hosts: vec![],
        node_id: "isolated-owner".into(),
        assets: None,
        web_proxy: false,
        allow_sharing: true,
    }
}
fn owner(root: &tempfile::TempDir) -> Arc<LocalWorkspace> {
    Arc::new(
        LocalWorkspace::with_session_dir(
            PathBuf::from(env!("CARGO_BIN_EXE_work-server-rpc-fixture")),
            root.path().join("data"),
            Some(root.path().join("transcripts")),
        )
        .unwrap(),
    )
}
#[tokio::test]
#[allow(clippy::too_many_lines)] // Keep the shared owner/PID and drain proof together.
async fn rpc_and_http_share_owner_and_event_replay() {
    // LocalSet is intentional: native AgentTransport futures are !Send.
    Box::pin(tokio::task::LocalSet::new()
        .run_until(async {
            let root = tempfile::tempdir().unwrap();
            let local = owner(&root);
            let node = local.node_owner();
            assert!(Arc::ptr_eq(&node, &local.clone().node_owner()));
            let mut server = HttpServer::start(node.clone(), config()).unwrap();
            let url = format!("http://{}/", server.address());
            let http = WorkClient::new(&url).unwrap();
            assert!(!http.auth().await.unwrap().authenticated);
            assert!(http.projects().await.is_err());
            assert!(http.login("111111").await.unwrap().authenticated);
            let project = local.create_project("shared", None, false).await.unwrap();
            assert_eq!(http.projects().await.unwrap(), vec![project.clone()]);
            http.rename_project(&project.id, "renamed from HTTP")
                .await
                .unwrap();
            assert_eq!(local.projects().await.unwrap()[0].name, "renamed from HTTP");
            let session = legacy_fixture::seed_legacy(&root.path().join("data"),&project.id,"legacy-owner-fixture");
            legacy_fixture::seed_legacy(&root.path().join("data"),&project.id,"legacy-owner-second");
            assert!(
                http.sessions()
                    .await
                    .unwrap()
                    .iter()
                    .any(|s| s.session_id == session)
            );
            http.rename_session(&session, "same owner").await.unwrap();
            http.session_preferences(&session, Some(true), Some(true))
                .await
                .unwrap();
            let actual = local.get_session(&session).await.unwrap();
            assert_eq!(actual.title.as_deref(), Some("same owner"));
            assert!(actual.pinned && actual.archived);
            http.session_preferences(&session, Some(false), None)
                .await
                .unwrap();
            http.open_session(&session).await.unwrap();
            let native_state = local
                .command_data::<Value>(&session, "get_state", Value::Null)
                .await
                .unwrap();
            let remote_state = http
                .command_data::<Value>(&session, "get_state", Value::Null)
                .await
                .unwrap();
            assert!(native_state["fixturePid"].is_u64());
            assert_eq!(native_state["fixturePid"], remote_state["fixturePid"]);
            let node_state = node
                .command_data::<Value>(&session, "get_state", Value::Null)
                .await
                .unwrap();
            assert_eq!(node_state["fixturePid"], native_state["fixturePid"]);
            let mut events = http.subscribe(&session, 0).await.unwrap();
            assert_eq!(events.next().await.unwrap().unwrap().event_type, "ready");
            local
                .command::<Value>(
                    &session,
                    json!({"type":"prompt","message":"isolated fixture only"}),
                )
                .await
                .unwrap();
            let mut settled_seq = 0;
            tokio::time::timeout(Duration::from_secs(5), async {
                while let Some(event) = events.next().await {
                    let event = event.unwrap();
                    if event.event_type == "agent_settled" {
                        settled_seq = event.seq.unwrap();
                        break;
                    }
                }
            })
            .await
            .unwrap();
            assert!(settled_seq > 0);
            // Both views observe the exact same event boundary, not copied files.
            let native = local
                .snapshot(&session)
                .await
                .expect("native fixture snapshot");
            let remote = http.snapshot(&session).await.unwrap();
            assert_eq!(remote.last_seq, native.last_seq);
            assert!(native.last_seq >= settled_seq);
            drop(events);
            assert!(matches!(http.create_session(Some(&project.id)).await,Err(work_client::ClientError::Api { status, .. }) if status==501));
            let created = "legacy-owner-second".to_owned();
            assert_eq!(
                local.get_session(&created).await.unwrap().session_id,
                created
            );
            server.drain().await.unwrap();
            // Draining the listener must NOT close or replace the local owner.
            assert!(
                local
                    .command::<Value>(&session, json!({"type":"get_state"}))
                    .await
                    .unwrap()
                    .success
            );
            assert!(
                local
                    .command::<Value>(&created, json!({"type":"get_state"}))
                    .await
                    .unwrap()
                    .success
            );
            local.shutdown().await;
            drop(server);
        }))
        .await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // One authenticated client exercises the complete boundary.
async fn security_and_port_conflict_do_not_claim_another_node() {
    let root = tempfile::tempdir().unwrap();
    let local = owner(&root);
    let mut server = HttpServer::start(local.node_owner(), config()).unwrap();
    let base = format!("http://{}", server.address());
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap();
    let health: Value = client
        .get(format!("{base}/api/v1/health"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health["nodeId"], "isolated-owner");
    for path in ["projects", "sessions", "capabilities", "agent-backends"] {
        assert_eq!(
            client
                .get(format!("{base}/api/v1/{path}"))
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
    }
    assert_eq!(
        client
            .get(format!("{base}/api/v1/health"))
            .header("host", "evil.invalid")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .get(format!("{base}/api/v1/health"))
            .header("origin", "http://evil.invalid")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let login = format!("{base}/api/v1/auth");
    assert_eq!(
        client
            .post(&login)
            .json(&json!({"password":"111111"}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .post(&login)
            .header("x-rpi-web", "1")
            .json(&json!({"password":"wrong"}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let response = client
        .post(&login)
        .header("x-rpi-web", "1")
        .json(&json!({"password":"111111"}))
        .send()
        .await
        .unwrap();
    let cookie = response.headers()["set-cookie"].to_str().unwrap();
    assert!(
        cookie.contains("HttpOnly")
            && cookie.contains("SameSite=Strict")
            && cookie.contains("Max-Age=43200")
    );
    assert!(!cookie.contains("111111"));
    assert_eq!(
        client
            .get(format!("{base}/api/v1/projects"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .delete(&login)
            .header("x-rpi-web", "1")
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .get(format!("{base}/api/v1/projects"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let mut conflicting = config();
    conflicting.bind = server.address();
    assert!(HttpServer::start(local.node_owner(), conflicting).is_err());
    assert!(local.projects().await.is_ok());
    let mut empty = config();
    empty.password.clear();
    assert!(HttpServer::start(local.node_owner(), empty).is_err());
    for _ in 0..8 {
        let _ = client
            .post(&login)
            .header("x-rpi-web", "1")
            .json(&json!({"password":"wrong"}))
            .send()
            .await
            .unwrap();
    }
    assert_eq!(
        client
            .post(&login)
            .header("x-rpi-web", "1")
            .json(&json!({"password":"111111"}))
            .send()
            .await
            .unwrap()
            .status(),
        429
    );
    server.drain().await.unwrap();
    local.shutdown().await;
    drop(server);
}

#[tokio::test]
async fn public_node_discovery_matches_the_existing_owner_and_is_uncached() {
    let root = tempfile::tempdir().unwrap();
    let local = owner(&root);
    let mut server = HttpServer::start(local.node_owner(), config()).unwrap();
    let base = format!("http://{}", server.address());
    let response = reqwest::Client::new()
        .get(format!("{base}/.well-known/rpi-node"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let document: Value = response.json().await.unwrap();
    let probe = WorkClient::probe_node(&base).await.unwrap();
    assert_eq!(probe.node_id(), "isolated-owner");
    assert!(!probe.auth.authenticated);
    assert_eq!(
        probe.discovery.owner_epoch,
        document["ownerEpoch"].as_str().unwrap()
    );
    assert_eq!(document["protocol"], "rpi-node");
    assert_eq!(document["version"], 1);
    assert_eq!(document["nodeId"], "isolated-owner");
    assert!(
        document["ownerEpoch"]
            .as_str()
            .is_some_and(|epoch| !epoch.is_empty())
    );
    assert_eq!(document["bindings"].as_array().unwrap().len(), 2);
    assert!(
        document["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|binding| {
                binding["transport"]["path"] == "/api/v1/realtime"
                    && binding["transport"]["subprotocol"]
                        == work_node_api::api::realtime::SUBPROTOCOL
            })
    );
    assert!(document["extension"]["capabilities"].is_array());
    server.drain().await.unwrap();
    local.shutdown().await;
    drop(server);
}

#[test]
fn advertised_address_cannot_be_local_or_contain_secrets() {
    for (value, hint) in [
        ("", "尚未配置分享地址"),
        ("   ", "尚未配置分享地址"),
        ("192.168.3.27:4180", "分享地址"),
        ("http://", "分享地址无效"),
    ] {
        let error = work_node_service::public_address(value).unwrap_err();
        assert!(error.contains(hint), "{error}");
        assert!(error.contains("HTTP(S)"), "{error}");
    }
    for value in [
        "rpi-work://local",
        "http://localhost:3031/",
        "http://127.0.0.1:3031/",
        "http://[::1]:3031/",
        "http://0.0.0.0:3031/",
        "http://host/?password=secret",
        "https://user:secret@host/",
        "https://host/path",
        "https://host/#secret",
    ] {
        assert!(
            work_node_service::public_address(value).is_err(),
            "accepted {value}"
        );
    }
    assert_eq!(
        work_node_service::public_address("http://192.168.3.27:4180/").unwrap(),
        (
            "http://192.168.3.27:4180/".into(),
            "192.168.3.27:4180".into()
        )
    );
}

#[tokio::test]
async fn static_assets_are_same_origin_and_bounded() {
    let root = tempfile::tempdir().unwrap();
    let local = owner(&root);
    let assets = root.path().join("web");
    std::fs::create_dir(&assets).unwrap();
    std::fs::write(assets.join("index.html"), "isolated Web bundle").unwrap();
    std::fs::write(root.path().join("secret.txt"), "do not expose").unwrap();
    let mut settings = config();
    settings.assets = Some(assets);
    let mut server = HttpServer::start(local.node_owner(), settings).unwrap();
    let base = format!("http://{}", server.address());
    let client = reqwest::Client::new();
    assert_eq!(
        client
            .get(format!("{base}/"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "isolated Web bundle"
    );
    assert_eq!(
        client
            .get(format!("{base}/secret.txt"))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        client
            .get(format!("{base}/%2e%2e%2fsecret.txt"))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    server.drain().await.unwrap();
    local.shutdown().await;
    drop(server);
}

#[tokio::test]
async fn host_consent_revokes_streams_and_logins_without_stopping_workspace() {
    Box::pin(tokio::task::LocalSet::new().run_until(async {
        let root = tempfile::tempdir().unwrap();
        let local = owner(&root);
        let mut settings = config(); settings.allow_sharing = false;
        let mut server = HttpServer::start(local.node_owner(), settings).unwrap();
        let consent = server.sharing_control();
        let url = format!("http://{}/",server.address());
        let http = WorkClient::new(&url).unwrap();
        assert!(!consent.enabled());
        assert!(http.health().await.unwrap().ok);
        assert!(matches!(http.auth().await,Err(work_client::ClientError::Api{status,..}) if status==403));
        assert!(http.login("111111").await.is_err());
        consent.set_enabled(true);
        http.login("111111").await.unwrap();
        let project = local.create_project("legacy consent",None,false).await.unwrap();
        let session = legacy_fixture::seed_legacy(&root.path().join("data"),&project.id,"legacy-consent-fixture");
        let native_pid = local.command_data::<Value>(&session,"get_state",Value::Null).await.unwrap()["fixturePid"].clone();
        let mut stream = http.subscribe(&session,0).await.unwrap();
        assert_eq!(stream.next().await.unwrap().unwrap().event_type,"ready");
        consent.set_enabled(true); // Idempotent consent is not a revocation.
        assert!(http.auth().await.unwrap().authenticated);
        consent.set_enabled(false);
        assert!(http.sessions().await.is_err());
        assert!(http.login("111111").await.is_err());
        local.rename_session(&session,"still local, not shared").await.unwrap();
        assert_eq!(local.command_data::<Value>(&session,"get_state",Value::Null).await.unwrap()["fixturePid"],native_pid);
        // Reopen before the stream is polled: generation, not merely bool,
        // must still end the old subscription and reject its old cookie.
        consent.set_enabled(true);
        // Frames already delivered before revocation may remain in the HTTP
        // client's buffer. They cannot be retracted; require the stream to end.
        tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(event) = stream.next().await {
                match event {
                    Ok(_) | Err(work_client::ClientError::WebSocket { status: None }) => {},
                    Err(error) => panic!("unexpected stream close: {error}"),
                }
            }
        }).await.unwrap();
        assert!(!http.auth().await.unwrap().authenticated);
        assert!(matches!(http.sessions().await,Err(work_client::ClientError::Api{status,..}) if status==401));
        http.login("111111").await.unwrap();
        assert_eq!(http.get_session(&session).await.unwrap().title.as_deref(),Some("still local, not shared"));
        let restored_pid = http.command_data::<Value>(&session,"get_state",Value::Null).await.unwrap()["fixturePid"].clone();
        assert_eq!(native_pid,restored_pid);
        server.drain().await.unwrap(); local.shutdown().await; drop(server);
    })).await;
}
