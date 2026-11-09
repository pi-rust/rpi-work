//! Auth tests use only temporary directories and the dedicated protocol fixture.
use reqwest::{Client, Response};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use work_node_runtime::workspace::LocalWorkspace;
use work_node_service::{HttpServer, ServerConfig};

struct Fixture {
    server: HttpServer,
    owner: Arc<LocalWorkspace>,
    _root: tempfile::TempDir,
}
impl Fixture {
    fn new(node_id: &str, assets: bool, sharing: bool) -> Self {
        Self::with_proxy(node_id, assets, sharing, false)
    }
    fn with_proxy(node_id: &str, assets: bool, sharing: bool, web_proxy: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let owner = Arc::new(
            LocalWorkspace::with_session_dir(
                PathBuf::from(env!("CARGO_BIN_EXE_work-server-rpc-fixture")),
                root.path().join("data"),
                Some(root.path().join("transcripts")),
            )
            .unwrap(),
        );
        let web = assets.then(|| {
            let web = root.path().join("web");
            std::fs::create_dir(&web).unwrap();
            std::fs::write(web.join("index.html"), "test-only SPA").unwrap();
            web
        });
        let server = HttpServer::start(
            owner.clone(),
            ServerConfig {
                bind: "127.0.0.1:0".parse().unwrap(),
                password: "isolated-test-password".into(),
                allowed_hosts: vec![],
                node_id: node_id.into(),
                assets: web,
                web_proxy,
                allow_sharing: sharing,
            },
        )
        .unwrap();
        Self {
            server,
            owner,
            _root: root,
        }
    }
    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.server.address())
    }
    async fn stop(mut self) {
        self.server.drain().await.unwrap();
        self.owner.shutdown().await;
    }
}
async fn post(client: &Client, f: &Fixture, path: &str, body: Value) -> Response {
    client
        .post(f.url(path))
        .header("x-rpi-web", "1")
        .json(&body)
        .send()
        .await
        .unwrap()
}
async fn redeem(client: &Client, f: &Fixture, ticket: &str) -> Response {
    post(
        client,
        f,
        "/api/v1/auth/qr/redeem",
        json!({"ticket":ticket}),
    )
    .await
}
fn browser() -> Client {
    Client::builder().cookie_store(true).build().unwrap()
}

#[tokio::test]
async fn metadata_is_explicit_no_store_and_not_a_sharing_bypass() {
    let client = Client::new();
    for (assets, web_proxy) in [(false, false), (true, false), (false, true), (true, true)] {
        let f = Fixture::with_proxy("entry-node", assets, false, web_proxy);
        // A declaration must not touch the workspace or start an agent. Even a
        // shut-down owner can serve metadata; API authorization remains guarded.
        f.owner.shutdown().await;
        let path = "/.well-known/rpi-work-entry";
        let r = client.get(f.url(path)).send().await.unwrap();
        assert_eq!(r.status(), if assets || web_proxy { 200 } else { 404 });
        assert_eq!(r.headers()["cache-control"], "no-store");
        assert!(r.headers().get("set-cookie").is_none());
        if assets || web_proxy {
            assert_eq!(
                r.json::<Value>().await.unwrap(),
                json!({"version":1,"mode":"hosted","nodeId":"entry-node"})
            );
        } else {
            assert_eq!(r.text().await.unwrap(), "");
        }
        for (header, value) in [
            ("host", "evil.invalid"),
            ("host", "127.0.0.1:1"),
            ("origin", "https://evil.invalid"),
            ("origin", "http://127.0.0.1:1"),
        ] {
            let r = client
                .get(f.url(path))
                .header(header, value)
                .send()
                .await
                .unwrap();
            assert_eq!(r.status(), 403);
            assert_eq!(r.headers()["cache-control"], "no-store");
            assert!(r.headers().get("access-control-allow-origin").is_none());
        }
        assert!(f.server.issue_qr_authorization().is_err());
        assert_eq!(
            post(&client, &f, "/api/v1/auth/qr", json!({}))
                .await
                .status(),
            403
        );
        assert_eq!(redeem(&client, &f, "invalid").await.status(), 403);
        assert_eq!(
            client
                .get(f.url("/api/v1/projects"))
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        f.stop().await;
    }
}

#[tokio::test]
async fn proxy_metadata_does_not_authenticate_workspace_requests() {
    let f = Fixture::with_proxy("proxy-node", false, true, true);
    f.owner.shutdown().await;
    let client = browser();
    let response = client
        .get(f.url("/.well-known/rpi-work-entry"))
        .header("origin", format!("http://{}", f.server.address()))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert!(response.headers().get("set-cookie").is_none());
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"version":1,"mode":"hosted","nodeId":"proxy-node"})
    );
    for path in [
        "/api/v1/projects",
        "/api/v1/sessions",
        "/api/v1/capabilities",
    ] {
        assert_eq!(client.get(f.url(path)).send().await.unwrap().status(), 401);
    }
    let auth = client.get(f.url("/api/v1/auth")).send().await.unwrap();
    assert_eq!(
        auth.json::<Value>().await.unwrap(),
        json!({"required":true,"authenticated":false})
    );
    // Proxy declaration does not serve SPA fallback from the API listener.
    assert_eq!(client.get(f.url("/")).send().await.unwrap().status(), 404);
    f.server.sharing_control().set_enabled(false);
    assert_eq!(
        client
            .get(f.url("/api/v1/projects"))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    f.stop().await;
}

#[tokio::test]
async fn owner_ticket_issues_ordinary_auth_cookie_once_and_can_logout() {
    let f = Fixture::new("qr-node", false, true);
    let qr = f.server.issue_qr_authorization().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert_eq!(qr.ticket.len(), 64);
    assert!(qr.ticket.bytes().all(|c| c.is_ascii_hexdigit()));
    assert!((now + 119..=now + 120).contains(&qr.expires_at));
    let value = serde_json::to_value(&qr).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 3);
    assert_eq!(value["nodeId"], "qr-node");
    assert!(value["expiresAt"].is_u64());
    let client = browser();
    let r = redeem(&client, &f, &qr.ticket).await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["cache-control"], "no-store");
    let qr_cookie = r.headers()["set-cookie"].to_str().unwrap().to_owned();
    assert!(qr_cookie.contains("HttpOnly; SameSite=Strict; Path=/; Max-Age=43200"));
    assert!(!qr_cookie.contains(&qr.ticket));
    assert_eq!(
        r.json::<Value>().await.unwrap(),
        json!({"required":true,"authenticated":true})
    );
    assert_eq!(
        client
            .get(f.url("/api/v1/projects"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(redeem(&Client::new(), &f, &qr.ticket).await.status(), 401);
    let password = post(
        &client,
        &f,
        "/api/v1/auth",
        json!({"password":"isolated-test-password"}),
    )
    .await;
    assert_eq!(
        qr_cookie.split('=').next(),
        password.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split('=')
            .next()
    );
    assert_eq!(
        client
            .delete(f.url("/api/v1/auth"))
            .header("x-rpi-web", "1")
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .get(f.url("/api/v1/projects"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    f.stop().await;
}

#[tokio::test]
async fn http_issue_needs_auth_and_all_mutations_retain_security_checks() {
    let f = Fixture::new("http-qr", true, true);
    let client = browser();
    assert_eq!(
        post(&client, &f, "/api/v1/auth/qr", json!({}))
            .await
            .status(),
        401
    );
    assert_eq!(
        post(
            &client,
            &f,
            "/api/v1/auth",
            json!({"password":"isolated-test-password"})
        )
        .await
        .status(),
        200
    );
    for body in [json!({"password":"not-accepted"}), json!(null), json!([])] {
        assert_eq!(
            post(&client, &f, "/api/v1/auth/qr", body).await.status(),
            422
        );
    }
    let r = post(&client, &f, "/api/v1/auth/qr", json!({})).await;
    assert_eq!(r.status(), 200);
    let qr: Value = r.json().await.unwrap();
    let ticket = qr["ticket"].as_str().unwrap();
    let anonymous = Client::new();
    for path in ["/api/v1/auth/qr", "/api/v1/auth/qr/redeem"] {
        let body = if path.ends_with("redeem") {
            json!({"ticket":ticket})
        } else {
            json!({})
        };
        assert_eq!(
            client
                .post(f.url(path))
                .json(&body)
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        for (header, value) in [("host", "evil.invalid"), ("origin", "https://evil.invalid")] {
            let r = client
                .post(f.url(path))
                .header("x-rpi-web", "1")
                .header(header, value)
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(r.status(), 403);
            assert_eq!(r.headers()["cache-control"], "no-store");
        }
        assert_eq!(client.get(f.url(path)).send().await.unwrap().status(), 405);
    }
    assert_eq!(
        post(
            &anonymous,
            &f,
            "/api/v1/auth/qr/redeem",
            json!({"ticket":ticket,"extra":1})
        )
        .await
        .status(),
        422
    );
    assert_eq!(
        post(&anonymous, &f, "/api/v1/auth/qr/redeem", json!({}))
            .await
            .status(),
        422
    );
    assert_eq!(
        redeem(&anonymous, &f, &"a".repeat(2048)).await.status(),
        413
    );
    assert_eq!(redeem(&anonymous, &f, ticket).await.status(), 200);
    f.stop().await;
}

#[tokio::test]
async fn concurrent_redemption_node_isolation_and_generation_revocation() {
    let f = Fixture::new("node-a", false, true);
    let other = Fixture::new("node-b", false, true);
    let client = Client::new();
    let qr = f.server.issue_qr_authorization().unwrap();
    assert_eq!(redeem(&client, &other, &qr.ticket).await.status(), 401);
    let (a, b) = tokio::join!(
        redeem(&client, &f, &qr.ticket),
        redeem(&client, &f, &qr.ticket)
    );
    let mut statuses = [a.status().as_u16(), b.status().as_u16()];
    statuses.sort_unstable();
    assert_eq!(statuses, [200, 401]);
    let qr = f.server.issue_qr_authorization().unwrap();
    let cookie_client = browser();
    assert_eq!(redeem(&cookie_client, &f, &qr.ticket).await.status(), 200);
    let old_ticket = f.server.issue_qr_authorization().unwrap();
    let policy = f.server.sharing_control();
    policy.set_enabled(true); // idempotent consent does not revoke
    assert_eq!(redeem(&client, &f, &old_ticket.ticket).await.status(), 200);
    let old_ticket = f.server.issue_qr_authorization().unwrap();
    policy.set_enabled(false);
    assert!(f.server.issue_qr_authorization().is_err());
    assert_eq!(redeem(&client, &f, &old_ticket.ticket).await.status(), 403);
    policy.set_enabled(true);
    assert_eq!(redeem(&client, &f, &old_ticket.ticket).await.status(), 401);
    assert_eq!(
        post(&cookie_client, &f, "/api/v1/auth/qr", json!({}))
            .await
            .status(),
        401
    );
    assert_eq!(
        cookie_client
            .get(f.url("/api/v1/projects"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let fresh = f.server.issue_qr_authorization().unwrap();
    assert_eq!(redeem(&client, &f, &fresh.ticket).await.status(), 200);
    f.server.begin_shutdown();
    assert!(f.server.issue_qr_authorization().is_err());
    f.stop().await;
    other.stop().await;
}

#[tokio::test]
async fn redemption_is_globally_rate_limited_not_reset_by_consent_toggles() {
    let f = Fixture::new("limited", false, true);
    let client = Client::new();
    let qr = f.server.issue_qr_authorization().unwrap();
    for _ in 0..10 {
        assert_eq!(redeem(&client, &f, "malformed").await.status(), 401);
    }
    let r = redeem(&client, &f, &qr.ticket).await;
    assert_eq!(r.status(), 429);
    assert!(r.headers().get("set-cookie").is_none());
    assert_eq!(r.headers()["cache-control"], "no-store");
    let policy = f.server.sharing_control();
    policy.set_enabled(false);
    policy.set_enabled(true);
    let fresh = f.server.issue_qr_authorization().unwrap();
    assert_eq!(redeem(&client, &f, &fresh.ticket).await.status(), 429);
    // Password login uses its existing independent limiter.
    assert_eq!(
        post(
            &client,
            &f,
            "/api/v1/auth",
            json!({"password":"isolated-test-password"})
        )
        .await
        .status(),
        200
    );
    f.stop().await;
}
