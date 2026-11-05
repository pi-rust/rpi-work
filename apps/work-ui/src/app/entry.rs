use dioxus::prelude::*;
use work_client::WorkspaceClient;

/// Host adapters provide operations; Settings never chooses a platform flow.
#[derive(Clone, Copy)]
pub(crate) struct ConnectionSessionControl {
    /// Native entries can own reauthorization; ordinary node selection stays in the workspace.
    pub restart: Option<EventHandler<()>>,
    pub forget: EventHandler<()>,
    pub warning: Signal<String>,
    pub busy: Signal<bool>,
}

/// The authenticated client must survive entry → workspace (including Android's cookie jar).
#[derive(Clone)]
pub(crate) struct EntryNode {
    pub address: String,
    pub client: WorkspaceClient,
}

// Entry mounts a workspace once; props identity is the node address, not its transport.
impl PartialEq for EntryNode {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
    }
}

#[component]
pub(crate) fn AuthorizedWorkspace(node: EntryNode) -> Element {
    use_context_provider(|| node);
    rsx! { super::workspace::App {} }
}

#[component]
pub(super) fn Root() -> Element {
    #[cfg(target_os = "android")]
    return rsx! { super::mobile_entry::MobileEntry {} };

    #[cfg(all(target_arch = "wasm32", not(target_os = "android")))]
    return rsx! { HostedEntry {} };

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    rsx! { super::workspace::App {} }
}

/// Only a directly opened static entry may opt out with an actual 404. A selected
/// Web target must identify its owner, never fall back to standalone node history.
/// A static server's 200 HTML SPA fallback is NOT evidence of independent UI.
#[cfg(any(target_arch = "wasm32", test))]
fn hosted_metadata(status: u16, body: &str, navigated: bool) -> Result<Option<String>, String> {
    if status == 404 {
        return if navigated {
            Err(
                "目标未提供宿主 Web 入口，请配置 Web assets 或同源代理；可使用浏览器返回原入口。"
                    .into(),
            )
        } else {
            Ok(None)
        };
    }
    if status != 200 {
        return Err("无法确认宿主入口，请重试。".into());
    }
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|_| "宿主入口信息无效，请重试。".to_owned())?;
    if value["version"].as_u64() != Some(1) || value["mode"].as_str() != Some("hosted") {
        return Err("宿主入口版本或模式无效，请重试。".into());
    }
    value["nodeId"]
        .as_str()
        .filter(|id| !id.trim().is_empty())
        .map(|id| Some(id.to_owned()))
        .ok_or_else(|| "宿主入口缺少节点标识，请重试。".into())
}

#[cfg(any(target_arch = "wasm32", test))]
fn hosted_authorized(auth: &work_client::AuthState) -> bool {
    // Hosted UI never treats required=false as permission to bypass login.
    auth.authenticated
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
enum WebGate {
    Independent,
    Login(EntryNode),
    Authorized(EntryNode),
}

#[cfg(target_arch = "wasm32")]
async fn discover() -> Result<WebGate, String> {
    use super::connections::with_timeout;

    // No workbench/session request occurs here. The browser owns cookies;
    // discovery is same-origin, uncached, bounded and never follows redirects.
    let mut request = document::eval(
        r#"
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), 12000);
        try {
            const response = await fetch('/.well-known/rpi-work-entry', {
                credentials: 'same-origin', cache: 'no-store', redirect: 'error',
                signal: controller.signal
            });
            const body = response.status === 404 ? '' : await response.text();
            dioxus.send({status: response.status, body, origin: location.origin,
                navigated: new URLSearchParams(location.hash.slice(1)).has('rpi-entry')});
        } catch (_) {
            dioxus.send({error: true});
        } finally {
            clearTimeout(timer);
        }
        "#,
    );
    let response = with_timeout(request.recv::<serde_json::Value>())
        .await?
        .map_err(|_| "无法读取宿主入口，请重试。".to_owned())?;
    let status = response["status"]
        .as_u64()
        .and_then(|status| u16::try_from(status).ok())
        .ok_or_else(|| "无法连接宿主，请检查网络后重试。".to_owned())?;
    let body = response["body"]
        .as_str()
        .ok_or_else(|| "宿主入口信息无效，请重试。".to_owned())?;
    let Some(_node_id) = hosted_metadata(
        status,
        body,
        response["navigated"].as_bool().unwrap_or(false),
    )?
    else {
        return Ok(WebGate::Independent);
    };
    let address = response["origin"]
        .as_str()
        .ok_or_else(|| "无法确定宿主地址。".to_owned())?
        .to_owned();
    let client = work_client::WorkClient::new(&address).map_err(|_| "宿主地址无效。".to_owned())?;
    let auth = with_timeout(client.auth())
        .await?
        .map_err(|error| match error {
            work_client::ClientError::Api { status, .. } if status.as_u16() == 403 => {
                "宿主未开启共享或拒绝访问，请在宿主桌面检查共享设置。".to_owned()
            }
            _ => "无法验证宿主登录状态，请重试。".to_owned(),
        })?;
    let node = EntryNode {
        address,
        client: client.into(),
    };
    Ok(if hosted_authorized(&auth) {
        WebGate::Authorized(node)
    } else {
        WebGate::Login(node)
    })
}

#[cfg(target_arch = "wasm32")]
#[component]
fn HostedEntry() -> Element {
    let mut attempt = use_signal(|| 0_u64);
    let gate = use_resource(move || {
        let _ = attempt();
        discover()
    });
    let result = gate.read_unchecked().clone();
    match result {
        Some(Ok(WebGate::Independent)) => rsx! { super::workspace::App {} },
        Some(Ok(WebGate::Authorized(node))) => {
            rsx! { HostedLogin { node, initially_authorized: true } }
        }
        Some(Ok(WebGate::Login(node))) => rsx! { HostedLogin { node } },
        other => {
            let error = other.and_then(Result::err);
            rsx! {
                EntryFrame {
                    h1 { "登录宿主" }
                    if let Some(message) = error {
                        p { role: "alert", class: "entry-error", "{message}" }
                        button {
                            r#type: "button",
                            onclick: move |_| attempt += 1,
                            "重试"
                        }
                    } else {
                        p { role: "status", "aria-live": "polite", "aria-busy": "true", "正在验证宿主入口…" }
                    }
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[component]
fn HostedLogin(node: EntryNode, #[props(default = false)] initially_authorized: bool) -> Element {
    use crate::features::settings::connection_panel::ConnectionScreen;
    let mut password = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut warning = use_signal(String::new);
    let mut authorized = use_signal(|| initially_authorized);
    let mut generation = use_signal(|| 0_u64);
    let mut login_task = use_signal(|| None::<dioxus::core::Task>);
    let client = node.client.clone();
    super::node_navigation::use_navigation(node.address.clone(), warning);
    use_context_provider(move || ConnectionSessionControl {
        // Fixed initial login is not a permanent restriction on node selection.
        restart: None,
        forget: EventHandler::new(move |()| {
            if *busy.peek() {
                return;
            }
            generation += 1;
            let attempt = *generation.peek();
            let client = client.clone();
            busy.set(true);
            warning.set(String::new());
            spawn(async move {
                let result = super::connections::with_timeout(client.logout()).await;
                if *generation.peek() != attempt {
                    return;
                }
                busy.set(false);
                match result {
                    Ok(Ok(auth)) if !auth.authenticated => {
                        password.set(String::new());
                        error.set(String::new());
                        authorized.set(false);
                    }
                    _ => warning.set("未能确认退出，请检查网络后重试；当前登录尚未清除。".into()),
                }
            });
        }),
        warning,
        busy,
    });
    if authorized() {
        return rsx! { AuthorizedWorkspace { node } };
    }
    let address = node.address.clone();
    rsx! {
        ConnectionEntryFrame {
            ConnectionScreen {
                address, fixed_address: true, fixed_target: true,
                password: password(), password_label: "宿主密码", submit_label: "登录宿主",
                help: "此页面由当前电脑托管，登录目标固定为当前宿主。请输入宿主密码，登录后访问工作台。".to_owned(),
                busy: busy(), error: error(), warning: warning(),
                onaddress: move |_| {},
                onpassword: move |value| { password.set(value); error.set(String::new()); },
                oncancel_attempt: move |()| {
                    generation += 1;
                    if let Some(task) = login_task.write().take() { task.cancel(); }
                    password.set(String::new()); busy.set(false); error.set(String::new());
                    warning.set("已取消等待连接；这不代表电脑已撤销登录处理，可重新连接。".into());
                },
                onsubmit: move |()| {
                    if busy() || password.peek().is_empty() { return; }
                    let secret = std::mem::take(&mut *password.write());
                    let client = node.client.clone();
                    generation += 1;
                    let attempt = *generation.peek();
                    busy.set(true); error.set(String::new()); warning.set(String::new());
                    login_task.set(Some(spawn(async move {
                        let result = super::connections::with_timeout(client.login(&secret)).await;
                        drop(secret);
                        if *generation.peek() != attempt { return; }
                        busy.set(false);
                        match result {
                            Ok(Ok(auth)) if hosted_authorized(&auth) => authorized.set(true),
                            Ok(Ok(_)) => error.set("宿主尚未确认登录，请检查密码与宿主认证配置后重试。".into()),
                            Ok(Err(_)) => error.set("登录失败，请检查宿主密码后重试。".into()),
                            Err(_) => error.set("登录超时，请检查网络后重试。".into()),
                        }
                    })));
                },
            }
        }
    }
}

/// Native and hosted Web mount exactly the same responsive entry shell.
#[cfg(any(target_os = "android", target_arch = "wasm32", test))]
#[component]
pub(crate) fn ConnectionEntryFrame(children: Element) -> Element {
    let appearance = crate::shared::theme::appearance::use_appearance();
    rsx! {
        style { {crate::shared::theme::appearance::STYLESHEETS} }
        style { {include_str!("../shared/theme/styles.css")} }
        style { {include_str!("entry.css")} }
        main { class: "connection-entry", "data-theme": appearance.read().theme.id(), "data-mode": appearance.read().mode,
            {children}
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[component]
fn EntryFrame(children: Element) -> Element {
    rsx! {
        ConnectionEntryFrame {
            crate::features::settings::connection_panel::ConnectionPanel { fixed_address: true, {children} }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_strict_404_is_independent() {
        assert_eq!(hosted_metadata(404, "not found", false).unwrap(), None);
        assert!(
            hosted_metadata(404, "not found", true).is_err(),
            "selected Web target must not fall back to standalone history"
        );
        for status in [0, 201, 204, 301, 401, 403, 500, 503] {
            assert!(hosted_metadata(status, "", false).is_err());
        }
        assert!(hosted_metadata(200, "<!doctype html><html></html>", false).is_err());
    }

    #[test]
    fn hosted_metadata_requires_version_mode_and_node_identity() {
        assert_eq!(
            hosted_metadata(
                200,
                r#"{"version":1,"mode":"hosted","nodeId":"node-1"}"#,
                true
            )
            .unwrap(),
            Some("node-1".into())
        );
        for body in [
            "null",
            "{}",
            "[]",
            r#"{"version":2,"mode":"hosted","nodeId":"n"}"#,
            r#"{"version":1,"mode":"independent","nodeId":"n"}"#,
            r#"{"version":1,"mode":"hosted","nodeId":" "}"#,
            r#"{"version":1,"mode":"hosted"}"#,
        ] {
            assert!(hosted_metadata(200, body, false).is_err(), "{body}");
        }
    }

    #[test]
    fn hosted_auth_cannot_downgrade_to_password_not_required() {
        for required in [false, true] {
            for authenticated in [false, true] {
                assert_eq!(
                    hosted_authorized(&work_client::AuthState {
                        required,
                        authenticated
                    }),
                    authenticated
                );
            }
        }
    }
}
