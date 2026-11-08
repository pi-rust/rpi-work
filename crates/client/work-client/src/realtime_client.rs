//! Native, per-auth-context shared WS actor. It never retries an execute.
//! Socket loss closes streams; the existing snapshot/cursor recovery owns replay.
use crate::{ClientError, EventStream, WorkClient};
use futures_core::Stream;
use futures_util::{SinkExt, StreamExt};
use reqwest::cookie::CookieStore;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    collections::HashMap,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    sync::{Mutex, OwnedSemaphorePermit, Semaphore, mpsc, oneshot, watch},
    time::Instant,
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::{
        Message, client::IntoClientRequest, http::HeaderValue, protocol::WebSocketConfig,
    },
};
use work_core::api::{
    WireEvent,
    realtime::{
        self, ClientFrame, Discovery, Hello, OperationOutcome, OperationRef, OperationStatus,
        RealtimeErrorCode, RealtimeOperationKind, SUBPROTOCOL, ServerFrame, VERSION,
    },
};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
const MAX_PENDING: usize = 16;
const MAX_SUBSCRIPTIONS: usize = 16;
const QUEUE_ITEMS: usize = 32;
const STREAM_ITEMS: usize = 64;
const STREAM_BYTES: usize = 4 * 1024 * 1024;
const HUB_BYTES: usize = 8 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const HEARTBEAT: Duration = Duration::from_secs(15);
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);

pub(crate) struct Hub {
    slot: Mutex<Option<Handle>>,
    fence: watch::Sender<u64>,
    auth_changes: AtomicUsize,
}
impl Default for Hub {
    fn default() -> Self {
        let (fence, _) = watch::channel(0);
        Self {
            slot: Mutex::new(None),
            fence,
            auth_changes: AtomicUsize::new(0),
        }
    }
}
impl Hub {
    pub(crate) fn invalidate(&self, epoch: u64) {
        self.fence.send_replace(epoch);
    }

    async fn get(&self, client: &WorkClient) -> Result<Handle, ClientError> {
        let epoch = client.realtime_epoch();
        let mut fence = self.fence.subscribe();
        if self.auth_changes.load(Ordering::Acquire) != 0 || *fence.borrow_and_update() != epoch {
            return Err(denied());
        }
        // This lock serializes connection creation only, never requests/owner work.
        let mut slot = tokio::select! {
            biased;
            _ = fence.changed() => return Err(denied()),
            slot = self.slot.lock() => slot,
        };
        if let Some(handle) = slot
            .as_ref()
            .filter(|h| h.epoch == epoch && !h.tx.is_closed())
        {
            return Ok(handle.clone());
        }
        let (socket, hello) = tokio::select! {
            biased;
            _ = fence.changed() => return Err(denied()),
            result = tokio::time::timeout(CONNECT_TIMEOUT, connect(client)) => result.map_err(|_| unavailable())??,
        };
        if client.realtime_epoch() != epoch || self.auth_changes.load(Ordering::Acquire) != 0 {
            return Err(denied());
        }
        let (tx, rx) = mpsc::channel(QUEUE_ITEMS);
        let handle = Handle {
            tx,
            epoch,
            current_epoch: client.realtime_epoch.clone(),
            outgoing: Arc::new(Semaphore::new(HUB_BYTES)),
            incoming: Arc::new(Semaphore::new(HUB_BYTES)),
            hello,
            ids: client.realtime_ids.clone(),
        };
        // The actor does not own Hub/WorkClient or a sender to its own inbox.
        // Dropping the last client closes the watch channel and releases it.
        tokio::spawn(run(
            socket,
            rx,
            fence,
            handle.incoming.clone(),
            handle.ids.clone(),
            handle.hello.owner_epoch.clone(),
        ));
        *slot = Some(handle.clone());
        Ok(handle)
    }
}

pub(crate) struct AuthChange<'a>(&'a WorkClient);
impl WorkClient {
    pub(crate) fn realtime_auth_change(&self) -> AuthChange<'_> {
        self.realtime.auth_changes.fetch_add(1, Ordering::AcqRel);
        self.fence_realtime();
        AuthChange(self)
    }
    fn next_realtime_id(&self, prefix: &str) -> String {
        next_id(&self.realtime_ids, prefix)
    }
}
impl Drop for AuthChange<'_> {
    fn drop(&mut self) {
        // Fence both before and after every auth attempt, including errors and
        // cancelled futures. A connection cannot be opened with an old cookie
        // while login/logout/QR redemption is in flight.
        self.0.fence_realtime();
        self.0.realtime.auth_changes.fetch_sub(1, Ordering::AcqRel);
    }
}
fn next_id(ids: &AtomicU64, prefix: &str) -> String {
    format!("{prefix}-{}", ids.fetch_add(1, Ordering::Relaxed))
}
fn unavailable() -> ClientError {
    ClientError::WebSocket { status: None }
}
fn denied() -> ClientError {
    ClientError::WebSocket { status: Some(401) }
}
fn limited() -> ClientError {
    realtime_error(RealtimeErrorCode::LimitExceeded)
}

#[derive(Clone)]
struct Handle {
    tx: mpsc::Sender<Packet>,
    epoch: u64,
    current_epoch: Arc<AtomicU64>,
    outgoing: Arc<Semaphore>,
    incoming: Arc<Semaphore>,
    hello: Hello,
    ids: Arc<AtomicU64>,
}
struct Packet {
    text: String,
    request_id: String,
    expected: Expected,
    reply: oneshot::Sender<Result<Reply, ClientError>>,
    install: Option<Subscription>,
    deadline: Instant,
    bytes: OwnedSemaphorePermit,
}
struct Pending {
    expected: Expected,
    reply: oneshot::Sender<Result<Reply, ClientError>>,
    deadline: Instant,
    _bytes: OwnedSemaphorePermit,
}
enum Expected {
    Reserved,
    Status(OperationRef),
    Result,
    Subscribed {
        session: String,
        subscription: String,
    },
}
impl Expected {
    fn accepts(&self, frame: &ServerFrame, owner: &str) -> bool {
        match (self, frame) {
            (Self::Reserved, ServerFrame::Reserved { operation, .. }) => {
                operation.owner_epoch == owner
            }
            (Self::Status(expected), ServerFrame::Status { operation, .. }) => {
                expected == operation
            }
            (Self::Result, ServerFrame::Result { .. }) | (_, ServerFrame::Error { .. }) => true,
            (
                Self::Subscribed {
                    session,
                    subscription,
                },
                ServerFrame::Subscribed {
                    session_id,
                    subscription_id,
                    ..
                },
            ) => session == session_id && subscription == subscription_id,
            _ => false,
        }
    }
}
struct Reply {
    frame: ServerFrame,
    _bytes: Arc<OwnedSemaphorePermit>,
}
impl Handle {
    async fn request(
        &self,
        frame: ClientFrame,
        install: Option<Subscription>,
    ) -> Result<Reply, ClientError> {
        if self.current_epoch.load(Ordering::Acquire) != self.epoch {
            return Err(denied());
        }
        let (request_id, expected) = match &frame {
            ClientFrame::Reserve { request_id, .. } => (request_id.clone(), Expected::Reserved),
            ClientFrame::Execute {
                request_id,
                operation,
                ..
            }
            | ClientFrame::Status {
                request_id,
                operation,
                ..
            } => (request_id.clone(), Expected::Status(operation.clone())),
            ClientFrame::Query { request_id, .. } => (request_id.clone(), Expected::Result),
            ClientFrame::Subscribe {
                request_id,
                session_id,
                subscription_id,
                ..
            } => (
                request_id.clone(),
                Expected::Subscribed {
                    session: session_id.clone(),
                    subscription: subscription_id.clone(),
                },
            ),
            _ => return Err(unavailable()),
        };
        let text = frame
            .encode_text()
            .map_err(|error| realtime_error(error.code))?;
        let bytes = self
            .outgoing
            .clone()
            .try_acquire_many_owned(u32::try_from(text.len()).map_err(|_| limited())?)
            .map_err(|_| limited())?;
        let (tx, rx) = oneshot::channel();
        self.tx
            .try_send(Packet {
                text,
                request_id,
                expected,
                reply: tx,
                install,
                deadline: Instant::now() + REQUEST_TIMEOUT,
                bytes,
            })
            .map_err(|_| limited())?;
        let reply = tokio::time::timeout(REQUEST_TIMEOUT, rx)
            .await
            .map_err(|_| unavailable())?
            .map_err(|_| unavailable())??;
        if self.current_epoch.load(Ordering::Acquire) != self.epoch {
            return Err(denied());
        }
        Ok(reply)
    }
}

pub(crate) async fn command<T: DeserializeOwned>(
    client: &WorkClient,
    session: &str,
    payload: Value,
) -> Result<crate::HostResponse<T>, ClientError> {
    if realtime::is_read_only_session_command(&payload) {
        let handle = client.realtime.get(client).await?;
        if !handle.hello.capabilities.command_queries {
            return Err(realtime_error(RealtimeErrorCode::Unsupported));
        }
        let reply = handle
            .request(
                ClientFrame::Query {
                    version: VERSION,
                    request_id: client.next_realtime_id("query"),
                    session_id: session.into(),
                    payload,
                },
                None,
            )
            .await?;
        return match reply.frame {
            ServerFrame::Result { value, .. } => {
                serde_json::from_value(value).map_err(ClientError::Json)
            }
            ServerFrame::Error { error, .. } => Err(realtime_error(error.code)),
            _ => Err(unavailable()),
        };
    }
    let (value, operation) = mutation_result(
        client,
        session,
        RealtimeOperationKind::SessionCommand,
        payload,
    )
    .await?;
    // A malformed result is not a negative acceptance ACK. Keep the receipt.
    serde_json::from_value(value).map_err(|_| ClientError::OperationOutcomeUnknown { operation })
}

pub(crate) async fn mutation(
    client: &WorkClient,
    session: &str,
    kind: RealtimeOperationKind,
    payload: Value,
) -> Result<Value, ClientError> {
    mutation_result(client, session, kind, payload)
        .await
        .map(|(value, _)| value)
}

async fn mutation_result(
    client: &WorkClient,
    session: &str,
    kind: RealtimeOperationKind,
    payload: Value,
) -> Result<(Value, OperationRef), ClientError> {
    // Check the full envelope before reserving. Use worst-case reference/ID
    // lengths, so reserve never creates a receipt for an oversized command.
    ClientFrame::Execute {
        version: VERSION,
        request_id: "x".repeat(realtime::MAX_ID_BYTES),
        operation: OperationRef {
            owner_epoch: "x".repeat(realtime::MAX_ID_BYTES),
            operation_id: "x".repeat(realtime::MAX_ID_BYTES),
        },
        payload: payload.clone(),
    }
    .validate()
    .map_err(|error| realtime_error(error.code))?;
    let handle = client.realtime.get(client).await?;
    let reply = handle
        .request(
            ClientFrame::Reserve {
                version: VERSION,
                request_id: client.next_realtime_id("reserve"),
                session_id: session.into(),
                client_instance_id: (*client.realtime_client_id).clone(),
                kind,
            },
            None,
        )
        .await?;
    let operation = match reply.frame {
        ServerFrame::Reserved { operation, .. } => operation,
        ServerFrame::Error { error, .. } => return Err(realtime_error(error.code)),
        _ => return Err(unavailable()),
    };
    // From this point every transport/timeout/auth/protocol failure retains the
    // receipt. No new reserve, resend, reconnect execute or HTTP retry occurs.
    let reply = handle
        .request(
            ClientFrame::Execute {
                version: VERSION,
                request_id: client.next_realtime_id("execute"),
                operation: operation.clone(),
                payload,
            },
            None,
        )
        .await
        .map_err(|_| ClientError::OperationOutcomeUnknown {
            operation: operation.clone(),
        })?;
    match reply.frame {
        ServerFrame::Status { status, .. } => {
            operation_status_value(status, operation.clone()).map(|value| (value, operation))
        }
        ServerFrame::Error { error, .. }
            if error.code != RealtimeErrorCode::OutcomeUnknown
                && error.code != RealtimeErrorCode::Internal =>
        {
            Err(realtime_error(error.code))
        }
        _ => Err(ClientError::OperationOutcomeUnknown { operation }),
    }
}
fn operation_status_value(
    status: OperationStatus,
    operation: OperationRef,
) -> Result<Value, ClientError> {
    match status {
        OperationStatus::KnownResult {
            outcome: OperationOutcome::Succeeded { value },
        } => Ok(value),
        OperationStatus::KnownResult {
            outcome: OperationOutcome::Failed { error },
        } if error.code != RealtimeErrorCode::OutcomeUnknown => Err(realtime_error(error.code)),
        _ => Err(ClientError::OperationOutcomeUnknown { operation }),
    }
}
fn realtime_error(code: RealtimeErrorCode) -> ClientError {
    let status = match code {
        RealtimeErrorCode::InvalidInput => reqwest::StatusCode::BAD_REQUEST,
        RealtimeErrorCode::AccessDenied => reqwest::StatusCode::FORBIDDEN,
        RealtimeErrorCode::NotFound => reqwest::StatusCode::NOT_FOUND,
        RealtimeErrorCode::Conflict => reqwest::StatusCode::CONFLICT,
        RealtimeErrorCode::LimitExceeded => reqwest::StatusCode::TOO_MANY_REQUESTS,
        RealtimeErrorCode::Unsupported => reqwest::StatusCode::NOT_IMPLEMENTED,
        RealtimeErrorCode::UnsupportedContent => reqwest::StatusCode::UNSUPPORTED_MEDIA_TYPE,
        RealtimeErrorCode::Unavailable => reqwest::StatusCode::SERVICE_UNAVAILABLE,
        _ => reqwest::StatusCode::INTERNAL_SERVER_ERROR,
    };
    ClientError::Api {
        status,
        message: "remote realtime operation failed".into(),
    }
}

struct Subscription {
    session: String,
    tx: mpsc::Sender<QueuedEvent>,
    bytes: Arc<Semaphore>,
    failed: Arc<AtomicBool>,
}
struct QueuedEvent {
    event: WireEvent,
    _local_bytes: OwnedSemaphorePermit,
    _hub_bytes: Arc<OwnedSemaphorePermit>,
}
pub(crate) async fn subscribe(
    client: &WorkClient,
    session: &str,
    after: u64,
) -> Result<EventStream, ClientError> {
    let handle = client.realtime.get(client).await?;
    let subscription = client.next_realtime_id("subscription");
    let (tx, rx) = mpsc::channel(STREAM_ITEMS);
    let failed = Arc::new(AtomicBool::new(false));
    let source = Subscription {
        session: session.into(),
        tx,
        bytes: Arc::new(Semaphore::new(STREAM_BYTES)),
        failed: failed.clone(),
    };
    let reply = handle
        .request(
            ClientFrame::Subscribe {
                version: VERSION,
                request_id: client.next_realtime_id("subscribe"),
                session_id: session.into(),
                subscription_id: subscription,
                after_seq: Some(after),
            },
            Some(source),
        )
        .await?;
    if let ServerFrame::Error { error, .. } = reply.frame {
        return Err(realtime_error(error.code));
    }
    Ok(Box::pin(WsEventStream {
        rx,
        failed,
        epoch: handle.epoch,
        current_epoch: handle.current_epoch,
        ended: false,
    }))
}
struct WsEventStream {
    rx: mpsc::Receiver<QueuedEvent>,
    failed: Arc<AtomicBool>,
    epoch: u64,
    current_epoch: Arc<AtomicU64>,
    ended: bool,
}
impl Stream for WsEventStream {
    type Item = Result<WireEvent, ClientError>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.ended {
            return Poll::Ready(None);
        }
        if self.current_epoch.load(Ordering::Acquire) != self.epoch
            || self.failed.load(Ordering::Acquire)
        {
            self.ended = true;
            self.rx.close();
            while self.rx.try_recv().is_ok() {}
            return Poll::Ready(Some(Err(unavailable())));
        }
        self.rx
            .poll_recv(cx)
            .map(|item| item.map(|item| Ok(item.event)))
    }
}
// Dropping rx is observed on the actor's bounded cleanup tick. No drop-time
// unbounded command queue, background task, or blocking destructor is needed.

async fn bounded_get<T: DeserializeOwned>(
    client: &WorkClient,
    path: &str,
) -> Result<T, ClientError> {
    let response = crate::authenticated(
        client.http.get(client.url(path)?).header("x-rpi-web", "1"),
        client.bearer_token.as_deref(),
    )
    .send()
    .await
    .map_err(|_| unavailable())?;
    if !response.status().is_success() {
        return Err(ClientError::Api {
            status: response.status(),
            message: "realtime discovery failed".into(),
        });
    }
    let mut body = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|_| unavailable())?;
        if bytes.len() + chunk.len() > realtime::MAX_MESSAGE_BYTES {
            return Err(limited());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| unavailable())
}
#[allow(clippy::too_many_lines)] // One bounded handshake with a single failure fence.
async fn connect(client: &WorkClient) -> Result<(Socket, Hello), ClientError> {
    if client.bearer_token.is_some() {
        return Err(denied());
    }
    let http_url = client.url("api/v1/realtime")?;
    if !http_url.username().is_empty()
        || http_url.password().is_some()
        || http_url.query().is_some()
        || http_url.fragment().is_some()
    {
        return Err(unavailable());
    }
    // Authentication is confirmed before discovery. Only an authenticated,
    // explicitly absent discovery endpoint can select legacy HTTP/SSE.
    let auth: crate::AuthState = bounded_get(client, "api/v1/auth").await?;
    if !auth.authenticated {
        return Err(denied());
    }
    let identity: Value = bounded_get(client, "api/v1/health").await?;
    let node_id = identity
        .get("nodeId")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(unavailable)?;
    let discovery: Discovery = match bounded_get(client, "api/v1/transports").await {
        Err(ClientError::Api { status, .. })
            if status.as_u16() == 404 || status.as_u16() == 405 =>
        {
            let current: Value = bounded_get(client, "api/v1/health").await?;
            if current.get("nodeId").and_then(Value::as_str) != Some(node_id) {
                return Err(unavailable());
            }
            return Err(ClientError::RealtimeNotAdvertised);
        }
        result => result?,
    };
    discovery.validate().map_err(|_| unavailable())?;
    if discovery.node_id != node_id {
        return Err(unavailable());
    }
    let mut ws_url = http_url.clone();
    ws_url
        .set_scheme(match http_url.scheme() {
            "http" => "ws",
            "https" => "wss",
            _ => return Err(unavailable()),
        })
        .map_err(|()| unavailable())?;
    let mut request = ws_url
        .as_str()
        .into_client_request()
        .map_err(|_| unavailable())?;
    request.headers_mut().insert(
        "sec-websocket-protocol",
        HeaderValue::from_static(SUBPROTOCOL),
    );
    if let Some(capability) = &client.control_capability {
        request
            .headers_mut()
            .insert("x-rpi-node-control", capability.clone());
    } else {
        let cookie = client.cookie_jar.cookies(&http_url).ok_or_else(denied)?;
        let cookie =
            HeaderValue::from_str(cookie.to_str().map_err(|_| denied())?).map_err(|_| denied())?;
        request.headers_mut().insert("cookie", cookie);
    }
    let config = WebSocketConfig::default()
        .max_message_size(Some(realtime::MAX_MESSAGE_BYTES))
        .max_frame_size(Some(realtime::MAX_MESSAGE_BYTES))
        .write_buffer_size(0)
        .max_write_buffer_size(realtime::MAX_MESSAGE_BYTES + 64 * 1024);
    let (mut socket, response) = connect_async_with_config(request, Some(config), false)
        .await
        .map_err(|error| {
            let status = match error {
                tokio_tungstenite::tungstenite::Error::Http(response) => {
                    Some(response.status().as_u16())
                }
                _ => None,
            };
            ClientError::WebSocket { status }
        })?;
    if response.status().as_u16() != 101
        || response
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|v| v.to_str().ok())
            != Some(SUBPROTOCOL)
    {
        return Err(unavailable());
    }
    loop {
        match socket
            .next()
            .await
            .ok_or_else(unavailable)?
            .map_err(|_| unavailable())?
        {
            Message::Text(text) => {
                let ServerFrame::Hello { hello, .. } =
                    ServerFrame::decode_text(&text).map_err(|_| unavailable())?
                else {
                    return Err(unavailable());
                };
                if hello.node_id != discovery.node_id
                    || hello.owner_epoch != discovery.owner_epoch
                    || hello.capabilities != discovery.capabilities
                {
                    return Err(unavailable());
                }
                return Ok((socket, hello));
            }
            Message::Ping(bytes) => socket
                .send(Message::Pong(bytes))
                .await
                .map_err(|_| unavailable())?,
            _ => return Err(unavailable()),
        }
    }
}

async fn write(
    socket: &mut Socket,
    message: Message,
    fence: &mut watch::Receiver<u64>,
) -> Result<(), ()> {
    tokio::select! {
        biased;
        _ = fence.changed() => Err(()),
        result = tokio::time::timeout(WRITE_TIMEOUT, socket.send(message)) => result.map_err(|_| ())?.map_err(|_| ()),
    }
}
#[allow(clippy::too_many_lines)] // Keep the actor's cancellation and admission loop together.
async fn run(
    mut socket: Socket,
    mut rx: mpsc::Receiver<Packet>,
    mut fence: watch::Receiver<u64>,
    incoming: Arc<Semaphore>,
    ids: Arc<AtomicU64>,
    owner: String,
) {
    let mut pending: HashMap<String, Pending> = HashMap::new();
    let mut subscriptions: HashMap<String, Subscription> = HashMap::new();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_read = Instant::now();
    let mut last_ping = Instant::now();
    let mut sent = 0;
    'actor: loop {
        // Borrow the generation once per turn, before touching queued writes.
        if fence.has_changed().unwrap_or(true) {
            break;
        }
        tokio::select! {
            biased;
            _ = fence.changed() => break,
            message = socket.next() => {
                let Some(Ok(message)) = message else { break; };
                last_read = Instant::now();
                match message {
                    Message::Text(text) => {
                        let Ok(frame) = ServerFrame::decode_text(&text) else { break; };
                        let Ok(size) = u32::try_from(text.len()) else { break; };
                        let Ok(bytes) = incoming.clone().try_acquire_many_owned(size) else { break; };
                        let bytes = Arc::new(bytes);
                        if let ServerFrame::Events { session_id, subscription_id, events, .. } = frame {
                            let Some(source) = subscriptions.get(&subscription_id) else { continue; };
                            if source.session != session_id { break; }
                            let mut overflow = false;
                            for event in events {
                                let Ok(size) = serde_json::to_vec(&event).map(|v| v.len()) else { overflow = true; break; };
                                let Ok(size) = u32::try_from(size) else { overflow = true; break; };
                                let Ok(local) = source.bytes.clone().try_acquire_many_owned(size) else { overflow = true; break; };
                                if source.tx.try_send(QueuedEvent { event, _local_bytes: local, _hub_bytes: bytes.clone() }).is_err() { overflow = true; break; }
                            }
                            if overflow {
                                source.failed.store(true, Ordering::Release);
                                subscriptions.remove(&subscription_id);
                                let frame = ClientFrame::Unsubscribe { version: VERSION, request_id: next_id(&ids, "close"), subscription_id };
                                let Ok(text) = frame.encode_text() else { break; };
                                sent += 1;
                                if write(&mut socket, Message::Text(text.into()), &mut fence).await.is_err() { break; }
                            }
                            continue;
                        }
                        let request_id = match &frame {
                            ServerFrame::Reserved { request_id, .. } | ServerFrame::Status { request_id, .. } | ServerFrame::Result { request_id, .. } | ServerFrame::Subscribed { request_id, .. } => request_id,
                            ServerFrame::Error { request_id: Some(id), .. } => id,
                            ServerFrame::Pong { .. } | ServerFrame::Unsubscribed { .. } => continue,
                            _ => break,
                        };
                        let Some(item) = pending.remove(request_id) else { break; };
                        // All operation results are tied to the connected owner.
                        if !item.expected.accepts(&frame, &owner) { break; }
                        if let (Expected::Subscribed { subscription, .. }, ServerFrame::Error { .. }) = (&item.expected, &frame) {
                            if let Some(source) = subscriptions.remove(subscription) { source.failed.store(true, Ordering::Release); }
                        }
                        let _ = item.reply.send(Ok(Reply { frame, _bytes: bytes }));
                    }
                    Message::Ping(bytes) => { if write(&mut socket, Message::Pong(bytes), &mut fence).await.is_err() { break; } }
                    Message::Pong(_) => {}
                    Message::Close(_) | Message::Binary(_) | Message::Frame(_) => break,
                }
            }
            packet = rx.recv() => {
                let Some(packet) = packet else { break; };
                if packet.reply.is_closed() { continue; }
                if pending.len() >= MAX_PENDING || packet.deadline <= Instant::now() {
                    let _ = packet.reply.send(Err(limited()));
                    continue;
                }
                if let Some(source) = packet.install {
                    if subscriptions.len() >= MAX_SUBSCRIPTIONS {
                        let _ = packet.reply.send(Err(limited()));
                        continue;
                    }
                    let Expected::Subscribed { subscription, .. } = &packet.expected else { break; };
                    subscriptions.insert(subscription.clone(), source);
                }
                pending.insert(packet.request_id, Pending { expected: packet.expected, reply: packet.reply, deadline: packet.deadline, _bytes: packet.bytes });
                sent += 1;
                if write(&mut socket, Message::Text(packet.text.into()), &mut fence).await.is_err() { break; }
            }
            _ = tick.tick() => {
                let now = Instant::now();
                if now.duration_since(last_read) >= IDLE_TIMEOUT || sent >= 3900 || pending.values().any(|p| p.deadline <= now) { break; }
                let closed: Vec<_> = subscriptions.iter().filter(|(_, sub)| sub.tx.is_closed()).map(|(id, _)| id.clone()).collect();
                for id in closed {
                    subscriptions.remove(&id);
                    let frame = ClientFrame::Unsubscribe { version: VERSION, request_id: next_id(&ids, "close"), subscription_id: id };
                    let Ok(text) = frame.encode_text() else { break 'actor; };
                    sent += 1;
                    if write(&mut socket, Message::Text(text.into()), &mut fence).await.is_err() { break 'actor; }
                }
                if now.duration_since(last_ping) >= HEARTBEAT {
                    last_ping = now;
                    let frame = ClientFrame::Ping { version: VERSION, request_id: next_id(&ids, "ping") };
                    let Ok(text) = frame.encode_text() else { break; };
                    sent += 1;
                    if write(&mut socket, Message::Text(text.into()), &mut fence).await.is_err() { break; }
                }
            }
        }
    }
    // Fail before dropping senders: queued deltas must not leak after overflow,
    // disconnect or auth revocation. No synthetic settled event is produced.
    for source in subscriptions.values() {
        source.failed.store(true, Ordering::Release);
    }
    subscriptions.clear();
    for (_, item) in pending {
        let _ = item.reply.send(Err(unavailable()));
    }
    rx.close();
    while let Ok(packet) = rx.try_recv() {
        let _ = packet.reply.send(Err(unavailable()));
    }
    let _ = tokio::time::timeout(Duration::from_millis(100), socket.close(None)).await;
}

pub(crate) fn legacy(error: &ClientError) -> bool {
    matches!(error, ClientError::RealtimeNotAdvertised)
}
