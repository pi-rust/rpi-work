use crate::state::scoped_spawn;
use dioxus::{core::Task, prelude::*};
use work_client::{ModelDescriptor, RuntimeState, ToolCatalog, WorkClient};

#[component]
pub fn SessionConfig(
    shared_client: Signal<Option<WorkClient>>,
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
    use_effect(move || {
        let generation = *session_generation.read();
        let id = selected_session.read().clone();
        let client = shared_client.read().clone();
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
            let model_result = client.models(&id).await;
            let level_result = client.thinking_levels(&id).await;
            let tool_result = client.tools(&id).await;
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
                Ok(items) => tools.set(Some(items)),
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
                "model" => client.set_model(&id, &value).await,
                "thinking" => client.set_thinking_level(&id, &value).await,
                "tools" => client.set_active_tools(&id, &active).await,
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
                    let refreshed = client.runtime_state(&id).await;
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
                } else { p { class: "meta", "节点工具状态未知；连接会话后读取。" } }
            }
            if !tools_only {
                if let Some(oncompact) = oncompact {
                    button { class: "button", disabled: *busy.read() || selected_session.read().is_none(), onclick: move |_| oncompact.call(()), "压缩上下文" }
                }
            }
            if !error.read().is_empty() { p { role: "alert", class: "error-text", "{error}" } }
        }
    }
}
