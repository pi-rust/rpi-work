use super::history_window::HistoryWindow;
use dioxus::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use work_client::{ClientError, MESSAGE_PAGE_SIZE, WorkspaceClient};

const OBSERVER: &str = include_str!("history_pager.js");
const CSS: &str = include_str!("history_pager.css");
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

#[component]
pub(crate) fn HistoryPager(
    scope: String,
    generation: u64,
    current_generation: Signal<u64>,
    client: Signal<Option<WorkspaceClient>>,
    session_id: String,
    mut window: Signal<HistoryWindow>,
    mut transcript: Signal<Vec<serde_json::Value>>,
    #[props(default)] native_queries: bool,
    #[props(default)] timeline_visible: bool,
) -> Element {
    let instance = use_hook(|| NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed));
    let id = format!("rpi-history-{instance}");
    let callback_scope = scope.clone();
    let load = use_callback(move |()| {
        if *current_generation.peek() != generation
            || !window.peek().matches(&callback_scope, generation)
        {
            return;
        }
        let Some(client) = client.peek().clone() else {
            return;
        };
        let Some(cursor) = window.write().claim() else {
            return;
        };
        let revision = window.peek().revision;
        let session_id = session_id.clone();
        let scope = callback_scope.clone();
        spawn(async move {
            let result = client
                .message_page(&session_id, MESSAGE_PAGE_SIZE, &cursor)
                .await;
            if *current_generation.peek() != generation
                || !window.peek().matches(&scope, generation)
                || window.peek().revision != revision
            {
                return;
            }
            match result {
                Ok(page) => {
                    // Capture the *current* reading location, not the location when the slow request began.
                    let encoded = serde_json::to_string(&scope).unwrap();
                    let script = format!(
                        "for(const s of window.__rpiScrollFollow?.instances?.values()||[]){{if(s.scope==={encoded})s.preserveReadingAnchor?.();}}dioxus.send(true);"
                    );
                    let _ = document::eval(&script).recv::<bool>().await;
                    if *current_generation.peek() != generation
                        || !window.peek().matches(&scope, generation)
                        || window.peek().revision != revision
                    {
                        return;
                    }
                    let tail = transcript.peek().clone();
                    let merged = window.write().apply(&cursor, page, &tail);
                    match merged {
                        Ok(messages) => transcript.set(messages),
                        Err(error) => window.write().fail(error),
                    }
                }
                Err(error) => {
                    let unsupported = matches!(error, ClientError::Unsupported { .. });
                    window.write().fail(format!("读取更早消息失败：{error}"));
                    if unsupported {
                        window.write().enabled = false;
                    }
                }
            }
        });
    });
    // Only fill the ten-tick rail when explicitly shown. Hidden navigation must
    // not download older history; normal user-driven pagination stays available.
    let fill_scope = scope.clone();
    use_effect(use_reactive(
        (&timeline_visible,),
        move |(timeline_visible,)| {
            let state = window.read();
            let fill = timeline_visible
                && state.matches(&fill_scope, generation)
                && state.enabled
                && state.has_more
                && !state.busy
                && state.error.is_none()
                && state.cursor.is_some()
                && state.pages < 10
                && !native_queries
                && super::user_timeline::needs_initial_history(&transcript.read());
            drop(state);
            if fill {
                load.call(());
            }
        },
    ));
    let mut observer_task = use_signal(|| None::<dioxus::core::Task>);
    let cursor = window.read().cursor.clone();
    let enabled = {
        let state = window.read();
        state.enabled && state.has_more && !state.busy && state.error.is_none()
    };
    let manual = {
        let state = window.read();
        state.enabled && state.has_more && !state.busy && state.cursor.is_some()
    };
    let effect_id = id.clone();
    use_effect(use_reactive(
        (&scope, &cursor, &enabled, &manual),
        move |(scope, cursor, enabled, manual)| {
            if let Some(task) = observer_task.write().take() {
                task.cancel();
            }
            let encoded_id = serde_json::to_string(&effect_id).unwrap();
            let encoded_scope = serde_json::to_string(&scope).unwrap();
            let script = format!(
                "const id={encoded_id};const scope={encoded_scope};const manual={manual};const enabled={};{OBSERVER}",
                enabled && cursor.is_some()
            );
            observer_task.set(Some(spawn(async move {
                let mut evaluator = document::eval(&script);
                while evaluator.recv::<bool>().await.is_ok() {
                    load.call(());
                }
            })));
        },
    ));
    let cleanup_id = serde_json::to_string(&id).unwrap();
    let cleanup_scope = scope.clone();
    use_drop(move || {
        if let Some(task) = observer_task.write().take() {
            task.cancel();
        }
        let _ = document::eval(&format!(
            "window.__rpiHistoryPagers?.get({cleanup_id})?.();"
        ));
        if let Ok(mut state) = window.try_write()
            && state.matches(&cleanup_scope, generation)
            && state.busy
        {
            state.fail("历史加载已取消，请重试。".into());
        }
    });
    let state = window.read();
    rsx! {
        style { {CSS} }
        div { id, class: "history-pager", "data-session-scope": scope, role: "region", aria_label: "会话历史分页", aria_busy: state.busy,
            if let Some(error) = &state.error {
                p { role: "status", "{error}" }
            }
            if state.enabled && state.has_more && state.cursor.is_some() {
                button { r#type: "button", class: "button", disabled: state.busy, onclick: move |_| load.call(()),
                    if state.busy { "正在加载更早消息…" } else if state.error.is_some() { "重试加载更早消息" } else { "加载更早消息" }
                }
            } else if state.has_more && state.error.is_none() {
                p { role: "status", "当前节点未提供可用的历史分页能力或真实消息游标。" }
            } else if !state.has_more && state.pages > 0 {
                p { role: "status", "已加载到会话开头" }
            }
        }
    }
}
