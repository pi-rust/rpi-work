use crate::shared::components::{Icon, Modal};
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};
use work_client::{Project, SessionSummary};

use super::{management::switcher_session_status, project_picker::project_label};

const CSS: &str = include_str!("session_switcher.css");

pub(super) fn matches_query(
    session: &SessionSummary,
    projects: &[Project],
    label: &str,
    query: &str,
) -> bool {
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
fn SessionSwitcherRow(
    session: SessionSummary,
    current: bool,
    project: String,
    tone: String,
    label: String,
    onsession: EventHandler<String>,
) -> Element {
    let id = session.session_id.clone();
    let title = session
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or("未命名会话")
        .to_owned();
    rsx! {
        button {
            key: "switch-{id}", "data-session-id": "{id}",
            class: if current { "session-switcher-row current" } else { "session-switcher-row" },
            aria_label: "{title}", "aria-description": "{project} · {label}",
            aria_current: if current { "true" } else { "false" },
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
    let mut node_expanded = use_signal(|| true);
    let mut collapsed_projects = use_signal(HashSet::<String>::new);
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
    let node_tree_label = if node_label.trim().is_empty() {
        "当前节点".to_owned()
    } else {
        node_label.clone()
    };
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
                    if display_mode == "grouped" {
                        button {
                            class: if *node_expanded.read() { "session-switcher-tree-node expanded" } else { "session-switcher-tree-node" },
                            r#type: "button", role: "treeitem", aria_expanded: node_expanded.to_string(),
                            aria_label: format!("{}节点，{} 个会话", node_tree_label, matching.len()),
                            onclick: move |_| node_expanded.toggle(),
                            Icon { name: "chevron-right" }
                            Icon { name: if node_tree_label == "本地节点" { "monitor" } else { "cloud" } }
                            strong { "{node_tree_label}" } span { "{matching.len()} 个会话" }
                        }
                        if *node_expanded.read() {
                            for project in projects.iter() {
                                {
                                    let project_id = project.id.clone();
                                    let project_name = project_label(&projects, Some(&project_id));
                                    let project_sessions = matching
                                        .iter()
                                        .filter(|(session, _)| session.project_id.as_deref() == Some(project_id.as_str()))
                                        .collect::<Vec<_>>();
                                    let expanded = !collapsed_projects.read().contains(&project_id);
                                    if project_sessions.is_empty() {
                                        rsx! {}
                                    } else {
                                        rsx! {
                                            div { class: "session-switcher-project-tree", role: "group",
                                                button {
                                                    class: if expanded { "session-switcher-tree-project expanded" } else { "session-switcher-tree-project" },
                                                    r#type: "button", aria_expanded: expanded.to_string(),
                                                    aria_label: format!("{} {} 的会话", if expanded { "收起" } else { "展开" }, project_name),
                                                    onclick: { let id = project_id.clone(); move |_| {
                                                        if collapsed_projects.peek().contains(&id) {
                                                            collapsed_projects.write().remove(&id);
                                                        } else {
                                                            collapsed_projects.write().insert(id.clone());
                                                        }
                                                    } },
                                                    Icon { name: "chevron-right" } Icon { name: "folder" }
                                                    strong { "{project_name}" } span { "{project_sessions.len()} 个会话" }
                                                }
                                                if expanded {
                                                    div { class: "session-switcher-project-children", role: "group",
                                                        for (session, (tone, label)) in project_sessions.iter() {
                                                            SessionSwitcherRow {
                                                                key: "switch-tree-{session.session_id}",
                                                                session: (*session).clone(),
                                                                current: current_session.as_ref() == Some(&session.session_id),
                                                                project: project_name.clone(), tone: (*tone).clone(), label: (*label).clone(), onsession,
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            {
                                let project_sessions = matching
                                    .iter()
                                    .filter(|(session, _)| session.project_id.is_none())
                                    .collect::<Vec<_>>();
                                if project_sessions.is_empty() {
                                    rsx! {}
                                } else {
                                    rsx! {
                                        div { class: "session-switcher-project-tree", role: "group",
                                            div { class: "session-switcher-tree-project standalone", role: "heading", aria_level: "4",
                                                Icon { name: "message" } strong { "独立会话" } span { "{project_sessions.len()} 个会话" }
                                            }
                                            div { class: "session-switcher-project-children", role: "group",
                                                for (session, (tone, label)) in project_sessions.iter() {
                                                    SessionSwitcherRow {
                                                        key: "switch-tree-{session.session_id}",
                                                        session: (*session).clone(),
                                                        current: current_session.as_ref() == Some(&session.session_id),
                                                        project: "独立会话".to_owned(), tone: (*tone).clone(), label: (*label).clone(), onsession,
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        for (session, (tone, label)) in matching.iter() {
                            SessionSwitcherRow {
                                key: "switch-flat-{session.session_id}",
                                session: (*session).clone(),
                                current: current_session.as_ref() == Some(&session.session_id),
                                project: project_label(&projects, session.project_id.as_deref()),
                                tone: (*tone).clone(), label: (*label).clone(), onsession,
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
