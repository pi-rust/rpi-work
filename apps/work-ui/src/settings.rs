use crate::components::Icon;
use dioxus::core::Task;
use dioxus::prelude::*;
use work_client::{AuthState, AuthStatus, Capabilities, ModelDescriptor, RuntimeState, WorkClient};

#[path = "devices.rs"]
mod devices;
use devices::DevicesPage;

#[component]
pub fn SettingsPage(
    shared_client: Signal<Option<WorkClient>>,
    requests: Signal<Vec<Task>>,
    active_page: Signal<String>,
    settings_section: Signal<String>,
    base_url: Signal<String>,
    status: Signal<String>,
    auth_state: Signal<Option<AuthState>>,
    password: Signal<String>,
    runtime_state: Signal<Option<RuntimeState>>,
    capabilities: Signal<Option<Capabilities>>,
    selected_session: Signal<Option<String>>,
    session_generation: Signal<u64>,
    onconnect: EventHandler<MouseEvent>,
    onlogin: EventHandler<MouseEvent>,
    onappearance: EventHandler<()>,
) -> Element {
    let section = settings_section.read().clone();
    rsx! {
        style { {include_str!("settings.css")} }
        section { class: "page-view settings-page",
            div { class: "settings-navigation",
                nav { aria_label: "设置分类",
                    for (id, icon, title) in [("settings", "settings", "通用"), ("devices", "monitor", "执行节点"), ("providers", "server", "Providers")] {
                        button { class: if section == id { "active" } else { "" }, aria_current: if section == id { "page" } else { "" },
                            onclick: move |_| settings_section.set(id.to_owned()), Icon { name: icon.to_owned() } "{title}" }
                    }
                }
                button { class: "settings-return", onclick: move |_| active_page.set("workspace".to_owned()), Icon { name: "chevron-right" } "返回会话" }
            }
            if section == "settings" {
                div { class: "page-heading", div { h1 { "设置" } p { "管理工作空间偏好、执行节点与模型连接。" } } }
                div { class: "settings-cards",
                    button { class: "settings-card", onclick: move |_| onappearance.call(()),
                        span { class: "settings-card-icon", Icon { name: "sparkles" } }
                        strong { "外观与主题" } p { "主题风格、明暗模式与界面密度。" } small { "本设备" } Icon { name: "chevron-right" }
                    }
                    button { class: "settings-card", onclick: move |_| settings_section.set("devices".to_owned()),
                        span { class: "settings-card-icon", Icon { name: "monitor" } }
                        strong { "执行节点" } p { "电脑服务端、手机客户端与节点连接。" } small { "电脑与手机连接" } Icon { name: "chevron-right" }
                    }
                    button { class: "settings-card", onclick: move |_| settings_section.set("providers".to_owned()),
                        span { class: "settings-card-icon", Icon { name: "server" } }
                        strong { "Providers" } p { "管理模型服务连接、认证与可用模型。" } small { "连接配置" } Icon { name: "chevron-right" }
                    }
                }
            } else if section == "devices" {
                DevicesPage { shared_client, requests, base_url, status, auth_state, password, capabilities, onconnect, onlogin }
            } else {
                ProviderSettings { shared_client, requests, runtime_state, capabilities, selected_session, session_generation, status, onreturn: move |_| active_page.set("workspace".to_owned()) }
            }
        }
    }
}

fn auth_label(auth: &work_client::AuthStatusProvider) -> &'static str {
    match auth.status.as_deref() {
        Some("anonymous") => "匿名 / 无需凭据",
        Some("missing") => "未配置凭据",
        Some("expired") => "凭据已过期",
        _ if auth.ready => "凭据就绪",
        _ => "认证状态未报告",
    }
}

fn supports(capabilities: Signal<Option<Capabilities>>, command: &str) -> bool {
    capabilities
        .read()
        .as_ref()
        .is_some_and(|caps| caps.supported_commands.iter().any(|item| item == command))
}

fn provider_name(provider: &str) -> &str {
    match provider {
        "deepseek" => "DeepSeek",
        "openai" => "OpenAI",
        other => other,
    }
}

#[component]
fn ProviderSettings(
    shared_client: Signal<Option<WorkClient>>,
    requests: Signal<Vec<Task>>,
    mut runtime_state: Signal<Option<RuntimeState>>,
    capabilities: Signal<Option<Capabilities>>,
    selected_session: Signal<Option<String>>,
    session_generation: Signal<u64>,
    mut status: Signal<String>,
    onreturn: EventHandler<MouseEvent>,
) -> Element {
    let mut models = use_signal(Vec::<ModelDescriptor>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut api_key = use_signal(String::new);
    let mut key_edited = use_signal(|| false);
    let mut provider_draft = use_signal(String::new);
    let mut base_url_draft = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let mut restart_required = use_signal(|| false);
    let mut auth_status = use_signal(AuthStatus::default);
    let mut config_open = use_signal(|| false);
    let mut model_open = use_signal(|| false);
    let mut model_provider = use_signal(String::new);
    let mut model_draft = use_signal(String::new);
    let mut model_revision = use_signal(|| 0u64);
    let mut model_busy = use_signal(|| false);
    let mut refresh_error = use_signal(String::new);
    let mut config_revision = use_signal(|| 0u64);
    let mut refresh_revision = use_signal(|| 0u64);
    let session = selected_session.read().clone();
    let provider_label = runtime_state
        .read()
        .as_ref()
        .and_then(|v| v.provider.clone())
        .unwrap_or_default();
    let model_label = runtime_state
        .read()
        .as_ref()
        .and_then(|v| v.model_id.clone())
        .unwrap_or_default();
    let active_tools = runtime_state
        .read()
        .as_ref()
        .map_or(0, |v| v.active_tools.len());
    let supported_commands = capabilities
        .read()
        .as_ref()
        .map(|caps| caps.supported_commands.join(", "))
        .unwrap_or_default();
    let providers: Vec<String> = models
        .read()
        .iter()
        .map(|m| m.provider.clone())
        .chain((!provider_label.is_empty()).then_some(provider_label.clone()))
        .chain(
            auth_status
                .read()
                .providers
                .iter()
                .map(|item| item.provider.clone()),
        )
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let key_supported = supports(capabilities, "set_api_key");
    let model_supported = supports(capabilities, "set_model");

    let refresh = Callback::new(move |()| {
        let (Some(session_id), Some(client)) = (
            selected_session.read().clone(),
            shared_client.read().clone(),
        ) else {
            models.set(Vec::new());
            auth_status.set(AuthStatus::default());
            loading.set(false);
            return;
        };
        let generation = *session_generation.read();
        *refresh_revision.write() += 1;
        let revision = *refresh_revision.read();
        let auth_supported = supports(capabilities, "get_auth_status");
        auth_status.set(AuthStatus::default());
        loading.set(true);
        refresh_error.set(String::new());
        scoped_spawn(requests, async move {
            let result = client.models(&session_id).await;
            let state = client.runtime_state(&session_id).await;
            let auth = if auth_supported {
                Some(client.auth_status(&session_id).await)
            } else {
                None
            };
            if *session_generation.read() != generation
                || selected_session.read().as_ref() != Some(&session_id)
                || *refresh_revision.read() != revision
            {
                return;
            }
            let mut failure = None;
            match result {
                Ok(items) => models.set(items),
                Err(value) => {
                    failure = Some(format!("读取可用模型失败：{value}"));
                }
            }
            match state {
                Ok(state) => runtime_state.set(Some(state)),
                Err(value) => failure = Some(format!("刷新运行状态失败：{value}")),
            }
            if let Some(auth_result) = auth {
                if let Ok(value) = auth_result {
                    auth_status.set(value);
                } else {
                    auth_status.set(AuthStatus::default());
                    failure = Some(match failure {
                        Some(existing) => {
                            format!("{existing}；认证状态未报告（读取失败，可重试）")
                        }
                        None => "认证状态未报告（读取失败，可重试）".into(),
                    });
                }
            }
            if let Some(value) = failure {
                refresh_error.set(value);
            }
            loading.set(false);
        });
    });
    use_effect(move || {
        // Track session and client changes synchronously; old responses are checked above.
        let _ = selected_session.read();
        let _ = shared_client.read();
        let _ = session_generation.read();
        *config_revision.write() += 1;
        config_open.set(false);
        *model_revision.write() += 1;
        model_busy.set(false);
        model_open.set(false);
        busy.set(false);
        api_key.set(String::new());
        key_edited.set(false);
        notice.set(String::new());
        restart_required.set(false);
        refresh.call(());
    });

    let select_model = Callback::new(move |value: String| {
        if value.is_empty() || *model_busy.read() {
            return;
        }
        if !supports(capabilities, "set_model") {
            error.set("当前节点未提供模型切换能力。".into());
            return;
        }
        let (Some(session_id), Some(client)) = (
            selected_session.read().clone(),
            shared_client.read().clone(),
        ) else {
            error.set("请先连接节点并选择一个会话。".into());
            return;
        };
        let generation = *session_generation.read();
        *model_revision.write() += 1;
        let revision = *model_revision.read();
        model_busy.set(true);
        error.set(String::new());
        scoped_spawn(requests, async move {
            let result = client.set_model(&session_id, &value).await;
            if *model_revision.read() != revision
                || *session_generation.read() != generation
                || selected_session.read().as_ref() != Some(&session_id)
            {
                return;
            }
            match result {
                Ok(response) => {
                    restart_required.set(
                        response
                            .get("requiresRestart")
                            .and_then(serde_json::Value::as_bool)
                            == Some(true),
                    );
                    let state = client.runtime_state(&session_id).await;
                    if *model_revision.read() != revision
                        || *session_generation.read() != generation
                        || selected_session.read().as_ref() != Some(&session_id)
                    {
                        return;
                    }
                    match state {
                        Ok(state) => {
                            runtime_state.set(Some(state));
                            model_open.set(false);
                            notice.set("模型选择已提交并回读运行状态。".into());
                            error.set(String::new());
                        }
                        Err(_) => error.set("模型选择已提交，但回读状态失败，请刷新检测。".into()),
                    }
                }
                Err(_) => error.set("设置模型失败，请检查节点连接后重试。".into()),
            }
            model_busy.set(false);
        });
    });
    let submit_key = move |_| {
        if *busy.read() {
            return;
        }
        let key = std::mem::take(&mut *api_key.write());
        let edited = *key_edited.read();
        key_edited.set(false);
        if !edited || key.trim().is_empty() {
            error.set("请输入 API Key 后再提交。".into());
            return;
        }
        if !supports(capabilities, "set_api_key") {
            error.set("当前节点未提供 API Key 配置能力，请在节点端配置。".into());
            return;
        }
        let provider = provider_draft.read().trim().to_owned();
        if provider.is_empty() {
            error.set("请选择或输入 Provider 标识，再重新输入 API Key。".into());
            return;
        }
        let (Some(session_id), Some(client)) = (
            selected_session.read().clone(),
            shared_client.read().clone(),
        ) else {
            error.set("请先连接节点并选择一个会话。".into());
            return;
        };
        let generation = *session_generation.read();
        *config_revision.write() += 1;
        let request_revision = *config_revision.read();
        busy.set(true);
        error.set(String::new());
        scoped_spawn(requests, async move {
            let result = client
                .command_data::<serde_json::Value>(
                    &session_id,
                    "set_api_key",
                    serde_json::json!({"provider": provider, "apiKey": key}),
                )
                .await;
            let current_request = *config_revision.read() == request_revision
                && config_open.read().to_owned()
                && provider_draft.read().trim() == provider;
            if *session_generation.read() != generation
                || selected_session.read().as_ref() != Some(&session_id)
            {
                return;
            }
            if !current_request {
                return;
            }
            match result {
                Ok(response) => {
                    restart_required.set(
                        response
                            .get("requiresRestart")
                            .and_then(serde_json::Value::as_bool)
                            == Some(true),
                    );
                    notice.set("节点已保存 API Key。".into());
                    status.set("Provider 凭据已由节点保存，请查看重启提示。".into());
                }
                // Never display a remote error that might echo credential input.
                Err(_) => {
                    error.set("API Key 提交失败，请检查节点连接与配置权限后重新输入。".into());
                }
            }
            busy.set(false);
        });
    };

    rsx! {
        div { class: "provider-page",
            div { class: "overline", "THE RIGHT MODEL FOR THE WORK." }
            div { class: "page-heading provider-header",
                div { h1 { "模型与 Provider" } p { "查看节点已发现的模型能力与凭据就绪状态。" } }
                div { class: "provider-header-actions",
                    button { class: "icon-button", aria_label: "刷新 Provider 信息", disabled: *loading.read(), onclick: move |_| refresh.call(()), Icon { name: "refresh" } }
                    button { class: "button primary", onclick: move |_| { *config_revision.write() += 1; busy.set(false); restart_required.set(false); notice.set(String::new()); error.set(String::new()); provider_draft.set(String::new()); api_key.set(String::new()); key_edited.set(false); config_open.set(true); }, Icon { name: "plus" } "添加连接" }
                }
            }
            div { class: "info-banner", role: "note", Icon { name: "info" } span { "模型列表来自当前节点的真实发现结果；凭据状态仅表示本机就绪情况，不代表远端已接受 API Key。" } }
            if session.is_none() {
                div { class: "empty-state", strong { "尚未选择会话" } p { "连接节点并选择会话后，可读取可用模型与运行状态。" } button { class: "button primary", onclick: onreturn, "返回会话选择" } }
            } else if *loading.read() && models.read().is_empty() {
                div { class: "loading-state", role: "status", "正在读取节点模型列表…" }
            }
            if !refresh_error.read().is_empty() {
                div { class: "error-state", role: "alert", p { "{refresh_error}" } button { class: "button", disabled: *loading.read(), onclick: move |_| refresh.call(()), "重试检测" } }
            }
            div { class: "provider-list",
                if models.read().is_empty() && !*loading.read() { p { class: "meta", "暂无可用模型，请检测连接或在节点端配置 Provider。" } }
                    for provider in providers.iter() {
                            {
                                let auth_snapshot = auth_status.read().clone();
                                let auth = auth_snapshot.providers.iter().find(|item| item.provider == *provider);
                                let model_count = models.read().iter().filter(|m| m.provider == *provider).count();
                                let capability = if models.read().iter().any(|m| &m.provider == provider && m.reasoning) { "推理 · 模型选择" } else if model_count > 0 { "模型选择" } else { "未报告" };
                                rsx! {
                                    article { class: "provider-group",
                                        div { class: "provider-card-heading",
                                            span { class: "provider-glyph", aria_hidden: "true", "{provider.chars().next().unwrap_or('?').to_ascii_uppercase()}" }
                                            div { strong { "{provider_name(provider)}" } code { "{provider}" } }
                                            span { class: if auth.is_some_and(|item| item.ready) { "auth-state ready" } else { "auth-state" }, "{auth.map_or(\"认证状态未报告\", auth_label)}" }
                                        }
                                        div { class: "provider-capability-summary", span { "已报告能力" } small { "{capability}" } }
                                        div { class: "provider-card-meta",
                                            span { "认证来源" }
                                            strong { "{auth.and_then(|item| item.source.as_deref()).filter(|value| !value.is_empty()).unwrap_or(\"未报告\")}" }
                                            span { "可用模型" }
                                            strong { "{model_count}" }
                                        }
                                        if auth.is_some_and(|item| item.oauth_available && !item.oauth) {
                                            p { class: "provider-oauth-note", "OAuth 由节点端管理，当前未显示为已登录" }
                                        }
                                        div { class: "provider-card-actions",
                                            button { class: "button", aria_label: "连接设置：{provider_name(provider)}", onclick: { let provider = provider.clone(); move |_| { *config_revision.write() += 1; busy.set(false); restart_required.set(false); provider_draft.set(provider.clone()); api_key.set(String::new()); key_edited.set(false); notice.set(String::new()); error.set(String::new()); config_open.set(true); } }, Icon { name: "settings" } "连接设置" }
                                            button { class: if provider_label == *provider { "button primary" } else { "button" }, disabled: *busy.read() || !model_supported || model_count == 0, onclick: { let provider = provider.clone(); let current = model_label.clone(); move |_| { *model_revision.write() += 1; model_busy.set(false); model_provider.set(provider.clone()); model_draft.set(current.clone()); error.set(String::new()); model_open.set(true); } }, if provider_label == *provider { "当前选择 · 更换模型" } else { "用于此会话" } }
                                            if provider_label == *provider { span { class: "sr-only", "当前会话 Provider" } }
                                        }
                                    }
                                }
                            }
                    }
                }
            if *config_open.read() {
            crate::components::Modal { id: "provider-connection-settings", title: "连接设置", class: "provider-connection-dialog", onclose: move |()| { *config_revision.write() += 1; busy.set(false); config_open.set(false); api_key.set(String::new()); key_edited.set(false); provider_draft.set(String::new()); },
            div { class: "modal-body",
            div { class: "provider-credentials",
                h3 { "Provider 配置" }
                p { class: "meta", "仅提交你主动输入的 API Key；提交后清空输入，不读取现有凭据。" }
                div { class: "form-field", label { r#for: "provider-id", "Provider 标识" }
                    input { id: "provider-id", list: "provider-identifiers", value: "{provider_draft}", placeholder: "例如 openai", oninput: move |event| { *config_revision.write() += 1; busy.set(false); restart_required.set(false); provider_draft.set(event.value()); api_key.set(String::new()); key_edited.set(false); error.set(String::new()); notice.set(String::new()); } }
                    datalist { id: "provider-identifiers", for provider in providers.iter() { option { value: "{provider}" } } }
                }
                div { class: "form-field", label { r#for: "provider-api-key", "API Key" } input { id: "provider-api-key", r#type: "password", autocomplete: "new-password", value: "{api_key}", oninput: move |event| { api_key.set(event.value()); key_edited.set(true); } } }
                button { class: "button primary", disabled: *busy.read(), onclick: submit_key, if *busy.read() { "正在提交…" } else { "保存 API Key 到节点" } }
                if !key_supported { p { class: "notice", "当前节点未提供 API Key 配置能力，请在节点端配置。" } }
                if !error.read().is_empty() { p { class: "connection-form-error", role: "alert", "{error}" } }
                div { class: "form-field", label { r#for: "provider-base-url", "自定义 Base URL · 本地草稿" } input { id: "provider-base-url", r#type: "url", value: "{base_url_draft}", placeholder: "https://api.example.com", oninput: move |event| base_url_draft.set(event.value()) } }
                p { class: "meta", "节点暂不支持修改 Base URL 或删除 Provider。此地址仅保留为当前页面草稿。" }
                button { class: "button", onclick: move |_| { *config_revision.write() += 1; busy.set(false); restart_required.set(false); base_url_draft.set(String::new()); provider_draft.set(String::new()); api_key.set(String::new()); key_edited.set(false); notice.set("已清除本地草稿。".into()); }, "清除本地草稿" }
                if !notice.read().is_empty() { p { class: "notice", role: "status", "{notice}" } }
                if *restart_required.read() { p { class: "notice", role: "status", "节点要求重启运行时后生效。请在节点端重启，然后刷新检测。" } }
            }
            }
            div { class: "modal-footer", button { class: "button", onclick: move |_| { *config_revision.write() += 1; busy.set(false); config_open.set(false); api_key.set(String::new()); key_edited.set(false); provider_draft.set(String::new()); }, "关闭" } }
            }
            }
            details { class: "runtime-diagnostics", open: false,
                summary { "运行诊断" }
                dl { class: "runtime-list",
                    div { dt { "Provider" } dd { if provider_label.is_empty() { "节点未返回" } else { "{provider_label}" } } }
                    div { dt { "当前模型" } dd { if model_label.is_empty() { "节点未返回" } else { "{model_label}" } } }
                    div { dt { "活动工具" } dd { "{active_tools} 个" } }
                }
                if let Some(caps) = capabilities.read().as_ref() {
                    p { class: "meta", "协议 {caps.protocol} · {caps.supported_commands.len()} 个命令 · {caps.supported_actions.len()} 个动作" }
                    p { class: "meta", "{supported_commands}" }
                }
                if !model_supported { p { class: "notice", "当前节点未提供模型切换能力。" } }
            }
            if *model_open.read() {
                crate::components::Modal { id: "provider-model-selection", title: "选择模型", class: "provider-model-dialog", onclose: move |()| { *model_revision.write() += 1; model_busy.set(false); model_open.set(false); model_draft.set(String::new()); },
                    div { class: "modal-body",
                        div { class: "form-field",
                            label { r#for: "provider-model-choice", "选择模型" }
                            select { id: "provider-model-choice", value: "{model_draft}", disabled: *model_busy.read(), onchange: move |event| model_draft.set(event.value()),
                                option { value: "", "请选择模型" }
                                for model in models.read().iter().filter(|m| m.provider == *model_provider.read()) { option { value: "{model.id}", "{model.name} · {model.id}" } }
                            }
                        }
                        if !error.read().is_empty() { p { class: "connection-form-error", role: "alert", "{error}" } }
                    }
                    div { class: "modal-footer",
                        button { class: "button primary", disabled: *model_busy.read() || !models.read().iter().any(|m| m.provider == *model_provider.read() && m.id == *model_draft.read()), onclick: move |_| select_model.call(model_draft.read().clone()), "用于此会话" }
                    }
                }
            }
        }
    }
}

fn scoped_spawn(
    requests: Signal<Vec<Task>>,
    future: impl std::future::Future<Output = ()> + 'static,
) {
    crate::state::scoped_spawn(requests, future);
}
