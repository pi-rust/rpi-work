#![cfg(not(target_arch = "wasm32"))]
//! Untrusted HTTP image responses, including chunked responses without a length.
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};
use work_client::{ClientError, PROJECT_IMAGE_MAX_BYTES, WorkClient};

async fn fetch(headers: &str, body: Vec<u8>) -> Result<work_client::ProjectImage, ClientError> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let headers = headers.to_owned();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut buffer).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&buffer[..n]);
            assert!(request.len() < 16384);
        }
        let request = String::from_utf8(request).unwrap();
        let first = request.lines().next().unwrap();
        assert!(
            first.starts_with("GET /api/v1/projects/project%2Fid/image?path="),
            "{first}"
        );
        assert!(first.contains("path=dir%2Fimage+%23%26.png"), "{first}");
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-token\r\n")
        );
        assert!(request.to_ascii_lowercase().contains("x-rpi-web: 1\r\n"));
        // Early rejection deliberately closes the socket before the whole body is sent.
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nConnection: close\r\n{headers}\r\n"
        );
        let _ = stream.write_all(&body);
    });
    let result = WorkClient::new_with_token(&format!("http://{address}/"), Some("fixture-token"))
        .unwrap()
        .image("project/id", "dir/image #&.png")
        .await;
    worker.join().unwrap();
    result
}

fn assert_status(result: Result<work_client::ProjectImage, ClientError>, expected: u16) {
    match result.unwrap_err() {
        ClientError::Api { status, .. } => assert_eq!(status.as_u16(), expected),
        error => panic!("expected HTTP status {expected}, got {error:?}"),
    }
}

#[tokio::test]
async fn image_mime_and_magic_are_both_validated() {
    for (mime, bytes) in [
        (
            "Image/PNG; charset=binary",
            b"\x89PNG\r\n\x1a\n\0".as_slice(),
        ),
        ("image/jpeg", b"\xff\xd8\xff\0".as_slice()),
        ("image/gif", b"GIF87a\0".as_slice()),
        ("image/gif", b"GIF89a\0".as_slice()),
        ("image/webp", b"RIFF\0\0\0\0WEBP".as_slice()),
        ("image/bmp", b"BM\0".as_slice()),
    ] {
        let image = fetch(
            &format!(
                "Content-Type: {mime}\r\nContent-Length: {}\r\n",
                bytes.len()
            ),
            bytes.to_vec(),
        )
        .await
        .unwrap();
        assert_eq!(image.bytes, bytes);
        assert_eq!(
            image.media_type,
            mime.split(';').next().unwrap().to_ascii_lowercase()
        );
    }
    for mime in [
        "image/svg+xml",
        "text/html",
        "application/json",
        "image/avif",
    ] {
        assert_status(
            fetch(
                &format!("Content-Type: {mime}\r\nContent-Length: 8\r\n"),
                b"\x89PNG\r\n\x1a\n".to_vec(),
            )
            .await,
            415,
        );
    }
    assert_status(fetch("Content-Length: 0\r\n", vec![]).await, 415);
    for bytes in [vec![], b"<html>not PNG</html>".to_vec(), b"GIF89a".to_vec()] {
        assert_status(
            fetch(
                &format!(
                    "Content-Type: image/png\r\nContent-Length: {}\r\n",
                    bytes.len()
                ),
                bytes,
            )
            .await,
            415,
        );
    }
}

#[tokio::test]
async fn image_limit_applies_to_declared_and_chunked_lengths() {
    assert_status(
        fetch(
            &format!(
                "Content-Type: image/png\r\nContent-Length: {}\r\n",
                PROJECT_IMAGE_MAX_BYTES + 1
            ),
            vec![],
        )
        .await,
        413,
    );
    // A valid signature at exactly the limit is accepted; one extra streamed byte is not.
    for length in [PROJECT_IMAGE_MAX_BYTES, PROJECT_IMAGE_MAX_BYTES + 1] {
        let mut bytes = vec![0; length];
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        let mut chunked = format!("{length:x}\r\n").into_bytes();
        chunked.extend_from_slice(&bytes);
        chunked.extend_from_slice(b"\r\n0\r\n\r\n");
        let result = fetch(
            "Content-Type: image/png\r\nTransfer-Encoding: chunked\r\n",
            chunked,
        )
        .await;
        if length == PROJECT_IMAGE_MAX_BYTES {
            assert_eq!(result.unwrap().bytes, bytes);
        } else {
            assert_status(result, 413);
        }
    }
}
