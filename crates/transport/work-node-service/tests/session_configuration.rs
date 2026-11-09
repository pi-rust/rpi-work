//! Configuration HTTP access uses the existing owner and security boundary.
#[path = "../../../../tests/support/legacy_fixture.rs"]
mod legacy_fixture;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use work_client::{ClientError, LocalWorkspace, WorkClient, WorkspaceClient};
use work_node_service::{HttpServer, ServerConfig};

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn configuration_routes_share_runtime_and_enforce_access() {
    Box::pin(tokio::task::LocalSet::new().run_until(async {
        let root = tempfile::tempdir().unwrap();
        let local = Arc::new(LocalWorkspace::with_session_dir(
            PathBuf::from(env!("CARGO_BIN_EXE_work-server-rpc-fixture")), root.path().join("data"), Some(root.path().join("transcripts")),
        ).unwrap());
        let mut server = HttpServer::start(local.node_owner(), ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(), password: "fixture-password".into(),
            allowed_hosts: vec![], node_id: "config-fixture".into(), assets: None, web_proxy: false, allow_sharing: true,
        }).unwrap();
        let url = format!("http://{}/", server.address());
        let http = WorkClient::new(&url).unwrap();
        let project = local.create_project("legacy fixture",None,false).await.unwrap();
        let session = legacy_fixture::seed_legacy(&root.path().join("data"),&project.id,"legacy-config-fixture");
        assert!(matches!(http.session_configuration(&session).await, Err(ClientError::Api { status, .. }) if status == 401));
        assert!(matches!(http.set_session_config_option(&session, "model", json!("fixture")).await, Err(ClientError::WebSocket { status: Some(401) })));
        http.login("fixture-password").await.unwrap();
        let remote = WorkspaceClient::Remote(http.clone());
        let native = WorkspaceClient::Local(local.clone());
        let observed = remote.session_configuration(&session).await.unwrap();
        assert_eq!(observed, native.session_configuration(&session).await.unwrap());
        assert_eq!(observed.protocol, "rpi");
        assert!(observed.supports_command("get_state"));
        assert!(observed.supports_command("set_model"));
        assert!(observed.supports_command("set_thinking_level"));
        assert!(!observed.supports_command("compact")); // Not advertised by this fixture.
        assert!(!observed.supports_command("prompt")); // Not this configuration surface.
        assert_eq!(observed.config_options, Vec::<Value>::new());
        let changed = remote.set_session_config_option(&session, "model", json!("fixture-model")).await.unwrap();
        assert_eq!(changed["request"]["type"], "set_model");
        assert_eq!(changed["request"]["modelId"], "fixture-model");
        let thinking = native.set_session_config_option(&session, "thinking", json!("high")).await.unwrap();
        assert_eq!(thinking["request"]["type"], "set_thinking_level");
        assert_eq!(thinking["request"]["level"], "high");
        let state: Value = local.command_data(&session, "get_state", Value::Null).await.unwrap();
        assert_eq!(changed["fixturePid"], state["fixturePid"]);
        assert_eq!(thinking["fixturePid"], state["fixturePid"]);
        assert!(matches!(remote.set_session_config_option(&session, "model", Value::Null).await, Err(ClientError::Api { status, .. }) if status == 400));
        assert!(matches!(remote.set_session_config_option(&session, "unknown", json!("x")).await, Err(ClientError::Api { status, .. }) if status == 501));
        assert!(matches!(remote.session_configuration("missing").await, Err(ClientError::Api { status, .. }) if status == 404));
        // A normal cookie is not sufficient to bypass Origin checks.
        let raw = reqwest::Client::builder().cookie_store(true).default_headers(
            [(reqwest::header::HeaderName::from_static("x-rpi-web"), reqwest::header::HeaderValue::from_static("1"))].into_iter().collect()
        ).build().unwrap();
        assert!(raw.post(format!("{url}api/v1/auth")).json(&json!({"password":"fixture-password"})).send().await.unwrap().status().is_success());
        let endpoint = format!("{url}api/v1/sessions/{session}/configuration");
        let response = raw.get(&endpoint).send().await.unwrap();
        assert!(response.status().is_success());
        assert_eq!(response.headers()[reqwest::header::CACHE_CONTROL], "no-store");
        let response = raw.post(&endpoint).header("Origin", "https://foreign.invalid").json(&json!({"configId":"model", "value":"x"})).send().await.unwrap();
        assert_eq!(response.status(), 403);
        let response = raw.post(&endpoint).json(&json!({"configId":"model"})).send().await.unwrap();
        assert_eq!(response.status(), 400);
        server.sharing_control().set_enabled(false);
        assert!(remote.session_configuration(&session).await.is_err());
        assert!(remote.set_session_config_option(&session, "model", json!("x")).await.is_err());
        assert!(native.session_configuration(&session).await.is_ok());
        server.drain().await.unwrap();
        local.shutdown().await;
    })).await;
}
