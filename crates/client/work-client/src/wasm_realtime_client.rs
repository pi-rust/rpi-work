//! Browser WebSocket hub. Browser WebSocket automatically carries same-origin
//! cookies; this module never moves bearer credentials into a URL or a frame.
use crate::{ClientError, EventStream, WorkClient};
use futures_channel::{mpsc, oneshot};
use futures_core::Stream;
use futures_util::{FutureExt, StreamExt, pin_mut, select_biased};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    collections::HashMap,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};
use wasm_bindgen::{JsCast, closure::Closure};
use wasm_bindgen_futures::spawn_local;
use web_sys::{BinaryType, Event, MessageEvent, WebSocket};
use work_core::api::{
    WireEvent,
    realtime::{
        self, ClientFrame, Discovery, Hello, OperationOutcome, OperationRef, OperationStatus,
        RealtimeErrorCode, RealtimeOperationKind, SUBPROTOCOL, ServerFrame, VERSION,
    },
};

const REQUEST_QUEUE: usize = 32;
const MAX_PENDING: usize = 16;
const MAX_SUBSCRIPTIONS: usize = 16;
const EVENT_QUEUE: usize = 64;
const BYTE_BUDGET: usize = 8 * 1024 * 1024;
const HEARTBEAT_MS: f64 = 15_000.0;
const IDLE_TIMEOUT_MS: f64 = 45_000.0;
const CONNECT_TIMEOUT_MS: u32 = 10_000;
const REQUEST_TIMEOUT_MS: u32 = 30_000;

type FrameResult = Result<Incoming, ClientError>;
struct Incoming {
    frame: ServerFrame,
    _bytes: ByteCharge,
}
struct ByteCharge {
    used: Arc<AtomicUsize>,
    size: usize,
}
impl Drop for ByteCharge {
    fn drop(&mut self) {
        self.used.fetch_sub(self.size, Ordering::AcqRel);
    }
}
fn charge(used: &Arc<AtomicUsize>, size: usize) -> Option<ByteCharge> {
    let mut current = used.load(Ordering::Acquire);
    loop {
        let next = current.checked_add(size)?;
        if next > BYTE_BUDGET {
            return None;
        }
        match used.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                return Some(ByteCharge {
                    used: used.clone(),
                    size,
                });
            }
            Err(actual) => current = actual,
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct Hub {
    slot: Arc<futures_util::lock::Mutex<Option<Handle>>>,
    fence: Arc<AtomicU64>,
}
impl Hub {
    pub(crate) fn invalidate(&self, epoch: u64) {
        self.fence.store(epoch, Ordering::Release);
    }
    async fn get(&self, client: &WorkClient) -> Result<Handle, ClientError> {
        let mut slot = self.slot.lock().await;
        let epoch = client.realtime_epoch();
        if let Some(handle) = slot
            .as_ref()
            .filter(|handle| handle.epoch == epoch && !handle.requests.is_closed())
        {
            return Ok(handle.clone());
        }
        let (socket, hello) = connect(client).await?;
        if client.realtime_epoch() != epoch {
            return Err(denied());
        }
        let (requests, receiver) = mpsc::channel(REQUEST_QUEUE);
        let handle = Handle {
            requests,
            epoch,
            current_epoch: client.realtime_epoch.clone(),
            fence: self.fence.clone(),
            ids: client.realtime_ids.clone(),
            outgoing: Arc::new(AtomicUsize::new(0)),
            hello,
        };
        spawn_local(run(socket, receiver, handle.clone()));
        *slot = Some(handle.clone());
        Ok(handle)
    }
}

pub(crate) struct AuthChange<'a>(&'a WorkClient);
impl WorkClient {
    pub(crate) fn realtime_auth_change(&self) -> AuthChange<'_> {
        self.fence_realtime();
        AuthChange(self)
    }
}
impl Drop for AuthChange<'_> {
    fn drop(&mut self) {
        self.0.fence_realtime();
    }
}
fn next_id(ids: &AtomicU64, prefix: &str) -> String {
    format!("{prefix}-{}", ids.fetch_add(1, Ordering::Relaxed))
}
fn denied() -> ClientError {
    ClientError::WebSocket { status: Some(401) }
}
fn unavailable() -> ClientError {
    ClientError::WebSocket { status: None }
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

#[derive(Clone)]
struct Handle {
    requests: mpsc::Sender<Packet>,
    epoch: u64,
    current_epoch: Arc<AtomicU64>,
    fence: Arc<AtomicU64>,
    ids: Arc<AtomicU64>,
    outgoing: Arc<AtomicUsize>,
    hello: Hello,
}
struct Packet {
    frame: ClientFrame,
    request_id: String,
    expected: Expected,
    reply: oneshot::Sender<Result<ServerFrame, ClientError>>,
    install: Option<(String, mpsc::Sender<Result<WireEvent, ClientError>>)>,
    _bytes: ByteCharge,
}
enum Expected {
    Reserved,
    Status(OperationRef),
    Result,
    Subscribed {
        session: String,
        subscription: String,
    },
    Unsubscribed,
    Pong,
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
            (Self::Result, ServerFrame::Result { .. }) => true,
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
            (Self::Unsubscribed, ServerFrame::Unsubscribed { .. }) => true,
            (Self::Pong, ServerFrame::Pong { .. }) => true,
            (_, ServerFrame::Error { .. }) => true,
            _ => false,
        }
    }
}
impl Handle {
    async fn request(
        &self,
        frame: ClientFrame,
        install: Option<(String, mpsc::Sender<Result<WireEvent, ClientError>>)>,
    ) -> Result<ServerFrame, ClientError> {
        if self.current_epoch.load(Ordering::Acquire) != self.epoch
            || self.fence.load(Ordering::Acquire) != self.epoch
        {
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
            ClientFrame::Unsubscribe { request_id, .. } => {
                (request_id.clone(), Expected::Unsubscribed)
            }
            ClientFrame::Ping { request_id, .. } => (request_id.clone(), Expected::Pong),
        };
        let text = frame.encode_text().map_err(|e| realtime_error(e.code))?;
        let bytes = charge(&self.outgoing, text.len())
            .ok_or_else(|| realtime_error(RealtimeErrorCode::LimitExceeded))?;
        let (reply, receiver) = oneshot::channel();
        let mut requests = self.requests.clone();
        requests
            .try_send(Packet {
                frame,
                request_id,
                expected,
                reply,
                install,
                _bytes: bytes,
            })
            .map_err(|_| realtime_error(RealtimeErrorCode::LimitExceeded))?;
        match futures_util::future::select(
            receiver.map(|result| result.map_err(|_| unavailable()).and_then(|x| x)),
            gloo_timers::future::sleep(std::time::Duration::from_millis(REQUEST_TIMEOUT_MS as u64))
                .map(|_| Err(unavailable())),
        )
        .await
        {
            futures_util::future::Either::Left((result, _)) => result,
            futures_util::future::Either::Right((result, _)) => result,
        }
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
        let frame = handle
            .request(
                ClientFrame::Query {
                    version: VERSION,
                    request_id: next_id(&client.realtime_ids, "query"),
                    session_id: session.into(),
                    payload,
                },
                None,
            )
            .await?;
        return match frame {
            ServerFrame::Result { value, .. } => {
                serde_json::from_value(value).map_err(ClientError::Json)
            }
            ServerFrame::Error { error, .. } => Err(realtime_error(error.code)),
            _ => Err(unavailable()),
        };
    }
    let value = mutation(
        client,
        session,
        RealtimeOperationKind::SessionCommand,
        payload,
    )
    .await?;
    serde_json::from_value(value).map_err(ClientError::Json)
}
pub(crate) async fn mutation(
    client: &WorkClient,
    session: &str,
    kind: RealtimeOperationKind,
    payload: Value,
) -> Result<Value, ClientError> {
    let handle = client.realtime.get(client).await?;
    let reserved = handle
        .request(
            ClientFrame::Reserve {
                version: VERSION,
                request_id: next_id(&client.realtime_ids, "reserve"),
                session_id: session.into(),
                client_instance_id: (*client.realtime_client_id).clone(),
                kind,
            },
            None,
        )
        .await?;
    let operation = match reserved {
        ServerFrame::Reserved { operation, .. } => operation,
        ServerFrame::Error { error, .. } => return Err(realtime_error(error.code)),
        _ => return Err(unavailable()),
    };
    let executed = handle
        .request(
            ClientFrame::Execute {
                version: VERSION,
                request_id: next_id(&client.realtime_ids, "execute"),
                operation: operation.clone(),
                payload,
            },
            None,
        )
        .await
        .map_err(|_| ClientError::OperationOutcomeUnknown {
            operation: operation.clone(),
        })?;
    match executed {
        ServerFrame::Status { status, .. } => operation_status_value(status, operation),
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

pub(crate) async fn subscribe(
    client: &WorkClient,
    session: &str,
    after: u64,
) -> Result<EventStream, ClientError> {
    let handle = client.realtime.get(client).await?;
    let subscription = next_id(&client.realtime_ids, "subscription");
    let (sender, receiver) = mpsc::channel(EVENT_QUEUE);
    let frame = ClientFrame::Subscribe {
        version: VERSION,
        request_id: next_id(&client.realtime_ids, "subscribe"),
        session_id: session.into(),
        subscription_id: subscription.clone(),
        after_seq: Some(after),
    };
    let subscribed = handle
        .request(frame, Some((subscription.clone(), sender)))
        .await?;
    if let ServerFrame::Error { error, .. } = subscribed {
        return Err(realtime_error(error.code));
    }
    Ok(Box::pin(WasmEventStream {
        receiver,
        handle,
        subscription,
        ended: false,
    }))
}
struct WasmEventStream {
    receiver: mpsc::Receiver<Result<WireEvent, ClientError>>,
    handle: Handle,
    subscription: String,
    ended: bool,
}
impl Stream for WasmEventStream {
    type Item = Result<WireEvent, ClientError>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.ended
            || self.handle.current_epoch.load(Ordering::Acquire) != self.handle.epoch
            || self.handle.fence.load(Ordering::Acquire) != self.handle.epoch
        {
            self.ended = true;
            return Poll::Ready(Some(Err(denied())));
        }
        self.receiver.poll_next_unpin(cx)
    }
}
impl Drop for WasmEventStream {
    fn drop(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        let request_id = next_id(&self.handle.ids, "unsubscribe");
        let frame = ClientFrame::Unsubscribe {
            version: VERSION,
            request_id: request_id.clone(),
            subscription_id: self.subscription.clone(),
        };
        if let Ok(text) = frame.encode_text() {
            if let Some(bytes) = charge(&self.handle.outgoing, text.len()) {
                let (reply, _receiver) = oneshot::channel();
                let _ = self.handle.requests.try_send(Packet {
                    frame,
                    request_id,
                    expected: Expected::Unsubscribed,
                    reply,
                    install: None,
                    _bytes: bytes,
                });
            }
        }
    }
}

struct WsSocket {
    socket: WebSocket,
    frames: mpsc::Receiver<FrameResult>,
    _open: Closure<dyn FnMut(Event)>,
    _message: Closure<dyn FnMut(MessageEvent)>,
    _error: Closure<dyn FnMut(Event)>,
    _close: Closure<dyn FnMut(Event)>,
}

impl Drop for WsSocket {
    fn drop(&mut self) {
        // The browser keeps event-handler function references on the WebSocket.
        // Detach them before Rust drops the wasm-bindgen closures; otherwise a
        // late error/close event calls into a dropped Rust closure and traps the
        // whole WASM event loop.
        self.socket.set_onopen(None);
        self.socket.set_onmessage(None);
        self.socket.set_onerror(None);
        self.socket.set_onclose(None);
        let _ = self.socket.close();
    }
}

async fn connect(client: &WorkClient) -> Result<(WsSocket, Hello), ClientError> {
    if client.bearer_token.is_some() {
        return Err(denied());
    }
    let auth: crate::AuthState = client.get("api/v1/auth").await?;
    if !auth.authenticated {
        return Err(denied());
    }
    let discovery: Discovery = match client.get("api/v1/transports").await {
        Err(ClientError::Api { status, .. })
            if status.as_u16() == 404 || status.as_u16() == 405 =>
        {
            return Err(ClientError::RealtimeNotAdvertised);
        }
        result => result?,
    };
    discovery.validate().map_err(|_| unavailable())?;
    let mut url = client.url("api/v1/realtime")?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(unavailable());
    }
    url.set_scheme(match url.scheme() {
        "http" => "ws",
        "https" => "wss",
        _ => return Err(unavailable()),
    })
    .map_err(|_| unavailable())?;
    let socket = WebSocket::new_with_str(url.as_str(), SUBPROTOCOL).map_err(|_| unavailable())?;
    socket.set_binary_type(BinaryType::Arraybuffer);
    let (frames, receiver) = mpsc::channel(64);
    let (open_tx, open_rx) = oneshot::channel::<Result<(), ClientError>>();
    let open_cell = std::cell::RefCell::new(Some(open_tx));
    let open = Closure::wrap(Box::new(move |_event: Event| {
        if let Some(sender) = open_cell.borrow_mut().take() {
            let _ = sender.send(Ok(()));
        }
    }) as Box<dyn FnMut(_)>);
    socket.set_onopen(Some(open.as_ref().unchecked_ref()));
    let used = Arc::new(AtomicUsize::new(0));
    let message_used = used.clone();
    let mut message_sender = frames.clone();
    let message = Closure::wrap(Box::new(move |event: MessageEvent| {
        let Some(text) = event.data().as_string() else {
            let _ = message_sender.try_send(Err(unavailable()));
            return;
        };
        let Some(bytes) = charge(&message_used, text.len()) else {
            let _ = message_sender.try_send(Err(realtime_error(RealtimeErrorCode::LimitExceeded)));
            return;
        };
        let result = ServerFrame::decode_text(&text)
            .map(|frame| Incoming {
                frame,
                _bytes: bytes,
            })
            .map_err(|e| realtime_error(e.code));
        let _ = message_sender.try_send(result);
    }) as Box<dyn FnMut(_)>);
    socket.set_onmessage(Some(message.as_ref().unchecked_ref()));
    let error_sender = frames.clone();
    let error = Closure::wrap(Box::new(move |_event: Event| {
        let mut sender = error_sender.clone();
        let _ = sender.try_send(Err(unavailable()));
    }) as Box<dyn FnMut(_)>);
    socket.set_onerror(Some(error.as_ref().unchecked_ref()));
    let close_sender = frames.clone();
    let close = Closure::wrap(Box::new(move |_event: Event| {
        let mut sender = close_sender.clone();
        let _ = sender.try_send(Err(unavailable()));
    }) as Box<dyn FnMut(_)>);
    socket.set_onclose(Some(close.as_ref().unchecked_ref()));
    let mut ws = WsSocket {
        socket,
        frames: receiver,
        _open: open,
        _message: message,
        _error: error,
        _close: close,
    };
    match futures_util::future::select(
        open_rx,
        gloo_timers::future::sleep(std::time::Duration::from_millis(CONNECT_TIMEOUT_MS as u64))
            .map(|_| Err(unavailable())),
    )
    .await
    {
        futures_util::future::Either::Left((result, _)) => result.map_err(|_| unavailable())??,
        futures_util::future::Either::Right((result, _)) => result?,
    }
    let incoming = ws.frames.next().await.ok_or_else(unavailable)??;
    let ServerFrame::Hello { hello, .. } = incoming.frame else {
        return Err(unavailable());
    };
    if hello.node_id != discovery.node_id
        || hello.owner_epoch != discovery.owner_epoch
        || hello.capabilities != discovery.capabilities
    {
        return Err(unavailable());
    }
    Ok((ws, hello))
}

struct BrowserSubscription {
    session: String,
    sender: mpsc::Sender<Result<WireEvent, ClientError>>,
}

async fn run(mut socket: WsSocket, mut requests: mpsc::Receiver<Packet>, handle: Handle) {
    let mut pending: HashMap<String, Packet> = HashMap::new();
    let mut deadlines: HashMap<String, f64> = HashMap::new();
    let mut subscriptions: HashMap<String, BrowserSubscription> = HashMap::new();
    let mut last_read = js_sys::Date::now();
    let mut last_ping = last_read;
    loop {
        let next_frame = socket.frames.next().fuse();
        let next_request = requests.next().fuse();
        let epoch_tick = gloo_timers::future::sleep(std::time::Duration::from_millis(100)).fuse();
        pin_mut!(next_frame, next_request, epoch_tick);
        select_biased! {
            incoming = next_frame => {
                let Some(result) = incoming else { break; };
                let incoming = match result { Ok(incoming) => incoming, Err(error) => { fail(&mut subscriptions, error); break; } };
                last_read = js_sys::Date::now();
                match incoming.frame {
                    ServerFrame::Events { session_id, subscription_id, events, .. } => {
                        let Some(subscription) = subscriptions.get_mut(&subscription_id) else { break; };
                        if subscription.session != session_id { break; }
                        let mut overflow = false;
                        for event in events { if subscription.sender.try_send(Ok(event)).is_err() { overflow = true; break; } }
                        if overflow {
                            let _ = subscription.sender.try_send(Err(unavailable()));
                            subscriptions.remove(&subscription_id);
                            let frame = ClientFrame::Unsubscribe { version: VERSION, request_id: next_id(&handle.ids, "close"), subscription_id };
                            let request_id = match &frame { ClientFrame::Unsubscribe { request_id, .. } => request_id.clone(), _ => unreachable!() };
                            if let Ok(text) = frame.encode_text() {
                                if let Some(bytes) = charge(&handle.outgoing, text.len()) {
                                    let (reply, _receiver) = oneshot::channel();
                                    if socket.socket.send_with_str(&text).is_err() { break; }
                                    deadlines.insert(request_id.clone(), js_sys::Date::now() + REQUEST_TIMEOUT_MS as f64);
                                    pending.insert(request_id.clone(), Packet { frame, request_id, expected: Expected::Unsubscribed, reply, install: None, _bytes: bytes });
                                } else { break; }
                            } else { break; }
                        }
                    }
                    frame => {
                        let request_id = match &frame {
                            ServerFrame::Reserved { request_id, .. } | ServerFrame::Status { request_id, .. } | ServerFrame::Result { request_id, .. } | ServerFrame::Subscribed { request_id, .. } | ServerFrame::Unsubscribed { request_id, .. } | ServerFrame::Pong { request_id, .. } => request_id,
                            ServerFrame::Error { request_id: Some(request_id), .. } => request_id,
                            _ => break,
                        };
                        let Some(packet) = pending.remove(request_id) else { break; };
                        deadlines.remove(request_id);
                        let owner = match &packet.expected {
                            Expected::Reserved => handle.hello.owner_epoch.as_str(),
                            Expected::Status(operation) => operation.owner_epoch.as_str(),
                            _ => "",
                        };
                        if !packet.expected.accepts(&frame, owner) { let _ = packet.reply.send(Err(unavailable())); break; }
                        let mut install = packet.install;
                        if let (Expected::Subscribed { session, subscription }, ServerFrame::Subscribed { .. }) = (&packet.expected, &frame) {
                            if let Some((id, sender)) = install.take() { if &id == subscription { subscriptions.insert(id, BrowserSubscription { session: session.clone(), sender }); } }
                        } else if let ServerFrame::Error { error, .. } = &frame {
                            if let Some((id, _sender)) = install.take() { subscriptions.remove(&id); let _ = packet.reply.send(Err(realtime_error(error.code))); continue; }
                        }
                        let _ = packet.reply.send(Ok(frame));
                    }
                }
            }
            _ = epoch_tick => {
                if handle.current_epoch.load(Ordering::Acquire) != handle.epoch
                    || handle.fence.load(Ordering::Acquire) != handle.epoch
                { break; }
                let now = js_sys::Date::now();
                let expired: Vec<String> = deadlines.iter().filter(|(_, deadline)| **deadline <= now).map(|(id, _)| id.clone()).collect();
                for id in expired { deadlines.remove(&id); if let Some(packet) = pending.remove(&id) { let _ = packet.reply.send(Err(unavailable())); } }
                if now - last_read >= IDLE_TIMEOUT_MS { break; }
                if now - last_ping >= HEARTBEAT_MS && pending.len() < MAX_PENDING {
                    let frame = ClientFrame::Ping { version: VERSION, request_id: next_id(&handle.ids, "ping") };
                    let request_id = match &frame { ClientFrame::Ping { request_id, .. } => request_id.clone(), _ => unreachable!() };
                    let text = match frame.encode_text() { Ok(text) => text, Err(_) => break };
                    let Some(bytes) = charge(&handle.outgoing, text.len()) else { break; };
                    let (reply, _receiver) = oneshot::channel();
                    if socket.socket.send_with_str(&text).is_err() { break; }
                    deadlines.insert(request_id.clone(), now + REQUEST_TIMEOUT_MS as f64);
                    pending.insert(request_id.clone(), Packet { frame, request_id, expected: Expected::Pong, reply, install: None, _bytes: bytes });
                    last_ping = now;
                }
            }
            request = next_request => {
                let Some(packet) = request else { break; };
                if pending.len() >= MAX_PENDING { let _ = packet.reply.send(Err(realtime_error(RealtimeErrorCode::LimitExceeded))); continue; }
                if matches!(packet.expected, Expected::Subscribed { .. }) && subscriptions.len() >= MAX_SUBSCRIPTIONS { let _ = packet.reply.send(Err(realtime_error(RealtimeErrorCode::LimitExceeded))); continue; }
                let text = match packet.frame.encode_text() { Ok(text) => text, Err(error) => { let _ = packet.reply.send(Err(realtime_error(error.code))); continue; } };
                if socket.socket.send_with_str(&text).is_err() { let _ = packet.reply.send(Err(unavailable())); break; }
                deadlines.insert(packet.request_id.clone(), js_sys::Date::now() + REQUEST_TIMEOUT_MS as f64);
                pending.insert(packet.request_id.clone(), packet);
            }
        }
        if handle.current_epoch.load(Ordering::Acquire) != handle.epoch
            || handle.fence.load(Ordering::Acquire) != handle.epoch
        {
            break;
        }
    }
    fail(&mut subscriptions, unavailable());
    for (_, packet) in pending {
        let _ = packet.reply.send(Err(unavailable()));
    }
    let _ = socket.socket.close();
}
fn fail(subscriptions: &mut HashMap<String, BrowserSubscription>, error: ClientError) {
    for subscription in subscriptions.values_mut() {
        let _ = subscription
            .sender
            .try_send(Err(error.clone_for_delivery()));
    }
    subscriptions.clear();
}
trait DeliveryClone {
    fn clone_for_delivery(&self) -> ClientError;
}
impl DeliveryClone for ClientError {
    fn clone_for_delivery(&self) -> ClientError {
        match self {
            ClientError::WebSocket { status } => ClientError::WebSocket { status: *status },
            ClientError::Api { status, message } => ClientError::Api {
                status: *status,
                message: message.clone(),
            },
            _ => unavailable(),
        }
    }
}
