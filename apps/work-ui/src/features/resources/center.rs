//! Resource navigation keeps discovery, node authority and local designs separate.
use super::library::{ResourceLibrary, ResourcePage};
use dioxus::prelude::*;
use work_client::WorkspaceClient;

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
    project: Option<String>,
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
    let context_key = serde_json::to_string(&(
        &kind,
        &project,
        &session,
        connection_epoch(),
        session_generation(),
    ))
    .unwrap_or_default();
    rsx! {
        for owner in [context_key] {
            ResourceCenterState { key: "{owner}", kind: kind.clone(), project: project.clone(), session: session.clone(), session_name: session_name.clone(), session_scope: session_scope.clone(),
                library, shared_client, connection_epoch, session_generation, idle, online, onopen, onconfigure, onback }
        }
    }
}

#[component]
fn ResourceCenterState(
    kind: String,
    project: Option<String>,
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
    let title = match kind.as_str() {
        "skills" => "技能库",
        "extensions" => "扩展",
        _ => "Packages",
    };
    let category = resource_kind(&kind);
    let project_label = project.as_deref().unwrap_or("未选择项目");
    rsx! {
        section { class: "resource-center", aria_label: "{title}资源中心",
            style { {include_str!("center.css")} }
            header { class: "resource-center-heading",
                div {
                    h1 { "{title}" }
                    p { "节点资源、发现目录和本地设计，分开管理。" }
                }
                button { class: "button", onclick: move |_| onback.call(()), "返回对话" }
            }
            div { class: "resource-center-context",
                span { "当前项目：{project_label}" }
                span { if online { "节点在线" } else { "节点离线" } }
                button { class: "button", disabled: session.is_none(), onclick: move |_| onconfigure.call(()), "会话资源设置" }
            }
            nav { class: "resource-center-views", aria_label: "资源视图",
                for (key, label) in [("node", "当前节点"), ("discovery", "发现资源"), ("design", "本地设计")] {
                    button {
                        r#type: "button", aria_pressed: view() == key,
                        class: if view() == key { "active" } else { "" },
                        onclick: move |_| {
                            view.set(key.to_owned());
                            if key == "discovery" { discovered.set(true); }
                            if key == "design" { designed.set(true); }
                        }, "{label}"
                    }
                }
            }
            // Keep visited owners mounted: switching views must not discard search,
            // trust context or an in-flight receipt. Context changes key the center.
            div { hidden: view() != "node",
                if let Some(project) = project.clone() {
                    super::native::NativeResources {
                        project, session: session.clone(), kind: category.to_owned(),
                        shared_client, connection_epoch, session_generation, idle, online, onopen, revision: revision(),
                    }
                } else {
                    p { class: "resource-empty", role: "status", "请先连接执行节点并选择项目，再查看节点资源。" }
                }
            }
            if discovered() {
                div { hidden: view() != "discovery",
                    if let Some(project) = project.clone() {
                        super::packages::PackageDiscovery {
                            project, resource_kind: category.to_owned(), shared_client, connection_epoch,
                            session_generation, online, onchanged: move |_| { revision += 1; },
                        }
                    } else { p { class: "resource-empty", "请选择项目后读取发现目录。" } }
                }
            }
            if designed() {
                div { hidden: view() != "design",
                    ResourcePage { kind, library, session_name, session_scope }
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
