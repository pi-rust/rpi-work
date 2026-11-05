//! Workspace-level node diagnostics. Mount the hook once, not once per project row.
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    pin::pin,
    rc::Rc,
    task::Poll,
};

use dioxus::{core::Task, prelude::*};
use work_client::{AuthState, Capabilities, Health, Project, WorkspaceClient};
use work_core::AgentDescriptor;

use crate::{
    app::{
        connections::{self, Phase},
        identity, local_node,
    },
    shared::components::{Icon, Modal},
};

pub const CSS: &str = include_str!("node_info.css");
const HEALTH_ERROR: &str = "节点不可达，请检查网络或节点服务。";
const AUTH_ERROR: &str = "无法读取节点认证状态，请刷新重试。";

#[derive(Clone, Default, PartialEq)]
pub struct Probe {
    pub loading: bool,
    pub health: Option<Health>,
    pub auth: Option<AuthState>,
    pub error: Option<String>,
    pub checked_at: String,
}

#[derive(Default)]
struct Runner {
    task: RefCell<Option<Task>>,
    ticket: Cell<u64>,
}

impl Runner {
    fn cancel(&self) -> u64 {
        let ticket = self.ticket.get().wrapping_add(1);
        self.ticket.set(ticket);
        if let Some(task) = self.task.borrow_mut().take() {
            task.cancel();
        }
        ticket
    }
}

fn same_scope(expected: (&str, u64, u64), current: (&str, u64, u64)) -> bool {
    expected == current
}

/// One cancellable, immediate + 30 second health/auth loop for the active workspace.
/// `epoch` must change when replacing a workspace client at the same address.
/// Reads of the probe/task use `peek` or nonreactive storage: probe writes must not
/// retrigger the effect. Refresh interrupts the interval, never an active request.
pub fn use_node_probe(
    client: Signal<Option<WorkspaceClient>>,
    address: Signal<String>,
    epoch: Signal<u64>,
    phase: Signal<Phase>,
) -> (Signal<Probe>, EventHandler) {
    let mut probe = use_signal(Probe::default);
    let mut revision = use_signal(|| 0_u64);
    let runner = use_hook(|| Rc::new(Runner::default()));
    let effect_runner = runner.clone();
    use_effect(move || {
        // Subscribe to every input even when disconnected.
        let node = client.read().clone();
        let scope_address = address.read().clone();
        let scope_epoch = *epoch.read();
        let current_phase = phase.read().clone();
        let _ = *revision.read();
        let runner = effect_runner.clone();
        let ticket = runner.cancel();
        probe.set(Probe::default());
        let Some(node) = node.filter(|_| matches!(current_phase, Phase::Online)) else {
            return;
        };
        probe.set(Probe {
            loading: true,
            ..Probe::default()
        });
        let task_runner = runner.clone();
        let task = spawn(async move {
            let valid = || {
                same_scope(
                    (&scope_address, scope_epoch, ticket),
                    (
                        address.peek().as_str(),
                        *epoch.peek(),
                        task_runner.ticket.get(),
                    ),
                ) && matches!(*phase.peek(), Phase::Online)
            };
            loop {
                if !valid() {
                    return;
                }
                let mut next = probe.peek().clone();
                next.loading = true;
                next.error = None;
                next.auth = None;
                probe.set(next.clone());
                read_node(&node, &mut next, &valid).await;
                if !valid() {
                    return;
                }
                // Local browser time on web and native WebViews. No address, server
                // value, or error is interpolated into JavaScript.
                let checked_at = checked_time().await;
                if !valid() {
                    return;
                }
                next.checked_at = checked_at;
                next.loading = false;
                probe.set(next);
                delay(30_000).await;
                if !valid() {
                    return;
                }
            }
        });
        *runner.task.borrow_mut() = Some(task);
    });
    use_drop(move || {
        runner.cancel();
    });
    let refresh = EventHandler::new(move |()| {
        if probe.peek().loading
            || !matches!(*phase.peek(), Phase::Online)
            || client.peek().is_none()
        {
            return;
        }
        // Set synchronously so two clicks before the next effect cannot queue
        // duplicate refreshes. The effect cancels the old interval task.
        probe.write().loading = true;
        let next = (*revision.peek()).wrapping_add(1);
        revision.set(next);
    });
    (probe, refresh)
}

async fn read_node(node: &WorkspaceClient, next: &mut Probe, valid: &impl Fn() -> bool) {
    // This budget covers both requests together, not 15s per endpoint.
    let mut reading_auth = false;
    let result = with_timeout(15_000, async {
        let health = node.health().await;
        if !valid() {
            return;
        }
        match health {
            Ok(health) => {
                let ready = health.ok;
                next.health = Some(health);
                if ready {
                    reading_auth = true;
                    let auth = node.auth().await;
                    if !valid() {
                        return;
                    }
                    match auth {
                        Ok(auth) => next.auth = Some(auth),
                        Err(_) => next.error = Some(AUTH_ERROR.into()),
                    }
                }
            }
            Err(_) => next.error = Some(HEALTH_ERROR.into()),
        }
    })
    .await;
    if result.is_err() {
        next.auth = None;
        next.error = Some(
            if reading_auth {
                AUTH_ERROR
            } else {
                HEALTH_ERROR
            }
            .into(),
        );
    }
}

async fn delay(milliseconds: u32) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(milliseconds).await;
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(std::time::Duration::from_millis(u64::from(milliseconds))).await;
}

async fn with_timeout<T>(milliseconds: u32, future: impl Future<Output = T>) -> Result<T, ()> {
    let mut future = pin!(future);
    let mut timer = pin!(delay(milliseconds));
    poll_fn(move |context| {
        if let Poll::Ready(value) = future.as_mut().poll(context) {
            return Poll::Ready(Ok(value));
        }
        if timer.as_mut().poll(context).is_ready() {
            return Poll::Ready(Err(()));
        }
        Poll::Pending
    })
    .await
}

async fn checked_time() -> String {
    // Bound the UI bridge too: a detached/unavailable WebView must not leave the
    // probe permanently loading. Every completion is identity-checked by caller.
    let result = with_timeout(1_000, document::eval(
        "const now = new Date(); dioxus.send(Number.isFinite(now.getTime()) ? now.toLocaleString() : '');",
    ).recv::<String>()).await;
    match result {
        Ok(Ok(value)) if !value.trim().is_empty() && value != "Invalid Date" => value,
        _ => "刚刚检查".into(),
    }
}

/// Phase failures and in-flight checks always outrank previously cached health.
pub fn status(phase: &Phase, probe: &Probe) -> (&'static str, &'static str) {
    match phase {
        Phase::Failed(_) => return ("offline", "节点不可达"),
        Phase::Idle => return ("unknown", "节点未连接"),
        Phase::Connecting | Phase::Authenticating => return ("connecting", "节点连接中"),
        Phase::PasswordRequired => return ("auth", "节点需要登录"),
        Phase::Online => {}
    }
    if probe.loading {
        return ("checking", "正在检查节点…");
    }
    if let Some(error) = &probe.error {
        return if error == AUTH_ERROR {
            ("unknown", "节点认证状态未知")
        } else {
            ("offline", "节点不可达")
        };
    }
    match &probe.health {
        Some(health) if !health.ok => ("unready", "节点未就绪"),
        Some(_) => match &probe.auth {
            Some(auth) if auth.required && !auth.authenticated => ("auth", "节点需要登录"),
            Some(_) => ("online", "节点在线"),
            None => ("unknown", "节点认证状态未知"),
        },
        None if probe.checked_at.is_empty() => ("connecting", "节点连接中"),
        None => ("unknown", "节点状态未知"),
    }
}

pub fn is_local(client: Option<&WorkspaceClient>, address: &str) -> bool {
    client.is_some_and(WorkspaceClient::is_local) || local_node::is_local(address)
}

/// Never return arbitrary malformed input. Validate before `node_origin`, which
/// intentionally tolerates invalid ports for identity matching (not UI display).
pub fn safe_address(raw: &str) -> String {
    let raw = raw.trim();
    if local_node::is_local(raw) {
        return "rpi-work://local".into();
    }
    let unknown = || "未报告".to_owned();
    if raw.len() > 2048 || raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return unknown();
    }
    let Some((scheme, rest)) = raw.split_once("://") else {
        return unknown();
    };
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return unknown();
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    // Backslashes and percent escapes have ambiguous browser URL semantics.
    if authority.contains(['\\', '%']) {
        return unknown();
    }
    let authority = authority.rsplit('@').next().unwrap_or_default();
    let candidate = format!("{scheme}://{authority}");
    match connections::normalize_address(&candidate, false) {
        Ok(valid) => identity::node_origin(&valid),
        Err(_) => unknown(),
    }
}

/// Location is independent of reachability, and HTTP alone is not proof of cloud hosting.
pub fn workspace_location(
    local: bool,
    address: &str,
) -> (&'static str, &'static str, &'static str) {
    if local {
        ("local", "本机工作空间", "monitor")
    } else if safe_address(address) == "未报告" {
        ("unknown", "未选择工作空间", "layout")
    } else {
        ("remote", "远程工作空间", "cloud")
    }
}

#[component]
pub fn WorkspaceLocation(local: bool, address: String, onsettings: EventHandler) -> Element {
    let (kind, label, icon) = workspace_location(local, &address);
    let address = safe_address(&address);
    rsx! {
        button { class: "workspace-location", "data-node-kind": kind,
            aria_label: format!("当前工作空间：{label}，切换执行节点"), title: "{label} · {address}",
            onclick: move |_| onsettings.call(()),
            Icon { name: icon }
            span { strong { "{label}" } small { "{address}" } }
            Icon { name: "chevron-right" }
        }
    }
}

#[component]
pub fn NodeBadge(
    local: bool,
    tone: String,
    label: String,
    project_name: String,
    onopen: EventHandler,
    oncontext: EventHandler,
) -> Element {
    let kind = if local { "local" } else { "remote" };
    let kind_label = if local {
        "本地节点"
    } else {
        "远程节点"
    };
    let icon = if local { "monitor" } else { "cloud" };
    let marker = match tone.as_str() {
        "auth" | "unready" => "!",
        "unknown" => "?",
        "offline" => "/",
        _ => "",
    };
    rsx! {
        button {
            r#type: "button",
            class: "project-node-button {tone}",
            "data-node-kind": kind,
            aria_label: format!("{project_name} · {kind_label} · {label} · 查看节点信息"),
            aria_haspopup: "dialog",
            title: format!("{kind_label} · {label} · 点击查看节点信息"),
            onclick: move |event| { event.stop_propagation(); onopen.call(()); },
            oncontextmenu: move |event| {
                event.prevent_default();
                event.stop_propagation();
                oncontext.call(());
            },
            Icon { name: icon }
            span {
                class: "project-node-status",
                "data-state": tone,
                aria_hidden: "true",
                "{marker}"
            }
        }
    }
}

fn reported(value: Option<&str>) -> String {
    value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("未报告")
        .to_owned()
}

fn capability_count(count: Option<usize>) -> String {
    match count {
        Some(count) if count > 0 => format!("{count} 项"),
        _ => "未报告".into(),
    }
}

fn auth_label(phase: &Phase, probe: &Probe) -> &'static str {
    if probe.loading
        || !matches!(phase, Phase::Online)
        || probe.error.is_some()
        || !probe.health.as_ref().is_some_and(|health| health.ok)
    {
        return if matches!(phase, Phase::PasswordRequired) {
            "需要登录"
        } else {
            "未知（当前未确认）"
        };
    }
    match &probe.auth {
        Some(auth) if !auth.required => "无需认证",
        Some(auth) if auth.authenticated => "已认证",
        Some(_) => "需要登录",
        None => "未知（当前未确认）",
    }
}

#[component]
pub fn NodeInfoDialog(
    project: Option<Project>,
    project_name: String,
    local: bool,
    address: String,
    phase: Phase,
    probe: Probe,
    descriptor: Option<AgentDescriptor>,
    capabilities: Option<Capabilities>,
    session_connection: Option<String>,
    onrefresh: EventHandler,
    onsettings: EventHandler,
    onclose: EventHandler,
) -> Element {
    let (tone, label) = status(&phase, &probe);
    let health = probe.health.as_ref();
    let identity = descriptor.as_ref().map(|descriptor| &descriptor.identity);
    let rows = [
        (
            "节点类型",
            if local {
                "本地节点".into()
            } else {
                "远程节点（含局域网）".into()
            },
        ),
        ("节点地址", safe_address(&address)),
        ("当前状态", label.into()),
        (
            "最近检查",
            if probe.checked_at.is_empty() {
                "尚未检查".into()
            } else {
                probe.checked_at.clone()
            },
        ),
        (
            "服务",
            reported(health.and_then(|health| health.service.as_deref())),
        ),
        (
            "版本",
            reported(
                health
                    .and_then(|health| health.version.as_deref())
                    .or_else(|| identity.and_then(|identity| identity.version.as_deref())),
            ),
        ),
        (
            "协议",
            // Transport identity is authoritative; health is the fallback, never
            // the capability API's potentially unrelated protocol/version.
            reported(
                identity
                    .map(|identity| identity.protocol.as_str())
                    .filter(|protocol| !protocol.trim().is_empty())
                    .or_else(|| health.and_then(|health| health.protocol.as_deref())),
            ),
        ),
        (
            "传输",
            reported(identity.map(|identity| identity.transport.as_str())),
        ),
        ("节点认证", auth_label(&phase, &probe).into()),
        (
            "可用命令",
            capability_count(
                capabilities
                    .as_ref()
                    .map(|caps| caps.supported_commands.len()),
            ),
        ),
        (
            "可用操作",
            capability_count(
                capabilities
                    .as_ref()
                    .map(|caps| caps.supported_actions.len()),
            ),
        ),
        (
            "项目",
            if project.is_some() {
                reported(Some(&project_name))
            } else {
                "独立会话（未关联项目）".into()
            },
        ),
        (
            "工作目录",
            reported(project.as_ref().map(|project| project.path.as_str())),
        ),
    ];
    let disabled = probe.loading || !matches!(phase, Phase::Online);
    let warning = if probe.error.as_deref() == Some(AUTH_ERROR) {
        Some(AUTH_ERROR)
    } else if probe.error.is_some() || matches!(phase, Phase::Failed(_)) {
        Some(HEALTH_ERROR)
    } else {
        None
    };
    rsx! {
        Modal {
            id: "project-node-info", title: "节点信息", class: "node-info-dialog", trap_focus: true, onclose,
            div { class: "modal-body node-info-body",
                p { class: "node-info-summary", role: "status", aria_live: "polite", "data-state": tone,
                    "{label}"
                }
                p { class: "node-info-note",
                    "云图标仅表示远程节点（包括局域网），不代表节点部署在云端。节点检查不代表会话连接或模型可用。"
                }
                if probe.loading {
                    p { class: "node-info-note", "正在检查；以下已缓存信息尚未重新确认。" }
                } else if probe.error.is_some() || !matches!(phase, Phase::Online) {
                    p { class: "node-info-note", "以下已缓存信息不代表当前节点可用。" }
                }
                if let Some(warning) = warning {
                    p { class: "node-info-warning", role: "alert", "{warning}" }
                }
                dl { class: "node-info-details",
                    for (name, value) in rows {
                        div { class: "node-info-row", key: "{name}",
                            dt { "{name}" }
                            dd { "{value}" }
                        }
                    }
                }
                if let Some(connection) = session_connection {
                    // Never echo session errors: they may contain URLs or tokens.
                    p { class: "node-info-note node-info-session",
                        strong { "会话连接：" }
                        {session_label(&connection)}
                        "（独立于节点健康检查）"
                    }
                }
            }
            div { class: "modal-footer node-info-actions",
                button { r#type: "button", class: "secondary", aria_label: "刷新节点状态", disabled,
                    onclick: move |_| onrefresh.call(()), "刷新节点状态"
                }
                button { r#type: "button", class: "secondary", onclick: move |_| onsettings.call(()), "节点设置" }
                button { r#type: "button", class: "primary", onclick: move |_| onclose.call(()), "关闭" }
            }
        }
    }
}

fn session_label(value: &str) -> &'static str {
    match value {
        "connected" | "online" | "已连接" => "已连接",
        "connecting" | "连接中" => "连接中",
        "disconnected" | "offline" | "未连接" => "未连接",
        "" => "未知",
        _ => "状态未确认，请在会话中查看详情",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> Probe {
        Probe {
            health: Some(Health {
                ok: true,
                service: None,
                version: None,
                protocol: None,
            }),
            auth: Some(AuthState {
                required: false,
                authenticated: false,
            }),
            ..Probe::default()
        }
    }

    #[test]
    fn phase_and_loading_outrank_cached_success() {
        let mut probe = ready();
        assert_eq!(status(&Phase::Online, &probe), ("online", "节点在线"));
        probe.loading = true;
        assert_eq!(status(&Phase::Online, &probe).0, "checking");
        assert_eq!(status(&Phase::Failed("secret".into()), &probe).0, "offline");
        assert_eq!(status(&Phase::Connecting, &probe).0, "connecting");
        assert_eq!(status(&Phase::Authenticating, &probe).0, "connecting");
        assert_eq!(status(&Phase::PasswordRequired, &probe).0, "auth");
        assert_eq!(status(&Phase::Idle, &probe).1, "节点未连接");
        assert_eq!(status(&Phase::Online, &Probe::default()).0, "connecting");
    }

    #[test]
    fn errors_and_unready_are_not_green() {
        let mut probe = ready();
        probe.error = Some(HEALTH_ERROR.into());
        assert_eq!(status(&Phase::Online, &probe).0, "offline");
        assert_eq!(auth_label(&Phase::Online, &probe), "未知（当前未确认）");
        probe.error = Some(AUTH_ERROR.into());
        assert_eq!(status(&Phase::Online, &probe).0, "unknown");
        probe.error = None;
        probe.health.as_mut().unwrap().ok = false;
        assert_eq!(status(&Phase::Online, &probe), ("unready", "节点未就绪"));
    }

    #[test]
    fn authentication_is_precise_and_neutral_without_requirement() {
        let mut probe = ready();
        assert_eq!(auth_label(&Phase::Online, &probe), "无需认证");
        probe.auth.as_mut().unwrap().required = true;
        assert_eq!(status(&Phase::Online, &probe).0, "auth");
        assert_eq!(auth_label(&Phase::Online, &probe), "需要登录");
        probe.auth.as_mut().unwrap().authenticated = true;
        assert_eq!(auth_label(&Phase::Online, &probe), "已认证");
        probe.auth = None;
        assert_eq!(status(&Phase::Online, &probe).0, "unknown");
    }

    #[test]
    fn addresses_strip_secrets_and_reject_ambiguous_input() {
        assert_eq!(
            safe_address("https://user:secret@Example.com:443/path?token=secret#secret"),
            "https://example.com"
        );
        assert_eq!(
            safe_address("HTTP://[::1]:3030/base?token=secret"),
            "http://[::1]:3030"
        );
        assert_eq!(safe_address("rpi-work://local/"), "rpi-work://local");
        for raw in [
            "secret",
            "javascript://secret",
            "file://secret",
            "https://host:secret",
            "https://host:0",
            "https://secret%40host",
            "https://secret\\host",
            "https://",
            "https://host\nsecret",
            "rpi-work://local?secret",
        ] {
            assert_eq!(safe_address(raw), "未报告", "accepted {raw:?}");
        }
    }

    #[test]
    fn local_classification_uses_typed_client_or_owned_address() {
        assert!(is_local(None, "rpi-work://local"));
        assert!(!is_local(None, "http://localhost:3030"));
        let remote = WorkspaceClient::new("http://localhost:3030").unwrap();
        assert!(!is_local(Some(&remote), "http://localhost:3030"));
        assert!(is_local(Some(&remote), "rpi-work://local"));
    }

    #[test]
    fn workspace_location_does_not_conflate_local_remote_cloud_and_online() {
        assert_eq!(
            workspace_location(true, "rpi-work://local"),
            ("local", "本机工作空间", "monitor")
        );
        for address in [
            "http://localhost:3031/",
            "http://192.168.3.27:3031/",
            "https://node.example/",
        ] {
            assert_eq!(
                workspace_location(false, address),
                ("remote", "远程工作空间", "cloud")
            );
        }
        assert_eq!(
            workspace_location(false, "about:blank"),
            ("unknown", "未选择工作空间", "layout")
        );
        assert!(
            !workspace_location(false, "https://node.example/")
                .1
                .contains("云端")
        );
    }

    #[test]
    fn scope_rejects_address_epoch_and_ticket_changes() {
        let scope = ("http://node", 2, 5);
        assert!(same_scope(scope, scope));
        assert!(!same_scope(scope, ("http://other", 2, 5)));
        assert!(!same_scope(scope, ("http://node", 3, 5)));
        assert!(!same_scope(scope, ("http://node", 2, 6)));
    }

    #[test]
    fn capability_counts_are_reported_only_when_nonempty() {
        assert_eq!(capability_count(None), "未报告");
        assert_eq!(capability_count(Some(0)), "未报告");
        assert_eq!(capability_count(Some(1)), "1 项");
        assert_eq!(capability_count(Some(142)), "142 项");
    }

    #[test]
    fn cached_auth_is_not_claimed_while_loading_or_disconnected() {
        let mut probe = ready();
        probe.auth = Some(AuthState {
            required: true,
            authenticated: true,
        });
        probe.loading = true;
        assert_eq!(auth_label(&Phase::Online, &probe), "未知（当前未确认）");
        probe.loading = false;
        assert_eq!(auth_label(&Phase::Idle, &probe), "未知（当前未确认）");
        assert_eq!(auth_label(&Phase::PasswordRequired, &probe), "需要登录");
    }

    #[test]
    fn session_errors_are_never_displayed_raw() {
        assert_eq!(session_label("connected"), "已连接");
        assert!(!session_label("https://user:secret@node?token=secret").contains("secret"));
    }
}
