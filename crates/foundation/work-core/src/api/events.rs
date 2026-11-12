//! Pure wire decoding. No connections, host ownership or history reconciliation.
use serde_json::Value;

use super::{EventDecodeError, WireEvent};
use crate::{ConnectionState, Event, EventKind, OutputMode, SettlementOutcome, Usage};

pub use work_node_api::api::events::{dialog_from_wire, wire_event};

pub trait WireEventProjection {
    /// Decode the wire payload into a UI event.
    ///
    /// # Errors
    /// Returns an error when a required event field is invalid.
    fn into_core_event(self) -> Result<Event, EventDecodeError>;
}

impl WireEventProjection for WireEvent {
    /// Preserve the wire identity while normalizing the event payload.
    ///
    /// # Errors
    /// Returns an error for an invalid required event field.
    fn into_core_event(self) -> Result<Event, EventDecodeError> {
        let id = self
            .id
            .clone()
            .or_else(|| self.seq.map(|seq| format!("seq-{seq}")))
            .unwrap_or_else(|| {
                let fingerprint = serde_json::to_string(&self.data).unwrap_or_default();
                format!("event-{}-{fingerprint}", self.event_type)
            });
        let seq = self.seq;
        let kind = normalize_event(&self.event_type, &self.data)?;
        Ok(Event { id, seq, kind })
    }
}

/// Convert agent wire event names to the transport-neutral core model.
///
/// # Errors
/// Returns an error when a tool event has no string `toolCallId`.
#[allow(clippy::too_many_lines)]
pub fn normalize_event(event_type: &str, data: &Value) -> Result<EventKind, EventDecodeError> {
    let object = data.as_object();
    let string = |key: &str| object.and_then(|map| map.get(key)).and_then(Value::as_str);
    let value_text = |value: Option<&Value>| -> Result<String, EventDecodeError> {
        Ok(value.map(display_text).unwrap_or_default())
    };
    let message = || {
        object
            .and_then(|map| map.get("message"))
            .and_then(Value::as_object)
    };
    let message_id = || {
        message()
            .and_then(|map| {
                map.get("entryId")
                    .or_else(|| map.get("messageId"))
                    .or_else(|| map.get("id"))
            })
            .and_then(Value::as_str)
            .or_else(|| string("messageId"))
            .or_else(|| string("entryId"))
            .unwrap_or("assistant-message")
    };
    if matches!(
        event_type,
        "message_start" | "message_update" | "message_end"
    ) && message()
        .and_then(|map| map.get("role").or_else(|| map.get("kind")))
        .and_then(Value::as_str)
        .is_some_and(|role| role != "assistant")
    {
        return Ok(EventKind::Ignored);
    }

    match event_type {
        // Subscription readiness confirms the channel, not a newly accepted agent run.
        // Treating it as Accepted makes an idle/reopened chat show Stop and
        // sends the first user message to the queue instead of starting a run.
        "operation_accepted" | "agent_start" => Ok(EventKind::Accepted),
        "agent_end" => Ok(EventKind::AgentEnd),
        "agent_settled" => Ok(EventKind::AgentSettled {
            outcome: settlement(string("outcome"), data),
            usage: usage(object.and_then(|map| map.get("usage"))),
        }),
        "resync_required" => Ok(EventKind::ResyncRequired {
            reason: string("reason")
                .unwrap_or("event history requires a snapshot")
                .into(),
        }),
        "ui_dialog_request" => Ok(EventKind::AwaitingInput),
        "tool_execution_start" => Ok(EventKind::ToolStarted {
            tool_id: required_string(object, "toolCallId")?,
            name: string("toolName").unwrap_or("tool").into(),
            arguments: value_text(
                object.and_then(|map| map.get("args").or_else(|| map.get("arguments"))),
            )?,
        }),
        "tool_execution_update" => Ok(EventKind::ToolOutput {
            tool_id: required_string(object, "toolCallId")?,
            content: object
                .and_then(|map| map.get("partialResult"))
                .map(tool_result_text)
                .unwrap_or_default(),
            mode: OutputMode::Snapshot,
        }),
        "tool_execution_end" => {
            let error = string("error").map(str::to_owned).or_else(|| {
                object.and_then(|map| map.get("isError")).and_then(|value| {
                    value
                        .as_bool()
                        .filter(|is_error| *is_error)
                        .map(|_| "tool failed".into())
                })
            });
            Ok(EventKind::ToolFinished {
                tool_id: required_string(object, "toolCallId")?,
                result: object
                    .and_then(|map| map.get("result"))
                    .map(tool_result_text)
                    .unwrap_or_default(),
                error,
            })
        }
        "message_update" => {
            let event_kind = string("eventType").or_else(|| {
                object
                    .and_then(|map| map.get("assistantMessageEvent"))
                    .and_then(|value| value.get("type"))
                    .and_then(Value::as_str)
            });
            if matches!(event_kind, Some("text_delta")) {
                return Ok(EventKind::TextDelta {
                    block_id: format!(
                        "{}-text-{}",
                        message_id(),
                        object
                            .and_then(|map| map.get("contentIndex"))
                            .and_then(Value::as_u64)
                            .unwrap_or(0)
                    ),
                    delta: object
                        .and_then(|map| map.get("delta"))
                        .map(content_text)
                        .unwrap_or_default(),
                });
            }
            if matches!(event_kind, Some("thinking_delta")) {
                return Ok(EventKind::ThinkingDelta {
                    block_id: format!(
                        "{}-thinking-{}",
                        message_id(),
                        object
                            .and_then(|map| map.get("contentIndex"))
                            .and_then(Value::as_u64)
                            .unwrap_or(0)
                    ),
                    delta: object
                        .and_then(|map| map.get("delta"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                });
            }
            if matches!(event_kind, Some("tool_call_delta")) {
                return Ok(EventKind::Ignored);
            }
            Ok(message_text_snapshot(
                message_id(),
                message().and_then(|map| map.get("content")),
                false,
            ))
        }
        "message_start" | "message_end" => Ok(message_text_snapshot(
            message_id(),
            message().and_then(|map| map.get("content")),
            event_type == "message_end",
        )),
        "connection_reconnecting" => {
            Ok(EventKind::ConnectionChanged(ConnectionState::Reconnecting))
        }
        "ready" | "connection_online" => Ok(EventKind::ConnectionChanged(ConnectionState::Online)),
        "connection_offline" => Ok(EventKind::ConnectionChanged(ConnectionState::Offline)),
        _ => Ok(EventKind::Ignored),
    }
}

fn message_text_snapshot(message_id: &str, content: Option<&Value>, complete: bool) -> EventKind {
    if let Some(Value::Array(items)) = content {
        EventKind::TextSnapshots {
            message_id: message_id.into(),
            snapshots: items
                .iter()
                .enumerate()
                .filter(|(_, item)| is_public_text_content(item))
                .map(|(index, item)| (format!("{message_id}-text-{index}"), content_text(item)))
                .collect(),
            thinking: items
                .iter()
                .enumerate()
                .filter_map(|(index, item)| {
                    let text = thinking_text(item);
                    (!text.is_empty()).then(|| (format!("{message_id}-thinking-{index}"), text))
                })
                .collect(),
            // An empty start/update is commonly a stream placeholder, not deletion.
            authoritative: !items.is_empty() || complete,
        }
    } else {
        // Preserve scalar/legacy slot-zero semantics, including absent content.
        EventKind::TextSnapshot {
            block_id: format!("{message_id}-text-0"),
            text: content.map(content_text).unwrap_or_default(),
        }
    }
}

/// Reasoning blocks carry their text in `thinking`; never surface redacted
/// signatures or any other opaque reasoning payload.
fn thinking_text(value: &Value) -> String {
    if value.get("type").and_then(Value::as_str) != Some("thinking")
        || value.get("redacted").and_then(Value::as_bool) == Some(true)
    {
        return String::new();
    }
    value
        .get("thinking")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn required_string(
    object: Option<&serde_json::Map<String, Value>>,
    key: &str,
) -> Result<String, EventDecodeError> {
    object
        .and_then(|map| map.get(key))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| EventDecodeError::missing_field(key))
}

/// Interpret an observed settlement without treating `agent_end` as completion.
#[must_use]
pub fn settlement(value: Option<&str>, data: &Value) -> SettlementOutcome {
    let value = value.or_else(|| {
        data.get("aborted")
            .and_then(|aborted| aborted.as_bool().filter(|value| *value).map(|_| "aborted"))
    });
    match value.unwrap_or("unknown") {
        "completed" => SettlementOutcome::Completed,
        "failed" => SettlementOutcome::Failed,
        "interrupted" | "aborted" => SettlementOutcome::Interrupted,
        "suspended" => SettlementOutcome::Suspended,
        "compacted" => SettlementOutcome::Compacted,
        "declined" | "skipped" => SettlementOutcome::Declined,
        other => SettlementOutcome::Unknown(if other == "unknown" {
            data.get("outcome")
                .and_then(Value::as_str)
                .unwrap_or(other)
                .to_owned()
        } else {
            other.to_owned()
        }),
    }
}

/// Read the supported wire usage aliases without inventing missing counts.
#[must_use]
pub fn usage(value: Option<&Value>) -> Option<Usage> {
    let object = value?.as_object()?;
    Some(Usage {
        input_tokens: number(object, "input").or_else(|| number(object, "inputTokens")),
        output_tokens: number(object, "output").or_else(|| number(object, "outputTokens")),
        cache_read_tokens: number(object, "cache_read").or_else(|| number(object, "cacheRead")),
        cache_write_tokens: number(object, "cache_write").or_else(|| number(object, "cacheWrite")),
    })
}

fn number(object: &serde_json::Map<String, Value>, key: &str) -> Option<u64> {
    object.get(key).and_then(Value::as_u64)
}

// Public assistant text is an allowlist: never stringify unknown content
// blocks, since images and private reasoning can carry non-public payloads.
/// Extract allowlisted public text, never images or private reasoning.
#[must_use]
pub fn content_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(content_text).collect(),
        Value::Object(object) if is_public_text_content(value) => object
            .get("content")
            .or_else(|| object.get("text"))
            .map(content_text)
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn is_public_text_content(value: &Value) -> bool {
    match value {
        Value::String(_) | Value::Array(_) => true,
        Value::Object(object) => {
            !object.contains_key("thinking")
                && match object.get("type") {
                    None => true,
                    Some(Value::String(kind)) => {
                        matches!(kind.as_str(), "text" | "input_text" | "output_text")
                    }
                    Some(_) => false,
                }
        }
        _ => false,
    }
}

fn is_image_content(value: &Value) -> bool {
    value.get("type").and_then(Value::as_str) == Some("image")
}

// Inspect only content blocks, not arbitrary nested tool metadata/arguments.
/// Inspect content blocks for images without inspecting arbitrary tool metadata.
#[must_use]
pub fn has_image_content(value: &Value) -> bool {
    let content = match value {
        Value::Object(object) if !object.contains_key("type") => {
            object.get("content").unwrap_or(value)
        }
        _ => value,
    };
    match content {
        Value::Array(items) => items.iter().any(is_image_content),
        _ => is_image_content(content),
    }
}

/// Recognize tool-call content blocks without inferring execution or availability.
#[must_use]
pub fn has_tool_call_content(value: &Value) -> bool {
    value.as_array().is_some_and(|items| {
        items.iter().any(|item| {
            matches!(
                item.get("type").and_then(Value::as_str),
                Some("toolCall" | "tool_call" | "tool_use")
            )
        })
    })
}

fn tool_result_text(value: &Value) -> String {
    if has_image_content(value) {
        // This is an internal transport string, not display/copy text. Retain
        // the complete result for the UI to parse and render image blocks.
        serde_json::to_string(value).unwrap_or_default()
    } else {
        display_text(value)
    }
}

fn display_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(display_text).collect(),
        Value::Object(object)
            if object.contains_key("thinking")
                || matches!(
                    object.get("type").and_then(Value::as_str),
                    Some("image" | "thinking" | "reasoning")
                ) =>
        {
            String::new()
        }
        Value::Object(object) if object.contains_key("content") || object.contains_key("text") => {
            object
                .get("content")
                .or_else(|| object.get("text"))
                .map(display_text)
                .unwrap_or_default()
        }
        Value::Object(_) | Value::Number(_) | Value::Bool(_) => {
            serde_json::to_string(value).unwrap_or_default()
        }
        Value::Null => String::new(),
    }
}
