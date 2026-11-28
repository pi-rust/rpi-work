//! Read-only conversation presentation; execution remains in the SDK projection.
use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;
use work_core::api::events::{content_text, has_image_content};
use work_core::{ConnectionState, MessageBlock, RunPhase, SettlementOutcome, ToolStatus};

use crate::session_controls::PendingRequests;
use crate::transcript::{OrderedRow, RowSource, history_id, message_id};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationView {
    pub observation: &'static str,
    pub observation_tone: &'static str,
    pub parts: Vec<Part>,
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Part {
    Message {
        key: String,
        #[serde(rename = "readingKey", skip_serializing_if = "Option::is_none")]
        reading_key: Option<String>,
        #[serde(rename = "forkEntryId", skip_serializing_if = "Option::is_none")]
        fork_entry_id: Option<String>,
        #[serde(rename = "ratingEntryId", skip_serializing_if = "Option::is_none")]
        rating_entry_id: Option<String>,
        role: &'static str,
        text: String,
        #[serde(skip)]
        attachments: Vec<ContentRef>,
    },
    Activity {
        key: String,
        label: &'static str,
        collapsed: bool,
        attention: bool,
        items: Vec<ActivityItem>,
    },
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ActivityItem {
    Tool {
        key: String,
        block: ToolView,
    },
    Note {
        key: String,
        label: &'static str,
        text: String,
    },
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolView {
    pub id: String,
    pub name: String,
    pub arguments: String,
    pub output: String,
    pub status: &'static str,
    pub status_label: &'static str,
    #[serde(skip)]
    pub attachments: Vec<ContentRef>,
}

/// A presentation-only reference to a top-level content block in history.
#[derive(Clone, Copy)]
pub struct ContentRef {
    message: usize,
    block: Option<usize>,
}

impl ContentRef {
    #[must_use]
    pub fn get(self, history: &[Value]) -> Option<&Value> {
        let content = history.get(self.message)?.get("content")?;
        match self.block {
            Some(index) => content.get(index),
            None => Some(content),
        }
    }
}

type AttachmentFilter = fn(&Value, &str) -> bool;

impl ActivityItem {
    fn key(&self) -> &str {
        match self {
            Self::Tool { key, .. } | Self::Note { key, .. } => key,
        }
    }
}

/// Group adjacent activity only, never claim that a display group is a run.
/// Completed tools alone cannot collapse a still-running or uncertain request.
#[must_use]
pub fn present(
    rows: &[OrderedRow],
    history: &[Value],
    blocks: &[MessageBlock],
    phase: Option<&RunPhase>,
    connection: Option<&ConnectionState>,
    pending: PendingRequests,
) -> ConversationView {
    present_with_attachments(rows, history, blocks, phase, connection, pending, None)
}

/// Allow a renderer to claim specific content blocks without serializing their payloads.
#[must_use]
pub fn present_with_attachments(
    rows: &[OrderedRow],
    history: &[Value],
    blocks: &[MessageBlock],
    phase: Option<&RunPhase>,
    connection: Option<&ConnectionState>,
    pending: PendingRequests,
    attachment: Option<AttachmentFilter>,
) -> ConversationView {
    let (observation, observation_tone, collapsed) = observation(phase, connection, pending);
    let results: HashMap<_, _> = history
        .iter()
        .enumerate()
        .filter(|(_, message)| {
            matches!(
                message.get("role").and_then(Value::as_str),
                Some("tool" | "toolResult" | "tool_result")
            )
        })
        .filter_map(|(index, message)| {
            Some((message.get("toolCallId")?.as_str()?, (index, message)))
        })
        .collect();
    let mut builder = Builder {
        parts: Vec::new(),
        activity: Vec::new(),
        seen_tools: HashSet::new(),
        collapsed,
        attachment,
    };
    for row in rows {
        match row.source {
            RowSource::History(index) => {
                if let Some(message) = history.get(index) {
                    builder.history(message, index, row.key, &results);
                }
            }
            RowSource::Projection(index) => {
                if let Some(block) = blocks.get(index) {
                    builder.block(block, row.key);
                }
            }
        }
    }
    builder.flush();
    ConversationView {
        observation,
        observation_tone,
        parts: builder.parts,
    }
}

fn observation(
    phase: Option<&RunPhase>,
    connection: Option<&ConnectionState>,
    pending: PendingRequests,
) -> (&'static str, &'static str, bool) {
    if connection.is_some_and(|value| *value != ConnectionState::Online) {
        return ("连接恢复中，等待新观察", "warning", false);
    }
    if pending.uncertain {
        return ("请求结果尚未核实", "warning", false);
    }
    if pending.configuring {
        return ("正在核对会话配置", "active", false);
    }
    if pending.submitting || pending.awaiting_reply || pending.cancelling {
        return ("请求尚待运行结算观察", "active", false);
    }
    let (label, tone) = match phase {
        None => ("尚无运行阶段观察", "muted"),
        Some(RunPhase::Ready) => ("会话就绪", "muted"),
        Some(RunPhase::Accepted | RunPhase::Preparing | RunPhase::Text | RunPhase::Tool) => {
            ("运行中", "active")
        }
        Some(RunPhase::AwaitingInput) => ("等待输入", "warning"),
        Some(RunPhase::AwaitingSettlement) => ("等待结算", "active"),
        Some(RunPhase::Cancelling) => ("停止请求处理中", "warning"),
        Some(RunPhase::Settled(outcome)) => match outcome {
            SettlementOutcome::Completed => ("已完成", "success"),
            SettlementOutcome::Failed => ("运行失败", "danger"),
            SettlementOutcome::Interrupted => ("已中断", "warning"),
            SettlementOutcome::Suspended => ("已挂起", "warning"),
            SettlementOutcome::Compacted => ("已压缩", "muted"),
            SettlementOutcome::Declined => ("已拒绝", "warning"),
            SettlementOutcome::Unknown(_) => ("结算结果未知", "warning"),
        },
    };
    (
        label,
        tone,
        matches!(phase, Some(RunPhase::Settled(SettlementOutcome::Completed))),
    )
}

struct Builder {
    parts: Vec<Part>,
    activity: Vec<ActivityItem>,
    seen_tools: HashSet<String>,
    collapsed: bool,
    attachment: Option<AttachmentFilter>,
}
impl Builder {
    fn attachment_refs(&self, content: &Value, message: usize, role: &str) -> Vec<ContentRef> {
        let Some(claim) = self.attachment else {
            return Vec::new();
        };
        match content {
            Value::Array(blocks) => blocks
                .iter()
                .enumerate()
                .filter(|(_, value)| claim(value, role))
                .map(|(block, _)| ContentRef {
                    message,
                    block: Some(block),
                })
                .collect(),
            value if claim(value, role) => vec![ContentRef {
                message,
                block: None,
            }],
            _ => Vec::new(),
        }
    }

    fn public_text(&self, content: &Value, role: &str) -> String {
        match self.attachment {
            Some(claim) if claim(content, role) => String::new(),
            Some(claim) if content.is_array() => content
                .as_array()
                .into_iter()
                .flatten()
                .filter(|value| !claim(value, role))
                .map(content_text)
                .collect(),
            _ => content_text(content),
        }
    }

    fn flush(&mut self) {
        if let Some(first) = self.activity.first() {
            let attention = self.activity.iter().any(|item| matches!(item, ActivityItem::Tool { block, .. } if block.status != "completed"));
            self.parts.push(Part::Activity {
                key: format!("activity/{}", first.key()),
                label: "工具与思考过程",
                collapsed: self.collapsed && !attention,
                attention,
                items: std::mem::take(&mut self.activity),
            });
        }
    }
    fn text(
        &mut self,
        key: String,
        reading_key: Option<String>,
        fork_entry_id: Option<&str>,
        role: &'static str,
        text: String,
        attachments: Vec<ContentRef>,
    ) {
        if !text.trim().is_empty() || !attachments.is_empty() {
            self.flush();
            self.parts.push(Part::Message {
                key,
                reading_key,
                fork_entry_id: fork_entry_id.map(str::to_owned),
                rating_entry_id: None,
                role,
                text,
                attachments,
            });
        }
    }
    fn tool(&mut self, block: ToolView) {
        if self.seen_tools.insert(block.id.clone()) {
            self.activity.push(ActivityItem::Tool {
                key: format!("tool/{}", block.id),
                block,
            });
        }
    }
    fn history(
        &mut self,
        message: &Value,
        index: usize,
        key: u64,
        results: &HashMap<&str, (usize, &Value)>,
    ) {
        let role = match message.get("role").and_then(Value::as_str) {
            Some("user") => "user",
            Some("assistant") => "assistant",
            _ => return,
        };
        let content = &message["content"];
        let id = history_id(message, index);
        // Ordered row keys and index-based fallback IDs are local to a view.
        let durable = message_id(message).is_some_and(|id| !id.is_empty());
        let fork_entry = (role == "assistant")
            .then(|| work_client::canonical_entry_id(message))
            .flatten();
        let first_part = self.parts.len();
        if role == "user" || !content.is_array() {
            let (identity, reading_key) = if role == "user" {
                (format!("row/{key}"), durable.then(|| format!("user/{id}")))
            } else {
                let identity = format!("text/{id}-text-0");
                let reading_key = durable.then(|| identity.clone());
                (identity, reading_key)
            };
            let mut text = self.public_text(content, role);
            if self.attachment.is_none() && role == "user" && has_image_content(content) {
                text.truncate(text.trim_end().len());
                if !text.is_empty() {
                    text.push_str("\n\n");
                }
                text.push_str("[图片附件]");
            }
            let attachments = self.attachment_refs(content, index, role);
            self.text(identity, reading_key, fork_entry, role, text, attachments);
            self.mark_rating(first_part, fork_entry);
            return;
        }
        for (part, value) in content.as_array().into_iter().flatten().enumerate() {
            match value.get("type").and_then(Value::as_str) {
                Some("toolCall" | "tool_call" | "tool_use") => {
                    let Some(call) = value
                        .get("id")
                        .or_else(|| value.get("toolCallId"))
                        .and_then(Value::as_str)
                    else {
                        continue;
                    };
                    let result = results.get(call).copied();
                    let status = match result
                        .and_then(|(_, result)| result.get("isError"))
                        .and_then(Value::as_bool)
                    {
                        Some(true) => "failed",
                        Some(false) => "completed",
                        None => "unknown",
                    };
                    self.tool(ToolView {
                        id: call.into(),
                        name: value
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("未提供工具名称")
                            .into(),
                        arguments: value
                            .get("arguments")
                            .or_else(|| value.get("input"))
                            .map_or_else(String::new, |args| {
                                args.as_str()
                                    .map_or_else(|| args.to_string(), str::to_owned)
                            }),
                        output: result.map_or_else(String::new, |(_, result)| {
                            self.public_text(&result["content"], "tool")
                        }),
                        status,
                        status_label: tool_label(status),
                        attachments: result.map_or_else(Vec::new, |(index, result)| {
                            self.attachment_refs(&result["content"], index, "tool")
                        }),
                    });
                }
                Some("thinking")
                    if value.get("redacted").and_then(Value::as_bool) != Some(true) =>
                {
                    if let Some(text) = value
                        .get("thinking")
                        .and_then(Value::as_str)
                        .filter(|text| !text.is_empty())
                    {
                        self.activity.push(ActivityItem::Note {
                            key: format!("{id}-thinking-{part}"),
                            label: "思考过程",
                            text: text.into(),
                        });
                    }
                }
                _ => {
                    let identity = format!("text/{id}-text-{part}");
                    let reading_key = durable.then(|| identity.clone());
                    let attachments = if self.attachment.is_some_and(|claim| claim(value, role)) {
                        vec![ContentRef {
                            message: index,
                            block: Some(part),
                        }]
                    } else {
                        Vec::new()
                    };
                    let text = if attachments.is_empty() {
                        content_text(value)
                    } else {
                        String::new()
                    };
                    self.text(identity, reading_key, fork_entry, role, text, attachments);
                }
            }
        }
        self.mark_rating(first_part, fork_entry);
    }
    // A canonical entry may yield several message parts around tool calls.
    // Give the complete reply one rating target at its last visible part.
    fn mark_rating(&mut self, first_part: usize, entry_id: Option<&str>) {
        if let Some(id) = entry_id
            && let Some(Part::Message {
                rating_entry_id, ..
            }) = self.parts[first_part..].last_mut()
        {
            *rating_entry_id = Some(id.to_owned());
        }
    }
    fn block(&mut self, block: &MessageBlock, _key: u64) {
        match block {
            MessageBlock::Text { id, text } | MessageBlock::PublicSummary { id, text } => {
                self.text(
                    format!("text/{id}"),
                    None,
                    None,
                    "assistant",
                    text.clone(),
                    Vec::new(),
                );
            }
            MessageBlock::Thinking { id, text } => self.activity.push(ActivityItem::Note {
                key: id.clone(),
                label: "思考过程",
                text: text.clone(),
            }),
            MessageBlock::ToolCall {
                id,
                name,
                arguments,
                output,
                status,
            } => {
                let status = match status {
                    ToolStatus::Running => "running",
                    ToolStatus::Completed => "completed",
                    ToolStatus::Failed => "failed",
                    ToolStatus::Cancelled => "cancelled",
                };
                self.tool(ToolView {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                    output: output.clone(),
                    status,
                    status_label: tool_label(status),
                    attachments: Vec::new(),
                });
            }
        }
    }
}
fn tool_label(status: &str) -> &'static str {
    match status {
        "running" => "运行中",
        "completed" => "已完成",
        "failed" => "失败",
        "cancelled" => "已取消",
        _ => "结果未观察",
    }
}

#[cfg(test)]
mod tests;
