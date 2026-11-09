//! Provider API tests never use the developer's home, configuration, or agent.
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use work_client::{LocalWorkspace, WorkClient, WorkspaceClient};
use work_core::api::{NodeApi, NodeErrorKind, ProviderAction, ProviderMutation};
use work_node_service::{HttpServer, ServerConfig};

const KEY: &str = "provider-secret-!literal-$(never-execute)-\"quoted\"";

fn config() -> ServerConfig {
    ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        password: "isolated-provider-test".into(),
        allowed_hosts: vec![],
        node_id: "provider-owner".into(),
        assets: None,
        web_proxy: false,
        allow_sharing: true,
    }
}
fn data(value: impl serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap()
}
fn mutation(revision: &str, action: Value) -> ProviderMutation {
    serde_json::from_value(json!({"expectedRevision":revision,"action":action})).unwrap()
}
fn redacted(value: impl serde::Serialize) {
    assert!(!data(value).to_string().contains("provider-secret"));
}

#[test]
fn provider_management_in_isolated_worker() {
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "isolated_provider_worker", "--nocapture"])
        .env("RPI_PROVIDER_TEST_ROOT", root.path())
        .env("RPI_CODING_AGENT_DIR", root.path().join("agent"))
        .env("HOME", root.path())
        .env("USERPROFILE", root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "worker failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn isolated_provider_worker() {
    let Some(root) = std::env::var_os("RPI_PROVIDER_TEST_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    assert_eq!(
        std::env::var_os("HOME"),
        Some(root.clone().into_os_string())
    );
    assert_eq!(
        std::env::var_os("USERPROFILE"),
        Some(root.clone().into_os_string())
    );
    assert_eq!(
        std::env::var_os("RPI_CODING_AGENT_DIR"),
        Some(root.join("agent").into_os_string())
    );
    tokio::task::LocalSet::new().run_until(async {
        std::fs::create_dir_all(root.join("agent")).unwrap();
        let local = Arc::new(LocalWorkspace::with_session_dir(
            root.join("nonexistent-host-must-not-launch"), root.join("data"),
            Some(root.join("transcripts")),
        ).unwrap());
        let node = local.node_owner();
        let local_port = WorkspaceClient::Local(local.clone());
        let mut server = HttpServer::start(node.clone(), config()).unwrap();
        let base = format!("http://{}", server.address());
        let http = WorkClient::new(&format!("{base}/")).unwrap();
        let remote = WorkspaceClient::Remote(http.clone());
        let raw = reqwest::Client::builder().cookie_store(true).build().unwrap();
        let catalog_url = format!("{base}/api/v1/providers");
        let release_url = format!("{catalog_url}/release-idle-hosts");
        assert_eq!(raw.get(&catalog_url).send().await.unwrap().status(), 401);
        for url in [&catalog_url, &release_url] {
            assert_eq!(raw.post(url).header("x-rpi-web", "1").json(&json!({})).send().await.unwrap().status(), 401);
        }
        assert_eq!(NodeApi::provider_catalog(&remote).await.unwrap_err().kind, NodeErrorKind::AccessDenied);
        http.login("isolated-provider-test").await.unwrap();
        assert_eq!(raw.post(format!("{base}/api/v1/auth")).header("x-rpi-web", "1")
            .json(&json!({"password":"isolated-provider-test"})).send().await.unwrap().status(), 200);
        let initial = NodeApi::provider_catalog(&local_port).await.unwrap();
        assert_eq!(data(&initial), data(http.provider_catalog().await.unwrap()));
        assert_eq!(data(&initial), data(local_port.provider_catalog().await.unwrap()));
        assert_eq!(data(&initial), data(NodeApi::provider_catalog(local.as_ref()).await.unwrap()));
        assert_eq!(data(&initial), data(NodeApi::provider_catalog(node.as_ref()).await.unwrap()));
        let definition = json!({"id":"transport-test","name":"Transport test","api":"openai-responses",
            "baseUrl":"https://invalid.example/v1","models":[{"id":"model","name":"Model","reasoning":true,"contextWindow":32000,"maxTokens":2048}]});
        let upsert = mutation(&initial.revision, json!({"type":"upsert","definition":definition}));
        // Both write routes retain marker, Host and Origin gates.
        for (url, body) in [(&catalog_url, data(&upsert)), (&release_url, json!({}))] {
            assert_eq!(raw.post(url).json(&body).send().await.unwrap().status(), 403);
            for (header, value) in [("origin", "https://foreign.invalid"), ("host", "foreign.invalid")] {
                assert_eq!(raw.post(url).header("x-rpi-web", "1").header(header, value).json(&body).send().await.unwrap().status(), 403);
            }
        }
        let mut invalid = data(&upsert);
        invalid["unknown"] = json!(KEY);
        let mut nested = data(&upsert);
        nested["action"]["definition"]["models"][0]["unknown"] = json!(KEY);
        let mut action_unknown = data(&upsert);
        action_unknown["action"]["unknown"] = json!(KEY);
        let mut bad_api = data(&upsert);
        bad_api["action"]["definition"]["api"] = json!(KEY);
        for body in [invalid, nested, action_unknown, bad_api, json!({})] {
            let response = raw.post(&catalog_url).header("x-rpi-web", "1").json(&body).send().await.unwrap();
            assert_eq!(response.status(), 400);
            assert!(!response.text().await.unwrap().contains("provider-secret"));
        }
        for body in [json!(null), json!([]), json!({"scope":"global"}), json!({"key":KEY})] {
            let response = raw.post(&release_url).header("x-rpi-web", "1").json(&body).send().await.unwrap();
            assert_eq!(response.status(), 400);
            assert!(!response.text().await.unwrap().contains("provider-secret"));
        }
        assert_eq!(raw.post(&release_url).header("x-rpi-web", "1").send().await.unwrap().status(), 400);
        assert_eq!(data(&initial), data(http.provider_catalog().await.unwrap()));
        let saved = http.provider_mutate(upsert.clone()).await.unwrap();
        assert!(saved.requires_restart);
        redacted(&saved);
        assert_ne!(saved.catalog.revision, initial.revision);
        assert_eq!(NodeApi::provider_mutate(&remote, upsert).await.unwrap_err().kind, NodeErrorKind::Conflict);
        let keyed = NodeApi::provider_mutate(&local_port, mutation(&saved.catalog.revision,
            json!({"type":"setKey","id":"transport-test","key":KEY}))).await.unwrap();
        redacted(&keyed);
        assert!(!format!("{keyed:?}").contains("provider-secret"));
        let auth: Value = serde_json::from_slice(&std::fs::read(root.join("agent/auth.json")).unwrap()).unwrap();
        // Native resolves config-value strings. Stored input is escaped literal,
        // never shell or environment interpolation.
        assert_eq!(auth["transport-test"]["key"], KEY.replace('$', "$$"));
        assert_eq!(data(&keyed.catalog), data(NodeApi::provider_catalog(&remote).await.unwrap()));
        assert_eq!(NodeApi::provider_mutate(&remote, mutation(&saved.catalog.revision,
            json!({"type":"setKey","id":"transport-test","key":KEY}))).await.unwrap_err().kind, NodeErrorKind::Conflict);
        let removed = remote.provider_mutate(mutation(&keyed.catalog.revision,
            json!({"type":"removeConfig","id":"transport-test"}))).await.unwrap();
        let entry = removed.catalog.providers.iter().find(|p| p.id == "transport-test").unwrap();
        assert!(!entry.configured);
        assert_eq!(entry.stored_credential, work_core::api::StoredCredential::ApiKey);
        let cleared = NodeApi::provider_mutate(&remote, mutation(&removed.catalog.revision,
            json!({"type":"clearCredential","id":"transport-test","location":"stored"}))).await.unwrap();
        redacted(&cleared);
        assert_eq!(data(&cleared.catalog), data(local_port.provider_catalog().await.unwrap()));
        // Config-source removal also travels through the typed route, preserving auth.
        let models_path = root.join("agent/models.json");
        let mut models: Value = serde_json::from_slice(&std::fs::read(&models_path).unwrap()).unwrap();
        models["providers"]["transport-test"] = json!({"api":"openai-completions","baseUrl":"https://invalid.example/v1","apiKey":KEY,
            "models":[{"id":"model","name":"Model","reasoning":false,"contextWindow":32000,"maxTokens":2048}]});
        std::fs::write(&models_path, serde_json::to_vec(&models).unwrap()).unwrap();
        let configured = http.provider_catalog().await.unwrap();
        redacted(&configured);
        assert_eq!(NodeApi::provider_mutate(&remote, mutation(&cleared.catalog.revision,
            json!({"type":"removeConfig","id":"transport-test"}))).await.unwrap_err().kind, NodeErrorKind::Conflict);
        let clean = http.provider_mutate(mutation(&configured.revision,
            json!({"type":"clearCredential","id":"transport-test","location":"config"}))).await.unwrap();
        redacted(&clean);
        assert!(!std::fs::read_to_string(&models_path).unwrap().contains("provider-secret"));
        let released = http.provider_release_hosts().await.unwrap();
        assert!(released.released.is_empty() && released.blocked.is_empty());
        remote.provider_release_hosts().await.unwrap();
        local_port.provider_release_hosts().await.unwrap();
        NodeApi::provider_release_hosts(&local_port).await.unwrap();
        NodeApi::provider_release_hosts(&remote).await.unwrap();
        assert_eq!(local.sessions().await.unwrap(), []);
        server.sharing_control().set_enabled(false);
        assert_eq!(NodeApi::provider_catalog(&remote).await.unwrap_err().kind, NodeErrorKind::AccessDenied);
        assert_eq!(NodeApi::provider_mutate(&remote, mutation(&clean.catalog.revision,
            json!({"type":"setKey","id":"transport-test","key":KEY}))).await.unwrap_err().kind, NodeErrorKind::AccessDenied);
        assert_eq!(NodeApi::provider_release_hosts(&remote).await.unwrap_err().kind, NodeErrorKind::AccessDenied);
        assert_eq!(data(&clean.catalog), data(NodeApi::provider_catalog(&local_port).await.unwrap()));
        server.drain().await.unwrap();
        local.shutdown().await;
    }).await;
}

#[tokio::test]
async fn old_server_has_no_fallback_and_echoed_errors_are_redacted() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = requests.clone();
    let app =
        axum::Router::new().fallback(move |method: axum::http::Method, uri: axum::http::Uri| {
            let observed = observed.clone();
            async move {
                observed
                    .lock()
                    .unwrap()
                    .push(format!("{method} {}", uri.path()));
                (axum::http::StatusCode::NOT_FOUND, KEY)
            }
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    let http = WorkClient::new(&format!("http://{address}/")).unwrap();
    let remote = WorkspaceClient::Remote(http.clone());
    let request = || ProviderMutation {
        expected_revision: "old".into(),
        action: ProviderAction::SetKey {
            id: "test".into(),
            key: KEY.into(),
        },
    };
    let error = http.provider_mutate(request()).await.unwrap_err();
    assert!(!format!("{error:?} {error}").contains("provider-secret"));
    assert_eq!(
        NodeApi::provider_catalog(&remote).await.unwrap_err().kind,
        NodeErrorKind::NotFound
    );
    assert_eq!(
        NodeApi::provider_mutate(&remote, request())
            .await
            .unwrap_err()
            .kind,
        NodeErrorKind::NotFound
    );
    assert_eq!(
        NodeApi::provider_release_hosts(&remote)
            .await
            .unwrap_err()
            .kind,
        NodeErrorKind::NotFound
    );
    assert_eq!(
        *requests.lock().unwrap(),
        [
            "POST /api/v1/providers",
            "GET /api/v1/providers",
            "POST /api/v1/providers",
            "POST /api/v1/providers/release-idle-hosts"
        ]
    );
    stop.send(()).unwrap();
    server.await.unwrap();
}
