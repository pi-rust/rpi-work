//! Disposable native metadata and one pinned reading window; never a second live transcript.
use dioxus::prelude::*;
use serde::Deserialize;
use work_client::{MessageSummaryPage, MessageWindowPage, MessageWindowQuery};

#[derive(Clone, Default)]
pub(super) struct Summaries {
    pub page: Option<MessageSummaryPage>,
    pub busy: bool,
    pub legacy: bool,
    pub error: Option<String>,
}

#[derive(Clone, Default)]
pub(super) struct Reading {
    pub query: Option<MessageWindowQuery>,
    pub page: Option<MessageWindowPage>,
    pub busy: bool,
    pub error: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct Command {
    pub action: String,
    #[serde(default)]
    pub target: String,
}

#[component]
pub(super) fn HistoricalReader(
    state: Signal<Reading>,
    onload: EventHandler<MessageWindowQuery>,
    onclose: EventHandler,
) -> Element {
    let current = state.read().clone();
    let query = current.query.clone();
    let anchor = query.as_ref().and_then(|query| query.anchor_id.clone());
    let first = current.page.as_ref().and_then(|page| page.first_id.clone());
    use_effect(use_reactive(
        (&anchor, &first, &current.busy),
        move |(anchor, _, busy)| {
            if !busy {
                let target = serde_json::to_string(&anchor).unwrap();
                let _ = document::eval(&format!(
                    "const panel=document.querySelector('.timeline-history-content');if(panel){{const id={target};const row=[...panel.querySelectorAll('[data-historical-id]')].find(row=>row.dataset.historicalId===id);panel.scrollTop=row?row.offsetTop-panel.offsetTop:0;}}"
                ));
            }
        },
    ));
    let before = current
        .page
        .as_ref()
        .filter(|page| page.has_more_before)
        .and_then(|page| page.next_before_id.clone());
    let after = current
        .page
        .as_ref()
        .filter(|page| page.has_more_after)
        .and_then(|page| page.next_after_id.clone());
    let leaf = query.as_ref().and_then(|query| query.leaf_id.clone());
    let before_leaf = leaf.clone();
    rsx! {
        if current.query.is_some() {
            section { class: "timeline-history-reader", aria_label: "历史消息窗口", aria_busy: current.busy,
                header { class: "timeline-history-controls",
                    strong { "历史消息" }
                    button { class: "button", onclick: move |_| onclose.call(()), "返回实时消息" }
                    button { class: "button", disabled: current.busy || before.is_none(), onclick: move |_| {
                        onload.call(MessageWindowQuery { before_id: before.clone(), leaf_id: before_leaf.clone(), ..MessageWindowQuery::default() });
                    }, "更早的历史消息" }
                    button { class: "button", disabled: current.busy || after.is_none(), onclick: move |_| {
                        onload.call(MessageWindowQuery { after_id: after.clone(), leaf_id: leaf.clone(), ..MessageWindowQuery::default() });
                    }, "更晚的历史消息" }
                }
                if current.busy { p { role: "status", "正在读取目标附近的消息…" } }
                if let Some(error) = current.error {
                    p { role: "alert", "{error}" }
                    button { class: "button", disabled: current.busy, onclick: move |_| {
                        if let Some(query) = query.clone() { onload.call(query); }
                    }, "重试历史消息" }
                }
                div { class: "timeline-history-content",
                    if let Some(page) = current.page {
                        for message in &page.messages {
                            { let entry = message["entryId"].as_str().unwrap_or_default(); rsx! {
                            div { key: "{entry}", "data-historical-id": "{entry}",
                                {super::timeline::render_history_in_context(message, &page.messages, &[])}
                            }
                            } }
                        }
                    }
                }
            }
        }
    }
}
