//! Standalone conversation widgets. The parent owns history deduplication, HTTP,
//! and scope/request keys. Render functions consume the current snapshot only.
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::fmt::Write as _;
use work_client::{PendingDialog, UiDialogOption, UiDialogPayload};
use work_core::{MessageBlock, ToolStatus};

pub const CSS: &str = include_str!("conversation.css");

#[derive(Clone, PartialEq)]
pub struct DialogAnswer {
    pub request_id: String,
    pub answer: Option<Value>,
}

fn text_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)?
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
    })
}

// Only public text blocks are read. In particular, a thinking block carrying a
// `text` field is never promoted to public assistant content.
fn public_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| match item.get("type").and_then(Value::as_str) {
                Some("text" | "input_text" | "output_text") => {
                    item.get("text").and_then(Value::as_str)
                }
                None if item.get("thinking").is_none() => item.get("text").and_then(Value::as_str),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

pub fn render_history(message: &Value) -> Element {
    render_history_in_context(message, &[], &[])
}

pub fn message_has_tool_call(message: &Value, id: &str) -> bool {
    message
        .get("content")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                matches!(
                    item.get("type").and_then(Value::as_str),
                    Some("toolCall" | "tool_use" | "tool_call")
                ) && item
                    .get("id")
                    .or_else(|| item.get("toolCallId"))
                    .and_then(Value::as_str)
                    == Some(id)
            })
        })
}

pub fn render_history_in_context(
    message: &Value,
    messages: &[Value],
    blocks: &[MessageBlock],
) -> Element {
    let role = message
        .get("role")
        .or_else(|| message.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if !matches!(
        role,
        "user" | "assistant" | "tool" | "toolResult" | "tool_result"
    ) {
        return rsx! {};
    }
    let text = message
        .get("content")
        .map(public_text)
        .or_else(|| text_field(message, &["text"]))
        .unwrap_or_default();
    if matches!(role, "tool" | "toolResult" | "tool_result") {
        if message
            .get("toolCallId")
            .and_then(Value::as_str)
            .is_some_and(|id| messages.iter().any(|item| message_has_tool_call(item, id)))
        {
            // The matching assistant call displays the result in its own card.
            return rsx! {};
        }
        return render_tool_result(message, text);
    }
    let user = role == "user";
    let calls = message
        .get("content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    matches!(
                        item.get("type").and_then(Value::as_str),
                        Some("toolCall" | "tool_use" | "tool_call")
                    )
                })
                .map(|(index, item)| {
                    let name = text_field(item, &["name", "toolName"])
                        .unwrap_or_else(|| "工具调用".into());
                    let args = item
                        .get("arguments")
                        .or_else(|| item.get("input"))
                        .or_else(|| item.get("args"))
                        .map(pretty_value)
                        .unwrap_or_default();
                    let key = text_field(item, &["id", "toolCallId"])
                        .unwrap_or_else(|| format!("content-{index}"));
                    (key, name, args)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if text.is_empty() && calls.is_empty() {
        return rsx! {};
    }
    rsx! {
        article { class: if user { "conversation-widget message timeline-message user-message" } else { "conversation-widget message timeline-message agent-message" },
            div { class: if user { "message-avatar" } else { "message-avatar agent-avatar" }, if user { "你" } else { "r↗" } }
            div { class: "message-content",
                div { class: "message-author", if user { "你" } else { "rpi" } }
                if user { div { class: "conversation-user-text", "{text}" } }
                else { {markdown_view(&text)} }
                for (key, name, args) in calls {
                    div { key: "{key}",
                        {
                            let result = messages.iter().rev().find(|item| {
                                matches!(item.get("role").or_else(|| item.get("kind")).and_then(Value::as_str), Some("tool" | "toolResult" | "tool_result"))
                                    && item.get("toolCallId").and_then(Value::as_str) == Some(key.as_str())
                            });
                            if let Some(result) = result {
                                let output = result.get("content").map(public_text).or_else(|| result.get("result").map(public_result)).unwrap_or_default();
                                let status = if result.get("isError").and_then(Value::as_bool) == Some(true) { ToolStatus::Failed } else { ToolStatus::Completed };
                                tool_view(&name, &args, &output, &status)
                            } else if let Some(block) = blocks.iter().find(|block| block.id() == key) {
                                render_message_block(block)
                            } else {
                                tool_view_with_label(&name, &args, "", "等待工具结果", false)
                            }
                        }
                    }
                }
            }
        }
    }
}

fn render_tool_result(message: &Value, text: String) -> Element {
    let name = text_field(message, &["toolName", "name"]).unwrap_or_else(|| "工具结果".into());
    let output = if text.is_empty() {
        message.get("result").map(public_result).unwrap_or_default()
    } else {
        text
    };
    let status = if message.get("isError").and_then(Value::as_bool) == Some(true) {
        ToolStatus::Failed
    } else {
        ToolStatus::Completed
    };
    let args = message
        .get("arguments")
        .or_else(|| message.get("args"))
        .map(pretty_value)
        .unwrap_or_default();
    tool_view(&name, &args, &output, &status)
}

pub fn render_message_block(block: &MessageBlock) -> Element {
    match block {
        MessageBlock::Text { text, .. } => rsx! {
            article { class: "conversation-widget message timeline-message agent-message",
                div { class: "message-avatar agent-avatar", "r↗" }
                div { class: "message-content",
                    div { class: "message-author", "rpi" }
                    {markdown_view(text)}
                }
            }
        },
        MessageBlock::PublicSummary { text, .. } => rsx! {
            aside { class: "conversation-widget conversation-progress", aria_label: "公开进度",
                span { class: "conversation-progress-label", "公开进度" }
                {markdown_view(text)}
            }
        },
        MessageBlock::ToolCall {
            name,
            arguments,
            output,
            status,
            ..
        } => tool_view(name, arguments, output, status),
    }
}

fn pretty_value(value: &Value) -> String {
    value.as_str().map_or_else(
        || serde_json::to_string_pretty(value).unwrap_or_default(),
        str::to_owned,
    )
}

fn public_result(value: &Value) -> String {
    if let Some(content) = value.get("content") {
        let text = public_text(content);
        if !text.is_empty() {
            return text;
        }
    }
    pretty_value(value)
}

fn tool_summary(name: &str, args: &str) -> String {
    let value = serde_json::from_str::<Value>(args).unwrap_or(Value::Null);
    let fields = [
        "path",
        "paths",
        "command",
        "query",
        "pattern",
        "url",
        "file",
        "directory",
    ];
    let summary = fields
        .iter()
        .find_map(|key| {
            let value = value.get(*key)?;
            let text = value.as_str().map(str::to_owned).or_else(|| {
                value.as_array().map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
            })?;
            (!text.trim().is_empty()).then(|| format!("{key}={text}"))
        })
        .unwrap_or_else(|| name.to_owned());
    let mut result = summary.chars().take(96).collect::<String>();
    if summary.chars().count() > 96 {
        result.push('…');
    }
    result
}

fn tool_view(name: &str, args: &str, output: &str, status: &ToolStatus) -> Element {
    let label = match status {
        ToolStatus::Running => "运行中",
        ToolStatus::Completed => "已完成",
        ToolStatus::Failed => "失败",
        ToolStatus::Cancelled => "已取消",
    };
    tool_view_with_label(name, args, output, label, *status == ToolStatus::Failed)
}

fn tool_view_with_label(
    name: &str,
    args: &str,
    output: &str,
    label: &str,
    failed: bool,
) -> Element {
    let args =
        serde_json::from_str::<Value>(args).map_or_else(|_| args.to_owned(), |v| pretty_value(&v));
    let summary = tool_summary(name, &args);
    let plan = plan_items(name, &args, output);
    let finished = plan
        .iter()
        .filter(|item| item.status == "completed")
        .count();
    let status_class = if failed { "failed" } else { "" };
    rsx! {
        div { class: "conversation-widget conversation-tool",
            if !plan.is_empty() {
                details { class: "inline-plan",
                    summary {
                        span { {glyph("plan")} "任务计划" }
                        span { "{finished}/{plan.len()} 已完成" {glyph("chevron")} }
                    }
                    ol {
                        for (index, item) in plan.iter().enumerate() {
                            li { key: "plan-{index}", class: "plan-item {item.status}",
                                {glyph(if item.status == "completed" { "check" } else { "clock" })}
                                div {
                                    strong { "{item.title}" }
                                    small { "{item.label}" }
                                    if !item.description.is_empty() { small { "{item.description}" } }
                                }
                            }
                        }
                    }
                }
            }
            details { class: "tool-call inline-call {status_class}",
                summary {
                    span { class: "call-icon", {glyph("tool")} }
                    div { class: "call-name", strong { "{name}" } code { "{summary}" } }
                    span { class: "call-status", "{label}" }
                    {glyph("chevron")}
                }
                if failed { div { class: "inline-call-warning", role: "status", "工具执行失败，请查看工具结果。" } }
                div { class: "call-body",
                    div { class: "call-section-title", "调用参数" }
                    pre { class: "call-arguments", "{args}" }
                    div { class: "call-section-title conversation-output-heading",
                        span { "工具结果" }
                        CopyOutput { text: output.to_owned() }
                    }
                    pre { class: if failed { "call-output error-text" } else { "call-output" }, "{output}" }
                }
            }
        }
    }
}

fn glyph(name: &str) -> Element {
    let path = match name {
        "check" => "M20 6 9 17l-5-5",
        "chevron" => "m6 9 6 6 6-6",
        "clock" => "M12 8v4l3 2M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0",
        "ask" => "M21 11a8 8 0 0 1-8 8H7l-5 3V11a9 9 0 0 1 19 0M8 9h8M8 13h5",
        "plan" => "M9 6h12M9 12h12M9 18h12M2 6l1 1 2-2M2 12l1 1 2-2M2 18l1 1 2-2",
        _ => "m12 3 9 5-9 5-9-5 9-5ZM3 12l9 5 9-5M3 16l9 5 9-5",
    };
    rsx! { svg { view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round", "aria-hidden": "true", path { d: path } } }
}

#[component]
fn CopyOutput(text: String) -> Element {
    let mut notice = use_signal(String::new);
    rsx! {
        span { class: "conversation-copy",
            button { r#type: "button", class: "button", aria_label: "复制工具输出", disabled: text.is_empty(),
                onclick: move |_| {
                    let encoded = serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".into());
                    spawn(async move {
                        let script = format!("try {{ await navigator.clipboard.writeText({encoded}); dioxus.send(true); }} catch (_) {{ dioxus.send(false); }}");
                        let copied = document::eval(&script).recv::<bool>().await.unwrap_or(false);
                        notice.set(if copied { "已复制".into() } else { "复制失败，请手动选择输出".into() });
                    });
                },
                "复制输出"
            }
            span { role: "status", "{notice}" }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanItem {
    pub title: String,
    pub description: String,
    pub status: &'static str,
    pub label: String,
}

/// Parse the structured plan payload shared by inline conversation blocks and
/// the Inspector. This deliberately accepts only real `todo/update_plan` data.
pub fn parse_plan(name: &str, args: &str, output: &str) -> Vec<PlanItem> {
    plan_items(name, args, output)
}

pub fn plan_from_projection(blocks: &[MessageBlock]) -> Vec<PlanItem> {
    blocks
        .iter()
        .rev()
        .find_map(|block| match block {
            MessageBlock::ToolCall {
                name,
                arguments,
                output,
                ..
            } => {
                let items = parse_plan(name, arguments, output);
                (!items.is_empty()).then_some(items)
            }
            MessageBlock::PublicSummary { .. } | MessageBlock::Text { .. } => None,
        })
        .unwrap_or_default()
}

pub fn plan_for_session(history: &[Value], projection: Option<&[MessageBlock]>) -> Vec<PlanItem> {
    let live = projection.map(plan_from_projection).unwrap_or_default();
    if !live.is_empty() {
        return live;
    }
    history
        .iter()
        .rev()
        .find_map(|message| {
            let calls = message.get("content")?.as_array()?;
            calls.iter().rev().find_map(|call| {
                if !matches!(
                    call.get("type").and_then(Value::as_str),
                    Some("toolCall" | "tool_use" | "tool_call")
                ) {
                    return None;
                }
                let name = call
                    .get("name")
                    .or_else(|| call.get("toolName"))
                    .and_then(Value::as_str)?;
                let args = call
                    .get("arguments")
                    .or_else(|| call.get("input"))
                    .or_else(|| call.get("args"))
                    .map(ToString::to_string)
                    .unwrap_or_default();
                let items = parse_plan(name, &args, "");
                (!items.is_empty()).then_some(items)
            })
        })
        .unwrap_or_default()
}

fn plan_items(name: &str, args: &str, output: &str) -> Vec<PlanItem> {
    fn items(value: &Value) -> Option<&Vec<Value>> {
        ["items", "todos", "plan", "steps"]
            .iter()
            .find_map(|key| value.get(*key)?.as_array())
            .or_else(|| value.get("details").and_then(items))
            .or_else(|| value.get("result").and_then(items))
    }

    if !matches!(
        name,
        "todo"
            | "todos"
            | "todo_write"
            | "todowrite"
            | "TodoWrite"
            | "update_plan"
            | "plan"
            | "rpi-todo"
    ) {
        return Vec::new();
    }
    let args = serde_json::from_str::<Value>(args).unwrap_or(Value::Null);
    let output = serde_json::from_str::<Value>(output).unwrap_or(Value::Null);
    let Some(items) = items(&output).or_else(|| items(&args)) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let title = item
                .as_str()
                .map(str::to_owned)
                .or_else(|| text_field(item, &["title", "step", "content", "text", "task"]))?;
            let actual = text_field(item, &["status", "state"]);
            let complete = item
                .get("completed")
                .or_else(|| item.get("done"))
                .and_then(Value::as_bool);
            let (status, label) = match actual.as_deref() {
                Some("completed" | "done" | "complete") => ("completed", "已完成".into()),
                Some("in_progress" | "running" | "active") => ("running", "进行中".into()),
                Some("pending" | "todo" | "not_started") => ("pending", "待处理".into()),
                Some("cancelled" | "canceled") => ("cancelled", "已取消".into()),
                Some(other) => ("unknown", format!("状态：{other}")),
                None if complete == Some(true) => ("completed", "已完成".into()),
                None if complete == Some(false) => ("pending", "待处理".into()),
                None => ("unknown", "状态未提供".into()),
            };
            Some(PlanItem {
                title,
                description: text_field(item, &["description"]).unwrap_or_default(),
                status,
                label,
            })
        })
        .collect()
}

// A small Markdown grammar rendered entirely as escaped text nodes. No HTML is
// interpreted, including HTML inside code fences or an unfinished streaming token.
#[derive(Debug, PartialEq)]
enum Inline {
    Text(String),
    Code(String),
    Strong(Vec<Inline>),
    Link(String, String),
}

fn safe_link(url: &str) -> bool {
    if url
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"' | '\'' | '\\'))
    {
        return false;
    }
    let lower = url.to_ascii_lowercase();
    if let Some(rest) = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
    {
        let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
        return !host.is_empty() && host != "." && !host.contains('@');
    }
    lower
        .strip_prefix("mailto:")
        .is_some_and(|rest| !rest.is_empty() && rest.contains('@'))
}

fn parse_inline(text: &str, depth: usize) -> Vec<Inline> {
    if depth > 8 {
        return vec![Inline::Text(text.into())];
    }
    let mut result = Vec::new();
    let mut literal = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        let mut matched = None;
        if let Some(after) = rest.strip_prefix('`') {
            if let Some(end) = after.find('`') {
                matched = Some((end + 2, Inline::Code(after[..end].into())));
            }
        } else if let Some(after) = rest.strip_prefix("**").or_else(|| rest.strip_prefix("__")) {
            let marker = &rest[..2];
            if let Some(end) = after.find(marker) {
                matched = Some((
                    end + 4,
                    Inline::Strong(parse_inline(&after[..end], depth + 1)),
                ));
            }
        } else if let Some(after) = rest.strip_prefix('[') {
            if let Some(end) = after.find("](") {
                let destination = &after[end + 2..];
                if let Some(close) = destination.find(')') {
                    let target = &destination[..close];
                    if safe_link(target) {
                        matched = Some((
                            end + close + 4,
                            Inline::Link(after[..end].into(), target.into()),
                        ));
                    }
                }
            }
        } else if rest.starts_with('\\') {
            let mut chars = rest.chars();
            chars.next();
            if let Some(c) = chars.next().filter(char::is_ascii_punctuation) {
                literal.push(c);
                rest = &rest[1 + c.len_utf8()..];
                continue;
            }
        }
        if let Some((consumed, node)) = matched {
            if !literal.is_empty() {
                result.push(Inline::Text(std::mem::take(&mut literal)));
            }
            result.push(node);
            rest = &rest[consumed..];
        } else {
            let ch = rest.chars().next().unwrap();
            literal.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    if !literal.is_empty() {
        result.push(Inline::Text(literal));
    }
    result
}

fn inline_nodes(nodes: &[Inline]) -> Element {
    rsx! { for (index, node) in nodes.iter().enumerate() {
        Fragment { key: "inline-{index}",
            {match node {
                Inline::Text(text) => rsx! { "{text}" },
                Inline::Code(text) => rsx! { code { "{text}" } },
                Inline::Strong(children) => rsx! { strong { {inline_nodes(children)} } },
                Inline::Link(label, url) => rsx! { a { href: "{url}", rel: "noopener noreferrer", target: "_blank", "{label}" } },
            }}
        }
    } }
}
fn inline(text: &str) -> Element {
    inline_nodes(&parse_inline(text, 0))
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let line = line.trim_start();
    let count = line.bytes().take_while(|b| *b == b'#').count();
    (count > 0 && count <= 6)
        .then(|| {
            line.get(count..)?
                .strip_prefix(' ')
                .map(|text| (count, text))
        })
        .flatten()
}
fn list_line(line: &str) -> Option<(bool, usize, &str)> {
    let line = line.trim_start();
    for prefix in ["- ", "* ", "+ "] {
        if let Some(text) = line.strip_prefix(prefix) {
            return Some((false, 1, text));
        }
    }
    let count = line.bytes().take_while(u8::is_ascii_digit).count();
    if count == 0 {
        return None;
    }
    let after = &line[count..];
    after
        .strip_prefix(". ")
        .or_else(|| after.strip_prefix(") "))
        .map(|text| (true, line[..count].parse().unwrap_or(1), text))
}
fn fence(line: &str) -> Option<(char, usize, &str)> {
    let line = line.trim_start();
    let marker = line.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let count = line.chars().take_while(|c| *c == marker).count();
    (count >= 3).then_some((marker, count, line[count..].trim()))
}
fn table_cells(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_owned())
        .collect()
}
fn table_separator(line: &str) -> bool {
    let cells = table_cells(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let dashes = cell.trim_matches(':');
            dashes.len() >= 3 && dashes.bytes().all(|b| b == b'-')
        })
}
fn starts_block(lines: &[&str], index: usize) -> bool {
    let line = lines[index];
    line.trim().is_empty()
        || heading(line).is_some()
        || list_line(line).is_some()
        || fence(line).is_some()
        || line.trim_start().starts_with('>')
        || (line.contains('|')
            && lines
                .get(index + 1)
                .is_some_and(|next| table_separator(next)))
}

pub fn markdown_view(text: &str) -> Element {
    markdown_blocks(text, 0)
}

fn markdown_blocks(text: &str, depth: usize) -> Element {
    if depth > 8 {
        return rsx! { div { class: "conversation-markdown", "{text}" } };
    }
    let lines: Vec<_> = text.lines().collect();
    let mut blocks: Vec<Element> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        if let Some((marker, count, language)) = fence(line) {
            let language = language.to_owned();
            i += 1;
            let start = i;
            while i < lines.len()
                && !fence(lines[i])
                    .is_some_and(|(m, c, tail)| m == marker && c >= count && tail.is_empty())
            {
                i += 1;
            }
            let code = lines[start..i].join("\n");
            if i < lines.len() {
                i += 1;
            }
            blocks.push(rsx! { div { class: "conversation-code",
                if !language.is_empty() { div { class: "code-language", "{language}" } }
                pre { code { "{code}" } }
            } });
        } else if let Some((level, content)) = heading(line) {
            blocks.push(match level {
                1 => rsx! { h1 { {inline(content)} } },
                2 => rsx! { h2 { {inline(content)} } },
                3 => rsx! { h3 { {inline(content)} } },
                4 => rsx! { h4 { {inline(content)} } },
                5 => rsx! { h5 { {inline(content)} } },
                _ => rsx! { h6 { {inline(content)} } },
            });
            i += 1;
        } else if line.trim_start().starts_with('>') {
            let mut quote = Vec::new();
            while i < lines.len() {
                let Some(content) = lines[i].trim_start().strip_prefix('>') else {
                    break;
                };
                quote.push(content.strip_prefix(' ').unwrap_or(content));
                i += 1;
            }
            blocks.push(rsx! { blockquote { {markdown_blocks(&quote.join("\n"), depth + 1)} } });
        } else if let Some((ordered, start, _)) = list_line(line) {
            let mut items = Vec::new();
            while i < lines.len() {
                let Some((same, _, content)) = list_line(lines[i]) else {
                    break;
                };
                if same != ordered {
                    break;
                }
                items.push(content.to_owned());
                i += 1;
            }
            blocks.push(if ordered {
                rsx! { ol { start: "{start}", for (index, item) in items.iter().enumerate() { li { key: "{index}", {inline(item)} } } } }
            } else {
                rsx! { ul { for (index, item) in items.iter().enumerate() { li { key: "{index}", {inline(item)} } } } }
            });
        } else if line.contains('|') && lines.get(i + 1).is_some_and(|next| table_separator(next)) {
            let headers = table_cells(line);
            let mut rows = Vec::new();
            i += 2;
            while i < lines.len() && lines[i].contains('|') && !lines[i].trim().is_empty() {
                rows.push(table_cells(lines[i]));
                i += 1;
            }
            blocks.push(rsx! { div { class: "conversation-table-scroll", tabindex: "0", role: "region", aria_label: "Markdown 表格，可横向滚动",
                table {
                    thead { tr { for (index, cell) in headers.iter().enumerate() { th { key: "{index}", scope: "col", {inline(cell)} } } } }
                    tbody { for (index, row) in rows.iter().enumerate() {
                        tr { key: "{index}", for (column, cell) in row.iter().enumerate() { td { key: "{column}", {inline(cell)} } } }
                    } }
                }
            } });
        } else {
            let start = i;
            i += 1;
            while i < lines.len() && !starts_block(&lines, i) {
                i += 1;
            }
            blocks.push(rsx! { p { {inline(&lines[start..i].join("\n"))} } });
        }
    }
    rsx! { div { class: "conversation-markdown", for (index, block) in blocks.into_iter().enumerate() { Fragment { key: "md-{index}", {block} } } } }
}

struct AskModel {
    question: String,
    context: String,
    header: String,
    options: Vec<UiDialogOption>,
    input: AskInput,
    permission: bool,
    unsupported: Option<String>,
    placeholder: String,
}

struct AskInput {
    multiple: bool,
    freeform: bool,
    input_only: bool,
}

fn invalid_permission(pending: &PendingDialog, raw: &Value, options: &[UiDialogOption]) -> bool {
    let ui = &pending.ui;
    let mut titles = std::collections::HashSet::new();
    let invalid_options = options.is_empty()
        || options.iter().any(|option| {
            let title = option
                .raw
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("");
            !option.raw.is_object()
                || title.is_empty()
                || title.trim() != title
                || !titles.insert(title.to_owned())
                || !matches!(
                    option.permission.as_deref(),
                    Some("allow_once" | "allow_always" | "reject_once" | "reject_always")
                )
        });
    !matches!(ui.kind.as_deref(), Some("selector" | "confirm"))
        || pending
            .tool_call_id
            .as_deref()
            .is_none_or(|id| id.trim().is_empty())
        || ui.allow_multiple
        || ui.allow_freeform
        || raw.get("questions").is_some()
        || invalid_options
}

fn ask_options(
    pending: &PendingDialog,
    raw: &Value,
    source: &Value,
    permission: bool,
    kind: &str,
) -> (Vec<UiDialogOption>, AskInput, Option<String>) {
    let ui = &pending.ui;
    let effective: UiDialogPayload = serde_json::from_value(source.clone()).unwrap_or_default();
    let mut options = if source.get("options").is_some() {
        effective.options.clone()
    } else {
        ui.options.clone()
    };
    let mut unsupported = None;
    if source
        .get("options")
        .or_else(|| raw.get("options"))
        .is_some_and(|options| !options.is_array())
    {
        unsupported = Some("请求的 options 数据不完整，当前无法安全回答。".into());
    }
    if !matches!(kind, "selector" | "input" | "confirm") {
        unsupported = Some(format!("当前不支持此问题类型：{kind}"));
    }
    if !matches!(
        ui.purpose.as_deref(),
        None | Some("question" | "permission")
    ) {
        unsupported = Some("请求的 purpose 不受支持。".into());
    }
    let has_multi = ["allowMultiple", "allow_multiple", "multiple"]
        .iter()
        .any(|key| source.get(*key).is_some());
    let multiple = !permission
        && kind != "confirm"
        && if has_multi {
            effective.allow_multiple
        } else {
            ui.allow_multiple
        };
    let has_free = ["allowFreeform", "allow_freeform", "allow_custom"]
        .iter()
        .any(|key| source.get(*key).is_some() || raw.get(*key).is_some());
    let freeform = !permission
        && kind != "confirm"
        && (kind == "input"
            || options.is_empty()
            || (has_free && (effective.allow_freeform || ui.allow_freeform)));
    if !permission && kind == "confirm" && options.is_empty() {
        options = ["Yes", "No"]
            .iter()
            .map(|title| UiDialogOption {
                title: Some((*title).into()),
                ..Default::default()
            })
            .collect();
    }
    if options.iter().any(|option| {
        option
            .title
            .as_deref()
            .is_none_or(|title| title.trim().is_empty())
    }) {
        unsupported = Some("请求包含没有有效标题的选项，无法提交回答。".into());
    }
    let input_only = kind == "input" || options.is_empty();
    (
        options,
        AskInput {
            multiple,
            freeform,
            input_only,
        },
        unsupported,
    )
}

fn ask_model(pending: &PendingDialog) -> AskModel {
    let ui = &pending.ui;
    let raw = serde_json::to_value(ui).unwrap_or(Value::Null);
    let permission = ui.purpose.as_deref() == Some("permission");
    let mut unsupported = None;
    let nested = if let Some(questions) = raw.get("questions") {
        if let Some(question) = questions
            .as_array()
            .filter(|qs| qs.len() == 1)
            .and_then(|qs| qs.first())
            .filter(|q| q.is_object())
        {
            Some(question.clone())
        } else {
            unsupported = Some("此请求包含多个问题或不完整的 questions 数据，当前无法安全回答；请取消并让 Agent 分别提问。".into());
            None
        }
    } else {
        None
    };
    let source = nested.as_ref().unwrap_or(&raw);
    let question = text_field(source, &["question", "summary", "title"])
        .or_else(|| text_field(&raw, &["question", "summary", "title", "message"]))
        .unwrap_or_default();
    if question.trim().is_empty() && unsupported.is_none() {
        unsupported = Some("请求未提供完整问题，无法提交回答。".into());
    }
    let kind = ui.kind.as_deref().unwrap_or("selector");
    let (options, input, option_error) = ask_options(pending, &raw, source, permission, kind);
    if option_error.is_some() {
        unsupported = option_error;
    }
    if permission && invalid_permission(pending, &raw, &options) {
        unsupported = Some("此授权请求不符合节点协议：需要 toolCallId、明确的单选 allow/reject 语义，且不能包含自由输入或多问题。".into());
    }
    if raw.get("supported").and_then(Value::as_bool) == Some(false)
        || raw
            .get("unsupported")
            .is_some_and(|v| v == &Value::Bool(true) || v.as_str().is_some())
    {
        unsupported = Some(
            text_field(&raw, &["unsupportedReason", "unsupported", "reason"])
                .unwrap_or_else(|| "节点不支持此对话请求。".into()),
        );
    }
    AskModel {
        question,
        context: text_field(source, &["context", "message"])
            .or_else(|| text_field(&raw, &["context", "message"]))
            .unwrap_or_default(),
        header: text_field(source, &["header"])
            .or_else(|| text_field(&raw, &["header"]))
            .unwrap_or_default(),
        input,
        options,
        permission,
        unsupported,
        placeholder: text_field(source, &["suggest", "placeholder"])
            .or_else(|| text_field(&raw, &["suggest", "placeholder"]))
            .unwrap_or_else(|| "填写你的想法…".into()),
    }
}

fn answer_value(model: &AskModel, selected: &[usize], draft: &str) -> Option<Value> {
    if model.unsupported.is_some() {
        return None;
    }
    let text = draft.trim();
    if !text.is_empty() && model.input.freeform {
        return Some(json!({"values":[text],"value":text,"text":text,"kind":"input"}));
    }
    if model.input.input_only
        || selected.is_empty()
        || (!model.input.multiple && selected.len() != 1)
    {
        return None;
    }
    let values: Option<Vec<_>> = selected
        .iter()
        .map(|index| model.options.get(*index)?.title.clone())
        .collect();
    Some(json!({"values":values?,"kind":"selector"}))
}

/// Key each card by scope + `request_id` in the parent. This component emits intent
/// only. A failed HTTP request keeps the selection/draft and releases the local
/// duplicate-submit latch when the parent supplies the error.
#[component]
pub fn AskCard(
    pending: PendingDialog,
    submitting: bool,
    error: Option<String>,
    onanswer: EventHandler<DialogAnswer>,
) -> Element {
    let mut selected = use_signal(Vec::<usize>::new);
    let mut draft = use_signal(String::new);
    let mut sent = use_signal(|| false);
    let mut validation = use_signal(|| None::<String>);
    let model = ask_model(&pending);
    use_effect(use_reactive!(|submitting, error| {
        if !submitting && error.is_some() {
            sent.set(false);
        }
    }));
    let backend_unsupported = error.as_deref().is_some_and(|text| {
        let text = text.to_ascii_lowercase();
        (text.contains("unsupported") && (text.contains("permission") || text.contains("dialog")))
            || text.contains("不支持授权")
    });
    let busy = submitting || *sent.read();
    let blocked = model.unsupported.is_some() || backend_unsupported;
    let request_id = pending.request_id.clone();
    // Encode arbitrary IDs without collisions, spaces or selector punctuation.
    let mut dom_id = String::from("ask-");
    for byte in request_id.as_bytes() {
        let _ = write!(dom_id, "{byte:02x}");
    }
    let submit_pending = pending.clone();
    let submit_id = request_id.clone();
    let cancel_id = request_id.clone();
    let selected_now = selected.read().clone();
    let valid = answer_value(&model, &selected_now, &draft.read()).is_some();
    let status = if submitting {
        "提交中…"
    } else if *sent.read() {
        "等待处理…"
    } else if blocked {
        "暂不支持"
    } else {
        "等待回答"
    };
    rsx! {
        article { class: "conversation-widget tool-call ask-card", "data-request-id": "{request_id}", aria_labelledby: "{dom_id}-title", aria_busy: if busy { "true" } else { "false" },
            div { class: "ask-card-heading", {glyph("ask")}
                strong { if model.permission { "工具执行需要授权" } else { "Agent 想确认一个问题" } }
                span { role: "status", "{status}" }
            }
            if !model.header.is_empty() { div { class: "ask-header", "{model.header}" } }
            h3 { id: "{dom_id}-title", if model.question.is_empty() { "请求内容不完整" } else { "{model.question}" } }
            if !model.context.is_empty() { div { class: "ask-context", {markdown_view(&model.context)} } }
            if model.permission {
                if let Some(tool_call_id) = pending.tool_call_id.as_ref() { p { class: "ask-tool-id", "工具调用：{tool_call_id}" } }
            }
            if let Some(reason) = model.unsupported.as_ref() { p { class: "ask-error", role: "alert", "{reason}" } }
            if backend_unsupported { p { class: "ask-error", role: "alert", "节点不支持此授权请求，请取消并检查节点能力。" } }
            form {
                onsubmit: move |event: FormEvent| {
                    event.prevent_default();
                    if busy || blocked || *sent.peek() { return; }
                    let current = ask_model(&submit_pending);
                    if let Some(answer) = answer_value(&current, &selected.read(), &draft.read()) {
                        sent.set(true); validation.set(None);
                        onanswer.call(DialogAnswer { request_id: submit_id.clone(), answer: Some(answer) });
                    } else { validation.set(Some("请选择选项或填写非空答案。".into())); }
                },
                if !model.options.is_empty() && !model.input.input_only {
                    fieldset { disabled: busy || blocked,
                        legend { if model.input.multiple { "可多选" } else if model.permission { "请选择一项授权决定" } else { "请选择一项" } }
                        for (index, option) in model.options.iter().enumerate() {
                            label { class: "ask-option", r#for: "{dom_id}-option-{index}",
                                input {
                                    id: "{dom_id}-option-{index}", name: "{dom_id}-choices", r#type: if model.input.multiple { "checkbox" } else { "radio" },
                                    aria_label: format!("{} · {}", request_id, option.title.as_deref().unwrap_or("无效选项")),
                                    checked: selected_now.contains(&index), disabled: busy || blocked,
                                    onchange: move |event: FormEvent| {
                                        if busy || blocked || *sent.peek() { return; }
                                        if model.input.multiple {
                                            let mut values = selected.write(); values.retain(|value| *value != index);
                                            if event.checked() { values.push(index); values.sort_unstable(); }
                                        } else { selected.set(vec![index]); }
                                        draft.set(String::new()); validation.set(None);
                                    },
                                }
                                span {
                                    strong { {option.title.as_deref().unwrap_or("无效选项")} }
                                    if let Some(description) = option.description.as_ref() { small { "{description}" } }
                                    if model.permission { small { class: "ask-permission", {permission_label(option.permission.as_deref())} } }
                                }
                            }
                        }
                    }
                }
                if model.input.freeform {
                    label { class: "ask-field-label", r#for: "{dom_id}-text", if model.input.input_only { "你的回答" } else { "其他答案（填写后将使用自由回答）" } }
                    textarea { id: "{dom_id}-text", class: "ask-textarea", rows: 2, aria_label: "{request_id} · 自由回答",
                        value: "{draft}", placeholder: "{model.placeholder}", disabled: busy || blocked,
                        oninput: move |event| {
                            if busy || blocked || *sent.peek() { return; }
                            draft.set(event.value()); selected.set(Vec::new()); validation.set(None);
                        },
                    }
                }
                if let Some(message) = validation.read().as_ref() { p { class: "ask-error", role: "alert", "{message}" } }
                if let Some(message) = error.as_ref() { p { class: "ask-error", role: "alert", "{message}" } }
                div { class: "ask-primary-actions",
                    button { r#type: "submit", class: "button primary", disabled: busy || blocked || !valid, aria_label: "{request_id} · 提交回答",
                        if submitting { "提交中…" } else if model.permission { "提交授权决定" } else { "提交回答" }
                    }
                    button { r#type: "button", class: "button", disabled: busy, aria_label: "{request_id} · 取消回答",
                        onclick: move |_| {
                            if busy || *sent.peek() { return; }
                            sent.set(true);
                            onanswer.call(DialogAnswer { request_id: cancel_id.clone(), answer: None });
                        }, "取消回答"
                    }
                }
            }
        }
    }
}

fn permission_label(permission: Option<&str>) -> &'static str {
    match permission {
        Some("allow_once") => "允许一次 · allow_once",
        Some("allow_always") => "始终允许 · allow_always",
        Some("reject_once") => "拒绝一次 · reject_once",
        Some("reject_always") => "始终拒绝 · reject_always",
        _ => "授权语义缺失或不受支持",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pending(ui: Value) -> PendingDialog {
        PendingDialog {
            request_id: "d".into(),
            tool_call_id: Some("t".into()),
            ui: serde_json::from_value(ui).unwrap(),
        }
    }
    #[test]
    fn answers_follow_tui_envelopes_and_never_infer_permissions() {
        let p = pending(
            json!({"question":"Pick", "options":[{"title":"Yes","value":"different"},"No"],"multiple":true}),
        );
        let m = ask_model(&p);
        assert!(!m.permission);
        assert_eq!(
            answer_value(&m, &[0, 1], ""),
            Some(json!({"values":["Yes","No"],"kind":"selector"}))
        );
        let input = ask_model(&pending(json!({"question":"Explain", "kind":"input"})));
        assert_eq!(
            answer_value(&input, &[], " hello "),
            Some(json!({"values":["hello"],"value":"hello","text":"hello","kind":"input"}))
        );
        assert_eq!(answer_value(&input, &[], " \n "), None);
        let permission = ask_model(&pending(
            json!({"purpose":"permission","kind":"selector","question":"Run?","options":[{"title":"Run once","permission":"allow_once"}]}),
        ));
        assert!(permission.unsupported.is_none());
        assert_eq!(
            answer_value(&permission, &[0], ""),
            Some(json!({"values":["Run once"],"kind":"selector"}))
        );
        let invalid = ask_model(&pending(
            json!({"purpose":"permission","kind":"selector","question":"Run?","options":["Yes","No"]}),
        ));
        assert!(invalid.unsupported.is_some());
        assert_eq!(answer_value(&invalid, &[0], ""), None);
        let invalid_alias = ask_model(&pending(
            json!({"purpose":"permission","kind":"selector","question":"Run?","options":[{"label":"Run","permission":"allow_once"}]}),
        ));
        assert!(invalid_alias.unsupported.is_some());
        let malformed = ask_model(&pending(
            json!({"question":"Pick", "options":{"title":"A"}}),
        ));
        assert!(malformed.unsupported.is_some());
    }
    #[test]
    fn nested_questions_are_explicit_and_batches_are_blocked() {
        let one = ask_model(&pending(
            json!({"questions":[{"question":"Events?","options":["A","B"],"allow_multiple":true}]}),
        ));
        assert_eq!(one.question, "Events?");
        assert!(one.input.multiple);
        assert!(one.unsupported.is_none());
        for questions in [json!([]), json!([{}, {}]), json!([{}]), json!("bad")] {
            assert!(
                ask_model(&pending(json!({"questions":questions})))
                    .unsupported
                    .is_some()
            );
        }
    }
    #[test]
    fn markdown_links_allow_only_explicit_safe_schemes() {
        for url in [
            "https://example.org/a?q=1",
            "http://localhost:3030",
            "mailto:a@example.org",
        ] {
            assert!(safe_link(url));
        }
        for url in [
            "javascript:alert(1)",
            "data:text/html,x",
            "//example.org",
            "https://",
            "https://x\n.test",
            "https://x\\@evil",
            "file:///a",
            "src/main.rs",
        ] {
            assert!(!safe_link(url));
        }
        assert_eq!(
            parse_inline("[ok](https://example.org) tail", 0),
            vec![
                Inline::Link("ok".into(), "https://example.org".into()),
                Inline::Text(" tail".into())
            ]
        );
        assert!(matches!(
            &parse_inline("<script>x</script>", 0)[0],
            Inline::Text(_)
        ));
    }
    #[test]
    fn thinking_is_hidden_and_plan_requires_real_structured_items() {
        assert_eq!(
            public_text(
                &json!([{"type":"thinking","text":"secret"},{"type":"text","text":"public"}])
            ),
            "public"
        );
        assert_eq!(
            plan_items("read", r#"{"items":[{"title":"x"}]}"#, "").len(),
            0
        );
        assert_eq!(plan_items("todo", "{}", "plain progress").len(), 0);
        let plan = plan_items(
            "update_plan",
            r#"{"plan":[{"step":"A","status":"completed"},{"step":"B","status":"in_progress"}]}"#,
            "",
        );
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].status, "completed");
        assert_eq!(plan[1].status, "running");
    }

    #[test]
    fn inspector_plan_prefers_live_projection_blocks() {
        let history = vec![json!({
            "role": "assistant",
            "content": [{
                "type": "toolCall",
                "name": "update_plan",
                "arguments": {"plan": [{"step": "历史计划", "status": "completed"}]}
            }]
        })];
        let blocks = vec![MessageBlock::ToolCall {
            id: "plan-live".into(),
            name: "update_plan".into(),
            arguments: r#"{"plan":[{"step":"直播计划","status":"in_progress"}]}"#.into(),
            output: String::new(),
            status: ToolStatus::Running,
        }];
        let plan = plan_for_session(&history, Some(&blocks));
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].title, "直播计划");
        assert_eq!(plan[0].status, "running");
    }

    #[test]
    fn inspector_plan_falls_back_to_native_assistant_tool_call_history() {
        let history = vec![json!({
            "role": "assistant",
            "content": [{
                "type": "toolCall",
                "name": "update_plan",
                "arguments": {"plan": [{"step": "历史步骤", "status": "completed"}]}
            }]
        })];
        let plan = plan_for_session(&history, Some(&[]));
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].title, "历史步骤");
        assert_eq!(plan[0].status, "completed");
    }
}
