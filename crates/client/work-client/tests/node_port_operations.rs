#![cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
//! Real dyn ports, one owner, fixture-only ACP; environment changes live in a child.
use futures_util::{FutureExt, StreamExt};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use work_client::{LocalWorkspace, WorkspaceClient};
use work_core::api::{NodeApi, NodeError, NodeErrorKind, NodeEventStream, WireEvent};

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn acp_typed_operations_share_owner_and_isolate_sessions() {
    const WORKER: &str = "WORK_NODE_PORT_OPERATIONS_WORKER";
    if std::env::var_os(WORKER).is_none() {
        let root = Directory(std::env::temp_dir().join(format!(
                "node-port-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )));
        std::fs::create_dir_all(&root.0).unwrap();
        let root_path = root.0.canonicalize().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "acp_typed_operations_share_owner_and_isolate_sessions",
                "--nocapture",
            ])
            .current_dir(&root_path)
            .env(WORKER, &root_path)
            .env("HOME", &root_path)
            .env("USERPROFILE", &root_path)
            .env("APPDATA", &root_path)
            .env("LOCALAPPDATA", &root_path)
            .env("XDG_CONFIG_HOME", &root_path)
            .env("XDG_DATA_HOME", &root_path)
            .env(
                "RPI_WORK_CODEX_ACP_BIN",
                env!("CARGO_BIN_EXE_acp-workspace-fixture"),
            )
            .env("RPI_WORK_CODEX_ACP_ARGS", "[]")
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(65);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "isolated worker: {status}");
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("isolated worker exceeded 65s deadline");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        return;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(run());
}

fn error<T>(result: Result<T, NodeError>, kind: NodeErrorKind) -> NodeError {
    let err = result
        .err()
        .expect("operation must fail, not imply support");
    assert_eq!(err.kind, kind, "{err:?}");
    let printed = format!("{err:?} {err}");
    for secret in [
        "private-canary",
        "unused-rpi",
        "node-port-",
        "external-fixture",
        "/private/",
        "D:\\",
        "\\\\",
    ] {
        assert!(
            !printed.contains(secret),
            "diagnostic leaked {secret}: {printed}"
        );
    }
    err
}

async fn next(stream: &mut NodeEventStream) -> WireEvent {
    tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("event deadline")
        .expect("event stream ended")
        .expect("event error")
}

async fn run() {
    let root = PathBuf::from(std::env::var_os("WORK_NODE_PORT_OPERATIONS_WORKER").unwrap());
    let local = Arc::new(LocalWorkspace::new(root.join("unused-rpi"), root.join("data")).unwrap());
    let node = local.node_owner();
    let facade = LocalWorkspace::from_node_owner(node.clone());
    let client = WorkspaceClient::Local(local.clone());
    assert!(Arc::ptr_eq(&node, &facade.node_owner()));
    let ports: [&dyn NodeApi; 4] = [node.as_ref(), local.as_ref(), &facade, &client];
    let outcome = std::panic::AssertUnwindSafe(tokio::time::timeout(Duration::from_secs(40), async {
        let id = ports[0].create_session_with_backend(None, "codex").await.unwrap();
        let other = ports[3].create_session_with_backend(None, "codex").await.unwrap();
        let access = ports[0].session_access(&id).await.unwrap();
        assert!(!access.snapshot_limit);
        assert!(!access.supports_command("get_queue"));
        let config = ports[0].session_configuration(&id).await.unwrap();
        assert_eq!(config.protocol, "acp");
        assert_eq!(config.supported_commands, [] as [std::string::String; 0]);
        assert_eq!(config.config_options[0]["currentValue"], "balanced");
        let handle = local.prepare_agent_session(&id).await.unwrap().unwrap();
        let mut pid = None;
        for api in ports {
            assert_eq!(api.session_access(&id).await.unwrap(), access);
            assert_eq!(api.session_configuration(&id).await.unwrap(), config);
            let result = api.set_session_config_option(&id, "mode-choice", json!("fast")).await.unwrap();
            assert!(result["fixturePid"].is_u64());
            if let Some(ref expected) = pid { assert_eq!(&result["fixturePid"], expected); }
            pid = Some(result["fixturePid"].clone());
            assert_eq!(ports[0].session_configuration(&id).await.unwrap().config_options[0]["currentValue"], "fast");
            api.set_session_config_option(&id, "mode-choice", json!("balanced")).await.unwrap();
            assert_eq!(api.session_configuration(&other).await.unwrap(), config);
            error(api.set_session_config_option(&id, "mode-choice", json!("private-canary")).await, NodeErrorKind::Internal);
            error(api.set_session_config_option(&id, "unknown", json!("private-canary")).await, NodeErrorKind::Unsupported);
            // Strict fixture panics on unexpected protocol requests: these must be rejected before dispatch.
            macro_rules! unsupported { ($expr:expr) => { error($expr.await, NodeErrorKind::Unsupported); }; }
            unsupported!(api.models(&id));
            unsupported!(api.auth_status(&id));
            unsupported!(api.thinking_levels(&id));
            unsupported!(api.tools(&id));
            unsupported!(api.runtime_state(&id));
            unsupported!(api.active_tools_state(&id));
            unsupported!(api.history(&id, None, None, 10));
            unsupported!(api.session_branch(&id));
            unsupported!(api.set_model(&id, "private-canary"));
            unsupported!(api.set_thinking_level(&id, "private-canary"));
            unsupported!(api.set_active_tools(&id, &[]));
            unsupported!(api.resume_pending(&id));
            unsupported!(api.compact(&id, None));
            unsupported!(api.queue(&id, true));
            unsupported!(api.queue_message(&id, "private-canary", "follow_up"));
            unsupported!(api.cancel_queued(&id, "private-canary"));
            unsupported!(api.clear_queue(&id));
            error(api.session_access("private-canary").await, NodeErrorKind::NotFound);
            error(api.session_configuration("private-canary").await, NodeErrorKind::NotFound);
            error(api.prompt("private-canary", "test").await, NodeErrorKind::NotFound);
        }
        assert!(handle.set_config_option(&other, "mode-choice", json!("fast")).await.is_err());
        assert!(handle.subscribe(&other, 0).await.is_err());
        assert_eq!(ports[0].session_configuration(&other).await.unwrap(), config);

        // Subscribe before a live prompt through each port, compare complete journal prefixes.
        let mut streams = Vec::new();
        for api in ports { streams.push(api.subscribe(&id, 0).await.unwrap()); }
        for api in ports {
            api.prompt(&id, "fresh").await.unwrap();
            api.abort(&id).await.unwrap();
        }
        let last = ports[0].snapshot(&id).await.unwrap().last_seq;
        let mut journals = Vec::new();
        for stream in &mut streams {
            let mut events = Vec::new();
            loop {
                let event = next(stream).await;
                let seq = event.seq.expect("ACP journal sequence");
                assert_eq!(seq, events.len() as u64 + 1);
                events.push(event);
                if seq == last { break; }
            }
            journals.push(events);
        }
        assert!(journals[0].iter().any(|e| e.event_type == "message_end"));
        for events in &journals[1..] { assert_eq!(events, &journals[0]); }
        for api in ports {
            let mut replay = api.subscribe(&id, last - 1).await.unwrap();
            assert_eq!(next(&mut replay).await, *journals[0].last().unwrap());
        }

        // Permission IDs are owner/session scoped; rejecting a foreign ID must not settle the prompt.
        let mut events = ports[0].subscribe(&id, last).await.unwrap();
        let turn = ports[3].prompt(&id, "permission");
        tokio::pin!(turn);
        let key = loop {
            tokio::select! {
                result = &mut turn => panic!("permission settled early: {result:?}"),
                event = events.next() => {
                    let event = event.unwrap().unwrap();
                    if event.event_type == "ui_dialog_request" { break event.data["requestId"].as_str().unwrap().to_owned(); }
                }
            }
        };
        let answer = json!({"kind":"selector","values":["Yes"]});
        let mut rejection = None;
        for api in ports {
            let err = error(api.dialog_response(&other, &key, answer.clone()).await, NodeErrorKind::InvalidInput);
            if let Some(ref expected) = rejection { assert_eq!(&err, expected); }
            rejection = Some(err);
            error(api.cancel_dialog(&other, &key).await, NodeErrorKind::InvalidInput);
        }
        ports[1].dialog_response(&id, &key, answer).await.unwrap();
        turn.await.unwrap();
        for api in ports {
            error(api.dialog_response(&id, &key, json!(null)).await, NodeErrorKind::InvalidInput);
        }
    })).catch_unwind().await;
    // Cleanup also runs on assertion failure and async deadline expiry.
    tokio::time::timeout(Duration::from_secs(10), local.shutdown())
        .await
        .expect("shutdown deadline");
    for api in ports {
        error(
            api.session_access("closed").await,
            NodeErrorKind::Unavailable,
        );
        error(
            api.session_configuration("closed").await,
            NodeErrorKind::Unavailable,
        );
        error(
            api.set_session_config_option("closed", "mode-choice", json!("fast"))
                .await,
            NodeErrorKind::Unavailable,
        );
        error(
            api.prompt("closed", "test").await,
            NodeErrorKind::Unavailable,
        );
        error(api.abort("closed").await, NodeErrorKind::Unavailable);
        error(api.queue("closed", false).await, NodeErrorKind::Unavailable);
        error(api.subscribe("closed", 0).await, NodeErrorKind::Unavailable);
    }
    match outcome {
        Ok(result) => result.expect("scenario deadline"),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
