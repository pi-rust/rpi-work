#![cfg(not(target_arch = "wasm32"))]

use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use work_client::{
    AgentConnection, ClientError, MESSAGE_PAGE_SIZE, MessagePage, MessagePageInfo, RemoteTransport,
    Snapshot, WorkClient, WorkspaceClient,
};

fn snapshot() -> Snapshot {
    serde_json::from_value(json!({
        "sessionId":"s", "messages":[{"entryId":"e3"},{"entryId":"e4"}],
        "firstId":"e3", "lastId":"e4", "total":4, "hasMore":true, "lastSeq":7
    }))
    .unwrap()
}

fn page() -> Value {
    json!({
        "messages":[{"entryId":"e1"},{"entryId":"e2"}], "total":4,
        "firstId":"e1", "lastId":"e2", "beforeId":"e3", "hasMore":false
    })
}

#[test]
fn snapshot_page_info_preserves_has_more_without_inventing_cursors() {
    let mut snapshot = snapshot();
    assert!(snapshot.has_more);
    assert_eq!(MESSAGE_PAGE_SIZE, 48);
    assert_eq!(
        MessagePageInfo::from_snapshot(&snapshot),
        MessagePageInfo {
            first_id: Some("e3".into()),
            has_more: true,
            total: 4,
            leaf_id: None,
        }
    );
    for message in [
        json!({"id":"e3"}),
        json!({"messageId":"e3"}),
        json!({"entryId":""}),
        json!({"entryId":"anonymous-1-1"}),
    ] {
        snapshot.messages[0] = message;
        let info = MessagePageInfo::from(&snapshot);
        assert!(info.has_more);
        assert_eq!(info.first_id, None);
    }
    let mut snapshot = self::snapshot();
    snapshot.last_id = None;
    assert_eq!(MessagePageInfo::from(&snapshot).first_id, None);
    snapshot = self::snapshot();
    snapshot.messages[1] = json!({"entryId":"e3"});
    assert_eq!(MessagePageInfo::from(&snapshot).first_id, None);
    let old: Snapshot = serde_json::from_value(json!({"sessionId":"s", "has_more":true})).unwrap();
    assert!(old.has_more);
    let mut truncated = self::snapshot();
    truncated.has_more = false;
    truncated.truncated = true;
    assert!(MessagePageInfo::from(&truncated).has_more);
    assert_eq!(
        MessagePageInfo::from(&truncated).first_id.as_deref(),
        Some("e3")
    );
}

#[test]
fn pages_require_exact_echo_real_unique_ids_and_an_advancing_boundary() {
    let valid: MessagePage = serde_json::from_value(page()).unwrap();
    valid.validate_before("e3").unwrap();
    for (field, value) in [
        ("beforeId", Value::Null),
        ("beforeId", json!("other")),
        ("firstId", Value::Null),
        ("firstId", json!("wrong")),
        ("lastId", json!("wrong")),
        ("total", json!(2)),
    ] {
        let mut invalid = page();
        invalid[field] = value;
        let invalid: MessagePage = serde_json::from_value(invalid).unwrap();
        assert!(
            matches!(invalid.validate_before("e3"), Err(ClientError::Unsupported { operation, .. }) if operation == "message_pagination")
        );
    }
    for messages in [
        json!([{"id":"e1"},{"entryId":"e2"}]),
        json!([{"messageId":"e1"},{"entryId":"e2"}]),
        json!([{"entryId":"e1"},{"entryId":"e1"}]),
        json!([{"entryId":"e1"},{"entryId":"e3"}]),
        json!([{"entryId":" "},{"entryId":"e2"}]),
    ] {
        let mut invalid = page();
        invalid["messages"] = messages;
        let invalid: MessagePage = serde_json::from_value(invalid).unwrap();
        assert!(invalid.validate_before("e3").is_err());
    }
    let empty: MessagePage = serde_json::from_value(json!({
        "messages":[], "total":4, "firstId":null,"lastId":null,
        "beforeId":"e1", "hasMore":false
    }))
    .unwrap();
    empty.validate_before("e1").unwrap();
    let mut stalled = empty;
    stalled.has_more = true;
    assert!(stalled.validate_before("e1").is_err());
}

fn session_access() -> Value {
    serde_json::to_value(work_core::api::SessionAccess {
        descriptor: work_core::AgentDescriptor {
            identity: work_core::TransportIdentity::new("rpi", None, "http"),
            capabilities: work_core::CapabilitySet::http_default(),
        },
        supported_commands: ["get_messages", "prompt", "abort", "ui_dialog_response"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        snapshot_limit: true,
        dialog_reply: true,
        prompt_content: None,
    })
    .unwrap()
}

fn is_access_request(request: &str) -> bool {
    request.starts_with("GET /api/v1/sessions/s/access HTTP/1.1\r\n")
}

// The existing auth HTTP fixture is private to lib.rs; keep this tiny protocol
// server here so requests (including their JSON bodies) can be asserted exactly.
fn server(responses: Vec<(String, String)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let mut responses = responses.into_iter();
        let mut response = responses.next();
        let mut requests = Vec::new();
        while response.is_some() {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let end = loop {
                let mut chunk = [0u8; 4096];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..end]);
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|n| n.trim().parse().unwrap())
                })
                .unwrap_or(0);
            while request.len() < end + length {
                let mut chunk = [0u8; 4096];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
            }
            let request = String::from_utf8(request).unwrap();
            // Access is requested on open/reconnect, independently of the scripted
            // snapshot/event sequence. No other request may bypass that sequence.
            let (content_type, body) = if is_access_request(&request) {
                json_response(&session_access())
            } else {
                let current = response.take().unwrap();
                response = responses.next();
                current
            };
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            requests.push(request);
        }
        requests
    });
    (format!("http://{address}"), handle)
}

fn json_response(value: &Value) -> (String, String) {
    ("application/json".into(), value.to_string())
}

// This fixture deliberately implements only the legacy HTTP/SSE protocol.
// Transport discovery/fallback and authenticated WS behavior have their own
// real-server tests; pagination must not consume scripted replies as discovery.
fn http_client(url: &str) -> WorkClient {
    let client = WorkClient::new(url).unwrap();
    client.set_remote_transport(RemoteTransport::HttpSseOnly);
    client
}

#[tokio::test]
async fn remote_limit_query_and_cursor_command_body_are_explicit() {
    let (url, server) = server(vec![
        json_response(&serde_json::to_value(snapshot()).unwrap()),
        json_response(
            &json!({"type":"response","command":"get_messages","success":true,"data":page()}),
        ),
    ]);
    let client = http_client(&url);
    client.snapshot_with_limit("s", 48).await.unwrap();
    let workspace = WorkspaceClient::from(client);
    workspace.message_page("s", 48, "e3").await.unwrap();
    let requests = server.join().unwrap();
    assert!(requests[0].starts_with("GET /api/v1/sessions/s/snapshot?limit=48 HTTP/1.1"));
    assert!(requests[1].starts_with("POST /api/v1/sessions/s/commands HTTP/1.1"));
    let body: Value = serde_json::from_str(requests[1].split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(
        body,
        json!({"type":"get_messages","limit":48,"beforeId":"e3"})
    );
}

#[tokio::test]
async fn legacy_hosts_cannot_silently_ignore_before_id() {
    let mut ignored = page();
    ignored.as_object_mut().unwrap().remove("beforeId");
    let (url, server) = server(vec![json_response(&json!({
        "type":"response","command":"get_messages","success":true,"data":ignored
    }))]);
    let workspace = WorkspaceClient::from(http_client(&url));
    assert!(matches!(workspace.message_page("s", 48, "e3").await,
        Err(ClientError::Unsupported { operation, .. }) if operation == "message_pagination"));
    server.join().unwrap();
}

#[tokio::test]
async fn connection_and_run_refresh_snapshots_are_bounded_without_moving_replay_cursor() {
    for use_workspace in [false, true] {
        let events = "id: 8\nevent: agent_start\ndata: {\"type\":\"agent_start\"}\n\nid: 9\nevent: agent_settled\ndata: {\"type\":\"agent_settled\",\"outcome\":\"completed\"}\n\n";
        let mut refreshed = snapshot();
        refreshed.has_more = false;
        refreshed.total = 2;
        let mut restored = snapshot();
        restored.first_id = Some("older-boundary".into());
        restored.messages[0]["entryId"] = json!("older-boundary");
        restored.last_seq = 10;
        let (url, server) = server(vec![
            json_response(
                &json!({"sessionId":"s","title":"Test","status":"idle","createdAt":0,"updatedAt":0,"lastSeq":7}),
            ),
            json_response(&serde_json::to_value(snapshot()).unwrap()),
            ("text/event-stream".into(), events.into()),
            json_response(&serde_json::to_value(&refreshed).unwrap()),
            json_response(&serde_json::to_value(&refreshed).unwrap()),
            json_response(&serde_json::to_value(&restored).unwrap()),
            ("text/event-stream".into(), String::new()),
        ]);
        let client = http_client(&url);
        let connection = if use_workspace {
            WorkspaceClient::from(client)
                .connect_session("s")
                .await
                .unwrap()
        } else {
            client.connect_session("s").await.unwrap()
        };
        let mut connection = AgentConnection::Http(connection);
        assert!(connection.message_page_info().unwrap().has_more);
        connection.next().await.unwrap().unwrap();
        assert!(!connection.message_page_info().unwrap().has_more);
        connection.next().await.unwrap().unwrap();
        // EOF marks reconnecting; the next call restores a fresh bounded snapshot.
        connection.next().await.unwrap().unwrap();
        connection.next().await.unwrap().unwrap();
        assert_eq!(
            connection.message_page_info().unwrap().first_id.as_deref(),
            Some("older-boundary")
        );
        let requests = server.join().unwrap();
        assert!(
            requests
                .iter()
                .filter(|request| is_access_request(request))
                .count()
                >= 2
        );
        let requests: Vec<_> = requests
            .into_iter()
            .filter(|request| !is_access_request(request))
            .collect();
        for index in [1, 3, 4, 5] {
            assert!(requests[index].starts_with("GET /api/v1/sessions/s/snapshot?limit=48 "));
        }
        assert!(requests[2].starts_with("GET /api/v1/sessions/s/events?after=7 "));
        assert!(requests[6].starts_with("GET /api/v1/sessions/s/events?after=10 "));
    }
}

#[tokio::test]
async fn silent_child_agent_stays_active_on_open_and_reconnect_until_live_settlement() {
    use work_core::{RunPhase, SettlementOutcome};

    for use_workspace in [false, true] {
        let running = json!({
            "sessionId":"s", "status":"running", "lastSeq":12,
            "latestSettlement":{"outcome":"completed"}, "events":[]
        });
        let settled = json!({
            "sessionId":"s", "status":"ready", "lastSeq":15,
            "latestSettlement":{"outcome":"completed"}, "events":[]
        });
        let (url, server) = server(vec![
            json_response(&json!({
                "sessionId":"s", "status":"running", "createdAt":0,
                "updatedAt":0, "lastSeq":12
            })),
            json_response(&running),
            (
                "text/event-stream".into(),
                "event: ready\ndata: {\"type\":\"ready\"}\n\n".into(),
            ),
            json_response(&running),
            (
                "text/event-stream".into(),
                concat!(
                    "id: 13\nevent: tool_execution_end\ndata: {\"type\":\"tool_execution_end\",\"toolCallId\":\"child\",\"result\":\"done\"}\n\n",
                    "id: 14\nevent: agent_end\ndata: {\"type\":\"agent_end\"}\n\n",
                    "id: 15\nevent: agent_settled\ndata: {\"type\":\"agent_settled\",\"outcome\":\"completed\"}\n\n",
                )
                .into(),
            ),
            json_response(&settled),
        ]);
        let client = http_client(&url);
        let mut connection = if use_workspace {
            WorkspaceClient::from(client)
                .connect_session("s")
                .await
                .unwrap()
        } else {
            client.connect_session("s").await.unwrap()
        };
        assert_eq!(connection.projection().phase, RunPhase::Accepted);
        assert_eq!(connection.projection().cursor, Some(12));
        for event in ["ready", "connection_reconnecting", "connection_online"] {
            assert_eq!(
                connection.next().await.unwrap().unwrap().wire.event_type,
                event
            );
            assert_eq!(connection.projection().phase, RunPhase::Accepted);
            assert_eq!(connection.projection().cursor, Some(12));
        }
        for (event, phase, cursor) in [
            ("tool_execution_end", RunPhase::Tool, 13),
            ("agent_end", RunPhase::AwaitingSettlement, 14),
            (
                "agent_settled",
                RunPhase::Settled(SettlementOutcome::Completed),
                15,
            ),
        ] {
            assert_eq!(
                connection.next().await.unwrap().unwrap().wire.event_type,
                event
            );
            assert_eq!(connection.projection().phase, phase);
            assert_eq!(connection.projection().cursor, Some(cursor));
        }
        let requests = server.join().unwrap();
        assert!(
            requests
                .iter()
                .filter(|request| is_access_request(request))
                .count()
                >= 2
        );
        let requests: Vec<_> = requests
            .into_iter()
            .filter(|request| !is_access_request(request))
            .collect();
        for index in [2, 4] {
            assert!(requests[index].starts_with("GET /api/v1/sessions/s/events?after=12 "));
        }
        assert_eq!(requests.len(), 6);
        assert!(
            !requests
                .iter()
                .any(|request| request.contains("\"type\":\"prompt\""))
        );
    }
}

#[tokio::test]
async fn initial_connection_reuses_fresh_access_but_reconnect_renegotiates() {
    let (url, server) = server(vec![
        json_response(
            &json!({"sessionId":"s","title":"fixture","status":"idle","createdAt":0,"updatedAt":0,"lastSeq":7}),
        ),
        json_response(&serde_json::to_value(snapshot()).unwrap()),
        ("text/event-stream".into(), String::new()),
        json_response(&serde_json::to_value(snapshot()).unwrap()),
        ("text/event-stream".into(), String::new()),
    ]);
    let client = WorkspaceClient::from(http_client(&url));
    let access = client.session_access("s").await.unwrap();
    let mut connection = client
        .connect_session_with_access("s", &access)
        .await
        .unwrap();
    assert_eq!(connection.messages().len(), 2);
    assert_eq!(
        connection.next().await.unwrap().unwrap().wire.event_type,
        "connection_reconnecting"
    );
    assert_eq!(
        connection.next().await.unwrap().unwrap().wire.event_type,
        "connection_online"
    );
    let requests = server.join().unwrap();
    assert_eq!(
        requests.iter().filter(|r| is_access_request(r)).count(),
        2,
        "one initial observation plus one reconnect; no duplicate initial access"
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.starts_with("POST /api/v1/sessions/s/open "))
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.starts_with("GET /api/v1/sessions/s/snapshot?limit=48 "))
            .count(),
        2
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.starts_with("GET /api/v1/sessions/s/events?after=7 "))
            .count(),
        2
    );
}
