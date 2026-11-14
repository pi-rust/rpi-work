//! Resource navigation keeps discovery, node authority and local designs separate.
use super::library::{ResourceLibrary, ResourcePage};
use crate::shared::components::Icon;
use dioxus::prelude::*;
use work_client::WorkspaceClient;
use work_core::api::NativeKind;

pub(super) fn resource_kind(kind: &str) -> &'static str {
    match kind {
        "skills" => "skill",
        "extensions" => "extension",
        _ => "package",
    }
}

#[component]
pub fn ResourceCenter(
    kind: String,
    categories: Vec<(String, String)>,
    project: Option<String>,
    project_name: Option<String>,
    session: Option<String>,
    session_name: Option<String>,
    session_scope: Option<String>,
    library: Signal<ResourceLibrary>,
    shared_client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    session_generation: Signal<u64>,
    idle: bool,
    online: bool,
    onopen: EventHandler<String>,
    onconfigure: EventHandler<()>,
    onback: EventHandler<()>,
) -> Element {
    let mut categories = categories;
    categories.sort_by_key(|(id, _)| match id.as_str() {
        "packages" => 0,
        "skills" => 1,
        _ => 2,
    });
    let context_key = serde_json::to_string(&(
        &kind,
        &project,
        &project_name,
        &session,
        connection_epoch(),
        session_generation(),
    ))
    .unwrap_or_default();
    rsx! {
        for owner in [context_key] {
            ResourceCenterState { key: "{owner}", kind: kind.clone(), project: project.clone(), project_name: project_name.clone(), session: session.clone(), session_name: session_name.clone(), session_scope: session_scope.clone(),
                categories: categories.clone(), library, shared_client, connection_epoch, session_generation, idle, online, onopen, onconfigure, onback }
        }
    }
}

#[component]
fn ResourceCenterState(
    kind: String,
    categories: Vec<(String, String)>,
    project: Option<String>,
    project_name: Option<String>,
    session: Option<String>,
    session_name: Option<String>,
    session_scope: Option<String>,
    library: Signal<ResourceLibrary>,
    shared_client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    session_generation: Signal<u64>,
    idle: bool,
    online: bool,
    onopen: EventHandler<String>,
    onconfigure: EventHandler<()>,
    onback: EventHandler<()>,
) -> Element {
    let mut view = use_signal(|| "node".to_owned());
    let mut discovered = use_signal(|| false);
    let mut designed = use_signal(|| false);
    let mut revision = use_signal(|| 0u64);
    let mut query = use_signal(String::new);
    let mut scope = use_signal(|| "all".to_owned());
    let mut counts = use_signal(|| None::<[usize; 3]>);
    let mut category_counts = use_signal(|| None::<[usize; 3]>);
    let mut install_open = use_signal(|| false);
    let mut category_selection = use_signal(|| {
        categories
            .iter()
            .find(|(id, _)| id == "packages")
            .or_else(|| categories.first())
            .map_or_else(|| "packages".to_string(), |(id, _)| id.clone())
    });
    let unified = kind == "resources";
    let current_kind = if unified {
        category_selection()
    } else {
        kind.clone()
    };
    let title = match kind.as_str() {
        "resources" => "资源中心",
        "skills" => "技能库",
        "extensions" => "扩展",
        _ => "Packages",
    };
    let category = resource_kind(&current_kind);
    let project_label = project_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .or(project.as_deref())
        .unwrap_or("未选择项目");
    rsx! {
        section { class: "resource-center", aria_label: "{title}资源中心",
            style { {include_str!("center.css")} }
            header { class: "resource-center-heading",
                div {
                    if unified { p { class: "resource-center-eyebrow", "让能力有序，让工作专注" } }
                    h1 { "{title}" }
                    if unified {
                        p { "管理 Packages、技能与扩展，为当前项目组合恰好的能力。" }
                    }
                }
                if unified {
                    button { class: "button primary", disabled: project.is_none(), onclick: move |_| {
                        view.set("node".to_owned());
                        install_open.set(true);
                        let _ = document::eval(r#"requestAnimationFrame(() => {
                            const entry = document.querySelector('.resource-center-panel:not([hidden]) .native-install-entry[open]');
                            entry?.scrollIntoView({block: 'start'});
                            entry?.querySelector('input')?.focus({preventScroll: true});
                        });"#);
                    }, Icon { name: "plus" } "导入资源" }
                } else {
                    button { class: "button", onclick: move |_| onback.call(()), "返回对话" }
                }
            }
            if unified {
                div { class: "resource-center-overview", aria_label: "节点资源概览",
                    for (index, name, icon, caption) in [(0, "Packages", "blocks", "已安装"), (1, "Skills", "skills", "目录技能"), (2, "Extensions", "tool", "目录扩展")] {
                        div {
                            span { class: "resource-overview-icon", Icon { name: icon } }
                            div {
                                p { "{name}" }
                                strong {
                                    if let Some(values) = counts() { "{values[index]:02}" } else { "—" }
                                    span { "{caption}" }
                                }
                            }
                        }
                    }
                }
            }
            div { class: if unified { "resource-center-surface" } else { "resource-simple-surface" },
            if !unified {
                div { class: "resource-center-context resource-simple-context",
                    span { class: "resource-context-project", "当前项目：{project_label}" }
                }
            } else {
                div { class: "resource-center-context",
                    span { class: "resource-context-project", Icon { name: "monitor" } strong { "当前节点" } Icon { name: "chevron-right" } Icon { name: "folder" } "{project_label}" }
                    span { class: if online { "resource-context-status online" } else { "resource-context-status offline" }, role: "status", if online { "节点在线" } else { "节点离线" } }
                }
                div { class: "resource-center-nav",
                nav { class: "resource-center-views", aria_label: "资源视图",
                    for (key, label) in [("node", "当前节点"), ("discovery", "发现资源"), ("design", "本地设计")] {
                        button {
                            r#type: "button", aria_pressed: view() == key,
                            aria_current: if view() == key { "page" } else { "false" },
                            class: if view() == key { "active" } else { "" },
                            onclick: move |_| {
                                view.set(key.to_owned());
                                if key == "discovery" { discovered.set(true); }
                                if key == "design" { designed.set(true); }
                            }, "{label}"
                        }
                    }
                }
                if view() == "node" {
                    label { class: "resource-center-search",
                        Icon { name: "search" }
                        input { r#type: "search", aria_label: "搜索节点资源", placeholder: "搜索资源名称或说明…", value: "{query}", oninput: move |e| query.set(e.value()) }
                    }
                }
                }
            }
            if unified {
                nav { class: "resource-center-categories", aria_label: "资源类型",
                    for (id, label) in categories {
                        { let count = category_counts().filter(|_| view() == "node").map(|values| values[match id.as_str() { "packages" => 0, "skills" => 1, _ => 2 }]); rsx! {
                        button { key: "{id}", r#type: "button",
                            aria_label: "{label}",
                            aria_pressed: current_kind == id,
                            onclick: move |_| { category_selection.set(id.clone()); query.set(String::new()); scope.set("all".to_owned()); },
                            "{label}"
                            if let Some(value) = count {
                                span { class: "resource-category-count", "{value}" }
                            }
                        }
                        } }
                    }
                    if view() == "node" {
                        select { class: "resource-center-scope", aria_label: "资源范围", value: "{scope}", onchange: move |e| scope.set(e.value()),
                            option { value: "all", "全部范围" } option { value: "project", "当前项目" } option { value: "global", "节点全局" }
                        }
                    }
                }
            }
            // Keep visited owners mounted: switching views must not discard search,
            // trust context or an in-flight receipt. Context changes key the center.
            div { class: "resource-center-panel", hidden: view() != "node",
                if let Some(project) = project.clone() {
                    super::native::NativeResources { key: "{category}", expanded_list: unified,
                        install_open_override: if unified { Some(install_open) } else { None },
                        query_override: if unified { Some(query) } else { None },
                        scope_override: if unified { Some(scope) } else { None },
                        oncatalog: move |value: Option<work_core::api::NativeCatalog>| {
                            category_counts.set(value.as_ref().map(|catalog| [
                                catalog.resources.iter().filter(|r| r.kind == NativeKind::Package).count(),
                                catalog.resources.iter().filter(|r| r.kind == NativeKind::Skill).count(),
                                catalog.resources.iter().filter(|r| r.kind == NativeKind::Extension).count(),
                            ]));
                            counts.set(value.as_ref().map(|catalog| [
                                catalog.resources.iter().filter(|r| r.kind == NativeKind::Package && r.facts["installed"] == true).count(),
                                catalog.resources.iter().filter(|r| r.kind == NativeKind::Skill).count(),
                                catalog.resources.iter().filter(|r| r.kind == NativeKind::Extension).count(),
                            ]));
                        },
                        project, session: session.clone(), kind: category.to_owned(),
                        shared_client, connection_epoch, session_generation, idle, online, onopen, revision: revision(),
                    }
                } else {
                    p { class: "resource-empty", role: "status", "请先连接执行节点并选择项目，再查看节点资源。" }
                }
            }
            if discovered() {
                div { class: "resource-center-panel", hidden: view() != "discovery",
                    if let Some(project) = project.clone() {
                        super::packages::PackageDiscovery { key: "{category}",
                            project, resource_kind: category.to_owned(), shared_client, connection_epoch,
                            session_generation, online, onchanged: move |()| { revision += 1; },
                        }
                    } else { p { class: "resource-empty", "请选择项目后读取发现目录。" } }
                }
            }
            if designed() {
                div { class: "resource-center-panel", hidden: view() != "design",
                    ResourcePage { key: "{category}", kind: current_kind, library, session_name, session_scope }
                }
            }
            if unified {
                footer { class: "resource-center-footer",
                    p { "节点已安装资源与本地设计独立。查看详情后，再决定是否用于当前会话。" }
                    button { class: "button", disabled: session.is_none(), onclick: move |_| onconfigure.call(()), "会话资源设置" }
                    button { class: "resource-center-back", aria_label: "返回对话", title: "返回对话", onclick: move |_| onback.call(()), Icon { name: "back" } }
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
    fn navigation_maps_to_exact_node_and_discovery_category() {
        assert_eq!(resource_kind("extensions"), "extension");
        assert_eq!(resource_kind("skills"), "skill");
        assert_eq!(resource_kind("packages"), "package");
    }
}
