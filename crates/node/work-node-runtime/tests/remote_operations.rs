#![cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
//! No Agent process is started: owner construction and receipt bookkeeping only.
use serde_json::{Value, json};
use work_core::api::realtime::{OperationOutcome, OperationStatus, RealtimeOperationKind};
use work_node_runtime::workspace::{
    LocalWorkspace,
    operations::{OperationClaim, OperationError},
};

#[tokio::test]
async fn owner_clones_share_receipts_and_shutdown_closes_only_new_dispatch() {
    let root = tempfile::tempdir().unwrap();
    let owner = LocalWorkspace::with_session_dir(
        root.path().join("must-not-launch-agent"),
        root.path().join("data"),
        Some(root.path().join("transcripts")),
    )
    .unwrap();
    let clone = owner.clone();
    assert_eq!(
        owner.remote_operations().owner_epoch(),
        clone.remote_operations().owner_epoch()
    );
    let reference = owner
        .remote_operations()
        .reserve(
            "authenticated-principal",
            "client",
            "opaque-session",
            RealtimeOperationKind::SetConfiguration,
        )
        .unwrap();
    let permit = match clone
        .remote_operations()
        .claim(
            "authenticated-principal",
            &reference,
            json!({"id":"opaque-acp-config","value":"mode"}),
        )
        .unwrap()
    {
        OperationClaim::Execute(permit) => permit,
        OperationClaim::Existing(_) => panic!("first claim must get the permit"),
    };
    assert_eq!(permit.kind(), RealtimeOperationKind::SetConfiguration);
    owner.shutdown().await;
    assert_eq!(
        clone.remote_operations().reserve(
            "authenticated-principal",
            "client",
            "s",
            RealtimeOperationKind::SessionCommand,
        ),
        Err(OperationError::Unavailable)
    );
    let known = permit
        .finish(OperationOutcome::Succeeded { value: Value::Null })
        .unwrap();
    assert!(matches!(known, OperationStatus::KnownResult { .. }));
    assert_eq!(
        owner
            .remote_operations()
            .status("authenticated-principal", &reference)
            .unwrap(),
        known
    );
    assert!(matches!(
        owner
            .remote_operations()
            .claim(
                "authenticated-principal",
                &reference,
                json!({"value":"mode","id":"opaque-acp-config"}),
            )
            .unwrap(),
        OperationClaim::Existing(_)
    ));
}
