use crate::shared::state::scoped_spawn;
use dioxus::{core::Task, prelude::*};
use work_client::{AgentClient, ModelDescriptor, RuntimeState, ToolCatalog, WorkspaceClient};

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeSessionAccess {
    pub models: bool,
    pub levels: bool,
    pub tools: bool,
    pub state: bool,
    pub summary: bool,
    pub compact: bool,
}

impl NativeSessionAccess {
    pub fn for_agent(agent: Option<&AgentClient>) -> Self {
        Self::from_commands(|command| agent.is_none_or(|agent| agent.supports_command(command)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use work_core::CapabilitySet;

    #[test]
    fn native_gating_uses_actual_operations_not_session_or_stream_support() {
        let acp = CapabilitySet::acp_default();
        assert!(acp.sessions && acp.streaming);
        assert_eq!(
            NativeSessionAccess::from_commands(|command| matches!(command, "prompt" | "abort")),
            NativeSessionAccess {
                models: false,
                levels: false,
                tools: false,
                state: false,
                summary: false,
                compact: false,
            }
        );
        let access = NativeSessionAccess::from_commands(|command| {
            matches!(command, "get_available_models" | "get_state")
        });
        assert!(access.models && access.state);
        assert!(!access.levels && !access.tools && !access.summary && !access.compact);
    }

    #[test]
    fn http_lane_keeps_existing_discovery_and_runtime_controls() {
        assert_eq!(
            NativeSessionAccess::for_agent(None),
            NativeSessionAccess {
                models: true,
                levels: true,
                tools: true,
                state: true,
                summary: true,
                compact: true,
            }
        );
    }
}

#[component]
pub fn SessionConfig(
    shared_client: Signal<Option<WorkspaceClient>>,
    native_agents: Signal<std::collections::HashMap<String, AgentClient>>,
    selected_session: Signal<Option<String>>,
    session_generation: Signal<u64>,
    requests: Signal<Vec<Task>>,
    mut runtime_state: Signal<Option<RuntimeState>>,
    mut status: Signal<String>,
    tools_only: bool,
    oncompact: Option<EventHandler>,
) -> Element {
    let mut models = use_signal(Vec::<ModelDescriptor>::new);
    let mut levels = use_signal(Vec::<String>::new);
    let mut tools = use_signal(|| None::<ToolCatalog>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let selected_native = selected_session
        .read()
        .as_ref()
        .and_then(|id| native_agents.read().get(id).cloned());
    let access = NativeSessionAccess::for_agent(selected_native.as_ref());
    use_effect(move || {
        let generation = *session_generation.read();
        let id = selected_session.read().clone();
        let client = shared_client.read().clone();
        let native_agent = id
            .as_ref()
            .and_then(|session_id| native_agents.read().get(session_id).cloned());
        let access = NativeSessionAccess::for_agent(native_agent.as_ref());
        models.set(Vec::new());
        levels.set(Vec::new());
        tools.set(None);
        error.set(String::new());
        let (Some(id), Some(client)) = (id, client) else {
            loading.set(false);
            return;
        };
        loading.set(true);
        scoped_spawn(requests, async move {
            let model_result = if !access.models {
                Ok(Vec::new())
            } else if let Some(agent) = native_agent.clone() {
                agent.models(&id).await.and_then(|value| {
                    serde_json::from_value(value["models"].clone())
                        .map_err(work_client::ClientError::from)
                })
            } else {
                client.models(&id).await
            };
            let level_result = if !access.levels {
                Ok(Vec::new())
            } else if let Some(agent) = native_agent.clone() {
                agent.thinking_levels(&id).await.and_then(|value| {
                    serde_json::from_value(value["levels"].clone())
                        .map_err(work_client::ClientError::from)
                })
            } else {
                client.thinking_levels(&id).await
            };
            let tool_result = if !access.tools {
                Ok(None)
            } else if let Some(agent) = native_agent {
                agent
                    .tools(&id)
                    .await
                    .and_then(|value| {
                        serde_json::from_value(value).map_err(work_client::ClientError::from)
                    })
                    .map(Some)
            } else {
                client.tools(&id).await.map(Some)
            };
            if *session_generation.read() != generation {
                return;
            }
            let mut errors = Vec::new();
            match model_result {
                Ok(items) => models.set(items),
                Err(e) => errors.push(format!("模型：{e}")),
            }
            match level_result {
                Ok(items) => levels.set(items),
                Err(e) => errors.push(format!("思考：{e}")),
            }
            match tool_result {
                Ok(items) => tools.set(items),
                Err(e) => errors.push(format!("工具：{e}")),
            }
            error.set(errors.join("；"));
            loading.set(false);
        });
    });
    let mut change = move |kind: &'static str, value: String| {
        if *busy.read() {
            return;
        }
        let (Some(id), Some(client)) = (
            selected_session.read().clone(),
            shared_client.read().clone(),
        ) else {
            return;
        };
        let generation = *session_generation.read();
        let native_agent = native_agents.read().get(&id).cloned();
        if let Some(agent) = native_agent.as_ref() {
            let supported = match kind {
                "model" | "thinking" => agent.supports_config_option(&id, kind),
                "tools" => agent.supports_command("set_active_tools"),
                _ => false,
            };
            if !supported {
                error.set(format!(
                    "当前 transport {} 不支持此配置操作。",
                    agent.descriptor().identity.protocol
                ));
                return;
            }
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
        busy.set(true);
        scoped_spawn(requests, async move {
            let result = match kind {
                "model" => {
                    if let Some(agent) = native_agent.clone() {
                        agent
                            .set_config_option(&id, "model", serde_json::json!(value))
                            .await
                    } else {
                        client.set_model(&id, &value).await
                    }
                }
                "thinking" => {
                    if let Some(agent) = native_agent.clone() {
                        agent
                            .set_config_option(&id, "thinking", serde_json::json!(value))
                            .await
                    } else {
                        client.set_thinking_level(&id, &value).await
                    }
                }
                "tools" => {
                    if let Some(agent) = native_agent.clone() {
                        agent.set_active_tools(&id, active.clone()).await
                    } else {
                        client.set_active_tools(&id, &active).await
                    }
                }
                _ => {
                    busy.set(false);
                    return;
                }
            };
            if *session_generation.read() != generation {
                return;
            }
            match result {
                Ok(_) => {
                    if native_agent
                        .as_ref()
                        .is_some_and(|agent| !agent.supports_command("get_state"))
                    {
                        busy.set(false);
                        status.set("会话配置已更新；当前 transport 不提供运行状态查询。".into());
                        error.set(String::new());
                        return;
                    }
                    let refreshed = if let Some(agent) = native_agent {
                        agent.state(&id).await.and_then(|value| {
                            serde_json::from_value(value).map_err(work_client::ClientError::from)
                        })
                    } else {
                        client.runtime_state(&id).await
                    };
                    if *session_generation.read() != generation {
                        return;
                    }
                    busy.set(false);
                    match refreshed {
                        Ok(state) => runtime_state.set(Some(state)),
                        Err(e) => {
                            error.set(format!("操作成功，刷新失败：{e}"));
                            return;
                        }
                    }
                    if kind == "tools" {
                        if let Some(catalog) = tools.write().as_mut() {
                            catalog.active_tools = active;
                        }
                    }
                    status.set("会话配置已更新".into());
                    error.set(String::new());
                }
                Err(e) => {
                    busy.set(false);
                    error.set(format!("操作失败：{e}"));
                }
            }
        });
    };
    rsx! {
        div { class: "session-config",
            if *loading.read() { p { class: "meta", "正在读取会话配置…" } }
            if selected_native.is_some() && (!access.models || !access.levels || !access.tools) {
                p { class: "meta", "当前原生 transport 未提供的模型/思考选项目录或工具配置已禁用；流式工具事件不表示可配置工具。ACP 显式配置 setter 仍可通过 facade/CLI 使用。" }
            }
            if !tools_only {
                div { class: "form-field", label { r#for: "session-model", "模型" }
                    select { id: "session-model", disabled: *busy.read() || models.read().is_empty(),
                        value: runtime_state.read().as_ref().and_then(|r| r.model_id.clone()).unwrap_or_default(),
                        onchange: move |event| change("model", event.value()),
                        option { value: "", "—" }
                        for model in models.read().iter() { option { value: "{model.id}", "{model.name} · {model.provider}" } }
                    }
                }
                div { class: "form-field", label { r#for: "session-thinking", "思考级别" }
                    select { id: "session-thinking", disabled: *busy.read() || levels.read().is_empty(),
                        value: runtime_state.read().as_ref().and_then(|r| r.thinking_level.clone()).unwrap_or_default(),
                        onchange: move |event| change("thinking", event.value()),
                        option { value: "", "—" }
                        for level in levels.read().iter() { option { value: "{level}", "{level}" } }
                    }
                }
            }
            details { open: true, summary { "Tools · 节点实际状态" }
                if let Some(catalog) = tools.read().as_ref() {
                    p { class: "meta", "可用工具 {catalog.tools.len()} 项；已启用 {catalog.active_tools.len()} 项。下列勾选表示节点实际 active_tools。" }
                    for tool in catalog.tools.iter() {
                        { let name = tool.clone(); rsx! {
                            label { class: "checkbox-row", input { r#type: "checkbox", checked: catalog.active_tools.contains(tool), disabled: *busy.read(), onchange: move |_| change("tools", name.clone()) } "{tool}" }
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
            if !error.read().is_empty() { p { role: "alert", class: "error-text", "{error}" } }
        }
    }
}
