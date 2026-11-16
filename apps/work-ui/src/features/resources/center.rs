//! Unified resource browsing with explicit count, installation and work scopes.
use super::{
    catalog::{
        ResourceChoice, counts, installation_origin, installed_resources, kind_icon, kind_key,
        kind_label,
    },
    library::{ResourceLibrary, ResourcePage},
    workflow::{InstallDialog, WorkDialog},
};
use crate::shared::components::{Icon, Modal};
use dioxus::prelude::*;
use work_client::WorkspaceClient;
use work_core::api::{
    NativeCatalog, NativeKind, NativeManagement, NativeResource, NativeScope, NodeApi,
    PackageBundle, PackageCatalog, PackageOffer,
};

pub(super) fn resource_kind(kind: &str) -> &'static str {
    match kind {
        "skills" => "skill",
        "extensions" => "extension",
        "all" | "resources" => "",
        _ => "package",
    }
}

#[component]
pub fn ResourceCenter(
    kind: String,
    categories: Vec<(String, String)>,
    project: Option<String>,
    project_name: Option<String>,
    projects: Vec<(String, String)>,
    node_label: String,
    session: Option<String>,
    session_name: Option<String>,
    session_scope: Option<String>,
    library: Signal<ResourceLibrary>,
    legacy: Signal<crate::features::meta_agent::MetaStore>,
    shared_client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    session_generation: Signal<u64>,
    idle: bool,
    online: bool,
    onopen: EventHandler<String>,
    onconfigure: EventHandler<()>,
    onback: EventHandler<()>,
) -> Element {
    let _ = (categories, project_name);
    let owner = serde_json::to_string(&(
        &project,
        &session,
        connection_epoch(),
        session_generation(),
        online,
    ))
    .unwrap_or_default();
    rsx! {
        ResourceCenterState { key: "{owner}", initial_kind: kind, project, projects, node_label,
            session, session_name, session_scope, library, legacy, shared_client, connection_epoch,
            session_generation, idle, online, onopen, onconfigure, onback }
    }
}

#[component]
fn ResourceCenterState(
    initial_kind: String,
    project: Option<String>,
    projects: Vec<(String, String)>,
    node_label: String,
    session: Option<String>,
    session_name: Option<String>,
    session_scope: Option<String>,
    library: Signal<ResourceLibrary>,
    legacy: Signal<crate::features::meta_agent::MetaStore>,
    shared_client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    session_generation: Signal<u64>,
    idle: bool,
    online: bool,
    onopen: EventHandler<String>,
    onconfigure: EventHandler<()>,
    onback: EventHandler<()>,
) -> Element {
    let mut view = use_signal(|| "public".to_owned());
    let connection = use_context::<crate::app::connections::ConnectionStore>();
    let mut install_draft = use_context::<super::install_handoff::InstallDraft>().0;
    let mut kind = use_signal(|| {
        if initial_kind == "resources" {
            "all".into()
        } else {
            initial_kind
        }
    });
    let mut query = use_signal(String::new);
    let mut work_project = use_signal(|| {
        project
            .clone()
            .or_else(|| projects.first().map(|(id, _)| id.clone()))
            .unwrap_or_default()
    });
    use_effect(use_reactive!(|projects| {
        if !projects.iter().any(|(id, _)| id == &*work_project.peek()) {
            work_project.set(
                projects
                    .first()
                    .map(|(id, _)| id.clone())
                    .unwrap_or_default(),
            );
        }
    }));
    let mut browse_scope = use_signal(|| "global".to_owned());
    let mut native = use_signal(|| None::<NativeCatalog>);
    let mut public = use_signal(|| None::<PackageCatalog>);
    let mut catalog_owner = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut native_error = use_signal(String::new);
    let mut public_error = use_signal(String::new);
    let mut revision = use_signal(|| 0u64);
    let mut ticket = use_signal(|| 0u64);
    let mut choices = use_signal(|| Vec::<ResourceChoice>::new());
    let mut replacement = use_signal(|| None::<ResourceChoice>);
    let mut detail = use_signal(|| None::<NativeResource>);
    let mut offer_detail = use_signal(|| None::<PackageOffer>);
    let mut installing = use_signal(|| Vec::<ResourceChoice>::new());
    let mut working = use_signal(|| None::<bool>);
    let mut selection_open = use_signal(|| false);
    let mut management_open = use_signal(|| false);
    let mut import_open = use_signal(|| false);
    let mut authoring_open = use_signal(|| false);
    let mut receipt = use_signal(String::new);
    let mut manage_error = use_signal(String::new);
    let mut managing = use_signal(|| false);
    let session_project = use_signal(|| project);
    use_effect(move || {
        let Some(draft) = install_draft.read().clone() else {
            return;
        };
        let Some(catalog) = public.read().clone() else {
            return;
        };
        let found = draft
            .iter()
            .map(|package| {
                catalog
                    .packages
                    .iter()
                    .find(|offer| super::packages::reference(offer) == *package)
                    .cloned()
                    .map(ResourceChoice::Public)
            })
            .collect::<Option<Vec<_>>>();
        install_draft.set(None);
        if let Some(choices) = found {
            installing.set(choices);
        } else {
            receipt.set("目标节点公共库缺少所选版本或摘要，请核对资源后重新选择。".into());
        }
    });
    use_effect(move || {
        let _ = revision.read();
        let target = if view() == "installed" && browse_scope() != "global" {
            browse_scope()
        } else {
            work_project()
        };
        let epoch = *connection_epoch.read();
        let generation = *session_generation.read();
        let next = ticket.peek().wrapping_add(1);
        ticket.set(next);
        native.set(None);
        public.set(None);
        native_error.set(String::new());
        public_error.set(String::new());
        catalog_owner.set(target.clone());
        if !online || target.is_empty() {
            loading.set(false);
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            loading.set(false);
            native_error.set("节点连接尚未就绪。".into());
            public_error.set("节点连接尚未就绪。".into());
            return;
        };
        loading.set(true);
        spawn(async move {
            let (installed, directory) = futures_util::future::join(
                NodeApi::native_catalog(&client, &target),
                NodeApi::package_catalog(&client, &target),
            )
            .await;
            if *connection_epoch.peek() != epoch
                || *session_generation.peek() != generation
                || *ticket.peek() != next
            {
                return;
            }
            loading.set(false);
            match installed {
                Ok(catalog) => native.set(Some(catalog)),
                Err(error) => native_error.set(format!("目录读取失败，请重试。{error}")),
            }
            match directory {
                Ok(catalog) => public.set(Some(catalog)),
                Err(error) => public_error.set(format!("公共库读取失败，请重试。{error}")),
            }
        });
    });
    let mut choose = move |choice: ResourceChoice| {
        if choices
            .peek()
            .iter()
            .any(|entry| entry.key() == choice.key())
        {
            choices.write().retain(|entry| entry.key() != choice.key());
        } else if choices.peek().iter().any(|entry| {
            entry.kind() == choice.kind()
                && (choice.kind() == NativeKind::Package
                    || entry.native_name() == choice.native_name())
        }) {
            replacement.set(Some(choice));
        } else {
            choices.write().push(choice);
        }
    };
    let manage_project = catalog_owner();
    let mut manage = move |request: NativeManagement| {
        if managing() || loading() || !online {
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        let project = manage_project.clone();
        let epoch = *connection_epoch.peek();
        let generation = *session_generation.peek();
        managing.set(true);
        receipt.set(String::new());
        manage_error.set(String::new());
        spawn(async move {
            let result = NodeApi::native_manage(&client, &project, request).await;
            if *connection_epoch.peek() != epoch || *session_generation.peek() != generation {
                return;
            }
            managing.set(false);
            match result {
                Ok(value) => {
                    if !value.success {
                        manage_error.set(format!("节点未完成操作。{}", value.observation));
                    }
                    receipt.set(format!("{}\n{}", value.observation, value.diagnostics));
                }
                Err(error) => manage_error.set(format!("操作结果待核对，请回读目录。{error}")),
            }
            revision += 1;
        });
    };
    let global = browse_scope() == "global";
    let installed = native
        .read()
        .as_ref()
        .map(|catalog| installed_resources(catalog, global));
    let values = match view().as_str() {
        "public" => public
            .read()
            .as_ref()
            .map(|catalog| counts(catalog.packages.iter().map(|offer| &offer.manifest.kind))),
        "installed" => installed
            .as_ref()
            .map(|resources| counts(resources.iter().map(|resource| &resource.kind))),
        _ => Some([
            library.read().extensions.len(),
            library.read().skills.len(),
            library
                .read()
                .packages
                .iter()
                .filter(|package| package.resource_kind.eq_ignore_ascii_case("package"))
                .count(),
        ]),
    };
    let total = values.map(|counts| counts.iter().sum::<usize>());
    let browse_label = projects
        .iter()
        .find(|(id, _)| id == &browse_scope())
        .map(|(_, name)| name.clone())
        .unwrap_or_default();
    let needle = query().trim().to_lowercase();
    let matches = |resource_kind: NativeKind, text: String| {
        (self::resource_kind(&kind()).is_empty() || kind_key(resource_kind) == kind())
            && text.to_lowercase().contains(&needle)
    };
    let offers = public
        .read()
        .as_ref()
        .map(|catalog| {
            catalog
                .packages
                .iter()
                .filter(|offer| {
                    matches(
                        offer.manifest.kind,
                        format!(
                            "{} {} {} {} {}",
                            offer.manifest.name,
                            offer.manifest.id,
                            offer.manifest.title,
                            offer.manifest.description,
                            offer.manifest.author
                        ),
                    )
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let installed_list = installed
        .as_ref()
        .map(|resources| {
            resources
                .iter()
                .filter(|resource| {
                    matches(
                        resource.kind,
                        format!(
                            "{} {} {}",
                            resource.name, resource.facts["description"], resource.facts["source"]
                        ),
                    )
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let filtered_count = if view() == "public" {
        offers.len()
    } else {
        installed_list.len()
    };
    let failed = if view() == "public" {
        public_error()
    } else {
        native_error()
    };
    let selected = choices();
    let has_package = selected
        .iter()
        .any(|choice| choice.kind() == NativeKind::Package);
    let selection = rsx! {
        div { class: "resource-selection-content",
            div { class: "resource-selection-heading",
                h2 { "已选工具 " span { "{selected.len()}" } }
                button { class: "resource-text-button", disabled: selected.is_empty(), onclick: move |_| choices.set(vec![]), "清空" }
            }
            label { class: "resource-flow-field", "工作目标"
                select { aria_label: "工作目标项目", value: "{work_project}", onchange: move |event| work_project.set(event.value()),
                    for (id, name) in &projects { option { value: "{id}", "{name} · {node_label}" } }
                }
            }
            if selected.is_empty() { p { class: "resource-selection-empty", "选择需要的扩展、技能或 Package，它们会保留在这里。" } }
            for choice in &selected {
                { let key = choice.key(); rsx! {
                    div { class: "resource-selected-item",
                        Icon { name: kind_icon(choice.kind()) }
                        div { strong { "{choice.name()}" } small { "{kind_label(choice.kind())} · v{choice.version()}" } }
                        button { class: "icon-button", aria_label: "移除{choice.name()}", onclick: move |_| choices.write().retain(|choice| choice.key() != key), Icon { name: "close" } }
                    }
                } }
            }
            p { class: "resource-flow-note", "一个主 Package，可搭配多个技能与扩展。" }
            if !selected.is_empty() && !has_package { p { role: "status", class: "resource-flow-note", "请选择主 Package 后开始工作。" } }
            button { class: "button primary", disabled: !has_package || !online || work_project().is_empty(), onclick: move |_| { selection_open.set(false); working.set(Some(false)); }, "开始工作" span { class: "resource-forward-icon", Icon { name: "back" } } }
            button { class: "button", disabled: !has_package || !online || work_project().is_empty(), onclick: move |_| { selection_open.set(false); working.set(Some(true)); }, "设为项目默认组合" }
            button { class: "resource-text-button", onclick: move |_| onback.call(()), "返回对话" }
        }
    };
    rsx! {
        section { class: "resource-center resource-center-v2", aria_label: "资源中心", aria_busy: loading(),
            style { {include_str!("center.css")} {include_str!("workbench.css")} }
            header { class: "resource-center-heading",
                div { p { class: "resource-center-eyebrow", "为下一项工作选择合适的能力" } h1 { "资源中心" }
                    p { "浏览扩展、技能与 Packages，组合后开始工作。" } }
                div { class: "resource-heading-actions",
                    button { class: "button", onclick: move |_| { view.set("design".into()); kind.set("packages".into()); import_open.set(true); }, Icon { name: "download" } "导入设计" }
                    button { class: "button", disabled: !online || work_project().is_empty(), onclick: move |_| authoring_open.set(true), Icon { name: "code" } "开发工具" }
                }
            }
            if !receipt().is_empty() { p { class: "resource-flow-note", role: "status", "{receipt}" } }
            div { class: "resource-library-toolbar",
                nav { class: "resource-center-views", aria_label: "资源视图",
                    for (id, name) in [("public", "公共库"), ("installed", "已安装"), ("design", "本地设计")] {
                        button { r#type: "button", class: if view() == id { "active" } else { "" }, aria_pressed: view() == id,
                            onclick: move |_| view.set(id.into()), "{name}" }
                    }
                }
                if view() == "installed" {
                    label { class: "resource-installed-scope", "查看范围"
                        select { aria_label: "已安装资源范围", value: "{browse_scope}", onchange: move |event| browse_scope.set(event.value()),
                            option { value: "global", "{node_label} · 全局" }
                            for (id, name) in &projects { option { value: "{id}", "{node_label} · {name}（项目可用）" } }
                        }
                    }
                }
            }
            div { class: "resource-count-context",
                strong {
                    match view().as_str() {
                        "public" => "公共库 · 目录资源", "installed" if global => "节点全局安装",
                        "installed" => "项目可用资源", _ => "本地设计",
                    }
                    if view() == "installed" { " · {node_label}" if !global { " · {browse_label}" } }
                    if let Some(total) = total { " · {total} 项" } else { " · 数量未核实" }
                }
                span {
                    match view().as_str() {
                        "public" => "按类型、资源 ID 与版本统计目录条目。",
                        "installed" if global => "仅统计此节点的全局安装版本。",
                        "installed" => "包含项目安装与全局可用资源，同一版本合并计数。",
                        _ => "本设备保存的设计条目，发布版本分别计数。",
                    }
                }
            }
            div { class: "resource-center-overview", aria_label: "当前来源的分类数量",
                for (index, id, label, icon) in [(0, "extensions", "扩展", "tool"), (1, "skills", "技能", "skills"), (2, "packages", "Packages", "blocks")] {
                    button { onclick: move |_| kind.set(id.into()), aria_label: "筛选{label}",
                        span { class: "resource-overview-icon", Icon { name: icon } }
                        span { span { class: "resource-overview-label", "{label}" }
                            strong { if let Some(values) = values { "{values[index]:02}" } else { "—" } }
                            small {
                                if values.is_none() { "数量未核实" }
                                else if view() == "installed" && !global {
                                    { let inherited = native.read().as_ref().map(|catalog| installed_resources(catalog, true)).map(|resources| counts(resources.iter().map(|resource| &resource.kind))[index]).unwrap_or(0); rsx! { "全局可用 {inherited} · 项目独有 {values.map_or(0, |counts| counts[index]).saturating_sub(inherited)}" } }
                                } else if view() == "public" { "目录资源版本" } else if view() == "installed" { "全局安装版本" } else { "本地设计条目" }
                            }
                        }
                        span { class: "resource-forward-icon", Icon { name: "back" } }
                    }
                }
            }
            div { class: "resource-center-workbench",
                div { class: "resource-center-surface",
                    if view() != "design" {
                        div { class: "resource-center-nav",
                            label { class: "resource-center-search", Icon { name: "search" }
                                input { r#type: "search", aria_label: "搜索资源", value: "{query}", placeholder: "搜索名称、用途或来源…", oninput: move |event| query.set(event.value()) } }
                            button { class: "resource-text-button resource-refresh", aria_label: "刷新目录 / 回读", title: "刷新目录 / 回读", disabled: loading() || !online, onclick: move |_| revision += 1,
                                Icon { name: "refresh" } span { class: "resource-refresh-label", "刷新目录 / 回读" } }
                        }
                    }
                    div { class: "resource-center-filter-row",
                        nav { class: "resource-center-categories", aria_label: "资源类型",
                            for (id, name) in [("all", "全部"), ("extensions", "扩展"), ("skills", "技能"), ("packages", "Packages")] {
                                button { aria_pressed: kind() == id, onclick: move |_| kind.set(id.into()), "{name}" }
                            }
                        }
                        if view() != "design" { span { class: "resource-result-count", "显示 {filtered_count} / 共 " if let Some(total) = total { "{total}" } else { "—" } " 项" } }
                    }
                    if view() == "design" {
                        for category in ["extensions", "skills", "packages"].into_iter().filter(|category| kind() == "all" || kind() == *category) {
                            ResourcePage { key: "{category}", kind: category.to_owned(), library, session_name: session_name.clone(), session_scope: session_scope.clone(), import_open_override: Some(import_open) }
                        }
                    } else {
                        if !online { p { class: "resource-empty", role: "status", "执行节点离线，请重新连接后读取资源。" } }
                        else if work_project().is_empty() { p { class: "resource-empty", role: "status", "请先选择或创建工作项目，再读取节点资源目录。" } }
                        else if loading() { p { class: "resource-empty", role: "status", "正在读取资源目录…" } }
                        if !failed.is_empty() { p { role: "alert", class: "resource-error resource-state-block", "{failed}" } }
                        if !loading() && failed.is_empty() && total.is_some() && filtered_count == 0 {
                            div { class: "resource-empty", role: "status", p { "没有匹配的资源，可调整关键词、分类或查看范围。" }
                                button { class: "button", onclick: move |_| { query.set(String::new()); kind.set("all".into()); }, "清除筛选" } }
                        }
                        div { class: "resource-center-list",
                            if view() == "public" {
                                for offer in offers {
                                    { let choice = ResourceChoice::Public(offer.clone()); let is_selected = selected.iter().any(|entry| entry.key() == choice.key()); let detail_offer = offer.clone(); let install_choice = choice.clone(); rsx! {
                                        article { class: "resource-workbench-row package-card", "data-resource-kind": kind_key(offer.manifest.kind),
                                            span { class: "resource-row-icon", Icon { name: kind_icon(offer.manifest.kind) } }
                                            div { class: "resource-row-heading", h2 { "{offer.manifest.title}" span { "{kind_label(offer.manifest.kind)}" } } p { "{offer.manifest.id} · v{offer.manifest.version}" } }
                                            p { class: "resource-row-description", "{offer.manifest.description}" }
                                            div { class: "resource-row-meta", span { "{offer.manifest.author}" } span { "{offer.file_count} 个文件" } }
                                            div { class: "resource-row-actions",
                                                span { if catalog_owner() == work_project() && native.read().as_ref().is_some_and(|catalog| choice.available(catalog).is_some()) { "工作项目可用" } else { "安装状态待核对" } }
                                                button { class: "button", onclick: move |_| offer_detail.set(Some(detail_offer.clone())), "查看详情" }
                                                button { class: "button", disabled: !online, onclick: move |_| installing.set(vec![install_choice.clone()]), "安装" }
                                                button { class: if is_selected { "button selected" } else { "button primary" }, aria_pressed: is_selected, onclick: move |_| choose(choice.clone()), if is_selected { "已选" } else { "选择" } }
                                            }
                                        }
                                    } }
                                }
                            } else {
                                for resource in installed_list {
                                    { let choice = ResourceChoice::Installed(resource.clone()); let display_name = choice.name(); let is_selected = selected.iter().any(|entry| entry.key() == choice.key()); let origin = native.read().as_ref().map(|catalog| installation_origin(&resource, catalog)).unwrap_or("安装来源待核对"); let open_resource = resource.clone(); let install_choice = choice.clone(); rsx! {
                                        article { class: "resource-workbench-row native-resource-card", "data-resource-kind": kind_key(resource.kind),
                                            span { class: "resource-row-icon", Icon { name: kind_icon(resource.kind) } }
                                            div { class: "resource-row-heading", h2 { "{display_name}" span { "{kind_label(resource.kind)}" } } p { "v{choice.version()}" } }
                                            if let Some(description) = resource.facts["description"].as_str() { p { class: "resource-row-description", "{description}" } }
                                            div { class: "resource-row-meta", span { "{origin}" } }
                                            div { class: "resource-row-actions", span { "已安装" }
                                                button { class: "button", onclick: move |_| { detail.set(Some(open_resource.clone())); receipt.set(String::new()); manage_error.set(String::new()); }, "查看详情" }
                                                button { class: "button", disabled: !online, onclick: move |_| installing.set(vec![install_choice.clone()]), "安装到…" }
                                                button { class: if is_selected { "button selected" } else { "button primary" }, aria_pressed: is_selected, onclick: move |_| choose(choice.clone()), if is_selected { "已选" } else { "选择" } }
                                            }
                                        }
                                    } }
                                }
                            }
                        }
                        div { class: "resource-center-footer",
                            p { if view() == "public" { "目录数量与工作项目独立。安装时选择节点，再选择全局或指定项目。" } else { "每行标明安装来源。目录中的安装状态未知项可在“节点管理”中核对。" } }
                            button { class: "resource-text-button", disabled: work_project().is_empty(), onclick: move |_| management_open.set(true), "节点管理" }
                            button { class: "resource-text-button", disabled: session.is_none(), onclick: move |_| onconfigure.call(()), "会话资源设置" }
                        }
                    }
                }
                aside { class: "resource-selection-panel", aria_label: "已选工具", {selection.clone()} }
            }
            if !selected.is_empty() {
                div { class: "resource-selection-dock",
                    button { class: "button", onclick: move |_| selection_open.set(true), "已选 {selected.len()} 项" }
                    button { class: "button primary", disabled: !has_package || !online, onclick: move |_| working.set(Some(false)), "开始工作" span { class: "resource-forward-icon", Icon { name: "back" } } }
                }
            }
            if selection_open() {
                Modal { id: "selected-tools", title: "已选工具", class: "resource-flow-dialog", trap_focus: true, onclose: move |()| selection_open.set(false),
                    div { class: "resource-flow-body", {selection} } }
            }
            if let Some(choice) = replacement() {
                { let title = if choice.kind() == NativeKind::Package { "替换主 Package？" } else { "替换工具版本？" }; rsx! {
                Modal { id: "replace-resource", title, class: "resource-flow-dialog", trap_focus: true, onclose: move |()| replacement.set(None),
                    div { class: "resource-flow-body", p { "将使用 {choice.name()} · v{choice.version()}，其他已选工具会保留。" }
                        div { class: "resource-flow-actions",
                            button { class: "button", onclick: move |_| replacement.set(None), "取消" }
                            button { class: "button primary", onclick: move |_| {
                                choices.write().retain(|entry| entry.kind() != choice.kind() || (choice.kind() != NativeKind::Package && entry.native_name() != choice.native_name()));
                                choices.write().push(choice.clone()); replacement.set(None);
                            }, "确认替换" }
                        }
                    }
                }
                } }
            }
            if let Some(resource) = detail() {
                super::detail::ResourceDetail { reference: resource.reference, name: resource.name, kind: resource.kind, scope: resource.scope, facts: resource.facts,
                    busy: managing(), loading: loading(), online, error: manage_error(), notice: receipt(), onmanage: move |request| manage(request),
                    onclose: move |()| detail.set(None) }
            }
            if let Some(offer) = offer_detail() {
                OfferDetail { key: "{offer.manifest.id}-{offer.manifest.version}", choice: ResourceChoice::Public(offer), project: work_project(), shared_client, connection_epoch, session_generation,
                    oninstall: move |choice| { offer_detail.set(None); installing.set(vec![choice]); }, onchanged: move |()| revision += 1, onclose: move |()| offer_detail.set(None) }
            }
            if !installing().is_empty() {
                InstallDialog { choices: installing(), project: if view() == "installed" && !global { browse_scope() } else { work_project() }, projects: projects.clone(),
                    shared_client, connection_epoch, session_generation, online,
                    oncomplete: move |(node, project, scope)| {
                        installing.set(vec![]);
                        if node == connection.address.peek().trim_end_matches('/') {
                            browse_scope.set(if scope == NativeScope::Global { "global".into() } else { project });
                            view.set("installed".into());
                        }
                        receipt.set(format!("安装已确认 · {node}"));
                        revision += 1;
                    },
                    onclose: move |()| { installing.set(vec![]); revision += 1; } }
            }
            if let Some(defaults) = working() {
                WorkDialog { choices: selected, project: work_project(), projects: projects.clone(), node_label,
                    session: session.clone(), session_project: session_project(), shared_client, connection_epoch, session_generation, online, idle, defaults,
                    onopen: move |id| { working.set(None); onopen.call(id); }, onchanged: move |()| revision += 1, onclose: move |()| working.set(None) }
            }
            if management_open() {
                Modal { id: "resource-management", title: "节点资源管理", class: "resource-flow-dialog", trap_focus: true, onclose: move |()| { management_open.set(false); revision += 1; },
                    div { class: "resource-flow-body",
                        super::native::NativeResources { project: work_project(), session: session.clone(), shared_client, connection_epoch, session_generation, idle, online, onopen }
                    }
                }
            }
            if authoring_open() {
                Modal { id: "resource-authoring", title: "资源开发", class: "resource-flow-dialog resource-authoring-dialog", trap_focus: true,
                    onclose: move |()| { authoring_open.set(false); revision += 1; },
                    div { class: "resource-flow-body",
                        crate::features::meta_agent::node::NodeAuthoring { project: work_project(), shared_client, connection_epoch, legacy,
                            onopen: move |id| { authoring_open.set(false); onopen.call(id); } }
                    }
                }
            }
        }
    }
}

#[component]
fn OfferDetail(
    choice: ResourceChoice,
    project: String,
    shared_client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    session_generation: Signal<u64>,
    oninstall: EventHandler<ResourceChoice>,
    onchanged: EventHandler<()>,
    onclose: EventHandler<()>,
) -> Element {
    let ResourceChoice::Public(offer) = choice else {
        return rsx! {};
    };
    let mut bundle = use_signal(|| None::<PackageBundle>);
    let mut error = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut acquiring = use_signal(|| false);
    let mut acquired = use_signal(|| offer.acquired);
    let mut notice = use_signal(String::new);
    let acquire_offer = offer.clone();
    let acquire_project = project.clone();
    let mut acquire = move |_| {
        if acquiring() || loading() || acquired() {
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        let target = super::packages::reference(&acquire_offer);
        let project = acquire_project.clone();
        let epoch = *connection_epoch.peek();
        let generation = *session_generation.peek();
        acquiring.set(true);
        error.set(String::new());
        notice.set(String::new());
        spawn(async move {
            let result = async {
                let receipt = NodeApi::package_acquire(&client, &project, target.clone())
                    .await
                    .map_err(|value| value.to_string())?;
                if receipt.package != target {
                    return Err("获取回执的版本或摘要不匹配，请回读核对。".to_owned());
                }
                let catalog = NodeApi::package_catalog(&client, &project)
                    .await
                    .map_err(|value| value.to_string())?;
                if !super::packages::acquired(Some(&catalog), &target) {
                    return Err("节点目录尚未确认获取结果，请重新读取。".into());
                }
                Ok::<_, String>(())
            }
            .await;
            if *connection_epoch.peek() != epoch || *session_generation.peek() != generation {
                return;
            }
            acquiring.set(false);
            match result {
                Ok(()) => {
                    acquired.set(true);
                    notice.set("节点获取已由目录确认，可以继续选择安装范围。".into());
                    onchanged.call(());
                }
                Err(message) => error.set(message),
            }
        });
    };
    let read_offer = offer.clone();
    use_effect(move || {
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        let reference = super::packages::reference(&read_offer);
        let project = project.clone();
        let epoch = *connection_epoch.peek();
        let generation = *session_generation.peek();
        spawn(async move {
            let result = NodeApi::package_bundle(&client, &project, reference.clone()).await;
            if *connection_epoch.peek() != epoch || *session_generation.peek() != generation {
                return;
            }
            loading.set(false);
            match result {
                Ok(value)
                    if value.manifest.id == reference.id
                        && value.manifest.version == reference.version
                        && value.sha256 == reference.sha256 =>
                {
                    bundle.set(Some(value))
                }
                Ok(_) => error.set("节点返回的版本或摘要不匹配，请重新读取。".into()),
                Err(value) => error.set(value.to_string()),
            }
        });
    });
    rsx! {
        Modal { id: "resource-offer", title: "资源详情", class: "resource-flow-dialog", trap_focus: true, onclose,
            div { class: "resource-flow-body",
                h3 { "{offer.manifest.title}" } p { "{offer.manifest.description}" }
                dl { class: "native-detail-facts",
                    dt { "类型" } dd { "{kind_label(offer.manifest.kind)}" }
                    dt { "版本" } dd { "v{offer.manifest.version}" }
                    dt { "来源" } dd { "{offer.manifest.author} · {offer.manifest.origin}" }
                    dt { "资源 ID" } dd { "{offer.manifest.id}" }
                    dt { "SHA-256" } dd { "{offer.sha256}" }
                    dt { "内容" } dd { "{offer.file_count} 个文件 · {offer.size_bytes} 字节" }
                }
                if loading() { p { role: "status", "正在读取内容…" } }
                if !error().is_empty() { p { class: "resource-error", role: "alert", "{error}" } }
                if !notice().is_empty() { p { role: "status", "{notice}" } }
                if let Some(value) = bundle() {
                    details { summary { "查看资源内容" }
                        if !value.manifest.instructions.is_empty() { p { "Package 指令路径 · {value.manifest.instructions.join(\" · \")}" } }
                        for (path, content) in value.sources { details { summary { "{path}" } pre { "{content}" } } }
                    }
                }
                div { class: "resource-flow-actions",
                    button { class: "button", disabled: bundle().is_none(), onclick: move |_| {
                        if let Some(bundle) = bundle() { spawn(async move { if let Err(message) = super::packages::download_bundle(&bundle).await { error.set(message); } }); }
                    }, "下载到本机" }
                    button { class: "button", disabled: loading() || acquiring() || acquired(), onclick: move |event| acquire(event), if acquired() { "节点已获取" } else if acquiring() { "正在获取…" } else { "获取到当前节点" } }
                    button { class: "button primary", disabled: acquiring(), onclick: move |_| oninstall.call(ResourceChoice::Public(offer.clone())), "安装" }
                }
            }
        }
    }
}
