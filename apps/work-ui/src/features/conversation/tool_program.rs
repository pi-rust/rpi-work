use base64::Engine;
use dioxus::prelude::*;
use futures_util::StreamExt;
use serde_json::Value;
use sha2::{Digest, Sha256};
use web_time::Instant;
use work_client::WorkspaceClient;
use work_core::api::{
    ToolProgramBodyQuery, ToolProgramBodyResponse, ToolProgramChildrenQuery, ToolProgramPhase,
    ToolProgramPhasePage, ToolProgramPhasesQuery, ToolProgramSnapshot,
};

fn body_view_kind(kind: &str) -> &'static str {
    let kind = kind.to_ascii_lowercase();
    if kind.contains("json") {
        "JSON"
    } else if kind.contains("markdown") || kind == "md" {
        "Markdown"
    } else if kind.contains("code")
        || kind.contains("source")
        || body_code_language(&kind, "") != "text"
    {
        "Code"
    } else if kind.contains("text") {
        "Text"
    } else {
        "Binary"
    }
}

fn body_code_language(kind: &str, codec: &str) -> &'static str {
    for value in [kind, codec] {
        let value = value.to_ascii_lowercase();
        let tokens = value.split(|character: char| !character.is_ascii_alphanumeric());
        for token in tokens {
            let language = match token {
                "rust" | "rs" => "rust",
                "javascript" | "js" => "javascript",
                "typescript" | "ts" => "typescript",
                "python" | "py" => "python",
                "json" => "json",
                "shell" | "sh" | "bash" => "shell",
                "html" => "html",
                "css" => "css",
                "sql" => "sql",
                _ => continue,
            };
            return language;
        }
    }
    "text"
}

fn body_display_text(kind: &str, text: String) -> String {
    if kind.to_ascii_lowercase().contains("json") {
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            return serde_json::to_string_pretty(&value).unwrap_or(text);
        }
    }
    text
}

fn body_is_complete(has_next_cursor: bool, byte_length: u64, total_bytes: u64) -> bool {
    !has_next_cursor && byte_length == total_bytes
}

fn body_chunk_bytes(data: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(data).ok()
}

fn body_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn body_chunk_sha256(data: &str) -> Option<String> {
    Some(body_sha256(&body_chunk_bytes(data)?))
}

const MAX_REALTIME_RETRIES: u32 = 8;
const MAX_REALTIME_RETRY_WINDOW_MS: u128 = 60_000;

fn realtime_retry_delay_ms(attempt: u32) -> u64 {
    500_u64.saturating_mul(1_u64 << attempt.min(4))
}

async fn realtime_retry_delay(attempt: u32) {
    let millis = realtime_retry_delay_ms(attempt);
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(millis as u32).await;
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
}

async fn page_is_visible() -> bool {
    document::eval("try { dioxus.send(document.visibilityState !== 'hidden'); } catch (_) { dioxus.send(true); }")
        .recv::<bool>()
        .await
        .unwrap_or(true)
}

async fn sync_timestamp() -> Option<String> {
    document::eval("try { dioxus.send(new Date().toISOString()); } catch (_) { dioxus.send(''); }")
        .recv::<String>()
        .await
        .ok()
        .filter(|value| !value.is_empty())
}

fn is_tool_program_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "tool_program_child_begin"
            | "tool_program_child_phase"
            | "tool_program_child_update"
            | "tool_program_child_resync_required"
            | "resync_required"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        body_chunk_sha256, body_code_language, body_display_text, body_is_complete, body_sha256,
        body_view_kind, is_tool_program_event, realtime_retry_delay_ms,
    };

    #[test]
    fn body_sha256_matches_decoded_payload() {
        assert_eq!(
            body_sha256(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn body_is_complete_gates_text_actions() {
        assert!(body_is_complete(false, 5, 5));
        assert!(!body_is_complete(true, 5, 5));
        assert!(!body_is_complete(false, 4, 5));
    }

    #[test]
    fn body_code_language_uses_explicit_descriptor_tokens() {
        assert_eq!(body_code_language("code/rust", "utf-8"), "rust");
        assert_eq!(
            body_code_language("source", "text/javascript"),
            "javascript"
        );
        assert_eq!(body_code_language("code", "opaque"), "text");
    }

    #[test]
    fn body_view_kind_uses_descriptor_kind() {
        assert_eq!(body_view_kind("application/json"), "JSON");
        assert_eq!(body_view_kind("application/javascript"), "Code");
        assert_eq!(body_view_kind("markdown"), "Markdown");
        assert_eq!(body_view_kind("source-code"), "Code");
        assert_eq!(body_view_kind("application/octet-stream"), "Binary");
    }

    #[test]
    fn json_body_text_is_pretty_printed_only_for_json_kind() {
        assert_eq!(
            body_display_text("json", "{\"a\":1}".into()),
            "{\n  \"a\": 1\n}"
        );
        assert_eq!(body_display_text("text", "{\"a\":1}".into()), "{\"a\":1}");
    }

    #[test]
    fn realtime_retry_delay_caps_at_eight_seconds() {
        assert_eq!(realtime_retry_delay_ms(0), 500);
        assert_eq!(realtime_retry_delay_ms(1), 1000);
        assert_eq!(realtime_retry_delay_ms(4), 8000);
        assert_eq!(realtime_retry_delay_ms(99), 8000);
    }

    #[test]
    fn body_chunk_sha256_uses_decoded_bytes() {
        assert_eq!(
            body_chunk_sha256("aGVsbG8="),
            Some("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into())
        );
        assert_eq!(body_chunk_sha256("not-base64"), None);
    }

    #[test]
    fn realtime_filter_accepts_only_native_child_events() {
        assert!(is_tool_program_event("tool_program_child_phase"));
        assert!(is_tool_program_event("tool_program_child_resync_required"));
        assert!(is_tool_program_event("tool_program_child_update"));
        assert!(is_tool_program_event("resync_required"));
        assert!(!is_tool_program_event("tool_program_child_snapshot"));
        assert!(!is_tool_program_event("message_update"));
        assert!(!is_tool_program_event("tool_execution_update"));
    }
}

/// Minimal read-only Tool Program workbench. It renders only native snapshot
/// metadata; ordinary transcript/tool output is never promoted into children.
#[component]
pub fn ToolProgramWorkbench(
    client: Signal<Option<WorkspaceClient>>,
    session_id: String,
    selected_child: Signal<Option<String>>,
    selected_phase: Signal<Option<String>>,
) -> Element {
    let mut snapshot = use_signal(|| None::<ToolProgramSnapshot>);
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| true);
    let mut children_loading = use_signal(|| false);
    let mut children_error = use_signal(|| None::<String>);
    let mut expanded = use_signal(Vec::<String>::new);
    let mut phase_filter = use_signal(|| "all".to_owned());
    let mut child_filter = use_signal(|| "all".to_owned());
    let mut child_query = use_signal(String::new);
    let mut phases = use_signal(Vec::<ToolProgramPhasePage>::new);
    let mut phase_loading = use_signal(|| None::<String>);
    let mut phase_errors = use_signal(Vec::<(String, String)>::new);
    let mut bodies = use_signal(Vec::<ToolProgramBodyResponse>::new);
    let mut body_loading = use_signal(|| None::<String>);
    let mut body_copy_status = use_signal(|| None::<String>);
    let mut body_errors = use_signal(Vec::<(String, String)>::new);
    let mut live_error = use_signal(|| None::<String>);
    let mut live_connected = use_signal(|| false);
    let mut live_retry_attempt = use_signal(|| 0_u32);
    let mut live_auto_reconnect = use_signal(|| true);
    let mut load_task = use_signal(|| None::<dioxus::core::Task>);
    let mut live_task = use_signal(|| None::<dioxus::core::Task>);
    let mut resync_loading = use_signal(|| false);
    let mut live_epoch = use_signal(|| 0_u64);
    let mut last_synced_at = use_signal(|| None::<String>);
    use_effect(use_reactive(
        (&session_id, &live_epoch),
        move |(session, _)| {
            if let Some(task) = load_task.write().take() {
                task.cancel();
            }
            if let Some(task) = live_task.write().take() {
                task.cancel();
            }
            snapshot.set(None);
            phases.set(Vec::new());
            bodies.set(Vec::new());
            phase_errors.set(Vec::new());
            body_errors.set(Vec::new());
            body_copy_status.set(None);
            loading.set(true);
            children_loading.set(false);
            children_error.set(None);
            phase_filter.set("all".into());
            child_filter.set("all".into());
            child_query.set(String::new());
            error.set(None);
            live_error.set(None);
            live_connected.set(false);
            live_retry_attempt.set(0);
            live_auto_reconnect.set(true);
            last_synced_at.set(None);
            let Some(client) = client.peek().clone() else {
                loading.set(false);
                return;
            };
            let session = session.clone();
            load_task.set(Some(spawn(async move {
                match client.tool_program_snapshot(&session).await {
                    Ok(value) => {
                        let after = value.last_seq;
                        snapshot.set(Some(value));
                        last_synced_at.set(sync_timestamp().await);
                        loading.set(false);
                        let live_client = client.clone();
                        let live_session = session.clone();
                        let stream_task = spawn(async move {
                            let mut after = after;
                            let mut attempt = 0_u32;
                            let mut retry_started_at = None::<Instant>;
                            loop {
                                if !page_is_visible().await {
                                    live_connected.set(false);
                                    live_error.set(Some("页面不可见，自动重连已暂停".into()));
                                    realtime_retry_delay(1).await;
                                    continue;
                                }
                                if !*live_auto_reconnect.read() {
                                    live_connected.set(false);
                                    live_error.set(Some("自动重连已暂停".into()));
                                    return;
                                }
                                match live_client.subscribe(&live_session, after).await {
                                    Ok(mut stream) => {
                                        retry_started_at = None;
                                        live_connected.set(true);
                                        live_retry_attempt.set(0);
                                        live_error.set(None);
                                        while let Some(result) = stream.next().await {
                                            let Ok(event) = result else {
                                                live_connected.set(false);
                                                retry_started_at.get_or_insert_with(Instant::now);
                                                live_error.set(Some(
                                                    "实时 WebSocket 连接中断，准备重连".into(),
                                                ));
                                                break;
                                            };
                                            if !is_tool_program_event(&event.event_type) {
                                                continue;
                                            }
                                            match live_client
                                                .tool_program_snapshot(&live_session)
                                                .await
                                            {
                                                Ok(value) => {
                                                    after = value.last_seq;
                                                    snapshot.set(Some(value));
                                                    last_synced_at.set(sync_timestamp().await);
                                                    phases.set(Vec::new());
                                                    bodies.set(Vec::new());
                                                    live_error.set(None);
                                                }
                                                Err(value) => {
                                                    live_error.set(Some(value.to_string()))
                                                }
                                            }
                                        }
                                        live_connected.set(false);
                                        retry_started_at.get_or_insert_with(Instant::now);
                                        live_error
                                            .set(Some("实时 WebSocket 已断开，准备重连".into()));
                                    }
                                    Err(_) => {
                                        retry_started_at.get_or_insert_with(Instant::now);
                                        live_connected.set(false);
                                        live_error
                                            .set(Some("实时 WebSocket 未连接，准备重连".into()));
                                    }
                                }
                                if retry_started_at.is_some_and(|started| {
                                    started.elapsed().as_millis() >= MAX_REALTIME_RETRY_WINDOW_MS
                                }) {
                                    live_auto_reconnect.set(false);
                                    live_error.set(Some("自动重连总时长已达上限".into()));
                                    return;
                                }
                                let delay_attempt = attempt;
                                attempt = attempt.saturating_add(1);
                                if attempt > MAX_REALTIME_RETRIES {
                                    live_auto_reconnect.set(false);
                                    live_retry_attempt.set(attempt);
                                    live_error.set(Some("自动重连次数已达上限".into()));
                                    return;
                                }
                                live_retry_attempt.set(attempt);
                                realtime_retry_delay(delay_attempt).await;
                            }
                        });
                        live_task.set(Some(stream_task));
                    }
                    Err(value) => {
                        error.set(Some(value.to_string()));
                        loading.set(false);
                    }
                }
            })));
        },
    ));
    use_effect(use_reactive((&selected_child,), move |(selected,)| {
        if let Some(child_id) = selected.as_ref() {
            if !expanded.peek().iter().any(|id| id == child_id.as_str()) {
                expanded.write().push(child_id.clone());
            }
        }
    }));
    use_drop(move || {
        if let Some(task) = load_task.write().take() {
            task.cancel();
        }
        if let Some(task) = live_task.write().take() {
            task.cancel();
        }
    });

    let current = snapshot.read();
    let Some(current) = current.as_ref() else {
        return if *loading.read() {
            rsx! { div { class: "tool-program-workbench tool-program-loading", role: "status", "正在读取 Tool Program…" } }
        } else if let Some(message) = error.read().as_ref() {
            // Unsupported is intentionally quiet; older nodes should not show a
            // broken workbench merely because they predate Tool Program support.
            if message.contains("unsupported") || message.contains("Unsupported") {
                rsx! {}
            } else {
                rsx! { div { class: "tool-program-workbench tool-program-error", role: "alert", "Tool Program 读取失败：{message}" } }
            }
        } else {
            rsx! {}
        };
    };
    if current.page.children.is_empty() {
        return rsx! {};
    }
    let resync_session = session_id.clone();
    let child_page_session = session_id.clone();
    let retry_attempt = *live_retry_attempt.read();
    // Precompute once per render: filtering runs per child, so avoid
    // re-allocating a lowercased query and re-reading the filter each time.
    let child_query_lower = child_query.read().trim().to_ascii_lowercase();
    let child_filter_value = child_filter.read().clone();
    let phase_filter_value = phase_filter.read().clone();

    rsx! {
        section { id: "tool-program-workbench", class: "tool-program-workbench", aria_label: "Tool Program 工作台",
            header { class: "tool-program-workbench-header",
                strong { "Tool Program" }
                span { class: "tool-program-workbench-meta",
                    "{current.page.children.len()} 个子任务 · lane {current.lane} · seq {current.start_seq}–{current.last_seq}"
                    if let Some(timestamp) = last_synced_at.read().as_ref() {
                        span { class: "tool-program-status", "最后同步 {timestamp}" }
                    }
                }
                if !expanded.read().is_empty() {
                    button { class: "tool-program-body-button", r#type: "button", onclick: move |_| expanded.set(Vec::new()), "全部收起" }
                }
                label { class: "tool-program-filter", "工具名"
                    input { value: "{child_query}", placeholder: "筛选 tool", oninput: move |event| child_query.set(event.value()) }
                }
                label { class: "tool-program-filter", "Child 筛选"
                    select { value: "{child_filter}", onchange: move |event| child_filter.set(event.value()),
                        option { value: "all", "全部" }
                        option { value: "active", "运行中" }
                        option { value: "error", "失败" }
                        option { value: "done", "已完成" }
                    }
                }
                label { class: "tool-program-filter", "Phase 筛选"
                    select { value: "{phase_filter}", onchange: move |event| phase_filter.set(event.value()),
                        option { value: "all", "全部" }
                        option { value: "attention", "异常/中断" }
                        option { value: "body", "包含 body" }
                    }
                }
                if let Some(cursor) = current.page.next_cursor.clone() {
                    button {
                        class: "tool-program-body-button", r#type: "button", disabled: *children_loading.read(),
                        onclick: move |_| {
                            children_loading.set(true);
                            children_error.set(None);
                            let request_client = client.peek().clone();
                            let session = child_page_session.clone();
                            let cursor = cursor.clone();
                            spawn(async move {
                                if let Some(request_client) = request_client {
                                    let query = ToolProgramChildrenQuery { direction: "newestFirst".into(), limit: 8, max_bytes: 32768, cursor: Some(cursor), ..Default::default() };
                                    match request_client.tool_program_children(&session, query).await {
                                        Ok(page) => { if let Some(current) = snapshot.write().as_mut() { current.page.children.extend(page.children); current.page.next_cursor = page.next_cursor; } },
                                        Err(value) => children_error.set(Some(value.to_string())),
                                    }
                                }
                                children_loading.set(false);
                            });
                        },
                        if *children_loading.read() { "读取中…" } else { "加载更多子任务" }
                    }
                }
                if let Some(message) = children_error.read().as_ref() {
                    p { class: "tool-program-error", role: "alert", "子任务分页失败：{message}" }
                }
                if current.resync_required {
                    span { class: "tool-program-status is-warning", role: "status", "需要重新同步" }
                    button {
                        class: "tool-program-body-button", r#type: "button",
                        disabled: *resync_loading.read(),
                        onclick: move |_| {
                            let Some(request_client) = client.peek().clone() else { return; };
                            let session = resync_session.clone();
                            resync_loading.set(true);
                            spawn(async move {
                                match request_client.tool_program_snapshot(&session).await {
                                    Ok(value) => {
                                        snapshot.set(Some(value));
                                        last_synced_at.set(sync_timestamp().await);
                                        phases.set(Vec::new());
                                        bodies.set(Vec::new());
                                    },
                                    Err(value) => error.set(Some(value.to_string())),
                                }
                                resync_loading.set(false);
                            });
                        },
                        if *resync_loading.read() { "同步中…" } else { "重新同步" }
                    }
                }
                span { class: "tool-program-status", "通道：WorkspaceClient realtime subscribe" }
                if let Some(message) = live_error.read().as_ref() {
                    span { class: "tool-program-status is-warning", role: "status", "{message}" }
                    if retry_attempt > 0 { span { class: "tool-program-status", "第 {retry_attempt} 次重连" } }
                    if retry_attempt > 0 {
                        button { class: "tool-program-body-button", r#type: "button", onclick: move |_| { live_auto_reconnect.set(false); if let Some(task) = live_task.write().take() { task.cancel(); } }, "停止重连" }
                    }
                    button { class: "tool-program-body-button", r#type: "button", onclick: move |_| { live_auto_reconnect.set(true); live_epoch += 1; }, if retry_attempt > 0 { "重新连接" } else { "重试实时连接" } }
                } else if *live_connected.read() {
                    span { class: "tool-program-status is-success", role: "status", "实时已连接" }
                } else {
                    span { class: "tool-program-status", role: "status", "实时连接中…" }
                }
            }
            div { class: "tool-program-children",
                for child in current.page.children.iter().filter(|child| {
                    let query_matches = child_query_lower.is_empty() || child.tool_name.to_ascii_lowercase().contains(&child_query_lower);
                    query_matches && match child_filter_value.as_str() { "active" => matches!(child.state.as_str(), "running" | "active" | "started"), "error" => matches!(child.state.as_str(), "failed" | "error"), "done" => matches!(child.state.as_str(), "completed" | "closed" | "done"), _ => true }
                }) {
                    {
                        let child_id = child.child_id.clone();
                        let observation = current.observations.iter().find(|item| item.child_id == child_id);
                        let phase = observation.and_then(|item| item.latest_phase.as_deref()).unwrap_or("unknown");
                        let outcome = observation.map(|item| item.outcome.as_str()).unwrap_or("unknown");
                        let child_state_class = match child.state.as_str() { "running" | "active" | "started" => "is-running", "failed" | "error" => "is-error", "cancelled" | "canceled" => "is-cancelled", "completed" | "closed" | "done" => "is-success", _ => "" };
                        let is_expanded = expanded.read().iter().any(|id| id == &child_id);
                        let page_phases: Vec<ToolProgramPhase> = phases.read().iter().find(|item| item.child_id == child_id).map(|item| item.phases.iter().filter(|item| match phase_filter_value.as_str() { "attention" => item.is_error == Some(true) || item.failed == Some(true) || item.interrupted == Some(true) || item.cancelled == Some(true), "body" => item.descriptor.is_some(), _ => true }).cloned().collect()).unwrap_or_default();
                        let is_loading = phase_loading.read().as_deref() == Some(child_id.as_str());
                        let has_phases = !page_phases.is_empty();
                        let phase_cursor = phases.read().iter().find(|item| item.child_id == child_id).and_then(|item| item.next_cursor.clone());
                        let client_for_click = client;
                        let session_for_click = session_id.clone();
                        let child_for_click = child_id.clone();
                        let phase_more_child = child_id.clone();
                        let phase_more_session = session_id.clone();
                        let phase_more_cursor = phase_cursor.clone();
                        let phases_id = format!("tool-program-phases-{child_id}");
                        rsx! {
                            article { class: if selected_child.read().as_deref() == Some(child_id.as_str()) { "tool-program-child-card is-selected" } else { "tool-program-child-card" }, key: "{child_id}",
                                button { class: "tool-program-child-toggle", r#type: "button", aria_expanded: is_expanded, aria_controls: phases_id.clone(),
                                    onclick: move |_| {
                                        selected_child.set(Some(child_for_click.clone()));
                                        let already = expanded.peek().iter().any(|id| id == &child_for_click);
                                        if already { expanded.write().retain(|id| id != &child_for_click); return; }
                                        expanded.write().push(child_for_click.clone());
                                        if phases.peek().iter().any(|item| item.child_id == child_for_click) { return; }
                                        phase_loading.set(Some(child_for_click.clone())); phase_errors.write().retain(|(id, _)| id != &child_for_click);
                                        let request_client = client_for_click.peek().clone();
                                        let session = session_for_click.clone(); let child = child_for_click.clone();
                                        spawn(async move {
                                            let Some(request_client) = request_client else { return; };
                                            let query = ToolProgramPhasesQuery { child_id: child.clone(), children: ToolProgramChildrenQuery { direction: "newestFirst".into(), limit: 8, max_bytes: 32768, ..Default::default() } };
                                            match request_client.tool_program_phases(&session, query).await { Ok(page) => phases.write().push(page), Err(error) => phase_errors.write().push((child.clone(), error.to_string())) }
                                            phase_loading.set(None);
                                        });
                                    },
                                    span { class: "tool-program-child-heading", strong { "{child.tool_name}" } span { class: "tool-program-status", "{phase}" } }
                                    span { class: "tool-program-child-details",
                                        span { "子任务 {child.ordinal}" } span { class: "tool-program-status {child_state_class}", "状态：{child.state}" } span { "结果：{outcome}" }
                                        span { class: "tool-program-status", "Trace：未提供关联" }
                                        if let Some(parent) = &child.parent_child_id { span { "父任务：{parent}" } }
                                        span { class: "tool-program-expand-hint", if is_expanded { "收起" } else { "查看 phases" } }
                                    }
                                }
                                if is_expanded {
                                    div { id: "{phases_id}", class: "tool-program-phases",
                                        if is_loading { p { class: "tool-program-loading", "正在读取 phases…" } }
                                        if let Some(error) = phase_errors.read().iter().find(|(id, _)| id == &child_id).map(|(_, message)| message.clone()) { p { class: "tool-program-error", role: "alert", "{error}" } }
                                        for item in page_phases { {
                                            let phase_id = item.entry_id.clone();
                                            let descriptor = item.descriptor.clone();
                                            let descriptor_id = descriptor.as_ref().map(|value| value.descriptor_id.clone());
                                            let body = bodies.read().iter().find(|value| value.phase_entry_id == phase_id).cloned();
                                            let body_loading_now = body_loading.read().as_deref() == Some(phase_id.as_str());
                                            let phase_body_error = body_errors.read().iter().find(|(id, _)| id == &phase_id).map(|(_, message)| message.clone());
                                            let body_client = client;
                                            let body_session = session_id.clone();
                                            let body_child = child_id.clone();
                                            let phase_select_id = phase_id.clone();
                                            let body_phase_id = phase_id.clone();
                                            let phase_state = if item.cancelled == Some(true) { "is-cancelled" } else if item.failed == Some(true) || item.is_error == Some(true) { "is-error" } else if item.interrupted == Some(true) { "is-interrupted" } else { "" };
                                            rsx! {
                                                div { class: if selected_phase.read().as_deref() == Some(phase_select_id.as_str()) { "tool-program-phase is-selected {phase_state}" } else { "tool-program-phase {phase_state}" }, key: "{phase_id}",
                                                    onclick: move |_| selected_phase.set(Some(phase_select_id.clone())),
                                                    span { class: "tool-program-phase-kind", "{item.phase}" }
                                                    span { "seq {item.seq}" }
                                                    if item.is_error == Some(true) { span { class: "tool-program-status is-warning", "错误" } }
                                                    if item.cancelled == Some(true) { span { class: "tool-program-status", "已取消" } }
                                                    if item.failed == Some(true) { span { class: "tool-program-status is-warning", "失败" } }
                                                    if item.interrupted == Some(true) { span { class: "tool-program-status is-warning", "中断" } }
                                                    if descriptor.is_some() && (body.is_none() || body.as_ref().is_some_and(|value| value.next_cursor.is_some())) {
                                                        button { class: "tool-program-body-button", r#type: "button",
                                                            onclick: move |_| {
                                                                let existing = bodies.peek().iter().find(|value| value.phase_entry_id == body_phase_id).cloned();
                                                                let cursor = existing.as_ref().and_then(|value| value.next_cursor.clone());
                                                                if existing.is_some() && cursor.is_none() { return; }
                                                                body_loading.set(Some(body_phase_id.clone())); body_errors.write().retain(|(id, _)| id != &body_phase_id);
                                                                let request_client = body_client.peek().clone(); let session = body_session.clone(); let child = body_child.clone(); let entry = body_phase_id.clone(); let body_descriptor_id = descriptor_id.clone();
                                                                spawn(async move {
                                                                    let Some(request_client) = request_client else { return; };
                                                                    let query = ToolProgramBodyQuery { child_id: child, phase_entry_id: entry.clone(), descriptor_id: body_descriptor_id.unwrap_or_default(), leaf_id: None, cursor, limit: 8, max_bytes: 65536 };
                                                                    match request_client.tool_program_body(&session, query).await { Ok(value) => { let mut all = bodies.write(); if let Some(old) = all.iter_mut().find(|old| old.phase_entry_id == value.phase_entry_id) { old.body.chunks.extend(value.body.chunks); old.body.byte_length += value.body.byte_length; old.next_cursor = value.next_cursor; } else { all.push(value); } }, Err(error) => body_errors.write().push((entry.clone(), error.to_string())) }
                                                                    body_loading.set(None);
                                                                });
                                                            },
                                                            if body_loading_now { "读取中…" } else if body.is_some() { "加载更多" } else { "查看 body" }
                                                        }
                                                    }
                                                }
                                                if let Some(body) = body {
                                                    {
                                                    let body_bytes = body.body.chunks.iter().try_fold(Vec::new(), |mut bytes, chunk| {
                                                        bytes.extend(body_chunk_bytes(&chunk.data)?);
                                                        Some(bytes)
                                                    });
                                                    let body_complete = body_is_complete(body.next_cursor.is_some(), body.body.byte_length, body.body.total_bytes);
                                                    let view_kind = body_view_kind(&body.body.descriptor.kind);
                                                    let body_text = body_complete.then(|| body_bytes.as_ref()).flatten().and_then(|bytes| String::from_utf8(bytes.clone()).ok()).map(|text| body_display_text(&body.body.descriptor.kind, text));
                                                    let download_data = body_bytes.as_ref().map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes));
                                                    let body_hash = body_bytes.as_ref().map(|bytes| body_sha256(bytes));
                                                    let download_name = format!("tool-program-{}.body", body.body.descriptor.body_id);
                                                    let copy_phase_id = phase_id.clone();
                                                    let copy_status_id = copy_phase_id.clone();
                                                    rsx! { div { class: "tool-program-body",
                                                        span { "{body.body.total_bytes} bytes · {body.body.chunks.len()} chunks · sha256 {body.body.descriptor.sha256}" }
                                                        span { class: "tool-program-body-meta", "查看器：{view_kind} · {body.body.descriptor.kind} · {body.body.descriptor.codec} · {body.body.descriptor.encoding} · 已加载 {body.body.byte_length} bytes" }
                                                        if let Some(data) = download_data {
                                                            {
                                                                let name = download_name.clone();
                                                                rsx! { button { class: "tool-program-body-button", r#type: "button", onclick: move |_| {
                                                                let data = data.clone();
                                                                let file_name = name.clone();
                                                                spawn(async move {
                                                                    let name = serde_json::to_string(&file_name).unwrap_or_else(|_| "\"body.bin\"".into());
                                                                    let script = format!("try {{ const raw=atob({data:?}); const bytes=Uint8Array.from(raw,c=>c.charCodeAt(0)); const a=document.createElement('a'); a.href=URL.createObjectURL(new Blob([bytes])); a.download={name}; a.click(); setTimeout(()=>URL.revokeObjectURL(a.href),1000); dioxus.send(true); }} catch (_) {{ dioxus.send(false); }}");
                                                                    let _ = document::eval(&script).recv::<bool>().await;
                                                                });
                                                                }, "下载已加载 body" } }
                                                            }
                                                        }
                                                        if let Some(text) = body_text.clone() {
                                                            button { class: "tool-program-body-button", r#type: "button", onclick: move |_| {
                                                                let payload = serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".into());
                                                                let success_id = copy_phase_id.clone();
                                                                let failure_id = copy_phase_id.clone();
                                                                spawn(async move {
                                                                    let script = format!("try {{ await navigator.clipboard.writeText({payload}); dioxus.send(true); }} catch (_) {{ dioxus.send(false); }}");
                                                                    let ok = document::eval(&script).recv::<bool>().await.unwrap_or(false);
                                                                    body_copy_status.set(Some(if ok { success_id } else { format!("error:{failure_id}") }));
                                                                });
                                                            }, if body_copy_status.read().as_deref() == Some(copy_status_id.as_str()) { "已复制" } else if body_copy_status.read().as_deref() == Some(format!("error:{copy_status_id}").as_str()) { "复制失败" } else { "复制完整文本" } }
                                                        }
                                                        if view_kind == "JSON" || view_kind == "Text" {
                                                            if let Some(text) = body_text.as_ref() {
                                                                pre { class: "tool-program-body-chunk", role: "region", aria_label: "Text body preview", "{text}" }
                                                            }
                                                        }
                                                        if view_kind == "Markdown" {
                                                            if let Some(text) = body_text.as_ref() {
                                                                div { class: "tool-program-body-markdown", role: "region", aria_label: "Markdown body preview", {super::timeline::markdown_view(text)} }
                                                            }
                                                        }
                                                        if view_kind == "Code" {
                                                            if let Some(text) = body_text.as_ref() {
                                                                div { class: "tool-program-body-code", role: "region", aria_label: "Code body preview", super::message_presentation::CodeBlock { source: text.clone(), language: body_code_language(&body.body.descriptor.kind, &body.body.descriptor.codec).to_owned() } }
                                                            }
                                                        }
                                                        if body.next_cursor.is_some() {
                                                            span { class: "tool-program-status is-warning", "内容未完整加载" }
                                                        } else if body.body.byte_length != body.body.total_bytes {
                                                            span { class: "tool-program-status is-warning", "返回内容长度与 descriptor 不一致" }
                                                        } else if body_hash.as_deref().is_some_and(|hash| hash.eq_ignore_ascii_case(&body.body.descriptor.sha256)) {
                                                            span { class: "tool-program-status is-success", "内容完整 · SHA-256 通过" }
                                                        } else {
                                                            span { class: "tool-program-status is-warning", "整体 SHA-256 不匹配" }
                                                        }
                                                        details { class: "tool-program-body-chunks", open: true,
                                                            summary { "查看 {body.body.chunks.len()} 个 chunks" }
                                                            for chunk in &body.body.chunks {
                                                                {
                                                                let chunk_hash = body_chunk_sha256(&chunk.data);
                                                                let chunk_class = if view_kind == "Code" { "tool-program-body-chunk is-code" } else { "tool-program-body-chunk" };
                                                                let chunk_valid = chunk_hash.as_deref().is_some_and(|hash| hash.eq_ignore_ascii_case(&chunk.sha256));
                                                                rsx! {
                                                                    span { class: "tool-program-body-meta", "chunk {chunk.index} · offset {chunk.byte_offset} · {chunk.byte_length} bytes" }
                                                                    if let Some(bytes) = body_chunk_bytes(&chunk.data) {
                                                                        if let Ok(text) = String::from_utf8(bytes.clone()) {
                                                                            pre { class: "{chunk_class}", key: "{chunk.index}", "{text}" }
                                                                        } else {
                                                                            pre { class: "tool-program-body-chunk is-binary", key: "{chunk.index}", "二进制 chunk · {bytes.len()} bytes" }
                                                                        }
                                                                    } else {
                                                                        pre { class: "tool-program-body-chunk is-binary", key: "{chunk.index}", "无法解码 chunk" }
                                                                    }
                                                                    if chunk_valid { span { class: "tool-program-status is-success", "chunk {chunk.index} 校验通过" } } else { span { class: "tool-program-status is-warning", "chunk {chunk.index} 校验失败" } }
                                                                }
                                                                }
                                                            }
                                                        }
                                                    } }
                                                    }
                                                }
                                                if let Some(error) = &phase_body_error { p { class: "tool-program-error", role: "alert", "{error}" } }
                                            }
                                        } }
                                        if phase_cursor.is_some() {
                                            button {
                                                class: "tool-program-body-button tool-program-phase-more", r#type: "button",
                                                disabled: is_loading,
                                                onclick: move |_| {
                                                    phase_loading.set(Some(phase_more_child.clone()));
                                                    phase_errors.write().retain(|(id, _)| id != &phase_more_child);
                                                    let request_client = client_for_click.peek().clone();
                                                    let session = phase_more_session.clone();
                                                    let child = phase_more_child.clone();
                                                    let cursor = phase_more_cursor.clone().expect("phase cursor checked above");
                                                    spawn(async move {
                                                        if let Some(request_client) = request_client {
                                                            let query = ToolProgramPhasesQuery { child_id: child.clone(), children: ToolProgramChildrenQuery { direction: "newestFirst".into(), limit: 8, max_bytes: 32768, cursor: Some(cursor), ..Default::default() } };
                                                            match request_client.tool_program_phases(&session, query).await {
                                                                Ok(page) => { let mut all = phases.write(); if let Some(old) = all.iter_mut().find(|old| old.child_id == page.child_id) { old.phases.extend(page.phases); old.next_cursor = page.next_cursor; } else { all.push(page); } }
                                                                Err(error) => phase_errors.write().push((child.clone(), error.to_string())),
                                                            }
                                                        }
                                                        phase_loading.set(None);
                                                    });
                                                },
                                                if is_loading { "读取中…" } else { "加载更多 phases" }
                                            }
                                        }
                                        if !is_loading && !has_phases && phase_errors.read().iter().all(|(id, _)| id != &child_id) { p { class: "tool-program-loading", "暂无 phase 记录" } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
