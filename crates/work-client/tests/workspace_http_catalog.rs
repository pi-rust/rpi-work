#![cfg(not(target_arch = "wasm32"))]

//! Native HTTP facade contract, not a browser/native combined E2E test. The wire
//! shapes match tests/fixtures/ui-fixture.cjs: GET catalog envelopes, POST rename
//! {title}/{name}, and POST metadata {archived,pinned} (null means unchanged).
//! Each listener owns its catalog in memory; no LocalWorkspace, agent, prompt,
//! user directory, fixed service port, or external server is involved.

use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use work_client::{Project, SessionSummary, WorkspaceClient};

const PROJECT_ID: &str = "shared-project";
const SESSION_ID: &str = "shared-session";

#[derive(Clone)]
struct Catalog {
    project: Project,
    session: SessionSummary,
}

impl Catalog {
    fn seed(owner: &str) -> Self {
        Self {
            project: Project {
                id: PROJECT_ID.into(),
                name: format!("{owner} project"),
                // A label only: this path is never accessed on disk.
                path: format!("/fixture-only/{owner}"),
            },
            session: serde_json::from_value(json!({
                "sessionId": SESSION_ID, "projectId": PROJECT_ID,
                "title": format!("{owner} session"), "status": "idle",
                "createdAt": 1, "updatedAt": 2, "lastSeq": 0,
                "archived": false, "pinned": false
            }))
            .unwrap(),
        }
    }

    fn respond(&mut self, request: &Request) -> Value {
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/api/v1/auth") => {
                json!({"authenticated": true, "required": false})
            }
            ("GET", "/api/v1/projects") => json!({"projects": [self.project]}),
            ("GET", "/api/v1/sessions") => json!({"sessions": [self.session]}),
            ("POST", "/api/v1/projects/shared-project/rename") => {
                let name = request.body["name"].as_str().expect("rename name");
                assert_eq!(request.body, json!({"name": name}));
                self.project.name = name.into();
                json!({"project": self.project})
            }
            ("POST", "/api/v1/sessions/shared-session/rename") => {
                let title = request.body["title"].as_str().expect("rename title");
                assert_eq!(request.body, json!({"title": title}));
                self.session.title = Some(title.into());
                json!({"sessionId": SESSION_ID, "title": title, "status": "idle"})
            }
            ("POST", "/api/v1/sessions/shared-session/metadata") => {
                assert_eq!(request.body.as_object().unwrap().len(), 2);
                for field in ["archived", "pinned"] {
                    let value = request.body.get(field).expect("metadata field");
                    assert!(value.is_null() || value.is_boolean());
                }
                if let Some(value) = request.body["archived"].as_bool() {
                    self.session.archived = value;
                }
                if let Some(value) = request.body["pinned"].as_bool() {
                    self.session.pinned = value;
                }
                json!({
                    "sessionId": SESSION_ID, "archived": self.session.archived,
                    "pinned": self.session.pinned
                })
            }
            _ => panic!("unexpected HTTP operation: {request:?}"),
        }
    }
}

#[derive(Debug, PartialEq)]
struct Request {
    method: String,
    path: String,
    body: Value,
}

fn read_request(stream: &mut TcpStream) -> Request {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    let header_end = loop {
        let count = stream.read(&mut chunk).unwrap();
        assert!(count > 0, "connection closed before HTTP headers");
        bytes.extend_from_slice(&chunk[..count]);
        assert!(bytes.len() <= 65_536, "oversized fixture request");
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
    let mut parts = headers.lines().next().unwrap().split_whitespace();
    let method = parts.next().unwrap().to_owned();
    let path = parts.next().unwrap().to_owned();
    let length = headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map_or(0, |(_, value)| value.trim().parse::<usize>().unwrap());
    assert!(length <= 65_536, "oversized fixture body");
    while bytes.len() < header_end + length {
        let count = stream.read(&mut chunk).unwrap();
        assert!(count > 0, "connection closed before HTTP body");
        bytes.extend_from_slice(&chunk[..count]);
    }
    let body = if length == 0 {
        Value::Null
    } else {
        serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap()
    };
    Request { method, path, body }
}

struct HttpOwner {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Vec<Request>>>,
}

impl HttpOwner {
    fn start(mut catalog: Catalog) -> Self {
        // Binding before spawning also serves as readiness synchronization.
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = thread::spawn(move || {
            let mut requests = Vec::new();
            while !stopping.load(Ordering::Acquire) {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("fixture accept failed: {error}"),
                };
                // Windows may inherit the listener's nonblocking mode. Only
                // accept is polled; request I/O must block with bounded timeouts.
                stream.set_nonblocking(false).unwrap();
                let request = read_request(&mut stream);
                let response = catalog.respond(&request).to_string();
                requests.push(request);
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                    response.len()
                )
                .unwrap();
            }
            requests
        });
        Self {
            address,
            stop,
            worker: Some(worker),
        }
    }

    fn client(&self) -> WorkspaceClient {
        WorkspaceClient::new(&format!("http://{}", self.address)).unwrap()
    }

    fn finish(mut self) -> Vec<Request> {
        self.stop.store(true, Ordering::Release);
        self.worker
            .take()
            .unwrap()
            .join()
            .expect("HTTP fixture failed")
    }
}

impl Drop for HttpOwner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            // Also clean up on assertion failure without causing a double panic.
            let _ = worker.join();
        }
    }
}

async fn assert_catalog(client: &WorkspaceClient, expected: &Catalog) {
    assert_eq!(
        client.projects().await.unwrap(),
        vec![expected.project.clone()]
    );
    assert_eq!(
        client.sessions().await.unwrap(),
        vec![expected.session.clone()]
    );
}

#[tokio::test]
async fn independent_http_facades_share_only_their_owners_catalog() {
    let mut expected = Catalog::seed("owner-a");
    let mut other_expected = Catalog::seed("owner-b");
    let owner = HttpOwner::start(expected.clone());
    let other_owner = HttpOwner::start(other_expected.clone());
    assert_ne!(owner.address, other_owner.address);
    // Separate constructors, not clones of one facade/client/cache.
    let client_a = owner.client();
    let client_b = owner.client();
    let client_c = other_owner.client();
    assert!(!client_a.is_local() && !client_b.is_local() && !client_c.is_local());

    tokio::time::timeout(Duration::from_secs(15), async {
        for client in [&client_a, &client_b, &client_c] {
            let auth = client.auth().await.unwrap();
            assert!(auth.authenticated && !auth.required);
        }
        assert_catalog(&client_a, &expected).await;
        assert_catalog(&client_b, &expected).await;
        assert_catalog(&client_c, &other_expected).await;

        let renamed = client_a
            .rename_session(SESSION_ID, "跨端重命名会话")
            .await
            .unwrap();
        assert_eq!(renamed.session_id, SESSION_ID);
        expected.session.title = Some("跨端重命名会话".into());
        assert_eq!(renamed.title, expected.session.title);
        assert_catalog(&client_b, &expected).await;

        expected.project.name = "跨端重命名项目".into();
        let project = client_a
            .rename_project(PROJECT_ID, &expected.project.name)
            .await
            .unwrap();
        assert_eq!(project, expected.project);
        assert_catalog(&client_b, &expected).await;

        // Separate partial updates must preserve the other flag, including when
        // archived sessions are returned in the complete catalog for UI filtering.
        for (archived, pinned) in [
            (None, Some(true)),
            (Some(true), None),
            (None, Some(false)),
            (Some(false), None),
        ] {
            let prefs = client_a
                .session_preferences(SESSION_ID, archived, pinned)
                .await
                .unwrap();
            if let Some(value) = archived {
                expected.session.archived = value;
            }
            if let Some(value) = pinned {
                expected.session.pinned = value;
            }
            assert_eq!(prefs.session_id, SESSION_ID);
            assert_eq!(prefs.archived, expected.session.archived);
            assert_eq!(prefs.pinned, expected.session.pinned);
            // No sleeps, invalidation, shared client state, or client-B mutations.
            assert_catalog(&client_b, &expected).await;
            assert_catalog(&client_c, &other_expected).await;
        }

        // The same opaque session/project IDs on a second owner remain distinct,
        // including mutations flowing in the opposite direction.
        client_c
            .rename_session(SESSION_ID, "Only owner B")
            .await
            .unwrap();
        other_expected.session.title = Some("Only owner B".into());
        client_c
            .session_preferences(SESSION_ID, Some(true), Some(true))
            .await
            .unwrap();
        other_expected.session.archived = true;
        other_expected.session.pinned = true;
        assert_catalog(&client_c, &other_expected).await;
        assert_catalog(&client_b, &expected).await;
        assert_catalog(&client_a, &expected).await;
    })
    .await
    .expect("native HTTP catalog contract timed out");

    let requests = owner.finish();
    let posts: Vec<_> = requests
        .iter()
        .filter(|request| request.method == "POST")
        .map(|request| (request.path.as_str(), request.body.clone()))
        .collect();
    assert_eq!(
        posts,
        vec![
            (
                "/api/v1/sessions/shared-session/rename",
                json!({"title": "跨端重命名会话"})
            ),
            (
                "/api/v1/projects/shared-project/rename",
                json!({"name": "跨端重命名项目"})
            ),
            (
                "/api/v1/sessions/shared-session/metadata",
                json!({"archived": null, "pinned": true})
            ),
            (
                "/api/v1/sessions/shared-session/metadata",
                json!({"archived": true, "pinned": null})
            ),
            (
                "/api/v1/sessions/shared-session/metadata",
                json!({"archived": null, "pinned": false})
            ),
            (
                "/api/v1/sessions/shared-session/metadata",
                json!({"archived": false, "pinned": null})
            ),
        ]
    );
    for path in ["/api/v1/projects", "/api/v1/sessions"] {
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "GET" && r.path == path)
                .count(),
            10
        );
    }
    let other_requests = other_owner.finish();
    assert_eq!(
        other_requests.iter().filter(|r| r.method == "POST").count(),
        2
    );
}
