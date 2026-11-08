//! Opt-in local acceptance host. Never resolves PATH or starts ACP agents.
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    path::PathBuf,
    sync::Arc,
};
use work_node_runtime::workspace::LocalWorkspace;
use work_server::{HttpServer, ServerConfig};

fn isolated_inputs() -> (PathBuf, PathBuf) {
    let root = PathBuf::from(
        std::env::var_os("WORK_CDE_TEST_ROOT").expect("explicit isolated root required"),
    );
    let native = PathBuf::from(
        std::env::var_os("RPI_RESOURCE_NATIVE_BIN").expect("explicit trusted executable required"),
    );
    assert!(root.is_absolute() && native.is_absolute() && native.is_file());
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&native).unwrap())),
        "77a9f89d61aadab02f9799ac6cd69fb194258f7369f195d553e8733f29ded4dc"
    );
    let agent = PathBuf::from(
        std::env::var_os("RPI_CODING_AGENT_DIR").expect("isolated agent root required"),
    );
    assert!(agent.starts_with(&root));
    (root, native)
}
async fn serve() {
    let (root, native) = isolated_inputs();
    let owner = Arc::new(
        LocalWorkspace::with_session_dir(native, root.join("node"), Some(root.join("sessions")))
            .unwrap(),
    );
    std::fs::create_dir_all(root.join("project")).unwrap();
    let project = owner
        .create_project("CDE acceptance", root.join("project").to_str(), false)
        .await
        .unwrap();
    let assets = std::env::var_os("WORK_CDE_ASSETS").map(PathBuf::from);
    let mut server = HttpServer::start(
        owner.clone(),
        ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            password: "loopback-cde-only".into(),
            allowed_hosts: vec![],
            node_id: "isolated-cde-node".into(),
            assets,
            web_proxy: false,
            allow_sharing: true,
        },
    )
    .unwrap();
    println!(
        "{}",
        serde_json::json!({"url":format!("http://{}",server.address()),"projectId":project.id,"project":project.path})
    );
    io::stdout().flush().unwrap();
    let (done, wait) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = io::stdin().read_line(&mut line);
        let _ = done.send(());
    });
    let _ = wait.await;
    server.drain().await.unwrap();
    owner.shutdown().await;
}
#[tokio::main(flavor = "current_thread")]
async fn main() {
    tokio::task::LocalSet::new().run_until(serve()).await;
}
