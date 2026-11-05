use crate::shared::state::scoped_spawn;
use dioxus::{core::Task, prelude::*};
use work_client::{
    ClientError, ModelDescriptor, RuntimeState, SessionConfiguration, ToolCatalog, WorkspaceClient,
};

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ConfigurationAccess {
    pub models: bool,
    pub levels: bool,
    pub tools: bool,
    pub state: bool,
    pub summary: bool,
    pub compact: bool,
}

impl ConfigurationAccess {
    pub fn for_configuration(config: Option<&SessionConfiguration>) -> Self {
        Self::from_commands(|command| config.is_some_and(|config| config.supports_command(command)))
    }

    fn from_commands(enabled: impl Fn(&str) -> bool) -> Self {
        Self {
            models: enabled("get_available_models"),
            levels: enabled("get_available_thinking_levels"),
            tools: enabled("get_available_tools"),
            state: enabled("get_state"),
            summary: enabled("get_session_branch"),
            compact: enabled("compact"),
        }
    }
}

// Describe read conflicts without replaying a setter or exposing the response body.
fn configuration_read_error(error: &ClientError) -> String {
    if matches!(error, ClientError::Api { status, .. } if status.as_u16() == 409) {
        "节点当前状态与配置读取请求冲突（HTTP 409）。本次读取未修改会话，请稍后重新读取配置。".into()
    } else {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use work_core::CapabilitySet;

    #[test]
    fn read_conflict_is_actionable_and_does_not_echo_response_diagnostics() {
        let message = configuration_read_error(&ClientError::Api {
            status: "409".parse().unwrap(),
            message: "secret /private/path token".into(),
        });
        assert!(message.contains("HTTP 409"));
        assert!(message.contains("未修改会话"));
        assert!(message.contains("重新读取配置"));
        assert!(!message.contains("secret"));
        assert!(!message.contains("/private/path"));
    }

    #[test]
    fn non_conflict_read_failures_keep_their_category() {
        let error = ClientError::RequestTimeout("读取配置".into());
        assert_eq!(configuration_read_error(&error), error.to_string());
    }

    #[test]
    fn native_gating_uses_actual_operations_not_session_or_stream_support() {
        let acp = CapabilitySet::acp_default();
        assert!(acp.sessions && acp.streaming);
        assert_eq!(
            ConfigurationAccess::from_commands(|command| matches!(command, "prompt" | "abort")),
            ConfigurationAccess {
                models: false,
                levels: false,
                tools: false,
                state: false,
                summary: false,
                compact: false,
            }
        );
        let access = ConfigurationAccess::from_commands(|command| {
            matches!(command, "get_available_models" | "get_state")
        });
        assert!(access.models && access.state);
        assert!(!access.levels && !access.tools && !access.summary && !access.compact);
    }

    #[test]
    fn missing_observation_is_disabled_and_commands_are_session_scoped() {
        let disabled = ConfigurationAccess::from_commands(|_| false);
        assert_eq!(ConfigurationAccess::for_configuration(None), disabled);
        let config = SessionConfiguration {
            protocol: "acp".into(),
            supported_commands: vec!["get_state".into(), "get_available_models".into()],
            config_options: vec![serde_json::json!({"id":"model"})],
        };
        let access = ConfigurationAccess::for_configuration(Some(&config));
        assert!(access.models && access.state);
        assert!(!access.levels && !access.tools && !access.compact);
        assert!(!config.supports_command("set_model"));
        assert_eq!(
            ConfigurationAccess::for_configuration(Some(&SessionConfiguration {
                protocol: "acp".into(),
                supported_commands: vec![],
                config_options: vec![],
            })),
            disabled
        );
    }

    #[test]
    fn missing_configuration_never_infers_http_controls() {
        let access = ConfigurationAccess::for_configuration(None);
        assert!(!access.models && !access.levels && !access.tools);
        assert!(!access.state && !access.summary && !access.compact);
    }
}

#[component]
pub fn SessionConfig(
    shared_client: Signal<Option<WorkspaceClient>>,
    selected_session: Signal<Option<String>>,
    session_generation: Signal<u64>,
    requests: Signal<Vec<Task>>,
    mut runtime_state: Signal<Option<RuntimeState>>,
    mut status: Signal<String>,
    tools_only: bool,
    #[props(default)] config_idle: bool,
    oncompact: Option<EventHandler>,
) -> Element {
    let mut models = use_signal(Vec::<ModelDescriptor>::new);
    let mut levels = use_signal(Vec::<String>::new);
    let mut tools = use_signal(|| None::<ToolCatalog>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut configuration = use_signal(|| None::<SessionConfiguration>);
    // Keep the same ACP wrapper mounted during a same-context reload so its
    // in-flight setter guard survives. The observation itself is still cleared.
    let mut known_protocol = use_signal(|| None::<String>);
    let mut mutation_error = use_signal(String::new);
    let mut refresh_revision = use_signal(|| 0u64);
    let mut load_epoch = use_signal(|| 0u64);
    let mut observed_generation = use_signal(|| None::<u64>);
    let access = ConfigurationAccess::for_configuration(configuration.read().as_ref());
    use_effect(move || {
        let _ = refresh_revision.read();
        let generation = *session_generation.read();
        let epoch = load_epoch.peek().wrapping_add(1);
        load_epoch.set(epoch);
        if *observed_generation.peek() != Some(generation) {
            observed_generation.set(Some(generation));
            known_protocol.set(None);
            mutation_error.set(String::new());
            busy.set(false);
        }
        let id = selected_session.read().clone();
        let client = shared_client.read().clone();
        models.set(Vec::new());
        levels.set(Vec::new());
        tools.set(None);
        configuration.set(None);
        error.set(String::new());
        let (Some(id), Some(client)) = (id, client) else {
            loading.set(false);
            return;
        };
        loading.set(true);
        scoped_spawn(requests, async move {
            let config = match client.session_configuration(&id).await {
                Ok(config) => config,
                Err(e) => {
                    if *session_generation.read() == generation && *load_epoch.peek() == epoch {
                        error.set(format!("无法读取会话配置能力：{}", configuration_read_error(&e)));
                        loading.set(false);
                    }
                    return;
                }
            };
            let access = ConfigurationAccess::for_configuration(Some(&config));
            let model_result = if access.models {
                client.models(&id).await
            } else {
                Ok(Vec::new())
            };
            let level_result = if access.levels {
                client.thinking_levels(&id).await
            } else {
                Ok(Vec::new())
            };
            let tool_result = if access.tools {
                client.tools(&id).await.map(Some)
            } else {
                Ok(None)
            };
            if *session_generation.read() != generation || *load_epoch.peek() != epoch {
                return;
            }
            let mut errors = Vec::new();
            match model_result {
                Ok(items) => models.set(items),
                Err(e) => errors.push(format!("模型：{}", configuration_read_error(&e))),
            }
            match level_result {
                Ok(items) => levels.set(items),
                Err(e) => errors.push(format!("思考：{}", configuration_read_error(&e))),
            }
            match tool_result {
                Ok(items) => tools.set(items),
                Err(e) => errors.push(format!("工具：{}", configuration_read_error(&e))),
            }
            known_protocol.set(Some(config.protocol.clone()));
            configuration.set(Some(config));
            error.set(errors.join("；"));
            loading.set(false);
        });
    });
    let mut change = move |kind: &'static str, value: String| {
        if *busy.read() || !config_idle {
            return;
        }
        let (Some(id), Some(client)) = (
            selected_session.read().clone(),
            shared_client.read().clone(),
        ) else {
            return;
        };
        let generation = *session_generation.read();
        let epoch = *load_epoch.peek();
        let supported = configuration.read().as_ref().is_some_and(|config| {
            config.supports_command(match kind {
                "model" => "set_model",
                "thinking" => "set_thinking_level",
                "tools" => "set_active_tools",
                _ => "",
            })
        });
        if !supported {
            mutation_error.set("当前会话未提供此配置操作。".into());
            return;
        }
        let active = tools
            .read()
            .as_ref()
            .map(|catalog| {
                let mut active = catalog.active_tools.clone();
                if active.contains(&value) {
                    active.retain(|tool| tool != &value);
                } else {
                    active.push(value.clone());
                }
                active
            })
            .unwrap_or_default();
        mutation_error.set(String::new());
        busy.set(true);
        scoped_spawn(requests, async move {
            let result = match kind {
                "model" => {
                    client
                        .set_session_config_option(&id, "model", serde_json::json!(value))
                        .await
                }
                "thinking" => {
                    client
                        .set_session_config_option(&id, "thinking", serde_json::json!(value))
                        .await
                }
                "tools" => client.set_active_tools(&id, &active).await,
                _ => {
                    busy.set(false);
                    return;
                }
            };
            if *session_generation.read() != generation {
                return;
            }
            if *load_epoch.peek() != epoch {
                busy.set(false);
                match result {
                    Ok(_) => {
                        refresh_revision += 1;
                    }
                    Err(e) => mutation_error.set(format!("操作失败：{e}")),
                }
                return;
            }
            match result {
                Ok(_) => {
                    if !access.state {
                        busy.set(false);
                        status.set("会话配置已更新；当前 transport 不提供运行状态查询。".into());
                        mutation_error.set(String::new());
                        return;
                    }
                    let refreshed = client.runtime_state(&id).await;
                    if *session_generation.read() != generation {
                        return;
                    }
                    busy.set(false);
                    if *load_epoch.peek() != epoch {
                        return;
                    }
                    match refreshed {
                        Ok(state) => runtime_state.set(Some(state)),
                        Err(e) => {
                            mutation_error.set(format!("操作成功，刷新失败：{e}"));
                            return;
                        }
                    }
                    if kind == "tools" {
                        if let Some(catalog) = tools.write().as_mut() {
                            catalog.active_tools = active;
                        }
                    }
                    status.set("会话配置已更新".into());
                    mutation_error.set(String::new());
                }
                Err(e) => {
                    busy.set(false);
                    mutation_error.set(format!("操作失败：{e}"));
                }
            }
        });
    };
    rsx! {
        div { class: "session-config",
            if *loading.read() { p { class: "meta", "正在读取会话配置…" } }
            if configuration.read().is_some() && (!access.models || !access.levels || !access.tools) {
                p { class: "meta", "当前会话未提供的目录或配置操作已禁用；流式工具事件不表示可配置工具。ACP 使用节点实际协商的选项。" }
            }
            if !tools_only {
                if known_protocol.read().as_deref() == Some("acp") {
                    if let Some(id) = selected_session.read().clone() {
                        crate::features::conversation::execution_backend::NegotiatedConfig {
                            key: "session-config-{id}-{session_generation}", shared_client, session_id: id, session_generation, requests, idle: config_idle, initial_configuration: configuration.read().clone(),
                        }
                    }
                } else {
                div { class: "form-field", label { r#for: "session-model", "模型" }
                    select { id: "session-model", disabled: !config_idle || *busy.read() || models.read().is_empty() || !configuration.read().as_ref().is_some_and(|config| config.supports_command("set_model")),
                        value: runtime_state.read().as_ref().and_then(|r| r.model_id.clone()).unwrap_or_default(),
                        onchange: move |event| change("model", event.value()),
                        option { value: "", "—" }
                        for model in models.read().iter() { option { value: "{model.id}", "{model.name} · {model.provider}" } }
                    }
                }
                div { class: "form-field", label { r#for: "session-thinking", "思考级别" }
                    select { id: "session-thinking", disabled: !config_idle || *busy.read() || levels.read().is_empty() || !configuration.read().as_ref().is_some_and(|config| config.supports_command("set_thinking_level")),
                        value: runtime_state.read().as_ref().and_then(|r| r.thinking_level.clone()).unwrap_or_default(),
                        onchange: move |event| change("thinking", event.value()),
                        option { value: "", "—" }
                        for level in levels.read().iter() { option { value: "{level}", "{level}" } }
                    }
                }
                }
            }
            details { open: true, summary { "Tools · 节点实际状态" }
                if let Some(catalog) = tools.read().as_ref() {
                    p { class: "meta", "可用工具 {catalog.tools.len()} 项；已启用 {catalog.active_tools.len()} 项。下列勾选表示节点实际 active_tools。" }
                    for tool in catalog.tools.iter() {
                        { let name = tool.clone(); rsx! {
                            label { class: "checkbox-row", input { r#type: "checkbox", checked: catalog.active_tools.contains(tool), disabled: !config_idle || *busy.read() || !configuration.read().as_ref().is_some_and(|config| config.supports_command("set_active_tools")), onchange: move |_| change("tools", name.clone()) } "{tool}" }
                        } }
                    }
                } else if !access.tools { p { class: "meta", "当前 transport 不支持工具目录/配置。" } }
                else { p { class: "meta", "节点工具状态未知；连接会话后读取。" } }
            }
            if !tools_only {
                if let Some(oncompact) = oncompact {
                    button { class: "button", disabled: !access.compact || *busy.read() || selected_session.read().is_none(), onclick: move |_| oncompact.call(()), "压缩上下文" }
                }
            }
            if !error.read().is_empty() {
                p { role: "alert", class: "error-text", "{error}" }
                button { class: "button", disabled: *loading.read() || *busy.read(), onclick: move |_| refresh_revision += 1, "重新读取配置" }
            }
            if !mutation_error.read().is_empty() { p { role: "alert", class: "error-text", "{mutation_error}" } }
        }
    }
}
