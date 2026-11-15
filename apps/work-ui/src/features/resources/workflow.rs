use super::catalog::{ResourceChoice, kind_label};
use crate::shared::components::Modal;
use dioxus::prelude::*;
use work_client::WorkspaceClient;
use work_core::api::{
    NativeApply, NativeCatalog, NativeCreate, NativeManagement, NativeOperationStatus,
    NativeReadiness, NativeScope, NativeSelection, NativeTools, NodeApi, PackageInstall,
};

fn installed_in(choice: &ResourceChoice, catalog: &NativeCatalog, scope: NativeScope) -> bool {
    catalog.resources.iter().any(|resource| {
        resource.scope == scope && resource.facts["installed"] == true && choice.matches(resource)
    })
}

async fn install_choice(
    client: &WorkspaceClient,
    project: &str,
    choice: &ResourceChoice,
    scope: NativeScope,
    same_node: bool,
) -> Result<(), String> {
    // Reread first on every explicit attempt, including recovery from a lost receipt.
    let catalog = NodeApi::native_catalog(client, project)
        .await
        .map_err(|e| e.to_string())?;
    if installed_in(choice, &catalog, scope) {
        return Ok(());
    }
    let receipt = match choice {
        ResourceChoice::Public(offer) => {
            let package = super::packages::reference(offer);
            let catalog = NodeApi::package_catalog(client, project)
                .await
                .map_err(|e| e.to_string())?;
            if !super::packages::acquired(Some(&catalog), &package) {
                let acquired = NodeApi::package_acquire(client, project, package.clone())
                    .await
                    .map_err(|e| e.to_string())?;
                if acquired.package != package {
                    return Err("节点获取回执的版本或摘要不匹配，请回读目录核对。".into());
                }
                let readback = NodeApi::package_catalog(client, project)
                    .await
                    .map_err(|e| e.to_string())?;
                if !super::packages::acquired(Some(&readback), &package) {
                    return Err("节点尚未确认获取到所选版本，请回读后重试。".into());
                }
            }
            NodeApi::package_install(
                client,
                project,
                PackageInstall {
                    package,
                    scope,
                    trusted: true,
                },
            )
            .await
            .map_err(|e| e.to_string())?
        }
        ResourceChoice::Installed(resource) => {
            let resource = if same_node {
                resource
            } else {
                catalog.resources.iter().find(|candidate| choice.matches(candidate))
                    .ok_or("目标节点没有此资源的匹配来源，请先在目标节点导入资源，或从公共库选择后安装。")?
            };
            let source = resource.facts["source"]
                .as_str()
                .filter(|source| !source.trim().is_empty())
                .ok_or("此资源没有可安装的节点来源，请在资源详情中核对。")?;
            NodeApi::native_manage(
                client,
                project,
                NativeManagement::Install {
                    scope,
                    kind: resource.kind,
                    name: resource.name.clone(),
                    source: source.into(),
                    version: resource.facts["version"].as_str().map(str::to_owned),
                    trusted: true,
                },
            )
            .await
            .map_err(|e| e.to_string())?
        }
    };
    if !receipt.success {
        return Err(format!(
            "安装未成功。{}\n{}",
            receipt.observation, receipt.diagnostics
        ));
    }
    let readback = NodeApi::native_catalog(client, project)
        .await
        .map_err(|e| e.to_string())?;
    if !installed_in(choice, &readback, scope) {
        return Err("收到安装回执，目录尚未确认所选范围的安装。请先回读核对。".into());
    }
    Ok(())
}

#[component]
pub(super) fn InstallDialog(
    choices: Vec<ResourceChoice>,
    project: String,
    projects: Vec<(String, String)>,
    shared_client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    session_generation: Signal<u64>,
    online: bool,
    #[props(default)] fixed_project: bool,
    oncomplete: EventHandler<(String, String, NativeScope)>,
    onclose: EventHandler<()>,
) -> Element {
    let mut target_project = use_signal(|| project);
    let target = super::install_target::use_install_target(projects.clone(), shared_client, online);
    let current_address = super::install_target::current_address();
    let same_node = target.read().address == current_address;
    let target_ready = target.read().ready();
    let navigation = use_hook(try_consume_context::<crate::app::node_navigation::NodeNavigation>);
    let handoff = navigation.is_some() && !same_node;
    let mut scope = use_signal(|| None::<NativeScope>);
    let mut trusted = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut progress = use_signal(|| Vec::<String>::new());
    let mut complete = use_signal(|| false);
    let choices_submit = choices.clone();
    let submit = Callback::new(move |read_only: bool| {
        if *busy.peek() || !target.peek().ready() || (!read_only && !*trusted.peek()) {
            return;
        }
        let (Some(scope), Some(client)) = (*scope.peek(), target.peek().client.clone()) else {
            return;
        };
        let project = target_project.peek().clone();
        if project.is_empty() {
            return;
        }
        if !target.peek().projects.iter().any(|(id, _)| id == &project) {
            error.set("请选择目标节点上的项目。".into());
            return;
        }
        let choices = choices_submit.clone();
        let epoch = *connection_epoch.peek();
        let generation = *session_generation.peek();
        busy.set(true);
        error.set(String::new());
        progress.set(vec![]);
        spawn(async move {
            let result = async {
                for choice in &choices {
                    if *connection_epoch.peek() != epoch || *session_generation.peek() != generation
                    {
                        return Err("连接或会话已变化，请重新读取资源。".into());
                    }
                    progress.write().push(format!(
                        "正在核对 {} · v{}",
                        choice.name(),
                        choice.version()
                    ));
                    if read_only {
                        let catalog = NodeApi::native_catalog(&client, &project)
                            .await
                            .map_err(|e| e.to_string())?;
                        if !installed_in(choice, &catalog, scope) {
                            return Err(format!(
                                "{} 尚未在此范围确认安装，可重新授权后继续安装。",
                                choice.name()
                            ));
                        }
                    } else {
                        install_choice(&client, &project, choice, scope, same_node).await?;
                    }
                    progress
                        .write()
                        .push(format!("{} · 安装目录已确认", choice.name()));
                }
                Ok::<_, String>(())
            }
            .await;
            if *connection_epoch.peek() != epoch || *session_generation.peek() != generation {
                return;
            }
            busy.set(false);
            trusted.set(false);
            match result {
                Ok(()) => complete.set(true),
                Err(message) => error.set(message),
            }
        });
    });
    let project_label = target
        .read()
        .projects
        .iter()
        .find(|(id, _)| id == &target_project())
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| target_project());
    let location_label = if scope() == Some(NativeScope::Global) {
        "节点全局".to_owned()
    } else {
        project_label
    };
    rsx! {
        Modal { id: "resource-install", title: "选择安装范围", class: "resource-flow-dialog", trap_focus: true, onclose,
            div { class: "resource-flow-body",
                p { class: "resource-flow-lead", "为所选资源选择安装位置。" }
                div { class: "resource-flow-box",
                    h3 { "待安装资源 · {choices.len()} 项" }
                    for choice in &choices {
                        p { "{choice.name()} · v{choice.version()} · {kind_label(choice.kind())}" }
                    }
                }
                super::install_target::TargetPicker {
                    target, project: target_project, current_projects: projects, shared_client, online,
                    disabled: busy() || complete(), fixed: fixed_project,
                    onchanged: move |()| {
                        scope.set(None); trusted.set(false); complete.set(false);
                        progress.set(vec![]); error.set(String::new());
                    },
                }
                if handoff {
                    p { class: "resource-flow-note", "进入目标节点后读取项目，并重新确认安装范围与授权。" }
                    button { class: "button primary",
                        disabled: choices.iter().any(|choice| matches!(choice, ResourceChoice::Installed(_))),
                        onclick: {
                            let choices = choices.clone();
                            move |_| {
                                if let Some(navigation) = navigation {
                                    let packages = choices.iter().filter_map(|choice| match choice {
                                        ResourceChoice::Public(offer) => Some(super::packages::reference(offer)),
                                        ResourceChoice::Installed(_) => None,
                                    }).collect();
                                    navigation.install.call((target.peek().address.clone(), packages));
                                }
                            }
                        }, "进入目标节点继续安装"
                    }
                    if choices.iter().any(|choice| matches!(choice, ResourceChoice::Installed(_))) {
                        p { class: "resource-flow-note", "此资源来自原节点目录，请在目标节点导入资源，或从公共库选择后安装。" }
                    }
                }
                fieldset { class: "resource-install-choices", disabled: busy() || complete() || !target_ready,
                    legend { "安装到哪里？" }
                    for (value, label, description) in [
                        (NativeScope::Global, "节点全局", "此节点上的所有项目都可使用。"),
                        (NativeScope::Project, "指定项目", "仅供所选项目使用。"),
                    ] {
                        label { class: "resource-install-choice",
                            input { r#type: "radio", name: "install-scope", value: if value == NativeScope::Global { "global" } else { "project" }, checked: scope() == Some(value),
                                onchange: move |_| { scope.set(Some(value)); trusted.set(false); error.set(String::new()); progress.set(vec![]); } }
                            span { strong { "{label}" } small { "{description}" } }
                        }
                    }
                }
                if scope() == Some(NativeScope::Project) {
                    label { class: "resource-flow-field", "安装项目"
                        select { aria_label: "安装项目", value: "{target_project}", disabled: busy() || complete() || fixed_project || !target_ready,
                            onchange: move |event| { target_project.set(event.value()); trusted.set(false); error.set(String::new()); progress.set(vec![]); },
                            for (id, label) in &target.read().projects { option { value: "{id}", "{label}" } }
                        }
                    }
                }
                if !complete() {
                    label { class: "resource-flow-trust",
                        input { r#type: "checkbox", checked: trusted(), disabled: busy() || !target_ready, onchange: move |event| trusted.set(event.checked()) }
                        span { "我已核对来源与使用要求，授权在所选范围安装。" }
                    }
                    p { class: "resource-flow-note", "安装过程会获取缺少的资源并回读节点目录。安装完成后可继续选择工作组合。" }
                }
                for entry in progress() { p { role: "status", class: "resource-flow-note", "{entry}" } }
                if !error().is_empty() { p { role: "alert", class: "resource-error", "{error}" } }
                if busy() { p { role: "status", "正在安装并核对结果，关闭弹窗后请重新读取目录。" } }
                if complete() { p { role: "status", "安装已由节点目录确认 · {target.read().address} · {location_label}" } }
                div { class: "resource-flow-actions",
                    button { class: "button", onclick: move |_| onclose.call(()), if fixed_project { "返回工作设置" } else { "取消" } }
                    if !error().is_empty() {
                        button { class: "button", disabled: busy() || !target_ready, onclick: move |_| submit.call(true), "仅回读安装结果" }
                    }
                    if complete() {
                        button { class: "button primary", onclick: move |_| { if let Some(scope) = scope() { oncomplete.call((target.peek().address.clone(), target_project(), scope)); } }, if fixed_project { "继续工作准备" } else { "完成安装" } }
                    } else {
                        button { class: "button primary", disabled: busy() || !target_ready || scope().is_none() || !trusted() || target_project().is_empty(),
                            onclick: move |_| submit.call(false), "确认安装" }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn WorkDialog(
    choices: Vec<ResourceChoice>,
    project: String,
    projects: Vec<(String, String)>,
    node_label: String,
    session: Option<String>,
    session_project: Option<String>,
    shared_client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    session_generation: Signal<u64>,
    online: bool,
    idle: bool,
    #[props(default)] defaults: bool,
    onopen: EventHandler<String>,
    onchanged: EventHandler<()>,
    onclose: EventHandler<()>,
) -> Element {
    let mut target_project = use_signal(|| project);
    let mut destination = use_signal(|| if defaults { "defaults" } else { "new" }.to_owned());
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let mut missing = use_signal(|| Vec::<ResourceChoice>::new());
    let mut attempt = use_signal(|| None::<NativeApply>);
    let mut created = use_signal(|| None::<String>);
    let mut create_submitted = use_signal(|| false);
    let mut done = use_signal(|| false);
    let mut default_done = use_signal(|| false);
    let choices_submit = choices.clone();
    let session_submit = session.clone();
    let session_project_submit = session_project.clone();
    let mut prepare = move |_| {
        if *busy.peek() || !online || *done.peek() {
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        let project = target_project.peek().clone();
        let destination = destination.peek().clone();
        if destination == "current" && (!idle || session_project_submit.as_ref() != Some(&project))
        {
            error.set("当前会话需属于此项目且在线空闲，请重新确认。".into());
            return;
        }
        let session = session_submit.clone();
        let choices = choices_submit.clone();
        let epoch = *connection_epoch.peek();
        let generation = *session_generation.peek();
        busy.set(true);
        error.set(String::new());
        notice.set("正在核对目标项目与资源版本…".into());
        spawn(async move {
            let result = async {
                let catalog = NodeApi::native_catalog(&client, &project)
                    .await
                    .map_err(|e| e.to_string())?;
                let absent = choices
                    .iter()
                    .filter(|choice| choice.available(&catalog).is_none())
                    .cloned()
                    .collect::<Vec<_>>();
                if !absent.is_empty() {
                    missing.set(absent);
                    return Ok(None);
                }
                let references = choices
                    .iter()
                    .map(|choice| {
                        choice
                            .available(&catalog)
                            .cloned()
                            .ok_or("资源目录已变化，请重新读取。".to_owned())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let package = references
                    .iter()
                    .find(|resource| resource.kind == work_core::api::NativeKind::Package)
                    .ok_or("请选择一个主 Package 后开始工作。")?;
                let mut target = NativeSelection {
                    main_package: package.reference.clone(),
                    package_instructions: choices
                        .iter()
                        .find_map(|choice| match choice {
                            ResourceChoice::Public(offer)
                                if offer.manifest.kind == work_core::api::NativeKind::Package =>
                            {
                                Some(offer.manifest.instructions.clone())
                            }
                            _ => None,
                        })
                        .unwrap_or_default(),
                    skills: references
                        .iter()
                        .filter(|r| r.kind == work_core::api::NativeKind::Skill)
                        .map(|r| r.reference.clone())
                        .collect(),
                    extensions: references
                        .iter()
                        .filter(|r| r.kind == work_core::api::NativeKind::Extension)
                        .map(|r| r.reference.clone())
                        .collect(),
                    tools: NativeTools::Default {},
                    supplementary_instructions: String::new(),
                };
                if destination == "new" {
                    if created.peek().is_none() {
                        if *create_submitted.peek() {
                            return Err("新建会话结果仍待核对，请返回会话列表确认。".into());
                        }
                        create_submitted.set(true);
                        let id = NodeApi::native_create(
                            &client,
                            &project,
                            NativeCreate {
                                developer: false,
                                selection: Some(target.clone()),
                            },
                        )
                        .await
                        .map_err(|e| {
                            format!("新建会话结果未确认，请返回会话列表核对，避免重复创建。{e}")
                        })?;
                        created.set(Some(id));
                    }
                    let id = created.peek().clone().ok_or("新会话标识未返回。")?;
                    let state = NodeApi::native_session_resources(&client, &id)
                        .await
                        .map_err(|e| e.to_string())?;
                    if state.readiness != NativeReadiness::Verified
                        || state.executable != Some(true)
                        || state
                            .configuration
                            .as_ref()
                            .and_then(|config| super::native::selection(&config.desired))
                            .as_ref()
                            != Some(&target)
                    {
                        return Err("新会话已创建，执行状态仍待核对。请回读此会话后继续。".into());
                    }
                    return Ok(Some(id));
                }
                let (revision, previous) = if destination == "current" {
                    let id = session.as_deref().ok_or("尚未选择当前会话。")?;
                    let state = NodeApi::native_session_resources(&client, id)
                        .await
                        .map_err(|e| e.to_string())?;
                    let pending = state.operations.last().is_some_and(|op| {
                        matches!(
                            op.status,
                            NativeOperationStatus::Pending | NativeOperationStatus::Blocked
                        )
                    });
                    if pending && attempt.peek().is_none() {
                        return Err(
                            "当前会话有待处理的资源操作，请在会话资源设置中恢复原操作。".into()
                        );
                    }
                    (
                        state
                            .configuration
                            .as_ref()
                            .map_or(0, |config| config.revision),
                        state
                            .configuration
                            .as_ref()
                            .and_then(|config| super::native::selection(&config.desired)),
                    )
                } else {
                    let state = NodeApi::native_project_resources(&client, &project)
                        .await
                        .map_err(|e| e.to_string())?;
                    (state.revision, state.selection)
                };
                if let Some(previous) = previous {
                    target.tools = previous.tools;
                    target.supplementary_instructions = previous.supplementary_instructions;
                    if target.main_package == previous.main_package {
                        target.package_instructions = previous.package_instructions;
                    }
                }
                let request = attempt.peek().clone().unwrap_or_else(|| NativeApply {
                    expected_revision: revision,
                    operation_id: super::native::operation_id(),
                    target,
                    supersedes_operation_id: None,
                });
                attempt.set(Some(request.clone()));
                let operation = if destination == "current" {
                    NodeApi::native_apply(
                        &client,
                        session.as_deref().ok_or("尚未选择会话。")?,
                        request.clone(),
                    )
                    .await
                } else {
                    NodeApi::native_apply_project(&client, &project, request.clone()).await
                }
                .map_err(|e| format!("应用结果待核对，重试将沿用原操作 ID。{e}"))?;
                if operation.status != NativeOperationStatus::Applied {
                    return Err(format!(
                        "资源操作尚未应用。{}",
                        operation.error.unwrap_or_default()
                    ));
                }
                if destination == "current" {
                    let id = session.ok_or("尚未选择会话。")?;
                    let state = NodeApi::native_session_resources(&client, &id)
                        .await
                        .map_err(|e| e.to_string())?;
                    if state.readiness != NativeReadiness::Verified
                        || state
                            .configuration
                            .as_ref()
                            .and_then(|config| super::native::selection(&config.desired))
                            .as_ref()
                            != Some(&request.target)
                    {
                        return Err("节点尚未确认这次会话组合，请使用原操作回读重试。".into());
                    }
                    Ok(Some(id))
                } else {
                    let state = NodeApi::native_project_resources(&client, &project)
                        .await
                        .map_err(|e| e.to_string())?;
                    if state.selection.as_ref() != Some(&request.target) {
                        return Err("项目默认组合回读尚未确认，请使用原操作重试。".into());
                    }
                    default_done.set(true);
                    Ok(None)
                }
            }
            .await;
            if *connection_epoch.peek() != epoch || *session_generation.peek() != generation {
                return;
            }
            busy.set(false);
            notice.set(String::new());
            match result {
                Ok(Some(id)) => {
                    done.set(true);
                    onchanged.call(());
                    onopen.call(id);
                }
                Ok(None) => {
                    if *default_done.peek() {
                        done.set(true);
                        onchanged.call(());
                        notice.set("项目默认工具组合已保存并回读确认。".into());
                    }
                }
                Err(message) => error.set(message),
            }
        });
    };
    let current_allowed =
        idle && session.is_some() && session_project.as_ref() == Some(&target_project());
    let committed = attempt().is_some() || created().is_some();
    let create_unknown = create_submitted() && created().is_none();
    let dialog_title = if defaults {
        "设置项目默认组合"
    } else {
        "开始工作"
    };
    rsx! {
        if missing().is_empty() {
            Modal { id: "resource-work", title: dialog_title,
                class: "resource-flow-dialog", trap_focus: true, onclose,
                div { class: "resource-flow-body",
                    p { class: "resource-flow-lead", "确认工作项目与工具组合，准备完成后进入会话。" }
                    label { class: "resource-flow-field", "工作项目"
                        select { aria_label: "工作项目", value: "{target_project}", disabled: busy() || committed || done(), onchange: move |event| { target_project.set(event.value()); error.set(String::new()); },
                            for (id, label) in &projects { option { value: "{id}", "{label} · {node_label}" } }
                        }
                    }
                    if !defaults {
                        label { class: "resource-flow-field", "工作会话"
                            select { aria_label: "工作会话", value: "{destination}", disabled: busy() || committed || done(), onchange: move |event| destination.set(event.value()),
                                option { value: "new", "新建会话" }
                                option { value: "current", disabled: !current_allowed, "用于当前会话" }
                            }
                        }
                    }
                    div { class: "resource-flow-box",
                        h3 { "工具组合 · {choices.len()} 项" }
                        for choice in &choices { p { "{choice.name()} · v{choice.version()} · {kind_label(choice.kind())}" } }
                    }
                    p { class: "resource-flow-note", "缺少资源时会弹出安装范围选择。会话应用与安装结果由节点回读确认。" }
                    if !error().is_empty() { p { class: "resource-error", role: "alert", "{error}" } }
                    if !notice().is_empty() { p { role: "status", "{notice}" } }
                    div { class: "resource-flow-actions",
                        button { class: "button", onclick: move |_| onclose.call(()), if done() { "完成" } else { "返回选择" } }
                        if !done() {
                            button { class: "button primary", disabled: busy() || !online || create_unknown || (destination() == "current" && !current_allowed),
                                onclick: move |event| prepare(event), if committed { "回读并重试原操作" } else { "准备并继续" } }
                        }
                    }
                }
            }
        } else {
            InstallDialog { choices: missing(), project: target_project(), projects,
                fixed_project: true, shared_client, connection_epoch, session_generation, online,
                oncomplete: move |_| { missing.set(vec![]); onchanged.call(()); },
                onclose: move |()| missing.set(vec![]),
            }
        }
    }
}
