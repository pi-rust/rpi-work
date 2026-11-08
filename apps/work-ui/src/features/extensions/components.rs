//! Thin package picker and isolated custom UI mount. Component-specific layouts
//! and behavior live in the component package, not in this renderer.
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use work_client::WorkspaceClient;
use work_core::components::{Bundle, Call, InstallRequest, Installed};

#[component]
pub(crate) fn ComponentsPage(
    shared_client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    online: bool,
) -> Element {
    let mut revision = use_signal(|| 0u64);
    let mut selected = use_signal(|| None::<(String, String)>);
    let mut install_open = use_signal(|| false);
    let mut package_url = use_signal(String::new);
    let mut package_text = use_signal(String::new);
    let mut candidate = use_signal(|| None::<Bundle>);
    let mut grants = use_signal(BTreeSet::<String>::new);
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let catalog = use_resource(move || {
        let client = shared_client.read().clone();
        let _ = *connection_epoch.read();
        let _ = *revision.read();
        async move {
            let client = client.ok_or_else(|| "请先连接节点".to_owned())?;
            client.component_catalog().await.map_err(|e| e.to_string())
        }
    });
    // A panel and an install candidate belong to one node connection only.
    use_effect(move || {
        let _ = *connection_epoch.read();
        selected.set(None);
        candidate.set(None);
        grants.set(BTreeSet::new());
        error.set(String::new());
    });
    let epoch = *connection_epoch.read();
    let items = catalog
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let active = selected.read().clone().and_then(|(id, panel)| {
        items
            .iter()
            .find(|c| c.manifest.id == id)
            .cloned()
            .map(|c| (c, panel))
    });
    rsx! {
        style { {include_str!("components.css")} }
        section { class: "component-settings", aria_label: "节点组件",
            div { class: "page-heading",
                div { h1 { "节点组件" } p { "组件提供能力和专用面板，在需要时加载；远程协作由宿主的 rpi-http-extension 提供。" } }
                button { class: "button", disabled: busy() || !online, onclick: move |_| install_open.set(!install_open()), "安装其他组件" }
            }
            if !items.iter().any(|component| component.manifest.id == "rpi-http") {
                div { class: "component-missing", role: "status",
                    strong { "当前宿主未提供远程协作扩展" }
                    p { "rpi-http-extension 未加载，无法在这里开启远程协作、配置 HTTP 或生成手机授权二维码。请升级或重新启动当前执行节点。" }
                }
            }
            if !online { p { class: "connection-form-error", "节点离线，组件操作暂不可用。" } }
            if let Some(Err(message)) = catalog.read().as_ref() {
                p { class: "connection-form-error", role: "alert", "{message}" }
                button { class: "button", onclick: move |_| revision += 1, "重试" }
            } else if catalog.read().is_none() { p { role: "status", "正在读取节点组件…" } }
            if !error.read().is_empty() { p { class: "connection-form-error", role: "alert", "{error}" } }
            if install_open() {
                div { class: "component-install",
                    h2 { "安装组件包" }
                    p { "从 HTTPS 地址下载，或粘贴组件包。Provider 扩展也通过这里安装。安装前检查身份和申请的节点能力；组件页只安装界面包与能力授权，不安装或加载节点宿主。" }
                    input { r#type: "url", aria_label: "组件包下载地址", placeholder: "https://…/component.json", value: "{package_url}", oninput: move |e| { package_url.set(e.value()); candidate.set(None); } }
                    button { class: "button", disabled: busy() || package_url.read().is_empty(), onclick: move |_| {
                        let address = package_url.read().clone();
                        busy.set(true); error.set(String::new()); candidate.set(None); grants.set(BTreeSet::new());
                        spawn(async move {
                            let result = work_client::download_component(&address).await;
                            if *connection_epoch.read() == epoch {
                                match result { Ok(bundle) => candidate.set(Some(bundle)), Err(e) => error.set(e.to_string()) }
                            }
                            busy.set(false);
                        });
                    }, "下载并检查" }
                    details { summary { "粘贴组件包 JSON" }
                        textarea { aria_label: "组件包 JSON", rows: "6", value: "{package_text}", oninput: move |e| { package_text.set(e.value()); candidate.set(None); } }
                        button { class: "button", disabled: busy(), onclick: move |_| {
                            let result = serde_json::from_str::<Bundle>(&package_text.read()).map_err(|e| e.to_string()).and_then(|b| { b.validate()?; Ok(b) });
                            grants.set(BTreeSet::new());
                            match result { Ok(bundle) => { candidate.set(Some(bundle)); error.set(String::new()); }, Err(e) => { candidate.set(None); error.set(e); } }
                        }, "检查组件包" }
                    }
                    if let Some(bundle) = candidate.read().clone() {
                        div { class: "component-review",
                            strong { "{bundle.manifest.title}" } code { "{bundle.manifest.id} · {bundle.manifest.version}" }
                            p { "{bundle.manifest.description}" }
                            p { class: "meta", "内容 SHA-256：{bundle.sha256}" }
                            for capability in bundle.manifest.actions.values().map(|a| a.capability.clone()).collect::<BTreeSet<_>>() {
                                label { class: "component-grant",
                                    input { r#type: "checkbox", checked: grants.read().contains(&capability), onchange: {
                                        let capability = capability.clone();
                                        move |e: FormEvent| { if e.checked() { grants.write().insert(capability.clone()); } else { grants.write().remove(&capability); } }
                                    } }
                                    "允许 {capability}"
                                }
                            }
                            button { class: "button primary", disabled: busy() || !online, onclick: move |_| {
                                let request = InstallRequest { bundle: bundle.clone(), grants: grants.read().clone() };
                                let client = shared_client.read().clone(); busy.set(true); error.set(String::new());
                                spawn(async move {
                                    if let Some(client) = client {
                                        let result = client.component_install(request).await;
                                        if *connection_epoch.read() == epoch {
                                            match result { Ok(_) => { candidate.set(None); install_open.set(false); revision += 1; }, Err(e) => error.set(e.to_string()) }
                                        }
                                    }
                                    busy.set(false);
                                });
                            }, "安装到当前节点" }
                        }
                    }
                }
            }
            if let Some((component, panel)) = active {
                div { key: "{epoch}:{component.manifest.id}:{component.sha256}:{panel}",
                    button { class: "button", onclick: move |_| selected.set(None), "返回组件目录" }
                    ComponentPanel { shared_client, connection_epoch, component, panel, online }
                }
            } else {
                div { class: "component-catalog",
                    for component in items {
                        article { class: "component-card", key: "{component.manifest.id}",
                            div { strong { if component.manifest.id == "rpi-http" { "远程协作" } else { "{component.manifest.title}" } } span { class: "meta", if component.manifest.id == "rpi-http" { "宿主扩展 · rpi-http-extension" } else { "{component.manifest.version}" } } }
                            p { if component.manifest.id == "rpi-http" { "由宿主扩展管理 HTTP 访问、共享开关、节点配置和手机授权。配置会保存在当前执行节点。" } else { "{component.manifest.description}" } }
                            for panel in &component.manifest.panels {
                                button { class: "button", disabled: !online || !component.enabled, onclick: {
                                    let id = component.manifest.id.clone(); let panel = panel.id.clone();
                                    move |_| selected.set(Some((id.clone(), panel.clone())))
                                }, "打开{panel.title}" }
                            }
                            if component.source != "bundled" && component.manifest.id != "rpi-http" {
                                button { class: "button", disabled: busy() || !online, onclick: {
                                    let id = component.manifest.id.clone();
                                    move |_| {
                                        let client = shared_client.read().clone(); let id = id.clone(); busy.set(true);
                                        spawn(async move {
                                            if let Some(client) = client {
                                                let result = client.component_remove(&id).await;
                                                if *connection_epoch.read() == epoch { match result { Ok(()) => revision += 1, Err(e) => error.set(e.to_string()) } }
                                            }
                                            busy.set(false);
                                        });
                                    }
                                }, "卸载" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameRequest {
    id: String,
    action: String,
    arguments: Value,
}

#[component]
fn ComponentPanel(
    shared_client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    component: Installed,
    panel: String,
    online: bool,
) -> Element {
    let epoch = *connection_epoch.read();
    let id = component.manifest.id.clone();
    let frame_id = format!("component-{epoch}-{id}-{panel}");
    let resource = use_resource(move || {
        let client = shared_client.read().clone();
        let id = id.clone();
        let panel = panel.clone();
        async move {
            client
                .ok_or_else(|| "请先连接节点".to_owned())?
                .component_panel(&id, &panel)
                .await
                .map_err(|e| e.to_string())
        }
    });
    let data = resource.read().clone();
    let mut bridge_error = use_signal(String::new);
    match data {
        Some(Ok(html)) => {
            let actions = component
                .manifest
                .actions
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            let id = component.manifest.id;
            rsx! {
                if !bridge_error.read().is_empty() { p { class: "connection-form-error", role: "alert", "{bridge_error}" } }
                iframe { id: frame_id.clone(), class: "component-frame", title: component.manifest.title,
                    "sandbox": "allow-scripts", referrerpolicy: "no-referrer",
                    onmounted: move |_| {
                        let id = id.clone(); let frame_id = frame_id.clone(); let html = html.clone(); let actions = actions.clone();
                        spawn(async move {
                            let script = format!("{}\nawait mountComponentPanel({}, {}, {});", work_core::components::UI_BRIDGE, json!(frame_id), json!(html), json!(actions));
                            let mut evaluation = document::eval(&script);
                            while let Ok(request) = evaluation.recv::<FrameRequest>().await {
                                if *connection_epoch.read() != epoch { break; }
                                let client = shared_client.read().clone();
                                let result = if !online { Err("节点离线".to_owned()) } else if let Some(client) = client {
                                    component_timeout(client.component_call(&id, Call { action: request.action, arguments: request.arguments })).await.and_then(|r| r.map_err(|e| e.to_string()))
                                } else { Err("节点连接已关闭".to_owned()) };
                                if *connection_epoch.read() != epoch { break; }
                                let response = match result { Ok(value) => json!({"id":request.id,"value":value}), Err(error) => json!({"id":request.id,"error":error}) };
                                if evaluation.send(response).is_err() { bridge_error.set("组件通信已关闭，请重新打开面板。".into()); break; }
                            }
                        });
                    }
                }
            }
        }
        Some(Err(error)) => rsx! { p { class: "connection-form-error", role: "alert", "{error}" } },
        None => rsx! { p { role: "status", "正在加载组件界面…" } },
    }
}

async fn component_timeout<T>(future: impl std::future::Future<Output = T>) -> Result<T, String> {
    use std::{
        future::{Future, poll_fn},
        pin::pin,
        task::Poll,
    };
    let mut future = pin!(future);
    let mut timer = pin!(async {
        #[cfg(target_arch = "wasm32")]
        gloo_timers::future::TimeoutFuture::new(12_000).await;
        #[cfg(not(target_arch = "wasm32"))]
        tokio::time::sleep(std::time::Duration::from_secs(12)).await;
    });
    poll_fn(move |cx| {
        if let Poll::Ready(value) = future.as_mut().poll(cx) {
            return Poll::Ready(Ok(value));
        }
        if timer.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err("组件请求超时".into()));
        }
        Poll::Pending
    })
    .await
}
