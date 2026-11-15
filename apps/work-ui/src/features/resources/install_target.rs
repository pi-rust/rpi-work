use crate::app::{connections, local_node, node_navigation::NodeNavigation};
use dioxus::prelude::*;
use work_client::WorkspaceClient;

#[derive(Clone)]
pub(super) struct InstallTarget {
    pub address: String,
    pub client: Option<WorkspaceClient>,
    pub projects: Vec<(String, String)>,
    pub loading: bool,
    pub password_required: bool,
    pub error: String,
    revision: u64,
}

impl InstallTarget {
    pub fn ready(&self) -> bool {
        self.client.is_some() && !self.loading && !self.password_required && self.error.is_empty()
    }
}

pub(super) fn current_address() -> String {
    use_context::<connections::ConnectionStore>()
        .address
        .read()
        .trim_end_matches('/')
        .to_owned()
}

pub(super) fn use_install_target(
    projects: Vec<(String, String)>,
    client: Signal<Option<WorkspaceClient>>,
    online: bool,
) -> Signal<InstallTarget> {
    let address = current_address();
    use_signal(|| InstallTarget {
        address,
        client: if online { client.peek().clone() } else { None },
        projects,
        loading: false,
        password_required: false,
        error: if online {
            String::new()
        } else {
            "节点离线，请重新连接后继续。".into()
        },
        revision: 0,
    })
}

#[component]
pub(super) fn TargetPicker(
    mut target: Signal<InstallTarget>,
    mut project: Signal<String>,
    current_projects: Vec<(String, String)>,
    shared_client: Signal<Option<WorkspaceClient>>,
    online: bool,
    disabled: bool,
    fixed: bool,
    onchanged: EventHandler<()>,
) -> Element {
    let current = current_address();
    let store = use_context::<connections::ConnectionStore>();
    let navigation = use_hook(try_consume_context::<NodeNavigation>);
    let mut custom = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut retry = use_signal(|| 0u64);
    let destination = use_memo(move || {
        let state = target.read();
        (state.address.clone(), state.revision)
    });
    let mut options = vec![current.clone()];
    if local_node::client().is_some() && !options.iter().any(|a| local_node::is_local(a)) {
        options.push(local_node::LOCAL_ADDRESS.into());
    }
    for address in &store.history.read().recent {
        let address = address.trim_end_matches('/').to_owned();
        if !options.contains(&address) {
            options.push(address);
        }
    }
    let selected = target.read().address.clone();
    if !options.contains(&selected) {
        options.push(selected.clone());
    }
    let current_effect = current.clone();
    use_effect(move || {
        let _ = retry.read();
        let (selected, revision) = destination();
        if selected == current_effect {
            return;
        }
        // Hosted browsers authenticate and operate at the target's own origin.
        if navigation.is_some() {
            return;
        }
        let candidate = target.peek().client.clone().map_or_else(
            || {
                if local_node::is_local(&selected) {
                    local_node::client().ok_or_else(|| "本机节点尚未就绪。".to_owned())
                } else {
                    WorkspaceClient::new(&selected).map_err(|error| error.to_string())
                }
            },
            Ok,
        );
        let client = match candidate {
            Ok(client) => client,
            Err(error) => {
                target.write().error = error;
                return;
            }
        };
        let secret = std::mem::take(&mut *password.write());
        target.write().loading = true;
        spawn(async move {
            let result = async {
                #[cfg(target_os = "android")]
                let (client, secret) = {
                    if secret.is_empty() {
                        return Ok((
                            client,
                            connections::ConnectionOutcome::PasswordRequired(
                                work_client::AuthState {
                                    required: true,
                                    authenticated: false,
                                },
                            ),
                        ));
                    }
                    let node = crate::app::connection_auth::manual_prepare(
                        &selected,
                        connections::NodePassword::new(secret, &selected),
                    )?
                    .login()
                    .await?;
                    (node.client, None)
                };
                #[cfg(not(target_os = "android"))]
                let secret = (!secret.is_empty()).then_some(secret);
                let outcome = connections::open(client.clone(), secret).await?;
                Ok::<_, String>((client, outcome))
            }
            .await;
            if target.peek().address != selected || target.peek().revision != revision {
                return;
            }
            let mut state = target.write();
            state.loading = false;
            match result {
                Ok((client, connections::ConnectionOutcome::Ready(workspace))) => {
                    state.client = Some(client);
                    state.password_required = false;
                    state.error.clear();
                    state.projects = workspace
                        .projects
                        .into_iter()
                        .map(|p| (p.id, p.name))
                        .collect();
                    project.set(
                        state
                            .projects
                            .first()
                            .map(|(id, _)| id.clone())
                            .unwrap_or_default(),
                    );
                }
                Ok((client, connections::ConnectionOutcome::PasswordRequired(_))) => {
                    state.client = Some(client);
                    state.password_required = true;
                    state.error.clear();
                }
                Err(error) => state.error = error,
            }
        });
    });
    let select = Callback::new(move |address: String| {
        if disabled || fixed {
            return;
        }
        if address != current && !local_node::is_local(&address) {
            if let Err(error) =
                connections::normalize_address(&address, cfg!(target_os = "android"))
            {
                target.write().error = error;
                return;
            }
        }
        password.set(String::new());
        let same = address == current;
        project.set(if same {
            current_projects
                .first()
                .map(|(id, _)| id.clone())
                .unwrap_or_default()
        } else {
            String::new()
        });
        let revision = target.peek().revision.wrapping_add(1);
        target.set(InstallTarget {
            address,
            client: if same && online {
                shared_client.peek().clone()
            } else {
                None
            },
            projects: if same {
                current_projects.clone()
            } else {
                vec![]
            },
            loading: false,
            password_required: false,
            error: String::new(),
            revision,
        });
        onchanged.call(());
    });
    rsx! {
        label { class: "resource-flow-field", "安装节点"
            select { aria_label: "安装节点", value: "{selected}", disabled: disabled || fixed,
                onchange: move |event| select.call(event.value()),
                for address in options {
                    option { value: "{address}", if local_node::is_local(&address) { "本机" } else { "{address}" } }
                }
            }
        }
        if fixed {
            p { class: "resource-flow-note", "工作准备中的资源安装到工作项目所属节点。" }
        } else {
            details {
                summary { "其他节点" }
                label { class: "resource-flow-field", "节点地址"
                    input { aria_label: "安装节点地址", r#type: "url", value: "{custom}", placeholder: "https://节点地址:端口", disabled,
                        oninput: move |event| custom.set(event.value()) }
                }
                button { class: "button", disabled: disabled || custom().trim().is_empty(),
                    onclick: move |_| {
                        match connections::normalize_address(&custom.peek(), cfg!(target_os = "android")) {
                            Ok(address) => select.call(address),
                            Err(error) => target.write().error = error,
                        }
                    }, "选择此节点"
                }
            }
        }
        if target.read().loading { p { role: "status", "正在连接目标节点并读取项目…" } }
        if target.read().ready() && target.read().projects.is_empty() {
            p { class: "resource-flow-note", role: "status", "目标节点还没有项目，请先在该节点添加项目后安装。" }
        }
        if target.read().password_required {
            label { class: "resource-flow-field", "目标节点密码"
                input { aria_label: "目标节点密码", r#type: "password", value: "{password}", disabled,
                    oninput: move |event| password.set(event.value()) }
            }
        }
        if !target.read().error.is_empty() { p { class: "resource-error", role: "alert", "{target.read().error}" } }
        if navigation.is_none() && (!target.read().error.is_empty() || target.read().password_required) {
            button { class: "button", disabled: disabled || target.read().loading,
                onclick: move |_| { target.write().error.clear(); retry += 1; },
                if target.read().password_required { "登录目标节点" } else { "重试连接目标节点" }
            }
        }
    }
}
