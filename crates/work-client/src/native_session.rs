//! Native-only, endpoint-bound cookie snapshots. These are credentials, not auth proof.

use std::sync::Arc;

use reqwest::{Url, cookie::CookieStore};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{ClientError, WorkClient, build_http_client};

const MAX_AGE: u64 = 12 * 60 * 60;
const MAX_COOKIE_HEADER: usize = 8192;

/// Opaque native authentication credential for encrypted storage by the UI.
///
/// Serialization contains a cookie credential: never log it or store it in plaintext.
/// This deliberately has no `Debug` or cookie accessor. The local expiry is only a
/// twelve-hour upper bound; callers must always recheck `WorkClient::auth` after
/// restoring, and must not extend the lifetime by saving a restored snapshot again.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RememberedSession {
    endpoint: String,
    cookie_header: String,
    saved_at: u64,
    expires_at: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionPayload {
    endpoint: String,
    cookie_header: String,
    saved_at: u64,
    expires_at: u64,
}

impl<'de> Deserialize<'de> for RememberedSession {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Even serde's type/unknown-field errors can quote supplied secrets.
        let payload = SessionPayload::deserialize(deserializer)
            .map_err(|_| D::Error::custom("invalid remembered session"))?;
        let saved = Self {
            endpoint: payload.endpoint,
            cookie_header: payload.cookie_header,
            saved_at: payload.saved_at,
            expires_at: payload.expires_at,
        };
        if !saved.valid() {
            return Err(D::Error::custom("invalid remembered session"));
        }
        Ok(saved)
    }
}

impl RememberedSession {
    /// Local expiration in Unix seconds, not authoritative service validity.
    #[must_use]
    pub fn expires_at(&self) -> u64 {
        self.expires_at
    }

    fn valid(&self) -> bool {
        root_endpoint(&self.endpoint).is_some_and(|url| url.as_str() == self.endpoint)
            && self.expires_at > self.saved_at
            && self.expires_at - self.saved_at <= MAX_AGE
            && cookie_pairs(&self.cookie_header).is_some()
    }
}

// Check the raw suffix as well: URL parsing otherwise normalizes dot segments,
// backslashes, whitespace and empty userinfo into innocuous-looking root URLs.
fn root_endpoint(value: &str) -> Option<Url> {
    if value.len() > 2048
        || value
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
        || value.contains(['\\', '?', '#', '@'])
    {
        return None;
    }
    let (_, authority) = value.split_once("://")?;
    if authority
        .find('/')
        .is_some_and(|index| &authority[index..] != "/")
    {
        return None;
    }
    let url = Url::parse(value).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none())
    .then_some(url)
}

fn auth_cookie_name(name: &str) -> bool {
    // The shared desktop/standalone HTTP server namespaces auth by node identity.
    name.strip_prefix("rpi_work_desktop_")
        .is_some_and(|suffix| suffix.len() == 16 && suffix.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn cookie_value(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| matches!(b, 0x21 | 0x23..=0x2b | 0x2d..=0x3a | 0x3c..=0x5b | 0x5d..=0x7e))
}

fn cookie_pairs(header: &str) -> Option<Vec<(&str, &str)>> {
    if header.is_empty() || header.len() > MAX_COOKIE_HEADER {
        return None;
    }
    let mut pairs = Vec::new();
    for pair in header.split("; ") {
        let (name, value) = pair.split_once('=')?;
        if !auth_cookie_name(name)
            || !cookie_value(value)
            || pairs.iter().any(|(previous, _)| *previous == name)
            || pairs.len() >= 32
        {
            return None;
        }
        pairs.push((name, value));
    }
    Some(pairs)
}

impl WorkClient {
    /// Snapshot only RPI auth cookies applicable to `/api/v1/auth`.
    ///
    /// `now` is Unix seconds. No password, bearer token, cookie attributes, or jar
    /// is persisted. A snapshot does not establish that the service accepts it.
    #[must_use]
    pub fn remembered_session(&self, now: u64) -> Option<RememberedSession> {
        let endpoint = root_endpoint(self.base_url.as_str())?;
        let auth_url = endpoint.join("api/v1/auth").ok()?;
        let header = self.cookie_jar.cookies(&auth_url)?;
        let header = header.to_str().ok()?;
        if header.len() > MAX_COOKIE_HEADER {
            return None;
        }
        let selected: Vec<_> = header
            .split(';')
            .map(str::trim)
            .filter(|pair| {
                pair.split_once('=')
                    .is_some_and(|(name, _)| auth_cookie_name(name))
            })
            .collect();
        let saved = RememberedSession {
            endpoint: endpoint.to_string(),
            cookie_header: selected.join("; "),
            saved_at: now,
            expires_at: now.checked_add(MAX_AGE)?,
        };
        saved.valid().then_some(saved)
    }

    /// Restore into a new native HTTP client and a new host-only cookie jar.
    ///
    /// Rejects non-root HTTP(S) addresses, different normalized origins (including
    /// scheme and port), malformed payloads, future save times and expired snapshots
    /// before any network I/O. Redirects remain disabled. Call `auth()` afterwards;
    /// local expiry does not supersede server expiry, revocation or logout.
    pub fn restore_session(
        base_url: &str,
        saved: RememberedSession,
        now: u64,
    ) -> Result<Self, ClientError> {
        let invalid = || ClientError::InvalidRememberedSession;
        let endpoint = root_endpoint(base_url).ok_or_else(invalid)?;
        if !saved.valid()
            || endpoint.as_str() != saved.endpoint
            || now < saved.saved_at
            || now >= saved.expires_at
        {
            return Err(invalid());
        }
        let pairs = cookie_pairs(&saved.cookie_header).ok_or_else(invalid)?;
        let cookie_jar = Arc::new(reqwest::cookie::Jar::default());
        for (name, value) in pairs {
            // Never interpret the entire Cookie header as one Set-Cookie value,
            // and never restore Domain, Path or arbitrary persisted attributes.
            let secure = if endpoint.scheme() == "https" {
                "; Secure"
            } else {
                ""
            };
            cookie_jar.add_cookie_str(
                &format!("{name}={value}; Path=/; HttpOnly{secure}"),
                &endpoint,
            );
        }
        Ok(Self {
            http: build_http_client(cookie_jar.clone()).map_err(|_| invalid())?,
            base_url: endpoint,
            bearer_token: None,
            cookie_jar,
            realtime_mode: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(
                crate::RemoteTransport::Auto.code(),
            )),
            realtime_epoch: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            realtime_ids: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            realtime_client_id: std::sync::Arc::new(format!(
                "work-client-{}",
                crate::NEXT_REALTIME_CLIENT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            )),
            realtime: std::sync::Arc::new(crate::realtime_client::Hub::default()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    const COOKIE: &str = "rpi_work_desktop_0123456789abcdef";
    const SECOND: &str = "rpi_work_desktop_fedcba9876543210";

    fn run<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn payload(endpoint: &str) -> Value {
        json!({
            "endpoint": endpoint,
            "cookieHeader": format!("{COOKIE}=fixture-token; {SECOND}=second-token"),
            "savedAt": 100,
            "expiresAt": 100 + MAX_AGE,
        })
    }

    fn saved(endpoint: &str) -> RememberedSession {
        serde_json::from_value(payload(endpoint)).unwrap()
    }

    fn read_request(stream: &mut std::net::TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let read = stream.read(&mut chunk).unwrap();
            assert_ne!(read, 0);
            bytes.extend_from_slice(&chunk[..read]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    return String::from_utf8(bytes).unwrap();
                }
            }
        }
    }

    fn respond(stream: &mut std::net::TcpStream, body: &str, headers: &str) {
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}", body.len()).unwrap();
    }

    #[test]
    fn real_login_snapshot_roundtrip_new_transport_and_logout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for step in 0..5 {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_request(&mut stream);
                let cookie = request
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("cookie:"));
                assert!(!request.to_ascii_lowercase().contains("authorization:"));
                match step {
                    0 => {
                        assert!(request.starts_with("POST /api/v1/auth "));
                        assert!(cookie.is_none());
                        respond(
                            &mut stream,
                            r#"{"authenticated":true}"#,
                            &format!(
                                "Set-Cookie: {COOKIE}=fixture-token; Path=/; HttpOnly\r\nSet-Cookie: {SECOND}=second-token; Path=/\r\nSet-Cookie: unrelated=not-persisted; Path=/\r\n"
                            ),
                        );
                    }
                    1 | 2 => {
                        assert!(request.starts_with("GET /api/v1/auth "));
                        let cookie = cookie.unwrap();
                        assert!(cookie.contains(&format!("{COOKIE}=fixture-token")));
                        assert!(cookie.contains(&format!("{SECOND}=second-token")));
                        if step == 2 {
                            assert!(!cookie.contains("unrelated"));
                        }
                        respond(&mut stream, r#"{"required":true,"authenticated":true}"#, "");
                    }
                    3 => {
                        assert!(request.starts_with("DELETE /api/v1/auth "));
                        assert!(cookie.is_some());
                        respond(
                            &mut stream,
                            r#"{"authenticated":false}"#,
                            &format!(
                                "Set-Cookie: {COOKIE}=; Max-Age=0; Path=/\r\nSet-Cookie: {SECOND}=; Max-Age=0; Path=/\r\n"
                            ),
                        );
                    }
                    _ => {
                        assert!(cookie.is_none());
                        respond(
                            &mut stream,
                            r#"{"required":true,"authenticated":false}"#,
                            "",
                        );
                    }
                }
            }
        });
        run(async {
            let client = WorkClient::new(&endpoint).unwrap();
            assert!(client.remembered_session(100).is_none());
            assert!(
                client
                    .login("fixture-password")
                    .await
                    .unwrap()
                    .authenticated
            );
            let snapshot = client.remembered_session(100).unwrap();
            assert_eq!(snapshot.expires_at(), 100 + MAX_AGE);
            let encoded = serde_json::to_string(&snapshot).unwrap();
            assert!(!encoded.contains("fixture-password"));
            assert!(!encoded.contains("unrelated"));
            let decoded = serde_json::from_str(&encoded).unwrap();
            let restored = WorkClient::restore_session(&endpoint, decoded, 101).unwrap();
            assert!(!Arc::ptr_eq(&client.cookie_jar, &restored.cookie_jar));
            drop(client);
            assert!(restored.auth().await.unwrap().authenticated);
            assert!(!restored.logout().await.unwrap().authenticated);
            assert!(restored.remembered_session(102).is_none());
        });
        server.join().unwrap();
    }

    #[test]
    fn invalid_binding_and_expiry_are_rejected_without_io() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/", listener.local_addr().unwrap());
        for target in [
            endpoint.replace("http:", "https:"),
            "http://127.0.0.1:1/".into(),
            endpoint.replace("127.0.0.1", "localhost"),
            format!("{endpoint}api/"),
            format!("{endpoint}a/.."),
            format!("{endpoint}?query"),
            format!("{endpoint}#fragment"),
            endpoint.replace("://", "://user:password@"),
            endpoint.replace("://", "://@"),
            format!(" {endpoint}"),
            endpoint.replace("http:", "ftp:"),
        ] {
            assert!(matches!(
                WorkClient::restore_session(&target, saved(&endpoint), 101),
                Err(ClientError::InvalidRememberedSession)
            ));
        }
        for now in [0, 99, 100 + MAX_AGE, 101 + MAX_AGE, u64::MAX] {
            assert!(WorkClient::restore_session(&endpoint, saved(&endpoint), now).is_err());
        }
        for now in [100, 99 + MAX_AGE] {
            assert!(WorkClient::restore_session(&endpoint, saved(&endpoint), now).is_ok());
        }
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn normalized_root_origin_is_exact_and_default_ports_are_equivalent() {
        let snapshot = saved("http://localhost/");
        let restored = WorkClient::restore_session("HTTP://LOCALHOST:80", snapshot, 100).unwrap();
        assert_eq!(restored.base_url.as_str(), "http://localhost/");
        assert!(
            WorkClient::restore_session("http://localhost:81", saved("http://localhost/"), 100)
                .is_err()
        );
        assert!(
            WorkClient::restore_session("https://localhost:443/", saved("https://localhost/"), 100)
                .is_ok()
        );
    }

    #[test]
    fn malformed_serialized_payloads_are_rejected_without_echoing_secrets() {
        let original = payload("http://localhost/");
        let mut malformed = Vec::new();
        for cookie in [
            String::new(),
            format!("{COOKIE}=secret\r\nInjected: yes"),
            format!("{COOKIE}=secret; Domain=elsewhere"),
            format!("{COOKIE}=secret; Path=/"),
            format!("{COOKIE}=secret; HttpOnly"),
            format!("{COOKIE}=\"secret\""),
            format!("{COOKIE}=secret\\evil"),
            format!("{COOKIE}=secret,evil"),
            format!("{COOKIE}=secret evil"),
            format!("{COOKIE}=secret; {COOKIE}=duplicate"),
            "unrelated=secret".into(),
            format!("{COOKIE}={}", "s".repeat(MAX_COOKIE_HEADER)),
        ] {
            let mut value = original.clone();
            value["cookieHeader"] = json!(cookie);
            malformed.push(value);
        }
        for (key, value) in [
            ("endpoint", json!("http://localhost/path")),
            ("endpoint", json!("http://LOCALHOST:80/")),
            ("savedAt", json!("secret")),
            ("expiresAt", json!(100)),
            ("expiresAt", json!(101 + MAX_AGE)),
            ("cookieHeader", json!(["secret"])),
            ("secret", json!("secret")),
        ] {
            let mut altered = original.clone();
            altered[key] = value;
            malformed.push(altered);
        }
        let mut missing = original.clone();
        missing.as_object_mut().unwrap().remove("savedAt");
        malformed.push(missing);
        for value in malformed {
            let error = serde_json::from_value::<RememberedSession>(value)
                .err()
                .unwrap();
            assert!(!format!("{error:?} {error}").contains("secret"));
        }
        let duplicate = format!(
            "{{\"endpoint\":\"http://localhost/\",\"endpoint\":\"secret\",\"cookieHeader\":\"{COOKIE}=secret\",\"savedAt\":100,\"expiresAt\":101}}"
        );
        let error = serde_json::from_str::<RememberedSession>(&duplicate)
            .err()
            .unwrap();
        assert!(!error.to_string().contains("secret"));
    }

    #[test]
    fn capture_filters_auth_path_and_unrelated_cookies_and_never_bearer() {
        let client =
            WorkClient::new_with_token("http://localhost/", Some("secret-bearer")).unwrap();
        assert!(client.remembered_session(100).is_none());
        client
            .cookie_jar
            .add_cookie_str("unrelated=secret; Path=/", &client.base_url);
        client.cookie_jar.add_cookie_str(
            &format!("{COOKIE}=wrong-path; Path=/other"),
            &client.base_url,
        );
        assert!(client.remembered_session(100).is_none());
        client
            .cookie_jar
            .add_cookie_str(&format!("{SECOND}=valid; Path=/api/v1"), &client.base_url);
        let snapshot = client.remembered_session(100).unwrap();
        let value = serde_json::to_value(snapshot).unwrap();
        assert_eq!(
            value,
            json!({"endpoint":"http://localhost/", "cookieHeader":format!("{SECOND}=valid"), "savedAt":100, "expiresAt":100 + MAX_AGE})
        );
        assert!(client.remembered_session(u64::MAX).is_none());
        let restored =
            WorkClient::restore_session("http://localhost/", saved("http://localhost/"), 100)
                .unwrap();
        assert!(restored.bearer_token.is_none());
        assert!(
            WorkClient::new("http://localhost/prefix/")
                .unwrap()
                .remembered_session(100)
                .is_none()
        );
    }

    #[test]
    fn unexpired_snapshot_does_not_override_service_revocation() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with("GET /api/v1/auth "));
            assert!(request.contains("fixture-token"));
            respond(
                &mut stream,
                r#"{"required":true,"authenticated":false}"#,
                "",
            );
        });
        run(async {
            let client = WorkClient::restore_session(&endpoint, saved(&endpoint), 101).unwrap();
            assert!(!client.auth().await.unwrap().authenticated);
        });
        server.join().unwrap();
    }

    #[test]
    fn restored_auth_never_follows_redirects_or_leaks_cookies() {
        let destination = TcpListener::bind("127.0.0.1:0").unwrap();
        destination.set_nonblocking(true).unwrap();
        let origin = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/", origin.local_addr().unwrap());
        let location = format!("http://{}/api/v1/auth", destination.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = origin.accept().unwrap();
            let request = read_request(&mut stream);
            assert!(request.contains("fixture-token"));
            write!(stream, "HTTP/1.1 307 Temporary Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        run(async {
            let client = WorkClient::restore_session(&endpoint, saved(&endpoint), 101).unwrap();
            assert!(client.auth().await.is_err());
        });
        server.join().unwrap();
        assert_eq!(
            destination.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}
