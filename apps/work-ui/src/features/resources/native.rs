//! Formal native resource controls. All authority and observations come from `NodeApi`.
use dioxus::prelude::*;
use std::collections::BTreeMap;
use work_client::WorkspaceClient;
use work_core::api::{
    NativeApply, NativeCatalog, NativeConfiguration, NativeCreate, NativeDesired, NativeKind,
    NativeManagement, NativeOperationStatus, NativeReadiness, NativeReference, NativeResource,
    NativeScope, NativeSelection, NativeSessionResources, NativeTools, NodeApi,
};

pub(super) fn recorded_version(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}
fn operation_id() -> String {
    format!(
        "work-{}",
        web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}
fn selection(desired: &NativeDesired) -> Option<NativeSelection> {
    Some(NativeSelection {
        main_package: desired.main_package.as_ref()?.reference.clone()?,
        package_instructions: desired.package_instructions.clone(),
        skills: desired
            .skills
            .iter()
            .map(|r| r.reference.clone())
            .collect::<Option<_>>()?,
        extensions: desired
            .extensions
            .iter()
            .map(|r| r.reference.clone())
            .collect::<Option<_>>()?,
        tools: desired.tools.clone(),
        supplementary_instructions: desired.supplementary_instructions.clone()?,
    })
}

fn project_observation(value: work_core::api::NativeProjectResources) -> NativeSessionResources {
    let configuration = value.selection.map(|selection| NativeConfiguration {
        revision: value.revision,
        desired: NativeDesired {
            main_package: Some(NativeReference {
                reference: Some(selection.main_package.clone()),
                identity: selection.main_package,
                diagnostic: None,
            }),
            package_instructions: selection.package_instructions,
            skills: selection
                .skills
                .into_iter()
                .map(|reference| NativeReference {
                    identity: reference.clone(),
                    reference: Some(reference),
                    diagnostic: None,
                })
                .collect(),
            extensions: selection
                .extensions
                .into_iter()
                .map(|reference| NativeReference {
                    identity: reference.clone(),
                    reference: Some(reference),
                    diagnostic: None,
                })
                .collect(),
            tools: selection.tools,
            supplementary_instructions: Some(selection.supplementary_instructions),
        },
        historical_observation: None,
    });
    let configured = configuration.is_some();
    NativeSessionResources {
        configuration,
        operations: vec![],
        readiness: if configured {
            NativeReadiness::Verified
        } else {
            NativeReadiness::Legacy
        },
        executable: None,
        actual: None,
        generation: None,
        diagnostic: value.diagnostic,
    }
}

fn recovery_admission(state: &NativeSessionResources) -> (bool, bool) {
    // This is permission to REQUEST explicit recovery, never proof of idle or
    // executability. Only the current typed owner operation grants this lane.
    let latest = state.operations.last();
    let retry = matches!(
        state.readiness,
        NativeReadiness::Blocked | NativeReadiness::Applying
    ) && latest.is_some_and(|op| {
        matches!(
            op.status,
            NativeOperationStatus::Blocked | NativeOperationStatus::Pending
        )
    });
    let correct = state.readiness == NativeReadiness::Blocked
        && latest.is_some_and(|op| op.status == NativeOperationStatus::Blocked);
    (retry, correct)
}
fn display_resource_name(name: &str) -> String {
    let Some(marker) = name.rfind("-v") else {
        return name.to_owned();
    };
    let suffix = &name[marker + 2..];
    let Some((version, digest)) = suffix.split_once('-') else {
        return name.to_owned();
    };
    let valid_version = version.matches('.').count() >= 1
        && !version.is_empty()
        && version.chars().all(|ch| ch.is_ascii_digit() || ch == '.');
    let valid_digest = digest.len() >= 8 && digest.chars().all(|ch| ch.is_ascii_hexdigit());
    if valid_version && valid_digest {
        name[..marker].to_owned()
    } else {
        name.to_owned()
    }
}

fn resource_matches(resource: &NativeResource, kind: &str, query: &str, scope: &str) -> bool {
    let category = match resource.kind {
        NativeKind::Package => "package",
        NativeKind::Skill => "skill",
        NativeKind::Extension => "extension",
    };
    (kind.is_empty() || category == kind)
        && (scope == "all"
            || scope
                == match resource.scope {
                    NativeScope::Project => "project",
                    NativeScope::Global => "global",
                })
        && format!(
            "{} {} {} {}",
            resource.name,
            resource.reference,
            resource.facts["description"].as_str().unwrap_or(""),
            resource.facts["source"].as_str().unwrap_or("")
        )
        .to_lowercase()
        .contains(&query.trim().to_lowercase())
}

fn extension_identity(resource: &NativeResource) -> String {
    format!(
        "{:?}:{}",
        resource.kind,
        display_resource_name(&resource.name)
    )
}

fn resource_preference(resource: &NativeResource) -> (u8, u8) {
    let installation = match resource.facts["installed"].as_bool() {
        Some(true) => 2,
        Some(false) => 0,
        None => 1,
    };
    let scope = u8::from(matches!(resource.scope, NativeScope::Project));
    (installation, scope)
}

fn deduplicate_extensions(resources: Vec<NativeResource>) -> Vec<NativeResource> {
    let mut unique = BTreeMap::<String, NativeResource>::new();
    for resource in resources {
        let key = extension_identity(&resource);
        let replace = unique
            .get(&key)
            .is_none_or(|current| resource_preference(&resource) > resource_preference(current));
        if replace {
            unique.insert(key, resource);
        }
    }
    unique.into_values().collect()
}

fn readiness_key(readiness: NativeReadiness) -> &'static str {
    match readiness {
        NativeReadiness::Legacy => "legacy",
        NativeReadiness::Verified => "verified",
        NativeReadiness::Closed => "closed",
        NativeReadiness::Applying => "applying",
        NativeReadiness::Blocked => "blocked",
        NativeReadiness::Unknown => "unknown",
    }
}

fn readiness_label(readiness: NativeReadiness) -> &'static str {
    match readiness {
        NativeReadiness::Legacy => "旧会话（未选择精确资源）",
        NativeReadiness::Verified => "已验证",
        NativeReadiness::Closed => "已关闭",
        NativeReadiness::Applying => "正在应用",
        NativeReadiness::Blocked => "需要处理",
        NativeReadiness::Unknown => "未核实",
    }
}

#[component]
pub fn NativeResources(
    project: String,
    session: Option<String>,
    shared_client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    session_generation: Signal<u64>,
    #[props(default = false)] idle: bool,
    #[props(default = false)] online: bool,
    onopen: EventHandler<String>,
    #[props(default)] kind: String,
    #[props(default)] configuration_only: bool,
    #[props(default)] revision: u64,
) -> Element {
    let mut catalog = use_signal(|| None::<NativeCatalog>);
    let mut observed = use_signal(|| None::<NativeSessionResources>);
    let mut editor = use_signal(|| None::<NativeSelection>);
    let mut dirty = use_signal(|| false);
    let mut refresh = use_signal(|| 0u64);
    let mut ticket = use_signal(|| 0u64);
    let mut busy = use_signal(|| false);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let mut catalog_error = use_signal(String::new);
    let mut session_error = use_signal(String::new);
    let mut receipt_resource = use_signal(|| None::<String>);
    let mut query = use_signal(String::new);
    let mut scope_filter = use_signal(|| "all".to_owned());
    let mut detail = use_signal(|| None::<NativeResource>);
    let mut last_request = use_signal(|| None::<NativeApply>);
    let mut scope_value = use_signal(|| "project".to_owned());
    let mut kind_value = use_signal(|| {
        if kind.is_empty() {
            "package".to_owned()
        } else {
            kind.clone()
        }
    });
    let mut source = use_signal(String::new);
    let mut name = use_signal(String::new);
    let mut version = use_signal(String::new);
    let mut trusted = use_signal(|| false);
    let project_load = project.clone();
    let session_load = session.clone();
    use_effect(use_reactive(
        (&project_load, &session_load, &online, &revision),
        move |(project, session, online, _)| {
            let _ = refresh.read();
            let epoch = *connection_epoch.read();
            let generation = *session_generation.read();
            let next = ticket.peek().wrapping_add(1);
            ticket.set(next);
            catalog_error.set(String::new());
            session_error.set(String::new());
            if !online {
                loading.set(false);
                catalog.set(None);
                observed.set(None);
                return;
            }
            let Some(client) = shared_client.read().clone() else {
                catalog_error.set("节点连接尚未就绪，请重新连接后重试。".into());
                loading.set(false);
                catalog.set(None);
                observed.set(None);
                return;
            };
            loading.set(true);
            spawn(async move {
                let result = NodeApi::native_catalog(&client, &project).await;
                let state = if let Some(id) = &session {
                    Some(NodeApi::native_session_resources(&client, id).await)
                } else {
                    None
                };
                let project_state = if session.is_none() {
                    Some(NodeApi::native_project_resources(&client, &project).await)
                } else {
                    None
                };
                if *connection_epoch.peek() != epoch
                    || *session_generation.peek() != generation
                    || *ticket.peek() != next
                {
                    return;
                }
                loading.set(false);
                match result {
                    Ok(value) => {
                        catalog.set(Some(value));
                    }
                    Err(e) => {
                        catalog.set(None);
                        catalog_error.set(e.to_string());
                    }
                }
                if let Some(result) = state {
                    match result {
                        Ok(value) => {
                            if !*dirty.peek() {
                                editor.set(if value.readiness == NativeReadiness::Blocked {
                                    value.operations.last().and_then(|op| selection(&op.target))
                                } else {
                                    value
                                        .configuration
                                        .as_ref()
                                        .and_then(|c| selection(&c.desired))
                                });
                            }
                            if let Some(op) = value.operations.last() {
                                let submitted =
                                    last_request.peek().as_ref().map(|r| r.operation_id.clone());
                                if matches!(
                                    op.status,
                                    NativeOperationStatus::Pending | NativeOperationStatus::Blocked
                                ) || submitted.is_none()
                                    || value
                                        .operations
                                        .iter()
                                        .any(|o| Some(&o.operation_id) == submitted.as_ref())
                                {
                                    last_request.set(op.retry.clone());
                                }
                            }
                            observed.set(Some(value));
                        }
                        Err(e) => {
                            observed.set(None);
                            session_error.set(e.to_string());
                        }
                    }
                } else if let Some(result) = project_state {
                    match result {
                        Ok(value) => {
                            if !*dirty.peek() {
                                editor.set(value.selection.clone());
                            }
                            observed.set(Some(project_observation(value)));
                        }
                        Err(e) => {
                            observed.set(None);
                            session_error.set(e.to_string());
                        }
                    }
                }
            });
        },
    ));
    let s = if scope_value() == "global" {
        NativeScope::Global
    } else {
        NativeScope::Project
    };
    let k = match kind_value().as_str() {
        "skill" => NativeKind::Skill,
        "extension" => NativeKind::Extension,
        _ => NativeKind::Package,
    };
    let project_manage = use_signal(|| project.clone());
    let mut manage = move |request: NativeManagement| {
        if *busy.peek() || *loading.peek() || !online {
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        let project = project_manage.peek().clone();
        let epoch = *connection_epoch.peek();
        let generation = *session_generation.peek();
        receipt_resource.set(match &request {
            NativeManagement::Inspect { reference }
            | NativeManagement::Update { reference, .. }
            | NativeManagement::Reinstall { reference, .. }
            | NativeManagement::Remove { reference } => Some(reference.clone()),
            _ => None,
        });
        busy.set(true);
        error.set(String::new());
        notice.set("正在调用节点原生资源管理器…".into());
        spawn(async move {
            let result = NodeApi::native_manage(&client, &project, request).await;
            if *connection_epoch.peek() != epoch || *session_generation.peek() != generation {
                return;
            }
            busy.set(false);
            match result {
                Ok(receipt) => {
                    if !receipt.success {
                        error.set(format!("原生管理拒绝：{}", receipt.observation));
                    }
                    notice.set(format!("{}\n{}", receipt.observation, receipt.diagnostics));
                    refresh += 1;
                }
                Err(e) => {
                    error.set(e.to_string());
                    notice.set("结果未验证；请回读后重试，不会自动放宽配置。".into());
                }
            }
        });
    };
    let session_apply = use_signal(|| session.clone());
    let project_apply = project.clone();
    let apply = Callback::new(move |(retry, supersede): (bool, bool)| {
        let recovery = observed
            .peek()
            .as_ref()
            .map(recovery_admission)
            .unwrap_or_default();
        if *busy.peek()
            || *loading.peek()
            || !online
            || !(idle || retry && recovery.0 || supersede && recovery.1)
        {
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        let id = session_apply.peek().clone();
        let project = project_apply.clone();
        let request = if retry {
            last_request.peek().clone()
        } else {
            editor.peek().clone().map(|target| NativeApply {
                expected_revision: observed
                    .peek()
                    .as_ref()
                    .and_then(|o| o.configuration.as_ref())
                    .map_or(0, |c| c.revision),
                operation_id: operation_id(),
                target,
                supersedes_operation_id: if supersede {
                    observed
                        .peek()
                        .as_ref()
                        .and_then(|o| o.operations.last())
                        .map(|op| op.operation_id.clone())
                } else {
                    None
                },
            })
        };
        let Some(request) = request else {
            error.set("请选择完整主 Package；unresolved 项需显式修正。".into());
            return;
        };
        last_request.set(Some(request.clone()));
        busy.set(true);
        error.set(String::new());
        notice.set("正在应用，验证成功前不会恢复输入。".into());
        let epoch = *connection_epoch.peek();
        let generation = *session_generation.peek();
        spawn(async move {
            let result = if let Some(id) = id.as_deref() {
                NodeApi::native_apply(&client, id, request).await
            } else {
                NodeApi::native_apply_project(&client, &project, request).await
            };
            if *connection_epoch.peek() != epoch || *session_generation.peek() != generation {
                return;
            }
            busy.set(false);
            match result {
                Ok(op) => {
                    notice.set(format!(
                        "{:?} · operation {} · revision {:?} · {}",
                        op.status,
                        op.operation_id,
                        op.result_revision,
                        op.error.as_deref().unwrap_or("")
                    ));
                    if op.status == NativeOperationStatus::Applied {
                        dirty.set(false);
                        if let Some(id) = id {
                            onopen.call(id);
                        }
                    }
                    refresh += 1;
                }
                Err(e) => {
                    error.set(e.to_string());
                    notice.set("Conflict / blocked / unknown：保留编辑和原操作 ID；请回读。原生已提交而应用待恢复时只能重试原 ID，不可 supersede。".into());
                    refresh += 1;
                }
            }
        });
    });
    let project_release = use_signal(|| project.clone());
    let release = move |_| {
        if *busy.peek() || *loading.peek() || !online {
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        let project = project_release.peek().clone();
        let epoch = *connection_epoch.peek();
        busy.set(true);
        spawn(async move {
            let result = NodeApi::native_release(&client, &project, s).await;
            if *connection_epoch.peek() != epoch {
                return;
            }
            busy.set(false);
            match result {
                Ok(report) => {
                    notice.set(format!("已释放 {} 个空闲会话，{} 个节点所有者未能释放。历史与配置保留；请核对结果后重试安装。",report.released.len(),report.blocked.len()));
                    error.set(
                        report
                            .blocked
                            .iter()
                            .map(|b| format!("{}: {}", b.project_id, b.diagnostic))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
                    refresh += 1;
                }
                Err(e) => error.set(e.to_string()),
            }
        });
    };
    let project_create = use_signal(|| project.clone());
    let mut create_session = move |developer: bool| {
        if *busy.peek() || *loading.peek() || !online {
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        let project = project_create.peek().clone();
        let epoch = *connection_epoch.peek();
        busy.set(true);
        spawn(async move {
            let result = NodeApi::native_create(
                &client,
                &project,
                NativeCreate {
                    developer,
                    selection: None,
                },
            )
            .await;
            if *connection_epoch.peek() != epoch {
                return;
            }
            busy.set(false);
            match result {
                Ok(id) => onopen.call(id),
                Err(e) => error.set(e.to_string()),
            }
        });
    };
    let current = observed.read().clone();
    let recovery = current.as_ref().map(recovery_admission).unwrap_or_default();
    let simple_resource =
        matches!(kind.as_str(), "extension" | "package" | "skill") && !configuration_only;
    let correction = online && recovery.1;
    rsx! {
        section { class: if simple_resource { "native-resources resource-simple-list" } else { "native-resources" }, aria_label: "原生执行资源", aria_busy: loading() || busy(),
            style { {include_str!("native.css")} }
            if configuration_only {
                h2 { if session.is_some() { "会话执行资源" } else { "项目默认资源组合" } }
            }
            if !configuration_only {
                div { class: "native-list-heading",
                    h2 { if simple_resource { "资源列表" } else { "节点资源" } }
                    if !simple_resource {
                        button { class: "button", disabled: busy() || loading() || !online, onclick: move |_| { error.set(String::new()); refresh += 1; }, "刷新目录 / 回读" }
                    }
                }
                if !simple_resource {
                    div { class: "native-resource-filters",
                        label { "搜索节点资源" input { r#type: "search", aria_label: "搜索节点资源", value: "{query}", placeholder: "名称、用途或来源…", oninput: move |e| query.set(e.value()) } }
                        label { "资源范围" select { aria_label: "资源范围", value: "{scope_filter}", onchange: move |e| scope_filter.set(e.value()),
                            option { value: "all", "全部范围" } option { value: "project", "当前项目" } option { value: "global", "节点全局" }
                        } }
                    }
                }
                if !online { p { class: "resource-empty resource-state-block", role: "status", "执行节点离线。重连后重新读取资源，不从本地设计推断安装状态。" } }
                else if loading() { p { class: "resource-loading resource-state-block", role: "status", "正在读取节点目录…" } }
                if !catalog_error().is_empty() { p { role: "alert", class: "resource-error resource-state-block", "目录读取失败：{catalog_error}。请重试刷新目录。" } }
                if !session_error().is_empty() { p { class: "native-list-count resource-state-block", role: "status", "会话选择未核实：{session_error}。可在会话资源设置中重试。" } }
                if let Some(catalog) = catalog.read().as_ref() {
                    { let mut resources = catalog.resources.iter().filter(|r| resource_matches(r, &kind, &query(), &scope_filter())).cloned().collect::<Vec<_>>(); if kind == "extension" { resources = deduplicate_extensions(resources); } rsx! {
                        if !simple_resource {
                            p { class: "native-list-count", "{resources.len()} 项资源 · 安装、会话选择与实际加载分别核实" }
                        }
                        if resources.is_empty() && !loading() {
                            p { class: "resource-empty", role: "status",
                                if simple_resource {
                                    match kind.as_str() {
                                        "extension" => "当前项目暂无扩展。",
                                        "package" => "当前项目暂无 Package。",
                                        _ => "当前项目暂无技能。",
                                    }
                                }
                                else if query().trim().is_empty() && scope_filter() == "all" { "此分类暂无节点资源。可前往发现资源，或展开下方安装入口。" }
                                else { "没有匹配的节点资源，请调整搜索或范围。" }
                            }
                        }
                        div { class: "native-resource-list",
                            for resource in resources {
                                { let open_resource = resource.clone(); let display_name = display_resource_name(&resource.name); let selected = current.as_ref().and_then(|s| s.configuration.as_ref()).map(|c| c.desired.main_package.as_ref().is_some_and(|r| r.reference.as_deref() == Some(resource.reference.as_str())) || c.desired.skills.iter().chain(&c.desired.extensions).any(|r| r.reference.as_deref() == Some(resource.reference.as_str()))); let installation = super::detail::installation_label(&resource.facts); let selection_text = match selected { Some(true) => "本会话配置已选择", Some(false) => "本会话配置未选择", None => "会话选择未核实" }; let install_class = match resource.facts["installed"].as_bool() { Some(true) => "installed", Some(false) => "not-installed", None => "unknown" }; let selection_class = match selected { Some(true) => "selected", Some(false) => "not-selected", None => "unknown" }; rsx! {
                                    article { key: "{resource.reference}", class: "native-resource-card",
                                        div { class: "native-resource-heading",
                                            h3 { "{display_name}" }
                                            span { class: "native-install-state {install_class}", role: "status", aria_label: "安装状态：{installation}", "{installation}" }
                                        }
                                        if !simple_resource {
                                            if let Some(description) = resource.facts["description"].as_str() { p { class: "native-resource-description", "{description}" } }
                                            p { class: "native-resource-meta",
                                                {super::detail::scope_label(resource.scope)}
                                                if let Some(version) = resource.facts["version"].as_str() { " · 记录版本 {version}" }
                                            }
                                            div { class: "native-resource-footer",
                                                span { class: "native-selection-state {selection_class}", role: "status", aria_label: "{selection_text}", "{selection_text}" }
                                                span { class: "native-load-state", role: "status", aria_label: "实际加载状态：未核实", title: "节点资源目录不提供实际加载证据；请在会话资源状态中核实", "实际加载：未核实" }
                                                button { class: "button", aria_haspopup: "dialog", onclick: move |_| { error.set(String::new()); notice.set(String::new()); detail.set(Some(open_resource.clone())); }, if resource.facts["installed"] == false { "下载安装" } else { "查看详情" } }
                                            }
                                        }
                                    }
                                } }
                            }
                        }
                        details { summary { "节点能力与目录诊断" } pre { "{catalog.profile}" } for diagnostic in &catalog.diagnostics { p { "{diagnostic}" } } }
                    } }
                }
            details { summary { "安装与高级管理" }
                p { "安装到节点不会自动选择或加载到会话。更新、重装和移除请使用对应资源的详情。" }
                label { "Scope" select { aria_label: "原生资源范围", value: "{scope_value}", onchange: move |e| { scope_value.set(e.value()); trusted.set(false); }, option { value: "project", "Project" } option { value: "global", "Global" } } }
                label { "Kind" select { aria_label: "原生资源类型", value: "{kind_value}", onchange: move |e| { kind_value.set(e.value()); trusted.set(false); }, option { value: "package", "Package" } option { value: "skill", "Skill" } option { value: "extension", "Extension" } } }
                label { "安装名称" input { aria_label: "原生安装名称", value: "{name}", oninput: move |e| { name.set(e.value()); trusted.set(false); } } }
                label { "节点来源（授权项目内路径）" input { aria_label: "节点资源来源", value: "{source}", oninput: move |e| { source.set(e.value()); trusted.set(false); } } }
                label { "记录版本" input { aria_label: "原生资源版本", value: "{version}", oninput: move |e| { version.set(e.value()); trusted.set(false); } } }
                label { input { r#type: "checkbox", checked: trusted(), onchange: move |e| trusted.set(e.checked()) } "我信任此节点来源并授权安装；不会编译源码，选择原生库可能执行代码。" }
                button { class: "button primary", disabled: busy() || loading() || !online || !trusted() || source().trim().is_empty(), onclick: move |_| manage(NativeManagement::Install { scope: s, kind: k, name: name(), source: source(), version: recorded_version(version()), trusted: trusted() }), "安装节点来源" }
                button { class: "button", disabled: busy() || loading(), onclick: move |_| manage(NativeManagement::Recover { scope: s }), "恢复原生安装事务" }
                button { class: "button", disabled: busy() || loading(), onclick: release, "释放此范围空闲会话（保留历史与配置）" }
                p { "正在执行、等待确认或仍有排队工作时不会释放。可能部分成功；释放后请手动重试安装。" }
            }
                div { class: "native-actions",
                    if kind.is_empty() || kind == "package" {
                        button { class: "button", disabled: busy() || loading() || !online, onclick: move |_| create_session(false), "新建基础 Package 会话" }
                        button { class: "button", disabled: busy() || loading() || !online, onclick: move |_| create_session(true), "新建开发 Package 会话" }
                    }
                }
            }
            if configuration_only {
                if loading() { p { role: "status", "正在读取资源组合…" } }
                if !online { p { role: "status", "执行节点离线，无法核实会话资源。" } }
                if !catalog_error().is_empty() { p { role: "alert", class: "resource-error", "目录读取失败：{catalog_error}" } }
            if !session_error().is_empty() { p { role: "alert", class: "resource-error", "会话资源读取失败：{session_error}" } }
            if let Some(state) = current {
                div { class: "native-readiness-status", "data-readiness": readiness_key(state.readiness), role: "status", aria_atomic: "true",
                    h3 { if session.is_some() { "会话资源：{readiness_label(state.readiness)}" } else { "项目默认资源组合" } }
                    p { match state.executable { Some(true) => "节点已验证可执行", Some(false) => "节点未允许执行", None => "可执行状态未核实" } }
                }
                details { summary { "会话状态诊断" } p { "host generation: {state.generation:?} · {state.diagnostic:?}" } }
                if let Some(config) = &state.configuration { p { class: "meta", "会话资源版本 r{config.revision}" } details { summary { "已保存 desired / unresolved / 历史 observation（不授权执行）" } pre { "{serde_json::to_string_pretty(config).unwrap_or_default()}" } } }
                if let Some(actual) = &state.actual { details { summary { "实际加载、有效工具与指令来源" } pre { "{actual}" } } }
                if state.readiness == NativeReadiness::Legacy { p { "旧会话保持 legacy。明确选择并应用才 opt-in；失败不会伪造已应用配置。" } }
                if editor.read().is_none() {
                    button { class: "button", disabled: busy() || loading() || !online || (!idle && !correction), onclick: move |_| {
                        if let Some(r) = catalog.peek().as_ref().and_then(|c| c.resources.iter().find(|r| r.kind == NativeKind::Package && r.facts["ownership"] == "resource")) {
                            editor.set(Some(NativeSelection { main_package: r.reference.clone(), package_instructions: vec![], skills: vec![], extensions: vec![], tools: NativeTools::Default {}, supplementary_instructions: String::new() })); dirty.set(true);
                        }
                    }, "明确启用 / 修正精确资源选择" }
                }
                if let Some(value) = editor.read().clone() {
                    fieldset { disabled: busy() || loading() || !online || (!idle && !correction),
                        legend { if session.is_some() { "会话资源选择" } else { "项目默认资源选择" } }
                        label { "主 Package" select { aria_label: "会话主 Package", value: "{value.main_package}", onchange: move |e| { if let Some(v) = editor.write().as_mut() { v.main_package = e.value(); } dirty.set(true); },
                            if let Some(c) = catalog.read().as_ref() { for r in c.resources.iter().filter(|r| r.kind == NativeKind::Package) { option { value: "{r.reference}", "{r.name} · {r.scope:?}" } } }
                        } }
                        label { "Package 持续指令路径（每行一项）" textarea { aria_label: "Package 指令映射", value: value.package_instructions.join("\n"), oninput: move |e| { if let Some(v) = editor.write().as_mut() { v.package_instructions = e.value().lines().filter(|s| !s.is_empty()).map(str::to_owned).collect(); } dirty.set(true); } } }
                        if let Some(c) = catalog.read().as_ref() { for r in c.resources.iter().filter(|r| r.kind != NativeKind::Package) {
                            { let reference = r.reference.clone(); let is_skill = r.kind == NativeKind::Skill; let checked = if is_skill { value.skills.contains(&reference) } else { value.extensions.contains(&reference) }; rsx! {
                                label { input { r#type: "checkbox", checked, onchange: move |e| { if let Some(v) = editor.write().as_mut() { let items = if is_skill { &mut v.skills } else { &mut v.extensions }; items.retain(|r| r != &reference); if e.checked() { items.push(reference.clone()); } } dirty.set(true); } } "{r.kind:?}: {r.name} · {r.scope:?}" }
                            } }
                        } }
                        label { "工具策略" select { aria_label: "会话工具策略", value: match value.tools { NativeTools::Default {} => "default", NativeTools::Only { .. } => "only", NativeTools::None {} => "none" }, onchange: move |e| { if let Some(v) = editor.write().as_mut() { v.tools = match e.value().as_str() { "none" => NativeTools::None {}, "only" => NativeTools::Only { names: vec![] }, _ => NativeTools::Default {} }; } dirty.set(true); }, option { value: "default", "default" } option { value: "only", "only" } option { value: "none", "none（零模型工具）" } } }
                        if let NativeTools::Only { names } = &value.tools { label { "Only tools（逗号分隔）" input { aria_label: "精确工具名称", value: names.join(","), oninput: move |e| { if let Some(v) = editor.write().as_mut() { v.tools = NativeTools::Only { names: e.value().split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect() }; } dirty.set(true); } } } }
                        label { "补充指令（≤64 KiB）" textarea { aria_label: "会话补充指令", value: "{value.supplementary_instructions}", oninput: move |e| { if let Some(v) = editor.write().as_mut() { v.supplementary_instructions = e.value(); } dirty.set(true); } } }
                    }
                }
                div { class: "native-actions",
                    button { class: "button primary", disabled: busy() || loading() || !online || !idle || editor.read().is_none(), onclick: move |_| apply.call((false, false)), if session.is_some() { "应用会话资源" } else { "保存项目默认组合" } }
                    button { class: "button", disabled: busy() || loading() || !online || last_request.read().is_none() || (!idle && !recovery.0), onclick: move |_| apply.call((true, false)), "重试原操作" }
                    if state.operations.last().is_some_and(|op| op.status == NativeOperationStatus::Blocked) { button { class: "button", disabled: busy() || loading() || !online || (!idle && !correction) || editor.read().is_none(), onclick: move |_| apply.call((false, true)), "明确修正被阻止的配置（需确认原操作未提交）" } }
                }
                if correction { p { class: "meta", "此会话当前只能读取历史。可重试原操作，或明确修正被阻止的配置；节点重新验证成功前不能发送消息。" } }
                else if !idle { p { class: "meta", "常规配置需会话在线、已同步且完全空闲。未完成的原操作可单独恢复，节点仍会重新验证。" } }
                for op in &state.operations { p { "{op.operation_id}: {op.status:?} · result revision {op.result_revision:?} · {op.error:?}" } }
            }
            }
            if busy() { p { role: "status", "节点操作中…" } }
            if !notice().is_empty() { pre { role: "status", "{notice}" } }
            if !error().is_empty() { p { role: "alert", class: "resource-error", "{error}" } }
            if let Some(resource) = detail.read().clone() {
                { let owns_receipt = receipt_resource.read().as_ref() == Some(&resource.reference); rsx! {
                super::detail::ResourceDetail {
                    key: "{resource.reference}", reference: resource.reference, name: resource.name, kind: resource.kind,
                    scope: resource.scope, facts: resource.facts, busy: busy(), loading: loading(), online,
                    error: if owns_receipt { error() } else { String::new() }, notice: if owns_receipt { notice() } else { String::new() }, onmanage: manage,
                    onclose: move |()| detail.set(None),
                }
                } }
            }
        }
    }
}

#[cfg(test)]
mod presentation_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn display_name_hides_version_and_digest_suffix_only() {
        assert_eq!(
            display_resource_name("rpi_acp-v0.2.13-37410c15e8a849f2aaeebce774f156d4"),
            "rpi_acp"
        );
        assert_eq!(display_resource_name("rpi_ask_user"), "rpi_ask_user");
        assert_eq!(
            display_resource_name("custom-vnext-not-a-digest"),
            "custom-vnext-not-a-digest"
        );
    }

    #[test]
    fn extension_list_deduplicates_versions_and_prefers_installed_project_entry() {
        let resources = deduplicate_extensions(vec![
            NativeResource {
                reference: "global-old".into(),
                scope: NativeScope::Global,
                kind: NativeKind::Extension,
                name: "review-v1.0.0-aaaaaaaa".into(),
                facts: json!({"installed": false}),
            },
            NativeResource {
                reference: "project-installed".into(),
                scope: NativeScope::Project,
                kind: NativeKind::Extension,
                name: "review-v1.1.0-bbbbbbbb".into(),
                facts: json!({"installed": true}),
            },
        ]);
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].reference, "project-installed");
    }

    #[test]
    fn category_scope_and_search_intersect_without_using_installation_or_lease() {
        let resource = NativeResource {
            reference: "project/extension/test".into(),
            scope: NativeScope::Project,
            kind: NativeKind::Extension,
            name: "Review 中文".into(),
            facts: json!({"source": "/node/long/source", "description": "代码审查", "inUse": true}),
        };
        assert!(resource_matches(&resource, "extension", " review ", "all"));
        assert!(resource_matches(&resource, "extension", "审查", "project"));
        assert!(resource_matches(&resource, "extension", "SOURCE", "all"));
        assert!(!resource_matches(&resource, "skill", "", "all"));
        assert!(!resource_matches(&resource, "extension", "", "global"));
        assert!(!resource_matches(&resource, "extension", "missing", "all"));
    }
}
