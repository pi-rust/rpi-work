//! CLI contract tests against a tiny isolated HTTP fixture. These do not claim
//! owner/runtime validation (covered separately by verify-headless-node.py).
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    process::{Command, Output},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

struct Request {
    line: String,
    body: Value,
    cookie: bool,
}
struct Node {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Node {
    fn start(authorize: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = thread::spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                let Ok((stream, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut input = BufReader::new(stream);
                let mut line = String::new();
                input.read_line(&mut line).unwrap();
                let mut length = 0;
                let mut cookie = false;
                loop {
                    let mut header = String::new();
                    input.read_line(&mut header).unwrap();
                    if header == "\r\n" || header.is_empty() {
                        break;
                    }
                    if let Some((name, value)) = header.split_once(':') {
                        if name.eq_ignore_ascii_case("Content-Length") {
                            length = value.trim().parse().unwrap();
                        }
                        if name.eq_ignore_ascii_case("Cookie") {
                            cookie = value.contains("headless-fixture-auth=only-this-invocation");
                        }
                    }
                }
                let mut bytes = vec![0; length];
                input.read_exact(&mut bytes).unwrap();
                let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
                let login = line.starts_with("POST /api/v1/auth ");
                let response = if line.contains("/api/v1/auth ") {
                    json!({"required": authorize, "authenticated": authorize && (login || cookie)})
                } else if line.starts_with("POST /api/v1/sessions ") && authorize && cookie {
                    json!({"sessionId": "created-by-existing-node"})
                } else {
                    json!({"required": false, "authenticated": false})
                };
                observed
                    .lock()
                    .unwrap()
                    .push(Request { line, body, cookie });
                let body = response.to_string();
                let set_cookie = if login {
                    "Set-Cookie: headless-fixture-auth=only-this-invocation; Path=/; HttpOnly; SameSite=Lax\r\n"
                } else {
                    ""
                };
                write!(input.get_mut(), "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n{set_cookie}Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
                input.get_mut().flush().unwrap();
            }
        });
        Self {
            url,
            requests,
            stop,
            worker: Some(worker),
        }
    }
    fn cli(&self, args: &[&str], password: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_work-cli"));
        command
            .args(["--node", &self.url])
            .args(args)
            .env_remove("RPI_WORK_TRANSPORT")
            .env_remove("RPI_WORK_PASSWORD")
            .env("RPI_WORK_URL", "invalid-unused-environment-target")
            .env("RPI_WORK_RPI_BIN", "DO_NOT_SPAWN_REAL_RPI");
        if password {
            command.env("RPI_WORK_PASSWORD", "synthetic-password");
        }
        command.output().unwrap()
    }
}
impl Drop for Node {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}

#[test]
fn session_creation_authenticates_then_uses_existing_node_typed_route() {
    let node = Node::start(true);
    let output = node.cli(&["session-create", "project-a"], true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!({"sessionId":"created-by-existing-node"})
    );
    let requests = node.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].line.starts_with("POST /api/v1/auth "));
    assert!(requests[1].line.starts_with("GET /api/v1/auth "));
    assert!(requests[1].cookie);
    assert!(requests[2].line.starts_with("POST /api/v1/sessions "));
    assert!(requests[2].cookie);
    assert_eq!(requests[2].body, json!({"projectId":"project-a"}));
}

#[test]
fn optional_auth_does_not_authorize_a_session_creation() {
    let node = Node::start(false);
    let output = node.cli(&["session-create", "project-a"], false);
    assert!(!output.status.success());
    assert_eq!(output.stdout, [] as [u8; 0]);
    let requests = node.requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        1,
        "no session mutation before authorization"
    );
    assert!(requests[0].line.starts_with("GET /api/v1/auth "));
}
