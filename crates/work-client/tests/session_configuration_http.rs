#![cfg(not(target_arch = "wasm32"))]
//! A legacy/missing endpoint is an explicit failure, never optimistic support.
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    time::Duration,
};
use work_client::{ClientError, WorkspaceClient};

#[tokio::test]
async fn unsupported_or_unauthenticated_http_configuration_never_falls_back() {
    for status in [401, 404] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for expected_method in ["GET", "POST"] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let n = stream.read(&mut buffer).unwrap();
                    assert_ne!(n, 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8_lossy(&bytes);
                assert!(request.starts_with(&format!(
                    "{expected_method} /api/v1/sessions/s/configuration "
                )));
                let body = "{\"error\":\"fixture rejection\"}";
                write!(stream, "HTTP/1.1 {status} Rejected\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                stream.flush().unwrap();
            }
        });
        let client = WorkspaceClient::new(&format!("http://{address}/")).unwrap();
        assert!(
            matches!(client.session_configuration("s").await, Err(ClientError::Api { status: actual, message }) if actual.as_u16() == status && message == "{\"error\":\"fixture rejection\"}")
        );
        assert!(
            matches!(client.set_session_config_option("s", "model", json!("m")).await, Err(ClientError::Api { status: actual, message }) if actual.as_u16() == status && message == "{\"error\":\"fixture rejection\"}")
        );
        server.join().unwrap();
    }
}
