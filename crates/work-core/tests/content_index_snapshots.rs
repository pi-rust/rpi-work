use serde_json::{Value, json};
use work_core::api::{WireEvent, events::normalize_event};
use work_core::{ApplyResult, Event, EventKind, MessageBlock, RunProjection};

fn wire(seq: u64, kind: &str, data: Value) -> Event {
    WireEvent {
        id: Some(format!("wire-{seq}")),
        seq: Some(seq),
        event_type: kind.into(),
        data,
    }
    .into_core_event()
    .unwrap()
}

fn full(seq: u64, kind: &str, id: &str, content: Value) -> Event {
    wire(
        seq,
        kind,
        json!({"contentIndex":99,"message":{
            "entryId":id,"role":"assistant","content":content
        }}),
    )
}

fn delta(seq: u64, id: &str, index: u64, text: &str) -> Event {
    wire(
        seq,
        "message_update",
        json!({
            "messageId":id,"eventType":"text_delta","contentIndex":index,"delta":text
        }),
    )
}

fn text_slots(run: &RunProjection) -> Vec<(&str, &str)> {
    run.blocks
        .iter()
        .filter_map(|block| match block {
            MessageBlock::Text { id, text } if !text.is_empty() => {
                Some((id.as_str(), text.as_str()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn full_snapshot_then_compact_delta_does_not_freeze_a_duplicate_hello() {
    let mut run = RunProjection::new("run");
    run.apply(delta(1, "m", 1, "Hello"));
    run.apply(full(
        2,
        "message_update",
        "m",
        json!([
            {"type":"thinking","thinking":"private"},
            {"type":"text","text":"Hello"}
        ]),
    ));
    // Previously this displayed both m-text-1=Hello and m-text-0=Hello.
    assert_eq!(text_slots(&run), vec![("m-text-1", "Hello")]);
    run.apply(delta(3, "m", 1, " world"));
    assert_eq!(text_slots(&run), vec![("m-text-1", "Hello world")]);
}

#[test]
fn every_whole_message_event_uses_actual_slots_one_and_three() {
    for kind in ["message_start", "message_update", "message_end"] {
        let mut run = RunProjection::new("run");
        let event = full(
            1,
            kind,
            "m",
            json!([
                {"type":"reasoning","text":"private"},
                {"type":"input_text","content":"first"},
                {"type":"toolCall","text":"secret","arguments":{"text":"secret"}},
                {"type":"output_text","text":"second"}
            ]),
        );
        assert_eq!(
            event.kind,
            EventKind::TextSnapshots {
                message_id: "m".into(),
                snapshots: vec![
                    ("m-text-1".into(), "first".into()),
                    ("m-text-3".into(), "second".into()),
                ],
                authoritative: true,
            }
        );
        assert_eq!(run.apply(event.clone()), ApplyResult::Applied);
        assert_eq!(
            text_slots(&run),
            vec![("m-text-1", "first"), ("m-text-3", "second")]
        );
        assert_eq!(run.cursor, Some(1));
        assert_eq!(run.apply(event), ApplyResult::IgnoredDuplicate);
        run.apply(delta(2, "m", 3, "!"));
        assert_eq!(
            text_slots(&run),
            vec![("m-text-1", "first"), ("m-text-3", "second!")]
        );
    }
}

#[test]
fn public_mapping_matches_content_text_without_leaking_private_or_tool_bodies() {
    let mut run = RunProjection::new("run");
    run.apply(full(
        1,
        "message_end",
        "m",
        json!([
            {"thinking":"private","text":"hidden"},
            {"type":"text","text":"public"},
            {"type":"reasoning","content":"private"},
            {"type":"tool_call","content":"secret"},
            {"type":"image","text":"secret"},
            {"type":"unknown","text":"secret"},
            "legacy string part",
            {"content":"legacy object part"},
            {"type":"text","text":""}
        ]),
    ));
    assert_eq!(
        text_slots(&run),
        vec![
            ("m-text-1", "public"),
            ("m-text-6", "legacy string part"),
            ("m-text-7", "legacy object part")
        ]
    );
    assert!(!format!("{:?}", run.blocks).contains("private"));
    assert!(!format!("{:?}", run.blocks).contains("secret"));
}

#[test]
fn retirement_clears_in_place_and_preserves_other_message_and_tool_indices() {
    let mut run = RunProjection::new("run");
    run.apply(delta(1, "m", 0, "obsolete"));
    run.apply(delta(2, "m-child", 0, "distinct"));
    run.apply(wire(
        3,
        "tool_execution_start",
        json!({"toolCallId":"tool","toolName":"read"}),
    ));
    run.apply(full(
        4,
        "message_update",
        "m",
        json!([
            {"type":"thinking","text":"hidden"}, {"type":"text","text":"current"}
        ]),
    ));
    assert_eq!(
        run.blocks.iter().map(MessageBlock::id).collect::<Vec<_>>(),
        vec!["m-text-0", "m-child-text-0", "tool", "m-text-1"]
    );
    assert_eq!(
        run.blocks[0],
        MessageBlock::Text {
            id: "m-text-0".into(),
            text: String::new()
        }
    );
    assert_eq!(
        text_slots(&run),
        vec![("m-child-text-0", "distinct"), ("m-text-1", "current")]
    );
    run.apply(full(
        5,
        "message_end",
        "m",
        json!([{"type":"text","text":"replacement"}]),
    ));
    assert_eq!(run.blocks[2].id(), "tool");
    assert_eq!(
        run.blocks[3],
        MessageBlock::Text {
            id: "m-text-1".into(),
            text: String::new()
        }
    );
    assert_eq!(
        text_slots(&run),
        vec![("m-text-0", "replacement"), ("m-child-text-0", "distinct")]
    );
}

#[test]
fn empty_start_update_or_missing_body_does_not_erase_nonzero_partial() {
    let mut run = RunProjection::new("run");
    run.apply(delta(1, "m", 1, "partial"));
    run.apply(full(2, "message_start", "m", json!([])));
    run.apply(full(3, "message_update", "m", json!([])));
    run.apply(wire(
        4,
        "message_end",
        json!({"message":{"entryId":"m","role":"assistant"}}),
    ));
    assert_eq!(text_slots(&run), vec![("m-text-1", "partial")]);
    // An explicit complete empty array, unlike an absent body, is authoritative.
    run.apply(full(5, "message_end", "m", json!([])));
    assert!(text_slots(&run).is_empty());
    assert_eq!(run.blocks[0].id(), "m-text-1");
}

#[test]
fn private_only_authoritative_snapshot_retires_public_text_without_exposing_private() {
    let mut run = RunProjection::new("run");
    run.apply(delta(1, "m", 1, "obsolete"));
    run.apply(full(
        2,
        "message_end",
        "m",
        json!([{"type":"thinking","text":"secret"}]),
    ));
    assert!(text_slots(&run).is_empty());
    assert_eq!(run.blocks.len(), 1);
}

#[test]
fn scalar_legacy_snapshots_and_stream_slot_zero_stay_compatible() {
    for body in [json!("Hello"), json!({"text":"Hello"})] {
        assert_eq!(
            normalize_event(
                "message_update",
                &json!({"message":{"entryId":"m","content":body}})
            )
            .unwrap(),
            EventKind::TextSnapshot {
                block_id: "m-text-0".into(),
                text: "Hello".into()
            }
        );
    }
    let mut run = RunProjection::new("run");
    run.apply(full(1, "message_start", "m", json!("")));
    run.apply(delta(2, "m", 0, "Hel"));
    run.apply(full(3, "message_update", "m", json!("Hello")));
    run.apply(delta(4, "m", 0, "!"));
    assert_eq!(text_slots(&run), vec![("m-text-0", "Hello!")]);
}

#[test]
fn stale_full_snapshot_and_deltas_cannot_mutate_any_slot() {
    let mut run = RunProjection::new("run");
    let snapshot = full(
        5,
        "message_update",
        "m",
        json!([
            {"type":"thinking"}, {"type":"text","text":"first"},
            {"type":"toolCall"}, {"type":"text","text":"second"}
        ]),
    );
    run.apply(snapshot.clone());
    run.apply(delta(6, "m", 3, "!"));
    let before = run.clone();
    assert_eq!(run.apply(snapshot), ApplyResult::IgnoredDuplicate);
    assert_eq!(
        run.apply(full(5, "message_end", "m", json!([]))),
        ApplyResult::IgnoredDuplicate
    );
    let mut stale = full(5, "message_end", "m", json!([]));
    stale.id = "different-id".into();
    assert_eq!(run.apply(stale), ApplyResult::IgnoredStale);
    assert_eq!(
        run.apply(delta(4, "m", 1, "stale")),
        ApplyResult::IgnoredStale
    );
    assert_eq!(run, before);
}
