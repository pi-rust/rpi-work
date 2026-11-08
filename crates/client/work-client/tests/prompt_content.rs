#![cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
use serde_json::{Value, json};
use work_client::{ClientError, LocalWorkspace};

#[test]
fn acp_content_blocks_are_rejected_without_text_fallback() {
    if std::env::var_os("WORK_PROMPT_ACP_WORKER").is_none() {
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "acp_content_blocks_are_rejected_without_text_fallback"
                ])
                .env("WORK_PROMPT_ACP_WORKER", "1")
                .env(
                    "RPI_WORK_CODEX_ACP_BIN",
                    env!("CARGO_BIN_EXE_acp-workspace-fixture")
                )
                .env("RPI_WORK_CODEX_ACP_ARGS", "[]")
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
            let root = std::env::temp_dir().join(format!("work-prompt-acp-{}", std::process::id()));
            let local = LocalWorkspace::new("unused-rpi".into(), root.clone()).unwrap();
            let id = local
                .create_session_with_backend(None, "codex")
                .await
                .unwrap();
            let access = local.session_access(&id).await.unwrap();
            assert!(access.prompt_content.is_none());
            assert!(!access.supports_prompt_content());
            for blocks in [
                json!([{"type":"text","text":"text only"}]),
                json!([{"type":"image","mimeType":"image/jpeg","data":"/9j/"}]),
            ] {
                assert!(local.prompt_content(&id, &blocks).await.is_err());
                assert!(
                    local
                        .node_owner()
                        .command::<Value>(&id, json!({"type":"prompt","contentBlocks":blocks}))
                        .await
                        .is_err()
                );
            }
            local.prompt(&id, "fresh").await.unwrap();
            local.shutdown().await;
            drop(local);
            std::fs::remove_dir_all(root).unwrap();
        });
}

#[tokio::test]
async fn old_or_malformed_remote_profiles_fail_closed_before_post() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    for profile in [
        None,
        Some(Value::Null),
        Some(json!({"image":true,"contentBlocksVersion":1})),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let n = stream.read(&mut request).unwrap();
            assert!(
                String::from_utf8_lossy(&request[..n])
                    .starts_with("GET /api/v1/sessions/s/access ")
            );
            let mut body = json!({"descriptor":{"identity":{"protocol":"rpi","transport":"http"},"capabilities":{}},"supportedCommands":["prompt"],"snapshotLimit":true,"dialogReply":false});
            // Build valid descriptor via the public DTO, avoiding fixture assumptions.
            body["descriptor"] = serde_json::to_value(work_core::AgentDescriptor {
                identity: work_core::TransportIdentity::new("rpi", None, "http"),
                capabilities: work_core::CapabilitySet::http_default(),
            })
            .unwrap();
            if let Some(profile) = profile {
                body["promptContent"] = profile;
            }
            let body = body.to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            stream.flush().unwrap();
        });
        let client = work_client::WorkClient::new(&format!("http://{addr}")).unwrap();
        assert!(matches!(
            client
                .prompt_content("s", &json!([{"type":"text","text":"x"}]))
                .await,
            Err(ClientError::Unsupported { .. })
        ));
        thread.join().unwrap();
    }
}
