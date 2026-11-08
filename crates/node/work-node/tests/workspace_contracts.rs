#![cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
//! The workspace owner can serve `NodeApi` without linking the client or a listener.
use std::path::PathBuf;
use work_core::api::{NodeApi, NodeErrorKind};
use work_node::workspace::{LocalError, LocalWorkspace, WorkspaceError};

#[tokio::test]
async fn node_workspace_is_an_independent_lazy_service_owner() {
    let root = tempfile::tempdir().unwrap();
    let node =
        LocalWorkspace::new(PathBuf::from("unused-fixture"), root.path().join("data")).unwrap();
    let same = node.clone();
    let api: &dyn NodeApi = &node;
    assert_eq!(api.projects().await.unwrap(), vec![]);
    let project = node.create_project("fixture", None, false).await.unwrap();
    assert_eq!(same.projects().await.unwrap(), vec![project.clone()]);
    assert_eq!(api.projects().await.unwrap(), vec![project]);
    node.suspend().await;
    assert_eq!(same.projects().await.unwrap().len(), 1);
    // Construction cannot acquire a lifecycle lease or observe the old state.
    let pending = api.projects();
    same.shutdown().await;
    assert_eq!(pending.await.unwrap_err().kind, NodeErrorKind::Unavailable);
    assert!(matches!(
        node.projects().await,
        Err(WorkspaceError::Local(LocalError::Shutdown))
    ));
}
