//! Native remembered connection policy. Only the Android bridge may persist its
//! serialized credentials, and only in Keystore-encrypted storage (never Web JS).
use super::connections::{normalize_address, with_timeout};
use super::entry::EntryNode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use work_client::{ClientError, RememberedSession, WorkClient, WorkspaceClient};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionEnvelope {
    version: u8,
    address: String,
    node_id: Option<String>,
    session: Option<RememberedSession>,
}

pub(super) struct StoredConnection {
    pub address: String,
    pub attempt: Option<ResumeAttempt>,
    pub unavailable: bool,
}

pub(super) struct ResumeAttempt {
    address: String,
    node_id: Option<String>,
    session: RememberedSession,
}

pub(super) struct ResumeFailure {
    pub discard: bool,
    pub message: &'static str,
}

fn failure(discard: bool) -> ResumeFailure {
    ResumeFailure {
        discard,
        message: if discard {
            "已记住电脑地址，登录已失效，请重新输入密码或扫码。"
        } else {
            "已记住连接，但暂时无法连接电脑。请检查网络后恢复连接，或重新登录。"
        },
    }
}

fn rejected(error: &ClientError) -> bool {
    matches!(error, ClientError::Api { status, .. } if matches!(status.as_u16(), 401 | 403))
}

fn valid_identity(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)
}

/// Plain HTTP is supported on trusted LANs, but must not replay a saved cookie
/// unattended after a process restart onto a potentially different network.
pub(super) fn automatic_restore_allowed(address: &str) -> bool {
    normalize_address(address, true).is_ok_and(|address| address.starts_with("https://"))
}

/// Treat the entire bridge result as untrusted. Never echo its serde error.
pub(super) fn decode(value: Value) -> StoredConnection {
    let address = value["address"]
        .as_str()
        .and_then(|raw| normalize_address(raw, true).ok())
        .unwrap_or_default();
    let mut stored = StoredConnection {
        address,
        attempt: None,
        unavailable: value["unavailable"].as_bool().unwrap_or(true),
    };
    if value["session"].is_null() {
        return stored;
    }
    let Ok(envelope) = serde_json::from_value::<SessionEnvelope>(value["session"].clone()) else {
        stored.unavailable = true;
        return stored;
    };
    if envelope.version != 1
        || stored.address.is_empty()
        || envelope.address != stored.address
        || envelope
            .node_id
            .as_ref()
            .is_some_and(|id| !valid_identity(id))
        || (envelope.session.is_some() && envelope.node_id.is_none())
    {
        stored.unavailable = true;
        return stored;
    }
    stored.attempt = envelope.session.map(|session| ResumeAttempt {
        address: stored.address.clone(),
        node_id: envelope.node_id,
        session,
    });
    stored
}

/// Called only after an authenticated login result passes its cancellation epoch.
/// An unsupported node can still remember its address without a cookie credential.
pub(super) fn encode(
    node: &EntryNode,
    node_id: Option<String>,
    now: u64,
) -> Option<(String, bool)> {
    let address = normalize_address(&node.address, true).ok()?;
    let node_id = node_id.filter(|id| valid_identity(id));
    // A transient identity-probe failure must never downgrade a saved cookie
    // into an unpinned credential sent to whoever next occupies this address.
    let session = node_id.as_ref().and_then(|_| match &node.client {
        WorkspaceClient::Remote(client) => client.remembered_session(now),
        #[cfg(not(target_os = "android"))]
        WorkspaceClient::Local(_) => None,
    });
    let has_session = session.is_some();
    serde_json::to_string(&SessionEnvelope {
        version: 1,
        address,
        node_id,
        session,
    })
    .ok()
    .filter(|payload| payload.len() <= 32768)
    .map(|payload| (payload, has_session))
}

impl ResumeAttempt {
    pub async fn resume(self, now: u64) -> Result<EntryNode, ResumeFailure> {
        // Validate time/origin/format before even the public identity probe.
        let client = WorkClient::restore_session(&self.address, self.session, now)
            .map_err(|_| failure(true))?;
        let expected = self
            .node_id
            .filter(|id| valid_identity(id))
            .ok_or_else(|| failure(true))?;
        let address = self.address;
        with_timeout(async move {
            // A different computer at the same IP must not receive the old cookie.
            // On HTTP this is an identity sanity check, not TLS authentication.
            let fresh = WorkClient::new(&address).map_err(|_| failure(true))?;
            let observed = fresh
                .qr_authorization_node()
                .await
                .map_err(|error| failure(rejected(&error)))?;
            if observed.as_deref() != Some(expected.as_str()) {
                return Err(failure(true));
            }
            let auth = client
                .auth()
                .await
                .map_err(|error| failure(rejected(&error)))?;
            if !auth.authenticated {
                return Err(failure(true));
            }
            Ok(EntryNode {
                address,
                client: client.into(),
            })
        })
        .await
        .map_err(|_| failure(false))?
    }
}

/// Optional pin from a fresh unauthenticated transport, never the saved cookie jar.
pub(super) async fn identity(address: &str) -> Option<String> {
    let client = WorkClient::new(address).ok()?;
    with_timeout(client.qr_authorization_node())
        .await
        .ok()?
        .ok()?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        future::Future,
        io::{Read, Write},
        net::TcpListener,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread::{self, JoinHandle},
        time::{Duration, Instant},
    };

    const COOKIE: &str = "rpi_work_desktop_0123456789abcdef=fixture-only";
    const PIN: &str = "fixture-node";

    fn run<T>(future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    struct Reply {
        path: &'static str,
        cookie: bool,
        status: u16,
        body: Option<Value>,
        set_cookie: bool,
    }

    fn probe(status: u16, body: Value) -> Reply {
        Reply {
            path: "/api/v1/health",
            cookie: false,
            status,
            body: Some(body),
            set_cookie: false,
        }
    }

    fn auth(status: u16, authenticated: bool) -> Reply {
        Reply {
            path: "/api/v1/auth",
            cookie: true,
            status,
            body: Some(json!({"required":false,"authenticated":authenticated})),
            set_cookie: false,
        }
    }

    fn pinned() -> Reply {
        probe(200, json!({"sharingEnabled":true,"nodeId":PIN}))
    }

    // Scripted native HTTP only: no model session, device claim, or real credential.
    // Stop/join on every exit; accept/read/write and stalled responses are bounded.
    struct Server {
        address: String,
        stop: Arc<AtomicBool>,
        worker: Option<JoinHandle<usize>>,
        expected: usize,
    }

    impl Server {
        fn new(replies: Vec<Reply>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = format!("http://{}", listener.local_addr().unwrap());
            listener.set_nonblocking(true).unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let stopping = stop.clone();
            let expected = replies.len();
            let worker = thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(40);
                let mut count = 0;
                while !stopping.load(Ordering::SeqCst) {
                    assert!(Instant::now() < deadline, "fixture deadline exceeded");
                    let mut socket = match listener.accept() {
                        Ok((socket, _)) => socket,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::park_timeout(Duration::from_millis(2));
                            continue;
                        }
                        Err(_) => panic!("fixture accept failed"),
                    };
                    // Accepted sockets inherit nonblocking mode on Windows.
                    // Read a complete bounded request instead of racing WouldBlock.
                    socket.set_nonblocking(false).unwrap();
                    let reply = replies.get(count).expect("unexpected HTTP request");
                    count += 1;
                    socket
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    socket
                        .set_write_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut bytes = Vec::new();
                    loop {
                        let mut buffer = [0_u8; 1024];
                        let read = socket.read(&mut buffer).ok().expect("fixture read failed");
                        assert!(read > 0 && bytes.len() < 16384, "invalid fixture request");
                        bytes.extend_from_slice(&buffer[..read]);
                        if bytes.windows(4).any(|part| part == b"\r\n\r\n") {
                            break;
                        }
                    }
                    let headers = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
                    // Boolean assertions only: never render credentials or request headers.
                    assert!(headers.starts_with(&format!("get {} http/1.1\r\n", reply.path)));
                    assert!(
                        !headers
                            .lines()
                            .any(|line| line.starts_with("authorization:"))
                    );
                    let cookies: Vec<_> = headers
                        .lines()
                        .filter_map(|line| line.strip_prefix("cookie:"))
                        .collect();
                    if reply.cookie {
                        assert!(cookies.len() == 1 && cookies[0].trim() == COOKIE);
                    } else {
                        assert!(cookies.is_empty(), "public probe must not carry cookies");
                    }
                    assert!(
                        !headers
                            .lines()
                            .any(|line| line.starts_with("content-length:")
                                && line.trim() != "content-length: 0")
                    );
                    drop(headers);
                    drop(bytes);
                    let Some(body) = &reply.body else {
                        let deadline = Instant::now() + Duration::from_secs(20);
                        while !stopping.load(Ordering::SeqCst) && Instant::now() < deadline {
                            thread::park_timeout(Duration::from_millis(2));
                        }
                        continue;
                    };
                    let body = body.to_string();
                    let cookie = if reply.set_cookie {
                        "Set-Cookie: rpi_work_desktop_0123456789abcdef=probe-only; Path=/; HttpOnly\r\n"
                    } else {
                        ""
                    };
                    assert!(write!(socket, "HTTP/1.1 {} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{cookie}Connection: close\r\n\r\n{body}", reply.status, body.len()).is_ok(), "fixture write failed");
                }
                count
            });
            Self {
                address,
                stop,
                worker: Some(worker),
                expected,
            }
        }

        fn finish(mut self) {
            self.stop.store(true, Ordering::SeqCst);
            let count = self
                .worker
                .take()
                .unwrap()
                .join()
                .ok()
                .expect("HTTP fixture failed");
            assert!(count == self.expected, "missing HTTP fixture request");
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(worker) = self.worker.take() {
                let result = worker.join();
                if !thread::panicking() {
                    assert!(result.is_ok(), "HTTP fixture failed");
                }
            }
        }
    }

    // Phone-policy decode intentionally rejects loopback. Bypass only that UI
    // boundary, keeping the real opaque credential validation and HTTP client.
    fn attempt(address: &str, node_id: Option<&str>) -> ResumeAttempt {
        let session = serde_json::from_value(envelope(address, 100)["session"].clone())
            .ok()
            .expect("valid synthetic session");
        ResumeAttempt {
            address: address.into(),
            node_id: node_id.map(str::to_owned),
            session,
        }
    }

    fn envelope(address: &str, saved_at: u64) -> Value {
        json!({"version":1,"address":address,"nodeId":PIN,"session":{
            "endpoint":format!("{address}/"),
            "cookieHeader":"rpi_work_desktop_0123456789abcdef=fixture-only",
            "savedAt":saved_at,"expiresAt":saved_at+43200
        }})
    }

    #[test]
    fn http_identity_uses_fresh_unauthenticated_client_and_requires_sharing() {
        for (body, expected) in [
            (json!({"sharingEnabled":true,"nodeId":PIN}), Some(PIN)),
            (json!({"sharingEnabled":false,"nodeId":PIN}), None),
            (json!({"sharingEnabled":true}), None),
            (json!({"sharingEnabled":true,"nodeId":""}), None),
        ] {
            let server = Server::new(vec![probe(200, body)]);
            assert!(run(identity(&server.address)).as_deref() == expected);
            server.finish();
        }
        for status in [401, 403, 500] {
            let server = Server::new(vec![probe(status, json!({"error":"fixture"}))]);
            assert!(run(identity(&server.address)).is_none());
            server.finish();
        }
    }

    #[test]
    fn http_resume_probes_pin_without_cookie_then_verifies_saved_cookie() {
        let mut public = pinned();
        // A cookie set by the probe must not replace the remembered credential.
        public.set_cookie = true;
        let server = Server::new(vec![public, auth(200, true), auth(200, true)]);
        let node = run(attempt(&server.address, Some(PIN)).resume(101))
            .ok()
            .expect("resume succeeds");
        assert!(node.address == server.address);
        assert!(
            run(node.client.auth())
                .ok()
                .is_some_and(|state| state.authenticated)
        );
        server.finish();
    }

    #[test]
    fn http_changed_missing_or_disabled_node_discards_without_auth_or_cookie_leak() {
        for body in [
            json!({"sharingEnabled":true,"nodeId":"different-node"}),
            json!({"sharingEnabled":true}),
            json!({"sharingEnabled":false,"nodeId":PIN}),
        ] {
            let server = Server::new(vec![probe(200, body)]);
            let failure = run(attempt(&server.address, Some(PIN)).resume(101))
                .err()
                .expect("pin rejected");
            assert!(failure.discard);
            server.finish();
        }
    }

    #[test]
    fn http_auth_false_and_401_403_discard_but_never_trust_required_false() {
        for (status, authenticated) in [(200, false), (401, true), (403, true)] {
            let server = Server::new(vec![pinned(), auth(status, authenticated)]);
            let failure = run(attempt(&server.address, Some(PIN)).resume(101))
                .err()
                .expect("auth rejected");
            assert!(failure.discard);
            server.finish();
        }
        for status in [401, 403] {
            let server = Server::new(vec![probe(status, json!({"error":"fixture"}))]);
            assert!(
                run(attempt(&server.address, Some(PIN)).resume(101))
                    .err()
                    .is_some_and(|failure| failure.discard)
            );
            server.finish();
        }
    }

    #[test]
    fn http_offline_and_500_preserve_credentials_for_retry() {
        for replies in [
            vec![probe(500, json!({"error":"fixture"}))],
            vec![pinned(), auth(500, true)],
        ] {
            let server = Server::new(replies);
            assert!(
                run(attempt(&server.address, Some(PIN)).resume(101))
                    .err()
                    .is_some_and(|failure| !failure.discard)
            );
            server.finish();
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        assert!(
            run(attempt(&address, Some(PIN)).resume(101))
                .err()
                .is_some_and(|failure| !failure.discard)
        );
    }

    #[test]
    fn http_probe_and_auth_timeouts_preserve_credentials() {
        for at_probe in [true, false] {
            let mut stalled = if at_probe { pinned() } else { auth(200, true) };
            stalled.body = None;
            let replies = if at_probe {
                vec![stalled]
            } else {
                vec![pinned(), stalled]
            };
            let server = Server::new(replies);
            let started = Instant::now();
            assert!(
                run(attempt(&server.address, Some(PIN)).resume(101))
                    .err()
                    .is_some_and(|failure| !failure.discard)
            );
            assert!(started.elapsed() >= Duration::from_secs(14));
            assert!(started.elapsed() < Duration::from_secs(20));
            server.finish();
        }
    }

    #[test]
    fn http_repeated_restore_uses_new_clients_and_original_opaque_expiry() {
        let mut unauthenticated = auth(200, false);
        unauthenticated.cookie = false;
        let mut unauthenticated_after = auth(200, false);
        unauthenticated_after.cookie = false;
        let server = Server::new(vec![
            unauthenticated,
            pinned(),
            auth(200, true),
            pinned(),
            auth(200, true),
            unauthenticated_after,
        ]);
        let fresh = WorkClient::new(&server.address).ok().expect("fresh client");
        assert!(!run(fresh.auth()).ok().expect("fresh auth").authenticated);
        let saved = attempt(&server.address, Some(PIN)).session;
        assert!(saved.expires_at() == 43300);
        for now in [101, 43299] {
            let restored = ResumeAttempt {
                address: server.address.clone(),
                node_id: Some(PIN.into()),
                session: saved.clone(),
            };
            assert!(run(restored.resume(now)).is_ok());
            // Resume does not resave/extend the opaque persisted TTL.
            assert!(saved.expires_at() == 43300);
            let payload = serde_json::to_value(&saved)
                .ok()
                .expect("synthetic session serializes");
            assert!(payload["savedAt"] == 100 && payload["expiresAt"] == 43300);
        }
        let fresh = WorkClient::new(&server.address)
            .ok()
            .expect("another fresh client");
        assert!(
            !run(fresh.auth())
                .ok()
                .expect("fresh auth after restores")
                .authenticated
        );
        let expired = ResumeAttempt {
            address: server.address.clone(),
            node_id: Some(PIN.into()),
            session: saved,
        };
        assert!(
            run(expired.resume(43300))
                .err()
                .is_some_and(|failure| failure.discard)
        );
        server.finish();
    }

    #[test]
    fn encode_captures_only_address_pin_cookie_and_ttl_or_explicit_null_session() {
        let address = "https://pc.example:13031";
        // Synthetic opaque cookie provider: no login password or QR ticket needed.
        let client = WorkClient::restore_session(address, attempt(address, None).session, 100)
            .ok()
            .expect("synthetic cookie provider");
        let node = EntryNode {
            address: format!("{address}/"),
            client: client.into(),
        };
        let (payload, has_session) = encode(&node, Some(PIN.into()), 100).expect("encoded session");
        assert!(has_session);
        let value: Value = serde_json::from_str(&payload).ok().expect("valid envelope");
        assert!(
            value
                == json!({"version":1,"address":address,"nodeId":PIN,
            "session":{"endpoint":format!("{address}/"),"cookieHeader":COOKIE,"savedAt":100,"expiresAt":43300}})
        );
        assert!(
            !payload.contains("password") && !payload.contains("ticket") && !payload.contains("qr")
        );
        assert!(
            decode(json!({"address":address,"session":value,"unavailable":false}))
                .attempt
                .is_some()
        );

        let node = EntryNode {
            address: address.into(),
            client: WorkClient::new(address)
                .ok()
                .expect("address-only client")
                .into(),
        };
        let (payload, has_session) = encode(&node, None, 100).expect("encoded address");
        assert!(!has_session);
        let value: Value = serde_json::from_str(&payload)
            .ok()
            .expect("valid address envelope");
        assert!(value == json!({"version":1,"address":address,"nodeId":null,"session":null}));
        let stored = decode(json!({"address":address,"session":value,"unavailable":false}));
        assert!(stored.address == address && stored.attempt.is_none() && !stored.unavailable);
    }

    #[test]
    fn storage_keeps_safe_address_when_missing_or_corrupt_without_accepting_a_session() {
        for session in [
            Value::Null,
            json!({"password":"never accepted"}),
            json!({"version":99}),
        ] {
            let stored = decode(
                json!({"address":"https://pc.example:13031/","session":session,"unavailable":false}),
            );
            assert!(stored.address == "https://pc.example:13031");
            assert!(stored.attempt.is_none());
        }
        for raw in [
            "https://user:pass@pc.example",
            "http://localhost",
            "http://127.0.0.1:13031",
            "http://[::1]:13031",
            "https://pc.example?token=bad",
            "not-an-address",
        ] {
            let stored = decode(json!({"address":raw,"session":null,"unavailable":false}));
            assert!(stored.address.is_empty());
            assert!(stored.attempt.is_none());
        }
    }

    #[test]
    fn envelope_version_node_identity_and_address_must_agree() {
        let address = "https://pc.example:13031";
        let good = envelope(address, 100);
        assert!(
            decode(json!({"address":address,"session":good,"unavailable":false}))
                .attempt
                .is_some()
        );
        for bad in [
            {
                let mut v = good.clone();
                v["address"] = json!("https://other.example");
                v
            },
            {
                let mut v = good.clone();
                v["version"] = json!(2);
                v
            },
            {
                let mut v = good.clone();
                v["nodeId"] = json!("\n");
                v
            },
            {
                let mut v = good.clone();
                v["extra"] = json!(true);
                v
            },
        ] {
            let stored = decode(json!({"address":address,"session":bad,"unavailable":false}));
            assert!(stored.attempt.is_none());
            assert!(stored.unavailable);
        }
    }

    #[test]
    fn only_authenticated_https_can_restore_without_network_confirmation() {
        assert!(automatic_restore_allowed("https://pc.example:13031"));
        assert!(!automatic_restore_allowed("http://192.168.3.27:13031"));
        assert!(!automatic_restore_allowed("http://pc.example"));
        assert!(!automatic_restore_allowed("https://user:pass@pc.example"));
        assert!(!automatic_restore_allowed("https://pc.example/path"));
    }

    #[test]
    fn failed_identity_probe_saves_address_only_and_old_unpinned_records_never_send_cookies() {
        let address = "https://pc.example:13031";
        let client = WorkClient::restore_session(address, attempt(address, None).session, 100)
            .ok()
            .unwrap();
        let node = EntryNode {
            address: address.into(),
            client: client.into(),
        };
        for identity in [None, Some(String::new()), Some("\n".into())] {
            let (payload, has_session) = encode(&node, identity, 100).unwrap();
            assert!(!has_session);
            let value: Value = serde_json::from_str(&payload).unwrap();
            assert!(value["session"].is_null());
            assert!(!payload.contains("cookieHeader"));
        }
        let mut old = envelope(address, 100);
        old["nodeId"] = Value::Null;
        let stored = decode(json!({"address":address,"session":old,"unavailable":false}));
        assert!(stored.attempt.is_none() && stored.unavailable);
        assert!(
            run(attempt(address, None).resume(101))
                .err()
                .is_some_and(|failure| failure.discard)
        );
    }

    #[test]
    fn expired_or_cross_endpoint_credentials_are_discarded_before_network() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for (address, session, now) in [
            (
                "https://pc.example",
                envelope("https://pc.example", 100),
                43300,
            ),
            (
                "https://other.example",
                {
                    let mut v = envelope("https://pc.example", 100);
                    v["address"] = json!("https://other.example");
                    v
                },
                101,
            ),
        ] {
            let attempt = decode(json!({"address":address,"session":session,"unavailable":false}))
                .attempt
                .unwrap();
            let failure = runtime.block_on(attempt.resume(now)).err().unwrap();
            assert!(failure.discard);
        }
    }
}
