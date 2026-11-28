use super::*;
use crate::transcript::TranscriptOrder;
use serde_json::json;

#[test]
fn native_attachments_keep_order_and_tool_ownership_without_serializing_payloads() {
    let history = [
        json!({"role":"user","entryId":"u","content":{"type":"image","data":"user-payload"}}),
        json!({"role":"assistant","entryId":"a","content":[
            {"type":"text","text":"before"},
            {"type":"video","data":"video-payload"},
            {"type":"toolCall","id":"call","name":"render"},
            {"type":"file","data":"file-payload"},
            {"type":"text","text":"after"},
        ]}),
        json!({"role":"toolResult","toolCallId":"call","isError":false,"content":[
            {"type":"image","data":"tool-payload"},
        ]}),
    ];
    let rows = TranscriptOrder::default().update("scope", &history, &[]);
    let view = present_with_attachments(
        &rows,
        &history,
        &[],
        None,
        None,
        PendingRequests::default(),
        Some(|value, _| matches!(value["type"].as_str(), Some("image" | "file" | "video"))),
    );
    let [
        Part::Message {
            attachments: user, ..
        },
        Part::Message { text: before, .. },
        Part::Message {
            attachments: video, ..
        },
        Part::Activity { items, .. },
        Part::Message {
            attachments: file, ..
        },
        Part::Message {
            text: after,
            rating_entry_id,
            ..
        },
    ] = view.parts.as_slice()
    else {
        panic!("unexpected transcript layout")
    };
    let [ActivityItem::Tool { block, .. }] = items.as_slice() else {
        panic!("expected one tool")
    };
    for (refs, data) in [
        (user, "user-payload"),
        (video, "video-payload"),
        (file, "file-payload"),
        (&block.attachments, "tool-payload"),
    ] {
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].get(&history).unwrap()["data"], data);
    }
    assert_eq!(
        (before.as_str(), after.as_str(), rating_entry_id.as_deref()),
        ("before", "after", Some("a"))
    );
    assert!(!serde_json::to_string(&view).unwrap().contains("-payload"));
}

fn view(
    history: &[Value],
    blocks: &[MessageBlock],
    phase: RunPhase,
    pending: PendingRequests,
) -> Value {
    let rows = TranscriptOrder::default().update("scope", history, blocks);
    serde_json::to_value(present(
        &rows,
        history,
        blocks,
        Some(&phase),
        Some(&ConnectionState::Online),
        pending,
    ))
    .unwrap()
}
fn tool(status: ToolStatus) -> MessageBlock {
    MessageBlock::ToolCall {
        id: "call".into(),
        name: "bash".into(),
        arguments: "{}".into(),
        output: "output".into(),
        status,
    }
}
#[test]
fn reading_keys_use_history_identity_without_replacing_render_keys() {
    let prompt = json!({"entryId": "user-id", "role": "user", "content": "prompt"});
    let reply = json!({"messageId": "answer-id", "role": "assistant", "content": [
        {"type": "text", "text": "first"}, {"type": "text", "text": "second"}
    ]});
    let first = view(
        &[prompt.clone(), reply.clone()],
        &[],
        RunPhase::Ready,
        PendingRequests::default(),
    );
    let shifted = view(
        &[
            json!({"role": "user", "content": "anonymous"}),
            prompt,
            reply,
            json!({"id": "", "role": "assistant", "content": "empty identity"}),
        ],
        &[MessageBlock::Text {
            id: "draft-text-0".into(),
            text: "streamed".into(),
        }],
        RunPhase::Ready,
        PendingRequests::default(),
    );
    let parts = shifted["parts"].as_array().unwrap();
    assert_ne!(first["parts"][0]["key"], parts[1]["key"]);
    for (index, key) in [
        "user/user-id",
        "text/answer-id-text-0",
        "text/answer-id-text-1",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(first["parts"][index]["readingKey"], *key);
        assert_eq!(parts[index + 1]["readingKey"], *key);
    }
    for index in [0, 4, 5] {
        assert!(parts[index].get("readingKey").is_none());
    }
}

#[test]
fn completed_tool_does_not_imply_agent_completion() {
    for phase in [
        RunPhase::Tool,
        RunPhase::AwaitingSettlement,
        RunPhase::Ready,
    ] {
        let value = view(
            &[],
            &[tool(ToolStatus::Completed)],
            phase,
            PendingRequests::default(),
        );
        assert_eq!(value["parts"][0]["collapsed"], false);
        assert_ne!(value["observation"], "已完成");
    }
}
#[test]
fn only_known_completed_settlement_defaults_to_collapsed() {
    for (outcome, collapsed, label) in [
        (SettlementOutcome::Completed, true, "已完成"),
        (SettlementOutcome::Failed, false, "运行失败"),
        (SettlementOutcome::Interrupted, false, "已中断"),
        (SettlementOutcome::Suspended, false, "已挂起"),
        (SettlementOutcome::Declined, false, "已拒绝"),
        (
            SettlementOutcome::Unknown("new-value".into()),
            false,
            "结算结果未知",
        ),
    ] {
        let value = view(
            &[],
            &[tool(ToolStatus::Completed)],
            RunPhase::Settled(outcome),
            PendingRequests::default(),
        );
        assert_eq!(value["parts"][0]["collapsed"], collapsed);
        assert_eq!(value["observation"], label);
    }
}
#[test]
fn old_completed_observation_cannot_collapse_pending_or_unknown_request() {
    for pending in [
        PendingRequests {
            submitting: true,
            ..Default::default()
        },
        PendingRequests {
            awaiting_reply: true,
            ..Default::default()
        },
        PendingRequests {
            cancelling: true,
            ..Default::default()
        },
        PendingRequests {
            configuring: true,
            ..Default::default()
        },
        PendingRequests {
            uncertain: true,
            ..Default::default()
        },
    ] {
        let value = view(
            &[],
            &[tool(ToolStatus::Completed)],
            RunPhase::Settled(SettlementOutcome::Completed),
            pending,
        );
        assert_eq!(value["parts"][0]["collapsed"], false);
        assert_ne!(value["observation"], "已完成");
        assert_ne!(value["observationTone"], "success");
    }
}
#[test]
fn reconnect_does_not_present_old_terminal_as_current_fact() {
    let blocks = [tool(ToolStatus::Completed)];
    let rows = TranscriptOrder::default().update("scope", &[], &blocks);
    let value = serde_json::to_value(present(
        &rows,
        &[],
        &blocks,
        Some(&RunPhase::Settled(SettlementOutcome::Completed)),
        Some(&ConnectionState::Reconnecting),
        PendingRequests::default(),
    ))
    .unwrap();
    assert_eq!(value["observation"], "连接恢复中，等待新观察");
    assert_eq!(value["parts"][0]["collapsed"], false);
}
#[test]
fn prose_is_a_sibling_outside_activity_and_cancelled_is_not_success() {
    let blocks = [
        tool(ToolStatus::Cancelled),
        MessageBlock::Text {
            id: "answer".into(),
            text: "final answer".into(),
        },
    ];
    let value = view(
        &[],
        &blocks,
        RunPhase::Settled(SettlementOutcome::Interrupted),
        PendingRequests::default(),
    );
    assert_eq!(value["parts"][0]["kind"], "activity");
    assert_eq!(
        value["parts"][0]["items"][0]["block"]["statusLabel"],
        "已取消"
    );
    assert_eq!(value["parts"][1]["kind"], "message");
    assert_eq!(value["parts"][1]["text"], "final answer");
}
#[test]
fn history_tools_survive_projection_absorption_and_result_is_not_assistant_prose() {
    let history = [
        json!({"id":"message", "role":"assistant", "content":[{"type":"toolCall","id":"call","name":"bash","arguments":{"command":"pwd"}}]}),
        json!({"id":"result", "role":"toolResult", "toolCallId":"call","isError":true,"content":[{"type":"text","text":"actual failure"}]}),
        json!({"id":"answer", "role":"assistant", "content":"explanation"}),
    ];
    let value = view(
        &history,
        &[tool(ToolStatus::Completed)],
        RunPhase::Ready,
        PendingRequests::default(),
    );
    assert_eq!(value["parts"].as_array().unwrap().len(), 2);
    let item = &value["parts"][0]["items"][0];
    assert_eq!(item["key"], "tool/call");
    assert_eq!(item["block"]["status"], "failed");
    assert_eq!(item["block"]["output"], "actual failure");
    assert_eq!(value["parts"][1]["text"], "explanation");
}
#[test]
fn history_call_without_result_has_unknown_not_running_or_success_status() {
    let history = [
        json!({"id":"message","role":"assistant","content":[{"type":"toolCall","id":"call","name":"read","arguments":{}}]}),
    ];
    let value = view(&history, &[], RunPhase::Ready, PendingRequests::default());
    assert_eq!(value["parts"][0]["items"][0]["block"]["status"], "unknown");
    assert_eq!(
        value["parts"][0]["items"][0]["block"]["statusLabel"],
        "结果未观察"
    );
}
#[test]
fn private_reasoning_signatures_images_and_unknown_roles_never_become_prose() {
    let history = [
        json!({"id":"a","role":"assistant","content":[{"type":"thinking","thinking":"private","redacted":true,"text":"private"},{"type":"image","text":"image secret"},{"type":"custom","text":"opaque secret"},{"type":"text","text":"public"}]}),
        json!({"id":"s","role":"system","content":"system secret"}),
    ];
    let value = view(&history, &[], RunPhase::Ready, PendingRequests::default());
    let text = value.to_string();
    assert!(text.contains("public"));
    for hidden in ["private", "secret", "signature"] {
        assert!(!text.contains(hidden));
    }
    assert_eq!(value["parts"].as_array().unwrap().len(), 1);
}
#[test]
fn permitted_thinking_stays_inside_activity_not_answer() {
    let history = [
        json!({"id":"a","role":"assistant","content":[{"type":"thinking","thinking":"visible reasoning","signature":"opaque"},{"type":"text","text":"answer"}]}),
    ];
    let value = view(&history, &[], RunPhase::Ready, PendingRequests::default());
    assert_eq!(value["parts"][0]["items"][0]["kind"], "note");
    assert_eq!(value["parts"][1]["text"], "answer");
    assert!(!value.to_string().contains("opaque"));
}

#[test]
fn failure_cancellation_unknown_and_running_items_keep_attention_even_after_completed_settlement() {
    for status in [
        ToolStatus::Failed,
        ToolStatus::Cancelled,
        ToolStatus::Running,
    ] {
        let value = view(
            &[],
            &[tool(status)],
            RunPhase::Settled(SettlementOutcome::Completed),
            PendingRequests::default(),
        );
        assert_eq!(value["parts"][0]["attention"], true);
        assert_eq!(value["parts"][0]["collapsed"], false);
    }
}

#[test]
fn fork_anchors_require_canonical_assistant_history() {
    let history = [
        json!({"entryId":"prompt","role":"user","content":"question"}),
        json!({"entryId":"pending","__rpiClientAnonymousId":true,"role":"assistant","content":"unresolved"}),
        json!({"entryId":"previous","__rpiClientAnonymousId":true,"__rpiClientCanonicalId":"committed","role":"assistant","content":"reconciled"}),
    ];
    let blocks = [MessageBlock::Text {
        id: "streamed-text-0".into(),
        text: "live projection".into(),
    }];
    let result = view(
        &history,
        &blocks,
        RunPhase::Ready,
        PendingRequests::default(),
    );
    let parts = result["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 4);
    assert_eq!(parts[2]["forkEntryId"], "committed");
    assert_eq!(parts[2]["ratingEntryId"], "committed");
    for index in [0, 1, 3] {
        assert!(parts[index].get("forkEntryId").is_none());
        assert!(parts[index].get("ratingEntryId").is_none());
    }
}

#[test]
fn a_reply_with_several_text_parts_has_one_rating_target() {
    let history = [json!({"entryId":"reply","role":"assistant","content":[
        {"type":"text","text":"first"},
        {"type":"thinking","thinking":"working"},
        {"type":"text","text":"last"},
        {"type":"text","text":" "}
    ]})];
    let result = view(&history, &[], RunPhase::Ready, PendingRequests::default());
    let rated_parts: Vec<_> = result["parts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|part| part.get("ratingEntryId").is_some())
        .collect();
    assert_eq!(rated_parts.len(), 1);
    assert_eq!(rated_parts[0]["text"], "last");
    assert_eq!(rated_parts[0]["ratingEntryId"], "reply");
}

#[test]
fn canonical_text_identity_survives_scalar_and_content_array_history_transfer() {
    for content in [json!("answer"), json!([{"type":"text","text":"answer"}])] {
        let blocks = [MessageBlock::Text {
            id: "message-text-0".into(),
            text: "answer".into(),
        }];
        let mut order = TranscriptOrder::default();
        let rows = order.update("scope", &[], &blocks);
        let before = serde_json::to_value(present(
            &rows,
            &[],
            &blocks,
            None,
            None,
            PendingRequests::default(),
        ))
        .unwrap();
        let history = [json!({"entryId":"message","role":"assistant","content":content})];
        let rows = order.update("scope", &history, &blocks);
        let after = serde_json::to_value(present(
            &rows,
            &history,
            &blocks,
            None,
            None,
            PendingRequests::default(),
        ))
        .unwrap();
        assert_eq!(before["parts"][0]["key"], after["parts"][0]["key"]);
        assert_eq!(after["parts"].as_array().unwrap().len(), 1);
    }
}
