//! A disposable index of public user inputs in the currently loaded view.
//! The host owns history loading, session generations and `ScrollFollow` navigation.
use super::native_timeline::{Command, HistoricalReader, Reading, Summaries};
use dioxus::prelude::*;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use work_client::{ClientError, MessageSummaryQuery, MessageWindowQuery, WorkspaceClient};

pub const CSS: &str = include_str!("user_timeline.css");
const INSTALLER: &str = include_str!("user_timeline.js");
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

/// A DOM identity, never a backend pagination cursor. Precedence matches history rendering.
#[must_use]
pub fn target_key(message: &Value, index: usize) -> Option<String> {
    if message
        .get("role")
        .or_else(|| message.get("kind"))
        .and_then(Value::as_str)
        != Some("user")
    {
        return None;
    }
    for field in ["entryId", "messageId", "id"] {
        if let Some(id) = message
            .get(field)
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
        {
            return Some(format!("entry:{id}"));
        }
    }
    Some(format!("loaded:{index}"))
}

#[derive(Debug, PartialEq)]
struct Entry {
    key: String,
    preview: String,
    loaded_index: usize,
}

fn block_text(block: &Value) -> Option<&str> {
    match block.get("type") {
        Some(Value::String(kind))
            if matches!(kind.as_str(), "text" | "input_text" | "output_text") =>
        {
            block.get("text").and_then(Value::as_str)
        }
        None if block.get("thinking").is_none() => block.get("text").and_then(Value::as_str),
        _ => None,
    }
}

fn preview(message: &Value) -> Option<String> {
    let mut text = String::new();
    let mut image = false;
    match message.get("content") {
        Some(Value::String(value)) => text.push_str(value),
        Some(Value::Array(blocks)) => {
            for block in blocks {
                // Match message_image::image_block's explicit kind, including error cards.
                // Never inspect a source, URL, encoded data or an arbitrary attachment object.
                if block.get("type").and_then(Value::as_str) == Some("image") {
                    image = true;
                } else if let Some(value) = block_text(block) {
                    text.push_str(value);
                }
            }
        }
        None => text.push_str(
            message
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        _ => {}
    }
    // Normalize before truncating by Unicode scalar values, not UTF-8 bytes. Strip
    // direction overrides/isolates and invisible controls from accessible labels too.
    let mut result = String::new();
    let mut space = false;
    let mut count = 0;
    for ch in text.chars() {
        if matches!(ch, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2069}' | '\u{feff}')
        {
            continue;
        }
        if ch.is_whitespace() || ch.is_control() {
            space = !result.is_empty();
            continue;
        }
        if space && count < 140 {
            result.push(' ');
            count += 1;
        }
        if count == 140 {
            break;
        }
        result.push(ch);
        count += 1;
        space = false;
    }
    let result = result.trim_end().to_owned();
    if !result.is_empty() {
        Some(result)
    } else if image {
        Some("图片消息".to_owned())
    } else {
        None
    }
}

fn entries(messages: &[Value]) -> Vec<Entry> {
    let mut seen = HashSet::new();
    messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            let key = target_key(message, index)?;
            let preview = preview(message)?;
            if !seen.insert(key.clone()) {
                return None;
            }
            Some(Entry {
                key,
                preview,
                loaded_index: index + 1,
            })
        })
        .collect()
}

/// Use the same visible, deduplicated inputs as the rail, not raw message count:
/// an initial 48-message suffix can consist almost entirely of tool traffic.
pub(super) fn needs_initial_history(messages: &[Value]) -> bool {
    entries(messages).len() < 10
}

fn timeline_entries(messages: &[Value], summaries: &Summaries) -> Vec<Entry> {
    let loaded = entries(messages);
    let Some(page) = summaries.page.as_ref().filter(|_| !summaries.legacy) else {
        return loaded;
    };
    let mut result: Vec<_> = page
        .messages
        .iter()
        .enumerate()
        .map(|(index, message)| Entry {
            key: format!("entry:{}", message.entry_id),
            preview: if message.preview.is_empty() {
                "图片消息".into()
            } else {
                // Reuse the public Unicode/control normalization for host metadata.
                preview(&serde_json::json!({"content":message.preview})).unwrap_or_default()
            },
            loaded_index: index + 1,
        })
        .collect();
    let mut before: Option<String> = None;
    for entry in loaded.into_iter().rev() {
        let key = entry.key.clone();
        if !result.iter().any(|item| item.key == key) {
            let index = before
                .as_ref()
                .and_then(|key| result.iter().position(|item| &item.key == key))
                .unwrap_or(result.len());
            result.insert(index, entry);
        }
        before = Some(key);
    }
    result
}

#[component]
pub fn UserTimeline(
    scope: String,
    transcript: Signal<Vec<Value>>,
    history: Signal<super::history_window::HistoryWindow>,
    client: Signal<Option<WorkspaceClient>>,
    session_id: String,
    #[props(default)] native_queries: bool,
) -> Element {
    let instance = use_hook(|| NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed));
    let id = format!("rpi-user-timeline-{instance}");
    let mut mounted = use_signal(|| false);
    let mut summaries = use_signal(Summaries::default);
    let mut reading = use_signal(Reading::default);
    let mut epoch = use_signal(|| 0_u64);
    let mut window_epoch = use_signal(|| 0_u64);
    let mut summary_task = use_signal(|| None::<dioxus::core::Task>);
    let mut window_task = use_signal(|| None::<dioxus::core::Task>);
    let mut bridge_task = use_signal(|| None::<dioxus::core::Task>);
    let request_session = session_id.clone();
    let load_summary = use_callback(move |earlier: bool| {
        if !native_queries || summaries.peek().busy {
            return;
        }
        let Some(client) = client.peek().clone() else {
            return;
        };
        let previous = summaries.peek().page.clone();
        if earlier
            && !previous
                .as_ref()
                .is_some_and(|page| page.has_more && page.next_before_id.is_some())
        {
            return;
        }
        let query = MessageSummaryQuery {
            before_id: if earlier {
                previous
                    .as_ref()
                    .and_then(|page| page.next_before_id.clone())
            } else {
                None
            },
            leaf_id: previous
                .as_ref()
                .and_then(|page| page.leaf_id.clone())
                .or_else(|| history.peek().leaf_id.clone()),
            ..MessageSummaryQuery::default()
        };
        let owner = *epoch.peek();
        summaries.write().busy = true;
        summaries.write().error = None;
        let session = request_session.clone();
        summary_task.set(Some(spawn(async move {
            let result = client.message_summaries(&session, &query).await;
            if *epoch.peek() != owner {
                return;
            }
            let mut state = summaries.write();
            state.busy = false;
            match result {
                Ok(mut page) => {
                    if earlier {
                        if let Some(old) = previous {
                            if page.has_more && page.next_before_id == old.next_before_id {
                                state.error = Some("摘要游标没有前进，请重试。".into());
                                return;
                            }
                            let seen: HashSet<_> = page
                                .messages
                                .iter()
                                .map(|message| message.entry_id.clone())
                                .collect();
                            page.messages.extend(
                                old.messages
                                    .into_iter()
                                    .filter(|message| !seen.contains(&message.entry_id)),
                            );
                            page.last_id = old.last_id;
                        }
                    }
                    state.page = Some(page);
                }
                Err(ClientError::Unsupported { .. }) if !earlier => {
                    state.legacy = true;
                }
                Err(error) => state.error = Some(format!("读取时间线摘要失败：{error}")),
            }
        })));
    });
    let request_session = session_id.clone();
    let load_window = use_callback(move |query: MessageWindowQuery| {
        let Some(client) = client.peek().clone() else {
            return;
        };
        if let Some(task) = window_task.write().take() {
            task.cancel();
        }
        window_epoch += 1;
        let ticket = *window_epoch.peek();
        let owner = *epoch.peek();
        {
            let mut state = reading.write();
            state.query = Some(query.clone());
            state.busy = true;
            state.error = None;
        }
        let session = request_session.clone();
        window_task.set(Some(spawn(async move {
            let result = client.message_window(&session, &query).await;
            if *epoch.peek() != owner || *window_epoch.peek() != ticket {
                return;
            }
            let mut state = reading.write();
            state.busy = false;
            match result {
                Ok(page) => state.page = Some(page),
                Err(error) => state.error = Some(format!("读取历史消息失败：{error}")),
            }
        })));
    });
    let close_reader = use_callback(move |()| {
        if let Some(task) = window_task.write().take() {
            task.cancel();
        }
        window_epoch += 1;
        reading.set(Reading::default());
    });
    let revision = history.read().revision;
    let leaf = history.read().leaf_id.clone();
    let snapshot_ready = history.read().snapshot_ready;
    use_effect(use_reactive(
        (&scope, &native_queries, &revision, &leaf, &snapshot_ready),
        move |(_, supported, _, _, ready)| {
            if let Some(task) = summary_task.write().take() {
                task.cancel();
            }
            close_reader.call(());
            epoch += 1;
            summaries.set(Summaries::default());
            if supported && ready {
                load_summary.call(false);
            }
        },
    ));
    let command = use_callback(move |command: Command| {
        if command.action == "load" {
            if summaries.peek().error.is_none() {
                load_summary.call(true);
            }
        } else if command.action == "navigate" {
            if let Some(leaf) = summaries
                .peek()
                .page
                .as_ref()
                .and_then(|page| page.leaf_id.clone())
            {
                if summaries.peek().page.as_ref().is_some_and(|page| {
                    page.messages
                        .iter()
                        .any(|item| item.entry_id == command.target)
                }) {
                    load_window.call(MessageWindowQuery {
                        anchor_id: Some(command.target),
                        leaf_id: Some(leaf),
                        ..MessageWindowQuery::default()
                    });
                }
            }
        } else if command.action == "live" {
            close_reader.call(());
        }
    });
    let ready = *mounted.read();
    let bridge_id = id.clone();
    use_effect(use_reactive((&scope, &ready), move |(_, ready)| {
        if let Some(task) = bridge_task.write().take() {
            task.cancel();
        }
        if !ready {
            return;
        }
        let encoded = serde_json::to_string(&bridge_id).unwrap();
        bridge_task.set(Some(spawn(async move {
            let mut eval = document::eval(&format!("const root=document.getElementById({encoded});if(root){{root.__rpiNativeTimelineDispose?.();const handler=e=>dioxus.send(e.detail);root.addEventListener('rpi-timeline-command',handler);root.__rpiNativeTimelineDispose=()=>root.removeEventListener('rpi-timeline-command',handler);}}"));
            while let Ok(value) = eval.recv::<Command>().await { command.call(value); }
        })));
    }));
    use_drop(move || {
        for mut task in [summary_task, window_task, bridge_task] {
            if let Some(task) = task.write().take() {
                task.cancel();
            }
        }
    });
    let native = native_queries && !summaries.read().legacy;
    let summary = summaries.read().clone();
    let loaded = timeline_entries(&transcript.read(), &summary);
    // This dependency is view metadata only, not another message/history store.
    let marker = serde_json::to_string(
        &loaded
            .iter()
            .map(|entry| (&entry.key, &entry.preview, entry.loaded_index))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let effect_id = id.clone();
    let is_mounted = *mounted.read();
    use_effect(use_reactive(
        (&scope, &marker, &is_mounted),
        move |(scope, _, ready)| {
            if ready {
                let encoded_id = serde_json::to_string(&effect_id).unwrap();
                let encoded_scope = serde_json::to_string(&scope).unwrap();
                let _ = document::eval(&format!(
                    "const id={encoded_id};const scope={encoded_scope};{INSTALLER}"
                ));
            }
        },
    ));
    let cleanup_id = serde_json::to_string(&id).unwrap();
    use_drop(move || {
        let _ = document::eval(&format!(
            "window.__rpiUserTimelines?.get({cleanup_id})?.dispose();"
        ));
    });
    let state = history.read();
    let has_more = if native {
        summary.page.as_ref().is_some_and(|page| page.has_more)
    } else {
        state.has_more
    };
    let busy = if native { summary.busy } else { state.busy };
    let error = if native {
        summary.error.as_ref()
    } else {
        state.error.as_ref()
    };
    let disabled = if native {
        summary.busy
            || summary
                .page
                .as_ref()
                .is_none_or(|page| page.next_before_id.is_none())
    } else {
        state.busy || !state.enabled || state.cursor.is_none()
    };
    let reason = if busy {
        "正在加载更早消息…"
    } else if !state.enabled {
        "当前节点不支持历史分页。"
    } else if state.cursor.is_none() {
        "缺少可用的历史消息游标。"
    } else if state.error.is_some() {
        "加载失败，可重试加载更早消息。"
    } else {
        "还有更早消息尚未加载。"
    };
    rsx! {
        HistoricalReader { state: reading, onload: move |query| load_window.call(query), onclose: move |_| close_reader.call(()) }
        div {
            id: "{id}", class: "conversation-timeline", "data-session-scope": scope,
            "data-native-timeline": native,
            hidden: loaded.is_empty() && !has_more && !busy && error.is_none(),
            onmounted: move |_| mounted.set(true),
            nav { class: "timeline-panel", aria_label: "用户输入时间线", aria_describedby: "{id}-description",
                p { id: "{id}-description", class: "timeline-sr-only",
                    "时间线默认显示最多十个刻度，输入不足十条时自动补载更早历史；上下滑动浏览更多输入，滑到较早端继续加载历史。点击刻度定位，鼠标悬停或手机按住显示消息预览，拖动刻度切换预览与定位，也支持上下键。仅索引真实已加载消息。"
                    if has_more { " 还有更早消息尚未加载。" }
                }
                if busy { span { class: "timeline-sr-only", role: "status", "正在读取时间线摘要…" } }
                if let Some(error) = error {
                    div { class: "timeline-query-error", role: "alert", "{error}"
                        if native { button { class: "button", disabled: busy, onclick: move |_| load_summary.call(summaries.peek().page.is_some()), "重试时间线" } }
                    }
                }
                if has_more {
                    button { r#type: "button", class: "timeline-load-earlier",
                        aria_label: "在时间线中显示更早的消息", aria_describedby: "{id}-history-status",
                        disabled, aria_busy: busy,
                        "data-auto-load": if error.is_none() { "true" } else { "false" },
                        span { aria_hidden: "true", if busy { "…" } else { "↑" } }
                    }
                    p { id: "{id}-history-status", class: "timeline-sr-only", role: "status", "{reason}" }
                }
                ol { class: "timeline-list",
                    for (position, entry) in loaded.iter().enumerate() {
                        li { key: "{entry.key}",
                            button { r#type: "button", class: "timeline-node",
                                "data-timeline-target": "{entry.key}", "data-preview": "{entry.preview}",
                                aria_label: format!("定位到用户输入 {}：{}", entry.loaded_index, entry.preview),
                                tabindex: if position == 0 { "0" } else { "-1" },
                                span { class: "timeline-mark", aria_hidden: "true" }
                            }
                        }
                    }
                }
            }
            p { class: "timeline-sr-only timeline-navigation-status", aria_live: "polite", role: "status" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_summary_entries_weave_loaded_users_without_duplicate_ids_or_sorting() {
        let page = serde_json::from_value(serde_json::json!({
            "filtered":true,"role":"user","messages":[
                {"entryId":"z","role":"user","preview":"older","timestamp":0,"hasImages":false},
                {"entryId":"a","role":"user","preview":"shared","timestamp":0,"hasImages":false}
            ],"firstId":"z","lastId":"a","nextBeforeId":null,"hasMore":false,"leafId":"leaf"
        }))
        .unwrap();
        let state = Summaries {
            page: Some(page),
            ..Summaries::default()
        };
        let messages = vec![
            json!({"entryId":"mid","role":"user","content":"middle"}),
            json!({"entryId":"a","role":"user","content":"shared"}),
            json!({"entryId":"new","role":"user","content":"live"}),
            json!({"entryId":"assistant","role":"assistant","content":"not a tick"}),
        ];
        let result = timeline_entries(&messages, &state);
        assert_eq!(
            result
                .iter()
                .map(|entry| entry.key.as_str())
                .collect::<Vec<_>>(),
            ["entry:z", "entry:mid", "entry:a", "entry:new"]
        );
        assert_eq!(result[2].preview, "shared");
        let legacy = Summaries {
            legacy: true,
            ..state
        };
        assert_eq!(timeline_entries(&messages, &legacy).len(), 3);
    }

    #[test]
    fn initial_history_fill_counts_visible_inputs_not_tool_messages() {
        let mut messages: Vec<_> = (0..48)
            .map(|i| json!({"entryId":format!("t-{i}"),"role":"assistant","content":[{"type":"toolCall","id":format!("call-{i}"),"name":"read"}]}))
            .collect();
        assert!(needs_initial_history(&messages));
        for i in 0..9 {
            messages.push(json!({"entryId":format!("u-{i}"),"role":"user","content":"input"}));
        }
        assert!(needs_initial_history(&messages));
        messages.push(json!({"entryId":"u-0","role":"user","content":"duplicate"}));
        messages.push(json!({"entryId":"blank","role":"user","content":" \n "}));
        messages.push(json!({"entryId":"private","role":"user","content":[{"type":"thinking","text":"private"}]}));
        assert!(needs_initial_history(&messages));
        messages.push(json!({"entryId":"image","role":"user","content":[{"type":"image"}]}));
        assert!(!needs_initial_history(&messages));
        messages.push(json!({"entryId":"extra","role":"user","content":"extra"}));
        assert!(!needs_initial_history(&messages));
    }

    #[test]
    fn only_user_role_or_kind_with_renderer_precedence() {
        for role in [
            "assistant",
            "tool",
            "thinking",
            "private",
            "system",
            "pending",
            "",
        ] {
            assert_eq!(target_key(&json!({"role":role,"kind":"user"}), 3), None);
        }
        assert_eq!(
            target_key(&json!({"kind":"user"}), 3).as_deref(),
            Some("loaded:3")
        );
        assert_eq!(target_key(&json!({"role":null,"kind":"user"}), 3), None);
    }

    #[test]
    fn ids_use_first_nonempty_string_and_never_become_cursors() {
        assert_eq!(
            target_key(
                &json!({"role":"user","entryId":"a","messageId":"b","id":"c"}),
                8
            )
            .as_deref(),
            Some("entry:a")
        );
        assert_eq!(
            target_key(
                &json!({"role":"user","entryId":" ","messageId":42,"id":"c"}),
                8
            )
            .as_deref(),
            Some("entry:c")
        );
        assert_eq!(
            target_key(&json!({"role":"user","cursor":"backend"}), 8).as_deref(),
            Some("loaded:8")
        );
    }

    #[test]
    fn deduplicates_real_ids_across_id_fields_but_not_local_messages() {
        let messages = vec![
            json!({"role":"user","entryId":"x","content":"old"}),
            json!({"role":"user","messageId":"x","content":"duplicate"}),
            json!({"role":"assistant","content":"skip"}),
            json!({"role":"user","content":"local"}),
            json!({"role":"user","content":"local"}),
        ];
        let result = entries(&messages);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].preview, "old");
        assert_eq!(result[1].key, "loaded:3");
        assert_eq!(result[1].loaded_index, 4);
        assert_eq!(result[2].key, "loaded:4");
    }

    #[test]
    fn unicode_is_truncated_by_characters() {
        let value = preview(&json!({"content": "中🦀".repeat(100)})).unwrap();
        assert_eq!(value.chars().count(), 140);
        assert_eq!(value, "中🦀".repeat(70));
    }

    #[test]
    fn whitespace_controls_and_bidi_do_not_leak_into_labels() {
        assert_eq!(
            preview(&json!({"content":" \n你\t 好\u{0000}世\u{202e}界\u{200b}  "})).as_deref(),
            Some("你 好 世界")
        );
    }

    #[test]
    fn only_public_text_blocks_are_read_without_json_or_attachment_inference() {
        let message = json!({"content":[{"type":"text","text":"a"},{"type":"input_text","text":"b"},{"type":"output_text","text":"c"},{"text":"d"},{"type":"thinking","text":"secret"},{"thinking":"secret","text":"secret"},{"type":"tool_use","text":"secret"},{"type":"attachment","name":"secret","text":"secret"}]});
        assert_eq!(preview(&message).as_deref(), Some("abcd"));
    }

    #[test]
    fn explicit_images_even_old_or_malformed_are_safe_and_text_wins() {
        for image in [
            json!({"type":"image","data":"BASE64_SECRET","mimeType":"image/png"}),
            json!({"type":"image","source":{"url":"https://secret"}}),
            json!({"type":"image"}),
        ] {
            assert_eq!(
                preview(&json!({"content":[image.clone()]})).as_deref(),
                Some("图片消息")
            );
            assert_eq!(
                preview(&json!({"content":[image,{"type":"text","text":"caption"}]})).as_deref(),
                Some("caption")
            );
        }
    }

    #[test]
    fn invisible_invalid_and_all_thinking_content_is_omitted() {
        for content in [
            Value::Null,
            json!(12),
            json!({"type":"image"}),
            json!([]),
            json!(" \n"),
            json!([{"type":"thinking","text":"private"}]),
            json!([{"type":"unknown","text":"secret"}]),
            json!([{"type":null,"text":"secret"}]),
        ] {
            assert_eq!(
                entries(&[json!({"role":"user","content":content,"text":"must not fallback"})]),
                Vec::<Entry>::new()
            );
        }
    }

    #[test]
    fn legacy_text_fallback_only_when_content_is_absent() {
        assert_eq!(
            preview(&json!({"text":"old input"})).as_deref(),
            Some("old input")
        );
        assert_eq!(preview(&json!({"content":"","text":"no"})), None);
        assert_eq!(preview(&json!({"content":null,"text":"no"})), None);
    }
}
