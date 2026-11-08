//! Real owner + authenticated HTTP image contract; no agent process is started.
use std::{path::PathBuf, sync::Arc};
use work_client::{
    ClientError, LocalWorkspace, PROJECT_IMAGE_MAX_BYTES, WorkClient, WorkspaceClient,
};
use work_server::{HttpServer, ServerConfig};

fn status(error: ClientError) -> u16 {
    match error {
        ClientError::Api { status, .. } => status.as_u16(),
        error => panic!("expected HTTP error, got {error:?}"),
    }
}

#[tokio::test]
async fn project_images_preserve_auth_scope_and_binary_text_separation() {
    let root = tempfile::tempdir().unwrap();
    let local = Arc::new(
        LocalWorkspace::with_session_dir(
            PathBuf::from(env!("CARGO_BIN_EXE_work-server-rpc-fixture")),
            root.path().join("data"),
            Some(root.path().join("sessions")),
        )
        .unwrap(),
    );
    let project_root = root.path().join("project");
    std::fs::create_dir(&project_root).unwrap();
    let project = local
        .create_project("images", Some(project_root.to_str().unwrap()), false)
        .await
        .unwrap();
    let name = "image # & space.PNG";
    let png = b"\x89PNG\r\n\x1a\n\0binary\xff";
    std::fs::write(project_root.join(name), png).unwrap();
    std::fs::write(project_root.join("notes.txt"), "plain UTF-8 text").unwrap();
    std::fs::write(project_root.join("fake.png"), "not an image").unwrap();
    std::fs::write(project_root.join("active.svg"), "<svg/>").unwrap();
    std::fs::write(root.path().join("outside.png"), png).unwrap();
    let oversized = std::fs::File::create(project_root.join("large.png")).unwrap();
    oversized
        .set_len(PROJECT_IMAGE_MAX_BYTES as u64 + 1)
        .unwrap();
    drop(oversized);
    let mut server = HttpServer::start(
        local.node_owner(),
        ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            password: "image-password".into(),
            allowed_hosts: vec![],
            node_id: "image-test".into(),
            assets: None,
            web_proxy: false,
            allow_sharing: true,
        },
    )
    .unwrap();
    let base = format!("http://{}/", server.address());
    let http = WorkClient::new(&base).unwrap();
    assert_eq!(
        status(http.image(&project.id, name).await.unwrap_err()),
        401
    );
    assert!(http.login("image-password").await.unwrap().authenticated);
    let remote = WorkspaceClient::from(http.clone());
    let native = WorkspaceClient::Local(local.clone());
    let image = remote.image(&project.id, name).await.unwrap();
    assert_eq!(image.bytes, png);
    assert_eq!(image.media_type, "image/png");
    assert_eq!(native.image(&project.id, name).await.unwrap(), image);
    // Exercise the neutral port adapters as well as the UI-facing facade.
    assert_eq!(
        work_core::api::NodeApi::image(&remote, &project.id, name)
            .await
            .unwrap(),
        image
    );
    assert_eq!(
        work_core::api::NodeApi::image(&native, &project.id, name)
            .await
            .unwrap(),
        image
    );
    assert_eq!(
        work_core::api::NodeApi::image(&http, &project.id, name)
            .await
            .unwrap(),
        image
    );
    assert_eq!(status(http.file(&project.id, name).await.unwrap_err()), 415);
    assert_eq!(
        http.file(&project.id, "notes.txt").await.unwrap().content,
        "plain UTF-8 text"
    );
    for path in ["notes.txt", "fake.png", "active.svg"] {
        assert_eq!(
            status(http.image(&project.id, path).await.unwrap_err()),
            415,
            "{path}"
        );
    }
    assert_eq!(
        status(http.image(&project.id, "large.png").await.unwrap_err()),
        413
    );
    assert_eq!(
        status(http.image(&project.id, "missing.png").await.unwrap_err()),
        404
    );
    assert_eq!(
        status(http.image("unknown-project", name).await.unwrap_err()),
        404
    );
    assert_eq!(
        status(http.image(&project.id, "../outside.png").await.unwrap_err()),
        403
    );
    assert_eq!(
        status(
            http.image(
                &project.id,
                root.path().join("outside.png").to_str().unwrap()
            )
            .await
            .unwrap_err()
        ),
        // Windows drive prefixes/backslashes are invalid input before scope checking.
        if cfg!(windows) { 400 } else { 403 }
    );

    // Raw authenticated response is binary (not JSON/base64), with defensive headers.
    let raw = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap();
    raw.post(format!("{base}api/v1/auth"))
        .header("x-rpi-web", "1")
        .json(&serde_json::json!({"password":"image-password"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let response = raw
        .get(format!("{base}api/v1/projects/{}/image", project.id))
        .query(&[("path", name)])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "image/png");
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(response.bytes().await.unwrap().as_ref(), png);
    server.drain().await.unwrap();
    local.shutdown().await;
}
