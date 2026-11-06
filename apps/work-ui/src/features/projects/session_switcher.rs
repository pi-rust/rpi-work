use crate::shared::components::{Icon, Modal};
use dioxus::prelude::*;
use std::collections::HashMap;
use work_client::{Project, SessionSummary};

use super::{management::switcher_session_status, project_picker::project_label};

const CSS: &str = include_str!("session_switcher.css");

fn matches_query(session: &SessionSummary, projects: &[Project], label: &str, query: &str) -> bool {
    let project = projects
        .iter()
        .find(|project| session.project_id.as_deref() == Some(&project.id));
    let searchable = format!(
        "{} {} {} {} {} {}",
        session.title.as_deref().unwrap_or("未命名会话"),
        project_label(projects, session.project_id.as_deref()),
        project.map_or("", |project| &project.path),
        session.session_id,
        label,
        session.status
    )
    .replace('\\', "/")
    .to_lowercase();
    let query = query.replace('\\', "/").to_lowercase();
    query
        .split_whitespace()
        .all(|word| searchable.contains(word))
}

#[component]
pub fn SessionSwitcher(
    projects: Vec<Project>,
    sessions: Vec<SessionSummary>,
    current_session: Option<String>,
    #[props(default = "grouped".to_owned())] display_mode: String,
    #[props(default)] node_label: String,
    #[props(default)] states: HashMap<String, (String, String)>,
    onsession: EventHandler<String>,
    onclose: EventHandler<()>,
) -> Element {
    let mut query = use_signal(String::new);
    let matching = sessions
        .iter()
        .filter_map(|session| {
            let state = states.get(&session.session_id).cloned().unwrap_or_else(|| {
                let (tone, label) = switcher_session_status(session, None);
                (tone.to_owned(), label.to_owned())
            });
            matches_query(session, &projects, &state.1, &query.read()).then_some((session, state))
        })
        .collect::<Vec<_>>();
    let first = matching
        .first()
        .map(|(session, _)| session.session_id.clone());
    rsx! {
        style { "{CSS}" }
        Modal { id: "session-switcher", title: "切换会话", class: "session-switcher-modal", trap_focus: true, onclose,
            div { class: "modal-body",
                div { class: "session-switcher-search",
                    Icon { name: "search" }
                    input { id: "session-switcher-query", r#type: "search", aria_label: "搜索会话或项目", aria_controls: "session-switcher-results",
                        placeholder: "搜索会话、项目或状态…", value: "{query}", autocomplete: "off", spellcheck: false,
                        oninput: move |event| query.set(event.value()),
                        onkeydown: move |event| {
                            if event.is_composing() { return; }
                            if event.key() == Key::ArrowDown {
                                event.prevent_default();
                                let _ = document::eval("document.querySelector('#session-switcher-results button')?.focus();");
                            } else if event.key() == Key::Enter {
                                event.prevent_default();
                                if let Some(id) = first.as_ref() { onsession.call(id.clone()); }
                            }
                        }
                    }
                    if !query.read().is_empty() {
                        button { class: "switcher-clear", aria_label: "清除会话搜索", onclick: move |_| {
                            query.set(String::new());
                            let _ = document::eval("document.getElementById('session-switcher-query')?.focus();");
                        }, Icon { name: "close" } }
                    }
                }
                div { class: "session-switcher-toolbar",
                    span { "所有项目 · {matching.len()} / {sessions.len()} 个会话" }
                    span { "状态以最近上报为准" }
                }
                div { class: "session-switcher-display-mode", role: "status",
                    if display_mode == "grouped" { "按节点分组" } else { "平铺会话" }
                    if display_mode == "grouped" && !node_label.is_empty() { span { " · {node_label}" } }
                }
                div { id: "session-switcher-results", class: "session-switcher-results", "data-session-display-mode": display_mode.clone(),
                    onkeydown: move |event| {
                        let direction = match event.key() {
                            Key::ArrowDown => "down", Key::ArrowUp => "up", Key::Home => "first", Key::End => "last", _ => return,
                        };
                        event.prevent_default();
                        let _ = document::eval(&format!(r"
                            const rows=[...document.querySelectorAll('#session-switcher-results button')];
                            const i=rows.indexOf(document.activeElement),n=rows.length;
                            const next={{down:(i+1)%n,up:(i+n-1)%n,first:0,last:n-1}}['{direction}'];
                            rows[next]?.focus();
                        "));
                    },
                    if display_mode == "grouped" && !node_label.is_empty() {
                        div { class: "session-switcher-node-heading", role: "heading", aria_level: "3",
                            Icon { name: "server" } strong { "{node_label}" } span { "{matching.len()} 个会话" }
                        }
                    }
                    for (session, (tone, label)) in matching.iter() {
                        {
                            let id = session.session_id.clone();
                            let current = current_session.as_ref() == Some(&id);
                            let title = session.title.as_deref().filter(|title| !title.trim().is_empty()).unwrap_or("未命名会话");
                            let project = project_label(&projects, session.project_id.as_deref());
                            rsx! {
                                button { key: "switch-{id}", "data-session-id": "{id}", class: if current { "session-switcher-row current" } else { "session-switcher-row" },
                                    aria_label: "{title}", "aria-description": "{project} · {label}", aria_current: if current { "true" } else { "false" },
                                    onclick: move |_| onsession.call(id.clone()),
                                    Icon { name: "message" }
                                    span { class: "switcher-session-content",
                                        strong { title: "{title}", "{title}" }
                                        small { class: "switcher-session-meta", span { title: "{project}", "{project}" } if current { em { "当前" } } }
                                    }
                                    span { class: "switcher-session-state {tone}",
                                        i { "aria-hidden": "true" } span { "{label}" }
                                    }
                                }
                            }
                        }
                    }
                    if matching.is_empty() {
                        p { class: "search-empty", role: "status", "没有匹配的会话，试试名称、项目或状态。" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_matches_multiple_words_project_paths_status_and_id() {
        let session: SessionSummary = serde_json::from_value(serde_json::json!({
            "sessionId":"test-id", "projectId":"project", "title":"检查中文搜索",
            "status":"waiting_for_input", "createdAt":0, "updatedAt":0, "lastSeq":0
        }))
        .unwrap();
        let projects = vec![Project {
            id: "project".into(),
            name: "Demo".into(),
            path: "D:/Projects/demo".into(),
        }];
        for query in [
            "",
            " 中文  DEMO ",
            "等待回答",
            "test-id",
            "d:\\projects\\demo",
        ] {
            assert!(
                matches_query(&session, &projects, "等待回答", query),
                "{query}"
            );
        }
        assert!(!matches_query(&session, &projects, "等待回答", "完成"));
        assert!(!matches_query(
            &session,
            &projects,
            "等待回答",
            "中文 missing"
        ));
    }
}
