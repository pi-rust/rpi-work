//! Snapshot-local coverage only; never a live/body-wide deduplication policy.
use super::{
    CLIENT_ANONYMOUS_ID, CLIENT_CANONICAL_ID, Snapshot, canonical_entry_id, entry_id, message_role,
};
use serde_json::{Value, json};

// Value equality compares the complete structure (including images, reasoning,
// tool calls and metadata), independently of object key order. Transport timing
// and ID aliases are excluded; authoritative IDs are checked before this fingerprint.
fn fingerprint(message: &Value) -> Value {
    let mut value = message.clone();
    if let Some(object) = value.as_object_mut() {
        for key in [
            "entryId",
            "messageId",
            "id",
            "webTiming",
            CLIENT_ANONYMOUS_ID,
            CLIENT_CANONICAL_ID,
        ] {
            object.remove(key);
        }
    }
    value
}

fn covering_message(messages: &[Value], used: &[bool], incoming: &Value) -> Option<usize> {
    let eligible = |index: usize, message: &Value| {
        !used[index] && message_role(message) == message_role(incoming)
    };
    if let Some(id) = entry_id(incoming) {
        if let Some(index) = messages
            .iter()
            .enumerate()
            .position(|(index, message)| eligible(index, message) && entry_id(message) == Some(id))
        {
            return Some(index);
        }
    }
    let timestamp = incoming
        .get("timestamp")
        .filter(|value| value.as_f64().is_some_and(f64::is_finite))?;
    let structure = fingerprint(incoming);
    messages.iter().enumerate().position(|(index, message)| {
        eligible(index, message)
            && !(entry_id(message).is_some() && entry_id(incoming).is_some())
            && message.get("timestamp") == Some(timestamp)
            && fingerprint(message) == structure
    })
}

pub(super) fn covered_events(snapshot: &Snapshot, annotated: &[Value]) -> Vec<bool> {
    let mut covered = vec![false; snapshot.events.len()];
    let mut used = vec![false; snapshot.messages.len()];
    let mut segment = Vec::new();
    let mut identity = None;
    for (index, item) in snapshot.events.iter().enumerate() {
        let event = &item.event;
        let kind = event["type"].as_str().unwrap_or_default();
        if matches!(
            kind,
            "operation_accepted" | "agent_start" | "agent_end" | "agent_settled"
        ) {
            segment.clear();
            identity = None;
        }
        if matches!(
            kind,
            "tool_execution_start" | "tool_execution_update" | "tool_execution_end"
        ) {
            // A historical call alone is not a result. Only a persisted result
            // with the same call ID can cover execution events for that call.
            covered[index] = event
                .get("toolCallId")
                .and_then(Value::as_str)
                .is_some_and(|call| {
                    snapshot.messages.iter().any(|message| {
                        matches!(
                            message_role(message),
                            Some("tool" | "toolResult" | "tool_result")
                        ) && message.get("toolCallId").and_then(Value::as_str) == Some(call)
                            && message.get("content").is_some_and(|value| !value.is_null())
                    })
                });
        }
        if !matches!(kind, "message_start" | "message_update" | "message_end") {
            continue;
        }
        // Compact updates have no raw message, but their normalized identity
        // must still join the segment covered by its complete message_end.
        let Some(message) = event
            .get("message")
            .or_else(|| annotated[index].get("message"))
        else {
            continue;
        };
        let role = message_role(&annotated[index]["message"]).unwrap_or("assistant");
        if !matches!(role, "assistant" | "user") {
            continue;
        }
        let key = (
            role.to_owned(),
            entry_id(&annotated[index]["message"]).map(str::to_owned),
        );
        if kind == "message_start" || identity.as_ref() != Some(&key) {
            segment.clear();
        }
        identity = Some(key);
        segment.push(index);
        // Never discard a partial stream on weak resemblance to history.
        // A complete end confirms coverage of this segment, not adjacent turns.
        if kind == "message_end" {
            // The envelope carries the durable entry ID in native RPC;
            // annotate also preserves an ID seen earlier in this segment.
            let mut complete = message.clone();
            if let Some(id) = canonical_entry_id(&annotated[index]["message"]) {
                complete["entryId"] = json!(id);
            }
            if let Some(history_index) = covering_message(&snapshot.messages, &used, &complete) {
                used[history_index] = true;
                for &index in &segment {
                    covered[index] = true;
                }
            }
            segment.clear();
            identity = None;
        }
    }
    covered
}
