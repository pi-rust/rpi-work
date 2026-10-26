//! In-memory client observations. No persistence, provider requests or exporter.
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::rc::Rc;
use work_client::WireEvent;
use work_core::{MessageBlock, ToolStatus};

pub const CSS: &str = include_str!("traces.css");
const TEXT_LIMIT: usize = 16 * 1024;
const MAX_SPANS: usize = 192;
const MAX_RUNS: usize = 32;
const MAX_SCOPES: usize = 16;
const MAX_SEEN: usize = 8192;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TraceStore {
    pub scopes: BTreeMap<String, TraceSession>,
    serial: u64,
    // A replacement store must reject callbacks issued by its predecessor.
    lifetime: Rc<()>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TraceSession {
    pub runs: Vec<TraceRun>,
    pub notice: Option<String>,
    /// Confirmed client resource snapshots awaiting a real run boundary.
    pending_resource_snapshots: Vec<Value>,
    /// Optional integration state while an application history request is pending.
    pub loading: bool,
    pub error: Option<String>,
    active: Option<u64>,
    cursor: Option<u64>,
    seen: HashSet<String>,
    seen_order: VecDeque<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraceRun {
    pub id: String,
    pub server_run_id: Option<String>,
    pub name: String,
    pub source: String,
    pub status: String,
    pub raw_outcome: Option<Value>,
    pub spans: Vec<TraceSpan>,
    pub usage: Option<Value>,
    /// Immutable client metadata for the accepted send; never sent as prompt text.
    pub resource_snapshot: Option<Value>,
    pub clock_failed: bool,
    uid: u64,
    started: Stamp,
    finished: Stamp,
    first_text: Stamp,
    message: Option<String>,
    message_counter: u64,
    agent_started: bool,
    provider: Option<String>,
    model: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraceSpan {
    pub id: String,
    pub parent: Option<String>,
    pub kind: String,
    pub name: String,
    pub status: String,
    pub input: String,
    pub output: String,
    pub error: Option<String>,
    pub usage: Option<Value>,
    started: Stamp,
    finished: Stamp,
    first_text: Stamp,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Stamp {
    ticket: Option<u64>,
    ms: Option<f64>,
}
impl Stamp {
    fn reserve(&mut self, ticket: u64) {
        if self.ticket.is_none() {
            self.ticket = Some(ticket);
        }
    }
    fn patch(&mut self, ticket: u64, ms: Option<f64>) {
        if self.ticket == Some(ticket) {
            self.ms = ms;
        }
    }
}

fn finite(ms: Option<f64>) -> Option<f64> {
    ms.filter(|v| v.is_finite() && *v >= 0.0)
}
fn difference(start: Option<f64>, end: Option<f64>) -> Option<f64> {
    match (finite(start), finite(end)) {
        (Some(a), Some(b)) if b >= a => Some(b - a),
        _ => None,
    }
}
fn duration(ms: Option<f64>) -> String {
    match finite(ms) {
        None => "—".into(),
        Some(v) if v < 1.0 => "<1ms".into(),
        Some(v) if v < 1000.0 => format!("{v:.0}ms"),
        Some(v) if v < 60_000.0 => format!("{:.2}s", v / 1000.0),
        Some(v) => format!("{:.1}m", v / 60_000.0),
    }
}
fn status_label(status: &str) -> &str {
    match status {
        "running" => "进行中",
        "completed" => "已完成",
        "failed" => "失败",
        "interrupted" | "aborted" | "cancelled" => "已停止",
        "awaiting_settlement" => "等待结算",
        "awaiting_input" => "等待输入",
        "suspended" => "已挂起",
        "compacted" => "已压缩",
        "declined" | "skipped" => "未执行",
        "history" => "历史 · 状态未知",
        "incomplete" => "部分内容 · 待核对",
        _ => "待核对",
    }
}
fn kind_label(kind: &str) -> &str {
    match kind {
        "agent" => "Agent",
        "generation" => "模型",
        "tool" => "工具",
        "context" => "上下文",
        "settlement" => "结算",
        "transport" => "连接",
        _ => "命令",
    }
}
fn limited(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit.saturating_sub(32).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[已截断]", &text[..end])
}
fn secret_key(key: &str) -> bool {
    let key: String = key
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect();
    key.contains("password")
        || key.contains("passwd")
        || key.contains("apikey")
        || key.contains("authorization")
        || key.contains("credential")
        || key.contains("secret")
        || key.contains("privatekey")
        || key.contains("cookie")
        || key == "token"
        || key.ends_with("token")
}
fn private_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase().replace(['_', '-'], "");
    matches!(
        key.as_str(),
        "thinking"
            | "reasoning"
            | "reasoningcontent"
            | "signature"
            | "thoughtsignature"
            | "rawrequest"
            | "providerrequest"
            | "requestbody"
            | "rawproviderrequest"
            | "request"
            | "credentials"
    ) || key.starts_with("thinking")
        || key.starts_with("reasoning")
}
// Also redact key/value pairs in free-form tool logs, including Authorization: Bearer … .
fn redact_text(text: &str) -> String {
    let text = limited(text, TEXT_LIMIT);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_alphabetic() {
            let begin = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '_' | '-'))
            {
                i += 1;
            }
            let word: String = chars[begin..i].iter().collect();
            out.push_str(&word);
            if word.eq_ignore_ascii_case("bearer")
                && chars.get(i).is_some_and(|c| c.is_whitespace())
            {
                out.push_str(" [已遮盖]");
                while i < chars.len() && chars[i].is_whitespace() && chars[i] != '\n' {
                    i += 1;
                }
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && !matches!(chars[i], '"' | '\'' | ',' | '}')
                {
                    i += 1;
                }
            } else if secret_key(&word) {
                let mut value = i;
                while value < chars.len() && matches!(chars[value], '"' | '\'' | ' ' | '\t') {
                    value += 1;
                }
                if value < chars.len() && matches!(chars[value], ':' | '=') {
                    value += 1;
                    while value < chars.len() && matches!(chars[value], ' ' | '\t') {
                        value += 1;
                    }
                    out.extend(chars[i..value].iter());
                    let quote = chars
                        .get(value)
                        .copied()
                        .filter(|c| matches!(c, '"' | '\''));
                    if let Some(q) = quote {
                        out.push(q);
                        value += 1;
                    }
                    out.push_str("[已遮盖]");
                    while value < chars.len() {
                        if quote.is_some_and(|q| chars[value] == q)
                            || (quote.is_none()
                                && matches!(chars[value], '\n' | '\r' | ',' | ';' | '}'))
                        {
                            break;
                        }
                        value += 1;
                    }
                    i = value;
                }
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    limited(&out, TEXT_LIMIT)
}
fn safe_value(value: &Value, depth: usize) -> Value {
    if depth > 8 {
        return Value::String("[层级已截断]".into());
    }
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .take(64)
                .filter_map(|(key, value)| {
                    if private_key(key) {
                        return None;
                    }
                    Some((
                        limited(key, 128),
                        if secret_key(key) {
                            Value::String("[已遮盖]".into())
                        } else {
                            safe_value(value, depth + 1)
                        },
                    ))
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .take(64)
                .filter(|v| {
                    !v.get("type")
                        .and_then(Value::as_str)
                        .is_some_and(|t| matches!(t, "thinking" | "reasoning"))
                })
                .map(|v| safe_value(v, depth + 1))
                .collect(),
        ),
        Value::String(text) => Value::String(redact_text(text)),
        other => other.clone(),
    }
}
fn display_value(value: &Value) -> String {
    let clean = safe_value(value, 0);
    match clean {
        Value::Null => String::new(),
        Value::String(s) => s,
        other => limited(
            &serde_json::to_string_pretty(&other).unwrap_or_default(),
            TEXT_LIMIT,
        ),
    }
}
fn clean_text(text: &str) -> String {
    // History arguments and tool logs often contain encoded JSON.
    serde_json::from_str::<Value>(text)
        .ok()
        .filter(|v| !v.is_string())
        .map_or_else(|| redact_text(text), |v| display_value(&v))
}
fn public_text(content: &Value) -> String {
    match content {
        Value::String(text) => clean_text(text),
        Value::Array(items) => limited(
            &items
                .iter()
                .filter(|item| item["type"].as_str() == Some("text"))
                .filter_map(|item| item["text"].as_str())
                .map(clean_text)
                .collect::<Vec<_>>()
                .join("\n"),
            TEXT_LIMIT,
        ),
        _ => String::new(),
    }
}
fn string_at<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .filter(|s| !s.is_empty())
}
fn new_span(id: String, parent: Option<String>, kind: &str, name: &str) -> TraceSpan {
    TraceSpan {
        id,
        parent,
        kind: kind.into(),
        name: clean_text(name),
        status: "running".into(),
        input: String::new(),
        output: String::new(),
        error: None,
        usage: None,
        started: Stamp::default(),
        finished: Stamp::default(),
        first_text: Stamp::default(),
    }
}
impl TraceRun {
    fn new(uid: u64, server: Option<String>, source: &str, name: &str) -> Self {
        Self {
            id: server
                .clone()
                .unwrap_or_else(|| format!("client-observation-{uid}")),
            server_run_id: server,
            name: clean_text(name),
            source: source.into(),
            status: "running".into(),
            raw_outcome: None,
            spans: vec![new_span("root".into(), None, "agent", "Agent 回合")],
            usage: None,
            resource_snapshot: None,
            clock_failed: false,
            uid,
            started: Stamp::default(),
            finished: Stamp::default(),
            first_text: Stamp::default(),
            message: None,
            message_counter: 0,
            agent_started: false,
            provider: None,
            model: None,
        }
    }
    fn span(&mut self, id: &str, parent: &str, kind: &str, name: &str) -> Option<usize> {
        if let Some(index) = self.spans.iter().position(|s| s.id == id) {
            return Some(index);
        }
        if self.spans.len() >= MAX_SPANS {
            return None;
        }
        self.spans
            .push(new_span(id.into(), Some(parent.into()), kind, name));
        Some(self.spans.len() - 1)
    }
    fn end_kind(&mut self, kind: &str, ticket: u64) {
        for span in &mut self.spans {
            if span.kind == kind && span.finished.ticket.is_none() {
                span.finished.reserve(ticket);
                span.status = "completed".into();
            }
        }
    }
    fn elapsed(&self) -> Option<f64> {
        difference(self.started.ms, self.finished.ms)
    }
    fn first(&self) -> Option<f64> {
        difference(self.started.ms, self.first_text.ms)
    }
    fn extent(&self) -> Option<f64> {
        self.spans
            .iter()
            .flat_map(|s| [s.started.ms, s.finished.ms, s.first_text.ms])
            .flatten()
            .chain(self.finished.ms)
            .filter_map(|ms| difference(self.started.ms, Some(ms)))
            .reduce(f64::max)
            .filter(|v| *v > 0.0)
    }
}

pub fn use_trace_store() -> Signal<TraceStore> {
    use_signal(TraceStore::default)
}

#[derive(Clone)]
struct ClockRequest {
    scope: String,
    run: u64,
    ticket: u64,
    lifetime: Rc<()>,
}
impl TraceStore {
    fn serial(&mut self) -> u64 {
        self.serial += 1;
        self.serial
    }
    fn session(&mut self, scope: &str) -> &mut TraceSession {
        if !self.scopes.contains_key(scope) && self.scopes.len() >= MAX_SCOPES {
            // Keep memory bounded; callbacks for evicted runs become no-ops.
            if let Some(key) = self.scopes.keys().next().cloned() {
                self.scopes.remove(&key);
            }
        }
        self.scopes.entry(scope.into()).or_default()
    }
    fn patch_clock(&mut self, request: &ClockRequest, ms: Option<f64>) {
        if !Rc::ptr_eq(&self.lifetime, &request.lifetime) {
            return;
        }
        let Some(session) = self.scopes.get_mut(&request.scope) else {
            return;
        };
        let Some(run) = session.runs.iter_mut().find(|r| r.uid == request.run) else {
            return;
        };
        let ms = finite(ms);
        if ms.is_none() {
            run.clock_failed = true;
        }
        run.started.patch(request.ticket, ms);
        run.finished.patch(request.ticket, ms);
        run.first_text.patch(request.ticket, ms);
        for span in &mut run.spans {
            span.started.patch(request.ticket, ms);
            span.finished.patch(request.ticket, ms);
            span.first_text.patch(request.ticket, ms);
        }
    }
    // Keep deduplication, run attribution and clock reservation in one ordered reducer.
    #[allow(clippy::too_many_lines)]
    fn ingest(&mut self, scope: &str, event: &WireEvent) -> Option<ClockRequest> {
        let kind = event.event_type.as_str();
        let data = &event.data;
        let delta_type = string_at(data, &["eventType"])
            .or_else(|| data["assistantMessageEvent"]["type"].as_str());
        // Do not clone, normalize, fingerprint or save private reasoning payloads.
        if matches!(kind, "thinking_delta" | "reasoning_delta")
            || matches!(
                delta_type,
                Some("thinking_delta" | "thinking_start" | "thinking_end" | "tool_call_delta")
            )
        {
            return None;
        }
        if !matches!(
            kind,
            "agent_start"
                | "operation_accepted"
                | "agent_end"
                | "agent_settled"
                | "message_start"
                | "message_update"
                | "message_end"
                | "tool_execution_start"
                | "tool_execution_update"
                | "tool_execution_end"
                | "phase"
                | "ui_dialog_request"
                | "connection_online"
                | "connection_offline"
                | "connection_reconnecting"
                | "resync_required"
                | "auto_compaction_start"
                | "auto_compaction_end"
                | "compaction_start"
                | "compaction_end"
                | "error"
        ) {
            return None;
        }
        let message = &data["message"];
        let role = string_at(message, &["role", "kind"]);
        if kind.starts_with("message_") && role.is_some_and(|r| r != "assistant") {
            return None;
        }
        let ticket = self.serial();
        let lifetime = self.lifetime.clone();
        let session = self.session(scope);
        let identity = event.id.as_ref().map(|id| limited(id, 256));
        if event
            .seq
            .is_some_and(|seq| session.cursor.is_some_and(|cursor| seq <= cursor))
            || identity
                .as_ref()
                .is_some_and(|id| session.seen.contains(id))
        {
            return None;
        }
        if let Some(seq) = event.seq {
            session.cursor = Some(seq);
        }
        if let Some(id) = identity {
            session.seen.insert(id.clone());
            session.seen_order.push_back(id);
            if session.seen_order.len() > MAX_SEEN {
                if let Some(old) = session.seen_order.pop_front() {
                    session.seen.remove(&old);
                }
            }
        }
        let server = string_at(data, &["runId", "run_id"])
            .or_else(|| string_at(&data["run"], &["id", "runId"]))
            .map(|s| limited(s, 256));
        let existing = server.as_ref().and_then(|id| {
            session
                .runs
                .iter()
                .position(|r| r.server_run_id.as_ref() == Some(id))
        });
        let active = session
            .active
            .and_then(|uid| session.runs.iter().position(|r| r.uid == uid));
        let tool_id = string_at(data, &["toolCallId", "tool_call_id"])
            .map(|id| format!("tool:{}", limited(id, 256)));
        let message_id = string_at(message, &["entryId", "messageId", "id"])
            .or_else(|| string_at(data, &["messageId", "entryId"]))
            .map(|id| format!("generation:{}", limited(id, 256)));
        let observation_id = tool_id.as_ref().or(message_id.as_ref());
        let by_observation = if server.is_none() {
            observation_id.and_then(|id| {
                session
                    .runs
                    .iter()
                    .rposition(|r| r.spans.iter().any(|s| &s.id == id))
            })
        } else {
            None
        };
        let awaiting = if server.is_none() && kind == "agent_settled" {
            let candidates: Vec<_> = session
                .runs
                .iter()
                .enumerate()
                .filter(|(_, r)| r.status == "awaiting_settlement" && r.finished.ticket.is_none())
                .map(|(i, _)| i)
                .collect();
            match candidates.as_slice() {
                [index] => Some(*index),
                [] => None,
                _ => {
                    session.notice =
                        Some("结算缺少 runId，多个运行等待结算，无法可靠归属。".into());
                    return None;
                }
            }
        } else {
            None
        };
        let index = existing.or(by_observation).or(awaiting).or_else(|| {
            active.filter(|index| {
                let r = &session.runs[*index];
                let new_start = kind == "agent_start" && (r.agent_started || server.is_some());
                !new_start
                    && (server.is_none()
                        || (r.server_run_id.is_none() && r.finished.ticket.is_none()))
            })
        });
        let index = if let Some(index) = index.filter(|index| {
            // Known observation IDs keep late events away from a new active run.
            existing.is_some()
                || by_observation.is_some()
                || session.runs[*index].finished.ticket.is_none()
        }) {
            if session.runs[index].server_run_id.is_none() && server.is_some() {
                session.runs[index].id = server.clone().unwrap_or_default();
                session.runs[index].server_run_id.clone_from(&server);
            }
            index
        } else {
            if kind.starts_with("connection_") || kind == "resync_required" {
                session.notice = Some(
                    if kind == "resync_required" {
                        "事件存在缺口；请由集成层恢复历史快照。"
                    } else {
                        "连接状态已变化，尚无可归属的运行。"
                    }
                    .into(),
                );
                return None;
            }
            if session.runs.len() >= MAX_RUNS {
                session.runs.remove(0);
            }
            session
                .runs
                .push(TraceRun::new(ticket, server, "live", "Agent 回合"));
            if let Some(snapshot) = session.pending_resource_snapshots.pop() {
                if let Some(run) = session.runs.last_mut() {
                    run.resource_snapshot = Some(snapshot);
                }
            }
            session.active = Some(ticket);
            session.runs.len() - 1
        };
        let run = &mut session.runs[index];
        if run.finished.ticket.is_some() {
            return None;
        }
        let mut sample = false;
        // Hosts may omit agent_start. The first non-terminal observation is still
        // a real client receive boundary; an isolated settlement cannot invent a
        // start time and therefore remains unknown.
        if run.started.ticket.is_none() && kind != "agent_settled" {
            run.started.reserve(ticket);
            run.spans[0].started.reserve(ticket);
            sample = true;
        }
        let begin_span =
            |run: &mut TraceRun, id: &str, parent: &str, kind: &str, name: &str| -> Option<usize> {
                run.span(id, parent, kind, name)
            };
        match kind {
            "agent_start" | "operation_accepted" => {
                if kind == "agent_start" {
                    run.agent_started = true;
                }
                if let Some(i) = begin_span(run, "context", "root", "context", "准备会话上下文")
                {
                    if run.spans[i].started.ticket.is_none() {
                        run.spans[i].started.reserve(ticket);
                        sample = true;
                    }
                }
                run.provider = string_at(data, &["provider"]).map(clean_text);
                run.model = string_at(data, &["model", "modelId"]).map(clean_text);
            }
            "phase" => {
                let phase = string_at(data, &["phase"]).unwrap_or("unknown");
                if matches!(phase, "preparing" | "context") {
                    if let Some(i) = begin_span(run, "context", "root", "context", "准备会话上下文")
                    {
                        if run.spans[i].started.ticket.is_none() {
                            run.spans[i].started.reserve(ticket);
                            sample = true;
                        }
                        if let Some(summary) = data["summary"].as_str() {
                            run.spans[i].output = clean_text(summary);
                        }
                    }
                }
            }
            "message_start" | "message_update" | "message_end" => {
                let explicit = string_at(message, &["entryId", "messageId", "id"])
                    .or_else(|| string_at(data, &["messageId", "entryId"]));
                let id = if let Some(id) = explicit {
                    limited(id, 256)
                } else if kind != "message_start" && run.message.is_some() {
                    run.message.clone().unwrap_or_default()
                } else {
                    run.message_counter += 1;
                    format!("client-message-{}", run.message_counter)
                };
                run.message = Some(id.clone());
                let span_id = format!("generation:{id}");
                if let Some(i) = begin_span(run, &span_id, "root", "generation", "正文流式阶段")
                {
                    if run.spans[i].started.ticket.is_none() {
                        run.end_kind("context", ticket);
                        run.spans[i].started.reserve(ticket);
                        sample = true;
                    }
                    let incoming = if delta_type == Some("text_delta") {
                        data.get("delta")
                            .or_else(|| data["assistantMessageEvent"].get("delta"))
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    } else {
                        None
                    };
                    if let Some(delta) = incoming {
                        // Re-sanitize the retained prefix plus fragment so split secret keys are covered.
                        let combined =
                            format!("{}{}", run.spans[i].output, limited(&delta, TEXT_LIMIT));
                        run.spans[i].output = clean_text(&combined);
                    } else if message.get("content").is_some() {
                        let text = public_text(&message["content"]);
                        // An empty end snapshot is common; do not erase content
                        // already received through deltas.
                        if !text.is_empty() {
                            run.spans[i].output = text;
                        }
                    }
                    if !run.spans[i].output.is_empty() && run.spans[i].first_text.ticket.is_none() {
                        run.spans[i].first_text.reserve(ticket);
                        run.first_text.reserve(ticket);
                        sample = true;
                    }
                    if kind == "message_end" {
                        run.spans[i].finished.reserve(ticket);
                        run.spans[i].status = "completed".into();
                        run.spans[i].usage = extract_usage(message.get("usage"));
                        if let Some(error) = message.get("errorMessage").and_then(Value::as_str) {
                            run.spans[i].error = Some(clean_text(error));
                            run.spans[i].status = "failed".into();
                        }
                        run.message = None;
                        sample = true;
                    }
                }
            }
            "tool_execution_start" | "tool_execution_update" | "tool_execution_end" => {
                let Some(id) = string_at(data, &["toolCallId", "tool_call_id"]) else {
                    session.notice = Some("工具事件缺少工具 ID，未归并。".into());
                    return None;
                };
                let span_id = format!("tool:{}", limited(id, 256));
                let name = string_at(data, &["toolName", "name"]).unwrap_or("tool");
                let parent = string_at(data, &["parentToolCallId"])
                    .map(|id| format!("tool:{}", limited(id, 256)))
                    .filter(|id| run.spans.iter().any(|s| &s.id == id))
                    .unwrap_or_else(|| "root".into());
                if let Some(i) = begin_span(run, &span_id, &parent, "tool", name) {
                    if kind == "tool_execution_start" {
                        run.end_kind("context", ticket);
                        if run.spans[i].started.ticket.is_none() {
                            run.spans[i].started.reserve(ticket);
                            sample = true;
                        }
                        run.spans[i].input = display_value(
                            data.get("args")
                                .or_else(|| data.get("arguments"))
                                .unwrap_or(&Value::Null),
                        );
                    } else if kind == "tool_execution_update" {
                        if run.spans[i].finished.ticket.is_none() {
                            run.spans[i].output =
                                display_value(data.get("partialResult").unwrap_or(&Value::Null));
                        }
                    } else {
                        if let Some(result) = data.get("result") {
                            let result_text = display_value(result);
                            if !result_text.is_empty() {
                                run.spans[i].output = result_text;
                            }
                        }
                        let error = data
                            .get("error")
                            .filter(|v| !v.is_null())
                            .map(display_value);
                        let failed = data["isError"].as_bool() == Some(true)
                            || data["failed"].as_bool() == Some(true)
                            || error.is_some();
                        run.spans[i].error =
                            error.or_else(|| failed.then(|| "工具执行失败；请查看输出。".into()));
                        run.spans[i].status = if failed { "failed" } else { "completed" }.into();
                        if run.spans[i].finished.ticket.is_none() {
                            run.spans[i].finished.reserve(ticket);
                            sample = true;
                        }
                    }
                }
            }
            "agent_end" => {
                run.status = "awaiting_settlement".into();
                run.spans[0].status.clone_from(&run.status);
                run.end_kind("generation", ticket);
                run.end_kind("context", ticket);
                if let Some(i) =
                    begin_span(run, "settlement", "root", "settlement", "等待运行时结算")
                {
                    run.spans[i].started.reserve(ticket);
                    run.spans[i].input = "agent_end".into();
                }
                sample = true;
            }
            "agent_settled" => {
                let raw = data.get("outcome").cloned().unwrap_or(Value::Null);
                let outcome = raw
                    .as_str()
                    .unwrap_or(if data["aborted"].as_bool() == Some(true) {
                        "aborted"
                    } else {
                        "unknown"
                    });
                run.status = match outcome {
                    "completed" | "failed" | "interrupted" | "aborted" | "suspended"
                    | "compacted" | "declined" | "skipped" => outcome,
                    _ => "settled_unknown",
                }
                .into();
                run.raw_outcome = Some(safe_value(&raw, 0));
                run.usage = extract_usage(data.get("usage"));
                run.finished.reserve(ticket);
                if let Some(i) = begin_span(run, "settlement", "root", "settlement", "运行时结算")
                {
                    run.spans[i].output = display_value(
                        &json!({"outcome":run.raw_outcome,"pendingResume":data.get("pendingResume")}),
                    );
                }
                for span in &mut run.spans {
                    if span.finished.ticket.is_none() {
                        span.finished.reserve(ticket);
                        span.status = if span.id == "root" || span.kind == "settlement" {
                            run.status.clone()
                        } else if run.status == "completed" {
                            "incomplete".into()
                        } else {
                            run.status.clone()
                        };
                    }
                }
                run.spans[0].status.clone_from(&run.status);
                run.spans[0].error = data
                    .get("error")
                    .filter(|v| !v.is_null())
                    .map(display_value);
                sample = true;
            }
            "auto_compaction_start"
            | "auto_compaction_end"
            | "compaction_start"
            | "compaction_end" => {
                let end = kind.ends_with("_end");
                let id = if end {
                    run.spans
                        .iter()
                        .rev()
                        .find(|s| s.id.starts_with("compaction:") && s.finished.ticket.is_none())
                        .map_or_else(|| format!("compaction:{ticket}"), |s| s.id.clone())
                } else {
                    format!("compaction:{ticket}")
                };
                if let Some(i) = begin_span(run, &id, "root", "context", "上下文压缩") {
                    if end {
                        run.spans[i].finished.reserve(ticket);
                        run.spans[i].status = if data["error"].is_null() {
                            "compacted"
                        } else {
                            "failed"
                        }
                        .into();
                        run.spans[i].error = data
                            .get("error")
                            .filter(|v| !v.is_null())
                            .map(display_value);
                        run.spans[i].output =
                            data.get("summary").map(display_value).unwrap_or_default();
                    } else {
                        run.spans[i].started.reserve(ticket);
                    }
                    sample = true;
                }
            }
            "ui_dialog_request" => {
                run.status = "awaiting_input".into();
                run.spans[0].status.clone_from(&run.status);
            }
            "connection_online"
            | "connection_offline"
            | "connection_reconnecting"
            | "resync_required" => {
                if kind == "connection_online" {
                    run.end_kind("transport", ticket);
                    session.notice = None;
                } else {
                    run.end_kind("transport", ticket);
                    if let Some(i) = begin_span(
                        run,
                        &format!("transport:{ticket}"),
                        "root",
                        "transport",
                        if kind == "resync_required" {
                            "事件缺口 · 等待历史恢复"
                        } else {
                            "连接中断 / 重连"
                        },
                    ) {
                        run.spans[i].started.reserve(ticket);
                        run.spans[i].error = data
                            .get("error")
                            .filter(|v| !v.is_null())
                            .map(display_value);
                    }
                    session.notice = Some("连接或事件流不完整；已接收的部分内容仍保留。".into());
                }
                sample = true;
            }
            "error" => {
                run.spans[0].error = data
                    .get("error")
                    .or_else(|| data.get("message"))
                    .map(display_value);
                session.notice = Some("收到错误，等待运行时结算确认最终结果。".into());
            }
            _ => {}
        }
        if run.spans.len() >= MAX_SPANS {
            session.notice = Some("步骤数量已达本地保留上限，后续步骤已省略。".into());
        }
        sample.then(|| ClockRequest {
            scope: scope.into(),
            run: run.uid,
            ticket,
            lifetime,
        })
    }
}

/// Latest observed live-run identity used to disambiguate a send from prior runs.
pub fn live_run_watermark(store: Signal<TraceStore>, scope: &str) -> Option<u64> {
    store.read().scopes.get(scope).and_then(|session| {
        session
            .runs
            .iter()
            .rev()
            .find(|run| run.source == "live")
            .map(|run| run.uid)
    })
}

/// Attach an immutable snapshot only after the prompt was accepted. A run may
/// begin and settle before the HTTP response arrives, so match by its watermark.
pub fn confirm_resource_snapshot(
    mut store: Signal<TraceStore>,
    scope: &str,
    after: Option<u64>,
    snapshot: Value,
) {
    let mut state = store.write();
    let session = state.session(scope);
    if let Some(run) = session.runs.iter_mut().rev().find(|run| {
        run.source == "live"
            && after.is_none_or(|watermark| run.uid > watermark)
            && run.resource_snapshot.is_none()
    }) {
        run.resource_snapshot = Some(snapshot);
    } else {
        session.pending_resource_snapshots.push(snapshot);
        if session.pending_resource_snapshots.len() > 8 {
            session.pending_resource_snapshots.remove(0);
        }
    }
}

/// Synchronous ordering/deduplication; only boundary events schedule a clock read.
pub fn observe_event(mut store: Signal<TraceStore>, scope: &str, event: &WireEvent) {
    let request = store.write().ingest(scope, event);
    if let Some(request) = request {
        // Create the eval now, before yielding, so bridge scripts retain receive order.
        let mut eval = document::eval(
            "try { dioxus.send(performance.now()); } catch (_) { dioxus.send(null); }",
        );
        spawn(async move {
            let ms = eval.recv::<Value>().await.ok().and_then(|v| v.as_f64());
            if let Ok(mut state) = store.try_write() {
                state.patch_clock(&request, ms);
            }
        });
    }
}

pub fn restore_history(mut store: Signal<TraceStore>, scope: &str, blocks: Vec<MessageBlock>) {
    store.write().restore(scope, blocks);
}
impl TraceStore {
    fn restore(&mut self, scope: &str, blocks: Vec<MessageBlock>) {
        let uid = self.serial();
        let session = self.session(scope);
        session.loading = false;
        session.error = None;
        // A block snapshot has no run boundaries. Never invent historical runs/times.
        let mut history = TraceRun::new(uid, None, "history", "历史快照 · 仅顺序与 I/O");
        history.id = "client-history-snapshot".into();
        history.status = "history".into();
        history.spans[0].status = "history".into();
        for block in blocks.into_iter().take(MAX_SPANS - 1) {
            let id = limited(block.id(), 256);
            // Stable IDs already observed live must not become duplicate history observations.
            if session
                .runs
                .iter()
                .filter(|r| r.source != "history")
                .flat_map(|r| &r.spans)
                .any(|s| s.id == format!("generation:{id}") || s.id == format!("tool:{id}"))
            {
                continue;
            }
            let (kind, name, input, output, status, error) = match block {
                MessageBlock::PublicSummary { text, .. } => (
                    "context",
                    "公开上下文摘要".into(),
                    String::new(),
                    clean_text(&text),
                    "history",
                    None,
                ),
                MessageBlock::Text { text, .. } => (
                    "generation",
                    "公开正文".into(),
                    String::new(),
                    clean_text(&text),
                    "history",
                    None,
                ),
                MessageBlock::ToolCall {
                    name,
                    arguments,
                    output,
                    status,
                    ..
                } => {
                    let label = match status {
                        ToolStatus::Running => "incomplete",
                        ToolStatus::Completed => "completed",
                        ToolStatus::Failed => "failed",
                        ToolStatus::Cancelled => "interrupted",
                    };
                    (
                        "tool",
                        clean_text(&name),
                        clean_text(&arguments),
                        clean_text(&output),
                        label,
                        (label == "failed").then(|| "历史工具失败；详情见输出。".into()),
                    )
                }
            };
            let span_id = format!("{kind}:{id}");
            let mut span = new_span(span_id.clone(), Some("root".into()), kind, &name);
            span.input = input;
            span.output = output;
            span.status = status.into();
            span.error = error;
            if let Some(index) = history.spans.iter().position(|s| s.id == span_id) {
                history.spans[index] = span;
            } else {
                history.spans.push(span);
            }
        }
        session.runs.retain(|r| r.source != "history");
        if history.spans.len() > 1 {
            if session.runs.len() >= MAX_RUNS {
                session.runs.remove(0);
            }
            session.runs.insert(0, history);
        }
    }
}

// Only explicit per-message/per-settlement usage. Never inspect stats or command output.
fn extract_usage(value: Option<&Value>) -> Option<Value> {
    let v = value?.as_object()?;
    let number = |keys: &[&str]| keys.iter().find_map(|k| v.get(*k).and_then(Value::as_u64));
    let cost = v
        .get("cost")
        .and_then(|v| {
            v.as_f64()
                .or_else(|| v.get("total").and_then(Value::as_f64))
        })
        .filter(|v| v.is_finite() && *v >= 0.0);
    let result = json!({"input":number(&["input", "inputTokens", "input_tokens"]), "output":number(&["output", "outputTokens", "output_tokens"]), "cache":number(&["cacheRead", "cache_read", "cache_read_tokens"]), "cost":cost});
    result
        .as_object()
        .is_some_and(|m| m.values().any(|v| !v.is_null()))
        .then_some(result)
}
fn metric(usage: Option<&Value>, key: &str) -> String {
    usage
        .and_then(|v| v[key].as_u64())
        .map_or_else(|| "—".into(), |v| v.to_string())
}
// Preserve the original saturating truncation, including negative totals and the
// rounded f64 representation of u64::MAX (2^64), after checking every boundary.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn token_total(value: f64) -> Option<u64> {
    if !value.is_finite() || value > 2_f64.powi(64) {
        return None;
    }
    if value <= 0.0 {
        return Some(0);
    }
    if value >= 2_f64.powi(64) {
        return Some(u64::MAX);
    }
    Some(value as u64)
}

fn usage_for(run: &TraceRun) -> Option<Value> {
    if run.usage.is_some() {
        return run.usage.clone();
    }
    let generations: Vec<_> = run
        .spans
        .iter()
        .filter(|s| s.kind == "generation")
        .collect();
    if generations.is_empty() {
        return None;
    }
    let mut result = serde_json::Map::new();
    for key in ["input", "output", "cache", "cost"] {
        let values: Option<Vec<f64>> = generations
            .iter()
            .map(|s| s.usage.as_ref()?.get(key)?.as_f64())
            .collect();
        let total = values.map(|values| values.into_iter().sum::<f64>());
        result.insert(
            key.into(),
            if key == "cost" {
                json!(total)
            } else {
                json!(total.and_then(token_total))
            },
        );
    }
    Some(Value::Object(result))
}
fn rows(run: &TraceRun) -> Vec<(usize, usize)> {
    fn walk(run: &TraceRun, parent: Option<&str>, depth: usize, out: &mut Vec<(usize, usize)>) {
        if depth > MAX_SPANS {
            return;
        }
        for (i, span) in run
            .spans
            .iter()
            .enumerate()
            .filter(|(_, s)| s.parent.as_deref() == parent)
        {
            out.push((i, depth));
            walk(run, Some(&span.id), depth + 1, out);
        }
    }
    let mut result = Vec::new();
    walk(run, None, 0, &mut result);
    result
}
fn bar_style(run: &TraceRun, span: &TraceSpan) -> Option<String> {
    let total = run.extent()?;
    let start = difference(run.started.ms, span.started.ms)?;
    let elapsed = difference(span.started.ms, span.finished.ms)?;
    let left = (start / total * 100.0).clamp(0.0, 100.0);
    let width = (elapsed / total * 100.0).clamp(0.0, 100.0 - left);
    Some(format!("left:{left:.3}%;width:{width:.3}%"))
}

#[component]
pub fn TraceEntry(scope: String, store: Signal<TraceStore>) -> Element {
    let mut open = use_signal(|| false);
    // Selection survives closing/reopening, and is isolated by node/session scope.
    let mut selections = use_signal(BTreeMap::<String, (u64, String, String)>::new);
    let state = store.read();
    let session = state.scopes.get(&scope);
    let latest = session.and_then(|s| s.runs.last());
    let saved = selections.read().get(&scope).cloned();
    let selected = saved
        .as_ref()
        .and_then(|(uid, _, _)| session?.runs.iter().find(|r| r.uid == *uid))
        .or(latest);
    let selected_span = selected.and_then(|run| {
        saved
            .as_ref()
            .and_then(|(_, id, _)| run.spans.iter().find(|s| &s.id == id))
            .or_else(|| run.spans.first())
    });
    let tab = saved
        .as_ref()
        .map_or("overview", |(_, _, tab)| tab.as_str());
    let latest_time = duration(latest.and_then(TraceRun::elapsed));
    let scope_select = scope.clone();
    let scope_mobile = scope.clone();
    rsx! {
        button {
            class: "session-trace-trigger", r#type: "button", aria_label: "查看会话追踪与耗时", aria_haspopup: "dialog", aria_expanded: "{open}",
            onclick: move |_| open.set(true),
            crate::components::Icon { name: "layout" }
            span { "追踪 · {latest_time}" }
        }
        if *open.read() {
            crate::components::Modal { id: "session-trace-dialog", title: "会话追踪", class: "trace-dialog", onclose: move |()| open.set(false),
                p { class: "trace-session-caption", title: "{scope}", "{scope}" }
                if let Some(notice) = session.and_then(|s| s.notice.as_ref()) {
                    p { class: "trace-notice", role: "status", "{notice}" }
                }
                if session.is_some_and(|s| s.loading) {
                    p { class: "trace-notice", role: "status", aria_live: "polite", "正在加载会话历史…" }
                }
                if let Some(error) = session.and_then(|s| s.error.as_ref()) {
                    p { class: "trace-error", role: "alert", "{clean_text(error)}" }
                }
                if let (Some(session), Some(run), Some(span)) = (session, selected, selected_span) {
                    div { class: "trace-mobile-picker",
                        label { r#for: "trace-round", "回合" }
                        select { id: "trace-round", value: "{run.uid}", onchange: move |event| {
                            if let Ok(uid) = event.value().parse::<u64>() { selections.write().insert(scope_mobile.clone(), (uid, "root".into(), "overview".into())); }
                        },
                            for (i, item) in session.runs.iter().enumerate() {
                                option { value: "{item.uid}", "{i + 1}. {item.name} · {status_label(&item.status)}" }
                            }
                        }
                    }
                    TraceMetrics { run: run.clone() }
                    div { class: "trace-layout",
                        aside { class: "trace-rounds", aria_label: "本会话的追踪回合",
                            div { class: "trace-rounds-caption", "{session.runs.len()} 个回合" }
                            for (i, item) in session.runs.iter().enumerate() {
                                {let scope = scope_select.clone(); let uid = item.uid; rsx! {
                                    button { r#type: "button", aria_current: if run.uid == uid { "true" } else { "false" }, onclick: move |_| { selections.write().insert(scope.clone(), (uid, "root".into(), "overview".into())); },
                                        small { "回合 {i + 1} · {source_label(&item.source)}" }
                                        strong { "{item.name}" }
                                        span { "{status_label(&item.status)} · " b { "{duration(item.elapsed())}" } }
                                    }
                                }}
                            }
                        }
                        section { class: "trace-main",
                            div { class: "trace-waterfall",
                                div { class: "trace-section-heading", strong { "调用时间线" } span { "{source_label(&run.source)} · {run.spans.len()} 步" } }
                                div { class: "trace-axis", span { "步骤" } div { span { "0 / —" } span { "{duration(run.extent())}" } } span { "耗时" } }
                                for (index, depth) in rows(run) {
                                    {let item = &run.spans[index]; let scope = scope.clone(); let uid = run.uid; let id = item.id.clone(); let current_tab = tab.to_owned(); rsx! {
                                        button { key: "{item.id}", r#type: "button", class: "trace-span-row", style: "--trace-depth:{depth}", aria_pressed: "{span.id == item.id}", "data-status": "{item.status}", title: "{item.name} · {status_label(&item.status)}", onclick: move |_| { selections.write().insert(scope.clone(), (uid, id.clone(), current_tab.clone())); },
                                            span { class: "trace-span-label", i { class: "trace-kind {item.kind}", "{kind_label(&item.kind)}" } span { "{item.name}" } }
                                            span { class: "trace-span-track", if let Some(style) = bar_style(run, item) { i { style } } else { em { "—" } } }
                                            span { class: "trace-span-duration", "{duration(difference(item.started.ms, item.finished.ms))}" }
                                        }
                                    }}
                                }
                            }
                            section { class: "trace-details", aria_label: "调用详情",
                                div { class: "trace-detail-title", strong { "{span.name}" } span { "data-status": "{span.status}", "{status_label(&span.status)}" } }
                                div { class: "trace-detail-tabs", role: "group", aria_label: "追踪详情分类",
                                    for (id, label) in [("overview", "概览"), ("io", "输入 / 输出"), ("metadata", "元数据")] {
                                        {let scope = scope.clone(); let uid = run.uid; let span_id = span.id.clone(); rsx! {
                                            button { r#type: "button", aria_pressed: "{tab == id}", onclick: move |_| { selections.write().insert(scope.clone(), (uid, span_id.clone(), id.into())); }, "{label}" }
                                        }}
                                    }
                                }
                                TraceDetails { scope: scope.clone(), run: run.clone(), span: span.clone(), tab: tab.to_owned() }
                            }
                        }
                    }
                    footer { class: "trace-footer",
                        code { title: "{run.id}", if run.server_run_id.is_some() { "服务端 runId · {run.id}" } else { "客户端观测 ID · {run.id}" } }
                        span { "客户端接收计时 · 未上报显示 —" }
                    }
                } else {
                    div { class: "trace-empty", role: "status",
                        crate::components::Icon { name: "layout" }
                        h3 { "此会话暂无追踪" }
                        p { "等待当前应用接收运行事件或恢复历史后，查看调用与耗时。" }
                    }
                }
            }
        }
    }
}
fn source_label(source: &str) -> &str {
    match source {
        "history" => "历史快照 · 无计时",
        "command" => "应用命令",
        _ => "客户端接收事件",
    }
}

#[component]
fn TraceMetrics(run: TraceRun) -> Element {
    let usage = usage_for(&run);
    let tokens = format!(
        "{} / {}",
        metric(usage.as_ref(), "input"),
        metric(usage.as_ref(), "output")
    );
    let cost = usage
        .as_ref()
        .and_then(|v| v["cost"].as_f64())
        .map_or_else(|| "—".into(), |v| format!("${v:.4}"));
    rsx! { div { class: "trace-metrics",
        for (name, value) in [("回合总耗时", duration(run.elapsed())), ("首段延迟", duration(run.first())), ("输入 / 输出 Token", tokens), ("费用 · 上报值", cost)] {
            div { small { "{name}" } strong { "{value}" } }
        }
    } }
}
fn root_output(run: &TraceRun) -> String {
    limited(
        &run.spans
            .iter()
            .filter(|s| s.kind == "generation")
            .map(|s| s.output.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        TEXT_LIMIT,
    )
}
#[component]
fn TraceDetails(scope: String, run: TraceRun, span: TraceSpan, tab: String) -> Element {
    let usage = if span.id == "root" {
        usage_for(&run)
    } else {
        span.usage.clone()
    };
    let output = if span.id == "root" && span.output.is_empty() {
        root_output(&run)
    } else {
        span.output.clone()
    };
    let metadata = serde_json::to_string_pretty(&json!({
        "scope":scope,"traceId":run.id,"serverRunId":run.server_run_id,"identitySource":if run.server_run_id.is_some(){"server"}else{"client observation"},
        "spanId":span.id,"parentSpanId":span.parent,"kind":span.kind,"source":source_label(&run.source),"status":span.status,
        "rawOutcome":run.raw_outcome,"provider":run.provider,"model":run.model,"usage":usage,
        "resourceSnapshot":run.resource_snapshot,"timingSource":"browser performance.now after receipt; includes bridge scheduling delay",
        "startOffsetMs":difference(run.started.ms,span.started.ms),"durationMs":difference(span.started.ms,span.finished.ms),
        "clockBridgeFailed":run.clock_failed,"remoteExport":false
    })).unwrap_or_default();
    rsx! {
        if let Some(error) = &span.error { p { class: "trace-error", role: "status", "{error}" } }
        if tab == "io" {
            div { class: "trace-io",
                section { h4 { "输入" } pre { if span.input.is_empty() { "未上报" } else { "{span.input}" } } }
                section { h4 { "输出" } pre { if output.is_empty() { "未上报" } else { "{output}" } } }
            }
        } else if tab == "metadata" {
            pre { class: "trace-metadata", "{metadata}" }
        } else {
            dl { class: "trace-overview",
                for (label, value) in [
                    ("状态", status_label(&span.status).to_owned()),
                    ("耗时", duration(difference(span.started.ms,span.finished.ms))),
                    ("开始偏移", duration(difference(run.started.ms,span.started.ms))),
                    ("Provider / 模型", format!("{} / {}",run.provider.as_deref().unwrap_or("—"),run.model.as_deref().unwrap_or("—"))),
                    ("输入 / 输出", format!("{} / {}",metric(usage.as_ref(),"input"),metric(usage.as_ref(),"output"))),
                    ("缓存读取 Token", metric(usage.as_ref(),"cache")),
                    ("首段等待", duration(if span.id == "root" {run.first()} else {difference(span.started.ms,span.first_text.ms)})),
                    ("计时来源", source_label(&run.source).into()),
                ] { div { dt { "{label}" } dd { "{value}" } } }
            }
            p { class: "trace-hint", "总耗时使用可测的起止差；并行步骤不相加。进行中的未闭合区间、历史时间及缺失用量显示 —。首段为公开文本接收延迟，包含浏览器桥接调度，不代表模型 TTFT。仅保存在应用内存；未向 Langfuse 导出。" }
            if run.clock_failed { p { class: "trace-notice", "浏览器时钟桥接失败，相关时间未知。" } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(seq: u64, kind: &str, data: Value) -> WireEvent {
        WireEvent {
            id: Some(seq.to_string()),
            seq: Some(seq),
            event_type: kind.into(),
            data,
        }
    }
    fn apply(
        store: &mut TraceStore,
        scope: &str,
        seq: u64,
        kind: &str,
        data: Value,
        ms: f64,
    ) -> bool {
        if let Some(request) = store.ingest(scope, &event(seq, kind, data)) {
            store.patch_clock(&request, Some(ms));
            true
        } else {
            false
        }
    }
    #[test]
    fn settlement_dedup_snapshot_and_scope() {
        let mut store = TraceStore::default();
        apply(
            &mut store,
            "n/s",
            1,
            "agent_start",
            json!({"runId":"r"}),
            100.0,
        );
        apply(
            &mut store,
            "n/s",
            2,
            "tool_execution_start",
            json!({"toolCallId":"t","args":{"apiKey":"secret"}}),
            110.0,
        );
        apply(
            &mut store,
            "n/s",
            3,
            "tool_execution_update",
            json!({"toolCallId":"t","partialResult":"one"}),
            120.0,
        );
        apply(
            &mut store,
            "n/s",
            4,
            "tool_execution_update",
            json!({"toolCallId":"t","partialResult":"one two"}),
            130.0,
        );
        assert_eq!(
            store.scopes["n/s"].runs[0].spans.last().unwrap().output,
            "one two"
        );
        apply(
            &mut store,
            "n/s",
            5,
            "tool_execution_end",
            json!({"toolCallId":"t","isError":true,"error":"missing file","result":"partial"}),
            140.0,
        );
        apply(&mut store, "n/s", 6, "agent_end", json!({}), 150.0);
        assert_eq!(store.scopes["n/s"].runs[0].status, "awaiting_settlement");
        assert_eq!(store.scopes["n/s"].runs[0].elapsed(), None);
        apply(
            &mut store,
            "n/s",
            7,
            "agent_settled",
            json!({"outcome":"suspended"}),
            180.0,
        );
        assert!(!apply(
            &mut store,
            "n/s",
            7,
            "agent_settled",
            json!({"outcome":"completed"}),
            999.0
        ));
        let run = &store.scopes["n/s"].runs[0];
        assert_eq!(run.elapsed(), Some(80.0));
        assert_eq!(run.raw_outcome, Some(json!("suspended")));
        assert!(
            run.spans
                .iter()
                .any(|s| s.error.as_deref() == Some("missing file") && s.status == "failed")
        );
        assert!(!format!("{run:?}").contains("secret"));
        apply(&mut store, "other/s", 1, "agent_start", json!({}), 300.0);
        assert_eq!(store.scopes["n/s"].runs.len(), 1);
    }
    #[test]
    fn private_reasoning_and_token_clock_budget() {
        let mut store = TraceStore::default();
        apply(&mut store, "s", 1, "agent_start", json!({}), 100.0);
        assert!(!apply(
            &mut store,
            "s",
            2,
            "message_update",
            json!({"eventType":"thinking_delta","delta":"PRIVATE"}),
            101.0
        ));
        assert!(apply(
            &mut store,
            "s",
            3,
            "message_update",
            json!({"eventType":"text_delta","messageId":"m","delta":"hello"}),
            120.0
        ));
        for seq in 4..100 {
            assert!(!apply(
                &mut store,
                "s",
                seq,
                "message_update",
                json!({"eventType":"text_delta","messageId":"m","delta":"!"}),
                121.0
            ));
        }
        assert!(!format!("{store:?}").contains("PRIVATE"));
        assert_eq!(store.scopes["s"].runs[0].first(), Some(20.0));
    }
    #[test]
    fn async_patch_targets_original_run_and_store() {
        let mut store = TraceStore::default();
        let request = store
            .ingest("s", &event(1, "agent_start", json!({"runId":"a"})))
            .unwrap();
        apply(
            &mut store,
            "s",
            2,
            "agent_settled",
            json!({"runId":"a","outcome":"completed"}),
            200.0,
        );
        apply(
            &mut store,
            "s",
            3,
            "agent_start",
            json!({"runId":"b"}),
            300.0,
        );
        store.patch_clock(&request, Some(100.0));
        assert_eq!(store.scopes["s"].runs[1].started.ms, Some(300.0));
        assert_eq!(store.scopes["s"].runs[0].elapsed(), Some(100.0));
        let mut replacement = TraceStore::default();
        apply(
            &mut replacement,
            "s",
            1,
            "agent_start",
            json!({"runId":"a"}),
            777.0,
        );
        replacement.patch_clock(&request, Some(1.0));
        assert_eq!(replacement.scopes["s"].runs[0].started.ms, Some(777.0));
    }
    #[test]
    fn missing_run_id_starts_new_round_and_late_tool_stays_put() {
        let mut store = TraceStore::default();
        apply(&mut store, "s", 1, "agent_start", json!({}), 10.0);
        apply(
            &mut store,
            "s",
            2,
            "tool_execution_start",
            json!({"toolCallId":"old"}),
            20.0,
        );
        apply(&mut store, "s", 3, "agent_end", json!({}), 30.0);
        apply(&mut store, "s", 4, "agent_start", json!({}), 40.0);
        assert_eq!(store.scopes["s"].runs.len(), 2);
        apply(
            &mut store,
            "s",
            5,
            "tool_execution_end",
            json!({"toolCallId":"old","result":"late"}),
            50.0,
        );
        assert_eq!(
            store.scopes["s"].runs[0]
                .spans
                .iter()
                .find(|s| s.id == "tool:old")
                .unwrap()
                .output,
            "late"
        );
        assert!(
            store.scopes["s"].runs[1]
                .spans
                .iter()
                .all(|s| s.id != "tool:old")
        );
    }
    #[test]
    fn history_unknown_and_replaced_by_id() {
        let mut store = TraceStore::default();
        let blocks = vec![MessageBlock::Text {
            id: "m".into(),
            text: "public".into(),
        }];
        store.restore("s", blocks.clone());
        store.restore("s", blocks);
        let run = &store.scopes["s"].runs[0];
        assert_eq!(store.scopes["s"].runs.len(), 1);
        assert_eq!(run.spans.len(), 2);
        assert_eq!(run.elapsed(), None);
        assert!(bar_style(run, &run.spans[1]).is_none());
        assert_eq!(difference(None, Some(0.0)), None);
        assert_eq!(difference(Some(4.0), Some(3.0)), None);
    }
    #[test]
    fn redaction_and_bounds() {
        let clean = display_value(
            &json!({"Authorization":"Bearer credential", "nested":{"password":"hunter2","apiKey":"key"},"rawRequest":{"body":"NEVER"},"content":[{"type":"thinking","thinking":"PRIVATE"},{"type":"text","text":"visible"}]}),
        );
        for secret in ["credential", "hunter2", "\"key\"", "NEVER", "PRIVATE"] {
            assert!(!clean.contains(secret), "{clean}");
        }
        let text = clean_text("Authorization: Bearer abc\npassword=xyz\napiKey: 'qwerty'");
        for secret in ["abc", "xyz", "qwerty"] {
            assert!(!text.contains(secret));
        }
        assert!(clean_text(&"中".repeat(TEXT_LIMIT)).len() <= TEXT_LIMIT);
    }
}
