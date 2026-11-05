//! Test-only extension of the ui-modules harness. No HTTP ACP, processes, or model calls.
//! Build: cargo build --manifest-path tests/harness/ui-modules/Cargo.toml
//!   --bin agent-backends --target wasm32-unknown-unknown --no-default-features
#![allow(dead_code)]
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};
use work_client::{AgentBackendOption, AgentClient, AgentTransport, ClientError, EventStream, SessionAgentBinding};
use work_core::{AgentDescriptor, CapabilitySet, TransportIdentity};

#[path = "../../../../../apps/work-ui/src/shared/icons.rs"]
pub(crate) mod icons;
#[path = "../../../../../apps/work-ui/src/shared/components.rs"]
pub(crate) mod components;
#[path = "../../../../../apps/work-ui/src/shared/state.rs"]
pub(crate) mod state;
mod shared { pub(crate) use crate::{components, state}; }
#[path = "../../../../../apps/work-ui/src/features/conversation/execution_backend.rs"]
pub(crate) mod execution_backend;

#[path = "../../../../../apps/work-ui/src/features/resources/session_config.rs"]
mod session_config;
mod features { pub(crate) mod conversation { pub(crate) use crate::execution_backend; } }

fn main() { dioxus::launch(App); }

type Reply<'a, T> = Pin<Box<dyn Future<Output = Result<T, ClientError>> + 'a>>;

#[derive(Clone)]
struct FixtureTransport {
    values: Rc<RefCell<std::collections::HashMap<String, Vec<Value>>>>,
    calls: Signal<Vec<Value>>,
    fail: Signal<bool>,
}

fn config_options() -> Vec<Value> {
    vec![
        json!({"id":"model", "name":"Model", "currentValue":"provider-a/model-a", "options":[
            {"group":"provider-a", "name":"Provider A", "options":[
                {"value":"provider-a/model-a", "name":"Model A"},
                {"value":"provider-a/model-long", "name":"A very long model label that must not widen a 320 pixel viewport"}]},
            {"group":"provider-b", "name":"Provider B", "options":[{"value":"provider-b/model-b", "name":"Model B"}]}]}),
        json!({"id":"mode", "name":"Mode", "currentValue":"default", "options":[
            {"value":"default", "name":"Default"}, {"value":"plan", "name":"Plan (read only)"}]}),
    ]
}

// These signatures are the ?Send async-trait ABI, avoiding a new harness dependency.
impl AgentTransport for FixtureTransport {
    fn descriptor(&self) -> AgentDescriptor {
        AgentDescriptor { identity: TransportIdentity::new("acp", Some("fixture-only".into()), "memory"), capabilities: CapabilitySet::acp_default() }
    }
    fn supports_config_option(&self, session_id: &str, id: &str) -> bool {
        self.session_config_options(session_id).iter().any(|option| option["id"] == id)
    }
    fn session_config_options(&self, session_id: &str) -> Vec<Value> {
        self.values.borrow_mut().entry(session_id.into()).or_insert_with(config_options).clone()
    }
    fn set_config_option<'a, 'b, 'c, 'f>(&'a self, session_id: &'b str, id: &'c str, value: Value) -> Reply<'f, Value>
    where 'a: 'f, 'b: 'f, 'c: 'f, Self: 'f {
        Box::pin(async move {
            let mut calls = self.calls;
            calls.write().push(json!({"sessionId":session_id,"id":id,"value":value}));
            let _ = document::eval("await new Promise(r=>setTimeout(r,100));dioxus.send(true)").recv::<bool>().await;
            if *self.fail.read() { return Err(ClientError::Sse("controlled fixture rejection".into())); }
            let mut all = self.values.borrow_mut();
            let options = all.entry(session_id.into()).or_insert_with(config_options);
            if let Some(option) = options.iter_mut().find(|option| option["id"] == id) { option["currentValue"] = value; }
            Ok(json!({"configOptions":options}))
        })
    }
    fn request<'a, 'b, 'c, 'f>(&'a self, _: &'b str, _: &'c str, _: Value) -> Reply<'f, Value>
    where 'a: 'f, 'b: 'f, 'c: 'f, Self: 'f {
        Box::pin(async { Err(ClientError::Sse("fixture forbids commands/model calls".into())) })
    }
    fn subscribe<'a, 'b, 'f>(&'a self, _: &'b str, _: u64) -> Reply<'f, EventStream>
    where 'a: 'f, 'b: 'f, Self: 'f {
        Box::pin(async { Err(ClientError::Sse("fixture has no stream".into())) })
    }
}

#[component]
fn App() -> Element {
    let mut session = use_signal(|| None::<String>);
    let mut selected = use_signal(|| "rpi".to_owned());
    // Deliberately retain this on deselection to exercise the component's defensive gate.
    let mut binding = use_signal(|| Some(SessionAgentBinding { backend: "claude".into(), external_session_id: Some("external-a".into()) }));
    let mut idle = use_signal(|| true);
    let mut busy = use_signal(|| false);
    let mut remote = use_signal(|| false);
    let mut fail = use_signal(|| false);
    let calls = use_signal(Vec::<Value>::new);
    let mut creations = use_signal(Vec::<Value>::new);
    let mut drafts = use_signal(|| std::collections::HashMap::from([("session-a".to_owned(), "draft A".to_owned()), ("session-b".to_owned(), "draft B".to_owned())]));
    let transport = use_hook(|| FixtureTransport { values: Rc::default(), calls, fail });
    let config_values = transport.values.clone();
    let mut config_notifications = use_signal(|| 0u64);
    let _notification_version = *config_notifications.read();
    let agent = use_hook(|| AgentClient::from_transport(transport));
    let mut workspace_view = use_signal(|| false);
    let mut parent_view = use_signal(|| false);
    let runtime_state = use_signal(|| None::<work_client::RuntimeState>);
    let status = use_signal(String::new);
    let mut workspace_url = use_signal(String::new);
    let mut shared_client = use_signal(|| None::<work_client::WorkspaceClient>);
    let session_generation = use_signal(|| 0u64);
    let requests = use_signal(Vec::<dioxus::core::Task>::new);
    use_effect(move || {
        spawn(async move {
            let url = document::eval("dioxus.send(location.origin)").recv::<String>().await.unwrap();
            shared_client.set(Some(work_client::WorkspaceClient::new(&url).unwrap()));
            workspace_url.set(url);
        });
    });
    let options = ["rpi", "claude", "codex"].into_iter().map(|id| AgentBackendOption {
        id: id.into(), name: if id == "rpi" { "RPI".into() } else { id.into() }, available: !*remote.read() || id == "rpi", reason: None,
    }).collect::<Vec<_>>();
    let scope = session.read().clone().unwrap_or_default();
    rsx! {
        style { {include_str!("../../../../../apps/work-ui/src/shared/theme/styles.css")} }
        style { {execution_backend::CSS} }
        style { "html,body {{overflow:auto}} main {{padding:12px;max-width:700px;margin:auto;min-width:0}} .fixture-controls {{display:flex;flex-wrap:wrap;gap:8px}} .fixture-controls button {{min-height:44px}} .fixture-menu {{margin-top:16px}} textarea,pre {{max-width:100%;box-sizing:border-box;overflow-wrap:anywhere;white-space:pre-wrap}}" }
        main {
            h1 { "Backend real-component fixture (memory only)" }
            div { class: "fixture-controls",
                for (id, backend, external) in [("session-a", "claude", "external-a"), ("session-b", "codex", "external-b")] {
                    button { onclick: move |_| { session.set(Some(id.into())); binding.set(Some(SessionAgentBinding { backend: backend.into(), external_session_id: Some(external.into()) })); }, "{id}" }
                }
                button { onclick: move |_| { session.set(None); selected.set("rpi".into()); }, "Deselect (stale binding)" }
                button { onclick: move |_| workspace_view.set(true), "Use workspace wrapper" }
                button { onclick: move |_| parent_view.set(true), "Use parent session configuration" }
                button { onclick: move |_| {
                    shared_client.set(Some(work_client::WorkspaceClient::new(&workspace_url.peek()).unwrap()));
                }, "Reconnect workspace access" }
                button { onclick: move |_| { let next = !*idle.read(); idle.set(next); }, "Toggle running" }
                button { onclick: move |_| { let next = !*busy.read(); busy.set(next); }, "Toggle busy" }
                button { onclick: move |_| { let next = !*remote.read(); remote.set(next); }, "Toggle remote catalog" }
                button { onclick: move |_| { let next = !*fail.read(); fail.set(next); }, "Toggle config failure" }
                button { onclick: move |_| {
                    if let Some(id) = session.peek().as_ref() {
                        let mut values = config_values.borrow_mut();
                        let options = values.entry(id.clone()).or_insert_with(config_options);
                        if let Some(model) = options.iter_mut().find(|option| option["id"] == "model") {
                            model["currentValue"] = json!("provider-a/model-a");
                        }
                    }
                    config_notifications += 1;
                }, "Push config notification" }
            }
            p { "Project: fixture-project" }
            textarea { aria_label: "Session draft", value: drafts.read().get(&scope).cloned().unwrap_or_default(),
                oninput: { let scope = scope.clone(); move |event| { drafts.write().insert(scope.clone(), event.value()); } }
            }
            div { class: "fixture-menu",
                execution_backend::BackendMenu {
                    key: "backend-{scope}", inline: true, options, selected: selected.read().clone(), binding: binding.read().clone(),
                    has_session: session.read().is_some(), busy: *busy.read(),
                    onchange: move |backend: String| {
                        selected.set(backend.clone());
                        if session.peek().is_some() {
                            // Fixture navigation is not a native-host integration test.
                            let id = format!("new-{}", creations.peek().len());
                            creations.write().push(json!({"projectId":"fixture-project","backend":backend,"sessionId":id}));
                            binding.set(Some(SessionAgentBinding { backend, external_session_id: Some(id.clone()) }));
                            session.set(Some(id));
                        }
                    }
                }
            }
            if *parent_view.read() && session.read().is_some() {
                session_config::SessionConfig { key: "parent-config-{scope}", shared_client, selected_session: session, session_generation, requests, runtime_state, status, tools_only: false, config_idle: *idle.read() }
            } else if *workspace_view.read() && session.read().is_some() {
                execution_backend::NegotiatedConfig { key: "workspace-config-{scope}", shared_client, session_id: scope.clone(), session_generation, requests, idle: *idle.read() }
            } else if session.read().is_some() {
                FixtureNegotiatedConfig { key: "config-{scope}", config_snapshot: agent.session_config_options(&scope), agent: agent.clone(), session_id: scope.clone(), idle: *idle.read() }
            }
            pre { id: "fixture-session", "{scope}" }
            pre { id: "fixture-creations", "{creations.read().iter().map(Value::to_string).collect::<Vec<_>>().join(\"\\n\")}" }
            pre { id: "fixture-calls", "{calls.read().iter().map(Value::to_string).collect::<Vec<_>>().join(\"\\n\")}" }
        }
    }
}

// The harness exercises production presentation with a memory transport. The
// workspace/HTTP wrapper is validated separately by isolated Rust fixtures.
#[derive(Props, Clone)]
struct FixtureNegotiatedConfigProps {
    agent: AgentClient, session_id: String, idle: bool, config_snapshot: Vec<Value>,
}
impl PartialEq for FixtureNegotiatedConfigProps {
    fn eq(&self, other: &Self) -> bool {
        self.session_id == other.session_id && self.idle == other.idle && self.config_snapshot == other.config_snapshot
    }
}
#[allow(non_snake_case)]
fn FixtureNegotiatedConfig(props: FixtureNegotiatedConfigProps) -> Element {
    let mut revision = use_signal(|| 0u64);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let _ = revision.read();
    rsx! {
        execution_backend::NegotiatedConfigControls {
            options: props.agent.session_config_options(&props.session_id), idle: props.idle, busy: *busy.read(), loading: false, error: error.read().clone(),
            onselect: move |(id, value): (String, String)| {
                if !props.idle || *busy.peek() { return; }
                let agent = props.agent.clone(); let session_id = props.session_id.clone();
                busy.set(true);
                spawn(async move {
                    match agent.set_config_option(&session_id, &id, json!(value)).await {
                        Ok(_) => { error.set(String::new()); revision += 1; }
                        Err(e) => error.set(format!("配置更新失败：{e}")),
                    }
                    busy.set(false);
                });
            },
        }
    }
}
