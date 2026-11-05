//! Real HTTP -> shared native owner -> isolated RPC process; not a performance benchmark.
#[path = "../../../tests/support/legacy_fixture.rs"]
mod legacy_fixture;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use work_node::workspace::LocalWorkspace;
use work_server::{HttpServer, ServerConfig};

#[tokio::test]
async fn native_timeline_http_contract() {
    tokio::task::LocalSet::new().run_until(async {
        let root = tempfile::tempdir().unwrap();
        let owner = Arc::new(LocalWorkspace::with_session_dir(
            PathBuf::from(env!("CARGO_BIN_EXE_work-server-rpc-fixture")),
            root.path().join("data"), Some(root.path().join("transcripts")),
        ).unwrap());
        let mut server = HttpServer::start(owner.clone(), ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(), password: "timeline-fixture".into(),
            allowed_hosts: vec![], node_id: "timeline-isolated".into(), assets: None, web_proxy: false, allow_sharing: true,
        }).unwrap();
        let base = format!("http://{}/api/v1", server.address());
        let client = reqwest::Client::builder().cookie_store(true).default_headers(
            [(reqwest::header::HeaderName::from_static("x-rpi-web"), reqwest::header::HeaderValue::from_static("1"))].into_iter().collect()
        ).build().unwrap();
        let project = owner.create_project("timeline fixture", None, false).await.unwrap();
        let id = legacy_fixture::seed_legacy(&root.path().join("data"), &project.id, "native-queries-fixture");
        let old = legacy_fixture::seed_legacy(&root.path().join("data"), &project.id, "legacy-no-queries-fixture");
        let summary = format!("{base}/sessions/{id}/message-summaries");
        let window = format!("{base}/sessions/{id}/message-window");
        for url in [&summary, &window] {
            assert_eq!(client.get(url).send().await.unwrap().status(), 401);
        }
        assert_eq!(client.post(format!("{base}/auth")).json(&json!({"password":"timeline-fixture"})).send().await.unwrap().status(), 200);
        // Real production client + real HTTP server + same native owner, not a
        // mocked HTTP route. Client validation must accept the host's page contract.
        let typed = work_client::WorkClient::new(base.strip_suffix("/api/v1").unwrap()).unwrap();
        typed.login("timeline-fixture").await.unwrap();
        let typed = work_client::WorkspaceClient::from(typed);
        let summaries = typed.message_summaries(&id, &work_client::MessageSummaryQuery::default()).await.unwrap();
        assert_eq!(summaries.messages[0].entry_id, "fixture-user");
        let target = work_client::MessageWindowQuery {
            anchor_id: Some(summaries.messages[0].entry_id.clone()),
            leaf_id: summaries.leaf_id,
            ..Default::default()
        };
        let messages = typed.message_window(&id, &target).await.unwrap();
        assert_eq!(messages.messages[0]["entryId"], "fixture-user");
        let pid = owner.command_data::<Value>(&id, "get_state", Value::Null).await.unwrap()["fixturePid"].clone();
        let response = client.get(&summary).send().await.unwrap();
        assert_eq!(response.status(), 200, "new summary route must reach native owner");
        assert_eq!(response.headers()[reqwest::header::CACHE_CONTROL], "no-store");
        let page = response.json::<Value>().await.unwrap();
        assert_eq!(page["request"], json!({"type":"get_message_summaries","limit":10,"role":"user"}));
        assert_eq!(page["fixturePid"], pid);
        let access = owner.session_access(&id).await.unwrap();
        for command in ["get_message_summaries", "get_message_window"] { assert!(access.supports_command(command)); }
        assert!(!access.supports_command("get_commands"));
        assert!(owner.command::<Value>(&id, json!({"type":"get_commands"})).await.is_err());
        for (url, params, expected) in [
            (&summary, vec![("limit","1"),("role","user"),("beforeId","opaque /?&"),("leafId","leaf /?&")], json!({"type":"get_message_summaries","limit":1,"role":"user","beforeId":"opaque /?&","leafId":"leaf /?&"})),
            (&window, vec![("limit","100"),("anchorId","opaque /?&"),("leafId","leaf /?&")], json!({"type":"get_message_window","limit":100,"anchorId":"opaque /?&","leafId":"leaf /?&"})),
            (&window, vec![("beforeId","before")], json!({"type":"get_message_window","limit":48,"beforeId":"before"})),
            (&window, vec![("afterId","after")], json!({"type":"get_message_window","limit":48,"afterId":"after"})),
        ] {
            let response = client.get(url).query(&params).send().await.unwrap();
            assert_eq!(response.status(), 200);
            let data = response.json::<Value>().await.unwrap();
            assert_eq!(data["request"], expected);
            assert_eq!(data["fixturePid"], pid);
            // Extra native data and opaque IDs must survive without local page synthesis.
            assert_eq!(data["nativeSentinel"], "unchanged");
        }
        let boundary = "x".repeat(512);
        let response = client.get(&summary).query(&[("beforeId", boundary.as_str())]).send().await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.json::<Value>().await.unwrap()["request"]["beforeId"], boundary);
        assert!(!owner.session_access(&old).await.unwrap().supports_command("get_message_window"));
        let long = "x".repeat(513);
        for (url, params) in [
            (&summary, vec![("limit","0")]), (&summary, vec![("limit","101")]),
            (&summary, vec![("limit","-1")]), (&summary, vec![("limit","1.0")]),
            (&summary, vec![("limit","+1")]), (&summary, vec![("role","")]),
            (&summary, vec![("limit","1"),("limit","2")]),
            (&summary, vec![("beforeId","a"),("beforeId","b")]),
            (&summary, vec![("role","assistant")]), (&summary, vec![("before_id","x")]),
            (&summary, vec![("anchorId","x")]), (&summary, vec![("extra","x")]),
            (&summary, vec![("beforeId","")]), (&summary, vec![("leafId"," ")]),
            (&summary, vec![("beforeId","bad\n")]), (&summary, vec![("leafId",long.as_str())]),
            (&window, vec![]), (&window, vec![("anchorId","a"),("beforeId","b")]),
            (&window, vec![("beforeId","b"),("afterId","c")]),
            (&window, vec![("anchorId","a"),("role","user")]),
            (&window, vec![("anchorId","a"),("limit","0")]),
            (&window, vec![("anchorId","a"),("limit","101")]),
            (&window, vec![("anchorId","a"),("anchorId","b")]),
            (&window, vec![("anchorId","a"),("leaf_id","leaf")]),
            (&window, vec![("anchorId","")]), (&window, vec![("afterId","bad\u{7f}")]),
            (&window, vec![("anchorId","a"),("leafId",long.as_str())]),
        ] {
            let response = client.get(url).query(&params).send().await.unwrap();
            assert_eq!(response.status(), 400, "{url} {params:?}");
            assert_eq!(response.headers()[reqwest::header::CACHE_CONTROL], "no-store");
        }
        for (endpoint, params) in [("message-summaries",vec![]), ("message-window",vec![("anchorId","a")])] {
            assert_eq!(client.get(format!("{base}/sessions/{old}/{endpoint}")).query(&params).send().await.unwrap().status(), 501);
        }
        for url in [&summary, &window] {
            let params = if url == &summary { vec![("leafId","conflict")] } else { vec![("anchorId","a"),("leafId","conflict")] };
            let response = client.get(url).query(&params).send().await.unwrap();
            assert_eq!(response.status(), 409, "native leaf conflict");
            let params = if url == &summary { vec![("beforeId","host-failure")] } else { vec![("anchorId","host-failure")] };
            assert_eq!(client.get(url).query(&params).send().await.unwrap().status(), 400, "native success:false must not be HTTP 200");
            assert_eq!(client.get(url).header("Origin","https://foreign.invalid").send().await.unwrap().status(), 403);
            assert_eq!(client.get(url).header("Host","foreign.invalid").send().await.unwrap().status(), 403);
        }
        assert_eq!(owner.command_data::<Value>(&id,"get_state",Value::Null).await.unwrap()["fixturePid"],pid);
        server.sharing_control().set_enabled(false);
        for url in [&summary, &window] { assert_eq!(client.get(url).send().await.unwrap().status(), 403); }
        // Revoking sharing must not stop or replace the owner.
        assert_eq!(owner.command_data::<Value>(&id,"get_state",Value::Null).await.unwrap()["fixturePid"],pid);
        server.drain().await.unwrap();
        owner.shutdown().await;
    }).await;
}
