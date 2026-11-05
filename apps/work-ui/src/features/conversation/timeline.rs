//! Standalone conversation widgets. The parent owns history deduplication, HTTP,
//! and scope/request keys. Render functions consume the current snapshot only.
use super::message_image::{ImageBlock, MessageImage, image_block};
use super::message_presentation::{CodeBlock, CopyButton, task_item};
use crate::features::observability::traces::{ToolTiming, tool_timing};
use crate::shared::components::Icon;
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
fn public_block_text(item: &Value) -> Option<&str> {
    match item.get("type") {
        Some(Value::String(kind))
            if matches!(kind.as_str(), "text" | "input_text" | "output_text") =>
        {
            item.get("text").and_then(Value::as_str)
        }
        None if item.get("thinking").is_none() => item.get("text").and_then(Value::as_str),
        _ => None,
    }
}

fn public_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(public_block_text)
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

#[derive(Debug, PartialEq)]
enum PublicPart {
    Text(String),
    Image(Result<ImageBlock, String>),
}

fn public_parts(value: &Value) -> Vec<PublicPart> {
    let Some(items) = value.as_array() else {
        let text = public_text(value);
        return if text.trim().is_empty() {
            Vec::new()
        } else {
            vec![PublicPart::Text(text)]
        };
    };
    let mut parts = Vec::new();
    for item in items {
        if let Some(image) = image_block(item) {
            parts.push(PublicPart::Image(image));
        } else {
            let text = public_block_text(item).unwrap_or_default();
            if !text.is_empty() {
                if let Some(PublicPart::Text(previous)) = parts.last_mut() {
                    previous.push_str(text);
                } else {
                    parts.push(PublicPart::Text(text.to_owned()));
                }
            }
        }
    }
    // Test after joining adjacent text chunks: a standalone streamed "\n"
    // can be the authored separator between two nonblank chunks. Keep that
    // whitespace verbatim, but do not give whitespace-only parts visual rows.
    parts.retain(|part| !matches!(part, PublicPart::Text(text) if text.trim().is_empty()));
    parts
}

// Split at tool boundaries before collecting public chunks: text must never
// jump across a call, and private thinking must never become public prose.
#[derive(Debug, PartialEq)]
enum HistoryPart {
    Public(PublicPart),
    Tool(usize, Value),
}

fn history_parts(content: &Value) -> Vec<HistoryPart> {
    let Some(items) = content.as_array() else {
        return public_parts(content)
            .into_iter()
            .map(HistoryPart::Public)
            .collect();
    };
    let mut parts = Vec::new();
    let mut pending = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if matches!(
            item["type"].as_str(),
            Some("toolCall" | "tool_use" | "tool_call")
        ) {
            parts.extend(
                public_parts(&Value::Array(std::mem::take(&mut pending)))
                    .into_iter()
                    .map(HistoryPart::Public),
            );
            parts.push(HistoryPart::Tool(index, item.clone()));
        } else {
            pending.push(item.clone());
        }
    }
    parts.extend(
        public_parts(&Value::Array(pending))
            .into_iter()
            .map(HistoryPart::Public),
    );
    parts
}

// Both projection and history use this exact keyed sibling list. In particular,
// a tool-only projection becoming history must not acquire a new article parent
// and reset the native disclosure state.
fn render_regions(regions: Vec<(String, Element)>) -> Element {
    rsx! {
        for (key, content) in regions {
            div {
                key: "{key}",
                class: if key.starts_with("tool-") { "timeline-content-region timeline-tool-region timeline-message" } else { "timeline-content-region" },
                {content}
            }
        }
    }
}

/// Preserve image-bearing tool payloads only until the tool renderer separates
/// public text from raster blocks. Never copy or display their encoded data.
fn history_tool_output(message: &Value) -> String {
    let content = message.get("content").or_else(|| message.get("result"));
    if let Some(content) = content {
        let body = content.get("content").unwrap_or(content);
        if body["type"] == "image"
            || body
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["type"] == "image"))
        {
            return serde_json::to_string(content).unwrap_or_default();
        }
    }
    let text = message.get("content").map(public_text).unwrap_or_default();
    if text.is_empty() {
        message.get("result").map(public_result).unwrap_or_default()
    } else {
        text
    }
}

fn tool_output_parts(output: &str) -> (String, Vec<Result<ImageBlock, String>>) {
    if let Ok(value) = serde_json::from_str::<Value>(output) {
        let body = value.get("content").unwrap_or(&value);
        if let Some(image) = image_block(body) {
            return (String::new(), vec![image]);
        }
        if let Some(items) = body.as_array() {
            let images: Vec<_> = items.iter().filter_map(image_block).collect();
            if !images.is_empty() {
                return (public_text(body), images);
            }
        }
    }
    (output.to_owned(), Vec::new())
}

fn assistant_heading() -> Element {
    rsx! {
        div { class: "message-heading assistant-message-heading",
            span { class: "assistant-avatar", aria_hidden: "true",
                Icon { name: "code" }
            }
            strong { "rpi" }
        }
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

fn history_role(message: &Value) -> &str {
    message
        .get("role")
        .or_else(|| message.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("")
}

pub fn render_history_in_context(
    message: &Value,
    messages: &[Value],
    blocks: &[MessageBlock],
) -> Element {
    let role = history_role(message);
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
        return render_tool_result(message);
    }
    let user = role == "user";
    if user {
        let mut parts = message.get("content").map(public_parts).unwrap_or_default();
        if parts.is_empty() && !text.trim().is_empty() {
            parts.push(PublicPart::Text(text));
        }
        if parts.is_empty() {
            return rsx! {};
        }
        return rsx! {
            article { class: "conversation-widget message timeline-message user-message", aria_label: "你的消息",
                div { class: "message-heading user-message-heading", strong { "你" } }
                div { class: "message-content",
                    for (index, part) in parts.into_iter().enumerate() {
                        match part {
                            PublicPart::Text(text) => rsx! { div { key: "public-text-{index}", class: "message-text-part", div { class: "conversation-user-text", "{text}" } } },
                            PublicPart::Image(image) => rsx! { MessageImage { key: "public-image-{index}", image } },
                        }
                    }
                }
            }
        };
    }
    let parts = message
        .get("content")
        .map(history_parts)
        .unwrap_or_else(|| history_parts(&Value::String(text)));
    let regions = parts
        .into_iter()
        .enumerate()
        .map(|(index, part)| match part {
            HistoryPart::Public(part) => (format!("public-{index}"), render_public_part(part)),
            HistoryPart::Tool(source_index, item) => {
                let name =
                    text_field(&item, &["name", "toolName"]).unwrap_or_else(|| "工具调用".into());
                let args = item
                    .get("arguments")
                    .or_else(|| item.get("input"))
                    .or_else(|| item.get("args"))
                    .map(pretty_value)
                    .unwrap_or_default();
                let key = text_field(&item, &["id", "toolCallId"])
                    .unwrap_or_else(|| format!("content-{source_index}"));
                let timing = tool_timing(&item).filter(|timing| !timing.running);
                let result = messages.iter().rev().find(|item| {
                    matches!(history_role(item), "tool" | "toolResult" | "tool_result")
                        && item.get("toolCallId").and_then(Value::as_str) == Some(key.as_str())
                });
                let view = if let Some(result) = result {
                    let output = history_tool_output(result);
                    let status = if result.get("isError").and_then(Value::as_bool) == Some(true) {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Completed
                    };
                    let timing = tool_timing(result)
                        .filter(|timing| !timing.running)
                        .or(timing);
                    tool_view_with_timing(&key, &name, &args, &output, &status, timing)
                } else if let Some(MessageBlock::ToolCall {
                    name,
                    arguments,
                    output,
                    status,
                    ..
                }) = blocks.iter().find(|block| block.id() == key)
                {
                    tool_view_with_timing(&key, name, arguments, output, status, timing)
                } else {
                    tool_view_with_label(&key, &name, &args, "", "等待工具结果", false, timing)
                };
                (format!("tool-{key}"), view)
            }
        })
        .collect();
    render_regions(regions)
}

fn render_public_part(part: PublicPart) -> Element {
    match part {
        PublicPart::Text(text) => rsx! {
            article { class: "conversation-widget message timeline-message agent-message", aria_label: "助手消息",
                {assistant_heading()}
                div { class: "message-content",
                    div { class: "message-text-part", {markdown_view(&text)} }
                }
            }
        },
        PublicPart::Image(image) => rsx! {
            div { class: "timeline-image-region", MessageImage { image } }
        },
    }
}

fn render_tool_result(message: &Value) -> Element {
    let name = text_field(message, &["toolName", "name"]).unwrap_or_else(|| "工具结果".into());
    let output = history_tool_output(message);
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
    let id = text_field(message, &["toolCallId", "tool_call_id"]).unwrap_or_default();
    render_regions(vec![(
        format!("tool-{id}"),
        tool_view_with_timing(
            &id,
            &name,
            &args,
            &output,
            &status,
            tool_timing(message).filter(|timing| !timing.running),
        ),
    )])
}

pub fn render_message_block(block: &MessageBlock) -> Element {
    render_message_block_with_timing(block, None)
}

fn render_message_block_with_timing(block: &MessageBlock, timing: Option<ToolTiming>) -> Element {
    match block {
        // Empty snapshots still clear projection state and retain their stable
        // slot; they must not create an avatar, a message gap or a blank line.
        MessageBlock::Text { text, .. } if text.trim().is_empty() => rsx! {},
        MessageBlock::Text { text, .. } => rsx! {
            article { class: "conversation-widget message timeline-message agent-message", aria_label: "助手消息",
                {assistant_heading()}
                div { class: "message-content",
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
            id,
            name,
            arguments,
            output,
            status,
        } => render_regions(vec![(
            format!("tool-{id}"),
            tool_view_with_timing(id, name, arguments, output, status, timing),
        )]),
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
    super::tool_display::target(name, args)
}

fn tool_result_hint(label: &str, failed: bool, has_output: bool) -> &'static str {
    if failed { "查看错误" } else if label == "已取消" { "已取消" } else if label == "运行中" || label == "等待工具结果" { "等待输出" } else if has_output { "有结果" } else { "无输出" }
}

/// Empty protocol payloads do not deserve an empty details section. Keep
/// false/zero, nonempty collections and partial streamed JSON inspectable.
fn has_tool_arguments(args: &str) -> bool {
    if args.trim().is_empty() {
        return false;
    }
    match serde_json::from_str::<Value>(args) {
        Ok(Value::Null) => false,
        Ok(Value::String(value)) => !value.trim().is_empty(),
        Ok(Value::Array(value)) => !value.is_empty(),
        Ok(Value::Object(value)) => !value.is_empty(),
        _ => true,
    }
}

fn tool_icon(name: &str) -> &'static str {
    super::tool_display::icon(name)
}

fn tool_view_with_timing(
    id: &str,
    name: &str,
    args: &str,
    output: &str,
    status: &ToolStatus,
    timing: Option<ToolTiming>,
) -> Element {
    let label = match status {
        ToolStatus::Running => "运行中",
        ToolStatus::Completed => "已完成",
        ToolStatus::Failed => "失败",
        ToolStatus::Cancelled => "已取消",
    };
    tool_view_with_label(
        id,
        name,
        args,
        output,
        label,
        *status == ToolStatus::Failed,
        timing,
    )
}

fn tool_view_with_label(
    id: &str,
    name: &str,
    args: &str,
    output: &str,
    label: &str,
    failed: bool,
    timing: Option<ToolTiming>,
) -> Element {
    rsx! {
        ToolCallView {
            key: "tool-{id}",
            id: id.to_owned(), name: name.to_owned(), args: args.to_owned(),
            output: output.to_owned(), label: label.to_owned(), failed, timing,
        }
    }
}

#[component]
fn ToolCallView(
    id: String,
    name: String,
    args: String,
    output: String,
    label: String,
    failed: bool,
    timing: Option<ToolTiming>,
) -> Element {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
    let instance = use_hook(|| NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed));
    let disclosure_id = format!("rpi-tool-disclosure-{instance}");
    // Freeze the native initial attribute. Later output/timing updates cannot
    // undo user toggles; only a transition into failure opens the same row once.
    let initial_open = use_signal(|| failed);
    let mount_id = serde_json::to_string(&disclosure_id).unwrap();
    let effect_id = disclosure_id.clone();
    use_effect(use_reactive((&failed,), move |(failed,)| {
        let encoded = serde_json::to_string(&effect_id).unwrap();
        let _ = document::eval(&format!(
            "const row=document.getElementById({encoded});if(row){{if(row.__rpiToolDisclosureFailure)row.__rpiToolDisclosureFailure({failed});else if({failed})row.open=true;}}"
        ));
    }));
    let (output, images) = tool_output_parts(&output);
    let has_output = !output.trim().is_empty() || !images.is_empty();
    let result_hint = tool_result_hint(&label, failed, has_output);
    let has_arguments = has_tool_arguments(&args);
    let args =
        serde_json::from_str::<Value>(&args).map_or_else(|_| args.clone(), |v| pretty_value(&v));
    let summary = tool_summary(&name, &args);
    let command = super::command_preview::command_argument(&args);
    let plan = plan_items(&name, &args, &output);
    let preview = super::tool_display::edit_preview(&name, &args);
    let edit_counts = preview
        .as_ref()
        .map(super::tool_display::EditPreview::line_counts);
    let reading = super::tool_display::is_read(&name);
    let skill_read = super::tool_display::is_skill_read(&name, &args);
    let action = if skill_read {
        super::tool_display::skill_read_action(&label, failed)
    } else {
        &name
    };
    let shell = super::tool_display::is_shell(&name);
    let running = matches!(label.as_str(), "运行中" | "等待工具结果");
    let state_class = if failed {
        "tool-error failed"
    } else if running {
        "tool-running"
    } else if label == "已取消" {
        "tool-cancelled"
    } else {
        "tool-done"
    };
    let kind_class = if shell {
        "shell-card"
    } else if skill_read {
        "read-card skill-card"
    } else if reading {
        "read-card"
    } else if preview.is_some() {
        "edit-card"
    } else {
        ""
    };
    let finished = plan
        .iter()
        .filter(|item| item.status == "completed")
        .count();
    rsx! {
        div { class: "conversation-widget conversation-tool", aria_busy: running,
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
            details { id: "{disclosure_id}", class: "tool-call inline-call tool-card {state_class} {kind_class}", open: initial_open(), "data-tool-id": "{id}",
                onmounted: move |_| {
                    let script = format!("const id={mount_id};const failed={failed};{}", include_str!("tool_disclosure.js"));
                    let _ = document::eval(&script);
                },
                summary {
                    span { class: "call-icon call-status tool-status", title: "{label}", role: "img", aria_label: "{label}",
                        if running { span { class: "tool-running-spinner", aria_hidden: "true" } }
                        else { {glyph(if failed { "info" } else if label == "已取消" { "close" } else if skill_read { "skills" } else { tool_icon(&name) })} }
                    }
                    div { class: "call-name tool-summary-text",
                        strong { class: "tool-action", title: "{name}", "{action}" }
                        if !summary.is_empty() { code { class: "tool-target", title: "{summary}", "{summary}" } }
                    }
                    if let Some((added, removed)) = edit_counts {
                        span { class: "tool-summary-meta edit-count", title: "调用参数中的增删行数预览，不代表实际落盘改动", aria_label: "变更预览：新增 {added} 行，删除 {removed} 行",
                            span { class: "add", "+{added}" }
                            span { class: "remove", "−{removed}" }
                        }
                    }
                    super::turn_timing::ToolElapsed { id: id.clone(), timing }
                    span { class: "tool-result-hint", role: "status", aria_label: "结果状态：{result_hint}", aria_live: "polite", "{result_hint}" }
                    if !failed && label == "已完成" {
                        span { class: "tool-success-mark", aria_hidden: "true", {glyph("check")} }
                    }
                    {glyph("chevron-right")}
                }
                if failed { div { class: "inline-call-warning", role: "status", "工具执行失败，请查看工具结果。" } }
                div { class: "call-body tool-detail",
                    if let Some(command) = command {
                        super::command_preview::CommandPreview { command }
                        details { class: "command-raw-arguments",
                            summary { aria_label: "完整调用参数", title: "完整调用参数",
                                ToolLabel { label: "完整调用参数", icon: "settings" }
                                {glyph("chevron")}
                            }
                            pre { class: "call-arguments", "{args}" }
                        }
                    } else if has_arguments && preview.is_some() {
                        details { class: "command-raw-arguments tool-raw-arguments",
                            summary { aria_label: "完整调用参数", title: "完整调用参数",
                                ToolLabel { label: "完整调用参数", icon: "settings" }
                                {glyph("chevron")}
                            }
                            pre { class: "call-arguments", "{args}" }
                        }
                    } else if has_arguments {
                        div { class: "call-section-title", ToolLabel { label: "调用参数", icon: "settings" } }
                        pre { class: "call-arguments", "{args}" }
                    }
                    if let Some(preview) = preview {
                        div { class: "tool-edit-preview",
                            div { class: "call-section-title", "变更预览 · 调用参数" }
                            if !preview.path.is_empty() { div { class: "tool-read-path", "{preview.path}" } }
                            div { class: "tool-edit-lines",
                                for (index, (old, new)) in preview.changes.into_iter().enumerate() {
                                    div { key: "edit-{index}", class: "tool-edit-hunk",
                                        for line in old.lines() { div { class: "tool-edit-line remove", span { "−" } code { "{line}" } } }
                                        for line in new.lines() { div { class: "tool-edit-line add", span { "+" } code { "{line}" } } }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "call-section-title conversation-output-heading",
                        ToolLabel { label: "工具结果", icon: "terminal" }
                        CopyOutput { text: output.clone() }
                    }
                    if reading && !summary.is_empty() { div { class: "tool-read-path", "{summary}" } }
                    if !failed && !reading && !shell && super::tool_display::is_markdown(&output) {
                        div { class: "call-output tool-markdown-output", {markdown_view(&output)} }
                    } else {
                        pre { class: if failed { "call-output error-text" } else { "call-output" },
                            if output.is_empty() && images.is_empty() {
                                span { class: "tool-empty-output", if running { "等待工具输出…" } else { "无输出" } }
                            } else { "{output}" }
                        }
                    }
                    for (index, image) in images.into_iter().enumerate() {
                        MessageImage { key: "tool-image-{index}", image }
                    }
                }
            }
        }
    }
}

fn glyph(name: &str) -> Element {
    rsx! { Icon { name: name.to_owned() } }
}

/// Small visual marker with a name available to assistive technology and hover.
#[component]
pub(super) fn ToolLabel(label: String, icon: String) -> Element {
    rsx! {
        span { class: "tool-label", role: "img", aria_label: "{label}", title: "{label}",
            Icon { name: icon }
            span { class: "tool-label-text", "{label}" }
        }
    }
}

#[component]
pub(super) fn CopyOutput(text: String, #[props(default)] command: bool) -> Element {
    rsx! {
        CopyButton {
            text,
            label: if command { "复制完整命令" } else { "复制工具输出" },
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
    FileLink(String, String),
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
                    if safe_link(target)
                        || super::message_links::looks_like_file(target)
                        || super::message_links::resolve_file("/__message_root", target).is_some()
                    {
                        let label = after[..end].to_owned();
                        let node = if safe_link(target) {
                            Inline::Link(label, target.into())
                        } else {
                            Inline::FileLink(label, target.into())
                        };
                        matched = Some((end + close + 4, node));
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
        if matched.is_none()
            && literal.chars().last().is_none_or(|c| {
                !c.is_alphanumeric() && !matches!(c, '_' | '/' | ':' | '.' | '@' | '-')
            })
        {
            if let Some(url) = super::message_links::bare_url(rest) {
                matched = Some((url.len(), Inline::Link(url.into(), url.into())));
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

// A source newline has one rendering owner. Emit one explicit break rather
// than relying on inherited pre-wrap (which can also preserve DOM whitespace).
// Code blocks and plain user messages keep their own verbatim whitespace.
fn inline_text(text: &str) -> Element {
    rsx! {
        for (index, line) in text.split('\n').enumerate() {
            Fragment { key: "line-{index}",
                if index > 0 { br {} }
                "{line}"
            }
        }
    }
}

fn inline_nodes(nodes: &[Inline]) -> Element {
    rsx! { for (index, node) in nodes.iter().enumerate() {
        Fragment { key: "inline-{index}",
            {match node {
                Inline::Text(text) => inline_text(text),
                Inline::Code(text) => if super::message_links::looks_like_file(text) {
                    rsx! { super::message_links::MessageFileLink { label: text.clone(), target: text.clone(), code: true } }
                } else { rsx! { code { "{text}" } } },
                Inline::Strong(children) => rsx! { strong { {inline_nodes(children)} } },
                Inline::Link(label, url) => rsx! { a { href: "{url}", rel: "noopener noreferrer", target: "_blank", {inline_text(label)} } },
                Inline::FileLink(label, target) => rsx! { super::message_links::MessageFileLink { label: label.clone(), target: target.clone(), code: false } },
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

fn list_item(text: &str) -> Element {
    if let Some((checked, content)) = task_item(text) {
        rsx! {
            li { class: "task-list-item",
                input { r#type: "checkbox", checked, disabled: true,
                    aria_label: if content.is_empty() { "任务状态" } else { content },
                }
                span { {inline(content)} }
            }
        }
    } else {
        rsx! { li { {inline(text)} } }
    }
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
            let closed = i < lines.len();
            if closed {
                i += 1;
            }
            blocks.push(if super::mermaid::is_mermaid(&language) {
                rsx! { super::mermaid::MermaidDiagram { source: code, closed } }
            } else {
                rsx! { CodeBlock { source: code, language } }
            });
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
                rsx! { ol { start: "{start}", for (index, item) in items.iter().enumerate() { Fragment { key: "{index}", {list_item(item)} } } } }
            } else {
                rsx! { ul { for (index, item) in items.iter().enumerate() { Fragment { key: "{index}", {list_item(item)} } } } }
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

    #[test]
    fn empty_tool_arguments_hide_only_empty_payloads() {
        for empty in [
            "", " \n ", "null", "{}", "{ }", "[]", "[ ]", "\"\"", "\"  \"",
        ] {
            assert!(!has_tool_arguments(empty), "empty payload {empty:?}");
        }
        for meaningful in [
            "false",
            "0",
            "[null]",
            "{\"path\":null}",
            "{\"path\":\"\"}",
            "\"null\"",
            "{\"path\":",
            "plain argument",
        ] {
            assert!(
                has_tool_arguments(meaningful),
                "meaningful payload {meaningful:?}"
            );
        }
    }

    #[test]
    fn tool_icons_use_explicit_semantics_with_an_unknown_fallback() {
        for (name, icon) in [
            ("bash", "terminal"),
            ("Bash", "terminal"),
            ("read_file", "file"),
            ("write", "edit"),
            ("apply_patch", "edit"),
            ("grep", "search"),
            ("websearch", "search"),
            ("update_plan", "plan"),
            ("mcp_custom__read", "tool"),
            ("unfamiliar", "tool"),
        ] {
            assert_eq!(tool_icon(name), icon, "{name}");
        }
    }

    #[test]
    fn public_content_preserves_text_image_order_and_hides_private_blocks() {
        let parts = public_parts(&json!([
            {"type":"text","text":"before"}, {"text":" adjacent"},
            {"type":"image","mimeType":"image/png","data":"AA=="},
            {"type":"thinking","text":"private","content":[{"type":"image","data":"hidden"}]},
            {"type":"text","text":"after"},
            {"type":"image","mimeType":"image/svg+xml","data":"secret"}
        ]));
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], PublicPart::Text("before adjacent".into()));
        assert!(matches!(&parts[1], PublicPart::Image(Ok(image)) if image.data == "AA=="));
        assert_eq!(parts[2], PublicPart::Text("after".into()));
        assert!(matches!(&parts[3], PublicPart::Image(Err(error)) if !error.contains("secret")));
        assert_eq!(
            public_parts(&json!([{"type":"text","text":"a"},{"type":"text","text":"b"}])),
            vec![PublicPart::Text("ab".into())]
        );
    }

    #[test]
    fn history_regions_follow_content_order_without_crossing_tools() {
        let call = json!({"type":"toolCall","id":"a","name":"bash","arguments":{}});
        let parts = history_parts(&json!([
            {"type":"text","text":"first"}, {"type":"text","text":"\n"},
            {"type":"text","text":"second"}, call,
            {"type":"thinking","text":"private"},
            {"type":"text","text":"after\n\ncall"},
            {"type":"image","mimeType":"image/png","data":"AA=="},
            {"type":"tool_use","id":"b","name":"read"},
            {"type":"text","text":"\r\n \t"},
            {"type":"tool_call","id":"c","name":"read"},
            {"type":"image","mimeType":"image/svg+xml","data":"secret"}
        ]));
        assert_eq!(parts.len(), 7);
        assert_eq!(
            parts[0],
            HistoryPart::Public(PublicPart::Text("first\nsecond".into()))
        );
        assert_eq!(parts[1], HistoryPart::Tool(3, call));
        assert_eq!(
            parts[2],
            HistoryPart::Public(PublicPart::Text("after\n\ncall".into()))
        );
        assert!(
            matches!(&parts[3], HistoryPart::Public(PublicPart::Image(Ok(image))) if image.data == "AA==")
        );
        assert!(matches!(&parts[4], HistoryPart::Tool(7, call) if call["id"] == "b"));
        assert!(matches!(&parts[5], HistoryPart::Tool(9, call) if call["id"] == "c"));
        assert!(
            matches!(&parts[6], HistoryPart::Public(PublicPart::Image(Err(error))) if !error.contains("secret"))
        );
        assert!(
            history_parts(
                &json!([{"type":"thinking","text":"private"},{"type":"text","text":"\n"}])
            )
            .is_empty()
        );
        assert_eq!(
            history_parts(&json!("real\n\ntext")),
            vec![HistoryPart::Public(PublicPart::Text("real\n\ntext".into()))]
        );
    }

    #[test]
    fn whitespace_only_public_parts_do_not_turn_tools_into_prose() {
        for content in [
            json!(" \r\n\t"),
            json!([{"type":"text","text":"\n"}, {"type":"text","text":" \t"}]),
            json!([
                {"type":"text","text":"\r\n\n"},
                {"type":"thinking","text":"private"},
                {"type":"toolCall","id":"t","name":"bash","arguments":{}}
            ]),
        ] {
            assert!(public_parts(&content).is_empty(), "{content}");
        }
        let parts = public_parts(&json!([
            {"type":"text","text":"  first"},
            {"type":"text","text":"\n"},
            {"type":"text","text":"second  "},
            {"type":"image","mimeType":"image/png","data":"AA=="},
            {"type":"text","text":"\n\n"}
        ]));
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], PublicPart::Text("  first\nsecond  ".into()));
        assert!(matches!(&parts[1], PublicPart::Image(Ok(_))));
        assert_eq!(
            public_parts(&json!("  first\n\nsecond  ")),
            vec![PublicPart::Text("  first\n\nsecond  ".into())]
        );
        // Tool output uses public_text, not the whitespace-only prose filter.
        assert_eq!(public_text(&json!([{"type":"text","text":"\n\n"}])), "\n\n");
    }

    #[test]
    fn tool_images_are_separated_from_display_and_copy_text() {
        let result = json!({"content":[
            {"type":"text","text":"result"},
            {"type":"image","mimeType":"image/png","data":"AA=="},
            {"type":"thinking","text":"private"}
        ],"details":{"secret":"not-public"}});
        let raw = history_tool_output(&result);
        let (text, images) = tool_output_parts(&raw);
        assert_eq!(text, "result");
        assert_eq!(images.len(), 1);
        assert_eq!(
            tool_output_parts(&result["content"].to_string()),
            (text, images)
        );
        assert_eq!(
            tool_output_parts("plain output"),
            ("plain output".into(), Vec::new())
        );
        assert_eq!(
            history_tool_output(&json!({"content":[],"result":"fallback"})),
            "fallback"
        );
    }

    fn pending(ui: Value) -> PendingDialog {
        PendingDialog {
            request_id: "d".into(),
            tool_call_id: Some("t".into()),
            response_id: None,
            ui: serde_json::from_value(ui).unwrap(),
        }
    }
    #[test]
    fn acp_permissions_render_as_single_choice_and_preserve_negotiated_ids() {
        let original = json!({
            "sessionId":"s", "toolCall":{"toolCallId":"t","title":"Execute tool?"},
            "options":[
                {"optionId":"deny-7","name":"Reject","kind":"reject_once"},
                {"optionId":"grant-42","name":"Allow","kind":"allow_once"}
            ]
        });
        let mut permission = pending(json!({
            "purpose":"permission", "kind":"selector", "question":"Execute tool?",
            "options":[
                {"title":"Reject","permission":"reject_once","optionId":"deny-7"},
                {"title":"Allow","permission":"allow_once","optionId":"grant-42"}
            ],
            "acpPermission":original
        }));
        permission.response_id = Some(json!("permission-4"));
        let model = ask_model(&permission);
        assert!(model.permission);
        assert!(model.unsupported.is_none(), "{:?}", model.unsupported);
        assert!(!model.input.freeform && !model.input.multiple);
        for (index, id) in [(0, "deny-7"), (1, "grant-42")] {
            let answer = answer_value(&model, &[index], "").unwrap();
            assert_eq!(
                permission.acp_permission_result(Some(&answer)).unwrap(),
                json!({"outcome":{"outcome":"selected","optionId":id}})
            );
        }
        assert!(answer_value(&model, &[], "allow").is_none());
        assert!(answer_value(&model, &[0, 1], "").is_none());
        assert_eq!(
            permission.acp_permission_result(None).unwrap(),
            json!({"outcome":{"outcome":"cancelled"}})
        );
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
