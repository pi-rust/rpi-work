use dioxus::prelude::*;
use serde_json::Value;
use work_client::WireEvent;
pub const CSS: &str = include_str!("activity.css");

#[derive(Clone)]
pub struct PendingReply {
    pub text: String,
    pub users_before: usize,
    pub cursor_before: Option<u64>,
    pub accepted: bool,
    pub responded: bool,
}

pub fn users(messages: &[Value]) -> usize {
    messages
        .iter()
        .filter(|message| message["role"].as_str() == Some("user"))
        .count()
}

pub fn echoed(pending: &PendingReply, messages: &[Value]) -> bool {
    messages
        .iter()
        .filter(|message| message["role"].as_str() == Some("user"))
        .skip(pending.users_before)
        .any(|message| {
            let content = &message["content"];
            let text = content.as_str().map_or_else(
                || {
                    content
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|block| block["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("")
                },
                str::to_owned,
            );
            text == pending.text
        })
}

pub fn has_reply(pending: &PendingReply, event: &WireEvent) -> bool {
    if event
        .seq
        .is_some_and(|seq| Some(seq) <= pending.cursor_before)
    {
        return false;
    }
    match event.event_type.as_str() {
        "tool_execution_start"
        | "tool_execution_update"
        | "tool_execution_end"
        | "agent_settled"
        | "ui_dialog_request" => true,
        "message_update" | "message_end" => {
            let message = &event.data["message"];
            if message["role"].as_str() == Some("user") {
                return false;
            }
            let delta = event.data["delta"]
                .as_str()
                .or_else(|| event.data["assistantMessageEvent"]["delta"].as_str());
            delta.is_some_and(|delta| !delta.is_empty())
                || (message["role"].as_str() == Some("assistant")
                    && (message["content"]
                        .as_str()
                        .is_some_and(|text| !text.is_empty())
                        || message["content"]
                            .as_array()
                            .is_some_and(|blocks| !blocks.is_empty())))
        }
        _ => false,
    }
}

pub fn replied_in_history(pending: &PendingReply, messages: &[Value]) -> bool {
    let mut users_seen = 0;
    let mut sent = false;
    for message in messages {
        if message["role"].as_str() == Some("user") {
            if users_seen >= pending.users_before
                && echoed(
                    &PendingReply {
                        users_before: 0,
                        ..pending.clone()
                    },
                    std::slice::from_ref(message),
                )
            {
                sent = true;
            }
            users_seen += 1;
        } else if sent
            && message["role"].as_str() == Some("assistant")
            && (message["content"]
                .as_str()
                .is_some_and(|text| !text.is_empty())
                || message["content"].as_array().is_some_and(|blocks| {
                    blocks.iter().any(|block| {
                        block["text"].as_str().is_some_and(|text| !text.is_empty())
                            || block["type"].as_str().is_some_and(|kind| kind != "text")
                    })
                }))
        {
            return true;
        }
    }
    false
}

#[component]
pub fn Spinner() -> Element {
    rsx! { span { class: "activity-spinner", aria_hidden: "true" } }
}

#[component]
pub fn Waiting(label: String, busy: bool) -> Element {
    rsx! { div { class: "reply-waiting", role: "status", aria_live: "polite",
        if busy { Spinner {} }
        else { span { class: "reply-paused", aria_hidden: "true" } }
        span { "{label}" }
    } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_identical_messages_and_previous_settlement_do_not_hide_pending_reply() {
        let pending = PendingReply {
            text: "继续".into(),
            users_before: 1,
            cursor_before: Some(9),
            accepted: true,
            responded: false,
        };
        let old = serde_json::json!({"role":"user","content":"继续"});
        assert!(!echoed(&pending, std::slice::from_ref(&old)));
        assert!(echoed(&pending, &[old.clone(), old]));
        let mut event = WireEvent {
            id: None,
            seq: Some(10),
            event_type: "message_end".into(),
            data: serde_json::json!({"message":{"role":"user","content":"继续"}}),
        };
        assert!(!has_reply(&pending, &event));
        event.event_type = "tool_execution_start".into();
        assert!(has_reply(&pending, &event));
        event.seq = Some(9);
        assert!(!has_reply(&pending, &event));
    }
}
