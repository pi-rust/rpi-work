use super::*;

#[allow(clippy::needless_pass_by_value)] // Owned boundary fixture inputs.
fn snapshot(messages: Vec<Value>, events: Vec<Value>) -> Snapshot {
    let events: Vec<_> = events
        .into_iter()
        .enumerate()
        .map(|(i, event)| json!({"seq":i + 1,"event":event}))
        .collect();
    serde_json::from_value(json!({"sessionId":"fixture", "lastSeq":events.len(),
        "messages":messages,"events":events}))
    .unwrap()
}

#[test]
fn snapshot_overlap_skips_whole_segment_but_keeps_active_stream_and_settlement() {
    let complete =
        json!({"role":"assistant","timestamp":42,"content":[{"type":"text","text":"done"}]});
    let snapshot = snapshot(
        vec![complete.clone()],
        vec![
            json!({"type":"agent_start"}),
            json!({"type":"message_start","message":{"role":"assistant","timestamp":42}}),
            json!({"type":"message_update","message":{"role":"assistant","timestamp":42},"eventType":"text_delta","delta":"done"}),
            json!({"type":"message_end","message":complete}),
            json!({"type":"agent_end"}),
            json!({"type":"agent_settled","outcome":"completed"}),
            json!({"type":"agent_start"}),
            json!({"type":"message_update","message":{"role":"assistant"},"eventType":"text_delta","delta":"next"}),
        ],
    );
    let projection = projection_from_snapshot(&snapshot).unwrap();
    assert_eq!(projection.blocks.len(), 1);
    assert_eq!(projection.cursor, Some(8));
    assert_eq!(projection.phase, work_core::RunPhase::Text);
}

#[test]
fn user_segment_identity_is_stable_promotes_and_does_not_cross_turns() {
    let mut identity = MessageIdentity::default();
    let mut messages = Vec::new();
    let mut ids = Vec::new();
    for turn in 0..2 {
        for (step, kind) in ["message_start", "message_update", "message_end"]
            .into_iter()
            .enumerate()
        {
            let mut message = json!({"role":"user","content":"same"});
            if step == 2 {
                message["id"] = json!(format!("user-{turn}"));
            }
            let event = identity.annotate(
                &json!({"type":kind,"message":message}),
                Some(turn * 3 + step as u64 + 1),
            );
            ids.push(event["message"]["entryId"].clone());
            merge_live_message(&mut messages, &event, None);
        }
    }
    assert_eq!(ids[0], ids[1]);
    assert_eq!(ids[0], ids[2]);
    assert_eq!(ids[3], ids[4]);
    assert_eq!(ids[3], ids[5]);
    assert_ne!(ids[0], ids[3]);
    assert_eq!(messages.len(), 2);
    assert_eq!(canonical_entry_id(&messages[0]), Some("user-0"));
    assert_eq!(canonical_entry_id(&messages[1]), Some("user-1"));
}

fn coverage(snapshot: &Snapshot) -> Vec<bool> {
    let mut identity = MessageIdentity::default();
    let annotated: Vec<_> = snapshot
        .events
        .iter()
        .map(|item| identity.annotate(&item.event, Some(item.seq)))
        .collect();
    snapshot_replay::covered_events(snapshot, &annotated)
}

#[test]
fn snapshot_coverage_requires_id_or_exact_full_structure_and_timestamp() {
    let base = json!({"role":"assistant","timestamp":42,"content":[
        {"type":"text","text":"same"},
        {"type":"image","data":"image-a","mimeType":"image/png"},
        {"type":"thinking","thinking":"private"},
        {"type":"toolCall","id":"call","name":"read","arguments":{"path":"a"}}
    ],"usage":{"output":1}});
    let mut cases = Vec::new();
    cases.push((base.clone(), base.clone(), true));
    let mut history = base.clone();
    history["entryId"] = json!("host");
    cases.push((history.clone(), base.clone(), true));
    let mut incoming = base.clone();
    incoming["messageId"] = json!("host");
    incoming["content"] = json!("host ID wins despite payload differences");
    cases.push((history.clone(), incoming, true));
    let mut incoming = base.clone();
    incoming["id"] = json!("other-host");
    cases.push((history, incoming, false));
    for (path, value) in [
        (vec!["content", "1", "data"], json!("image-b")),
        (vec!["content", "2", "thinking"], json!("other private")),
        (vec!["content", "3", "arguments", "path"], json!("b")),
        (vec!["usage", "output"], json!(2)),
        (vec!["timestamp"], json!(43)),
        (vec!["role"], json!("user")),
    ] {
        let mut incoming = base.clone();
        let mut cursor = &mut incoming;
        for key in path {
            cursor = if let Ok(index) = key.parse::<usize>() {
                &mut cursor[index]
            } else {
                &mut cursor[key]
            };
        }
        *cursor = value;
        cases.push((base.clone(), incoming, false));
    }
    let mut missing_timestamp = base.clone();
    missing_timestamp
        .as_object_mut()
        .unwrap()
        .remove("timestamp");
    cases.push((missing_timestamp.clone(), missing_timestamp, false));
    for (history, incoming, expected) in cases {
        let snapshot = snapshot(
            vec![history.clone()],
            vec![json!({"type":"message_end","message":incoming})],
        );
        assert_eq!(
            coverage(&snapshot),
            vec![expected],
            "history={history}, incoming={incoming}"
        );
    }
}

#[test]
fn snapshot_coverage_ignores_transport_timing_but_requires_numeric_timestamp() {
    let complete = json!({"role":"assistant","timestamp":42,"content":"same"});
    let mut history = complete.clone();
    history["entryId"] = json!("formal");
    history["webTiming"] = json!({"elapsedMs":123,"running":false});
    let snapshot = snapshot(
        vec![history],
        vec![json!({"type":"message_end","message":complete})],
    );
    assert_eq!(
        coverage(&snapshot),
        vec![true],
        "display timing is not native message identity"
    );
    for timestamp in [json!("42"), json!({"invalid":42}), json!(null)] {
        let message = json!({"role":"assistant","timestamp":timestamp,"content":"same"});
        let snapshot = self::snapshot(
            vec![message.clone()],
            vec![json!({"type":"message_end","message":message})],
        );
        assert_eq!(
            coverage(&snapshot),
            vec![false],
            "malformed timestamps do not prove overlap"
        );
    }
}

#[test]
fn snapshot_coverage_is_one_to_one_and_preserves_unfinished_segments() {
    let message = json!({"role":"assistant","timestamp":42,"content":"same"});
    let snapshot = snapshot(
        vec![message.clone()],
        vec![
            json!({"type":"message_start","message":message}),
            json!({"type":"message_end","message":message}),
            json!({"type":"message_start","message":message}),
            json!({"type":"message_end","message":message}),
            json!({"type":"message_start","message":message}),
        ],
    );
    assert_eq!(coverage(&snapshot), vec![true, true, false, false, false]);
    let projection = projection_from_snapshot(&snapshot).unwrap();
    assert_eq!(projection.blocks.len(), 2);
    // Filtering must not shift the anonymous identity counter.
    assert_eq!(projection.blocks[0].id(), "anonymous-3-2-text-0");
    assert_eq!(projection.blocks[1].id(), "anonymous-5-3-text-0");
}

#[test]
fn snapshot_tools_require_actual_result_not_just_historical_call() {
    let snapshot = snapshot(
        vec![
            json!({"role":"assistant","content":[{"type":"toolCall","id":"pending","name":"read"}]}),
            json!({"role":"toolResult","toolCallId":"done","content":[]}),
            json!({"role":"toolResult","toolCallId":"empty"}),
        ],
        ["pending", "done", "empty", "unknown"]
            .into_iter()
            .flat_map(|call| {
                [
                    "tool_execution_start",
                    "tool_execution_update",
                    "tool_execution_end",
                ]
                .into_iter()
                .map(move |kind| json!({"type":kind,"toolCallId":call,"toolName":"read"}))
            })
            .chain([
                json!({"type":"agent_end"}),
                json!({"type":"agent_settled","outcome":"completed","usage":{"input":3}}),
            ])
            .collect(),
    );
    assert_eq!(
        coverage(&snapshot),
        vec![
            false, false, false, true, true, true, false, false, false, false, false, false, false,
            false
        ]
    );
    let projection = projection_from_snapshot(&snapshot).unwrap();
    assert_eq!(projection.blocks.len(), 3);
    assert_eq!(
        projection.phase,
        work_core::RunPhase::Settled(SettlementOutcome::Completed)
    );
    assert_eq!(projection.cursor, Some(14));
}

#[test]
fn user_segment_breaks_assistant_context_and_reconciles_formal_id_without_timestamp() {
    let mut identity = MessageIdentity::default();
    let assistant = identity.annotate(
        &json!({"type":"message_update","message":{"role":"assistant"}}),
        Some(1),
    );
    let start = identity.annotate(
        &json!({"type":"message_start","message":{"role":"user","content":"same"}}),
        Some(2),
    );
    let end = identity.annotate(&json!({"type":"message_end","message":{"role":"user","entryId":"formal","content":"same"}}), Some(3));
    assert_eq!(start["message"]["entryId"], end["message"]["entryId"]);
    assert_ne!(assistant["message"]["entryId"], start["message"]["entryId"]);
    let next = identity.annotate(
        &json!({"type":"message_update","message":{"role":"assistant"}}),
        Some(4),
    );
    assert_ne!(assistant["message"]["entryId"], next["message"]["entryId"]);
    let mut messages = Vec::new();
    merge_live_message(&mut messages, &start, Some(2));
    merge_live_message(&mut messages, &end, Some(3));
    let mut projection = RunProjection::new("fixture");
    reconcile_messages(
        &mut messages,
        &mut projection,
        vec![json!({"role":"user","entryId":"formal","content":"same"})],
        false,
    );
    assert_eq!(messages.len(), 1);
    assert_eq!(entry_id(&messages[0]), Some("formal"));
}

#[test]
fn snapshot_end_without_id_must_respect_explicit_id_seen_earlier_in_segment() {
    for (start_id, expected) in [("same-host", true), ("different-host", false)] {
        let snapshot = snapshot(
            vec![json!({"role":"assistant","entryId":"same-host","timestamp":42,"content":"same"})],
            vec![
                json!({"type":"message_start","message":{"role":"assistant","entryId":start_id}}),
                json!({"type":"message_end","message":{"role":"assistant","timestamp":42,"content":"same"}}),
            ],
        );
        assert_eq!(
            coverage(&snapshot),
            vec![expected, expected],
            "start_id={start_id}"
        );
    }
}

#[test]
fn fully_anonymous_user_rounds_with_identical_body_and_timestamp_stay_distinct() {
    let mut identity = MessageIdentity::default();
    let mut messages = Vec::new();
    for turn in 0..2 {
        for (step, kind) in ["message_start", "message_update", "message_end"]
            .into_iter()
            .enumerate()
        {
            let event = identity.annotate(
                &json!({"type":kind,"message":{"role":"user","timestamp":42,"content":"same"}}),
                Some(turn * 3 + step as u64 + 1),
            );
            merge_live_message(&mut messages, &event, None);
        }
    }
    assert_eq!(messages.len(), 2);
    assert_ne!(entry_id(&messages[0]), entry_id(&messages[1]));
}
