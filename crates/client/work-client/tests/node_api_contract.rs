#![cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]

//! Exercise the new port against the existing owner using only our RPC fixture.
//! No HTTP listener, installed Agent, external model or user data is accessed.
use futures_util::FutureExt;
use std::{path::PathBuf, sync::Arc, time::Duration};
use work_client::LocalWorkspace;
use work_core::api::{NodeApi, NodeErrorKind};

#[path = "../../../../tests/support/legacy_fixture.rs"]
mod legacy_fixture;
struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "work-node-api-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn node_port_delegates_to_the_same_workspace_without_a_second_owner() {
    let directory = TestDirectory::new();
    let workspace = LocalWorkspace::with_session_dir(
        PathBuf::from(env!("CARGO_BIN_EXE_rpc-fixture")),
        directory.0.join("data"),
        Some(directory.0.join("transcripts")),
    )
    .unwrap();
    let node = workspace.node_owner();
    let other_access = LocalWorkspace::from_node_owner(node.clone());
    assert!(Arc::ptr_eq(&node, &workspace.clone().node_owner()));
    assert!(Arc::ptr_eq(&node, &other_access.node_owner()));
    let result =
        std::panic::AssertUnwindSafe(tokio::time::timeout(Duration::from_secs(15), async {
            let api: &dyn NodeApi = &workspace;
            assert_eq!(
                api.capabilities().await.unwrap(),
                workspace.capabilities().await.unwrap()
            );
            assert_eq!(
                api.projects().await.unwrap(),
                [] as [work_client::Project; 0]
            );
            let project = workspace
                .create_project("fixture project", None, false)
                .await
                .unwrap();
            assert_eq!(api.projects().await.unwrap(), vec![project.clone()]);
            let node_api: &dyn NodeApi = node.as_ref();
            assert_eq!(node_api.projects().await.unwrap(), vec![project.clone()]);
            assert_eq!(
                other_access.projects().await.unwrap(),
                vec![project.clone()]
            );
            assert_eq!(
                api.create_session(Some(&project.id))
                    .await
                    .unwrap_err()
                    .kind,
                NodeErrorKind::Unsupported
            );
            workspace.suspend().await;
            let id = legacy_fixture::seed_legacy(
                &directory.0.join("data"),
                &project.id,
                "legacy-port-fixture",
            );
            // Old sessions remain legacy and still exercise the same owner.
            assert!(api.native_session_resources(&id).await.is_err()); // old host has no resource catalog

            assert_eq!(
                api.sessions().await.unwrap(),
                workspace.sessions().await.unwrap()
            );
            workspace.rename_session(&id, "same owner").await.unwrap();
            let opened = api.open_session(&id).await.unwrap();
            assert_eq!(opened.session_id, id);
            assert_eq!(opened.title.as_deref(), Some("same owner"));
            let current = workspace.get_session(&id).await.unwrap();
            // Opening updates the native registry timestamp asynchronously;
            // compare stable metadata, not two different observation times.
            assert_eq!(opened.session_id, current.session_id);
            assert_eq!(opened.project_id, current.project_id);
            assert_eq!(opened.title, current.title);
            assert_eq!(opened.created_at, current.created_at);
            assert!(current.updated_at >= opened.updated_at);
            let snapshot = api.snapshot(&id).await.unwrap();
            assert_eq!(snapshot.session_id, id);
            assert_eq!(snapshot, workspace.snapshot(&id).await.unwrap());
            assert_eq!(
                api.open_session("missing").await.unwrap_err().kind,
                NodeErrorKind::NotFound
            );
            assert_eq!(
                api.snapshot("missing").await.unwrap_err().kind,
                NodeErrorKind::NotFound
            );
        }))
        .catch_unwind()
        .await;
    // Always stop fixture children before deleting the isolated data directory,
    // even when an assertion or timeout fails.
    workspace.shutdown().await;
    assert!(matches!(
        other_access.projects().await,
        Err(work_client::ClientError::Local(
            work_client::LocalError::Shutdown
        ))
    ));
    assert_eq!(
        NodeApi::projects(node.as_ref()).await.unwrap_err().kind,
        NodeErrorKind::Unavailable
    );
    let api: &dyn NodeApi = &workspace;
    assert_eq!(
        api.sessions().await.unwrap_err().kind,
        NodeErrorKind::Unavailable
    );
    assert_eq!(
        api.projects().await.unwrap_err().kind,
        NodeErrorKind::Unavailable
    );
    assert_eq!(
        api.snapshot("closed").await.unwrap_err().kind,
        NodeErrorKind::Unavailable
    );
    match result {
        Ok(result) => result.expect("isolated node service port completes"),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
