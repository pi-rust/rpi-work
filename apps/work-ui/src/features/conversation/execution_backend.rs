use crate::shared::components::Modal;
use crate::shared::state::scoped_spawn;
use dioxus::{core::Task, prelude::*};
use work_client::{AgentBackendOption, SessionAgentBinding, WorkspaceClient};

pub const CSS: &str = include_str!("execution_backend.css");

// Execution-backend controls live in session/resource settings, never in the
// header or chat composer. Existing session bindings remain independent.
pub const COMPOSER_CONTROLS_VISIBLE: bool = false;

/// Selection is a creation preference, never a mutation of a running session.
#[component]
pub fn BackendMenu(
    options: Vec<AgentBackendOption>,
    selected: String,
    binding: Option<SessionAgentBinding>,
    has_session: bool,
    busy: bool,
    #[props(default)] inline: bool,
    onchange: EventHandler<String>,
) -> Element {
    let mut options = options;
    for (id, name) in [
        ("rpi", "RPI"),
        ("claude", "Claude"),
        ("codex", "Codex"),
        ("opencode", "OpenCode"),
    ] {
        if !options.iter().any(|option| option.id == id) {
            options.push(AgentBackendOption {
                id: id.into(),
                name: name.into(),
                available: false,
                reason: Some("当前节点尚未公布此后端，或该后端不可用。".into()),
            });
        }
    }
    let mut pending = use_signal(|| None::<String>);
    // A stale binding must never override the next session's creation preference.
    let binding = binding.filter(|_| has_session);
    let current = binding
        .as_ref()
        .map_or(selected, |binding| binding.backend.clone());
    rsx! {
        details { class: if inline { "execution-backend-menu execution-backend-inline" } else { "execution-backend-menu" }, open: inline,
            summary { "执行后端 · {current}" }
            div { class: "execution-backend-options",
                p { "执行后端与模型独立。新后端使用独立会话，不迁移上下文。" }
                if options.is_empty() { p { role: "status", "后端目录尚未就绪；新会话默认 RPI。" } }
                for option in options {
                    button {
                        r#type: "button", disabled: busy || !option.available,
                        aria_pressed: current == option.id,
                        onclick: { let id = option.id.clone(); let current = current.clone(); move |_| {
                            if id == current { return; }
                            if has_session { pending.set(Some(id.clone())); }
                            else { onchange.call(id.clone()); }
                        } },
                        "{option.name}"
                    }
                    if let Some(reason) = option.reason { p { class: "backend-reason", "{reason}" } }
                }
                p { class: "backend-reason", "仅可选择当前节点公布的可用后端。Agent 在节点宿主运行，不在浏览器或手机上启动；可用不代表已登录或模型调用成功。" }
                if let Some(binding) = binding {
                    p { class: "backend-binding", "当前会话绑定：{binding.backend}" }
                    if let Some(id) = binding.external_session_id { p { class: "backend-reason", "Agent session: {id}" } }
                }
            }
        }
        if let Some(id) = pending.read().clone() {
            Modal { id: "execution-backend-confirm", title: "使用其他执行后端创建新会话？", class: "backend-confirm",
                onclose: move |()| pending.set(None),
                p { "将使用 {id} 在同一项目创建新会话。原会话与未发送草稿保留；历史、权限和运行上下文不会迁移，也不会停止原会话。" }
                div { class: "dialog-actions",
                    button { disabled: busy, onclick: move |_| { pending.set(None); onchange.call(id.clone()); }, "确认创建新会话" }
                    button { onclick: move |_| pending.set(None), "取消" }
                }
            }
        }
    }
}

pub(crate) fn switch_locked(
    online: bool,
    busy: bool,
    has_session: bool,
    binding_known: bool,
) -> bool {
    !online || busy || (has_session && !binding_known)
}

/// ACP supplies real config ids and values; access goes through the node.
#[component]
pub fn NegotiatedConfig(
    shared_client: Signal<Option<WorkspaceClient>>,
    session_id: String,
    session_generation: Signal<u64>,
    requests: Signal<Vec<Task>>,
    idle: bool,
    #[props(default)] initial_configuration: Option<work_client::SessionConfiguration>,
) -> Element {
    let mut revision = use_signal(|| 0u64);
    let mut busy = use_signal(|| false);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut mutation_error = use_signal(String::new);
    let mut initialized = use_signal(|| false);
    let mut options = use_signal(Vec::<serde_json::Value>::new);
    let mut load_epoch = use_signal(|| 0u64);
    let read_id = session_id.clone();
    use_effect(move || {
        let _ = revision.read();
        let generation = *session_generation.read();
        let client = shared_client.read().clone();
        let id = read_id.clone();
        let epoch = load_epoch.peek().wrapping_add(1);
        load_epoch.set(epoch);
        options.set(Vec::new());
        error.set(String::new());
        loading.set(true);
        if !*initialized.peek() {
            initialized.set(true);
            if let Some(config) = initial_configuration.as_ref() {
                options.set(config.config_options.clone());
                loading.set(false);
                return;
            }
        }
        let Some(client) = client else {
            loading.set(false);
            return;
        };
        scoped_spawn(requests, async move {
            let result = client.session_configuration(&id).await;
            if *session_generation.read() != generation || *load_epoch.peek() != epoch {
                return;
            }
            match result {
                Ok(config) => options.set(config.config_options),
                Err(e) => error.set(format!("无法读取协商配置：{e}")),
            }
            loading.set(false);
        });
    });
    rsx! {
        NegotiatedConfigControls {
            options: options.read().clone(), idle, busy: *busy.read(), loading: *loading.read(), error: [error.read().clone(), mutation_error.read().clone()].into_iter().filter(|error| !error.is_empty()).collect::<Vec<_>>().join("；"),
            onselect: move |(id, value): (String, String)| {
                if !idle || *busy.peek() || *loading.peek() { return; }
                let Some(client) = shared_client.peek().clone() else { return; };
                let generation = *session_generation.peek();
                let session_id = session_id.clone();
                mutation_error.set(String::new());
                busy.set(true);
                scoped_spawn(requests, async move {
                    let result = client.set_session_config_option(&session_id, &id, serde_json::json!(value)).await;
                    if *session_generation.peek() != generation { return; }
                    busy.set(false);
                    match result {
                        Ok(_) => { mutation_error.set(String::new()); revision += 1; }
                        Err(e) => mutation_error.set(format!("配置更新失败：{e}")),
                    }
                });
            },
        }
    }
}

/// Presentational controls shared with the isolated in-memory UI harness.
#[component]
pub fn NegotiatedConfigControls(
    options: Vec<serde_json::Value>,
    idle: bool,
    busy: bool,
    loading: bool,
    error: String,
    onselect: EventHandler<(String, String)>,
) -> Element {
    rsx! {
        div { class: if options.is_empty() { "configuration-status" } else { "negotiated-config" },
            if loading { p { role: "status", "正在读取节点协商配置…" } }
            for config in options.iter() {
                if let (Some(id), Some(name), Some(values)) = (config["id"].as_str(), config["name"].as_str(), config["options"].as_array()) {
                    label { "{name}"
                        select {
                            aria_label: "ACP {name}",
                            value: config["currentValue"].as_str().unwrap_or_default(),
                            disabled: !idle || busy || loading,
                            onchange: { let id = id.to_owned(); move |event: Event<FormData>| {
                                if idle && !busy && !loading { onselect.call((id.clone(), event.value())); }
                            } },
                            for value in values {
                                if let Some(group) = value["options"].as_array() {
                                    optgroup { label: value["name"].as_str().unwrap_or_default(),
                                        for entry in group {
                                            if let Some(value_id) = entry["value"].as_str() {
                                                option { value: value_id, selected: config["currentValue"].as_str() == Some(value_id), {entry["name"].as_str().unwrap_or(value_id)} }
                                            }
                                        }
                                    }
                                } else if let Some(value_id) = value["value"].as_str() {
                                    option { value: value_id, selected: config["currentValue"].as_str() == Some(value_id), {value["name"].as_str().unwrap_or(value_id)} }
                                }
                            }
                        }
                    }
                }
            }
            if !idle { span { "运行期间不可更改配置" } }
            if !error.is_empty() { p { role: "alert", "{error}" } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backend_switch_fails_closed_for_unknown_binding_offline_and_creation_in_flight() {
        assert!(switch_locked(false, false, false, false));
        assert!(switch_locked(true, true, false, false));
        assert!(switch_locked(true, false, true, false));
        assert!(!switch_locked(true, false, true, true));
        assert!(!switch_locked(true, false, false, false));
        assert!(!COMPOSER_CONTROLS_VISIBLE);
    }
}
