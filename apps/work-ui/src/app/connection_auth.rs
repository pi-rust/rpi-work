//! Android entry authentication. Passwords are memory-only and never Debug/Serialize.
//! Verified session cookies may separately be saved using Android Keystore.

use super::connections::{NodePassword, normalize_address, with_timeout};
use work_client::{AuthState, WorkspaceClient};

// No Clone, Debug or Serialize: an attempt consumes its bound password once.
pub(super) struct ManualAttempt {
    pub address: String,
    client: WorkspaceClient,
    secret: String,
}

pub(super) fn manual_prepare(
    address: &str,
    mut password: NodePassword,
) -> Result<ManualAttempt, String> {
    let address = normalize_address(address, true)?;
    let secret = password
        .take_for(&address)
        .ok_or("请输入当前电脑节点的密码。")?;
    let client = WorkspaceClient::new(&address).map_err(|_| "无法创建节点连接，请检查地址。")?;
    Ok(ManualAttempt {
        address,
        client,
        secret,
    })
}

fn require_authenticated(auth: AuthState) -> Result<(), String> {
    if auth.authenticated {
        Ok(())
    } else {
        Err("节点未确认登录，请检查密码后重试。".into())
    }
}

async fn authenticate(client: WorkspaceClient, secret: String) -> Result<WorkspaceClient, String> {
    with_timeout(async move {
        let login = client.login(&secret).await;
        drop(secret);
        require_authenticated(login.map_err(|_| "登录失败，请检查节点地址和密码。")?)?;
        // Do not trust required=false or a successful HTTP status. Verify the
        // authenticated session using the very same client's cookie jar.
        require_authenticated(
            client
                .auth()
                .await
                .map_err(|_| "无法验证登录状态，请重试。")?,
        )?;
        Ok(client)
    })
    .await?
}

impl ManualAttempt {
    pub async fn login(self) -> Result<super::entry::EntryNode, String> {
        let client = authenticate(self.client, self.secret).await?;
        Ok(super::entry::EntryNode {
            address: self.address,
            client,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::connections::NodePassword;

    #[test]
    fn manual_addresses_use_phone_policy() {
        for (raw, expected) in [
            (" HTTPS://PC.example:13030/ ", "https://pc.example:13030"),
            ("http://192.168.1.8:13030", "http://192.168.1.8:13030"),
            ("https://[2001:db8::8]:443/", "https://[2001:db8::8]:443"),
        ] {
            let attempt = manual_prepare(raw, NodePassword::new("fixture-only".into(), raw));
            assert!(attempt.is_ok());
            assert_eq!(attempt.ok().unwrap().address, expected);
        }
        for raw in [
            "",
            "pc.example:13030",
            "rpi-work://local",
            "http://localhost:13030",
            "http://127.0.0.1",
            "http://[::1]",
            "http://0.0.0.0",
            "http://0x7f000001",
            "https://user:pass@pc.example",
            "https://pc.example/path",
            "https://pc.example?password=unused",
            "https://pc.example#fragment",
            "http://pc.example:0",
            "http://pc.example:65536",
        ] {
            assert!(manual_prepare(raw, NodePassword::new("fixture-only".into(), raw)).is_err());
        }
    }

    #[test]
    fn password_must_be_nonempty_bound_and_consumed_once() {
        let address = "https://pc.example:13030";
        assert!(manual_prepare(address, NodePassword::default()).is_err());
        for other in [
            "https://other.example:13030",
            "http://pc.example:13030",
            "https://pc.example:13031",
        ] {
            assert!(
                manual_prepare(other, NodePassword::new("fixture-only".into(), address)).is_err()
            );
        }
        let mut draft = NodePassword::new("fixture-only".into(), address);
        assert!(manual_prepare(address, std::mem::take(&mut draft)).is_ok());
        assert!(draft.is_empty());
        assert!(manual_prepare(address, draft).is_err());
    }

    // The fixture bypasses phone address preparation ONLY to bind a loopback
    // socket. Production manual entry always goes through manual_prepare.
    #[cfg(not(target_arch = "wasm32"))]
    fn fixture(states: Vec<bool>, cookie: bool) -> (WorkspaceClient, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        use std::time::{Duration, Instant};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let thread = std::thread::spawn(move || {
            for (index, authenticated) in states.into_iter().enumerate() {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "missing auth fixture request");
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => panic!("auth fixture accept failed"),
                    }
                };
                // Windows can inherit listener nonblocking mode on accept.
                // This fixture reads synchronously with a finite read timeout.
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let header_end = loop {
                    let mut buffer = [0_u8; 1024];
                    let count = socket.read(&mut buffer).unwrap();
                    assert!(
                        count > 0 && request.len() < 16384,
                        "invalid fixture request"
                    );
                    request.extend_from_slice(&buffer[..count]);
                    if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
                let method = if index == 0 { "post" } else { "get" };
                assert!(
                    headers.starts_with(&format!("{method} /api/v1/auth http/1.1\r\n")),
                    "only auth endpoint is permitted"
                );
                let has_cookie = headers
                    .lines()
                    .any(|line| line.starts_with("cookie:") && line.contains("entry_fixture=1"));
                assert_eq!(
                    has_cookie,
                    index > 0 && cookie,
                    "cookie jar must survive login"
                );
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                while request.len() < header_end + length {
                    let mut buffer = [0_u8; 1024];
                    let count = socket.read(&mut buffer).unwrap();
                    assert!(count > 0, "incomplete fixture body");
                    request.extend_from_slice(&buffer[..count]);
                }
                if index == 0 {
                    let value: serde_json::Value =
                        serde_json::from_slice(&request[header_end..header_end + length])
                            .unwrap_or_default();
                    // Do not print the body or password, even on assertion failure.
                    assert!(
                        value["password"].as_str() == Some("fixture-only"),
                        "password body missing"
                    );
                }
                drop(request);
                let body =
                    serde_json::json!({"required":false,"authenticated":authenticated}).to_string();
                let set_cookie = if index == 0 && cookie {
                    "Set-Cookie: entry_fixture=1; Path=/; HttpOnly\r\n"
                } else {
                    ""
                };
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{set_cookie}Connection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        (WorkspaceClient::new(&address).unwrap(), thread)
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn http_login_verifies_auth_and_reuses_cookie_client() {
        let (client, server) = fixture(vec![true, true, true, true], true);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let client = authenticate(client, "fixture-only".into()).await;
            assert!(client.is_ok(), "fixture login should succeed");
            assert!(client.ok().unwrap().auth().await.unwrap().authenticated);
        });
        server.join().unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn http_rejects_unauthenticated_login_missing_cookie_and_revoked_verification() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for (states, cookie) in [
            (vec![true, false], true),
            (vec![true, false], false),
            (vec![true, true, false], true),
        ] {
            let (client, server) = fixture(states, cookie);
            assert!(
                runtime
                    .block_on(authenticate(client, "fixture-only".into()))
                    .is_err()
            );
            server.join().unwrap();
        }
    }

    #[test]
    fn auth_requires_explicit_authenticated_even_when_not_required() {
        for required in [false, true] {
            assert!(
                require_authenticated(work_client::AuthState {
                    required,
                    authenticated: true
                })
                .is_ok()
            );
            assert!(
                require_authenticated(work_client::AuthState {
                    required,
                    authenticated: false
                })
                .is_err()
            );
        }
    }
}
