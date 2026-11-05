//! These tests need no client, executor, filesystem, Agent or model.
use serde_json::{Value, json};
use work_core::api::{
    PendingDialog, Snapshot, WireEvent,
    events::{dialog_from_wire, normalize_event},
};
use work_core::{ConnectionState, EventKind, OutputMode, SettlementOutcome};

fn wire(kind: &str, data: Value) -> WireEvent {
    WireEvent {
        id: None,
        seq: None,
        event_type: kind.into(),
        data,
    }
}

// Accept owned JSON so fixture calls can build each independent case inline.
#[allow(clippy::needless_pass_by_value)]
fn permission(options: Value, response_id: Value) -> PendingDialog {
    dialog_from_wire(&wire(
        "ui_dialog_request",
        json!({
            "requestId":"ui-id", "jsonRpcRequestId":response_id,
            "ui":{"purpose":"permission","acpPermission":{"options":options}}
        }),
    ))
    .unwrap()
    .unwrap()
}

fn options() -> Value {
    json!([
        {"optionId":"deny-7","name":"No","kind":"reject_once"},
        {"optionId":"allow-9","name":"Yes","kind":"allow_once"}
    ])
}

#[test]
fn snapshot_sparse_defaults_and_legacy_alias_keep_the_original_wire_shape() {
    let snapshot: Snapshot =
        serde_json::from_value(json!({"sessionId":"s","has_more":true})).unwrap();
    assert_eq!(
        serde_json::to_value(snapshot).unwrap(),
        json!({
            "sessionId":"s","status":null,"messages":[],"total":0,"truncated":false,
            "lastSeq":0,"pendingResume":false,"pendingDialog":null,"pendingDialogs":[],
            "latestSettlement":null,"leafId":null,"firstId":null,"lastId":null,"hasMore":true,"events":[]
        })
    );
}

#[test]
fn snapshot_retains_opaque_history_and_pending_request_identity() {
    let messages = json!([{"entryId":"entry","role":"assistant","content":[
        {"type":"thinking","thinking":"opaque runtime history"},
        {"type":"text","text":"public"},
        {"type":"image","data":"fixture-image","mimeType":"image/png"},
        {"type":"toolCall","id":"call","arguments":{"future":true}}
    ]}]);
    let ui = json!({"question":"Choose","options":["A"],"future":{"version":2}});
    let dialogs = json!([
        {"requestId":"one","responseId":7,"toolCallId":"call","ui":ui},
        {"requestId":"two","responseId":"7","ui":ui}
    ]);
    let snapshot: Snapshot = serde_json::from_value(json!({
        "sessionId":"s","status":"awaiting_input","messages":messages,
        "pendingDialog":dialogs[0],"pendingDialogs":dialogs,"lastSeq":9,
        "events":[{"seq":9,"event":{"type":"ready","opaque":true}}]
    }))
    .unwrap();
    let encoded = serde_json::to_value(snapshot).unwrap();
    assert_eq!(encoded["messages"], messages);
    assert_eq!(encoded["pendingDialogs"][0]["responseId"], 7);
    assert_eq!(encoded["pendingDialogs"][1]["responseId"], "7");
    assert_eq!(encoded["pendingDialog"]["ui"], ui);
    assert_eq!(encoded["events"][0]["event"]["opaque"], true);
}

#[test]
fn dialog_decoding_preserves_raw_ids_and_rejects_only_the_existing_invalid_shape() {
    for id in [json!(7), json!("7")] {
        let dialog = permission(options(), id.clone());
        assert_eq!(dialog.response_id, Some(id));
        assert_eq!(dialog.ui.raw["acpPermission"]["options"], options());
    }
    assert!(
        dialog_from_wire(&wire("ui_dialog_request", json!({"ui":[]})))
            .unwrap()
            .is_none()
    );
    assert!(
        dialog_from_wire(&wire("ui_dialog_request", json!({"requestId":"r","ui":[]}))).is_err()
    );
}

#[test]
fn wire_identity_prefers_explicit_id_then_sequence_then_exact_fingerprint() {
    let event = WireEvent {
        id: Some("native-id".into()),
        seq: Some(4),
        ..wire("ready", json!({}))
    };
    let event = event.into_core_event().unwrap();
    assert_eq!(event.id, "native-id");
    assert_eq!(event.seq, Some(4));
    assert_eq!(
        event.kind,
        EventKind::ConnectionChanged(ConnectionState::Online)
    );
    let event = WireEvent {
        seq: Some(4),
        ..wire("ready", json!({}))
    };
    assert_eq!(event.into_core_event().unwrap().id, "seq-4");
    assert_eq!(
        wire("ready", json!({"future":1}))
            .into_core_event()
            .unwrap()
            .id,
        "event-ready-{\"future\":1}"
    );
}

#[test]
fn required_tool_fields_fail_without_echoing_the_event_payload() {
    for kind in [
        "tool_execution_start",
        "tool_execution_update",
        "tool_execution_end",
    ] {
        for id in [Value::Null, json!(7)] {
            let error = normalize_event(kind, &json!({"toolCallId":id,"secret":"do-not-echo"}))
                .unwrap_err();
            assert_eq!(error.to_string(), "event is missing toolCallId");
            assert!(!format!("{error:?}").contains("do-not-echo"));
        }
    }
}

#[test]
fn connection_end_and_settlement_remain_distinct() {
    assert_eq!(
        normalize_event("ready", &json!({})).unwrap(),
        EventKind::ConnectionChanged(ConnectionState::Online)
    );
    assert_eq!(
        normalize_event("agent_start", &json!({})).unwrap(),
        EventKind::Accepted
    );
    assert_eq!(
        normalize_event("agent_end", &json!({})).unwrap(),
        EventKind::AgentEnd
    );
    let event = normalize_event(
        "agent_settled",
        &json!({
            "aborted":true,"usage":{"inputTokens":3,"output":4,"cacheRead":5,"cache_write":6}
        }),
    )
    .unwrap();
    let EventKind::AgentSettled {
        outcome,
        usage: Some(usage),
    } = event
    else {
        panic!("expected settlement usage")
    };
    assert_eq!(outcome, SettlementOutcome::Interrupted);
    assert_eq!(usage.input_tokens, Some(3));
    assert_eq!(usage.output_tokens, Some(4));
    assert_eq!(usage.cache_read_tokens, Some(5));
    assert_eq!(usage.cache_write_tokens, Some(6));
}

#[test]
fn public_text_allowlist_and_image_tool_results_are_unchanged() {
    let event = normalize_event(
        "message_end",
        &json!({"message":{
            "role":"assistant","entryId":"m","content":[
                {"type":"thinking","text":"private"},
                {"type":"image","text":"not public","data":"fixture"},
                {"type":"unknown","text":"not public"},
                {"type":"text","text":"public"}
            ]
        }}),
    )
    .unwrap();
    assert_eq!(
        event,
        EventKind::TextSnapshots {
            message_id: "m".into(),
            snapshots: vec![("m-text-3".into(), "public".into())],
            authoritative: true,
        }
    );
    for role in ["user", "tool", "toolResult"] {
        assert_eq!(
            normalize_event(
                "message_end",
                &json!({"message":{"role":role,"content":"text"}})
            )
            .unwrap(),
            EventKind::Ignored
        );
    }
    for kind in ["thinking_delta", "tool_call_delta"] {
        assert_eq!(
            normalize_event(
                "message_update",
                &json!({"eventType":kind,"delta":"private"})
            )
            .unwrap(),
            EventKind::Ignored
        );
    }
    let result = json!({"content":[{"type":"image","data":"fixture","mimeType":"image/png"},{"type":"text","text":"caption"}]});
    let event = normalize_event(
        "tool_execution_update",
        &json!({"toolCallId":"t","partialResult":result}),
    )
    .unwrap();
    let EventKind::ToolOutput {
        tool_id,
        content,
        mode,
    } = event
    else {
        panic!("expected tool output")
    };
    assert_eq!(tool_id, "t");
    assert_eq!(mode, OutputMode::Snapshot);
    assert_eq!(serde_json::from_str::<Value>(&content).unwrap(), result);
}

#[test]
fn acp_selection_uses_exact_ids_and_cancel_is_never_inferred_allow() {
    let dialog = permission(options(), json!(7));
    for (name, id) in [("No", "deny-7"), ("Yes", "allow-9")] {
        assert_eq!(
            dialog
                .acp_permission_result(Some(&json!({"kind":"selector","values":[name]})))
                .unwrap(),
            json!({"outcome":{"outcome":"selected","optionId":id}})
        );
    }
    let cancelled = json!({"outcome":{"outcome":"cancelled"}});
    assert_eq!(dialog.acp_permission_result(None).unwrap(), cancelled);
    assert_eq!(dialog.acp_permission_policy(false).unwrap(), cancelled);
    assert_eq!(
        dialog.acp_permission_policy(true).unwrap(),
        json!({"outcome":{"outcome":"selected","optionId":"allow-9"}})
    );
    let denied_only = permission(
        json!([{"optionId":"deny","name":"No","kind":"reject_always"}]),
        json!("id"),
    );
    assert_eq!(denied_only.acp_permission_policy(true).unwrap(), cancelled);
}

#[test]
fn acp_rejects_ambiguous_and_forged_answers_without_changing_policy_semantics() {
    let dialog = permission(options(), json!(7));
    for answer in [
        json!({}),
        json!({"kind":"input","values":["Yes"]}),
        json!({"kind":"selector","values":[]}),
        json!({"kind":"selector","values":["Yes","No"]}),
        json!({"kind":"selector","values":["unlisted"]}),
    ] {
        assert!(dialog.acp_permission_result(Some(&answer)).is_err());
    }
    for result in [
        json!({}),
        json!({"outcome":{"outcome":"selected","optionId":"forged"}}),
        json!({"outcome":{"outcome":"cancelled","optionId":"allow-9"}}),
    ] {
        assert!(dialog.validate_acp_permission_result(&result).is_err());
    }
    let mut ambiguous = options();
    ambiguous[1]["name"] = json!("No");
    assert!(
        permission(ambiguous, json!(7))
            .acp_permission_result(Some(&json!({"kind":"selector","values":["No"]})))
            .is_err()
    );
    let mut duplicate = options();
    duplicate[1]["optionId"] = json!("deny-7");
    assert!(
        permission(duplicate, json!(7))
            .acp_permission_policy(true)
            .is_err()
    );
    let unknown = permission(
        json!([{"optionId":"x","name":"Yes","kind":"unknown"}]),
        json!(7),
    );
    assert!(
        unknown
            .acp_permission_result(Some(&json!({"kind":"selector","values":["Yes"]})))
            .is_err()
    );
    let mut missing = dialog;
    missing.response_id = None;
    assert!(missing.acp_permission_result(None).is_err());
    assert!(missing.acp_permission_policy(false).is_err());
    // Preserve the original opt-in policy helper: it validates the negotiated
    // option payload, not the transport's outstanding request/response identity.
    assert_eq!(
        missing.acp_permission_policy(true).unwrap(),
        json!({"outcome":{"outcome":"selected","optionId":"allow-9"}})
    );
}
