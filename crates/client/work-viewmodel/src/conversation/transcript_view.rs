//! Read-only conversation presentation; execution remains in the SDK projection.
use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;
use work_core::api::events::content_text;
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
        role: &'static str,
        text: String,
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
}
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
    let (observation, observation_tone, collapsed) = observation(phase, connection, pending);
    let results: HashMap<_, _> = history
        .iter()
        .filter(|message| {
            matches!(
                message.get("role").and_then(Value::as_str),
                Some("tool" | "toolResult" | "tool_result")
            )
        })
        .filter_map(|message| Some((message.get("toolCallId")?.as_str()?, message)))
        .collect();
    let mut builder = Builder {
        parts: Vec::new(),
        activity: Vec::new(),
        seen_tools: HashSet::new(),
        collapsed,
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
}
impl Builder {
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
    ) {
        if !text.trim().is_empty() {
            self.flush();
            self.parts.push(Part::Message {
                key,
                reading_key,
                fork_entry_id: fork_entry_id.map(str::to_owned),
                role,
                text,
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
        results: &HashMap<&str, &Value>,
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
        if role == "user" || !content.is_array() {
            let (identity, reading_key) = if role == "user" {
                (format!("row/{key}"), durable.then(|| format!("user/{id}")))
            } else {
                let identity = format!("text/{id}-text-0");
                let reading_key = durable.then(|| identity.clone());
                (identity, reading_key)
            };
            self.text(
                identity,
                reading_key,
                fork_entry,
                role,
                content_text(content),
            );
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
                        .and_then(|result| result.get("isError"))
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
                        output: result
                            .map_or_else(String::new, |result| content_text(&result["content"])),
                        status,
                        status_label: tool_label(status),
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
                    self.text(identity, reading_key, fork_entry, role, content_text(value));
                }
            }
        }
    }
    fn block(&mut self, block: &MessageBlock, _key: u64) {
        match block {
            MessageBlock::Text { id, text } | MessageBlock::PublicSummary { id, text } => {
                self.text(format!("text/{id}"), None, None, "assistant", text.clone());
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
