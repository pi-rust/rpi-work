use super::*;

// Owned inputs keep the event fixtures readable and mirror boundary snapshots.
#[allow(clippy::needless_pass_by_value)]
fn fixture(messages: Vec<Value>, events: Vec<Value>) -> Snapshot {
    let events: Vec<_> = events
        .into_iter()
        .enumerate()
        .map(|(i, event)| json!({"seq":i + 1,"event":event}))
        .collect();
    serde_json::from_value(json!({"sessionId":"dedup-fixture","lastSeq":events.len(),"messages":messages,"events":events})).unwrap()
}

#[allow(clippy::needless_pass_by_value)] // Fixture frames are built and consumed at each call site.
fn live(
    identity: &mut MessageIdentity,
    messages: &mut Vec<Value>,
    projection: &mut RunProjection,
    data: Value,
    seq: u64,
) {
    let data = identity.annotate(&data, Some(seq));
    apply_live_message_event(
        messages,
        projection,
        WireEvent {
            id: Some(seq.to_string()),
            seq: Some(seq),
            event_type: data["type"].as_str().unwrap().into(),
            data,
        },
    )
    .unwrap();
}

#[test]
fn duplicate_regression_snapshot_promotion_without_end_keeps_canonical_owner() {
    let snapshot = fixture(
        vec![json!({"entryId":"formal","role":"assistant","content":"persisted"})],
        vec![
            json!({"type":"message_start","message":{"role":"assistant","content":""}}),
            json!({"type":"message_update","eventType":"text_delta","delta":"new answer","message":{"entryId":"formal","role":"assistant","content":"new answer"}}),
        ],
    );
    let (messages, projection) = snapshot_state(&snapshot).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0]["content"], "new answer",
        "restoration must publish the newest canonical body as well as its projection"
    );
    assert_eq!(
        projection.blocks.len(),
        1,
        "unfinished text must not be discarded"
    );
    assert_eq!(
        projection.blocks[0].id(),
        "formal-text-0",
        "replay must use the same proven owner as live events"
    );
    assert_eq!(projection.cursor, Some(2));
    assert_eq!(
        snapshot.messages[0]["content"], "persisted",
        "projection replay must not mutate snapshot history"
    );
}

#[test]
fn duplicate_regression_roleless_delta_after_promotion_keeps_canonical_owner() {
    let mut messages = vec![json!({"entryId":"formal","role":"assistant","content":"persisted"})];
    let mut projection = RunProjection::new("fixture");
    let mut identity = MessageIdentity::default();
    for (seq, data) in [
        json!({"type":"message_start","message":{"role":"assistant","content":[]}}),
        json!({"type":"message_update","eventType":"text_delta","delta":"new","message":{"entryId":"formal","role":"assistant","content":"new"}}),
        json!({"type":"message_update","eventType":"text_delta","delta":" tail","message":{}}),
    ].into_iter().enumerate() {
        live(&mut identity, &mut messages, &mut projection, data, seq as u64 + 1);
    }
    assert_eq!(
        projection.blocks.len(),
        1,
        "a roleless delta must not reopen the old anonymous slot: {:?}",
        projection.blocks
    );
    assert_eq!(projection.blocks[0].id(), "formal-text-0");
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0]["content"], "new tail",
        "a public delta must extend the retained body rather than be hidden by it"
    );
}

#[test]
fn duplicate_regression_public_delta_preserves_structured_content_and_private_blocks() {
    let original = json!({"entryId":"formal","role":"assistant","content":[
        {"type":"thinking","thinking":"must remain private"},
        {"type":"text","text":"before"},
        {"type":"toolCall","id":"call","name":"read","arguments":{}},
        {"type":"text","text":"after"}
    ]});
    let mut messages = vec![original.clone()];
    let mut projection = RunProjection::new("fixture");
    let mut identity = MessageIdentity::default();
    live(
        &mut identity,
        &mut messages,
        &mut projection,
        json!({"type":"message_update","eventType":"text_delta","contentIndex":3,"delta":" tail","message":{"entryId":"formal"}}),
        1,
    );
    assert_eq!(messages[0]["content"][0], original["content"][0]);
    assert_eq!(messages[0]["content"][1], original["content"][1]);
    assert_eq!(messages[0]["content"][2], original["content"][2]);
    assert_eq!(messages[0]["content"][3]["text"], "after tail");
    assert_eq!(messages.len(), 1);
}

#[test]
fn duplicate_regression_snapshot_does_not_append_already_persisted_deltas_twice() {
    for start_id in [None, Some("formal")] {
        let mut start = json!({"type":"message_start","message":{"role":"assistant","content":[]}});
        if let Some(id) = start_id {
            start["message"]["entryId"] = json!(id);
        }
        let snapshot = fixture(
            vec![json!({"entryId":"formal","role":"assistant","content":"answer"})],
            vec![
                start,
                json!({"type":"message_update","eventType":"text_delta","delta":"answer","message":{"entryId":"formal"}}),
            ],
        );
        let (messages, projection) = snapshot_state(&snapshot).unwrap();
        assert_eq!(messages[0]["content"], "answer");
        assert_eq!(projection.blocks.len(), 1);
        assert_eq!(projection.blocks[0].id(), "formal-text-0");
    }
}

#[test]
fn duplicate_regression_unanchored_snapshot_delta_keeps_authoritative_history_body() {
    let snapshot = fixture(
        vec![json!({"entryId":"formal","role":"assistant","content":"prefix tail"})],
        vec![
            json!({"type":"message_update","eventType":"text_delta","delta":"tail","message":{"entryId":"formal"}}),
        ],
    );
    let (messages, projection) = snapshot_state(&snapshot).unwrap();
    assert_eq!(messages[0]["content"], "prefix tail");
    assert_eq!(projection.blocks.len(), 1);
    assert_eq!(
        projection.blocks[0],
        work_core::MessageBlock::Text {
            id: "formal-text-0".into(),
            text: "prefix tail".into()
        }
    );
}

#[test]
fn duplicate_regression_structured_start_keeps_text_delta_in_its_own_part() {
    let original = json!({"entryId":"formal","role":"assistant","content":[
        {"type":"text","text":"before"},
        {"type":"toolCall","id":"call","name":"read","arguments":{}},
        {"type":"text","text":"after"}
    ]});
    let mut messages = Vec::new();
    let mut projection = RunProjection::new("fixture");
    let mut identity = MessageIdentity::default();
    live(
        &mut identity,
        &mut messages,
        &mut projection,
        json!({"type":"message_start","message":original}),
        1,
    );
    live(
        &mut identity,
        &mut messages,
        &mut projection,
        json!({"type":"message_update","eventType":"text_delta","contentIndex":0,"delta":" tail","message":{}}),
        2,
    );
    assert_eq!(messages[0]["content"][0]["text"], "before tail");
    assert_eq!(messages[0]["content"][1], original["content"][1]);
    assert_eq!(messages[0]["content"][2]["text"], "after");
}

#[test]
fn duplicate_regression_roleless_user_end_inherits_only_the_open_segment() {
    let mut identity = MessageIdentity::default();
    let start = identity.annotate(
        &json!({"type":"message_start","message":{"role":"user","content":"hello"}}),
        Some(1),
    );
    let end = identity.annotate(
        &json!({"type":"message_end","message":{"entryId":"user-formal","content":"hello"}}),
        Some(2),
    );
    assert_eq!(end["message"]["role"], "user");
    assert_eq!(end["message"]["entryId"], start["message"]["entryId"]);
    assert_eq!(canonical_entry_id(&end["message"]), Some("user-formal"));
    let next = identity.annotate(&json!({"type":"message_start","message":{}}), Some(3));
    assert_eq!(
        next["message"]["role"], "assistant",
        "an independent start must not inherit the old user's role"
    );
}

#[test]
fn duplicate_regression_timestamp_change_is_not_an_identity_boundary() {
    for role in ["assistant", "user"] {
        let mut identity = MessageIdentity::default();
        let mut messages = Vec::new();
        let mut projection = RunProjection::new("dedup-fixture");
        live(
            &mut identity,
            &mut messages,
            &mut projection,
            json!({"type":"message_start","message":{"role":role,"timestamp":42,"content":"partial"}}),
            1,
        );
        let first_id = messages[0]["entryId"].clone();
        live(
            &mut identity,
            &mut messages,
            &mut projection,
            json!({"type":"message_end","message":{"entryId":"formal","role":role,"timestamp":43,"content":"complete"}}),
            2,
        );
        assert_eq!(
            messages.len(),
            1,
            "{role}: first formal ID promotes the open segment even if timestamp changed"
        );
        assert_eq!(messages[0]["entryId"], first_id);
        assert_eq!(messages[0]["content"], "complete");
        assert_eq!(canonical_entry_id(&messages[0]), Some("formal"));
    }
}

#[test]
fn duplicate_regression_snapshot_covers_promoted_whole_segment() {
    let end = json!({"entryId":"formal","role":"assistant","timestamp":43,"content":"complete"});
    let snapshot = fixture(
        vec![end.clone()],
        vec![
            json!({"type":"message_start","message":{"role":"assistant","timestamp":42,"content":"partial"}}),
            json!({"type":"message_update","eventType":"text_delta","delta":"complete","message":{"role":"assistant","timestamp":42}}),
            json!({"type":"message_end","message":end}),
        ],
    );
    let projection = projection_from_snapshot(&snapshot).unwrap();
    assert!(
        projection.blocks.is_empty(),
        "completed formal history must cover its entire anonymous segment: {:?}",
        projection.blocks
    );
    assert_eq!(projection.cursor, Some(3));
}

#[test]
fn duplicate_regression_live_promotion_converges_existing_history_and_projection() {
    for start_has_body in [false, true] {
        let canonical =
            json!({"entryId":"formal","role":"assistant","timestamp":42,"content":"complete"});
        let start = json!({"type":"message_start","message":{"role":"assistant","timestamp":42,"content":if start_has_body {"partial"} else {""}}});
        let snapshot = fixture(vec![canonical.clone()], vec![start.clone()]);
        let mut projection = projection_from_snapshot(&snapshot).unwrap();
        let mut identity = MessageIdentity::default();
        identity.annotate(&start, Some(1));
        let mut messages = snapshot.messages;
        live(
            &mut identity,
            &mut messages,
            &mut projection,
            json!({"type":"message_update","eventType":"text_delta","delta":"complete","message":{"role":"assistant","content":"complete"}}),
            2,
        );
        live(
            &mut identity,
            &mut messages,
            &mut projection,
            json!({"type":"message_end","message":canonical.clone()}),
            3,
        );
        assert_eq!(
            messages.len(),
            1,
            "authoritative alias may not leave an extra anonymous row"
        );
        assert_eq!(messages[0]["entryId"], "formal");
        assert!(
            projection
                .blocks
                .iter()
                .all(|block| block.id().starts_with("formal-text-")),
            "all text blocks belong to their surviving history owner: {:?}",
            projection.blocks
        );
        assert_eq!(
            projection.blocks.len(),
            1,
            "promotion updates its existing projection slot"
        );
        reconcile_messages(&mut messages, &mut projection, vec![canonical], false);
        assert_eq!(messages.len(), 1, "settlement remains converged");
    }
}

#[test]
fn duplicate_regression_bodyless_delta_after_promotion_keeps_one_projection_slot() {
    let canonical =
        json!({"entryId":"formal","role":"assistant","timestamp":42,"content":"old snapshot"});
    let mut messages = vec![canonical];
    let mut projection = RunProjection::new("fixture");
    let mut identity = MessageIdentity::default();
    for (seq, data) in [
        json!({"type":"message_start","message":{"role":"assistant","timestamp":42,"content":[]}}),
        json!({"type":"message_update","eventType":"text_delta","delta":"new","message":{"role":"assistant"}}),
        json!({"type":"message_update","eventType":"text_delta","delta":"new","message":{"role":"assistant","entryId":"formal","content":"new"}}),
        json!({"type":"message_update","eventType":"text_delta","delta":" tail","message":{"role":"assistant"}}),
        json!({"type":"message_end","message":{"role":"assistant","entryId":"formal","content":"new tail"}}),
    ].into_iter().enumerate() {
        live(&mut identity, &mut messages, &mut projection, data, seq as u64 + 1);
        assert_eq!(projection.blocks.len(), usize::from(seq > 0), "event {} must update the same slot: {:?}", seq + 1, projection.blocks);
        if seq == 3 {
            assert_eq!(messages[0]["content"], "new tail", "a bodyless event must preserve the body and append its public delta");
            assert_eq!(projection.blocks[0].id(), "formal-text-0");
        }
    }
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["content"], "new tail");
}

// Actual rpc/events.rs compact-deltas shape: identity lives on the envelope;
// delta frames intentionally carry no message snapshot.
fn compact_segment(id: Option<&str>, text: &str) -> Vec<Value> {
    let mut events = vec![
        json!({"type":"message_start","message":{"kind":"assistant","timestamp":42,"content":[]}}),
        json!({"type":"message_update","eventType":"text_delta","contentIndex":0,"delta":text,
            "assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":text}}),
        json!({"type":"message_end","message":{"kind":"assistant","timestamp":42,"content":[{"type":"text","text":text}]}}),
    ];
    if let Some(id) = id {
        for event in &mut events {
            event["messageId"] = json!(id);
        }
    }
    events
}

#[test]
fn duplicate_regression_compact_envelope_identity_has_one_owner_before_and_after_settlement() {
    let mut messages = Vec::new();
    let mut projection = RunProjection::new("fixture");
    let mut identity = MessageIdentity::default();
    for (index, data) in compact_segment(Some("host-entry"), "real wire answer")
        .into_iter()
        .enumerate()
    {
        live(
            &mut identity,
            &mut messages,
            &mut projection,
            data,
            index as u64 + 1,
        );
        assert_eq!(
            projection.blocks.len(),
            usize::from(index > 0),
            "empty starts create no invented slot; compact text addresses its own slot"
        );
        if index > 0 {
            assert_eq!(projection.blocks[0].id(), "host-entry-text-0");
        }
    }
    assert_eq!(messages.len(), 1);
    assert_eq!(entry_id(&messages[0]), Some("host-entry"));
    let canonical = json!({"entryId":"host-entry","role":"assistant","timestamp":42,"content":[{"type":"text","text":"real wire answer"}]});
    reconcile_messages(&mut messages, &mut projection, vec![canonical], false);
    assert_eq!(
        messages.len(),
        1,
        "settlement must not append an anonymous copy"
    );
}

#[test]
fn duplicate_regression_compact_legacy_without_ids_uses_open_segment_not_global_fallback() {
    let mut messages = Vec::new();
    let mut projection = RunProjection::new("fixture");
    let mut identity = MessageIdentity::default();
    for (index, data) in compact_segment(None, "legacy answer")
        .into_iter()
        .enumerate()
    {
        live(
            &mut identity,
            &mut messages,
            &mut projection,
            data,
            index as u64 + 1,
        );
        assert_eq!(
            projection.blocks.len(),
            usize::from(index > 0),
            "bodyless delta must inherit the open segment identity without inventing a start slot"
        );
    }
    assert_eq!(messages.len(), 1);
}

#[test]
fn duplicate_regression_compact_envelope_snapshot_covers_persisted_whole_segment() {
    let canonical =
        json!({"entryId":"host-entry","role":"assistant","timestamp":99,"content":"complete"});
    let snapshot = fixture(
        vec![canonical.clone()],
        compact_segment(Some("host-entry"), "complete"),
    );
    let (messages, projection) = snapshot_state(&snapshot).unwrap();
    assert_eq!(messages, vec![canonical]);
    assert_eq!(
        projection.blocks.len(),
        0,
        "envelope ID proves overlap even if timestamp/body representation differs"
    );
}

#[test]
fn duplicate_regression_compact_identical_replies_with_distinct_envelope_ids_remain_distinct() {
    let mut messages = Vec::new();
    let mut projection = RunProjection::new("fixture");
    let mut identity = MessageIdentity::default();
    for (index, data) in compact_segment(Some("first"), "same answer")
        .into_iter()
        .chain(compact_segment(Some("second"), "same answer"))
        .enumerate()
    {
        live(
            &mut identity,
            &mut messages,
            &mut projection,
            data,
            index as u64 + 1,
        );
    }
    assert_eq!(messages.len(), 2);
    assert_eq!(entry_id(&messages[0]), Some("first"));
    assert_eq!(entry_id(&messages[1]), Some("second"));
    assert_eq!(projection.blocks.len(), 2);
}

#[test]
fn duplicate_regression_envelope_user_echo_merges_the_persisted_user_not_a_new_anonymous_row() {
    let canonical = json!({"entryId":"user-entry","role":"user","content":"same user text"});
    let mut messages = vec![canonical.clone()];
    let mut projection = RunProjection::new("fixture");
    let mut identity = MessageIdentity::default();
    for (index, kind) in ["message_start", "message_end"].into_iter().enumerate() {
        live(
            &mut identity,
            &mut messages,
            &mut projection,
            json!({"type":kind,"messageId":"user-entry","message":{"kind":"user","content":"same user text"}}),
            index as u64 + 1,
        );
    }
    assert_eq!(messages.len(), 1);
    assert_eq!(entry_id(&messages[0]), Some("user-entry"));
    assert_eq!(projection.blocks.len(), 0);
    let snapshot = fixture(
        vec![canonical],
        vec![
            json!({"type":"message_start","messageId":"user-entry","message":{"kind":"user","content":"same user text"}}),
            json!({"type":"message_end","messageId":"user-entry","message":{"kind":"user","content":"same user text"}}),
        ],
    );
    let (messages, projection) = snapshot_state(&snapshot).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(projection.blocks.len(), 0);
}

#[test]
fn duplicate_regression_compact_update_cannot_inherit_an_unfinished_user_identity() {
    let mut identity = MessageIdentity::default();
    identity.annotate(&json!({"type":"message_start","messageId":"user","message":{"kind":"user","content":"question"}}), Some(1));
    let update = identity.annotate(&json!({"type":"message_update","messageId":"assistant","eventType":"text_delta","delta":"answer"}), Some(2));
    assert_eq!(update["message"]["role"], "assistant");
    assert_eq!(entry_id(&update["message"]), Some("assistant"));
}

#[test]
fn duplicate_regression_event_id_is_not_a_message_identity_and_blank_envelope_ids_fall_back() {
    let mut identity = MessageIdentity::default();
    let anonymous = identity.annotate(&json!({"type":"message_update","id":"wire-event","eventType":"text_delta","delta":"answer"}), Some(1));
    assert_ne!(entry_id(&anonymous["message"]), Some("wire-event"));
    let explicit = identity.annotate(&json!({"type":"message_start","messageId":" ","message":{"entryId":"nested","kind":"assistant","content":[]}}), Some(2));
    assert_eq!(entry_id(&explicit["message"]), Some("nested"));
}

#[test]
fn duplicate_regression_compact_snapshot_different_authoritative_ids_never_cover_each_other() {
    let canonical = json!({"entryId":"older","role":"assistant","timestamp":42,"content":"same"});
    let snapshot = fixture(
        vec![canonical.clone()],
        compact_segment(Some("newer"), "same"),
    );
    let (messages, projection) = snapshot_state(&snapshot).unwrap();
    assert_eq!(messages, vec![canonical]);
    assert_eq!(projection.blocks.len(), 1);
    assert_eq!(projection.blocks[0].id(), "newer-text-0");
}

fn public_slots(projection: &RunProjection) -> Vec<(&str, &str)> {
    projection
        .blocks
        .iter()
        .filter_map(|block| match block {
            work_core::MessageBlock::Text { id, text } if !text.is_empty() => {
                Some((id.as_str(), text.as_str()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn structured_snapshot_empty_history_then_live_tail_uses_only_slot_one() {
    let snapshot = fixture(
        vec![],
        vec![
            json!({"type":"message_start","messageId":"m","message":{"role":"assistant","content":[]}}),
            json!({"type":"message_update","messageId":"m","eventType":"text_delta","contentIndex":1,"delta":"Hello"}),
            json!({"type":"message_update","contentIndex":1,"messageId":"m","message":{"role":"assistant","content":[{"type":"thinking","text":"private"},{"type":"text","text":"Hello"}]}}),
        ],
    );
    let (mut messages, mut projection) = snapshot_state(&snapshot).unwrap();
    assert!(
        messages.is_empty(),
        "event-only rows retain chronological projection ownership"
    );
    let mut identity = MessageIdentity::default();
    for item in &snapshot.events {
        identity.annotate(&item.event, Some(item.seq));
    }
    live(
        &mut identity,
        &mut messages,
        &mut projection,
        json!({"type":"message_update","messageId":"m","eventType":"text_delta","contentIndex":1,"delta":" tail"}),
        4,
    );
    assert_eq!(public_slots(&projection), vec![("m-text-1", "Hello tail")]);
    assert!(
        !projection
            .blocks
            .iter()
            .any(|block| block.id() == "m-text-0")
    );
    assert!(!format!("{:?}", projection.blocks).contains("private"));
}

#[test]
fn structured_snapshot_replay_start_resets_matching_history_slots_not_other_messages() {
    let history = json!({"entryId":"m","role":"assistant","content":[
        {"type":"thinking","text":"private"}, {"type":"text","text":"first"},
        {"type":"toolCall","id":"call","name":"read","arguments":{}},
        {"type":"output_text","text":"second"}, {"type":"image","data":"AA==","mimeType":"image/png"}
    ]});
    let snapshot = fixture(
        vec![history.clone()],
        vec![
            json!({"type":"message_update","messageId":"m-child","eventType":"text_delta","delta":"unrelated"}),
            json!({"type":"message_start","messageId":"m","message":{"role":"assistant","content":[]}}),
            json!({"type":"message_update","messageId":"m","eventType":"text_delta","contentIndex":1,"delta":"first"}),
            json!({"type":"message_update","messageId":"m","eventType":"text_delta","contentIndex":3,"delta":"second"}),
        ],
    );
    let (messages, projection) = snapshot_state(&snapshot).unwrap();
    assert_eq!(messages, vec![history]);
    assert_eq!(
        public_slots(&projection),
        vec![
            ("m-child-text-0", "unrelated"),
            ("m-text-1", "first"),
            ("m-text-3", "second")
        ]
    );
}

#[test]
fn structured_snapshot_full_update_uses_each_part_not_envelope_index() {
    let mut messages = vec![];
    let mut projection = RunProjection::new("fixture");
    let mut identity = MessageIdentity::default();
    live(
        &mut identity,
        &mut messages,
        &mut projection,
        json!({"type":"message_update","messageId":"m","contentIndex":3,"message":{"role":"assistant","content":[
            {"type":"reasoning","text":"private"}, {"type":"input_text","text":"first"},
            {"type":"toolCall","id":"call","name":"read","text":"secret"}, {"type":"output_text","text":"second"},
            {"type":"image","text":"secret"}
        ]}}),
        1,
    );
    live(
        &mut identity,
        &mut messages,
        &mut projection,
        json!({"type":"message_update","messageId":"m","eventType":"text_delta","contentIndex":3,"delta":"!"}),
        2,
    );
    assert_eq!(
        public_slots(&projection),
        vec![("m-text-1", "first"), ("m-text-3", "second!")]
    );
    assert_eq!(messages[0]["content"][1]["text"], "first");
    assert_eq!(messages[0]["content"][3]["text"], "second!");
    assert!(!format!("{:?}", projection.blocks).contains("private"));
    assert!(!format!("{:?}", projection.blocks).contains("secret"));
}

#[test]
fn structured_snapshot_batch_promotion_uses_surviving_owner_for_every_part() {
    let mut messages = vec![json!({"entryId":"formal","role":"assistant","content":"old"})];
    let mut projection = RunProjection::new("fixture");
    let mut identity = MessageIdentity::default();
    live(
        &mut identity,
        &mut messages,
        &mut projection,
        json!({"type":"message_start","message":{"role":"assistant","content":[]}}),
        1,
    );
    live(
        &mut identity,
        &mut messages,
        &mut projection,
        json!({"type":"message_update","eventType":"text_delta","contentIndex":1,"delta":"first"}),
        2,
    );
    live(
        &mut identity,
        &mut messages,
        &mut projection,
        json!({"type":"message_update","messageId":"formal","contentIndex":3,"message":{"role":"assistant","content":[{"type":"thinking","text":"private"},{"type":"text","text":"first"},{"type":"toolCall","text":"secret"},{"type":"text","text":"second"}]}}),
        3,
    );
    live(
        &mut identity,
        &mut messages,
        &mut projection,
        json!({"type":"message_update","messageId":"formal","eventType":"text_delta","contentIndex":3,"delta":"!"}),
        4,
    );
    assert_eq!(messages.len(), 1);
    assert_eq!(
        public_slots(&projection),
        vec![("formal-text-1", "first"), ("formal-text-3", "second!")]
    );
}

#[test]
fn structured_snapshot_run_boundary_clears_anchor_and_new_start_resets_only_same_id() {
    let mut messages = vec![
        json!({"entryId":"m","role":"assistant","content":[{"type":"thinking","text":"private"},{"type":"text","text":"old"}]}),
    ];
    let mut projection = RunProjection::new("fixture");
    let mut identity = MessageIdentity::default();
    for (index, event) in [
        json!({"type":"message_start","messageId":"other","message":{"role":"assistant","content":[{"type":"text","text":"same"}]}}),
        json!({"type":"agent_start"}),
        json!({"type":"message_update","messageId":"m","eventType":"text_delta","contentIndex":1,"delta":"!"}),
        json!({"type":"agent_start"}),
        json!({"type":"message_start","messageId":"m","message":{"role":"assistant","content":[]}}),
        json!({"type":"message_update","messageId":"m","eventType":"text_delta","contentIndex":1,"delta":"same"}),
    ].into_iter().enumerate() {
        live(&mut identity, &mut messages, &mut projection, event, index as u64 + 1);
        if index == 2 { assert_eq!(messages[0]["content"][1]["text"], "old!", "unanchored delta extends matching history"); }
    }
    assert_eq!(
        public_slots(&projection),
        vec![("other-text-0", "same"), ("m-text-1", "same")]
    );
    assert_eq!(
        messages.len(),
        2,
        "identical text never collapses distinct identities"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn structured_snapshot_http_restore_and_stale_frames_keep_one_live_slot() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    let snapshot = fixture(
        vec![],
        vec![
            json!({"type":"message_start","messageId":"m","message":{"role":"assistant","content":[]}}),
            json!({"type":"message_update","messageId":"m","eventType":"text_delta","contentIndex":1,"delta":"Hello"}),
            json!({"type":"message_update","messageId":"m","message":{"role":"assistant","content":[{"type":"thinking","text":"private"},{"type":"text","text":"Hello"}]}}),
        ],
    );
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let client = WorkClient::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    client.set_remote_transport(crate::RemoteTransport::HttpSseOnly);
    let access = json!({"descriptor":client.descriptor(),"supportedCommands":[],"snapshotLimit":false,"dialogReply":false}).to_string();
    let snapshot_body = serde_json::to_string(&snapshot).unwrap();
    let stale = json!({"type":"message_start","messageId":"wrong","message":{"role":"assistant","content":[{"type":"text","text":"stale"}]}});
    let tail =
        json!({"type":"message_update","eventType":"text_delta","contentIndex":1,"delta":" tail"});
    let event_body = format!(
        "id: 3\nevent: message_start\ndata: {stale}\n\nid: 4\nevent: message_update\ndata: {tail}\n\n"
    );
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            for (expected, content_type, body) in [
                ("/access", "application/json", access.as_str()),
                ("/snapshot", "application/json", snapshot_body.as_str()),
                ("/events?after=3", "text/event-stream", event_body.as_str()),
            ] {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                assert!(String::from_utf8_lossy(&request).contains(expected));
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        }
    });
    let mut connection = SessionConnection {
        client: WorkspaceClient::Remote(client),
        session_id: "dedup-fixture".into(),
        projection: RunProjection::new("dedup-fixture"),
        stream: Box::pin(futures_util::stream::empty()),
        snapshot_limit: false,
        message_page_info: MessagePageInfo::default(),
        history_len: 0,
        messages: vec![],
        pending_dialogs: vec![],
        reconnect_attempt: 0,
        reconnecting: false,
        connection_epoch: 0,
        identity: MessageIdentity::default(),
    };
    for _ in 0..2 {
        connection.restore_snapshot().await.unwrap();
        assert_eq!(
            public_slots(&connection.projection),
            vec![("m-text-1", "Hello")]
        );
        assert_eq!(
            connection.next().await.unwrap().unwrap().applied,
            ApplyResult::IgnoredStale
        );
        connection.next().await.unwrap().unwrap();
        assert_eq!(
            public_slots(&connection.projection),
            vec![("m-text-1", "Hello tail")]
        );
        assert!(connection.messages.is_empty());
    }
    server.join().unwrap();
}

// This fixture was captured from the installed native runtime, not hand-written
// message ID assumptions. Its provider was an isolated loopback canned server.
#[test]
fn captured_native_short_reply_has_one_owner_at_every_boundary() {
    let captured: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native-short-reply.json"
    ))
    .unwrap();
    let canonical: Vec<Value> = serde_json::from_value(captured["messages"].clone()).unwrap();
    let events: Vec<Value> = serde_json::from_value(captured["events"].clone()).unwrap();
    let mut messages = canonical[..1].to_vec();
    let mut projection = RunProjection::new("native-short");
    let mut identity = MessageIdentity::default();
    for (index, data) in events.into_iter().enumerate() {
        let kind = data["type"].as_str().unwrap().to_owned();
        live(
            &mut identity,
            &mut messages,
            &mut projection,
            data,
            index as u64 + 1,
        );
        if kind == "agent_settled" {
            reconcile_messages(&mut messages, &mut projection, canonical.clone(), false);
        }
        let history_replies = messages
            .iter()
            .filter(|m| message_role(m) == Some("assistant"))
            .count();
        let uncovered_text = projection.blocks.iter().filter(|block| {
            matches!(block, work_core::MessageBlock::Text { text, .. } if !text.trim().is_empty())
                && !messages.iter().filter_map(entry_id).any(|id| {
                    block.id() == id || block.id().starts_with(&format!("{id}-text-"))
                })
        }).count();
        assert!(
            history_replies + uncovered_text <= 1,
            "duplicate owner after {kind}: {messages:?}, {:?}",
            projection.blocks
        );
    }
    assert_eq!(messages, canonical);
    assert_eq!(projection.blocks.len(), 1);
    assert_eq!(projection.blocks[0].id(), "native-short-assistant-text-0");
}

#[test]
fn captured_native_short_reply_reopens_without_a_stream_copy() {
    let captured: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native-short-reply.json"
    ))
    .unwrap();
    let canonical: Vec<Value> = serde_json::from_value(captured["messages"].clone()).unwrap();
    let events: Vec<Value> = serde_json::from_value(captured["events"].clone()).unwrap();
    let snapshot = fixture(canonical.clone(), events);
    let (messages, projection) = snapshot_state(&snapshot).unwrap();
    assert_eq!(messages, canonical);
    assert!(
        projection.blocks.is_empty(),
        "persisted message must cover its captured complete stream"
    );
}

#[test]
fn boundary_summaries_cannot_insert_or_overwrite_transcript_messages() {
    for kind in [
        "turn_start",
        "turn_end",
        "agent_end",
        "operation_finished",
        "unknown_summary",
    ] {
        for id in [None, Some("existing"), Some("not-a-new-entry")] {
            let original = json!({"entryId":"existing","role":"assistant","timestamp":42,"content":"original"});
            let mut messages = vec![original.clone()];
            let mut projection = RunProjection::new("summaries");
            let mut identity = MessageIdentity::default();
            let mut summary = json!({"type":kind,"message":{"role":"assistant","timestamp":42,"content":"summary is not a transcript publication"}});
            if let Some(id) = id {
                summary["message"]["entryId"] = json!(id);
            }
            live(&mut identity, &mut messages, &mut projection, summary, 1);
            assert_eq!(
                messages,
                vec![original],
                "summary {kind} with ID {id:?} changed history"
            );
            assert!(projection.blocks.is_empty());
        }
    }
}

#[test]
fn repeated_real_replies_keep_both_entries_despite_equal_turn_summaries() {
    let mut messages = Vec::new();
    let mut projection = RunProjection::new("repeated");
    let mut identity = MessageIdentity::default();
    let mut seq = 0;
    for id in ["first", "second"] {
        for event in [
            json!({"type":"agent_start"}),
            json!({"type":"message_start","messageId":id,"message":{"role":"assistant","timestamp":42,"content":[]}}),
            json!({"type":"message_end","messageId":id,"message":{"role":"assistant","timestamp":42,"content":[{"type":"text","text":"identical"}]}}),
            json!({"type":"turn_end","message":{"role":"assistant","timestamp":42,"content":[{"type":"text","text":"identical"}]}}),
            json!({"type":"agent_settled","outcome":"completed"}),
        ] {
            seq += 1;
            live(&mut identity, &mut messages, &mut projection, event, seq);
        }
    }
    assert_eq!(
        messages.iter().filter_map(entry_id).collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert_eq!(projection.blocks.len(), 2);
}

#[test]
fn summary_filter_keeps_tool_and_settlement_control_events() {
    let mut messages = Vec::new();
    let mut projection = RunProjection::new("control");
    let mut identity = MessageIdentity::default();
    for (index, event) in [
        json!({"type":"agent_start"}),
        json!({"type":"tool_execution_start","toolCallId":"call","toolName":"read","args":{}}),
        json!({"type":"tool_execution_end","toolCallId":"call","result":"result"}),
        json!({"type":"agent_end"}),
        json!({"type":"agent_settled","outcome":"completed"}),
    ]
    .into_iter()
    .enumerate()
    {
        live(
            &mut identity,
            &mut messages,
            &mut projection,
            event,
            index as u64 + 1,
        );
    }
    assert!(messages.is_empty());
    assert_eq!(
        projection.phase,
        work_core::RunPhase::Settled(work_core::SettlementOutcome::Completed)
    );
    assert!(
        matches!(&projection.blocks[..], [work_core::MessageBlock::ToolCall { id, status: work_core::ToolStatus::Completed, .. }] if id == "call")
    );
}
