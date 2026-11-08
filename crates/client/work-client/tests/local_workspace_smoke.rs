#![cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]

use futures_util::FutureExt;
use std::path::PathBuf;
use std::sync::Arc;
use work_client::{LocalWorkspace, WorkspaceClient};

struct Smoke {
    root: PathBuf,
    client: Option<Arc<LocalWorkspace>>,
}
impl Smoke {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "work-local-smoke-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self {
            root: std::fs::canonicalize(root).unwrap(),
            client: None,
        }
    }
    fn start(&mut self, executable: PathBuf) -> Arc<LocalWorkspace> {
        let client = Arc::new(
            LocalWorkspace::with_session_dir(
                executable,
                self.root.join("data"),
                Some(self.root.join("transcripts")),
            )
            .unwrap(),
        );
        self.client = Some(client.clone());
        client
    }
}
impl Drop for Smoke {
    fn drop(&mut self) {
        let root = self.root.clone();
        if let Some(client) = self.client.take() {
            // Also clean up after assertion failures without deleting files while
            // an extcore host is still writing them.
            tokio::spawn(async move {
                client.shutdown().await;
                let _ = std::fs::remove_dir_all(root);
            });
        } else {
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

async fn verify_suspend_and_branches(facade: &WorkspaceClient, id: &str, control: &str) {
    // Switching to a remote workspace stops children, but returning to local
    // must reuse the same facade and restore the exact saved session.
    facade.suspend().await;
    let restored = facade.open_session(id).await.unwrap();
    assert_eq!(restored.session_id, id);
    assert_eq!(restored.title.as_deref(), Some("Persistent title"));
    assert!(restored.pinned && restored.archived);
    facade.tools(id).await.unwrap();
    assert_eq!(facade.control_session().await.unwrap(), control);
    assert_eq!(facade.sessions().await.unwrap().len(), 1);
    let fork = facade
        .fork_session(id, None, Some("Fork smoke"))
        .await
        .unwrap();
    let cloned = facade.clone_session(id, Some("Clone smoke")).await.unwrap();
    assert_ne!(fork.session_id, id);
    assert_ne!(cloned.session_id, id);
    assert_ne!(fork.session_id, cloned.session_id);
    facade.delete_session(&fork.session_id).await.unwrap();
    facade.delete_session(&cloned.session_id).await.unwrap();
    assert_eq!(facade.sessions().await.unwrap().len(), 1);
}

/// No prompt, model request, HTTP server, global session directory, or user project.
#[tokio::test]
#[ignore = "requires installed rpi; set RPI_WORK_SMOKE_BIN to an explicit executable"]
async fn actual_rpi_local_workspace_restart_without_http_or_model() {
    let executable =
        std::env::var_os("RPI_WORK_SMOKE_BIN").map_or_else(|| PathBuf::from("rpi"), PathBuf::from);
    let version = std::process::Command::new(&executable)
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    eprintln!(
        "actual native binary: {}: {}",
        executable.display(),
        String::from_utf8_lossy(&version.stdout).trim()
    );
    let mut smoke = Smoke::new();
    let client = smoke.start(executable.clone());
    let result = std::panic::AssertUnwindSafe(tokio::time::timeout(
        std::time::Duration::from_secs(60),
        async {
            let facade = WorkspaceClient::Local(client.clone());
            assert!(facade.is_local());
            assert_eq!(facade.descriptor().identity.transport, "native");
            let project = client
                .create_project_directory("Outside data root", &smoke.root.join("project"), true)
                .await
                .unwrap();
            std::fs::write(smoke.root.join("project/hello.txt"), "persistent file\n").unwrap();
            let reopened = client
                .create_project("Same canonical root", Some(&project.path), false)
                .await
                .unwrap();
            assert_eq!(project.id, reopened.id);
            let id = client.create_session(Some(&project.id)).await.unwrap();
            client
                .rename_session(&id, "Persistent title")
                .await
                .unwrap();
            client
                .session_preferences(&id, Some(true), Some(true))
                .await
                .unwrap();
            let summary = client.get_session(&id).await.unwrap();
            assert_eq!(summary.project_id.as_deref(), Some(project.id.as_str()));
            assert!(summary.pinned && summary.archived);
            let state: serde_json::Value = client
                .command_data(&id, "get_state", serde_json::Value::Null)
                .await
                .unwrap();
            eprintln!("native state: {state}");
            client.tools(&id).await.unwrap();
            let connection = facade.connect_session(&id).await.unwrap();
            assert_eq!(connection.session_id(), id);
            drop(connection);
            client.file(&project.id, "hello.txt").await.unwrap();
            client.git_status(&project.id).await.unwrap();
            let control = client.control_session().await.unwrap();
            assert_ne!(control, id);
            assert_eq!(client.projects().await.unwrap().len(), 1);
            assert_eq!(
                client.sessions().await.unwrap().len(),
                1,
                "control context must remain hidden"
            );
            verify_suspend_and_branches(&facade, &id, &control).await;
            client.shutdown().await;
            assert!(
                client.open_session(&id).await.is_err(),
                "closed facade must not spawn again"
            );
            let restarted = smoke.start(executable);
            let summary = restarted.open_session(&id).await.unwrap();
            assert_eq!(summary.session_id, id);
            assert_eq!(summary.title.as_deref(), Some("Persistent title"));
            assert_eq!(summary.project_id.as_deref(), Some(project.id.as_str()));
            assert!(summary.pinned && summary.archived);
            restarted.runtime_state(&id).await.unwrap();
            restarted.tools(&id).await.unwrap();
            restarted.file(&project.id, "hello.txt").await.unwrap();
            restarted.git_status(&project.id).await.unwrap();
            assert_eq!(
                std::fs::read_to_string(smoke.root.join("project/hello.txt")).unwrap(),
                "persistent file\n"
            );
            assert!(smoke.root.join("project/.git").is_dir());
            assert!(
                std::fs::read_dir(smoke.root.join("transcripts"))
                    .unwrap()
                    .next()
                    .is_some()
            );
        },
    ))
    .catch_unwind()
    .await;
    client.shutdown().await;
    if let Some(client) = smoke.client.take() {
        client.shutdown().await;
    }
    match result {
        Ok(result) => result.expect("bounded native smoke"),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
