//! Shared, data-only discovery. Node acquisition is never installation or session selection.
use dioxus::prelude::*;
use work_client::WorkspaceClient;
use work_core::api::{
    NativeScope, NodeApi, PackageBundle, PackageCatalog, PackageInstall, PackageOffer, PackageRef,
};

fn reference(offer: &PackageOffer) -> PackageRef {
    PackageRef {
        id: offer.manifest.id.clone(),
        version: offer.manifest.version.clone(),
        sha256: offer.sha256.clone(),
    }
}

fn acquired(catalog: Option<&PackageCatalog>, target: &PackageRef) -> bool {
    catalog.is_some_and(|c| {
        c.packages
            .iter()
            .any(|p| reference(p) == *target && p.acquired)
    })
}

fn may_install(online: bool, busy: bool, acquired: bool, trusted: bool) -> bool {
    online && !busy && acquired && trusted
}

#[derive(Clone, Debug, PartialEq)]
struct DiscoveryContext {
    project: String,
    epoch: u64,
    generation: u64,
    online: bool,
}

fn fresh(
    expected: &DiscoveryContext,
    current: &DiscoveryContext,
    ticket: u64,
    latest: u64,
) -> bool {
    expected == current && current.online && ticket == latest
}

#[derive(Clone, Copy)]
enum Action {
    Preview,
    Download,
    Acquire,
    Install,
}

/// Key the entire state owner, not just its DOM: reconnect/project/session changes
/// discard trust, pending observations and receipts. Dioxus cancels scoped tasks
/// on unmount; live epoch/generation/ticket checks also guard every async result.
#[component]
pub fn PackageDiscovery(
    project: String,
    shared_client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    session_generation: Signal<u64>,
    online: bool,
    onchanged: EventHandler<()>,
    #[props(default)] resource_kind: String,
) -> Element {
    let context = DiscoveryContext {
        project,
        epoch: connection_epoch(),
        generation: session_generation(),
        online: online && shared_client.read().is_some(),
    };
    let key = serde_json::to_string(&(
        &context.project,
        context.epoch,
        context.generation,
        context.online,
    ))
    .unwrap_or_default();
    rsx! {
        for owner in [key] {
            PackageDiscoveryState { key: "{owner}", context: context.clone(), shared_client, connection_epoch, session_generation, onchanged, resource_kind: resource_kind.clone() }
        }
    }
}

#[component]
fn PackageDiscoveryState(
    context: DiscoveryContext,
    shared_client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    session_generation: Signal<u64>,
    onchanged: EventHandler<()>,
    resource_kind: String,
) -> Element {
    let mut catalog = use_signal(|| None::<PackageCatalog>);
    let mut target = use_signal(|| None::<PackageRef>);
    let mut bundle = use_signal(|| None::<PackageBundle>);
    let mut query = use_signal(String::new);
    let mut kind = use_signal(|| "all".to_owned());
    let mut scope = use_signal(|| NativeScope::Project);
    let mut trusted = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let mut ticket = use_signal(|| 0u64);
    let mut refresh = use_signal(|| 0u64);
    // Frozen per keyed owner. No callback may retain a project from another mount.
    let mounted = use_signal(|| context.clone());
    let is_fresh = move |expected: &DiscoveryContext, request: u64| {
        let current = DiscoveryContext {
            project: mounted.peek().project.clone(),
            epoch: *connection_epoch.peek(),
            generation: *session_generation.peek(),
            online: mounted.peek().online && shared_client.peek().is_some(),
        };
        fresh(expected, &current, request, *ticket.peek())
    };
    use_effect(move || {
        let _ = refresh.read();
        let expected = mounted.peek().clone();
        let next = ticket.peek().wrapping_add(1);
        ticket.set(next);
        catalog.set(None);
        trusted.set(false);
        error.set(String::new());
        if !expected.online {
            busy.set(false);
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        busy.set(true);
        spawn(async move {
            let result = NodeApi::package_catalog(&client, &expected.project).await;
            if !is_fresh(&expected, next) {
                return;
            }
            busy.set(false);
            match result {
                Ok(value) => catalog.set(Some(value)),
                Err(e) => error.set(format!("精选目录不可用（旧服务或不支持时不会回退）：{e}")),
            }
        });
    });

    let mut act = move |action: Action| {
        if *busy.peek() || !mounted.peek().online {
            return;
        }
        let (Some(client), Some(package)) = (shared_client.peek().clone(), target.peek().clone())
        else {
            return;
        };
        let expected = mounted.peek().clone();
        // Check live connection before issuing a write, not just after awaiting it.
        if !is_fresh(&expected, *ticket.peek()) {
            return;
        }
        let install_scope = *scope.peek();
        let confirmed_trust = *trusted.peek();
        if matches!(action, Action::Install)
            && !may_install(
                expected.online,
                *busy.peek(),
                acquired(catalog.peek().as_ref(), &package),
                confirmed_trust,
            )
        {
            return;
        }
        let next = ticket.peek().wrapping_add(1);
        ticket.set(next);
        busy.set(true);
        error.set(String::new());
        notice.set(String::new());
        if matches!(action, Action::Install) {
            trusted.set(false);
        }
        spawn(async move {
            match action {
                Action::Preview | Action::Download => {
                    let result =
                        NodeApi::package_bundle(&client, &expected.project, package.clone()).await;
                    if !is_fresh(&expected, next) {
                        return;
                    }
                    match result {
                        Ok(value)
                            if value.manifest.id == package.id
                                && value.manifest.version == package.version
                                && value.sha256 == package.sha256 =>
                        {
                            bundle.set(Some(value.clone()));
                            if matches!(action, Action::Download) {
                                // Browser/webview Blob only; never write an arbitrary native path.
                                let result = download_bundle(&value).await;
                                if !is_fresh(&expected, next) {
                                    return;
                                }
                                match result {
                                    Ok(()) => notice.set("已请求下载 UTF-8 JSON 资源包；请在本机下载列表核对。此操作不获取到执行节点，也不安装。".into()),
                                    Err(e) => error.set(e),
                                }
                            }
                        }
                        Ok(_) => {
                            error.set("节点返回的包身份或摘要不匹配，已拒绝预览与下载。".into())
                        }
                        Err(e) => error.set(format!("读取包失败（不回退其他来源）：{e}")),
                    }
                }
                Action::Acquire => {
                    let result =
                        NodeApi::package_acquire(&client, &expected.project, package.clone()).await;
                    if !is_fresh(&expected, next) {
                        return;
                    }
                    match result {
                        Ok(value) if value.package == package => {
                            notice.set(
                                "节点获取请求完成，正在权威回读目录；获取不等于安装、选择或加载。"
                                    .into(),
                            );
                            catalog.set(None);
                            refresh += 1;
                        }
                        Ok(_) => {
                            error.set("获取回执身份不匹配；请刷新目录核对，未推定已获取。".into())
                        }
                        Err(e) => {
                            error.set(format!("获取失败或结果未知，请刷新核对；不会自动重试：{e}"))
                        }
                    }
                }
                Action::Install => {
                    let result = NodeApi::package_install(
                        &client,
                        &expected.project,
                        PackageInstall {
                            package,
                            scope: install_scope,
                            trusted: confirmed_trust,
                        },
                    )
                    .await;
                    if !is_fresh(&expected, next) {
                        return;
                    }
                    match result {
                        Ok(receipt) if receipt.success => {
                            notice.set(format!("当前节点安装回执成功 · {install_scope:?}：{}\n{}\n未自动启用、应用或运行。请用下方原生控件选择并应用，在会话实际加载状态中核对。", receipt.observation, receipt.diagnostics));
                            onchanged.call(());
                        }
                        Ok(receipt) => error.set(format!(
                            "安装未成功：{}\n{}",
                            receipt.observation, receipt.diagnostics
                        )),
                        Err(e) => error.set(format!(
                            "安装失败或结果未知，请在下方回读核对；不会自动重试：{e}"
                        )),
                    }
                }
            }
            if !is_fresh(&expected, next) {
                return;
            }
            busy.set(false);
        });
    };
    let selected = target();
    let offer = catalog
        .read()
        .as_ref()
        .and_then(|c| c.packages.iter().find(|p| Some(reference(p)) == selected))
        .cloned();
    let filter = query().trim().to_lowercase();
    let kind_filter = if resource_kind.is_empty() {
        kind()
    } else {
        resource_kind.clone()
    };
    let offers = catalog
        .read()
        .as_ref()
        .map(|c| {
            c.packages
                .iter()
                .filter(|p| {
                    let category = format!("{:?}", p.manifest.kind).to_lowercase();
                    (kind_filter == "all" || category == kind_filter)
                        && format!(
                            "{} {} {} {} {}",
                            p.manifest.id,
                            p.manifest.name,
                            p.manifest.title,
                            p.manifest.description,
                            p.manifest.author
                        )
                        .to_lowercase()
                        .contains(&filter)
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    rsx! {
        section { class: "package-discovery", aria_label: "精选 Package 发现", aria_busy: busy(),
            style { {include_str!("packages.css")} }
            h3 { "随应用发布的精选目录" }
            p { "非在线市场 · 仅分发 UTF-8 文本资源，不含二进制，不编译或执行。基础 / 开发 Package 与独立 Skill 以节点返回目录为准。" }
            p { class: "package-meta", "目标：当前执行节点 · 项目 {context.project}。下载到本机 ≠ 获取到节点 ≠ 安装 ≠ 会话已选择 / 已加载。" }
            div { class: "package-filters",
                label { "筛选目录" input { r#type: "search", value: "{query}", placeholder: "名称、描述或作者", oninput: move |e| query.set(e.value()) } }
                if resource_kind.is_empty() { label { "资源类型" select { aria_label: "资源类型", value: "{kind}", onchange: move |e| kind.set(e.value()),
                    option { value: "all", "全部" }
                    option { value: "package", "Package" }
                    option { value: "skill", "Skill" }
                } } }
                button { class: "button", disabled: busy() || !context.online, onclick: move |_| { trusted.set(false); refresh += 1; }, "刷新精选目录" }
            }
            if !context.online { p { class: "package-state-block", role: "status", "执行节点离线；重连后重新读取目录并确认信任。" } }
            if let Some(c) = catalog.read().as_ref() {
                p { class: "package-meta", "目录来源：{c.source}" }
                for diagnostic in &c.diagnostics { p { "{diagnostic}" } }
                if offers.is_empty() { p { class: "package-state-block", role: "status", "没有匹配的精选资源。" } }
            }
            div { class: "package-list",
                for item in offers {
                    { let item_ref = reference(&item); let active = selected.as_ref() == Some(&item_ref); rsx! {
                        article { class: "package-card",
                            h4 { "{item.manifest.title} · {item.manifest.version}" }
                            p { "{item.manifest.description}" }
                            p { class: "package-meta", "{item.manifest.kind:?} · {item.manifest.author} · {item.file_count} 个文件 / {item.size_bytes} 字节" }
                            p { if item.acquired { "当前执行节点：已获取（非已安装）" } else { "当前执行节点：未获取（不代表未安装）" } }
                            button { class: "button", disabled: busy(), aria_pressed: active, onclick: move |_| {
                                ticket += 1;
                                target.set(Some(item_ref.clone()));
                                bundle.set(None);
                                trusted.set(false);
                                error.set(String::new());
                                notice.set(String::new());
                                act(Action::Preview);
                            }, if active { "重新读取详情 / 内容预览" } else { "详情 / 内容预览" } }
                        }
                    } }
                }
            }
            if let Some(item) = offer {
                section { class: "package-detail", aria_label: "精选包详情",
                    h4 { "{item.manifest.title} · {item.manifest.version}" }
                    p { "标识：{item.manifest.id} · 原生名称：{item.manifest.name}" }
                    p { "作者：{item.manifest.author} · 来源：{item.manifest.origin}" }
                    p { class: "package-meta", "SHA-256：{item.sha256}" }
                    p { "安装：请在“当前节点”中核对，本目录不从已获取状态推断安装。会话选择与实际加载请在会话资源设置中核对。" }
                    div { class: "package-actions",
                        button { class: "button", disabled: busy() || !context.online, onclick: move |_| act(Action::Download), "下载到本机" }
                        button { class: "button", disabled: busy() || !context.online || item.acquired, onclick: move |_| act(Action::Acquire), "获取到当前节点" }
                    }
                    fieldset { disabled: busy() || !context.online,
                        legend { "明确授权安装到当前节点" }
                        label { "安装范围" select { aria_label: "安装范围", value: if scope() == NativeScope::Global { "global" } else { "project" }, onchange: move |e| {
                            scope.set(if e.value() == "global" { NativeScope::Global } else { NativeScope::Project });
                            trusted.set(false);
                            notice.set(String::new());
                            ticket += 1;
                        }, option { value: "project", "当前项目（Project）" } option { value: "global", "节点全局（Global）" } } }
                        label { class: "package-trust", input { r#type: "checkbox", checked: trusted(), onchange: move |e| trusted.set(e.checked()) } "我已检查内容，信任此来源，并明确授权在所选范围安装资源。安装不会自动选择、应用或运行。" }
                        button { class: "button primary", disabled: !may_install(context.online, busy(), item.acquired, trusted()), onclick: move |_| act(Action::Install), "安装" }
                        if !item.acquired { p { "请先获取到当前节点，并等待目录回读确认，再授权安装。" } }
                    }
                    if let Some(value) = bundle.read().as_ref() {
                        details { open: true, summary { "只读内容预览（纯文本，不执行）" }
                            p { "{value.manifest.description}" }
                            for instruction in &value.manifest.instructions { p { "指令映射：{instruction}" } }
                            for (path, content) in &value.sources {
                                details { summary { "{path}" } pre { "{content}" } }
                            }
                        }
                    }
                }
            }
            if busy() { p { class: "package-state-block package-loading", role: "status", "正在读取或提交节点操作，请等待。离页或断线不代表节点取消写入，返回后请回读核对。" } }
            if !notice().is_empty() { pre { class: "package-state-block package-notice", role: "status", "{notice}" } }
            if !error().is_empty() { p { class: "package-state-block package-error", role: "alert", "{error}" } }
        }
    }
}

async fn download_bundle(bundle: &PackageBundle) -> Result<(), String> {
    let text = serde_json::to_string_pretty(bundle).map_err(|e| e.to_string())?;
    let data = serde_json::to_string(&format!("{text}\n")).map_err(|e| e.to_string())?;
    // A constant filename avoids treating metadata as a browser-local path.
    let script = format!(
        "try {{ const blob=new Blob([{data}],{{type:'application/json;charset=utf-8'}}); const url=URL.createObjectURL(blob); const a=document.createElement('a'); a.href=url; a.download='curated-resource.rpi-package.json'; a.click(); setTimeout(()=>URL.revokeObjectURL(url),1000); dioxus.send({{ok:true}}); }} catch(error) {{ dioxus.send({{ok:false,error:String(error)}}); }}"
    );
    let result = document::eval(&script)
        .recv::<serde_json::Value>()
        .await
        .map_err(|e| format!("下载不可用：{e}"))?;
    if result["ok"] == true {
        Ok(())
    } else {
        Err(format!("下载失败：{}", result["error"]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_requires_every_explicit_gate() {
        for online in [false, true] {
            for busy in [false, true] {
                for acquired in [false, true] {
                    for trusted in [false, true] {
                        assert_eq!(
                            may_install(online, busy, acquired, trusted),
                            online && !busy && acquired && trusted
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn rejects_reconnect_project_generation_and_late_ticket() {
        let expected = DiscoveryContext {
            project: "p1".into(),
            epoch: 7,
            generation: 9,
            online: true,
        };
        assert!(fresh(&expected, &expected, 3, 3));
        let mut current = expected.clone();
        current.project = "p2".into();
        assert!(!fresh(&expected, &current, 3, 3));
        current = expected.clone();
        current.epoch += 1;
        assert!(!fresh(&expected, &current, 3, 3));
        current = expected.clone();
        current.generation += 1;
        assert!(!fresh(&expected, &current, 3, 3));
        current = expected.clone();
        current.online = false;
        assert!(!fresh(&expected, &current, 3, 3));
        assert!(!fresh(&expected, &expected, 3, 4));
    }

    #[test]
    fn only_exact_authoritative_acquired_entry_unlocks_install() {
        let mut catalog: PackageCatalog = serde_json::from_value(serde_json::json!({
            "source": "embedded", "diagnostics": [], "packages": [{
                "manifest": {
                    "schemaVersion": 1, "id": "base", "name": "base", "version": "1.0.0",
                    "kind": "package", "title": "Base", "description": "Text only",
                    "author": "Work", "origin": "embedded", "instructions": []
                },
                "sha256": "digest", "fileCount": 1, "sizeBytes": 10, "acquired": true
            }]
        }))
        .unwrap();
        let target = reference(&catalog.packages[0]);
        assert!(acquired(Some(&catalog), &target));
        let mut other = target.clone();
        other.sha256 = "changed".into();
        assert!(!acquired(Some(&catalog), &other));
        other = target.clone();
        other.version = "2.0.0".into();
        assert!(!acquired(Some(&catalog), &other));
        other = target.clone();
        other.id = "developer".into();
        assert!(!acquired(Some(&catalog), &other));
        catalog.packages[0].acquired = false;
        assert!(!acquired(Some(&catalog), &target));
    }

    #[test]
    fn no_catalog_never_authorizes_acquisition() {
        let target = PackageRef {
            id: "base".into(),
            version: "1.0.0".into(),
            sha256: "digest".into(),
        };
        assert!(!acquired(None, &target));
    }
}
