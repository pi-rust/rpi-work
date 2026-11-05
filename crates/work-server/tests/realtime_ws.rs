//! Real loopback WS/HTTP and an isolated native stub, never an Agent/model.
#[path = "../../../tests/support/legacy_fixture.rs"]
mod legacy_fixture;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use work_client::{RemoteTransport, WorkClient};
use work_core::api::realtime::{
    ClientFrame, Discovery, OperationOutcome, OperationRef, OperationStatus, RealtimeOperationKind,
    SUBPROTOCOL, ServerFrame, VERSION,
};
use work_node::workspace::LocalWorkspace;
use work_server::{HttpServer, ServerConfig};
type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

struct Fixture {
    server: HttpServer,
    owner: Arc<LocalWorkspace>,
    root: tempfile::TempDir,
    cookie: String,
    session: String,
}
impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let owner = Arc::new(
            LocalWorkspace::with_session_dir(
                PathBuf::from(env!("CARGO_BIN_EXE_work-server-rpc-fixture")),
                root.path().join("data"),
                Some(root.path().join("sessions")),
            )
            .unwrap(),
        );
        let server = HttpServer::start(
            owner.clone(),
            ServerConfig {
                bind: "127.0.0.1:0".parse().unwrap(),
                password: "isolated-ws-fixture".into(),
                allowed_hosts: vec![],
                node_id: "isolated-ws-node".into(),
                assets: None,
                web_proxy: false,
                allow_sharing: true,
            },
        )
        .unwrap();
        let project = owner
            .create_project("ws fixture", None, false)
            .await
            .unwrap();
        let session = legacy_fixture::seed_legacy(
            &root.path().join("data"),
            &project.id,
            "ws-native-fixture",
        );
        let login = reqwest::Client::new()
            .post(format!("http://{}/api/v1/auth", server.address()))
            .header("x-rpi-web", "1")
            .json(&json!({"password":"isolated-ws-fixture"}))
            .send()
            .await
            .unwrap();
        assert_eq!(login.status(), 200);
        let cookie = login
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        Self {
            server,
            owner,
            root,
            cookie,
            session,
        }
    }
    fn request(&self) -> tokio_tungstenite::tungstenite::http::Request<()> {
        let mut request = format!("ws://{}/api/v1/realtime", self.server.address())
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("cookie", self.cookie.parse().unwrap());
        request
            .headers_mut()
            .insert("sec-websocket-protocol", SUBPROTOCOL.parse().unwrap());
        request
    }
    async fn connect(&self) -> (Ws, work_core::api::realtime::Hello) {
        let (mut ws, response) = connect_async(self.request()).await.unwrap();
        assert_eq!(response.status(), 101);
        assert_eq!(response.headers()["sec-websocket-protocol"], SUBPROTOCOL);
        let ServerFrame::Hello { hello, .. } = next(&mut ws).await else {
            panic!("hello required")
        };
        assert_eq!(hello.node_id, "isolated-ws-node");
        (ws, hello)
    }
    async fn finish(mut self) {
        self.server.drain().await.unwrap();
        self.owner.shutdown().await;
    }
}
async fn next(ws: &mut Ws) -> ServerFrame {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match ws.next().await.unwrap().unwrap() {
                Message::Text(text) => return ServerFrame::decode_text(text.as_str()).unwrap(),
                Message::Ping(_) => ws.flush().await.unwrap(),
                Message::Pong(_) => {}
                _ => panic!("unexpected close or binary frame"),
            }
        }
    })
    .await
    .unwrap()
}
async fn send(ws: &mut Ws, frame: ClientFrame) {
    ws.send(Message::Text(frame.encode_text().unwrap().into()))
        .await
        .unwrap();
}
async fn reserve(ws: &mut Ws, session: &str, id: &str) -> OperationRef {
    send(
        ws,
        ClientFrame::Reserve {
            version: VERSION,
            request_id: id.into(),
            session_id: session.into(),
            client_instance_id: "isolated-client".into(),
            kind: RealtimeOperationKind::SessionCommand,
        },
    )
    .await;
    loop {
        if let ServerFrame::Reserved { operation, .. } = next(ws).await {
            return operation;
        }
    }
}
async fn status(ws: &mut Ws) -> OperationStatus {
    loop {
        if let ServerFrame::Status { status, .. } = next(ws).await {
            return status;
        }
    }
}
async fn subscribe(ws: &mut Ws, session: &str, sub: &str, after: u64, id: &str) {
    send(
        ws,
        ClientFrame::Subscribe {
            version: VERSION,
            request_id: id.into(),
            session_id: session.into(),
            subscription_id: sub.into(),
            after_seq: Some(after),
        },
    )
    .await;
    loop {
        if let ServerFrame::Subscribed {
            subscription_id, ..
        } = next(ws).await
        {
            assert_eq!(subscription_id, sub);
            break;
        }
    }
}
async fn closed(ws: &mut Ws) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match ws.next().await {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(Message::Ping(_))) => {
                    let _ = ws.flush().await;
                }
                _ => {}
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn handshake_discovery_origin_cookie_protocol_and_query_are_checked() {
    let f = Fixture::new().await;
    let http = reqwest::Client::new();
    let discovery_url = format!("http://{}/api/v1/transports", f.server.address());
    assert_eq!(http.get(&discovery_url).send().await.unwrap().status(), 401);
    let discovery: Discovery = http
        .get(&discovery_url)
        .header("cookie", &f.cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    discovery.validate().unwrap();
    assert_eq!(discovery.node_id, "isolated-ws-node");
    for (header, value, expected) in [
        ("cookie", "", 401),
        ("origin", "https://evil.invalid", 403),
        ("origin", "null", 403),
        ("host", "evil.invalid", 403),
        ("sec-websocket-protocol", "unsupported.v1", 400),
    ] {
        let mut request = f.request();
        request.headers_mut().insert(
            tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(header.as_bytes())
                .unwrap(),
            value.parse().unwrap(),
        );
        let error = connect_async(request).await.unwrap_err();
        let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
            panic!("expected handshake rejection")
        };
        assert_eq!(response.status().as_u16(), expected);
    }
    let mut request = f.request();
    *request.uri_mut() = format!(
        "ws://{}/api/v1/realtime?not_auth=ignored",
        f.server.address()
    )
    .parse()
    .unwrap();
    let error = connect_async(request).await.unwrap_err();
    assert!(
        matches!(error, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 400)
    );
    let (mut ws, hello) = f.connect().await;
    assert_eq!(hello.owner_epoch, discovery.owner_epoch);
    send(
        &mut ws,
        ClientFrame::Ping {
            version: VERSION,
            request_id: "ping-1".into(),
        },
    )
    .await;
    assert!(
        matches!(next(&mut ws).await, ServerFrame::Pong {request_id, ..} if request_id == "ping-1")
    );
    ws.close(None).await.unwrap();
    f.finish().await;
}

#[tokio::test]
async fn ws_execution_and_http_status_share_receipts_without_second_native_write() {
    let f = Fixture::new().await;
    let (mut ws, _) = f.connect().await;
    let reference = reserve(&mut ws, &f.session, "reserve-1").await;
    let payload = json!({"type":"set_thinking_level","level":"medium"});
    send(
        &mut ws,
        ClientFrame::Execute {
            version: VERSION,
            request_id: "exec-1".into(),
            operation: reference.clone(),
            payload: payload.clone(),
        },
    )
    .await;
    let first = status(&mut ws).await;
    let OperationStatus::KnownResult {
        outcome: OperationOutcome::Succeeded { value },
    } = &first
    else {
        panic!("expected RPC result, not Agent settlement")
    };
    assert_eq!(value["success"], true);
    assert_eq!(value["data"]["request"]["level"], "medium");
    assert!(value["data"]["request"]["id"].is_string());
    send(
        &mut ws,
        ClientFrame::Execute {
            version: VERSION,
            request_id: "exec-duplicate".into(),
            operation: reference.clone(),
            payload,
        },
    )
    .await;
    assert_eq!(
        status(&mut ws).await,
        first,
        "native request ID must be cached, not regenerated by a second write"
    );
    let response = reqwest::Client::new()
        .get(format!(
            "http://{}/api/v1/realtime/operations/{}/{}",
            f.server.address(),
            reference.owner_epoch,
            reference.operation_id
        ))
        .header("cookie", &f.cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.json::<OperationStatus>().await.unwrap(), first);
    send(
        &mut ws,
        ClientFrame::Execute {
            version: VERSION,
            request_id: "exec-conflict".into(),
            operation: reference,
            payload: json!({"type":"set_thinking_level","level":"high"}),
        },
    )
    .await;
    assert!(
        matches!(next(&mut ws).await, ServerFrame::Error {error, ..} if error.code == work_core::api::realtime::RealtimeErrorCode::Conflict)
    );
    ws.close(None).await.unwrap();
    f.finish().await;
}

#[tokio::test]
async fn disconnected_started_rpc_completes_and_reconnect_restores_session_cursor() {
    let f = Fixture::new().await;
    let (mut ws, first_hello) = f.connect().await;
    let before = f
        .owner
        .command::<Value>(&f.session, json!({"type":"get_state"}))
        .await
        .unwrap()
        .data
        .unwrap()["fixturePid"]
        .clone();
    assert!(before.is_u64());
    subscribe(&mut ws, &f.session, "sub-old", 0, "subscribe-old").await;
    let reference = reserve(&mut ws, &f.session, "reserve-gated").await;
    let release = f.root.path().join("release-ws-rpc");
    send(&mut ws, ClientFrame::Execute {version: VERSION, request_id: "exec-gated".into(), operation: reference.clone(), payload: json!({"type":"prompt","message":format!("realtime-auth-gate:{}", release.display())})}).await;
    let cursor = loop {
        if let ServerFrame::Events { events, .. } = next(&mut ws).await {
            if let Some(event) = events
                .iter()
                .find(|event| event.event_type == "fixture_started")
            {
                break event.seq.unwrap();
            }
        }
    };
    ws.close(None).await.unwrap();
    drop(ws);
    let (mut ws, hello) = f.connect().await;
    assert_eq!(hello.owner_epoch, first_hello.owner_epoch);
    assert_ne!(hello.connection_id, first_hello.connection_id);
    send(
        &mut ws,
        ClientFrame::Status {
            version: VERSION,
            request_id: "status-dispatching".into(),
            operation: reference.clone(),
        },
    )
    .await;
    assert_eq!(status(&mut ws).await, OperationStatus::Dispatching);
    std::fs::write(&release, b"release isolated stub").unwrap();
    for i in 0..200 {
        send(
            &mut ws,
            ClientFrame::Status {
                version: VERSION,
                request_id: format!("status-{i}"),
                operation: reference.clone(),
            },
        )
        .await;
        match status(&mut ws).await {
            OperationStatus::KnownResult {
                outcome: OperationOutcome::Succeeded { value },
            } => {
                assert_eq!(value["data"]["accepted"], true);
                break;
            }
            OperationStatus::Dispatching => {
                assert!(i < 199);
                tokio::time::sleep(Duration::from_millis(15)).await;
            }
            _ => panic!("receipt must survive disconnect"),
        }
    }
    subscribe(&mut ws, &f.session, "sub-new", cursor, "subscribe-new").await;
    loop {
        if let ServerFrame::Events {
            session_id,
            subscription_id,
            events,
            ..
        } = next(&mut ws).await
        {
            assert_eq!(session_id, f.session);
            assert_eq!(subscription_id, "sub-new");
            for event in &events {
                if let Some(seq) = event.seq {
                    assert!(seq > cursor);
                }
            }
            if events
                .iter()
                .any(|event| event.event_type == "agent_settled")
            {
                break;
            }
        }
    }
    let after = f
        .owner
        .command::<Value>(&f.session, json!({"type":"get_state"}))
        .await
        .unwrap()
        .data
        .unwrap()["fixturePid"]
        .clone();
    assert_eq!(before, after);
    ws.close(None).await.unwrap();
    f.finish().await;
}

#[tokio::test]
async fn logout_sharing_and_drain_close_upgraded_connections() {
    let mut f = Fixture::new().await;
    let (mut ws, _) = f.connect().await;
    let response = reqwest::Client::new()
        .delete(format!("http://{}/api/v1/auth", f.server.address()))
        .header("cookie", &f.cookie)
        .header("x-rpi-web", "1")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    closed(&mut ws).await;
    // A second isolated authenticated principal; no production auth state touched.
    let login = reqwest::Client::new()
        .post(format!("http://{}/api/v1/auth", f.server.address()))
        .header("x-rpi-web", "1")
        .json(&json!({"password":"isolated-ws-fixture"}))
        .send()
        .await
        .unwrap();
    f.cookie = login.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .into();
    let (mut ws, _) = f.connect().await;
    f.server.sharing_control().set_enabled(false);
    closed(&mut ws).await;
    f.server.sharing_control().set_enabled(true);
    let login = reqwest::Client::new()
        .post(format!("http://{}/api/v1/auth", f.server.address()))
        .header("x-rpi-web", "1")
        .json(&json!({"password":"isolated-ws-fixture"}))
        .send()
        .await
        .unwrap();
    f.cookie = login.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .into();
    let (mut ws, _) = f.connect().await;
    f.server.drain().await.unwrap();
    closed(&mut ws).await;
    f.owner.shutdown().await;
}

#[tokio::test]
async fn reused_request_ids_binary_and_invalid_envelopes_close_without_execution() {
    let f = Fixture::new().await;
    let (mut ws, _) = f.connect().await;
    for _ in 0..2 {
        send(
            &mut ws,
            ClientFrame::Ping {
                version: VERSION,
                request_id: "reused".into(),
            },
        )
        .await;
    }
    closed(&mut ws).await;
    let (mut ws, _) = f.connect().await;
    ws.send(Message::Binary(vec![0; 8].into())).await.unwrap();
    closed(&mut ws).await;
    let (mut ws, _) = f.connect().await;
    ws.send(Message::Text(
        r#"{"type":"ping","version":1,"requestId":"invalid","extra":"no"}"#.into(),
    ))
    .await
    .unwrap();
    closed(&mut ws).await;
    f.finish().await;
}

#[tokio::test]
async fn multiplexed_sessions_keep_their_own_sequences_and_subscription_identity() {
    let f = Fixture::new().await;
    let project = f
        .owner
        .create_project("second isolated project", None, false)
        .await
        .unwrap();
    let second = legacy_fixture::seed_legacy(
        &f.root.path().join("data"),
        &project.id,
        "ws-second-fixture",
    );
    let (mut ws, _) = f.connect().await;
    subscribe(&mut ws, &f.session, "first", 0, "subscribe-first").await;
    subscribe(&mut ws, &second, "second", 0, "subscribe-second").await;
    for session in [&f.session, &second] {
        assert!(
            f.owner
                .command::<Value>(
                    session,
                    json!({"type":"prompt","message":"isolated multiplex fixture"})
                )
                .await
                .unwrap()
                .success
        );
    }
    let mut last = std::collections::HashMap::<String, u64>::new();
    let mut settled = std::collections::HashSet::new();
    while settled.len() < 2 {
        if let ServerFrame::Events {
            session_id,
            subscription_id,
            events,
            ..
        } = next(&mut ws).await
        {
            assert_eq!(
                subscription_id,
                if session_id == f.session {
                    "first"
                } else {
                    assert_eq!(session_id, second);
                    "second"
                }
            );
            for event in events {
                if let Some(seq) = event.seq {
                    assert!(seq > *last.get(&session_id).unwrap_or(&0));
                    last.insert(session_id.clone(), seq);
                }
                if event.event_type == "agent_settled" {
                    settled.insert(session_id.clone());
                }
            }
        }
    }
    assert!(last.contains_key(&f.session) && last.contains_key(&second));
    ws.close(None).await.unwrap();
    f.finish().await;
}

#[tokio::test]
async fn active_subscription_limit_and_tombstones_are_bounded_and_not_rebindable() {
    let f = Fixture::new().await;
    let (mut ws, _) = f.connect().await;
    for i in 0..16 {
        subscribe(
            &mut ws,
            &f.session,
            &format!("sub-{i}"),
            0,
            &format!("request-{i}"),
        )
        .await;
    }
    send(
        &mut ws,
        ClientFrame::Subscribe {
            version: VERSION,
            request_id: "over-limit".into(),
            session_id: f.session.clone(),
            subscription_id: "over-limit".into(),
            after_seq: None,
        },
    )
    .await;
    loop {
        if let ServerFrame::Error {
            request_id, error, ..
        } = next(&mut ws).await
        {
            assert_eq!(request_id.as_deref(), Some("over-limit"));
            assert_eq!(
                error.code,
                work_core::api::realtime::RealtimeErrorCode::LimitExceeded
            );
            break;
        }
    }
    send(
        &mut ws,
        ClientFrame::Unsubscribe {
            version: VERSION,
            request_id: "unsubscribe".into(),
            subscription_id: "sub-0".into(),
        },
    )
    .await;
    loop {
        if matches!(next(&mut ws).await, ServerFrame::Unsubscribed { .. }) {
            break;
        }
    }
    send(
        &mut ws,
        ClientFrame::Subscribe {
            version: VERSION,
            request_id: "rebind-old".into(),
            session_id: f.session.clone(),
            subscription_id: "sub-0".into(),
            after_seq: None,
        },
    )
    .await;
    loop {
        if let ServerFrame::Error { error, .. } = next(&mut ws).await {
            assert_eq!(
                error.code,
                work_core::api::realtime::RealtimeErrorCode::Conflict
            );
            break;
        }
    }
    subscribe(&mut ws, &f.session, "replacement", 0, "fresh-subscription").await;
    ws.close(None).await.unwrap();
    f.finish().await;
}

#[tokio::test]
async fn node_connection_quota_rejects_excess_and_releases_closed_slots() {
    let f = Fixture::new().await;
    let mut connections = Vec::new();
    for _ in 0..16 {
        connections.push(f.connect().await.0);
    }
    let error = connect_async(f.request()).await.unwrap_err();
    assert!(
        matches!(error, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 429)
    );
    for mut ws in connections {
        ws.close(None).await.unwrap();
        closed(&mut ws).await;
    }
    let (mut ws, _) = f.connect().await;
    ws.close(None).await.unwrap();
    f.finish().await;
}

#[tokio::test]
async fn slow_ws_peer_is_disconnected_without_blocking_owner_or_other_transports() {
    let f = Fixture::new().await;
    let (mut slow, _) = f.connect().await;
    let MaybeTlsStream::Plain(tcp) = slow.get_ref() else {
        panic!("fixture must use loopback plain TCP")
    };
    socket2::SockRef::from(tcp)
        .set_recv_buffer_size(64 * 1024)
        .unwrap();
    subscribe(&mut slow, &f.session, "slow", 0, "slow-subscribe").await;
    let pid = f
        .owner
        .command::<Value>(&f.session, json!({"type":"get_state"}))
        .await
        .unwrap()
        .data
        .unwrap()["fixturePid"]
        .clone();
    assert!(pid.is_u64());
    // Do not poll the WS while the native stub produces 8 MiB. One connection's
    // 4 MiB outbox must never await its peer or stall the owner's IPC reader.
    assert!(
        f.owner
            .command::<Value>(
                &f.session,
                json!({"type":"prompt","message":"realtime-ws-burst-fixture"})
            )
            .await
            .unwrap()
            .success
    );
    tokio::time::sleep(Duration::from_millis(750)).await;
    let (mut healthy, _) = f.connect().await;
    send(
        &mut healthy,
        ClientFrame::Ping {
            version: VERSION,
            request_id: "healthy-ping".into(),
        },
    )
    .await;
    assert!(matches!(next(&mut healthy).await, ServerFrame::Pong { .. }));
    let after = tokio::time::timeout(
        Duration::from_secs(5),
        f.owner
            .command::<Value>(&f.session, json!({"type":"get_state"})),
    )
    .await
    .unwrap()
    .unwrap()
    .data
    .unwrap()["fixturePid"]
        .clone();
    assert_eq!(pid, after);
    let reason = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match slow.next().await {
                Some(Ok(Message::Close(frame))) => break frame.map(|frame| u16::from(frame.code)),
                Some(Err(_)) | None => break None,
                Some(Ok(Message::Ping(_))) => {
                    let _ = slow.flush().await;
                }
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    if let Some(code) = reason {
        assert_eq!(code, 1013);
    }
    healthy.close(None).await.unwrap();
    f.finish().await;
}

#[tokio::test]
async fn peer_that_never_polls_or_answers_protocol_heartbeat_expires() {
    let f = Fixture::new().await;
    let (mut ws, _) = f.connect().await;
    // Actual production 15s/45s timers, not an AuthLease unit test. Native
    // tungstenite auto-pong runs only when polled; we intentionally stop polling.
    tokio::time::sleep(Duration::from_secs(46)).await;
    closed(&mut ws).await;
    f.finish().await;
}

#[tokio::test]
async fn ws_status_rejects_another_login_principal_and_oversized_execute() {
    let f = Fixture::new().await;
    let (mut first, _) = f.connect().await;
    let reference = reserve(&mut first, &f.session, "reserve-scope").await;
    let login = reqwest::Client::new()
        .post(format!("http://{}/api/v1/auth", f.server.address()))
        .header("x-rpi-web", "1")
        .json(&json!({"password":"isolated-ws-fixture"}))
        .send()
        .await
        .unwrap();
    let cookie = login.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let mut request = f.request();
    request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    let (mut second, _) = connect_async(request).await.unwrap();
    assert!(matches!(next(&mut second).await, ServerFrame::Hello { .. }));
    send(
        &mut second,
        ClientFrame::Status {
            version: VERSION,
            request_id: "other-principal".into(),
            operation: reference.clone(),
        },
    )
    .await;
    assert!(
        matches!(next(&mut second).await, ServerFrame::Error {error, ..} if error.code == work_core::api::realtime::RealtimeErrorCode::AccessDenied)
    );
    second.close(None).await.unwrap();
    let raw = serde_json::to_string(&json!({"type":"execute","version":VERSION,"requestId":"oversized","operation":reference,"payload":{"type":"prompt","message":"x".repeat(work_core::api::realtime::MAX_MESSAGE_BYTES)}})).unwrap();
    let _ = first.send(Message::Text(raw.into())).await;
    closed(&mut first).await;
    let response = reqwest::Client::new()
        .get(format!(
            "http://{}/api/v1/realtime/operations/{}/{}",
            f.server.address(),
            reference.owner_epoch,
            reference.operation_id
        ))
        .header("cookie", &f.cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.json::<OperationStatus>().await.unwrap(),
        OperationStatus::Reserved,
        "oversized frame must not publish execution"
    );
    f.finish().await;
}

#[tokio::test]
async fn work_client_can_explicitly_use_ws_events_with_http_auth_cookie_jar() {
    let f = Fixture::new().await;
    let client = WorkClient::new(&format!("http://{}", f.server.address())).unwrap();
    assert_eq!(client.remote_transport(), RemoteTransport::Auto);
    client.login("isolated-ws-fixture").await.unwrap();
    client.set_remote_transport(RemoteTransport::WebSocketOnly);
    let mut events = client.subscribe(&f.session, 0).await.unwrap();
    let response = f
        .owner
        .command::<Value>(
            &f.session,
            json!({"type":"prompt","message":"work-client-ws-event-fixture"}),
        )
        .await
        .unwrap();
    assert!(response.success);
    let mut settled = false;
    for _ in 0..32 {
        let Some(event) = tokio::time::timeout(Duration::from_secs(2), events.next())
            .await
            .unwrap()
        else {
            break;
        };
        let event = event.unwrap();
        if event.event_type == "agent_settled" {
            settled = true;
            break;
        }
    }
    assert!(
        settled,
        "the existing WorkClient projection source must receive WS WireEvents"
    );
    client.logout().await.unwrap();
    f.finish().await;
}

#[tokio::test]
async fn work_client_websocket_only_routes_mutation_through_receipt_executor() {
    let f = Fixture::new().await;
    let client = WorkClient::new(&format!("http://{}", f.server.address())).unwrap();
    client.login("isolated-ws-fixture").await.unwrap();
    client.set_remote_transport(RemoteTransport::WebSocketOnly);
    let response: work_client::HostResponse<Value> = client
        .command(
            &f.session,
            json!({"type":"set_thinking_level","level":"medium"}),
        )
        .await
        .unwrap();
    assert!(response.success);
    assert_eq!(response.data.unwrap()["request"]["level"], "medium");
    // The command returns the adapter HostResponse; it does not claim agent
    // settlement and it uses the same server-side operation receipt path.
    client.logout().await.unwrap();
    f.finish().await;
}

#[tokio::test]
async fn websocket_execute_disconnect_returns_receipt_reference_without_retry() {
    let mut f = Fixture::new().await;
    let client = WorkClient::new(&format!("http://{}", f.server.address())).unwrap();
    client.login("isolated-ws-fixture").await.unwrap();
    client.set_remote_transport(RemoteTransport::WebSocketOnly);
    let release = f.root.path().join("release-client-unknown");
    let session = f.session.clone();
    let command_release = release.clone();
    let command = tokio::spawn(async move {
        client.command::<Value>(&session, json!({"type":"prompt","message":format!("realtime-auth-gate:{}", command_release.display())})).await
    });
    let mut owner_events = f.owner.subscribe(&f.session, 0).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if owner_events.next().await.unwrap().unwrap().event_type == "fixture_started" {
                break;
            }
        }
    })
    .await
    .unwrap();
    f.server.begin_shutdown();
    std::fs::write(&release, b"release client unknown fixture").unwrap();
    let error = command.await.unwrap().unwrap_err();
    let operation = match error {
        work_client::ClientError::OperationOutcomeUnknown { operation } => operation,
        other => panic!("expected a queryable unknown receipt, got {other:?}"),
    };
    assert!(!operation.operation_id.is_empty());
    f.server.drain().await.unwrap();
    f.owner.shutdown().await;
}

#[tokio::test]
async fn websocket_only_never_silently_downgrades_an_unauthenticated_client() {
    let f = Fixture::new().await;
    let client = WorkClient::new(&format!("http://{}", f.server.address())).unwrap();
    client.set_remote_transport(RemoteTransport::WebSocketOnly);
    let error = match client.subscribe(&f.session, 0).await {
        Ok(_) => panic!("unauthenticated WebSocketOnly must not downgrade"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        work_client::ClientError::WebSocket { status: Some(401) }
    ));
    f.finish().await;
}

#[tokio::test]
async fn auto_mode_keeps_http_control_plane_and_uses_ws_when_advertised() {
    let f = Fixture::new().await;
    let client = WorkClient::new(&format!("http://{}", f.server.address())).unwrap();
    client.login("isolated-ws-fixture").await.unwrap();
    client.set_remote_transport(RemoteTransport::Auto);
    let mut events = client.subscribe(&f.session, 0).await.unwrap();
    assert!(
        f.owner
            .command::<Value>(
                &f.session,
                json!({"type":"prompt","message":"auto-ws-event-fixture"})
            )
            .await
            .unwrap()
            .success
    );
    let first = tokio::time::timeout(Duration::from_secs(2), events.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(
        first.event_type.as_str(),
        "ready" | "operation_accepted" | "agent_start" | "message_start" | "message_update"
    ));
    client.logout().await.unwrap();
    f.finish().await;
}

#[tokio::test]
async fn cursor_ahead_of_session_log_requires_explicit_resync() {
    let f = Fixture::new().await;
    let (mut ws, _) = f.connect().await;
    subscribe(
        &mut ws,
        &f.session,
        "future-cursor",
        u64::MAX,
        "future-cursor-request",
    )
    .await;
    let mut ready_keys = Vec::new();
    let resync = tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            if let ServerFrame::Events { events, .. } = next(&mut ws).await {
                for event in events {
                    if event.event_type == "ready" {
                        ready_keys = event
                            .data
                            .as_object()
                            .map(|m| m.keys().cloned().collect())
                            .unwrap_or_default();
                    }
                    if event.event_type == "resync_required" {
                        return;
                    }
                }
            }
        }
    })
    .await;
    assert!(
        resync.is_ok(),
        "future cursor cannot silently wait forever; ready metadata keys={ready_keys:?}"
    );
    ws.close(None).await.unwrap();
    f.finish().await;
}

#[tokio::test]
async fn readonly_queries_do_not_consume_mutation_reservations_or_allow_writes() {
    let f = Fixture::new().await;
    let (mut ws, hello) = f.connect().await;
    assert!(hello.capabilities.command_queries);
    // Exhaust this principal's outstanding reservation quota, then prove that
    // normal live-state reads still work without reserve/execute side effects.
    for i in 0..32 {
        reserve(&mut ws, &f.session, &format!("reserved-{i}")).await;
    }
    send(
        &mut ws,
        ClientFrame::Query {
            version: VERSION,
            request_id: "read-state".into(),
            session_id: f.session.clone(),
            payload: json!({"type":"get_state"}),
        },
    )
    .await;
    let ServerFrame::Result {
        request_id, value, ..
    } = next(&mut ws).await
    else {
        panic!("read must not require an operation reservation")
    };
    assert_eq!(request_id, "read-state");
    assert_eq!(value["success"], true);
    assert!(value["data"]["fixturePid"].is_u64());
    let invalid = ClientFrame::Query {
        version: VERSION,
        request_id: "not-a-read".into(),
        session_id: f.session.clone(),
        payload: json!({"type":"prompt","message":"must not execute"}),
    };
    ws.send(Message::Text(
        serde_json::to_string(&invalid).unwrap().into(),
    ))
    .await
    .unwrap();
    closed(&mut ws).await;
    f.finish().await;
}
