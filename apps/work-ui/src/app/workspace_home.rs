use crate::{
    features::conversation::{composer_extras, composer_input, model_picker},
    shared::{
        brand::BrandMark,
        components::{Icon, Modal},
        state::{Drafts, draft_for, update_draft},
    },
};
use dioxus::prelude::*;
use std::collections::HashMap;
use work_client::{ClientError, ModelDescriptor, WorkspaceClient};

pub(super) const CSS: &str = include_str!("workspace_home.css");

const SUGGESTIONS: [(&str, &str, &str); 4] = [
    ("code", "编写代码", "帮我实现一个功能："),
    (
        "shield",
        "审查代码",
        "帮我审查这段代码，找出潜在问题并给出改进建议：",
    ),
    (
        "sparkles",
        "梳理思路",
        "我有一个想法，帮我梳理可执行的实现方案：",
    ),
    (
        "folder",
        "探索项目",
        "帮我了解当前项目的结构、主要模块和运行方式。",
    ),
];

#[derive(Clone)]
pub(super) struct FirstMessage {
    pub scope: String,
    pub model: Option<ModelDescriptor>,
    pub thinking: String,
}

#[derive(Clone, Default)]
pub(super) struct Preferences {
    model: Option<ModelDescriptor>,
    thinking: String,
}

pub(super) async fn configure_first_message(
    client: &WorkspaceClient,
    id: &str,
    first: &FirstMessage,
    default_model: Option<(String, String)>,
) -> Result<(), String> {
    let model = first
        .model
        .as_ref()
        .map(|model| (model.provider.as_str(), model.id.as_str()))
        .or_else(|| {
            default_model
                .as_ref()
                .map(|(provider, id)| (provider.as_str(), id.as_str()))
        });
    if model.is_none() && first.thinking.is_empty() {
        return Ok(());
    }
    let result = super::config_requests::bounded("应用新会话配置", async {
        let configuration = client.session_configuration(id).await?;
        if let Some((provider, model)) = model {
            if !configuration.supports_command("set_model") {
                return Err(ClientError::Protocol("当前会话不支持切换模型".into()));
            }
            let response = client.set_model_with_provider(id, model, provider).await?;
            if response["requiresRestart"] == true {
                return Err(ClientError::Protocol("模型设置需要重启后生效".into()));
            }
        }
        if !first.thinking.is_empty() {
            if !configuration.supports_command("set_thinking_level")
                || !client.thinking_levels(id).await?.contains(&first.thinking)
            {
                return Err(ClientError::Protocol("当前会话不支持所选思考等级".into()));
            }
            let response = client.set_thinking_level(id, &first.thinking).await?;
            if response["requiresRestart"] == true {
                return Err(ClientError::Protocol("思考等级需要重启后生效".into()));
            }
        }
        let observed = client.runtime_state(id).await?;
        if let Some((provider, id)) = model {
            let expected = ModelDescriptor {
                id: id.to_string(),
                provider: provider.to_string(),
                name: String::new(),
                reasoning: false,
                api: None,
                context_window: None,
                max_tokens: None,
            };
            if !model_picker::matches_model(
                &expected,
                observed.provider.as_deref().unwrap_or_default(),
                observed.model_id.as_deref().unwrap_or_default(),
            ) {
                return Err(ClientError::Protocol("模型回读与所选配置不一致".into()));
            }
        }
        if !first.thinking.is_empty()
            && observed.thinking_level.as_deref() != Some(first.thinking.as_str())
        {
            return Err(ClientError::Protocol("思考等级回读与所选配置不一致".into()));
        }
        Ok(())
    })
    .await;
    result.map_err(|error| format!("新会话配置待确认：{error}。草稿已保留。"))
}

pub(super) async fn configured_models(
    client: &WorkspaceClient,
) -> Result<Vec<ModelDescriptor>, String> {
    super::config_requests::bounded("读取节点模型", client.provider_catalog())
        .await
        .map_err(|error| format!("无法读取已配置的模型：{error}"))
        .map(|catalog| {
            catalog
                .providers
                .into_iter()
                .filter(|provider| provider.configured)
                .filter_map(|provider| provider.definition)
                .flat_map(|provider| {
                    provider
                        .models
                        .into_iter()
                        .map(move |model| ModelDescriptor {
                            provider: provider.id.clone(),
                            id: model.id,
                            name: model.name,
                            reasoning: model.reasoning,
                            api: None,
                            context_window: model.context_window,
                            max_tokens: model.max_tokens,
                        })
                })
                .collect()
        })
}

#[component]
pub(super) fn Home(
    scope: String,
    drafts: Signal<Drafts>,
    attachments: Signal<composer_extras::AttachmentStore>,
    mut preferences: Signal<HashMap<String, Preferences>>,
    projects: Vec<(String, String, String)>,
    project_id: Option<String>,
    node: String,
    online: bool,
    busy: bool,
    ready: bool,
    models: Vec<ModelDescriptor>,
    default_model: Option<(String, String)>,
    model_loading: bool,
    model_error: Option<String>,
    error: Option<String>,
    onproject: EventHandler<String>,
    onadd: EventHandler,
    onconnect: EventHandler,
    onrefresh: EventHandler,
    onsubmit: EventHandler<FirstMessage>,
) -> Element {
    let mut project_open = use_signal(|| false);
    let mut more_open = use_signal(|| false);
    let preference = preferences.read().get(&scope).cloned().unwrap_or_default();
    let (provider, model_id) = preference
        .model
        .as_ref()
        .map(|model| (model.provider.clone(), model.id.clone()))
        .or(default_model)
        .unwrap_or_default();
    let project = projects
        .iter()
        .find(|(id, _, _)| Some(id) == project_id.as_ref());
    let project_name = project.map_or_else(|| "选择项目".to_string(), |(_, name, _)| name.clone());
    let project_path = project.map(|(_, _, path)| path.clone());
    let files = attachments.read().snapshot(&scope);
    let can_send = ready
        && online
        && project.is_some()
        && !busy
        && !files.loading
        && (!draft_for(&drafts.read(), &scope).text.trim().is_empty() || !files.files.is_empty());
    let submit_scope = scope.clone();
    let submit = Callback::new(move |()| {
        if can_send {
            let preference = preferences
                .peek()
                .get(&submit_scope)
                .cloned()
                .unwrap_or_default();
            onsubmit.call(FirstMessage {
                scope: submit_scope.clone(),
                model: preference.model,
                thinking: preference.thinking,
            });
        }
    });
    rsx! {
        div { class: "home-scroll",
            section { class: "home-content",
                div { class: "home-heading",
                    BrandMark { class: "home-mark" }
                    h1 { "今天，想做点什么？" }
                    p { "一个想法，一段代码，或者一个待解决的问题。" }
                }
                div { class: "home-editor",
                    div { class: "home-composer",
                        button { class: "home-project", r#type: "button",
                            aria_label: "选择新对话的项目", aria_haspopup: "dialog", aria_expanded: project_open(),
                            title: project_path, disabled: busy,
                            onclick: move |_| project_open.set(true),
                            Icon { name: "folder" } span { class: "home-project-name", "{project_name}" }
                            span { class: "home-project-node", "{node}" } Icon { name: "chevron" }
                        }
                        composer_extras::AttachmentPicker {
                            scope: scope.clone(), store: attachments, presentation: "files".to_string(),
                            images_supported: true, disabled: busy,
                            onquote: { let scope = scope.clone(); move |text| {
                                let current = draft_for(&drafts.peek(), &scope).text;
                                let separator = if current.is_empty() { "" } else { "\n" };
                                update_draft(drafts, &scope, format!("{current}{separator}{text}"));
                            } },
                        }
                        composer_input::ComposerInput {
                            scope: scope.clone(), drafts, disabled: busy || !ready,
                            placeholder: "有什么想法？尽管告诉 rpi…".to_string(), onenter: submit,
                        }
                        div { class: "home-tools",
                            button { class: "home-more icon-button", r#type: "button", aria_label: "附件与更多",
                                aria_haspopup: "dialog", aria_expanded: more_open(), disabled: busy,
                                onclick: move |_| more_open.set(true), Icon { name: "plus" }
                            }
                            model_picker::ModelPicker {
                                models, current_model: model_id, current_provider: provider,
                                loading: model_loading, error: model_error, read_supported: online,
                                write_supported: online, idle: !busy, busy: false, commit_revision: 0,
                                anchor: ".home-tools .composer-model-picker", preference: true,
                                onrefresh, onselect: { let scope = scope.clone(); move |value| {
                                    preferences.write().entry(scope.clone()).or_default().model = Some(value);
                                } },
                            }
                            model_picker::ThinkingPicker {
                                levels: ["off", "low", "medium", "high"].map(str::to_string).to_vec(),
                                current_level: preference.thinking, disabled: busy, write_supported: true,
                                busy: false, error: None, commit_revision: 0, onrefresh: move |()| {},
                                anchor: ".home-tools .composer-thinking-select", preference: true,
                                onselect: { let scope = scope.clone(); move |value| {
                                    preferences.write().entry(scope.clone()).or_default().thinking = value;
                                } },
                            }
                            button { class: "send-button home-send", r#type: "button",
                                disabled: !can_send, aria_busy: busy, aria_label: "发送消息",
                                onclick: move |_| submit.call(()), Icon { name: "arrow" }
                            }
                        }
                    }
                    if busy { p { class: "home-feedback", role: "status", "正在准备新会话…" } }
                    if let Some(error) = error { p { class: "home-feedback", role: "alert", "{error}" } }
                }
                div { class: "home-suggestions", aria_label: "快速开始",
                    for (icon, label, prompt) in SUGGESTIONS {
                        button { key: "{label}", r#type: "button", disabled: busy || !ready,
                            onclick: { let scope = scope.clone(); move |_| {
                                let current = draft_for(&drafts.peek(), &scope).text;
                                let separator = if current.is_empty() { "" } else { "\n" };
                                update_draft(drafts, &scope, format!("{current}{separator}{prompt}"));
                                let _ = document::eval("document.querySelector('.home-composer textarea')?.focus()");
                            } },
                            Icon { name: icon } span { "{label}" }
                        }
                    }
                }
                p { class: "home-footnote", "从一个想法开始，和 rpi 一起完成。" }
            }
        }
        if project_open() {
            Modal { id: "home-projects", title: "在哪个项目开始？", class: "home-dialog", trap_focus: true,
                subtitle: "为新对话选择一个工作项目。".to_string(), onclose: move |()| project_open.set(false),
                div { class: "home-projects",
                    for (id, name, path) in projects {
                        {
                            let selected = Some(&id) == project_id.as_ref();
                            rsx! {
                                button { key: "{id}", r#type: "button", class: "home-project-choice",
                                    aria_pressed: selected, title: "{path}", onclick: move |_| {
                                        onproject.call(id.clone());
                                        project_open.set(false);
                                    },
                                    Icon { name: "folder" } strong { "{name}" } span { "{node}" }
                                    if selected { Icon { name: "check" } }
                                }
                            }
                        }
                    }
                    if online {
                        button { class: "button", r#type: "button", onclick: move |_| { project_open.set(false); onadd.call(()); }, "打开项目" }
                    } else {
                        button { class: "button", r#type: "button", onclick: move |_| { project_open.set(false); onconnect.call(()); }, "工作协同" }
                    }
                }
            }
        }
        if more_open() {
            Modal { id: "home-more", title: "附件与更多", class: "home-dialog", trap_focus: true,
                onclose: move |()| more_open.set(false),
                div { class: "home-more-row", span { "附件" }
                    composer_extras::AttachmentPicker { scope, store: attachments, presentation: "button".to_string(),
                        images_supported: true, disabled: busy, onquote: move |_: String| {},
                    }
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[component]
pub(super) fn Fullscreen(#[props(default)] labelled: bool) -> Element {
    let mut state = use_signal(|| (false, false));
    let mut error = use_signal(|| false);
    use_effect(move || {
        spawn(async move {
            let mut events = document::eval(
                r"
                window.__rpiFullscreenCleanup?.();
                const sync = () => dioxus.send([Boolean(document.fullscreenEnabled), Boolean(document.fullscreenElement)]);
                document.addEventListener('fullscreenchange', sync);
                window.__rpiFullscreenCleanup = () => document.removeEventListener('fullscreenchange', sync);
                sync();
            ",
            );
            while let Ok(next) = events.recv::<(bool, bool)>().await {
                state.set(next);
            }
        });
    });
    use_drop(|| {
        let _ = document::eval("window.__rpiFullscreenCleanup?.()");
    });
    let (supported, active) = state();
    let label = if active {
        "退出全屏"
    } else {
        "进入全屏"
    };
    rsx! {
        button { class: if labelled { "button fullscreen-button" } else { "icon-button fullscreen-button" }, r#type: "button",
            aria_label: label, aria_pressed: active, disabled: !supported,
            title: if supported { label } else { "当前浏览器不支持全屏" },
            onclick: move |_| {
                error.set(false);
                let mut action = document::eval(r"
                    try {
                        if (document.fullscreenElement) await document.exitFullscreen();
                        else await document.documentElement.requestFullscreen();
                        dioxus.send(true);
                    } catch (_) { dioxus.send(false); }
                ");
                spawn(async move { error.set(!action.recv::<bool>().await.unwrap_or(false)); });
            },
            Icon { name: if active { "fullscreen-exit" } else { "fullscreen" } }
            if labelled { "{label}" }
        }
        if error() { span { class: "fullscreen-error", role: "status", "浏览器未能切换全屏" } }
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
#[component]
pub(super) fn Fullscreen(#[props(default)] labelled: bool) -> Element {
    use dioxus::desktop::{tao::window::Fullscreen as WindowFullscreen, use_window};
    let desktop = use_window();
    let mut active = use_signal(|| desktop.window.fullscreen().is_some());
    let label = if active() {
        "退出全屏"
    } else {
        "进入全屏"
    };
    rsx! {
        button { class: if labelled { "button fullscreen-button" } else { "icon-button fullscreen-button" }, r#type: "button",
            aria_label: label, aria_pressed: active(), title: label,
            onclick: move |_| {
                let next = desktop.window.fullscreen().is_none();
                desktop.window.set_fullscreen(next.then_some(WindowFullscreen::Borderless(None)));
                active.set(next);
            },
            Icon { name: if active() { "fullscreen-exit" } else { "fullscreen" } }
            if labelled { "{label}" }
        }
    }
}

#[cfg(target_os = "android")]
#[component]
pub(super) fn Fullscreen(#[props(default)] labelled: bool) -> Element {
    let _ = labelled;
    rsx! {}
}
