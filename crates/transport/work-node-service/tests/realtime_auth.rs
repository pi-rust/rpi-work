//! Real HTTP authentication and operation-status checks with temporary owners.
use reqwest::{Client, header};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Arc};
use work_core::api::realtime::{
    OperationOutcome, OperationRef, OperationStatus, RealtimeOperationKind,
};
use work_node_runtime::workspace::{LocalWorkspace, operations::OperationClaim};
use work_node_service::{HttpServer, ServerConfig};

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
