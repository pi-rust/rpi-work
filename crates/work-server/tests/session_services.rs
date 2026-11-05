//! No agent/model/user directory: this test only runs the existing RPC fixture.
#[path = "../../../tests/support/legacy_fixture.rs"]
mod legacy_fixture;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use work_node::workspace::{LocalError, LocalWorkspace, WorkspaceError};
use work_server::{HttpServer, ServerConfig};

#[tokio::test]
async fn session_access_and_typed_dialogs_share_owner_and_security() {
    tokio::task::LocalSet::new().run_until(async {
        let root = tempfile::tempdir().unwrap();
        let owner = Arc::new(LocalWorkspace::with_session_dir(
            PathBuf::from(env!("CARGO_BIN_EXE_work-server-rpc-fixture")),
            root.path().join("data"), Some(root.path().join("transcripts")),
        ).unwrap());
        let mut server = HttpServer::start(owner.clone(), ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(), password: "fixture-password".into(),
            allowed_hosts: vec![], node_id: "services-fixture".into(), assets: None, web_proxy: false, allow_sharing: true,
        }).unwrap();
        let base = format!("http://{}/api/v1", server.address());
        let client = reqwest::Client::builder().cookie_store(true).default_headers(
            [(reqwest::header::HeaderName::from_static("x-rpi-web"), reqwest::header::HeaderValue::from_static("1"))].into_iter().collect()
        ).build().unwrap();
        let project = owner.create_project("legacy fixture",None,false).await.unwrap();
        let id = legacy_fixture::seed_legacy(&root.path().join("data"),&project.id,"legacy-services-fixture");
        let access_url = format!("{base}/sessions/{id}/access");
        let dialog_url = format!("{base}/sessions/{id}/dialogs");
        assert_eq!(client.get(&access_url).send().await.unwrap().status(), 401);
        assert_eq!(client.post(&dialog_url).json(&json!({"requestId":"x","cancelled":true})).send().await.unwrap().status(), 401);
        assert!(client.post(format!("{base}/auth")).json(&json!({"password":"fixture-password"})).send().await.unwrap().status().is_success());
        let access = owner.session_access(&id).await.unwrap();
        assert_eq!(access.descriptor, owner.descriptor());
        // These prove the result is not the bounded configuration command list.
        for command in ["prompt", "abort", "get_history", "get_queue", "ui_dialog_response"] {
            assert!(access.supports_command(command), "missing {command}");
        }
        assert!(!access.supports_command("compact")); // not advertised
        assert!(access.supports_command("stop"));
        assert!(!access.supports_command("get_commands")); // advertised, outside service allowlist
        assert!(access.snapshot_limit);
        assert!(access.dialog_reply);
        let response = client.get(&access_url).send().await.unwrap();
        assert_eq!(response.headers()[reqwest::header::CACHE_CONTROL], "no-store");
        assert_eq!(response.json::<Value>().await.unwrap(), serde_json::to_value(access).unwrap());
        for payload in [json!({"requestId":"x","answer":{"kind":"selector","values":["Yes"]}}), json!({"requestId":"x","cancelled":true})] {
            // The RPI fixture accepts these through the unchanged command path.
            let response = client.post(&dialog_url).json(&payload).send().await.unwrap();
            assert_eq!(response.status(), 200);
            assert_eq!(response.headers()[reqwest::header::CACHE_CONTROL], "no-store");
            assert_eq!(response.json::<Value>().await.unwrap()["accepted"], true);
        }
        for payload in [
            json!({"requestId":"x"}),
            json!({"requestId":"x","cancelled":false}),
            json!({"requestId":"x","cancelled":true,"answer":null}),
            json!({"requestId":"x","cancelled":true,"responseId":7}),
            json!({"requestId":"x","result":{"outcome":{"outcome":"selected","optionId":"allow"}}}),
        ] {
            let response = client.post(&dialog_url).json(&payload).send().await.unwrap();
            assert_eq!(response.status(), 400, "{payload}");
            assert_eq!(response.headers()[reqwest::header::CACHE_CONTROL], "no-store");
        }
        for endpoint in [&access_url, &dialog_url] {
            let response = client.post(endpoint).header("Origin", "https://foreign.invalid").json(&json!({"requestId":"x","cancelled":true})).send().await.unwrap();
            assert_eq!(response.status(), 403);
        }
        assert_eq!(client.get(format!("{base}/sessions/missing/access")).send().await.unwrap().status(), 404);
        server.sharing_control().set_enabled(false);
        assert_eq!(client.get(&access_url).send().await.unwrap().status(), 403);
        assert_eq!(client.post(&dialog_url).json(&json!({"requestId":"x","cancelled":true})).send().await.unwrap().status(), 403);
        assert!(owner.session_access(&id).await.is_ok());
        server.drain().await.unwrap();
        owner.shutdown().await;
        assert!(matches!(owner.session_access(&id).await, Err(WorkspaceError::Local(LocalError::Shutdown))));
    }).await;
}
