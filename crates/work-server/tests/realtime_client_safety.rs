//! Adversarial protocol peers on random loopback ports, without any owner/Agent.
use axum::{
    Json, Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::oneshot;
use work_client::{ClientError, RemoteTransport, WorkClient};
use work_core::api::realtime::{
    ClientFrame, Discovery, Hello, OperationOutcome, OperationRef, OperationStatus,
    RealtimeOperationKind, SUBPROTOCOL, ServerFrame, VERSION,
};

#[derive(Clone, Copy)]
enum Mode {
    Legacy,
    AuthDenied,
    UpgradeDenied,
    WrongNode,
    WrongOwner,
    WrongReceipt,
    NegativeAck,
    InvalidResult,
    CloseExecute,
    Echo,
}
struct Peer {
    mode: Mode,
    http_writes: AtomicUsize,
    executes: AtomicUsize,
    reserved: Mutex<Vec<RealtimeOperationKind>>,
    payloads: Mutex<Vec<Value>>,
}
struct Fixture {
    peer: Arc<Peer>,
    client: WorkClient,
    stop: oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}
impl Fixture {
    async fn new(mode: Mode, transport: RemoteTransport) -> Self {
        let peer = Arc::new(Peer {
            mode,
            http_writes: AtomicUsize::new(0),
            executes: AtomicUsize::new(0),
            reserved: Mutex::new(vec![]),
            payloads: Mutex::new(vec![]),
        });
        let app = Router::new()
            .route("/api/v1/auth", get(auth).post(login))
            .route(
                "/api/v1/health",
                get(|| async { Json(json!({"ok":true,"nodeId":"expected-node"})) }),
            )
            .route("/api/v1/transports", get(discovery))
            .route("/api/v1/realtime", get(upgrade))
            .route("/api/v1/sessions/s/commands", post(http_command))
            .with_state(peer.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .unwrap();
        });
        let client = WorkClient::new(&format!("http://{address}")).unwrap();
        if !matches!(mode, Mode::AuthDenied) {
            client.login("isolated-protocol-peer").await.unwrap();
        }
        client.set_remote_transport(transport);
        Self {
            peer,
            client,
            stop,
            task,
        }
    }
    async fn finish(self) {
        drop(self.client);
        let _ = self.stop.send(());
        tokio::time::timeout(Duration::from_secs(2), self.task)
            .await
            .unwrap()
            .unwrap();
    }
}
async fn login() -> impl IntoResponse {
    (
        [(
            "set-cookie",
            "rpi-work-peer=isolated; HttpOnly; Path=/; SameSite=Strict",
        )],
        Json(json!({"authenticated":true})),
    )
}
async fn auth(State(peer): State<Arc<Peer>>) -> Response {
    if matches!(peer.mode, Mode::AuthDenied) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({"required":true,"authenticated":true})).into_response()
}
async fn discovery(State(peer): State<Arc<Peer>>) -> Response {
    if matches!(peer.mode, Mode::Legacy) {
        return StatusCode::NOT_FOUND.into_response();
    }
    Json(Discovery::new(
        "expected-node".into(),
        "expected-owner".into(),
    ))
    .into_response()
}
async fn http_command(State(peer): State<Arc<Peer>>) -> Json<Value> {
    peer.http_writes.fetch_add(1, Ordering::Relaxed);
    Json(host_response(true))
}
fn host_response(success: bool) -> Value {
    json!({"type":"response","id":"original-native-id","command":"prompt","success":success,"data":{"accepted":success}})
}
async fn upgrade(State(peer): State<Arc<Peer>>, ws: WebSocketUpgrade) -> Response {
    if matches!(peer.mode, Mode::UpgradeDenied) {
        return StatusCode::NOT_FOUND.into_response();
    }
    ws.protocols([SUBPROTOCOL])
        .on_upgrade(move |socket| serve(socket, peer))
}
async fn frame(socket: &mut WebSocket, frame: ServerFrame) -> bool {
    socket
        .send(Message::Text(frame.encode_text().unwrap().into()))
        .await
        .is_ok()
}
async fn serve(mut socket: WebSocket, peer: Arc<Peer>) {
    let mut hello = Hello {
        version: VERSION,
        node_id: "expected-node".into(),
        owner_epoch: "expected-owner".into(),
        connection_id: "peer-connection".into(),
        capabilities: Default::default(),
    };
    if matches!(peer.mode, Mode::WrongNode) {
        hello.node_id = "changed-node".into();
    }
    if matches!(peer.mode, Mode::WrongOwner) {
        hello.owner_epoch = "changed-owner".into();
    }
    if !frame(
        &mut socket,
        ServerFrame::Hello {
            version: VERSION,
            hello,
        },
    )
    .await
    {
        return;
    }
    let mut number = 0;
    while let Ok(Some(Ok(message))) =
        tokio::time::timeout(Duration::from_secs(5), socket.next()).await
    {
        let Message::Text(text) = message else {
            break;
        };
        match ClientFrame::decode_text(&text).unwrap() {
            ClientFrame::Reserve {
                request_id, kind, ..
            } => {
                number += 1;
                peer.reserved.lock().unwrap().push(kind);
                let operation = OperationRef {
                    owner_epoch: if matches!(peer.mode, Mode::WrongReceipt) {
                        "changed-owner"
                    } else {
                        "expected-owner"
                    }
                    .into(),
                    operation_id: format!("operation-{number}"),
                };
                if !frame(
                    &mut socket,
                    ServerFrame::Reserved {
                        version: VERSION,
                        request_id,
                        operation,
                    },
                )
                .await
                {
                    break;
                }
            }
            ClientFrame::Execute {
                request_id,
                operation,
                payload,
                ..
            } => {
                peer.executes.fetch_add(1, Ordering::Relaxed);
                peer.payloads.lock().unwrap().push(payload.clone());
                if matches!(peer.mode, Mode::CloseExecute) {
                    break;
                }
                let value = match peer.mode {
                    Mode::NegativeAck => host_response(false),
                    Mode::InvalidResult => json!({"unrelated":"not-an-ack"}),
                    Mode::Echo => payload,
                    _ => host_response(true),
                };
                let status = OperationStatus::KnownResult {
                    outcome: OperationOutcome::Succeeded { value },
                };
                if !frame(
                    &mut socket,
                    ServerFrame::Status {
                        version: VERSION,
                        request_id,
                        operation,
                        status,
                    },
                )
                .await
                {
                    break;
                }
            }
            _ => break,
        }
    }
}

#[tokio::test]
async fn auto_legacy_fallback_requires_authenticated_absent_discovery() {
    let f = Fixture::new(Mode::Legacy, RemoteTransport::Auto).await;
    assert!(f.client.prompt("s", "isolated").await.unwrap().success);
    assert_eq!(f.peer.http_writes.load(Ordering::Relaxed), 1);
    assert_eq!(f.peer.executes.load(Ordering::Relaxed), 0);
    f.finish().await;

    let f = Fixture::new(Mode::Legacy, RemoteTransport::WebSocketOnly).await;
    assert!(matches!(
        f.client.prompt("s", "isolated").await,
        Err(ClientError::RealtimeNotAdvertised)
    ));
    assert_eq!(f.peer.http_writes.load(Ordering::Relaxed), 0);
    f.finish().await;
}

#[tokio::test]
async fn auth_upgrade_identity_and_receipt_failures_never_downgrade() {
    for mode in [
        Mode::AuthDenied,
        Mode::UpgradeDenied,
        Mode::WrongNode,
        Mode::WrongOwner,
        Mode::WrongReceipt,
    ] {
        let f = Fixture::new(mode, RemoteTransport::Auto).await;
        assert!(f.client.prompt("s", "isolated").await.is_err());
        assert_eq!(f.peer.http_writes.load(Ordering::Relaxed), 0);
        assert_eq!(f.peer.executes.load(Ordering::Relaxed), 0);
        f.finish().await;
    }
}

#[tokio::test]
async fn negative_host_ack_is_not_changed_into_success_by_the_receipt_wrapper() {
    let f = Fixture::new(Mode::NegativeAck, RemoteTransport::WebSocketOnly).await;
    let response = f.client.prompt("s", "isolated").await.unwrap();
    assert!(!response.success);
    assert_eq!(response.data.unwrap()["accepted"], false);
    assert_eq!(response.id.unwrap(), "original-native-id");
    assert_eq!(f.peer.executes.load(Ordering::Relaxed), 1);
    f.finish().await;
}

#[tokio::test]
async fn disconnect_or_malformed_result_preserves_receipt_without_auto_retry() {
    for mode in [Mode::CloseExecute, Mode::InvalidResult] {
        let f = Fixture::new(mode, RemoteTransport::Auto).await;
        let error = f.client.prompt("s", "isolated").await.unwrap_err();
        let ClientError::OperationOutcomeUnknown { operation } = error else {
            panic!("receipt must survive: {error:?}")
        };
        assert_eq!(operation.owner_epoch, "expected-owner");
        assert_eq!(operation.operation_id, "operation-1");
        assert_eq!(f.peer.executes.load(Ordering::Relaxed), 1);
        assert_eq!(f.peer.reserved.lock().unwrap().len(), 1);
        assert_eq!(f.peer.http_writes.load(Ordering::Relaxed), 0);
        f.finish().await;
    }
}

#[tokio::test]
async fn configuration_and_dialog_keep_exact_kind_and_payload_on_the_same_hub() {
    let f = Fixture::new(Mode::Echo, RemoteTransport::WebSocketOnly).await;
    assert_eq!(
        f.client
            .set_session_config_option("s", "mode", json!("plan"))
            .await
            .unwrap(),
        json!({"configId":"mode","value":"plan"})
    );
    assert_eq!(
        f.client
            .dialog_response("s", "dialog-1", json!({"choice":"yes"}))
            .await
            .unwrap(),
        json!({"requestId":"dialog-1","answer":{"choice":"yes"}})
    );
    assert_eq!(
        f.client.cancel_dialog("s", "dialog-2").await.unwrap(),
        json!({"requestId":"dialog-2","cancelled":true})
    );
    assert_eq!(
        *f.peer.reserved.lock().unwrap(),
        [
            RealtimeOperationKind::SetConfiguration,
            RealtimeOperationKind::DialogResponse,
            RealtimeOperationKind::CancelDialog
        ]
    );
    assert_eq!(f.peer.executes.load(Ordering::Relaxed), 3);
    assert_eq!(f.peer.http_writes.load(Ordering::Relaxed), 0);
    f.finish().await;
}

#[tokio::test]
async fn oversized_execute_is_rejected_before_any_reservation() {
    let f = Fixture::new(Mode::Echo, RemoteTransport::WebSocketOnly).await;
    let error = f
        .client
        .prompt("s", &"x".repeat(2 * 1024 * 1024))
        .await
        .unwrap_err();
    assert!(matches!(error, ClientError::Api { status, .. } if status.as_u16() == 429));
    assert_eq!(f.peer.reserved.lock().unwrap().len(), 0);
    assert_eq!(f.peer.executes.load(Ordering::Relaxed), 0);
    f.finish().await;
}
