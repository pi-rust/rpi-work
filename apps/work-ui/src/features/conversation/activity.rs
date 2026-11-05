use dioxus::prelude::*;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use work_client::WireEvent;
pub const CSS: &str = include_str!("activity.css");
const OUTPUT_MOTION: &str = include_str!("output_motion.js");
static NEXT_SPINNER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct PendingReply {
    pub text: String,
    /// Raw editor text can differ from the attachment-expanded echo text.
    pub draft_text: Option<String>,
    /// Ordered visual content for the optimistic row; never persisted locally.
    pub content: Option<Value>,
    pub users_before: usize,
    /// Stable ID of the last loaded user at send time; `None` uses legacy counting.
    pub user_boundary: Option<String>,
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

pub fn echo_message<'a>(pending: &PendingReply, messages: &'a [Value]) -> Option<&'a Value> {
    let (start, users_to_skip) = match pending.user_boundary.as_deref() {
        Some(boundary) => {
            let index = messages.iter().position(|message| {
                message["role"].as_str() == Some("user")
                    && ["entryId", "messageId", "id"]
                        .iter()
                        .any(|key| message[*key].as_str() == Some(boundary))
            })?;
            (index + 1, 0)
        }
        None => (0, pending.users_before),
    };
    messages[start..]
        .iter()
        .filter(|message| message["role"].as_str() == Some("user"))
        .skip(users_to_skip)
        .find(|message| {
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
            let same_images = pending.content.as_ref().is_none_or(|expected| {
                let images = |content: &Value| {
                    content
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|block| block["type"] == "image")
                        .map(|block| (block["mimeType"].clone(), block["data"].clone()))
                        .collect::<Vec<_>>()
                };
                images(expected) == images(content)
            });
            text == pending.text && same_images
        })
}

#[cfg(test)]
pub fn echoed(pending: &PendingReply, messages: &[Value]) -> bool {
    echo_message(pending, messages).is_some()
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
    let Some(echo) = echo_message(pending, messages) else {
        return false;
    };
    messages
        .iter()
        .skip_while(|message| !std::ptr::eq(*message, echo))
        .skip(1)
        .any(|message| {
            message["role"].as_str() == Some("assistant")
                && (message["content"]
                    .as_str()
                    .is_some_and(|text| !text.is_empty())
                    || message["content"].as_array().is_some_and(|blocks| {
                        blocks.iter().any(|block| {
                            block["text"].as_str().is_some_and(|text| !text.is_empty())
                                || block["type"].as_str().is_some_and(|kind| kind != "text")
                        })
                    }))
        })
}

#[component]
pub fn Spinner(
    #[props(default)] scope: Option<String>,
    #[props(default = "activity-spinner".to_owned())] class: String,
    #[props(default)] title: Option<String>,
) -> Element {
    let instance = use_hook(|| NEXT_SPINNER.fetch_add(1, Ordering::Relaxed));
    let id = format!("rpi-output-spinner-{instance}");
    let rate = super::output_motion::use_rate(scope);
    let mount_id = serde_json::to_string(&id).unwrap();
    let cleanup_id = mount_id.clone();
    use_drop(move || {
        let _ = document::eval(&format!("window.__rpiOutputMotion?.get({cleanup_id})?.();"));
    });
    rsx! {
        span {
            id, class, title, aria_hidden: "true", "data-output-rate": "{rate:.2}",
            onmounted: move |_| {
                let _ = document::eval(&format!("const id={mount_id};{OUTPUT_MOTION}"));
            }
        }
    }
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
            draft_text: None,
            content: None,
            users_before: 1,
            user_boundary: None,
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

    #[test]
    fn prepended_history_does_not_echo_until_a_new_user_arrives_after_boundary() {
        for key in ["entryId", "messageId", "id"] {
            let pending = PendingReply {
                text: "继续".into(),
                draft_text: None,
                content: None,
                users_before: 1,
                user_boundary: Some("old-user".into()),
                cursor_before: None,
                accepted: true,
                responded: false,
            };
            let mut old_user = serde_json::json!({"role":"user","content":"继续"});
            old_user[key] = "old-user".into();
            let assistant = serde_json::json!({"role":"assistant","content":"旧回复"});
            let mut messages = vec![old_user, assistant.clone()];
            assert!(echo_message(&pending, &messages).is_none());
            assert!(!replied_in_history(&pending, &messages));

            messages.insert(
                0,
                serde_json::json!({"role":"user","id":"earlier-user","content":"继续"}),
            );
            assert!(!echoed(&pending, &messages));
            assert!(!replied_in_history(&pending, &messages));

            messages.push(serde_json::json!({
                "role":"user","id":"new-user","content":[{"type":"text","text":"继续"}]
            }));
            assert!(echoed(&pending, &messages));
            assert!(std::ptr::eq(
                echo_message(&pending, &messages).unwrap(),
                &raw const messages[3]
            ));
            assert!(!replied_in_history(&pending, &messages));

            messages.push(assistant);
            assert!(replied_in_history(&pending, &messages));
        }
    }

    #[test]
    fn matching_caption_or_filename_alone_cannot_acknowledge_a_different_image() {
        let content = serde_json::json!([
            {"type":"text","text":"[图片附件：photo.png]"},
            {"type":"image","mimeType":"image/png","data":"actual-image"}
        ]);
        let pending = PendingReply {
            text: "[图片附件：photo.png]".into(),
            draft_text: Some(String::new()),
            content: Some(content.clone()),
            users_before: 0,
            user_boundary: None,
            cursor_before: None,
            accepted: true,
            responded: true,
        };
        let mut other = content.clone();
        other[1]["data"] = serde_json::json!("other-image");
        assert!(!echoed(
            &pending,
            &[serde_json::json!({"role":"user","content":other})]
        ));
        assert!(!echoed(
            &pending,
            &[serde_json::json!({"role":"user","content":pending.text})]
        ));
        assert!(echoed(
            &pending,
            &[serde_json::json!({"role":"user","content":content})]
        ));
    }

    #[test]
    fn missing_user_boundary_never_falls_back_to_old_identical_text() {
        let pending = PendingReply {
            text: "继续".into(),
            draft_text: None,
            content: None,
            users_before: 1,
            user_boundary: Some("missing-user".into()),
            cursor_before: None,
            accepted: true,
            responded: false,
        };
        let messages = vec![
            serde_json::json!({"role":"user","id":"earlier-user","content":"继续"}),
            serde_json::json!({"role":"assistant","id":"missing-user","content":"旧回复"}),
            serde_json::json!({"role":"user","id":"old-user","content":"继续"}),
            serde_json::json!({"role":"assistant","content":"旧回复"}),
        ];
        assert!(echo_message(&pending, &messages).is_none());
        assert!(!echoed(&pending, &messages));
        assert!(!replied_in_history(&pending, &messages));

        let legacy = PendingReply {
            user_boundary: None,
            ..pending
        };
        assert!(std::ptr::eq(
            echo_message(&legacy, &messages).unwrap(),
            &raw const messages[2]
        ));
        assert!(replied_in_history(&legacy, &messages));
    }
}
