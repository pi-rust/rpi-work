#![cfg(not(target_arch = "wasm32"))]
//! Isolated HTTP fixture: typed endpoints only, no raw command fallback.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    time::Duration,
};
use work_client::{ClientError, WorkspaceClient};

#[tokio::test]
async fn access_and_dialogs_use_typed_http_without_fallback() {
    for status in [200, 401, 404] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for (method, path, expected) in [
                ("GET", "access", None),
                (
                    "POST",
                    "dialogs",
                    Some(json!({"requestId":"s:7", "answer":{"outcome":"cancelled"}})),
                ),
                (
                    "POST",
                    "dialogs",
                    Some(json!({"requestId":"s:7", "cancelled":true})),
                ),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                let (header_end, length) = loop {
                    let n = stream.read(&mut buffer).unwrap();
                    assert_ne!(n, 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&bytes[..end]);
                        assert!(
                            header.starts_with(&format!("{method} /api/v1/sessions/s/{path} "))
                        );
                        let length = header
                            .lines()
                            .find_map(|line| {
                                let (key, value) = line.split_once(':')?;
                                key.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        break (end + 4, length);
                    }
                };
                while bytes.len() < header_end + length {
                    let n = stream.read(&mut buffer).unwrap();
                    assert_ne!(n, 0);
                    bytes.extend_from_slice(&buffer[..n]);
                }
                if let Some(expected) = expected {
                    assert_eq!(
                        serde_json::from_slice::<Value>(&bytes[header_end..header_end + length])
                            .unwrap(),
                        expected
                    );
                }
                // Success access is tested with the actual owner in the ACP fixture.
                let response_status = if path == "access" && status == 200 {
                    404
                } else {
                    status
                };
                let body = if response_status == 200 {
                    "{\"accepted\":true}"
                } else {
                    "{\"error\":\"fixture rejection\"}"
                };
                write!(stream, "HTTP/1.1 {response_status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                stream.flush().unwrap();
            }
        });
        let client = WorkspaceClient::new(&format!("http://{address}/")).unwrap();
        assert!(matches!(
            client.session_access("s").await,
            Err(ClientError::Api { .. })
        ));
        for result in [
            client
                .dialog_response("s", "s:7", json!({"outcome":"cancelled"}))
                .await,
            client.cancel_dialog("s", "s:7").await,
        ] {
            if status == 200 {
                assert_eq!(result.unwrap(), json!({"accepted":true}));
            } else {
                assert!(
                    matches!(result, Err(ClientError::Api { status: actual, .. }) if actual.as_u16() == status)
                );
            }
        }
        server.join().unwrap();
    }
}
