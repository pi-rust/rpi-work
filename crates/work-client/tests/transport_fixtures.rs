#![cfg(not(target_arch = "wasm32"))]

use serde_json::{Value, json};
use std::path::PathBuf;
use work_client::{AcpClient, AgentClient, RpiRpcClient};
use work_core::MessageBlock;

#[tokio::test]
async fn rpc_fixture_does_not_infer_omitted_commands_from_history() {
    let client = RpiRpcClient::spawn(env!("CARGO_BIN_EXE_rpc-fixture"), ".")
        .await
        .unwrap();
    let agent = AgentClient::from_transport(client);
    let session = agent.open_session(".").await.unwrap();
    assert!(agent.supports("history"));
    assert!(!agent.supports_command("compact"));
    assert!(matches!(
        agent.compact(session.id(), None).await,
        Err(work_client::ClientError::Unsupported { .. })
    ));
    agent.stop().await.unwrap();
}

#[tokio::test]
async fn rpc_fixture_round_trip_matches_responses_and_events() {
    let client = RpiRpcClient::spawn(env!("CARGO_BIN_EXE_rpc-fixture"), ".")
        .await
        .expect("RPC fixture starts");
    let descriptor = client.descriptor();
    assert_eq!(descriptor.identity.version.as_deref(), Some("1.2.0"));
    assert!(descriptor.capabilities.prompt);
    assert!(descriptor.capabilities.models);
    assert!(descriptor.capabilities.thinking_levels);
    assert!(descriptor.capabilities.tools);
    assert!(descriptor.capabilities.queue);
    assert!(descriptor.capabilities.history);
    assert!(descriptor.capabilities.provider_management);
    assert!(!descriptor.capabilities.projects);
    let agent = AgentClient::from_transport(client.clone());
    let session = agent.open_session(".").await.expect("RPC session");
    assert_ne!(session.id(), "");
    session
        .set_config_option("model", json!("fixture-model"))
        .await
        .expect("set model config");
    let response = agent
        .request(session.id(), "get_state", json!({}))
        .await
        .expect("response");
    assert_eq!(response["accepted"], true);
    assert_eq!(
        agent.messages(session.id(), Some(10), None).await.unwrap()["total"],
        0
    );
    assert_eq!(
        agent
            .history(session.id(), Some(10), None, None)
            .await
            .unwrap()["total"],
        0
    );
    assert!(matches!(
        agent.session_branch(session.id(), Some(10), None).await,
        Err(work_client::ClientError::Unsupported { operation, .. })
            if operation == "unfiltered_session_branch"
    ));
    let custom_preview = agent
        .request(
            session.id(),
            "get_session_branch",
            json!({"customTypes":["rpi-work:fixture"],"limit":10}),
        )
        .await
        .unwrap();
    assert_eq!(custom_preview["filtered"], true);
    assert_eq!(agent.queue(session.id(), false).await.unwrap()["count"], 0);
    assert_eq!(
        agent.models(session.id()).await.unwrap()["models"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        agent.thinking_levels(session.id()).await.unwrap()["levels"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        agent.tools(session.id()).await.unwrap()["tools"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        agent.auth_status(session.id()).await.unwrap()["providers"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(agent.stats(session.id()).await.unwrap()["messageCount"], 0);
    let mut connection = session.connect().await.expect("event subscription");
    let _ = session.prompt("hello").await.expect("prompt");
    let mut saw_text = false;
    let mut saw_settled = false;
    for _ in 0..4 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), connection.next())
            .await
            .expect("event timeout")
            .expect("event result")
            .expect("event");
        saw_text |= event.wire.event_type == "message_update"
            && event.wire.data["delta"] == "fixture reply";
        saw_settled |= event.wire.event_type == "agent_settled";
    }
    assert!(saw_text);
    assert!(saw_settled);
    client.stop().await.expect("stop");
}

#[tokio::test]
async fn acp_fixture_negotiates_session_and_streams_updates() {
    let client = AcpClient::spawn(env!("CARGO_BIN_EXE_acp-fixture"), &[], ".")
        .await
        .expect("ACP fixture starts");
    assert_eq!(
        client.descriptor().identity.agent_name.as_deref(),
        Some("fixture-agent")
    );
    let descriptor = client.descriptor();
    assert_eq!(
        descriptor.identity.agent_name.as_deref(),
        Some("fixture-agent")
    );
    assert!(!descriptor.capabilities.history);
    assert!(!descriptor.capabilities.session_load);
    let agent = AgentClient::from_transport(client.clone());
    let Err(error) = agent.load_session("existing", ".").await else {
        panic!("load session is not negotiated");
    };
    assert!(matches!(
        error,
        work_client::ClientError::Unsupported { .. }
    ));
    let session = agent.open_session(".").await.expect("new session");
    assert_eq!(session.id(), "session-fixture");
    let descriptor = agent.descriptor();
    assert!(!descriptor.capabilities.models);
    assert!(!descriptor.capabilities.thinking_levels);
    assert!(!descriptor.capabilities.tools);
    assert!(agent.supports_config_option(session.id(), "model"));
    assert!(agent.supports_config_option(session.id(), "thinking"));
    assert!(!agent.supports_config_option("unknown-session", "model"));
    assert!(!agent.supports_command("get_state"));
    let error = session
        .set_config_option("unknown", json!(true))
        .await
        .expect_err("unknown config option");
    assert!(matches!(
        error,
        work_client::ClientError::Unsupported { .. }
    ));
    session
        .set_config_option("model", json!("fixture-model"))
        .await
        .expect("set model config");
    let mut connection = session.connect().await.expect("event subscription");
    let result = session.prompt("hello").await.expect("prompt");
    assert_eq!(result["stopReason"], "end_turn");
    let mut saw_permission = false;
    let mut saw_message = false;
    for _ in 0..3 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), connection.next())
            .await
            .expect("event timeout")
            .expect("event result")
            .expect("event");
        if event.wire.event_type == "ui_dialog_request" {
            saw_permission = true;
            assert_eq!(event.wire.data["requestId"], "4");
            assert_eq!(event.wire.data["jsonRpcRequestId"], 4);
            agent
                .respond_to_agent_request(
                    json!(4),
                    json!({"outcome":{"outcome":"selected","optionId":"allow"}}),
                )
                .await
                .expect("permission response");
        }
        if event.wire.event_type == "message_update" {
            saw_message = true;
        }
    }
    assert!(saw_permission);
    assert!(saw_message);
    assert_eq!(connection.projection().blocks.len(), 1);
    match &connection.projection().blocks[0] {
        MessageBlock::Text { text, .. } => assert_eq!(text, "fixture reply"),
        block => panic!("unexpected projected block: {block:?}"),
    }
    client.stop().await.expect("stop");
}

#[tokio::test]
async fn acp_permission_round_trip_waits_for_validated_ui_or_policy_answer() {
    for string_id in [false, true] {
        for choice in ["ui-allow", "ui-reject", "policy-allow", "cancel"] {
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                let mut args = vec!["--wait-permission".into()];
                if string_id {
                    args.push("--string-permission-id".into());
                }
                let client = AcpClient::spawn(env!("CARGO_BIN_EXE_acp-fixture"), &args, ".")
                    .await.unwrap();
                let agent = AgentClient::from_transport(client.clone());
                let session = agent.open_session(".").await.unwrap();
                let mut connection = session.connect().await.unwrap();
                let mut prompt = std::pin::pin!(session.prompt("needs permission"));
                let event = tokio::select! {
                    result = &mut prompt => panic!("prompt completed before permission: {result:?}"),
                    event = connection.next() => event.unwrap().unwrap(),
                };
                assert_eq!(event.wire.event_type, "ui_dialog_request");
                let dialog = connection.pending_dialogs().first().unwrap().clone();
                let expected_id = if string_id { json!("4") } else { json!(4) };
                assert_eq!(dialog.response_id, Some(expected_id.clone()));
                assert_eq!(dialog.ui.purpose.as_deref(), Some("permission"));
                assert_eq!(dialog.tool_call_id.as_deref(), Some("tool-1"));

                // The next client request also has id=4; server-initiated
                // permission IDs must remain in a separate namespace.
                session.set_config_option("model", json!("fixture-model")).await.unwrap();
                let forged = json!({"outcome":{"outcome":"selected","optionId":"allow"}});
                assert!(agent.respond_to_agent_request(expected_id.clone(), forged).await.is_err());
                assert!(agent.respond_to_agent_request(json!("unknown"),
                    json!({"outcome":{"outcome":"cancelled"}})).await.is_err());
                let wrong_type = if string_id { json!(4) } else { json!("4") };
                assert!(agent.respond_to_agent_request(wrong_type,
                    json!({"outcome":{"outcome":"cancelled"}})).await.is_err());
                assert!(dialog.acp_permission_result(Some(&json!({"kind":"selector","values":["unlisted"]}))).is_err());
                tokio::select! {
                    result = &mut prompt => panic!("invalid response unblocked prompt: {result:?}"),
                    () = tokio::time::sleep(std::time::Duration::from_millis(20)) => {}
                }
                let result = match choice {
                    "ui-allow" => dialog.acp_permission_result(Some(&json!({"kind":"selector","values":["Allow"]}))),
                    "ui-reject" => dialog.acp_permission_result(Some(&json!({"kind":"selector","values":["Reject"]}))),
                    "policy-allow" => dialog.acp_permission_policy(true),
                    _ => dialog.acp_permission_policy(false),
                }.unwrap();
                if choice.ends_with("allow") {
                    assert_eq!(result["outcome"]["optionId"], "grant-42");
                }
                agent.respond_to_agent_request(expected_id.clone(), result.clone()).await.unwrap();
                assert!(agent.respond_to_agent_request(expected_id, result).await.is_err());
                let reply = prompt.await.unwrap();
                assert_eq!(reply["stopReason"], if choice == "cancel" { "cancelled" } else { "end_turn" });
                let mut saw_closed = false;
                let mut saw_settled = false;
                while !saw_closed || !saw_settled {
                    let event = connection.next().await.unwrap().unwrap();
                    saw_closed |= event.wire.event_type == "ui_dialog_closed";
                    saw_settled |= event.wire.event_type == "agent_settled";
                }
                assert_eq!(connection.pending_dialogs(), []);
                agent.stop().await.unwrap();
            }).await.expect("ACP permission round trip must not hang");
        }
    }
}

#[tokio::test]
async fn acp_fixture_loads_session_when_negotiated() {
    let client = AcpClient::spawn(
        env!("CARGO_BIN_EXE_acp-fixture"),
        &["--load-session".into()],
        ".",
    )
    .await
    .expect("ACP load fixture starts");
    let agent = AgentClient::from_transport(client.clone());
    assert!(!agent.supports("history"));
    assert!(agent.supports("session_load"));
    assert!(agent.supports("session_resume"));
    let session = agent
        .load_session("session-fixture", ".")
        .await
        .expect("session/load");
    assert_eq!(session.id(), "session-fixture");
    let resumed = agent
        .resume_session("session-fixture", ".")
        .await
        .expect("session/resume");
    assert_eq!(resumed.id(), "session-fixture");
    assert!(!agent.supports("models"));
    assert!(agent.supports_config_option(resumed.id(), "model"));
    client.stop().await.expect("stop");
}
#[tokio::test]
async fn acp_load_and_config_do_not_invent_catalog_or_history_operations() {
    let client = AcpClient::spawn(
        env!("CARGO_BIN_EXE_acp-fixture"),
        &["--load-session".into(), "--drop-config-after-set".into()],
        ".",
    )
    .await
    .unwrap();
    let agent = AgentClient::from_transport(client.clone());
    let first = agent.open_session(".").await.unwrap();
    let empty = agent.load_session("no-config", ".").await.unwrap();
    let alias = agent.resume_session("alias-session", ".").await.unwrap();
    assert!(agent.supports_config_option(first.id(), "model"));
    assert!(!agent.supports_config_option(empty.id(), "model"));
    assert!(agent.supports_config_option(alias.id(), "thinking"));
    assert!(!agent.supports("history") && !agent.supports("tools") && !agent.supports("models"));
    for command in [
        "get_state",
        "get_history",
        "get_messages",
        "get_session_branch",
        "get_stats",
        "get_available_tools",
        "set_active_tools",
        "get_available_models",
        "get_available_thinking_levels",
        "compact",
        "resume_pending",
        "get_queue",
    ] {
        assert!(
            !agent.supports_command(command),
            "{command} must not be advertised"
        );
    }
    for result in [
        agent.state(first.id()).await,
        agent.history(first.id(), None, None, None).await,
        agent.tools(first.id()).await,
        agent.models(first.id()).await,
        agent.thinking_levels(first.id()).await,
        agent.compact(first.id(), None).await,
    ] {
        assert!(matches!(
            result,
            Err(work_client::ClientError::Unsupported { .. })
        ));
    }
    // Fixture asserts the received ID is the advertised thought_level, not
    // the facade's thinking alias. Its response withdraws all config options.
    alias
        .set_config_option("thinking", json!("high"))
        .await
        .unwrap();
    assert!(!agent.supports_config_option(alias.id(), "thinking"));
    assert!(
        agent.supports_config_option(first.id(), "model"),
        "other session must be unchanged"
    );
    assert!(matches!(
        alias.set_config_option("thinking", json!("high")).await,
        Err(work_client::ClientError::Unsupported { .. })
    ));
    agent.stop().await.unwrap();
}

#[tokio::test]
#[ignore = "requires a local rpi runtime; performs ACP/RPC handshake and config negotiation"]
async fn installed_rpi_runtime_passes_rpc_and_acp_handshakes() {
    let executable =
        std::env::var_os("RPI_WORK_RPI_BIN").map_or_else(|| PathBuf::from("rpi"), PathBuf::from);
    let cwd = std::env::current_dir().expect("current directory");

    let rpc = AgentClient::spawn_rpc(&executable, &cwd)
        .await
        .expect("real RPC runtime starts");
    let session = rpc.open_session("").await.expect("real RPC session");
    let state = rpc
        .request(session.id(), "get_state", Value::Null)
        .await
        .expect("real RPC get_state");
    assert!(state.get("sessionId").is_some());
    if rpc.supports("thinking_levels") {
        session
            .set_config_option("thinking", json!("high"))
            .await
            .expect("real RPC config option");
    }
    if rpc.supports("history") {
        rpc.messages(session.id(), Some(20), None)
            .await
            .expect("real RPC messages");
        rpc.history(session.id(), Some(20), None, None)
            .await
            .expect("real RPC history");
        match rpc.session_branch(session.id(), Some(20), None).await {
            Ok(branch) => assert!(branch["entries"].is_array()),
            Err(work_client::ClientError::Unsupported { operation, .. })
                if operation == "unfiltered_session_branch" =>
            {
                eprintln!(
                    "runtime exposes only custom-entry branch previews; full branch/compaction summary is unsupported"
                );
                let preview = rpc
                    .request(
                        session.id(),
                        "get_session_branch",
                        json!({"customTypes":["rpi-work:smoke"],"limit":20}),
                    )
                    .await
                    .expect("real RPC explicit custom-entry query");
                assert_eq!(preview["filtered"], true);
            }
            result => panic!("unexpected real RPC session branch result: {result:?}"),
        }
        rpc.stats(session.id()).await.expect("real RPC stats");
    }
    if rpc.supports("queue") {
        rpc.queue(session.id(), false)
            .await
            .expect("real RPC queue");
    }
    if rpc.supports("models") {
        rpc.models(session.id()).await.expect("real RPC models");
    }
    if rpc.supports("thinking_levels") {
        rpc.thinking_levels(session.id())
            .await
            .expect("real RPC thinking levels");
    }
    if rpc.supports("tools") {
        rpc.tools(session.id()).await.expect("real RPC tools");
    }
    rpc.stop().await.expect("real RPC stops");

    let acp = AgentClient::spawn_acp(&executable, &["--acp".into()], &cwd)
        .await
        .expect("real ACP runtime initializes");
    let session = acp
        .open_session(cwd.to_string_lossy().as_ref())
        .await
        .expect("real ACP session/new");
    assert_ne!(session.id(), "");
    if acp.supports_config_option(session.id(), "thinking") {
        session
            .set_config_option("thinking", json!("high"))
            .await
            .expect("real ACP config option");
    }
    if acp.supports("session_resume") {
        let resumed = acp
            .resume_session(session.id(), cwd.to_string_lossy().as_ref())
            .await
            .expect("real ACP session/resume");
        assert_ne!(resumed.id(), "");
    }
    acp.stop().await.expect("real ACP stops");
}
