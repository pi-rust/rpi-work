//! Real HTTP/SSE with temporary owners and the dedicated RPC stub only.
#[path = "../../../tests/support/legacy_fixture.rs"]
mod legacy_fixture;
use futures_util::StreamExt;
use reqwest::{Client, header};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Arc, time::Duration};
use work_core::api::realtime::{
    OperationOutcome, OperationRef, OperationStatus, RealtimeOperationKind,
};
use work_node::workspace::{LocalWorkspace, operations::OperationClaim};
use work_server::{HttpServer, ServerConfig};

fn config() -> ServerConfig {
    ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        password: "isolated-test-only".into(),
        allowed_hosts: vec![],
        node_id: "realtime-auth-fixture".into(),
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
async fn login(client: &Client, url: &str) -> String {
    let response = client
        .post(format!("{url}/api/v1/auth"))
        .header("x-rpi-web", "1")
        .json(&json!({"password":"isolated-test-only"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let pair = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let token = pair.split_once('=').unwrap().1;
    // Test-only bridge into the owner ledger. Neither token nor principal is
    // logged or sent in URLs; production HTTP derives the same identity privately.
    format!("{:x}", Sha256::digest(token.as_bytes()))
}
fn status_url(base: &str, reference: &OperationRef) -> String {
    format!(
        "{base}/api/v1/realtime/operations/{}/{}",
        reference.owner_epoch, reference.operation_id
    )
}

#[tokio::test]
async fn status_query_requires_the_same_cookie_principal_and_is_read_only() {
    let root = tempfile::tempdir().unwrap();
    let owner = owner(&root);
    let mut server = HttpServer::start(owner.clone(), config()).unwrap();
    let url = format!("http://{}", server.address());
    let client = Client::builder().cookie_store(true).build().unwrap();
    let principal = login(&client, &url).await;
    let reference = owner
        .remote_operations()
        .reserve(
            &principal,
            "client",
            "session",
            RealtimeOperationKind::SessionCommand,
        )
        .unwrap();
    let query = status_url(&url, &reference);
    let response = client.get(&query).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        response.json::<OperationStatus>().await.unwrap(),
        OperationStatus::Reserved
    );
    let anonymous = Client::new();
    assert_eq!(anonymous.get(&query).send().await.unwrap().status(), 401);
    let other = Client::builder().cookie_store(true).build().unwrap();
    login(&other, &url).await;
    assert_eq!(other.get(&query).send().await.unwrap().status(), 403);
    let OperationClaim::Execute(permit) = owner
        .remote_operations()
        .claim(&principal, &reference, json!({"type":"prompt"}))
        .unwrap()
    else {
        panic!("expected first permit")
    };
    let expected = permit
        .finish(OperationOutcome::Succeeded {
            value: json!({"success":true,"data":{"accepted":true}}),
        })
        .unwrap();
    assert_eq!(
        client
            .get(&query)
            .send()
            .await
            .unwrap()
            .json::<OperationStatus>()
            .await
            .unwrap(),
        expected
    );
    let old_owner = OperationRef {
        owner_epoch: "old-owner-epoch".into(),
        operation_id: reference.operation_id.clone(),
    };
    assert_eq!(
        client
            .get(status_url(&url, &old_owner))
            .send()
            .await
            .unwrap()
            .json::<OperationStatus>()
            .await
            .unwrap(),
        OperationStatus::OutcomeUnknown
    );
    assert_eq!(
        client
            .post(&query)
            .header("x-rpi-web", "1")
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        client
            .get(format!("{url}/api/v1/transports"))
            .send()
            .await
            .unwrap()
            .status(),
        200,
        "phase B now provides the authenticated WS endpoint"
    );
    client
        .delete(format!("{url}/api/v1/auth"))
        .header("x-rpi-web", "1")
        .send()
        .await
        .unwrap();
    assert_eq!(client.get(&query).send().await.unwrap().status(), 401);
    server.drain().await.unwrap();
    owner.shutdown().await;
}

#[tokio::test]
async fn logout_ends_existing_sse_but_does_not_stop_the_shared_owner() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let root = tempfile::tempdir().unwrap();
            let owner = owner(&root);
            let mut server = HttpServer::start(owner.clone(), config()).unwrap();
            let url = format!("http://{}", server.address());
            let client = Client::builder().cookie_store(true).build().unwrap();
            login(&client, &url).await;
            let project = owner.create_project("isolated", None, false).await.unwrap();
            let session = legacy_fixture::seed_legacy(
                &root.path().join("data"),
                &project.id,
                "logout-sse-fixture",
            );
            let response = client
                .get(format!("{url}/api/v1/sessions/{session}/events?after=0"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            let mut events = response.bytes_stream();
            assert!(
                tokio::time::timeout(Duration::from_secs(3), events.next())
                    .await
                    .unwrap()
                    .unwrap()
                    .is_ok()
            );
            let before = owner
                .command::<Value>(&session, json!({"type":"get_state"}))
                .await
                .unwrap()
                .data
                .unwrap()["fixturePid"]
                .clone();
            assert!(before.is_u64());
            assert_eq!(
                client
                    .delete(format!("{url}/api/v1/auth"))
                    .header("x-rpi-web", "1")
                    .send()
                    .await
                    .unwrap()
                    .status(),
                200
            );
            tokio::time::timeout(Duration::from_secs(3), async {
                while let Some(item) = events.next().await {
                    item.unwrap();
                }
            })
            .await
            .unwrap();
            assert_eq!(
                client
                    .get(format!("{url}/api/v1/projects"))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                401
            );
            let after = owner
                .command::<Value>(&session, json!({"type":"get_state"}))
                .await
                .unwrap()
                .data
                .unwrap()["fixturePid"]
                .clone();
            assert_eq!(
                before, after,
                "logout must not restart or close the native owner"
            );
            server.drain().await.unwrap();
            owner.shutdown().await;
        })
        .await;
}

#[tokio::test]
async fn logout_drops_http_waiter_but_drain_still_waits_for_started_rpc() {
    tokio::task::LocalSet::new().run_until(async {
        let root = tempfile::tempdir().unwrap();
        let owner = owner(&root);
        let mut server = HttpServer::start(owner.clone(), config()).unwrap();
        let url = format!("http://{}", server.address());
        let client = Client::builder().cookie_store(true).build().unwrap();
        login(&client, &url).await;
        let project = owner.create_project("isolated", None, false).await.unwrap();
        let session = legacy_fixture::seed_legacy(&root.path().join("data"), &project.id, "logout-rpc-gate-fixture");
        let before = owner.command::<Value>(&session, json!({"type":"get_state"})).await.unwrap().data.unwrap()["fixturePid"].clone();
        assert!(before.is_u64());
        let mut events = client.get(format!("{url}/api/v1/sessions/{session}/events?after=0")).send().await.unwrap().bytes_stream();
        let release = root.path().join("release-fixture-rpc");
        let prompt = client.post(format!("{url}/api/v1/sessions/{session}/commands")).header("x-rpi-web", "1")
            .json(&json!({"type":"prompt","message":format!("realtime-auth-gate:{}", release.display())}));
        let response = tokio::spawn(async move { prompt.send().await });
        tokio::time::timeout(Duration::from_secs(3), async {
            let mut buffered = String::new();
            loop {
                let bytes = events.next().await.unwrap().unwrap();
                buffered.push_str(std::str::from_utf8(&bytes).unwrap());
                if buffered.contains("fixture_started") { break; }
            }
        }).await.unwrap();
        assert_eq!(client.delete(format!("{url}/api/v1/auth")).header("x-rpi-web", "1").send().await.unwrap().status(), 200);
        assert_eq!(tokio::time::timeout(Duration::from_secs(1), response).await.unwrap().unwrap().unwrap().status(), 401);
        drop(events);
        let draining = server.drain();
        tokio::pin!(draining);
        assert!(tokio::time::timeout(Duration::from_millis(50), &mut draining).await.is_err(), "accepted owner work must outlive the HTTP waiter");
        std::fs::write(&release, b"release isolated stub").unwrap();
        tokio::time::timeout(Duration::from_secs(3), &mut draining).await.unwrap().unwrap();
        let mut replay = owner.subscribe(&session, 0).await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if replay.next().await.unwrap().unwrap().event_type == "agent_settled" { break; }
            }
        }).await.unwrap();
        let after = owner.command::<Value>(&session, json!({"type":"get_state"})).await.unwrap().data.unwrap()["fixturePid"].clone();
        assert_eq!(before, after);
        owner.shutdown().await;
    }).await;
}
