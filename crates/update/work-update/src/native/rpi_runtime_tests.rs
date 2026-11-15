use super::*;
use sha2::{Digest, Sha256};
use std::{
    io::Cursor,
    net::{TcpListener, TcpStream},
    sync::mpsc,
    time::Duration,
};

fn archive() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(runtime_filename(), zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"verified fixture executable").unwrap();
    zip.finish().unwrap().into_inner()
}

struct Server {
    address: String,
    stop: mpsc::Sender<()>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn new(archive: Vec<u8>, checksum: String, slow: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let (stop, stopped) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            loop {
                if stopped.try_recv().is_ok() {
                    return;
                }
                let (mut socket, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if stopped.recv_timeout(Duration::from_millis(5)).is_ok() {
                            return;
                        }
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = [0; 4096];
                let length = socket.read(&mut request).unwrap_or(0);
                let request = String::from_utf8_lossy(&request[..length]);
                let body = if request.contains("GET /archive ") {
                    archive.as_slice()
                } else {
                    checksum.as_bytes()
                };
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                if slow {
                    let _ = socket.write_all(&body[..1]);
                    let _ = socket.flush();
                    let _ = stopped.recv_timeout(Duration::from_secs(5));
                    return;
                }
                let _ = socket.write_all(body);
            }
        });
        Self {
            address,
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        // Also wake a pending accept on platforms without useful polling.
        let _ = TcpStream::connect(self.address.trim_start_matches("http://"));
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

fn release(server: &Server, archive: &[u8]) -> RuntimeRelease {
    RuntimeRelease {
        version: "1.2.3".into(),
        target: "x86_64-pc-windows-msvc".into(),
        archive_name: "rpi-fixture.zip".into(),
        archive_size: archive.len() as u64,
        archive_sha256: format!("{:x}", Sha256::digest(archive)),
        archive_url: format!("{}/archive", server.address),
        checksum_url: format!("{}/checksum", server.address),
    }
}

fn no_staging(directory: &Path) -> bool {
    fs::read_dir(directory).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".rpi-")
    })
}

#[tokio::test]
async fn verified_preparation_keeps_old_binary_until_atomic_activation() {
    let root = tempfile::tempdir().unwrap();
    let final_binary = root.path().join(runtime_filename());
    fs::write(&final_binary, b"existing runtime").unwrap();
    let archive = archive();
    let server = Server::new(
        archive.clone(),
        format!("{:x}  fixture.zip", Sha256::digest(&archive)),
        false,
    );
    let metadata = release(&server, &archive);
    let mut events = Vec::new();
    let prepared = prepare_archive(
        metadata.clone(),
        root.path().into(),
        lock_install(root.path()).unwrap(),
        reqwest::Client::new(),
        |event| events.push(event),
    )
    .await
    .unwrap();
    assert_eq!(fs::read(&final_binary).unwrap(), b"existing runtime");
    assert_eq!(
        fs::read(prepared.executable()).unwrap(),
        b"verified fixture executable"
    );
    assert!(
        lock_install(root.path()).is_err(),
        "preparation owns the installer lock through validation"
    );
    drop(prepared);
    assert_eq!(fs::read(&final_binary).unwrap(), b"existing runtime");
    assert!(
        no_staging(root.path()),
        "discarding preparation removes all staging files"
    );
    assert!(events.contains(&RuntimeInstallProgress::Verifying));
    assert!(events.contains(&RuntimeInstallProgress::Extracting));
    let prepared = prepare_archive(
        metadata,
        root.path().into(),
        lock_install(root.path()).unwrap(),
        reqwest::Client::new(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(prepared.activate().unwrap(), final_binary);
    assert_eq!(
        fs::read(&final_binary).unwrap(),
        b"verified fixture executable"
    );
    assert!(no_staging(root.path()));
    assert!(lock_install(root.path()).is_ok());
}

#[tokio::test]
async fn invalid_digest_and_checksum_leave_existing_runtime_and_release_lock() {
    let root = tempfile::tempdir().unwrap();
    let final_binary = root.path().join(runtime_filename());
    fs::write(&final_binary, b"existing runtime").unwrap();
    let archive = archive();
    let server = Server::new(archive.clone(), "0".repeat(64), false);
    let mut metadata = release(&server, &archive);
    metadata.archive_sha256 = "1".repeat(64);
    let result = prepare_archive(
        metadata,
        root.path().into(),
        lock_install(root.path()).unwrap(),
        reqwest::Client::new(),
        |_| {},
    )
    .await;
    assert!(result.err().unwrap().message.contains("SHA-256"));
    let result = prepare_archive(
        release(&server, &archive),
        root.path().into(),
        lock_install(root.path()).unwrap(),
        reqwest::Client::new(),
        |_| {},
    )
    .await;
    assert!(result.err().unwrap().message.contains("校验文件"));
    assert_eq!(fs::read(final_binary).unwrap(), b"existing runtime");
    assert!(no_staging(root.path()));
    assert!(lock_install(root.path()).is_ok());
}

#[tokio::test]
async fn interrupted_download_cleans_staging_and_can_be_retried() {
    let root = tempfile::tempdir().unwrap();
    let archive = archive();
    let server = Server::new(archive.clone(), String::new(), true);
    let directory = root.path().to_owned();
    let metadata = release(&server, &archive);
    let lock = lock_install(root.path()).unwrap();
    let (ready, mut received) = tokio::sync::mpsc::unbounded_channel();
    let task = tokio::spawn(prepare_archive(
        metadata,
        directory,
        lock,
        reqwest::Client::new(),
        move |event| {
            if matches!(event, RuntimeInstallProgress::Downloading { received, .. } if received > 0)
            {
                let _ = ready.send(());
            }
        },
    ));
    tokio::time::timeout(Duration::from_secs(3), received.recv())
        .await
        .unwrap()
        .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(no_staging(root.path()));
    assert!(lock_install(root.path()).is_ok());
    assert!(!root.path().join(runtime_filename()).exists());
}

#[test]
fn extraction_rejects_escaping_paths_and_never_writes_outside_staging() {
    let root = tempfile::tempdir().unwrap();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(
        format!("../{}", runtime_filename()),
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(b"invalid").unwrap();
    let archive = zip.finish().unwrap().into_inner();
    let server = Server::new(Vec::new(), String::new(), false);
    let metadata = release(&server, &archive);
    let path = root.path().join("archive.zip");
    fs::write(&path, archive).unwrap();
    let mut output = tempfile::tempfile().unwrap();
    assert!(
        extract_binary(&path, &mut output, &metadata)
            .unwrap_err()
            .contains("路径")
    );
    assert!(!root.path().join(runtime_filename()).exists());
}
