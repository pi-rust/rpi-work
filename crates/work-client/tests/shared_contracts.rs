//! Existing import paths must name the exact shared type, not a copied DTO.
#[test]
fn original_client_exports_are_the_core_contract_types() {
    macro_rules! same_type {
        ($($name:ident),+ $(,)?) => {
            $(let _: fn(work_client::$name) -> work_core::api::$name = std::convert::identity;)+
        };
    }
    same_type!(
        AgentBackendOption,
        AuthState,
        AuthStatus,
        AuthStatusProvider,
        Capabilities,
        ChangedFile,
        CompactionEntry,
        Diff,
        FileContent,
        FileEntry,
        FileListing,
        GitStatus,
        Health,
        History,
        ModelDescriptor,
        PendingDialog,
        Project,
        ProjectCreation,
        ProjectFolder,
        ProjectFolders,
        QrAuthorization,
        QueueCancellation,
        QueueItem,
        QueueMutation,
        QueueSnapshot,
        RuntimeState,
        SessionAgentBinding,
        SessionBranch,
        SessionMutation,
        SessionPreferences,
        SessionSummary,
        Snapshot,
        SnapshotEvent,
        ToolCatalog,
        UiDialogOption,
        UiDialogPayload,
        WireEvent,
    );
    let _: fn(
        work_client::HostResponse<serde_json::Value>,
    ) -> work_core::api::HostResponse<serde_json::Value> = std::convert::identity;
}

#[test]
fn neutral_core_errors_preserve_the_client_boundary_categories() {
    use serde_json::json;
    use work_client::{ClientError, PendingDialog, WireEvent};
    use work_core::api::events::WireEventProjection as _;

    fn decode(wire: WireEvent) -> Result<work_core::Event, ClientError> {
        // The existing '?' style still converts the new neutral error.
        Ok(wire.into_core_event()?)
    }

    let dialog: PendingDialog = serde_json::from_value(json!({
        "requestId":"r", "ui":{"question":"Permission?"}
    }))
    .unwrap();
    let error: ClientError = dialog.acp_permission_result(None).unwrap_err().into();
    assert!(matches!(&error, ClientError::Acp(message)
        if message == "invalid or ambiguous ACP permission selection"));
    assert_eq!(
        error.to_string(),
        "ACP transport error: invalid or ambiguous ACP permission selection"
    );

    let wire = WireEvent {
        id: None,
        seq: None,
        event_type: "tool_execution_start".into(),
        data: json!({}),
    };
    assert!(matches!(decode(wire), Err(ClientError::Sse(message))
        if message == "event is missing toolCallId"));
    assert!(
        matches!(work_client::normalize_event("tool_execution_end", &json!({})),
        Err(ClientError::Sse(message)) if message == "event is missing toolCallId")
    );
    let malformed = WireEvent {
        id: None,
        seq: None,
        event_type: "ui_dialog_request".into(),
        data: json!({"requestId":"r","ui":[]}),
    };
    assert!(matches!(
        work_client::dialog_from_wire(&malformed),
        Err(ClientError::Json(_))
    ));
}
