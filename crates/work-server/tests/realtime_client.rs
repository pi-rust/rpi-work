//! Integration coverage for the native WorkClient WebSocket hub.
//!
//! These tests deliberately use the isolated RPC fixture.  They never start an
//! real Agent, model, or existing node. Session processes are dedicated stubs.
#[path = "../../../tests/support/legacy_fixture.rs"]
mod legacy_fixture;

use futures_util::{SinkExt, StreamExt, future::join_all};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use work_client::{RemoteTransport, WorkClient};
use work_core::api::realtime::{SUBPROTOCOL, ServerFrame};
use work_node::workspace::LocalWorkspace;
use work_server::{HttpServer, ServerConfig};

type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

struct Fixture {
    server: HttpServer,
    owner: Arc<LocalWorkspace>,
    root: tempfile::TempDir,
    cookie: String,
    sessions: [String; 2],
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
                password: "isolated-client-hub".into(),
                allowed_hosts: vec![],
                node_id: "isolated-client-hub-node".into(),
                assets: None,
                web_proxy: false,
                allow_sharing: true,
            },
        )
        .unwrap();
        let project = owner
            .create_project("client hub fixture", None, false)
            .await
            .unwrap();
        let first =
            legacy_fixture::seed_legacy(&root.path().join("data"), &project.id, "client-hub-first");
        let second = legacy_fixture::seed_legacy(
            &root.path().join("data"),
            &project.id,
            "client-hub-second",
        );
        let login = reqwest::Client::new()
            .post(format!("http://{}/api/v1/auth", server.address()))
            .header("x-rpi-web", "1")
            .json(&json!({"password":"isolated-client-hub"}))
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
            sessions: [first, second],
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

    async fn connect(&self) -> Ws {
        let (mut ws, response) = connect_async(self.request()).await.unwrap();
        assert_eq!(response.status(), 101);
        assert_eq!(response.headers()["sec-websocket-protocol"], SUBPROTOCOL);
        loop {
            match ws.next().await.unwrap().unwrap() {
                Message::Text(text) => {
                    if matches!(
                        ServerFrame::decode_text(text.as_str()).unwrap(),
                        ServerFrame::Hello { .. }
                    ) {
                        return ws;
                    }
                }
                Message::Ping(bytes) => ws.send(Message::Pong(bytes)).await.unwrap(),
                Message::Pong(_) => {}
                other => panic!("unexpected handshake frame: {other:?}"),
            }
        }
    }

    async fn finish(mut self) {
        self.server.drain().await.unwrap();
        self.owner.shutdown().await;
        // Keep the temporary directory alive until the isolated owner has
        // stopped; the native fixture process uses it for its session files.
        drop(self.root);
    }
}

async fn close_socket(mut ws: Ws) {
    let _ = ws.close(None).await;
}

async fn client(fixture: &Fixture) -> WorkClient {
    let client = WorkClient::new(&format!("http://{}", fixture.server.address())).unwrap();
    client.login("isolated-client-hub").await.unwrap();
    client.set_remote_transport(RemoteTransport::WebSocketOnly);
    client
}

/// Fifteen independently authenticated sockets leave exactly the 16th node
/// slot.  Both subscriptions and both kinds of command must then share the
/// WorkClient hub; dropping one stream must not tear down the other.
#[tokio::test]
async fn cloned_clients_share_the_sixteenth_socket_and_subscription_lifetime() {
    let f = Fixture::new().await;
    let mut occupied = Vec::new();
    for _ in 0..15 {
        occupied.push(f.connect().await);
    }

    let client = client(&f).await;
    let clone = client.clone();
    let first = client.subscribe(&f.sessions[0], 0).await.unwrap();
    let mut second = clone.subscribe(&f.sessions[1], 0).await.unwrap();

    let query_client = client.clone();
    let mutation_client = client.clone();
    let (query, mutation) = tokio::join!(
        query_client.command::<Value>(&f.sessions[0], json!({"type":"get_state"})),
        mutation_client.command::<Value>(
            &f.sessions[1],
            json!({"type":"set_thinking_level","level":"medium"}),
        ),
    );
    assert!(query.unwrap().success);
    let mutation = mutation.unwrap();
    assert!(mutation.success);
    assert_eq!(mutation.data.unwrap()["request"]["level"], "medium");

    // Put a post-subscription event in the second stream after the first one
    // is dropped.  A shared hub must keep the second consumer alive.
    drop(first);
    f.owner
        .command::<Value>(
            &f.sessions[1],
            json!({"type":"prompt","message":"hub-drop-isolation"}),
        )
        .await
        .unwrap();
    let saw_settled = tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(event) = second.next().await {
            if event.unwrap().event_type == "agent_settled" {
                return true;
            }
        }
        false
    })
    .await
    .unwrap();
    assert!(
        saw_settled,
        "the surviving subscription must receive events"
    );

    drop(second);
    drop(clone);
    drop(client);
    for ws in occupied {
        close_socket(ws).await;
    }
    f.finish().await;
}

/// Logout increments the client's realtime epoch before the HTTP lease is
/// revoked.  A queued event from the old stream must therefore never escape
/// the fence, while a subsequent login can establish a fresh subscription.
#[tokio::test]
async fn logout_fences_cached_stream_and_relogin_can_subscribe() {
    let f = Fixture::new().await;
    let client = client(&f).await;
    let mut old = client.subscribe(&f.sessions[0], 0).await.unwrap();
    assert_eq!(old.next().await.unwrap().unwrap().event_type, "ready");

    f.owner
        .command::<Value>(
            &f.sessions[0],
            json!({"type":"prompt","message":"hub-logout-fence"}),
        )
        .await
        .unwrap();
    // Allow the native fixture's deterministic event batch to reach the
    // client's bounded queue before logout; the queued values are intentional.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let state = client.logout().await.unwrap();
    assert!(!state.authenticated);

    let old_result = tokio::time::timeout(Duration::from_secs(3), old.next())
        .await
        .unwrap();
    assert!(
        matches!(old_result, None | Some(Err(_))),
        "logout must fence cached events, got {old_result:?}"
    );

    client.login("isolated-client-hub").await.unwrap();
    client.set_remote_transport(RemoteTransport::WebSocketOnly);
    let mut fresh = client.subscribe(&f.sessions[0], 0).await.unwrap();
    assert_eq!(fresh.next().await.unwrap().unwrap().event_type, "ready");

    drop(fresh);
    drop(old);
    drop(client);
    f.finish().await;
}

/// Read-only queries use the hub's query path rather than the 32-operation
/// reservation pool.  A large concurrent read burst must not prevent a
/// mutation from reserving and executing on the same socket.
#[tokio::test]
async fn concurrent_get_state_queries_do_not_consume_mutation_reservations() {
    let f = Fixture::new().await;
    let client = client(&f).await;
    let reads = async {
        // Stay within the intentional 16-pending/32-queued admission bounds.
        // A client hub must reject excess work, not grow an unbounded queue.
        for _ in 0..8 {
            let batch = (0..8)
                .map(|_| client.command::<Value>(&f.sessions[0], json!({"type":"get_state"})));
            for result in join_all(batch).await {
                assert!(result.unwrap().success);
            }
        }
    };
    let mutation_client = client.clone();
    let mutation = tokio::spawn(async move {
        mutation_client
            .command::<Value>(
                "client-hub-first",
                json!({"type":"set_thinking_level","level":"high"}),
            )
            .await
    });
    let (_, mutation) = tokio::join!(reads, mutation);
    let mutation = mutation.unwrap().unwrap();
    assert!(mutation.success);
    assert_eq!(mutation.data.unwrap()["request"]["level"], "high");

    drop(client);
    f.finish().await;
}
